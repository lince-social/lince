//! Carrying mail for other Organs (Ontology C4).
//!
//! A mailbox is a SERVICE a Cell offers, not a mode. Modes are mutually
//! exclusive because they choose which binary runs with which flags and
//! limits; carrying mail is additive, so an Institute running one VPS holds
//! its own People once, in its own Cell, and carries mail for others from the
//! same process rather than deploying a second Cell to duplicate them into.
//!
//! Three properties define the role, and each one is enforced here rather than
//! promised:
//!
//! 1. **It converges nothing.** No function in this module reaches the sync
//!    path. A bundle arrives opaque, is stored opaque, and leaves opaque.
//! 2. **It holds no key that opens what it carries.** Bundles are sealed to
//!    the recipient's Cells; the carrier is not one of them, which is the
//!    whole reason a third party may hold them at all.
//! 3. **It accepts from anyone, but only FOR a registered recipient.**
//!    Limiting senders to the carrier's own circle would halve the point,
//!    since being reachable while offline matters most for people outside it.
//!
//! # Who may collect
//!
//! The hard part is not the lookup, it is DEVICE ADDITION: a recipient who
//! enrols a new phone must be able to collect from it, so a mailbox that
//! snapshotted node ids at registration would lock them out of their own mail.
//!
//! So registration stores the recipient's ROOT key, and a collector presents
//! its current signed roster. The roster must chain from that root key, and
//! the connection's node id must be one the roster names. The mailbox
//! therefore tracks device changes without polling for them, and a revoked
//! device stops collecting as soon as its Organ publishes the roster that
//! removed it — the same eventual-consistency the rest of revocation has.

use crate::error::EngineError;
use crate::roster::SignedRoster;

/// The most one deposited bundle may weigh.
///
/// Far below `wire::MAX_FRAME_BYTES`, and stated at the door rather than
/// discovered: the frame cap protects this Cell's memory, while this protects
/// a RECIPIENT's quota from being spent by one caller in one frame. A sender
/// with more to say sends more bundles, which is what batching already does.
pub const MAX_BUNDLE_BYTES: usize = 1024 * 1024;

/// What a new registration may hold, in bytes, unless the operator says
/// otherwise. Deliberately modest: a mailbox is a favour with a disk behind
/// it, and an operator should be able to size the box by member count.
pub const DEFAULT_QUOTA_BYTES: i64 = 64 * 1024 * 1024;

