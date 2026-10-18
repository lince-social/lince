use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use nucleus::{
    karma::{CanonicalHash, canonical_hash},
    transfer::{
        TransferActivationEvidence, TransferRevisionEvidence,
        outcome::Content as OutcomeContent,
        receipt::{
            Authority, Bundle, Measurement, PartyAttribution, RecordedFloat, Scope, ScopeRequest,
            Source,
        },
    },
};

use crate::{Engine, EngineError};

mod analysis;
mod browsing;
mod collection;
mod commands;
mod delivery;
mod history;
mod lifecycle;
mod participation;
mod person;
mod projection;
mod sources;

pub use person::sign_person_consent;

#[derive(Debug, Clone)]
pub struct VerifiedTransferReceipt {
    bundle: Bundle,
    heads: BTreeMap<CanonicalHash, Vec<CanonicalHash>>,
}

impl VerifiedTransferReceipt {
    pub fn bundle(&self) -> &Bundle {
        &self.bundle
    }

    pub fn heads(&self) -> &BTreeMap<CanonicalHash, Vec<CanonicalHash>> {
        &self.heads
    }
}

fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::Conflict {
        code: "transfer_receipt_invalid",
        message: message.into(),
    }
}

fn proof_shape_error(message: String) -> EngineError {
    if matches!(
        message.as_str(),
        "Receipt correction is missing its original"
            | "Public Person identities require their own explicit consent signatures"
            | "Receipt statement is missing its Need proof"
            | "Receipt correction history is incomplete"
    ) {
        EngineError::Conflict {
            code: "transfer_receipt_incomplete",
            message,
        }
    } else {
        invalid(message)
    }
}

fn withheld(message: impl Into<String>) -> EngineError {
    EngineError::Conflict {
        code: "transfer_receipt_authority_withheld",
        message: message.into(),
    }
}

impl Engine {
    pub async fn create_transfer_receipt_authority(
        &self,
        root: &crate::trust::Signer,
        cell: &str,
        now: DateTime<Utc>,
    ) -> Result<Authority, EngineError> {
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?;
        if root.actor_uid != organ.uid
            || !crate::roster::is_root_key_id(&root.key_id)
            || !self.key_chains(&organ.uid, &root.public_key_b64()).await?
        {
            return Err(invalid(
                "Receipt authority requires this Organ's accepted owner key",
            ));
        }
        let roster = self.roster_of(&organ.uid).await?.ok_or_else(|| {
            invalid("Refresh the Organ's device roster before authorizing receipts")
        })?;
        let roster_until = DateTime::parse_from_rfc3339(&roster.roster.not_after)
            .map_err(|_| invalid("Invalid receipt device-roster lifetime"))?
            .timestamp();
        if roster.roster.root_key != root.public_key_b64()
            || roster_until <= now.timestamp()
            || !crate::roster::verify_with(
                &roster.roster.root_key,
                &crate::roster::roster_signing_payload(&roster.roster)?,
                &roster.signature,
            )
        {
            return Err(invalid(
                "Refresh a signed current roster before authorizing receipts",
            ));
        }
        let members: Vec<_> = roster
            .roster
            .cells
            .iter()
            .filter(|entry| entry.cell_uid == cell)
            .collect();
        if members.len() != 1 || !members[0].may(crate::roster::CAP_REPRESENT) {
            return Err(invalid(
                "Only a currently authorized representative device may sign public receipts",
            ));
        }
        let member = members[0];
        let mut successions = Vec::new();
        let mut cursor = root.public_key_b64();
        let changes = self.published_successions(&organ.uid).await?;
        let mut seen = BTreeSet::new();
        let anchor = self.current_root_key(&organ.uid).await?;
        while anchor.as_deref() != Some(cursor.as_str()) {
            let Some(change) = changes.iter().rev().find(|change| change.new_key == cursor) else {
                break;
            };
            if successions.len() == 8 || !seen.insert(change.old_key.clone()) {
                return Err(invalid(
                    "Receipt identity history exceeds its bounded proof budget",
                ));
            }
            successions.push(nucleus::social::RootSuccession {
                old_key: change.old_key.clone(),
                new_key: change.new_key.clone(),
                created_at: change.created_at.clone(),
                signature: change.signature.clone(),
            });
            cursor = change.old_key.clone();
        }
        successions.reverse();
        let mut authority = Authority {
            organ: organ.uid,
            roster_version: roster.roster.version,
            root_key: root.public_key_b64(),
            signing_key: member.operational_key.clone(),
            key_id: crate::roster::cell_key_id(cell),
            issued_at: now.timestamp(),
            expires_at: now
                .timestamp()
                .saturating_add(nucleus::transfer::receipt::MAX_AUTHORITY_SECONDS)
                .min(roster_until),
            successions,
            participation: None,
            signature: String::new(),
        };
        authority.signature = root.sign_bytes(&authority.signing_bytes().map_err(invalid)?);
        authority.validate(now.timestamp()).map_err(invalid)?;
        self.verify_transfer_receipt_authority(&authority).await?;
        Ok(authority)
    }

