use super::*;
use nucleus::transfer::{
    TransferAgreementTransitionEvidence, TransferOccurrenceSettlementApplicationEvidence,
    TransferOccurrenceSettlementCompensationEvidence, TransferOccurrenceSettlementEvidence,
    outcome::Role, receipt::Content,
};
use store::sqlx::Row;

impl Engine {
    async fn receipt_source_fact(&self, uid: &str) -> Result<nucleus::Fact, EngineError> {
        let fact = store::facts::get(&self.store.pool, uid)
            .await?
            .ok_or_else(|| invalid("Original lifecycle evidence is unavailable"))?;
        if !nucleus::fact::verify_chain_step(&fact)
            || !crate::trust::verify_fact(&self.store, &fact).await?
        {
            return Err(invalid(
                "Lifecycle publication requires verified original evidence",
            ));
        }
        Ok(fact)
    }

    pub(super) async fn receipt_settlement(
        &self,
        occurrence: &store::transfers::TransferOccurrenceRow,
        uid: &str,
    ) -> Result<
        (
            nucleus::Fact,
            store::transfers::OccurrenceSettlementSliceRow,
        ),
        EngineError,
    > {
        let fact = self.receipt_source_fact(uid).await?;
        let evidence: TransferOccurrenceSettlementEvidence = serde_json::from_str(
            fact.payload
                .as_deref()
                .ok_or_else(|| invalid("Settlement payload is unavailable"))?,
        )?;
        let slice = store::transfers::occurrence_settlement_slice(
            &self.store.pool,
            &evidence.settlement_uid,
        )
        .await?
        .ok_or_else(|| invalid("Original settlement slice is unavailable"))?;
        let revision = store::transfers::revision_fact(
            &self.store.pool,
            &occurrence.transfer_uid,
            occurrence.revision,
        )
        .await?
        .ok_or_else(|| invalid("Original settlement agreement is unavailable"))?;
        let revision = self.receipt_source_fact(&revision.uid).await?;
        let terms: TransferRevisionEvidence = serde_json::from_str(
            revision
                .payload
                .as_deref()
                .ok_or_else(|| invalid("Original settlement agreement is unavailable"))?,
        )?;
        let source_owner = terms
            .terms
            .promises
            .iter()
            .find(|promise| promise.uid == occurrence.promise_uid)
            .and_then(|promise| promise.person_uid.as_deref())
            .ok_or_else(|| invalid("Original settlement source owner is unavailable"))?;
        if evidence.action != "settle-transfer-occurrence"
            || evidence.transfer_uid != occurrence.transfer_uid
            || evidence.occurrence_uid != occurrence.uid
            || evidence.promise_uid != occurrence.promise_uid
            || evidence.owner_person_uid != source_owner
            || fact.actor_uid.as_deref() != Some(&evidence.owner_person_uid)
            || fact.record_uid != occurrence.transfer_uid
            || !fact.delta.is_zero()
            || slice.evidence_fact_uid != fact.uid
            || slice.occurrence_uid != occurrence.uid
            || slice.transfer_uid != occurrence.transfer_uid
            || slice.promise_uid != occurrence.promise_uid
            || slice.owner_person_uid != evidence.owner_person_uid
            || evidence.cumulative_after.to_bits() != slice.cumulative_after.to_bits()
            || evidence.cumulative_before.to_bits() != slice.cumulative_before.to_bits()
            || evidence.canonical_quantity.to_bits() != slice.canonical_quantity.to_bits()
            || evidence.remaining_after.to_bits() != slice.remaining_after.to_bits()
            || evidence.canonical_unit_uid != occurrence.unit_uid
            || evidence.canonical_unit_uid != slice.canonical_unit_uid
        {
            return Err(invalid(
                "Settlement differs from verified original source evidence",
            ));
        }
        RecordedFloat::new(slice.canonical_quantity).map_err(invalid)?;
        RecordedFloat::new(slice.cumulative_before).map_err(invalid)?;
        RecordedFloat::new(slice.cumulative_after).map_err(invalid)?;
        RecordedFloat::new(slice.remaining_after).map_err(invalid)?;
        Ok((fact, slice))
    }

