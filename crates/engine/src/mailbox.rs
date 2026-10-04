use crate::error::EngineError;
use crate::roster::SignedRoster;

pub const MAX_BUNDLE_BYTES: usize = 1024 * 1024;

pub const DEFAULT_QUOTA_BYTES: i64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    NotRegistered,
    QuotaFull,
    TooLarge,
    Unreadable,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::NotRegistered => write!(f, "this mailbox carries no mail for that Organ"),
            Refusal::QuotaFull => write!(f, "that recipient's mailbox is full"),
            Refusal::TooLarge => write!(f, "that bundle is larger than this mailbox accepts"),
            Refusal::Unreadable => write!(f, "that is not a sealed bundle this build can carry"),
        }
    }
}

impl Refusal {
    pub fn code(&self) -> &'static str {
        match self {
            Refusal::NotRegistered => "mailbox_not_registered",
            Refusal::QuotaFull => "mailbox_quota_full",
            Refusal::TooLarge => "mailbox_bundle_too_large",
            Refusal::Unreadable => "mailbox_unreadable",
        }
    }
}

impl crate::Engine {
    pub async fn is_a_mailbox(&self) -> Result<bool, EngineError> {
        Ok(!store::mailbox::registrations(&self.store.pool)
            .await?
            .is_empty())
    }

    pub async fn accept_bundle(&self, body: &str, from_node: &str) -> Result<String, Refusal> {
        if body.len() > MAX_BUNDLE_BYTES {
            return Err(Refusal::TooLarge);
        }
        let bundle: crate::seal::SealedBundle =
            serde_json::from_str(body).map_err(|_| Refusal::Unreadable)?;
        if bundle.v != crate::seal::SEAL_VERSION {
            return Err(Refusal::Unreadable);
        }

        crate::seal::validate_envelope(&bundle, nucleus::execution::now().timestamp())
            .map_err(|_| Refusal::Unreadable)?;
        let body = serde_json::to_string(&bundle).map_err(|_| Refusal::Unreadable)?;
        let uid = crate::seal::delivery_id(&bundle);
        let now = nucleus::execution::now();
        let global_limit = store::cells::config(&self.store.pool, "lince.social")
            .await
            .map_err(|_| Refusal::Unreadable)?
            .and_then(|value| value["max_storage_bytes"].as_i64())
            .unwrap_or(1024 * 1024 * 1024);
        let expires_at =
            chrono::DateTime::from_timestamp(bundle.expires_at, 0).ok_or(Refusal::Unreadable)?;
        let stored = store::mailbox::delivery::deposit(
            &self.store.pool,
            &store::mailbox::HeldBundle {
                uid: uid.clone(),
                to_organ: bundle.to_organ.clone(),
                from_organ: bundle.from_organ.clone(),
                from_cell: bundle.from_cell.clone(),
                from_node: from_node.to_owned(),
                bytes: body.len() as i64,
                body,
                received_at: now.to_rfc3339(),
                expires_at: expires_at.to_rfc3339(),
            },
            global_limit,
        )
        .await
        .map_err(|_| Refusal::Unreadable)?;
        match stored {
            store::mailbox::delivery::Deposit::Stored => {}
            store::mailbox::delivery::Deposit::NotRegistered => return Err(Refusal::NotRegistered),
            store::mailbox::delivery::Deposit::Full => return Err(Refusal::QuotaFull),
            store::mailbox::delivery::Deposit::Conflict => return Err(Refusal::Unreadable),
        }
        Ok(uid)
    }

    pub async fn may_collect(
        &self,
        organ_uid: &str,
        node_id: &str,
        presented: &SignedRoster,
    ) -> Result<bool, EngineError> {
        let Some(registration) = store::mailbox::registration(&self.store.pool, organ_uid).await?
        else {
            return Ok(false);
        };
        if presented.roster.organ_uid != organ_uid {
            return Ok(false);
        }
        if !crate::roster::roster_signature_is_valid(presented) {
            return Ok(false);
        }
        let chains = presented.roster.root_key == registration.root_key
            || self
                .key_chains(organ_uid, &presented.roster.root_key)
                .await?;
        if !chains {
            return Ok(false);
        }
        if self
            .roster_of(organ_uid)
            .await?
            .is_some_and(|known| known.roster.version > presented.roster.version)
        {
            return Ok(false);
        }
        if !store::mailbox::delivery::advance_roster(
            &self.store.pool,
            organ_uid,
            presented.roster.version,
            &serde_json::to_string(presented).map_err(EngineError::Json)?,
        )
        .await?
        {
            return Ok(false);
        }
        let authorized = presented.roster.cells.iter().any(|cell| {
            cell.node_id == node_id
                && cell
                    .capabilities
                    .iter()
                    .any(|cap| cap == crate::roster::CAP_WRITE)
        });
        if authorized {
            store::mailbox::delivery::acknowledge(&self.store.pool, organ_uid, node_id, &[])
                .await?;
        }
        Ok(authorized)
    }