    pub async fn prepare_transfer_receipt_scope(
        &self,
        request: ScopeRequest,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Scope, EngineError> {
        self.require_permission(actor, "transfer:read").await?;
        let roles = request
            .public_person_roles
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        if roles.len() != request.public_person_roles.len()
            || roles.len() > 2
            || roles.contains(&nucleus::transfer::receipt::ApprovalRole::Origin)
        {
            return Err(invalid(
                "Choose each public Person role at most once: giver or receiver",
            ));
        }
        if !nucleus::valid_uid(&request.occurrence, "toc")
            || request.expected_revision == 0
            || request.outcome_records.len() > nucleus::transfer::receipt::MAX_MEASURES
            || request
                .outcome_records
                .iter()
                .any(|uid| !nucleus::valid_uid(uid, "r"))
            || request
                .outcome_records
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != request.outcome_records.len()
        {
            return Err(invalid(
                "Select at most 32 distinct outcome Records for publication review",
            ));
        }
        let occurrence = store::transfers::occurrence(&self.store.pool, &request.occurrence)
            .await?
            .ok_or_else(|| invalid("Choose a resource occurrence"))?;
        self.require_transfer_origin_authority(&occurrence.transfer_uid)
            .await?;
        self.refuse_unreadable(actor, std::slice::from_ref(&occurrence.transfer_uid))
            .await?;
        let current = store::transfers::get(&self.store.pool, &occurrence.transfer_uid)
            .await?
            .ok_or_else(|| invalid("Transfer no longer exists"))?;
        if u64::try_from(current.revision).ok() != Some(request.expected_revision)
            || !occurrence.quantity.is_finite()
            || occurrence.quantity <= 0.0
            || occurrence.giver_person_uid == occurrence.receiver_person_uid
        {
            return Err(invalid(
                "Review the current resource Transfer before preparing a receipt",
            ));
        }
        let agreement = store::transfers::revision_fact(
            &self.store.pool,
            &occurrence.transfer_uid,
            occurrence.revision,
        )
        .await?
        .ok_or_else(|| invalid("Original Transfer revision is unavailable"))?;
        let activation = store::facts::get(&self.store.pool, &occurrence.activation_fact_uid)
            .await?
            .ok_or_else(|| invalid("Original activation evidence is unavailable"))?;
        for fact in [&agreement, &activation] {
            if fact.record_uid != occurrence.transfer_uid
                || !nucleus::fact::verify_chain_step(fact)
                || !crate::trust::verify_fact(&self.store, fact).await?
            {
                return Err(invalid(
                    "Public receipt requires verified original Transfer evidence",
                ));
            }
        }
        let revision: TransferRevisionEvidence = serde_json::from_str(
            agreement
                .payload
                .as_deref()
                .ok_or_else(|| invalid("Missing original agreement payload"))?,
        )?;
        let active: TransferActivationEvidence = serde_json::from_str(
            activation
                .payload
                .as_deref()
                .ok_or_else(|| invalid("Missing original activation payload"))?,
        )?;
        if revision.terms.transfer.uid != occurrence.transfer_uid
            || revision.terms.revision != occurrence.revision
            || active.transfer_uid != occurrence.transfer_uid
            || active.revision != occurrence.revision
            || active.revision_fact_hash.as_deref() != Some(&agreement.hash)
            || active.action != "activate-transfer-occurrence"
            || activation.actor_uid.as_deref() != Some(&active.actor_person_uid)
            || !active.occurrences.iter().any(|original| {
                original.uid == occurrence.uid
                    && original.quantity.to_bits() == occurrence.quantity.to_bits()
                    && original.giver_person_uid == occurrence.giver_person_uid
                    && original.receiver_person_uid == occurrence.receiver_person_uid
                    && original.promise_uid == occurrence.promise_uid
                    && original.record_uid == occurrence.record_uid
                    && original.concept_uid == occurrence.concept_uid
                    && original.unit_uid == occurrence.unit_uid
            })
        {
            return Err(invalid(
                "Receipt source differs from the original signed occurrence",
            ));
        }
        let mut organs = Vec::new();
        for person in [
            &occurrence.giver_person_uid,
            &occurrence.receiver_person_uid,
        ] {
            let frozen = revision
                .terms
                .parties
                .iter()
                .find(|party| &party.person_uid == person)
                .ok_or_else(|| invalid("Receipt party is absent from the signed agreement"))?;
            self.refuse_unreadable(actor, std::slice::from_ref(person))
                .await?;
            let organ = frozen.organ_uid.as_ref()
                .filter(|uid| nucleus::valid_uid(uid, "r"))
                .ok_or_else(|| invalid("Original signed party Organ is unavailable; current membership cannot supply historical attribution"))?;
            let original = active
                .occurrences
                .iter()
                .find(|original| original.uid == occurrence.uid)
                .ok_or_else(|| invalid("Original occurrence is unavailable"))?;
            let activated_organ = if person == &occurrence.giver_person_uid {
                original.giver_organ_uid.as_ref()
            } else {
                original.receiver_organ_uid.as_ref()
            };
            if activated_organ != Some(organ) {
                return Err(invalid(
                    "Activated party Organ differs from the signed agreement",
                ));
            }
            organs.push(organ.clone());
        }
        let origin = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?
            .uid;
        if revision.terms.transfer.origin_organ_uid.as_deref() != Some(&origin) {
            return Err(invalid(
                "Original signed source Organ differs from the canonical origin",
            ));
        }
        if request.public_measurements.len() > nucleus::transfer::receipt::MAX_MEASURES {
            return Err(invalid("Select at most 32 public measurements"));
        }
        let mut measurements = BTreeMap::new();
        for measure in &request.public_measurements {
            if measurements
                .insert(measure.id().map_err(invalid)?, measure.clone())
                .is_some()
            {
                return Err(invalid("Public measurements must not repeat"));
            }
        }
        for uid in &request.outcome_records {
            self.refuse_unreadable(actor, std::slice::from_ref(uid))
                .await?;
            let outcome = store::transfer_evidence::get(&self.store.pool, uid).await?;
            let expected_author = match &outcome.evidence.content {
                OutcomeContent::NeedBinding { .. }
                | OutcomeContent::Statement {
                    role: nucleus::transfer::outcome::Role::Receiver,
                    ..
                } => &occurrence.receiver_person_uid,
                _ => &occurrence.giver_person_uid,
            };
            if outcome.evidence.occurrence != occurrence.uid
                || outcome.evidence.transfer != occurrence.transfer_uid
                || outcome.evidence.author != *expected_author
                || outcome.evidence.agreement_revision != occurrence.revision
                || !crate::trust::verify_fact(&self.store, &outcome.fact).await?
            {
                return Err(invalid(
                    "Selected outcome lacks its original role or signing proof",
                ));
            }
            let mut dependencies = Vec::new();
            if let OutcomeContent::NeedBinding { terms, .. } = &outcome.evidence.content {
                dependencies.push(terms.selection.need.clone());
            }
            if let OutcomeContent::Statement {
                need_binding: Some(binding),
                ..
            } = &outcome.evidence.content
            {
                let binding = store::transfer_evidence::get(&self.store.pool, binding).await?;
                dependencies.push(binding.evidence.record);
                if let OutcomeContent::NeedBinding { terms, .. } = binding.evidence.content {
                    dependencies.push(terms.selection.need);
                }
            }
            let measure = outcome.evidence.content.measure();
            if let Some(object) = &measure.object {
                dependencies.push(object.clone());
            }
            if let Some(previous) = outcome.evidence.content.supersedes() {
                dependencies.push(previous.to_owned());
            }
            self.refuse_unreadable(actor, &dependencies).await?;
            let public = Measurement {
                meaning: measure.meaning.clone(),
                unit: measure.unit.clone(),
                object_commitment: measure
                    .object
                    .as_ref()
                    .map(|object| {
                        canonical_hash("lince.transfer.receipt.object.v1", &(&origin, object))
                            .map_err(|error| invalid(error.to_string()))
                    })
                    .transpose()?,
            };
            measurements.insert(public.id().map_err(invalid)?, public);
        }
        let mut scope = Scope {
            audience: request.audience,
            upfront_terms: None,
            public_persons: roles
                .into_iter()
                .map(|role| nucleus::transfer::receipt::PublicPerson {
                    role,
                    person: if role == nucleus::transfer::receipt::ApprovalRole::Giver {
                        occurrence.giver_person_uid.clone()
                    } else {
                        occurrence.receiver_person_uid.clone()
                    },
                })
                .collect(),
            party_attribution: PartyAttribution::LaterOrganEndorsement,
            original_authorities: Vec::new(),
            source: Source {
                occurrence: Source::occurrence_id(&origin, &occurrence.uid).map_err(invalid)?,
                origin,
                activated_at: activation.at.timestamp(),
                agreement_revision: occurrence.revision,
                agreement_commitment: Source::fact_commitment(&agreement.hash).map_err(invalid)?,
                activation_commitment: Source::fact_commitment(&activation.hash)
                    .map_err(invalid)?,
            },
            giver: organs[0].clone(),
            receiver: organs[1].clone(),
            measurements: measurements.into_values().collect(),
            stages: request.stages,
            agreed_quantity: request
                .publish_agreed_quantity
                .then(|| RecordedFloat::new(occurrence.quantity))
                .transpose()
                .map_err(invalid)?,
            canonical_unit: None,
            issued_at: now.timestamp(),
            expires_at: request.expires_at,
        };
        if request.use_upfront_terms {
            if let Some(terms) = &revision.terms.transfer.public_history {
                let expiry = scope.source.activated_at.checked_add(i64::from(terms.retention_seconds)).ok_or_else(|| invalid("The public-history term exceeds the supported date range"))?;
                scope.expires_at = scope.expires_at.min(expiry);
                if scope.expires_at <= now.timestamp() || !terms.permits(&scope) {
                    return Err(invalid("Review a new scope for renewal or expanded public fields/audience; the original public-history terms do not authorize this projection"));
                }
                scope.upfront_terms = Some(terms.clone());
            }
        }
        self.attach_original_participation(
            &mut scope,
            &revision,
            &agreement.hash,
            &active,
            &occurrence.giver_person_uid,
            &occurrence.receiver_person_uid,
        )
        .await?;
        scope.validate().map_err(invalid)?;
        Ok(scope)
    }

    pub async fn verify_transfer_receipt(
        &self,
        bundle: &Bundle,
        now: DateTime<Utc>,
    ) -> Result<VerifiedTransferReceipt, EngineError> {
        bundle
            .validate(now.timestamp())
            .map_err(proof_shape_error)?;
        self.verify_receipt_signatures(bundle, now).await?;
        let publication = &bundle.publication;
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

    pub(crate) async fn verify_transfer_receipt_fragment(
        &self,
        bundle: &Bundle,
        now: DateTime<Utc>,
    ) -> Result<(), EngineError> {
        bundle
            .validate_fragment(now.timestamp())
            .map_err(proof_shape_error)?;
        self.verify_receipt_signatures(bundle, now).await
    }

    async fn verify_receipt_signatures(
        &self,
        bundle: &Bundle,
        now: DateTime<Utc>,
    ) -> Result<(), EngineError> {
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
                bundle,
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
                bundle,
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
        let bundle = review.as_bundle();
        for original in &review.scope.original_authorities {
            self.verify_transfer_receipt_authority(&original.authority)
                .await?;
        }
        for authority in &review.authorities {
            self.verify_transfer_receipt_authority(authority).await?;
        }
        for approval in &review.approvals {
            self.verify_transfer_receipt_signature(
                &bundle,
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
        document
            .validate(now.timestamp())
            .map_err(proof_shape_error)?;
        match document {
            Document::Authority(authority) => {
                self.verify_transfer_receipt_authority(authority).await
            }
            Document::Review(review) => self.verify_transfer_receipt_review(review, now).await,
            Document::Publication(bundle) => {
                self.verify_transfer_receipt(bundle, now).await.map(|_| ())
            }
            Document::ProofFragment(bundle) => {
                self.verify_transfer_receipt_fragment(bundle, now).await
            }
        }
    }

    async fn verify_transfer_receipt_authority(
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
        bundle: &Bundle,
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
        for authority in &bundle.authorities {
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
