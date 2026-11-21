// non-lean-yet
// Scope: impl Engine.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 42.
// - [review: §5 Errors] Check discarded error causes and preserve useful context. Sites 196.
// - [Clippy probe: clippy::as_conversions] Use From or TryFrom when it preserves the conversion
//   contract. Sites 21.
// - [Clippy probe: clippy::map_err_ignore] Preserve relevant causes and context at the boundary.
//   Sites 196.
// - [Clippy probe: clippy::missing_errors_doc] State the observable failure cases for exported
//   functions. Sites 8, 93, 133.
// - [rustc probe: missing_docs] Document the externally visible contracts where applicable. Sites
//   8, 93, 133.

use super::{VerifiedTransferReceipt, invalid, proof_shape_error, withheld};
use crate::{Engine, EngineError};
use chrono::{DateTime, Utc};
use nucleus::transfer::receipt::{Authority, Bundle};
use std::collections::{BTreeMap, BTreeSet};

impl Engine {
    pub async fn verify_transfer_receipt(
        &self,
        bundle: &Bundle,
        now: DateTime<Utc>,
    ) -> Result<VerifiedTransferReceipt, EngineError> {
        bundle
            .validate(now.timestamp())
            .map_err(proof_shape_error)?;
        self.verify_receipt_signatures(bundle, now).await?;
        let _projection = nucleus::performance::Span::new("receipt-head-projection");
        let publication = &bundle.publication;
        nucleus::performance::add(
            "receipt-head-events-projected",
            publication.events.len() as u64,
        );
        let superseded: BTreeSet<_> = publication
            .events
            .iter()
            .filter_map(|signed| signed.event.supersedes.as_ref())
            .collect();
        let mut heads: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for signed in &publication.events {
            let id = signed.event.id().map_err(invalid)?;
            if !superseded.contains(&id) {
                heads
                    .entry(signed.event.slot(&publication.scope).map_err(invalid)?)
                    .or_default()
                    .push(id);
            }
        }
        for ids in heads.values_mut() {
            ids.sort();
        }
        Ok(VerifiedTransferReceipt {
            bundle: bundle.clone(),
            heads,
        })
    }

    async fn verify_receipt_signatures(
        &self,
        bundle: &Bundle,
        now: DateTime<Utc>,
    ) -> Result<(), EngineError> {
        let _span = nucleus::performance::Span::new("receipt-signature-verification");
        for authority in &bundle.authorities {
            self.verify_transfer_receipt_authority(authority).await?;
        }
        let publication = &bundle.publication;
        for original in &publication.scope.original_authorities {
            self.verify_transfer_receipt_authority(&original.authority)
                .await?;
        }
        for approval in &publication.approvals {
            let organ = publication.scope.organ(approval.role);
            self.verify_transfer_receipt_signature(
                &bundle.authorities,
                organ,
                &approval.key_id,
                approval.issued_at,
                &approval
                    .signing_bytes(&publication.scope)
                    .map_err(invalid)?,
                &approval.signature,
            )
            .await?;
        }
        for approval in &publication.person_approvals {
            self.verify_receipt_person_approval(&publication.scope, approval, now)
                .await?;
        }
        for signed in &publication.events {
            self.verify_transfer_receipt_signature(
                &bundle.authorities,
                publication.scope.organ(signed.event.content.role()),
                &signed.key_id,
                signed.event.issued_at,
                &signed.signing_bytes().map_err(invalid)?,
                &signed.signature,
            )
            .await?;
        }
        Ok(())
    }

    pub async fn verify_transfer_receipt_review(
        &self,
        review: &nucleus::transfer::receipt::Review,
        now: DateTime<Utc>,
    ) -> Result<(), EngineError> {
        review.validate(now.timestamp()).map_err(invalid)?;
        self.verify_receipt_review_signatures(review, now).await
    }

    async fn verify_receipt_review_signatures(
        &self,
        review: &nucleus::transfer::receipt::Review,
        now: DateTime<Utc>,
    ) -> Result<(), EngineError> {
        let _span = nucleus::performance::Span::new("receipt-signature-verification");
        for original in &review.scope.original_authorities {
            self.verify_transfer_receipt_authority(&original.authority)
                .await?;
        }
        for authority in &review.authorities {
            self.verify_transfer_receipt_authority(authority).await?;
        }
        for approval in &review.approvals {
            self.verify_transfer_receipt_signature(
                &review.authorities,
                review.scope.organ(approval.role),
                &approval.key_id,
                approval.issued_at,
                &approval.signing_bytes(&review.scope).map_err(invalid)?,
                &approval.signature,
            )
            .await?;
        }
        for approval in &review.person_approvals {
            self.verify_receipt_person_approval(&review.scope, approval, now)
                .await?;
        }
        Ok(())
    }

    pub async fn verify_transfer_receipt_document(
        &self,
        document: &nucleus::transfer::receipt::Document,
        now: DateTime<Utc>,
    ) -> Result<(), EngineError> {
        use nucleus::transfer::receipt::Document;
        let _span = nucleus::performance::Span::new("receipt-document-verification");
        nucleus::performance::add("receipt-validity-only-verifications", 1);
        document
            .validate(now.timestamp())
            .map_err(proof_shape_error)?;
        match document {
            Document::Authority(authority) => {
                self.verify_transfer_receipt_authority(authority).await
            }
            Document::Review(review) => self.verify_receipt_review_signatures(review, now).await,
            Document::Publication(bundle) | Document::ProofFragment(bundle) => {
                self.verify_receipt_signatures(bundle, now).await
            }
        }
    }