    pub async fn bundles_for(
        &self,
        organ_uid: &str,
        limit: i64,
    ) -> Result<Vec<store::mailbox::HeldBundle>, EngineError> {
        Ok(store::mailbox::for_recipient(&self.store.pool, organ_uid, limit).await?)
    }

    pub async fn bundles_for_device(
        &self,
        organ: &str,
        node: &str,
        limit: i64,
    ) -> Result<Vec<store::mailbox::HeldBundle>, EngineError> {
        let rows =
            store::mailbox::delivery::for_device(&self.store.pool, organ, node, limit).await?;
        let mut bytes = 4096usize;
        let mut selected = Vec::new();
        for row in rows {
            let encoded = serde_json::to_vec(&crate::wire::MailboxBundle {
                uid: row.uid.clone(),
                from_organ: row.from_organ.clone(),
                from_cell: row.from_cell.clone(),
                body: row.body.clone(),
                received_at: row.received_at.clone(),
                expires_at: row.expires_at.clone(),
            })
            .map_err(EngineError::Json)?;
            if bytes.saturating_add(encoded.len() + 1) > crate::wire::MAX_FRAME_BYTES {
                break;
            }
            bytes += encoded.len() + 1;
            selected.push(row);
        }
        Ok(selected)
    }

    pub async fn confirm_collected(
        &self,
        organ_uid: &str,
        node_id: &str,
        uids: &[String],
    ) -> Result<u64, EngineError> {
        Ok(
            store::mailbox::delivery::acknowledge(&self.store.pool, organ_uid, node_id, uids)
                .await?,
        )
    }

    pub async fn sweep_mailbox(&self) -> Result<u64, EngineError> {
        Ok(store::mailbox::sweep_expired(&self.store.pool).await?)
    }

    pub async fn expiries_for(
        &self,
        from_node: &str,
    ) -> Result<Vec<store::mailbox::HeldBundle>, EngineError> {
        let notices = store::mailbox::expiries_for_node(&self.store.pool, from_node).await?;
        if !notices.is_empty() {
            store::mailbox::notices_handed(&self.store.pool, from_node).await?;
        }
        Ok(notices)
    }

    pub async fn forget_expiries(
        &self,
        from_node: &str,
        uids: &[String],
    ) -> Result<u64, EngineError> {
        Ok(store::mailbox::notices_heard(&self.store.pool, from_node, uids).await?)
    }

    pub async fn note_expired_mail(
        &self,
        carrier_node: &str,
        reports: &[(String, String)],
    ) -> Result<Vec<String>, EngineError> {
        let mut believed = Vec::new();
        for (uid, expired_at) in reports {
            if store::mail_left::mark_expired(&self.store.pool, carrier_node, uid, expired_at)
                .await?
            {
                believed.push(uid.clone());
            }
        }
        Ok(believed)
    }

    pub async fn seal_batch_for(
        &self,
        to_organ: &str,
        root: Option<&str>,
        batch: &crate::sync::OpBatch,
    ) -> Result<crate::seal::SealedBundle, EngineError> {
        let Some(their_roster) = self.roster_of(to_organ).await? else {
            return Err(EngineError::Consequence(
                "we hold no roster for that Organ, so there is nothing to seal to".into(),
            ));
        };
        let now = nucleus::execution::now();
        let recipients: Vec<crate::seal::SealingKey> = their_roster
            .roster
            .cells
            .iter()
            .filter(|cell| {
                cell.capabilities
                    .iter()
                    .any(|cap| cap == crate::roster::CAP_WRITE)
            })
            .filter_map(|cell| cell.sealing_key.clone())
            .filter(|key| {
                chrono::DateTime::parse_from_rfc3339(&key.not_after)
                    .map(|when| when.with_timezone(&chrono::Utc) > now)
                    .unwrap_or(false)
            })
            .collect();
        let Some(cell) = store::cells::local(&self.store.pool).await? else {
            return Err(EngineError::Consequence(
                "this Cell has no Cell Record".into(),
            ));
        };
        let signing = {
            let held = self.organ_signer.lock().await;
            let signer = held.as_ref().ok_or_else(|| {
                EngineError::Consequence("this Cell holds no operational key to sign mail".into())
            })?;
            ed25519_dalek::SigningKey::from_bytes(&signer.secret_bytes())
        };
        let mail = crate::seal::MailedBatch {
            root: root.map(str::to_string),
            batch: batch.clone(),
        };
        crate::seal::seal(&mail, &cell.uid, to_organ, &recipients, &signing)
            .map_err(|why| EngineError::Consequence(why.to_string()))
    }