    pub(super) async fn receipt_cancellation(
        &self,
        occurrence: &store::transfers::TransferOccurrenceRow,
        uid: &str,
    ) -> Result<(nucleus::Fact, Content), EngineError> {
        #[derive(serde::Deserialize)]
        struct Applied {
            action: String,
            cancellation: String,
            revision: u64,
            quantity: nucleus::DecimalValue,
            occurrences: Vec<String>,
            agreement_facts: Vec<String>,
            request_id: String,
        }
        let fact = self.receipt_source_fact(uid).await?;
        let applied: Applied = serde_json::from_str(
            fact.payload
                .as_deref()
                .ok_or_else(|| invalid("Cancellation payload is unavailable"))?,
        )?;
        let row = store::sqlx::query("SELECT c.*, a.person_uid AS applied_by, a.request_id AS application_request, oc.quantity_mantissa AS effect_mantissa, oc.quantity_scale AS effect_scale FROM transfer_cancellation c JOIN transfer_occurrence_cancellation oc ON oc.cancellation_uid = c.uid JOIN transfer_cancellation_application a ON a.cancellation_uid = c.uid AND a.fact_uid = c.applied_fact_uid WHERE c.uid = ? AND c.transfer_uid = ? AND c.applied_fact_uid = ? AND oc.occurrence_uid = ? AND oc.fact_uid = c.applied_fact_uid")
            .bind(&applied.cancellation).bind(&occurrence.transfer_uid).bind(uid).bind(&occurrence.uid)
            .fetch_optional(&self.store.pool).await?
            .ok_or_else(|| invalid("Choose an applied cancellation, not a proposal"))?;
        let people: Vec<String> = serde_json::from_str(&row.get::<String, _>("required_people"))?;
        let occurrences: Vec<String> = serde_json::from_str(&row.get::<String, _>("occurrences"))?;
        let quantity = store::exact::read_decimal(&row, "quantity")?;
        let effect = store::exact::read_decimal(&row, "effect")?;
        if applied.action != "apply-transfer-cancellation"
            || applied.revision != row.get::<i64, _>("revision") as u64
            || applied.revision <= occurrence.revision
            || applied.quantity != quantity
            || effect != quantity
            || !quantity.is_positive()
            || applied.occurrences != occurrences
            || !occurrences.contains(&occurrence.uid)
            || applied.request_id != row.get::<String, _>("application_request")
            || fact.record_uid != occurrence.transfer_uid
            || fact.actor_uid.as_deref() != Some(row.get::<String, _>("applied_by").as_str())
            || !fact.delta.is_zero()
            || people.is_empty()
            || people.len() > 128
            || applied.agreement_facts.len() != people.len()
            || people.iter().collect::<BTreeSet<_>>().len() != people.len()
            || !people
                .iter()
                .any(|person| Some(person) == fact.actor_uid.as_ref())
        {
            return Err(invalid(
                "Cancellation differs from its original applied terms",
            ));
        }
        let proposal_uid: String = row.get("proposal_fact_uid");
        let proposal = self.receipt_source_fact(&proposal_uid).await?;
        let revision: TransferRevisionEvidence = serde_json::from_str(
            proposal
                .payload
                .as_deref()
                .ok_or_else(|| invalid("Cancellation revision is unavailable"))?,
        )?;
        let recorded_revision = store::transfers::revision_fact(
            &self.store.pool,
            &occurrence.transfer_uid,
            applied.revision,
        )
        .await?
        .ok_or_else(|| invalid("Cancellation revision is unavailable"))?;
        let terms = revision
            .terms
            .cancellations
            .iter()
            .find(|terms| terms.uid == applied.cancellation)
            .ok_or_else(|| invalid("Cancellation is missing from its signed revision"))?;
        if proposal.uid != recorded_revision.uid
            || proposal.record_uid != occurrence.transfer_uid
            || !proposal.delta.is_zero()
            || proposal.actor_uid.as_deref() != Some(row.get::<String, _>("proposer_uid").as_str())
            || revision.action != "propose-transfer-cancellation"
            || revision.terms.transfer.uid != occurrence.transfer_uid
            || revision.terms.revision != applied.revision
            || terms.quantity != quantity
            || terms.occurrences != occurrences
            || terms.required_people != people
            || terms.exchange_path_uid != occurrence.exchange_path_uid
        {
            return Err(invalid(
                "Cancellation requires its original signed proposal revision",
            ));
        }
        let mut agreed = BTreeSet::new();
        for uid in &applied.agreement_facts {
            let approval = self.receipt_source_fact(uid).await?;
            let evidence: TransferAgreementTransitionEvidence = serde_json::from_str(
                approval
                    .payload
                    .as_deref()
                    .ok_or_else(|| invalid("Cancellation agreement is unavailable"))?,
            )?;
            if evidence.action != "set-transfer-agreement-level"
                || evidence.transfer_uid != occurrence.transfer_uid
                || evidence.revision != applied.revision
                || evidence.revision_fact_hash.as_deref() != Some(proposal.hash.as_str())
                || evidence.from_level != 1
                || evidence.to_level != 2
                || !people.contains(&evidence.person_uid)
                || !revision.terms.parties.iter().any(|party| {
                    party.uid == evidence.party_uid
                        && party.person_uid == evidence.person_uid
                        && party.organ_uid == evidence.organ_uid
                })
                || approval.actor_uid.as_deref() != Some(&evidence.person_uid)
                || approval.record_uid != occurrence.transfer_uid
                || !approval.delta.is_zero()
                || !agreed.insert(evidence.person_uid)
            {
                return Err(invalid(
                    "Cancellation requires every affected party's signed agreement",
                ));
            }
        }
        Ok((
            fact,
            Content::Cancelled {
                quantity,
                revision: applied.revision,
            },
        ))
    }

