use crate::error::EngineError;
use crate::pairing::EnrolmentInvite;
use crate::roster::{ROOT_KEY_ID, RosterOutcome, SignedRoster};
use crate::trust::Signer;

struct Joining<'a>(&'a crate::Engine);

impl<'a> Joining<'a> {
    fn begin(engine: &'a crate::Engine) -> Joining<'a> {
        engine
            .joining
            .store(true, std::sync::atomic::Ordering::SeqCst);
        Joining(engine)
    }
}

impl Drop for Joining<'_> {
    fn drop(&mut self) {
        self.0
            .joining
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

#[async_trait::async_trait]
pub trait CellTransport: Send + Sync {
    async fn pairing_invite(&self) -> Result<Option<crate::pairing::PairingInvite>, EngineError> {
        Ok(None)
    }
    async fn call(
        &self,
        _node: &str,
        _request: crate::calls::Request,
    ) -> Result<crate::calls::Snapshot, EngineError> {
        Err(EngineError::Consequence(
            "Calls are unavailable on this transport".into(),
        ))
    }
    fn local_node_id(&self) -> Option<String> {
        None
    }
    fn peer_network(&self) -> Option<serde_json::Value> {
        None
    }
    async fn enrol(&self, invite: &EnrolmentInvite) -> Result<SignedRoster, EngineError>;
    async fn audit_against(
        &self,
        contact_organ: &str,
    ) -> Result<Option<crate::wire::AuditAgreement>, EngineError>;
    async fn carrier_probe(&self, node_id: &str) -> crate::wire::CarrierProbe;
    async fn collect_mail_now(&self) -> Result<usize, EngineError>;
    async fn sync_now(&self) -> Result<usize, EngineError>;
    async fn ask_to_be_carried(&self, node_id: &str) -> Result<(), EngineError>;
    async fn redeem_mailbox_invite(
        &self,
        node_id: &str,
        token: &str,
    ) -> Result<(String, i64), EngineError>;
}

impl crate::Engine {
    pub(crate) fn peer_network_status(&self) -> Option<serde_json::Value> {
        self.transport_for("read network status").ok().and_then(|transport| transport.peer_network())
    }

    pub async fn current_pairing_invite(
        &self,
    ) -> Result<Option<crate::pairing::PairingInvite>, EngineError> {
        self.transport_for("issue a reachable invitation")?
            .pairing_invite()
            .await
    }
    pub async fn create_organ_identity(&self) -> Result<SignedRoster, EngineError> {
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("This Cell has no Organ".into()))?;
        if let Some(roster) = self.roster_of(&organ.uid).await? {
            return Ok(roster);
        }
        let node_id = self
            .transport_for("create its Organ")?
            .local_node_id()
            .ok_or_else(|| {
                EngineError::Consequence("Wait for the network endpoint to open".into())
            })?;
        let path = self
            .root_key_path
            .lock()
            .expect("root key path")
            .clone()
            .ok_or_else(|| EngineError::Consequence("This Cell has no key directory".into()))?;
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("This Cell has no device identity".into()))?;
        let operational_key = self
            .local_organ_public_key()
            .await?
            .ok_or_else(|| EngineError::Consequence("This Cell has no operational key".into()))?;
        let root = Signer::load_or_create(&path, &organ.uid, ROOT_KEY_ID)?;
        self.publish_root_key(&root).await?;
        let roster = self
            .publish_roster(
                &root,
                vec![crate::roster::CellEntry {
                    cell_uid: cell.uid,
                    node_id,
                    label: cell.label,
                    operational_key,
                    sealing_key: self.published_sealing_key().await?,
                    front_door: false,
                    capabilities: crate::roster::full_capabilities(),
                }],
            )
            .await?;
        self.notify_config_changed();
        Ok(roster)
    }

    pub fn set_enroller(&self, enroller: std::sync::Weak<dyn CellTransport>) {
        *self.enroller.lock().expect("enroller") = Some(enroller);
    }

    pub async fn join_from_code(&self, code: &str) -> Result<SignedRoster, EngineError> {
        let invite = EnrolmentInvite::decode(code)?;
        let enroller = self
            .enroller
            .lock()
            .expect("enroller")
            .clone()
            .and_then(|weak| weak.upgrade())
            .ok_or_else(|| {
                EngineError::Consequence(
                    "this Cell has no network endpoint, so it cannot reach the device that \
                     showed the code."
                        .into(),
                )
            })?;
        enroller.enrol(&invite).await
    }

    pub async fn audit_contact(
        &self,
        contact_organ: &str,
    ) -> Result<Option<crate::wire::AuditAgreement>, EngineError> {
        let transport = self
            .enroller
            .lock()
            .expect("enroller")
            .clone()
            .and_then(|weak| weak.upgrade())
            .ok_or_else(|| {
                EngineError::Consequence(
                    "this Cell has no network endpoint, so it cannot compare logs with \
                     anyone."
                        .into(),
                )
            })?;
        transport.audit_against(contact_organ).await
    }

    pub async fn carrier_probe(&self, node_id: &str) -> crate::wire::CarrierProbe {
        let transport = self
            .enroller
            .lock()
            .expect("enroller")
            .clone()
            .and_then(|weak| weak.upgrade());
        match transport {
            Some(transport) => transport.carrier_probe(node_id).await,
            None => crate::wire::CarrierProbe::Unreachable,
        }
    }

    pub async fn collect_mail_now(&self) -> Result<usize, EngineError> {
        let transport = self
            .enroller
            .lock()
            .expect("enroller")
            .clone()
            .and_then(|weak| weak.upgrade())
            .ok_or_else(|| {
                EngineError::Consequence(
                    "this Cell has no network endpoint, so it cannot reach a mailbox.".into(),
                )
            })?;
        transport.collect_mail_now().await
    }