    pub(crate) async fn outgoing_mail_policy_hash(&self, to: &str) -> Result<String, EngineError> {
        let pool = &self.store.pool;
        let contact = store::organs::contact(pool, to).await?.ok_or_else(|| EngineError::Forbidden("The mail recipient is no longer a contact".into()))?;
        let hidden = store::visibility::hidden_from_organ(pool, to).await?;
        let mut hidden: Vec<_> = hidden.into_iter().collect();
        hidden.sort();
        let mut selection = if let Some(raw) = &contact.share_protein { crate::share::selected(self, to, raw).await?.into_iter().collect::<Vec<_>>() } else { Vec::new() };
        selection.sort();
        let grants: Vec<(String, String)> = store::sqlx::query_as("SELECT root_record, state FROM replica_grant WHERE contact_organ = ? ORDER BY root_record").bind(to).fetch_all(pool).await?;
        Ok(nucleus::fact::sha256_hex(&serde_json::to_vec(&(contact.trust, contact.mode, contact.sync_out, contact.scope_version, contact.scope_fields, contact.share_protein, selection, hidden, grants))?))
    }

    pub async fn prepare_outgoing_mail(
        &self,
        to_organ: &str,
        root: Option<&str>,
        batch: &crate::sync::OpBatch,
    ) -> Result<store::mailbox::outbox::Envelope, EngineError> {
        let roster = self.roster_of(to_organ).await?.ok_or_else(|| {
            EngineError::Consequence("No recipient roster for outgoing mail".into())
        })?;
        if !crate::roster::roster_signature_is_valid(&roster) {
            return Err(EngineError::Forbidden(
                "The recipient roster has expired".into(),
            ));
        }
        let requested_copies = store::cells::config(&self.store.pool, "lince.social")
            .await?
            .and_then(|value| value["mailbox_copies"].as_i64())
            .unwrap_or(2)
            .clamp(1, 2);
        let policy_hash = self.outgoing_mail_policy_hash(to_organ).await?;
        let bytes = serde_json::to_vec(&(
            "lince.mailbox.intent.v1",
            &policy_hash,
            to_organ,
            root,
            batch,
            &roster.roster.root_key,
            &roster.roster.cells,
            requested_copies,
        ))
        .map_err(EngineError::Json)?;
        let intent = nucleus::fact::sha256_hex(&bytes);
        if let Some(held) = store::mailbox::outbox::get(&self.store.pool, &intent).await? {
            return Ok(held);
        }
        let bundle = self.seal_batch_for(to_organ, root, batch).await?;
        let body = serde_json::to_string(&bundle).map_err(EngineError::Json)?;
        if body.len() > MAX_BUNDLE_BYTES {
            return Err(EngineError::Consequence(
                "This batch exceeds the mailbox envelope limit".into(),
            ));
        }
        let expires_at = chrono::DateTime::from_timestamp(bundle.expires_at, 0)
            .ok_or_else(|| EngineError::Consequence("Invalid outgoing envelope expiry".into()))?
            .to_rfc3339();
        let queued = store::mailbox::outbox::prepare(
            &self.store.pool,
            &store::mailbox::outbox::Envelope {
                intent,
                uid: crate::seal::delivery_id(&bundle),
                to_organ: to_organ.into(),
                body,
                expires_at,
                requested_copies,
                next_attempt: 0,
            },
        )
        .await?;
        store::sqlx::query("INSERT INTO mailbox_outbox_authority(uid, policy_hash) VALUES (?, ?) ON CONFLICT(uid) DO NOTHING").bind(&queued.uid).bind(policy_hash).execute(&self.store.pool).await?;
        Ok(queued)
    }

    pub async fn open_mailed(
        &self,
        bundle: &crate::seal::SealedBundle,
    ) -> Result<crate::seal::OpenedBundle, EngineError> {
        if !store::organs::local(&self.store.pool)
            .await?
            .is_some_and(|organ| organ.uid == bundle.to_organ)
        {
            return Err(EngineError::Forbidden(
                "This mail is addressed to another Organ".into(),
            ));
        }
        let Some(sender) = self.roster_of(&bundle.from_organ).await? else {
            return Err(EngineError::Consequence(
                "a bundle arrived from an Organ whose roster we do not hold".into(),
            ));
        };
        let Some(entry) = sender
            .roster
            .cells
            .iter()
            .find(|cell| cell.cell_uid == bundle.from_cell)
        else {
            return Err(EngineError::Consequence(
                "a bundle claims a Cell that its Organ's roster does not name".into(),
            ));
        };
        if !entry
            .capabilities
            .iter()
            .any(|cap| cap == crate::roster::CAP_WRITE)
        {
            return Err(EngineError::Forbidden(
                "A restricted carrier cannot author private mail".into(),
            ));
        }
        let verifying = crate::seal::verifying_key(&entry.operational_key).ok_or_else(|| {
            EngineError::Consequence("that Cell's published key is unusable".into())
        })?;
        let keyring = self
            .sealing_keyring()
            .await?
            .ok_or_else(|| EngineError::Consequence("this Cell has no mail keys".into()))?;
        crate::seal::open(bundle, &verifying, &keyring.open_keys())
            .map_err(|why| EngineError::Consequence(why.to_string()))
    }