/// Why a deposit was turned away.
///
/// Separated from a generic error because these are ANSWERS, not faults: a
/// sender that hears `NotRegistered` should stop trying and tell its owner
/// that this carrier does not serve that recipient, while one that hears
/// `QuotaFull` should retry later or try the recipient's other pickup point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// This box carries nothing for that Organ. Says nothing about whether the
    /// Organ exists — a mailbox is not a directory.
    NotRegistered,
    /// The recipient's quota is spent. Their problem to resolve, by collecting
    /// or by asking the operator for more, which is the right incentive: a
    /// bundle is charged to the person who chose this mailbox.
    QuotaFull,
    /// Bigger than `MAX_BUNDLE_BYTES`.
    TooLarge,
    /// Not a sealed bundle at all, or one this build cannot parse.
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
    /// The stable code a peer matches on.
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
    /// Whether this Cell is carrying mail for anyone.
    ///
    /// The service is on when there is something to carry FOR, which makes the
    /// switch the registration list rather than a separate flag that could
    /// disagree with it. An operator turning the service off deregisters, and
    /// the recipients' mail goes with them rather than sitting unreachable.
    pub async fn is_a_mailbox(&self) -> Result<bool, EngineError> {
        Ok(!store::mailbox::registrations(&self.store.pool)
            .await?
            .is_empty())
    }

    /// Take a sealed bundle for a registered recipient.
    ///
    /// Deliberately NOT verifying the bundle's signature: the carrier has no
    /// business deciding whether a sender is one the recipient wants to hear
    /// from, and could not judge it anyway — it holds neither the recipient's
    /// contacts nor their trust decisions. The signature is checked by the
    /// recipient on opening, which is the only place it means anything. What
    /// the carrier checks is what a carrier can: is this for someone I serve,
    /// does it fit, and is it shaped like a bundle at all.
    pub async fn accept_bundle(&self, body: &str, from_node: &str) -> Result<String, Refusal> {
        if body.len() > MAX_BUNDLE_BYTES {
            return Err(Refusal::TooLarge);
        }
        let bundle: crate::seal::SealedBundle =
            serde_json::from_str(body).map_err(|_| Refusal::Unreadable)?;
        if bundle.v != crate::seal::SEAL_VERSION {
            // Fail closed on a construction we do not implement, rather than
            // storing bytes we cannot describe. `AGENTS.md`: never negotiate
            // down, and there is no installed base to be gentle with.
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
                // One number with the sealing-key retention window, so nothing
                // uncollected is ever lost to a key rotation.
                expires_at: (now + chrono::Duration::days(crate::seal::RETENTION_DAYS))
                    .to_rfc3339(),
            },
        )
        .await
        .map_err(|_| Refusal::Unreadable)?;
        Ok(uid)
    }

    /// Decide whether the Cell on this connection may collect for `organ_uid`,
    /// given the roster it presented.
    ///
    /// Three conditions, and dropping any one of them opens a different hole:
    /// the Organ must be registered here (otherwise this is a stranger asking
    /// about somebody else's mail), the roster must chain from the root key
    /// registration recorded (otherwise anyone could present a roster they
    /// signed themselves), and the connection's node id must be a Cell that
    /// roster names (otherwise holding a copy of someone's public roster —
    /// which every contact does — would be enough to collect their mail).
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
        // Chaining, not equality: an Organ that rotated its root key since
        // registering is still itself, and the succession is what says so.
        // Equality here would break every recipient the first time they
        // rotated, which is the one moment they most need their mail.
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

    /// Hand over what is held, without deleting it.
    ///
    /// Collection is CONFIRMED separately: a bundle deleted when it was sent
    /// but never received is gone for good, and the whole point of a mailbox
    /// is that mail survives the recipient being unreachable. So the delete
    /// waits for the recipient to say it landed.
    pub async fn bundles_for(
        &self,
        organ_uid: &str,
        limit: i64,
    ) -> Result<Vec<store::mailbox::HeldBundle>, EngineError> {
        Ok(store::mailbox::for_recipient(&self.store.pool, organ_uid, limit).await?)
    }

    /// Drop bundles the recipient confirmed, and report how many went.
    pub async fn confirm_collected(
        &self,
        organ_uid: &str,
        uids: &[String],
    ) -> Result<u64, EngineError> {
        Ok(store::mailbox::collected(&self.store.pool, organ_uid, uids).await?)
    }

    /// Delete everything past its retention date, leaving the sender notices
    /// behind. Safe and cheap to run on a timer or at boot.
    pub async fn sweep_mailbox(&self) -> Result<u64, EngineError> {
        Ok(store::mailbox::sweep_expired(&self.store.pool).await?)
    }

    /// What this box owes the sender on the far end of `from_node`: every
    /// bundle of theirs that expired uncollected.
    ///
    /// Handing them over stamps `notified_at`, so the operator's "senders
    /// still to be told" count means what it says. The rows survive until the
    /// sender acknowledges them, because a Cell that crashes between the
    /// answer and the ack must be told again rather than never.
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

    /// Drop the notices a sender acknowledged.
    pub async fn forget_expiries(
        &self,
        from_node: &str,
        uids: &[String],
    ) -> Result<u64, EngineError> {
        Ok(store::mailbox::notices_heard(&self.store.pool, from_node, uids).await?)
    }

    /// Believe a carrier's expiry report only where it names mail we left with
    /// THAT carrier, and return how many of our own deposits it accounted for.
    ///
    /// A uid we have no row for is not an error and not a warning: an operator
    /// who re-created their box, or a carrier reporting somebody else's
    /// bundle, both look like this, and neither is worth a person's attention.
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

    /// Seal a batch for `to_organ`, readable by every device of theirs that
    /// published a mail key and by nobody else.
    ///
    /// Refuses rather than degrading. An Organ none of whose Cells publishes a
    /// sealing key simply cannot be mailed, and the only alternative to saying
    /// so is leaving something readable with a third party, which is the one
    /// thing this whole path exists to prevent.
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
            // An expired key is not sealed to: the private half was meant to be
            // gone, so a bundle sealed to it is one nobody can ever open.
            .filter(|key| {
                chrono::DateTime::parse_from_rfc3339(&key.not_after)
                    .map(|when| when.with_timezone(&chrono::Utc) > now)
                    .unwrap_or(false)
            })
            .collect();
        let Some(cell) = store::cells::local(&self.store.pool).await? else {
            return Err(EngineError::Consequence("this Cell has no Cell Record".into()));
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

    /// Open a bundle collected from a carrier, verifying the SENDER.
    ///
    /// Out of a mailbox there is no connection to say who sent this, so the
    /// signature is the anchor and the sender's own roster is what checks it.
    /// A bundle from an Organ we hold no roster for cannot be opened at all,
    /// which is correct: you can only be mailed by someone you already know.
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
        let verifying = crate::seal::verifying_key(&entry.operational_key)
            .ok_or_else(|| EngineError::Consequence("that Cell's published key is unusable".into()))?;
        let keyring = self
            .sealing_keyring()
            .await?
            .ok_or_else(|| EngineError::Consequence("this Cell has no mail keys".into()))?;
        crate::seal::open(bundle, &verifying, &keyring.open_keys())
            .map_err(|why| EngineError::Consequence(why.to_string()))
    }
}

