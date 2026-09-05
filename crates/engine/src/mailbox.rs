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

        let registration = store::mailbox::registration(&self.store.pool, &bundle.to_organ)
            .await
            .map_err(|_| Refusal::Unreadable)?
            .ok_or(Refusal::NotRegistered)?;
        let held = store::mailbox::held_bytes(&self.store.pool, &bundle.to_organ)
            .await
            .map_err(|_| Refusal::Unreadable)?;
        let bytes = body.len() as i64;
        if held + bytes > registration.quota_bytes {
            return Err(Refusal::QuotaFull);
        }

        let now = chrono::Utc::now();
        let uid = format!("mb-{}", uuid::Uuid::new_v4());
        store::mailbox::deposit(
            &self.store.pool,
            &store::mailbox::HeldBundle {
                uid: uid.clone(),
                to_organ: bundle.to_organ.clone(),
                from_organ: bundle.from_organ.clone(),
                from_cell: bundle.from_cell.clone(),
                from_node: from_node.to_string(),
                body: body.to_string(),
                bytes,
                received_at: now.to_rfc3339(),
                expires_at: (now + chrono::Duration::days(crate::seal::RETENTION_DAYS))
                    .to_rfc3339(),
            },
        )
        .await
        .map_err(|_| Refusal::Unreadable)?;
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
        Ok(presented
            .roster
            .cells
            .iter()
            .any(|cell| cell.node_id == node_id))
    }

    pub async fn bundles_for(
        &self,
        organ_uid: &str,
        limit: i64,
    ) -> Result<Vec<store::mailbox::HeldBundle>, EngineError> {
        Ok(store::mailbox::for_recipient(&self.store.pool, organ_uid, limit).await?)
    }

    pub async fn confirm_collected(
        &self,
        organ_uid: &str,
        uids: &[String],
    ) -> Result<u64, EngineError> {
        Ok(store::mailbox::collected(&self.store.pool, organ_uid, uids).await?)
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
        let now = chrono::Utc::now();
        let recipients: Vec<crate::seal::SealingKey> = their_roster
            .roster
            .cells
            .iter()
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

    pub async fn open_mailed(
        &self,
        bundle: &crate::seal::SealedBundle,
    ) -> Result<crate::seal::OpenedBundle, EngineError> {
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
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let expires_at =
            (chrono::Utc::now() + chrono::Duration::days(INVITE_TTL_DAYS)).to_rfc3339();
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
        let Some((label, quota)) = store::mailbox::redeem_invite(
            &self.store.pool,
            &crate::roster::hash_token(token),
            &organ_uid,
        )
        .await?
        else {
            return Err(EngineError::Consequence(
                "that invite cannot be used: it is unknown, expired, or already spent".into(),
            ));
        };
        store::mailbox::register(
            &self.store.pool,
            &organ_uid,
            &presented.roster.root_key,
            &label,
            quota,
        )
        .await?;
        store::mailbox::registration(&self.store.pool, &organ_uid)
            .await?
            .ok_or_else(|| EngineError::Consequence("registration did not stick".into()))
    }
}