    pub async fn process_recovered_mail(&self) -> Result<usize, EngineError> {
        let _guard = self.mailbox_delivery_lock.lock().await;
        let mut imported = 0;
        for (uid, body) in store::mailbox::delivery::pending(&self.store.pool).await? {
            let result = async {
                let bundle: crate::seal::SealedBundle =
                    serde_json::from_str(&body).map_err(EngineError::Json)?;
                if crate::seal::delivery_id(&bundle) != uid {
                    return Err(EngineError::Consequence(
                        "The mailbox changed the envelope identity".into(),
                    ));
                }
                let opened = self.open_mailed(&bundle).await?;
                self.import_mailed_batch(&opened).await
            }
            .await;
            let error = result.as_ref().err().map(|error| match error {
                EngineError::Store(_) | EngineError::Io(_) => "Storage is temporarily unavailable. The encrypted envelope remains saved.",
                EngineError::Forbidden(_) | EngineError::Conflict { .. } => "This saved message is not currently authorized. Check device and contact permissions before retrying.",
                EngineError::ExecutionLimit(_) => "Message recovery reached its work limit. The encrypted envelope remains saved.",
                _ => "This saved message could not be opened or imported. Sync the sender's roster and this device's mail keys, then retry.",
            });
            if let Ok(count) = result {
                imported += count;
            }
            store::mailbox::delivery::processed(&self.store.pool, &uid, error).await?;
        }
        Ok(imported)
    }
}

pub const INVITE_TTL_DAYS: i64 = 7;

impl crate::Engine {
    pub async fn note_carry_request(
        &self,
        presented: &crate::roster::SignedRoster,
    ) -> Result<(), EngineError> {
        let organ_uid = presented.roster.organ_uid.clone();
        if !crate::roster::roster_signature_is_valid(presented) {
            return Err(EngineError::Consequence(
                "that roster does not verify".into(),
            ));
        }
        if !self
            .key_chains(&organ_uid, &presented.roster.root_key)
            .await?
        {
            return Err(EngineError::Consequence(
                "this Cell holds no key for that Organ, so it cannot tell who is asking. \
                 Pair first, or use an invite code."
                    .into(),
            ));
        }
        let label = store::organs::contact(&self.store.pool, &organ_uid)
            .await?
            .and_then(|contact| contact.slug)
            .unwrap_or_default();
        store::mailbox::ask_to_be_carried(
            &self.store.pool,
            &organ_uid,
            &presented.roster.root_key,
            &label,
        )
        .await?;
        Ok(())
    }

    pub async fn issue_mailbox_invite(
        &self,
        label: &str,
        quota_bytes: i64,
    ) -> Result<String, EngineError> {
        let token = format!(
            "{}{}",
            nucleus::execution::uuid().simple(),
            nucleus::execution::uuid().simple()
        );
        let expires_at =
            (nucleus::execution::now() + chrono::Duration::days(INVITE_TTL_DAYS)).to_rfc3339();
        store::mailbox::put_invite(
            &self.store.pool,
            &crate::roster::hash_token(&token),
            label,
            if quota_bytes > 0 {
                quota_bytes
            } else {
                DEFAULT_QUOTA_BYTES
            },
            &expires_at,
        )
        .await?;
        Ok(token)
    }

    pub async fn redeem_mailbox_invite(
        &self,
        token: &str,
        presented: &crate::roster::SignedRoster,
    ) -> Result<store::mailbox::Registration, EngineError> {
        if !crate::roster::roster_signature_is_valid(presented) {
            return Err(EngineError::Consequence(
                "that roster does not verify".into(),
            ));
        }
        let organ_uid = presented.roster.organ_uid.clone();
        if !store::mailbox::delivery::redeem_and_register(
            &self.store.pool,
            &crate::roster::hash_token(token),
            &organ_uid,
            &presented.roster.root_key,
        )
        .await?
        {
            return Err(EngineError::Consequence(
                "that invite cannot be used: it is unknown, expired, or already spent".into(),
            ));
        }
        store::mailbox::registration(&self.store.pool, &organ_uid)
            .await?
            .ok_or_else(|| EngineError::Consequence("registration did not stick".into()))
    }
}