    pub async fn sync_now(&self) -> Result<usize, EngineError> {
        let transport = self
            .enroller
            .lock()
            .expect("enroller")
            .clone()
            .and_then(|weak| weak.upgrade())
            .ok_or_else(|| {
                EngineError::Consequence(
                    "this Cell has no network endpoint, so it cannot sync with anyone.".into(),
                )
            })?;
        transport.sync_now().await
    }

    pub async fn ask_carrier(&self, node_id: &str) -> Result<(), EngineError> {
        self.transport_for("ask anyone to carry your mail")?
            .ask_to_be_carried(node_id)
            .await
    }

    pub async fn redeem_carry_code(&self, code: &str) -> Result<(String, i64), EngineError> {
        let invite = crate::pairing::MailboxInviteCode::decode(code)?;
        self.transport_for("reach the box that issued that code")?
            .redeem_mailbox_invite(&invite.node_id, &invite.token)
            .await
    }

    pub(crate) fn transport_for(&self, what: &str) -> Result<std::sync::Arc<dyn CellTransport>, EngineError> {
        self.enroller
            .lock()
            .expect("enroller")
            .clone()
            .and_then(|weak| weak.upgrade())
            .ok_or_else(|| {
                EngineError::Consequence(format!(
                    "this Cell has no network endpoint, so it cannot {what}."
                ))
            })
    }

    pub(crate) fn identity_in_flux(&self) -> bool {
        self.joining.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub async fn may_enrol(&self) -> Result<(), EngineError> {
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("this Cell has no Organ".into()))?;
        if self.roster_of(&organ.uid).await?.is_some() {
            return Err(EngineError::Consequence(
                "this Cell already has a published identity of its own. Joining another \
                 Organ would abandon it, and every contact holding its key with it."
                    .into(),
            ));
        }
        if store::organs::holds_own_records(&self.store.pool).await? {
            return Err(EngineError::Consequence(
                "this Cell already holds Records of its own, so joining another Organ \
                 would have to merge two identities. Enrol a device that has not been \
                 used yet."
                    .into(),
            ));
        }
        Ok(())
    }

    pub async fn join_organ(
        &self,
        invite: &EnrolmentInvite,
        signed: &SignedRoster,
        operational: Signer,
    ) -> Result<(), EngineError> {
        self.may_enrol().await?;
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("this Cell has no Cell Record".into()))?;
        if signed.roster.organ_uid != invite.organ_uid {
            return Err(EngineError::Consequence(
                "the roster returned is for a different Organ than the code offered".into(),
            ));
        }
        if signed.roster.root_key != invite.root_key {
            return Err(EngineError::Consequence(
                "the roster returned is signed by a different root key than the code \
                 carried"
                    .into(),
            ));
        }
        if !signed
            .roster
            .cells
            .iter()
            .any(|member| member.cell_uid == cell.uid)
        {
            return Err(EngineError::Consequence(
                "the roster returned does not list this device".into(),
            ));
        }

        if signed.roster.version < 1 || !crate::roster::roster_signature_is_valid(signed) {
            return Err(EngineError::Consequence(
                "The returned roster has an invalid signature or has expired".into(),
            ));
        }
        let member = signed
            .roster
            .cells
            .iter()
            .find(|member| member.cell_uid == cell.uid)
            .expect("membership checked");
        if member.operational_key != operational.public_key_b64() {
            return Err(EngineError::Consequence(
                "The roster does not authorize this device's key".into(),
            ));
        }
        let mut cells = std::collections::HashSet::new();
        let mut nodes = std::collections::HashSet::new();
        if signed.roster.cells.iter().any(|member| {
            member.cell_uid.is_empty()
                || member.node_id.is_empty()
                || !cells.insert(&member.cell_uid)
                || !nodes.insert(&member.node_id)
        }) {
            return Err(EngineError::Consequence(
                "The roster contains duplicate or missing device identities".into(),
            ));
        }

        let base_url = store::organs::local(&self.store.pool)
            .await?
            .map(|organ| organ.base_url)
            .unwrap_or_default();
        let _joining = Joining::begin(self);
        store::organs::adopt_identity(&self.store.pool, &invite.organ_uid, &base_url).await?;
        crate::trust::adopt_key(
            &self.store,
            &invite.organ_uid,
            ROOT_KEY_ID,
            &invite.root_key,
        )
        .await?;
        match self.adopt_roster(signed).await? {
            RosterOutcome::Accepted | RosterOutcome::NotNewer => {}
            RosterOutcome::Expired => {
                return Err(EngineError::Consequence(
                    "the roster returned has already expired".into(),
                ));
            }
            RosterOutcome::Refused => {
                return Err(EngineError::Consequence(
                    "the roster returned is not signed by the root key the code carried".into(),
                ));
            }
        }
        self.set_organ_signer(operational).await?;
        self.notify_config_changed();
        tracing::info!(
            organ = %invite.organ_uid,
            cell = %cell.uid,
            members = signed.roster.cells.len(),
            "this Cell is now a member of an existing Organ"
        );
        Ok(())
    }

    pub async fn operational_key_for(&self, organ_uid: &str) -> Result<Signer, EngineError> {
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("this Cell has no Cell Record".into()))?;
        let key_id = crate::roster::cell_key_id(&cell.uid);
        Ok(match self.organ_signer.lock().await.as_ref() {
            Some(existing) => Signer::from_bytes(organ_uid, &key_id, existing.secret_bytes()),
            None => Signer::generate(organ_uid, &key_id),
        })
    }
}