/// How long a mailbox invite stays good.
///
/// Seven days, against enrolment's ten minutes, and the difference is the
/// point. An enrolment code is read off a screen in the next few minutes and
/// grants membership in an identity; this one is sent in a message and grants
/// the right to leave sealed bytes on your own quota. It has to survive the
/// other person getting to their messages tomorrow, and what it grants is
/// small enough that it can.
pub const INVITE_TTL_DAYS: i64 = 7;

impl crate::Engine {
    /// Take an ask from an Organ that wants its mail carried here.
    ///
    /// Refused unless we already hold a root key for them. That is the bound
    /// on this table: one row per Organ we have a relationship with, so no
    /// rate limit is needed to keep a stranger from filling a disk with
    /// requests — a stranger cannot make a row at all. Someone with no prior
    /// relationship gets in the other way, by redeeming a code the operator
    /// handed them, which is the same consent given in advance.
    pub async fn note_carry_request(
        &self,
        presented: &crate::roster::SignedRoster,
    ) -> Result<(), EngineError> {
        let organ_uid = presented.roster.organ_uid.clone();
        if !crate::roster::roster_signature_is_valid(presented) {
            return Err(EngineError::Consequence("that roster does not verify".into()));
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
        // Their own label for themselves, and nothing more: it goes beside the
        // uid in the operator's panel, never instead of it.
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

    /// Issue a single-use code that lets whoever holds it register themselves.
    ///
    /// Returns the PLAINTEXT, which exists only here and wherever the operator
    /// pastes it. The row holds a hash.
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

    /// Spend a code and register the presenter.
    ///
    /// The roster is checked against ITSELF here, not against a key we hold —
    /// we hold none, which is the whole reason a code exists. That is
    /// trust-on-first-use, the same as pairing and enrolment, and the code is
    /// what carries the trust: it travelled a channel the operator chose.
    ///
    /// The claim happens FIRST. A claim that fails means unknown, expired or
    /// already spent, and none of those may go on to touch the registration
    /// table.
    pub async fn redeem_mailbox_invite(
        &self,
        token: &str,
        presented: &crate::roster::SignedRoster,
    ) -> Result<store::mailbox::Registration, EngineError> {
        if !crate::roster::roster_signature_is_valid(presented) {
            return Err(EngineError::Consequence("that roster does not verify".into()));
        }
        let organ_uid = presented.roster.organ_uid.clone();
        let Some((label, quota)) = store::mailbox::redeem_invite(
            &self.store.pool,
            &crate::roster::hash_token(token),
            &organ_uid,
        )
        .await?
        else {
            // One wording for all three causes. Telling them apart would let a
            // holder of many guessed codes learn which ones ever existed.
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