    pub(super) async fn verify_transfer_receipt_authority(
        &self,
        authority: &Authority,
    ) -> Result<(), EngineError> {
        let cell = authority
            .key_id
            .strip_prefix("ed25519:cell:")
            .and_then(|value| value.strip_suffix(":v1"))
            .filter(|value| nucleus::valid_uid(value, "r"))
            .ok_or_else(|| invalid("Receipt authority requires a canonical device signing key"))?;
        if let Some(current) = self.roster_of(&authority.organ).await?
            && current.roster.version >= authority.roster_version
        {
            let mut entries = current
                .roster
                .cells
                .iter()
                .filter(|entry| entry.cell_uid == cell);
            let allowed = entries.next().is_some_and(|entry| {
                entry.operational_key == authority.signing_key
                    && entry.may(crate::roster::CAP_REPRESENT)
            }) && entries.next().is_none();
            if !allowed {
                return Err(withheld(
                    "A known current roster withholds this receipt device's authority",
                ));
            }
        }
        let mut previous = None;
        let mut previous_time = None;
        let (root_known, root_current) = Self::root_identity_authority_on(
            &mut *self.store.pool.acquire().await?,
            &authority.organ,
            &authority.root_key,
        )
        .await?;
        let mut anchored = root_current;
        let mut known = root_known;
        let mut seen = BTreeSet::new();
        for change in &authority.successions {
            let at = DateTime::parse_from_rfc3339(&change.created_at)
                .map_err(|_| invalid("Invalid receipt identity succession time"))?;
            if !root_current
                && (store::roster::is_revoked(&self.store.pool, &authority.organ, &change.old_key)
                    .await?
                    || store::roster::is_revoked(
                        &self.store.pool,
                        &authority.organ,
                        &change.new_key,
                    )
                    .await?)
            {
                return Err(withheld("A receipt identity succession key is revoked"));
            }
            if crate::roster::is_root_key_id(&authority.key_id)
                || at.timestamp() > authority.issued_at
                || previous_time.is_some_and(|time| time > at.timestamp())
                || change.old_key == change.new_key
                || previous.is_some_and(|key| key != &change.old_key)
                || !seen.insert(&change.old_key)
                || seen.contains(&change.new_key)
                || !crate::roster::verify_with(
                    &change.old_key,
                    &crate::roster::succession_signing_payload(
                        &authority.organ,
                        &change.old_key,
                        &change.new_key,
                        &change.created_at,
                    ),
                    &change.signature,
                )
            {
                return Err(invalid("Invalid or revoked receipt identity succession"));
            }
            let (old_known, old_current) = Self::root_identity_authority_on(
                &mut *self.store.pool.acquire().await?,
                &authority.organ,
                &change.old_key,
            )
            .await?;
            known |= old_known;
            anchored |= old_current;
            previous = Some(&change.new_key);
            previous_time = Some(at.timestamp());
        }
        if store::roster::is_revoked(&self.store.pool, &authority.organ, &authority.root_key)
            .await?
            || store::roster::is_revoked(&self.store.pool, &authority.organ, &authority.signing_key)
                .await?
        {
            return Err(withheld("A receipt signing or root key is revoked"));
        }
        if !anchored {
            if known {
                return Err(withheld(
                    "The receipt owner's current identity authority is withheld",
                ));
            }
            return Err(EngineError::Conflict {
                code: "transfer_receipt_unknown_identity",
                message: "Recognize this Organ's identity before accepting its receipt authority"
                    .into(),
            });
        }
        if crate::roster::is_root_key_id(&authority.key_id)
            || previous.is_some_and(|key| key != &authority.root_key)
            || !crate::roster::verify_with(
                &authority.root_key,
                &authority.signing_bytes().map_err(invalid)?,
                &authority.signature,
            )
        {
            return Err(invalid(
                "Receipt signing authority has no accepted identity binding",
            ));
        }
        Ok(())
    }

    async fn verify_transfer_receipt_signature(
        &self,
        authorities: &[Authority],
        organ: &str,
        key_id: &str,
        issued_at: i64,
        bytes: &[u8],
        signature: &str,
    ) -> Result<(), EngineError> {
        let mut valid = false;
        let mut eligible = false;
        if crate::roster::is_root_key_id(key_id)
            && let Some(key) = self.current_root_key(organ).await?
            && self.key_chains(organ, &key).await?
        {
            eligible = true;
            valid = crate::roster::verify_with(&key, bytes, signature);
        }
        for authority in authorities {
            if authority.organ == organ
                && authority.participation.is_none()
                && authority.key_id == key_id
                && issued_at >= authority.issued_at
                && issued_at < authority.expires_at
            {
                eligible = true;
                valid |= crate::roster::verify_with(&authority.signing_key, bytes, signature);
                break;
            }
        }
        if !eligible {
            return Err(EngineError::Conflict {
                code: "transfer_receipt_incomplete",
                message: "Receipt signature needs its historical signing authority".into(),
            });
        }
        if !valid {
            return Err(EngineError::Conflict {
                code: "transfer_receipt_signature_invalid",
                message: "Receipt signature is invalid".into(),
            });
        }
        Ok(())
    }
}