    pub(super) async fn receipt_compensation(
        &self,
        person: &str,
        actor: Option<&str>,
        occurrence: &store::transfers::TransferOccurrenceRow,
        uid: &str,
    ) -> Result<(nucleus::Fact, Content), EngineError> {
        let fact = self.receipt_source_fact(uid).await?;
        let evidence: TransferOccurrenceSettlementCompensationEvidence = serde_json::from_str(
            fact.payload
                .as_deref()
                .ok_or_else(|| invalid("Compensation payload is unavailable"))?,
        )?;
        let correction =
            store::transfers::occurrence_settlement_compensation_for_fact(&self.store.pool, uid)
                .await?
                .ok_or_else(|| invalid("Original compensation is unavailable"))?;
        let slice = store::transfers::occurrence_settlement_slice(
            &self.store.pool,
            &evidence.settlement_uid,
        )
        .await?
        .ok_or_else(|| invalid("Original compensated settlement is unavailable"))?;
        self.refuse_unreadable(actor, std::slice::from_ref(&slice.local_record_uid))
            .await?;
        let (settlement, _) = self
            .receipt_settlement(occurrence, &slice.evidence_fact_uid)
            .await?;
        let application = self
            .receipt_source_fact(&slice.application_fact_uid)
            .await?;
        let applied: TransferOccurrenceSettlementApplicationEvidence = serde_json::from_str(
            application
                .payload
                .as_deref()
                .ok_or_else(|| invalid("Original application is unavailable"))?,
        )?;
        if evidence.action != "compensate-transfer-occurrence-settlement"
            || evidence.compensation_uid != correction.uid
            || evidence.settlement_uid != correction.settlement_uid
            || evidence.original_application_fact_uid != correction.original_application_fact_uid
            || evidence.owner_person_uid != person
            || evidence.owner_person_uid != slice.owner_person_uid
            || evidence.owner_person_uid != correction.owner_person_uid
            || evidence.idempotency_key != correction.idempotency_key
            || evidence.transfer_uid != occurrence.transfer_uid
            || evidence.transfer_uid != correction.transfer_uid
            || evidence.occurrence_uid != occurrence.uid
            || evidence.occurrence_uid != correction.occurrence_uid
            || evidence.original_application_fact_uid != slice.application_fact_uid
            || fact.actor_uid.as_deref() != Some(person)
            || application.actor_uid.as_deref() != Some(person)
            || fact.record_uid != slice.local_record_uid
            || fact.record_uid != correction.local_record_uid
            || application.record_uid != slice.local_record_uid
            || applied.action != "apply-transfer-occurrence-settlement"
            || applied.settlement_uid != slice.uid
            || applied.evidence_fact_uid != settlement.uid
            || applied.owner_person_uid != person
            || applied.transfer_uid != occurrence.transfer_uid
            || applied.occurrence_uid != occurrence.uid
            || applied.promise_uid != occurrence.promise_uid
            || fact.cause.kind != nucleus::CauseKind::Compensation
            || fact.cause.uid.as_deref() != Some(&application.uid)
            || store::exact::negate(application.delta)? != fact.delta
        {
            return Err(invalid(
                "Compensation differs from its original owner's application",
            ));
        }
        let role = if person == occurrence.giver_person_uid {
            Role::Giver
        } else if person == occurrence.receiver_person_uid {
            Role::Receiver
        } else {
            return Err(invalid(
                "Only the original party can publish their compensation",
            ));
        };
        Ok((
            fact,
            Content::Compensated {
                role,
                settlement: Source::fact_commitment(&settlement.hash).map_err(invalid)?,
                application: Source::fact_commitment(&application.hash).map_err(invalid)?,
            },
        ))
    }
}
