// non-lean-yet

use super::super::{
    Action, ActionOutcome, ControlFlow, DateTime, DispatchOutcome, Engine, EngineError,
    PromiseState, Utc, VerifiedActionAuthorship, invalid_dispatch, transfer_phase_locked,
    transfer_request_id_conflict, transfer_settlement_conflict, transfer_settlement_values_match,
};

impl Engine {
    pub(in super::super) async fn dispatch_transfers_settlement(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
        verified_authorship: Option<VerifiedActionAuthorship>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
            Action::SettleTransfer {
                transfer,
                actor: person,
            } => {
                if transfer_phase_locked() {
                    return Err(EngineError::Conflict {
                        code: "transfer_phase_5_not_available",
                        message:
                            "transfer settlement is not available before occurrence confirmation"
                                .into(),
                    });
                }
                let transfer = self.resolve(&transfer).await?;
                let person = self.resolve(&person).await?;
                self.require_transfer_person(&transfer, &person, actor.as_deref())
                    .await?;
                outcome.facts = self.settle_all_local(&transfer, &person, now).await?;
            }
            Action::SettleTransferOccurrence {
                expected_effects_hash,
                occurrence,
                request_id,
                person,
                canonical_quantity,
                expected_remaining_quantity,
                expected_local_delta,
                expected_application_formula_hash,
                expected_application_formula_version,
                expected_remainder_policy,
            } => {
                let acting = self
                    .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                    .await?;
                self.require_permission(actor.as_deref(), "transfer:update")
                    .await?;
                if !canonical_quantity.is_finite() || canonical_quantity <= 0.0 {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_quantity_invalid",
                        "settlement quantity must be finite and positive",
                    ));
                }
                if !expected_remaining_quantity.is_finite() || !expected_local_delta.is_finite() {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_preview_invalid",
                        "settlement preview quantities must be finite",
                    ));
                }

                if let Some(replayed) = store::transfers::occurrence_settlement_for_request(
                    &self.store.pool,
                    &request_id,
                )
                .await?
                {
                    let slice = &replayed.slice;
                    let replay_remaining_before = store::exact::sum_exact([
                        store::transfer_accounting::amount(slice.remaining_after)?,
                        store::transfer_accounting::amount(slice.canonical_quantity)?,
                    ])?
                    .to_f64();
                    if slice.occurrence_uid != occurrence
                        || slice.owner_person_uid != acting
                        || !transfer_settlement_values_match(
                            slice.canonical_quantity,
                            canonical_quantity,
                        )
                        || !transfer_settlement_values_match(
                            replay_remaining_before,
                            expected_remaining_quantity,
                        )
                        || !transfer_settlement_values_match(
                            slice.local_delta,
                            expected_local_delta,
                        )
                        || slice.application_formula_hash != expected_application_formula_hash
                        || slice.application_formula_version != expected_application_formula_version
                        || slice.remainder_policy != expected_remainder_policy
                        || store::transfer_effects::reviewed_hash(
                            &self.store.pool,
                            &slice.application_fact_uid,
                        )
                        .await?
                            != expected_effects_hash
                    {
                        return Err(transfer_request_id_conflict());
                    }
                    self.prepare_counterparty_applications(&slice.transfer_uid, now)
                        .await?;
                    outcome.created = Some(slice.uid.clone());
                    return Ok(ControlFlow::Break(outcome));
                }
                if store::transfers::phase5_request_for_request(&self.store.pool, &request_id)
                    .await?
                    .is_some()
                    || store::transfers::phase4_request_for_request(&self.store.pool, &request_id)
                        .await?
                        .is_some()
                {
                    return Err(transfer_request_id_conflict());
                }

                let occurrence_row = store::transfers::occurrence(&self.store.pool, &occurrence)
                    .await?
                    .ok_or_else(|| {
                        transfer_settlement_conflict(
                            "transfer_settlement_occurrence_missing",
                            "the settlement occurrence no longer exists",
                        )
                    })?;
                let source =
                    store::misc::get_promise(&self.store.pool, &occurrence_row.promise_uid)
                        .await?
                        .ok_or_else(|| {
                            transfer_settlement_conflict(
                                "transfer_settlement_source_missing",
                                "the occurrence source promise no longer exists",
                            )
                        })?;
                let Some(local_record_uid) = source.record_uid.as_deref() else {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_concrete_record_required",
                        "a concept-only promise must be refined to a concrete Record before settlement",
                    ));
                };
                if occurrence_row.record_uid.as_deref() != Some(local_record_uid) {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_source_mismatch",
                        "the occurrence no longer matches its signed source promise",
                    ));
                }
                let policy_record = store::transfer_accounting::bound_record(
                    &self.store.pool,
                    &occurrence_row.transfer_uid,
                    occurrence_row
                        .exchange_uid
                        .as_deref()
                        .unwrap_or(&occurrence_row.promise_uid),
                    &acting,
                    Some(local_record_uid),
                )
                .await?;
                let local_record_uid = policy_record
                    .as_deref()
                    .ok_or_else(|| EngineError::UnknownRecord(occurrence.clone()))?;
                self.reject_direct_transfer_record_mutation(local_record_uid)
                    .await?;
                self.require_permission(actor.as_deref(), "record:update")
                    .await?;
                let local_record = store::records::get(&self.store.pool, local_record_uid)
                    .await?
                    .ok_or_else(|| {
                        transfer_settlement_conflict(
                            "transfer_settlement_local_record_missing",
                            "the source promise's local Record is unavailable",
                        )
                    })?;
                let local_organ_uid = store::organs::local(&self.store.pool)
                    .await?
                    .map(|organ| organ.uid);
                if local_record
                    .organ_uid
                    .as_deref()
                    .is_some_and(|origin| Some(origin) != local_organ_uid.as_deref())
                {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_foreign_record",
                        "settlement cannot alter a Record originating in another Cell",
                    ));
                }
                if source.party_uid.as_deref() != Some(acting.as_str()) {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_not_source_owner",
                        "only the concrete source-promise owner may apply this occurrence",
                    ));
                }
                if source.delta == 0.0 || !source.delta.is_finite() {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_direction_invalid",
                        "the source promise must have a finite non-zero direction",
                    ));
                }
                if source.state != PromiseState::Active {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_not_active",
                        "only an active occurrence may be settled",
                    ));
                }
                if !occurrence_row.delivery_claimed || !occurrence_row.receipt_claimed {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_confirmation_required",
                        "both delivery and receipt must currently be confirmed",
                    ));
                }
                if occurrence_row.disputed {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_disputed",
                        "a disputed occurrence cannot be settled",
                    ));
                }

                let progress =
                    store::transfers::occurrence_settlement_progress(&self.store.pool, &occurrence)
                        .await?
                        .ok_or_else(|| {
                            transfer_settlement_conflict(
                                "transfer_settlement_occurrence_missing",
                                "the settlement occurrence no longer exists",
                            )
                        })?;
                if !transfer_settlement_values_match(
                    progress.remaining_quantity,
                    expected_remaining_quantity,
                ) {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_remaining_stale",
                        format!(
                            "reviewed remaining quantity was {expected_remaining_quantity}, current remaining quantity is {}",
                            progress.remaining_quantity
                        ),
                    ));
                }
                if canonical_quantity > progress.remaining_quantity {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_quantity_exceeds_remaining",
                        "settlement quantity exceeds the occurrence's remaining quantity",
                    ));
                }

                let application = store::transfer_accounting::effective(
                    &self.store.pool,
                    &store::transfer_accounting::Binding {
                        transfer: &occurrence_row.transfer_uid,
                        exchange: occurrence_row
                            .exchange_uid
                            .as_deref()
                            .unwrap_or(&occurrence_row.promise_uid),
                        occurrence: Some(&occurrence),
                        person: &acting,
                        record: Some(local_record_uid),
                        unit: occurrence_row.unit_uid.as_deref(),
                        outgoing: source.delta < 0.0,
                    },
                )
                .await?;
                let application_formula = application.formula;
                let application_formula_version = application.version;
                let application_formula_hash = application.formula_hash;
                if application_formula_hash != expected_application_formula_hash
                    || application_formula_version != expected_application_formula_version
                {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_formula_stale",
                        "the private application formula changed after settlement review",
                    ));
                }
                let remainder_policy = store::transfers::effective_occurrence_remainder_policy(
                    &self.store.pool,
                    &occurrence,
                    &acting,
                )
                .await?;
                if remainder_policy != expected_remainder_policy {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_remainder_policy_stale",
                        "the remainder policy changed after settlement review",
                    ));
                }

                let applied =
                    store::transfer_accounting::applied(&self.store.pool, &occurrence, &acting)
                        .await?;
                let canonical_after = store::exact::sum_exact([
                    applied.canonical,
                    nucleus::transfer::application::amount(canonical_quantity)?,
                ])?;
                let group = store::transfer_effects::quote(
                    &self.store.pool,
                    &store::transfer_accounting::Binding {
                        transfer: &occurrence_row.transfer_uid,
                        exchange: occurrence_row
                            .exchange_uid
                            .as_deref()
                            .unwrap_or(&occurrence_row.promise_uid),
                        occurrence: Some(&occurrence),
                        person: &acting,
                        record: Some(local_record_uid),
                        unit: occurrence_row.unit_uid.as_deref(),
                        outgoing: source.delta < 0.0,
                    },
                    canonical_after,
                )
                .await?;
                store::transfer_effects::require_review(
                    group.as_ref(),
                    expected_effects_hash.as_deref(),
                )?;
                let (exact_delta, exact_after) = if let Some(group) = &group {
                    (group.effects[0].delta, group.effects[0].cumulative_after)
                } else {
                    store::transfer_accounting::calculate(
                        &application_formula,
                        canonical_after,
                        applied.local,
                    )?
                };
                let local_delta = exact_delta.to_f64();
                let local_cumulative_after = exact_after.to_f64();
                if !local_delta.is_finite()
                    || !transfer_settlement_values_match(local_delta, expected_local_delta)
                {
                    return Err(transfer_settlement_conflict(
                        "transfer_settlement_preview_stale",
                        format!(
                            "reviewed local delta was {expected_local_delta}, current local delta is {local_delta}"
                        ),
                    ));
                }

                let signer = self
                    .transfer_person_signer(&acting, verified_authorship.as_ref())
                    .await?;
                let commit = store::transfers::settle_occurrence(
                    &self.store.pool,
                    store::transfers::OccurrenceSettlementInput {
                        expected_effects_hash,
                        occurrence_uid: occurrence,
                        idempotency_key: request_id,
                        actor_person_uid: acting,
                        canonical_quantity,
                        local_record_uid: local_record_uid.to_string(),
                        local_delta,
                        local_cumulative_after,
                        application_formula_hash,
                        application_formula,
                        application_formula_version,
                        remainder_policy,
                        authorization_intent_uid: verified_authorship
                            .as_ref()
                            .map(|value| value.intent_uid.clone()),
                    },
                    now,
                    |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                )
                .await?;
                match commit {
                    store::transfers::OccurrenceSettlementCommit::Committed(committed) => {
                        let effects = store::transfer_effects::facts(
                            &self.store.pool,
                            &committed.application_fact.uid,
                        )
                        .await?;
                        outcome.created = Some(committed.slice.uid);
                        outcome.facts = self.publish_committed_fact(committed.evidence_fact);
                        outcome
                            .facts
                            .extend(self.publish_committed_fact(committed.application_fact));
                        for fact in committed.satiation_facts {
                            outcome.facts.extend(self.publish_committed_fact(fact));
                        }
                        for fact in effects {
                            outcome.facts.extend(self.publish_committed_fact(fact));
                        }
                    }
                    store::transfers::OccurrenceSettlementCommit::Replayed(replayed) => {
                        outcome.created = Some(replayed.slice.uid);
                    }
                }
                self.prepare_counterparty_applications(&occurrence_row.transfer_uid, now)
                    .await?;
            }
            Action::BeginTransferSettlement {
                transfer,
                occurrence,
                expected_revision,
                expected_remaining_quantity,
                canonical_quantity,
                request_id,
                person,
            } => {
                let transfer = self.resolve(&transfer).await?;
                let acting = self
                    .transfer_action_person(actor.as_deref(), Some(&person), Some(&person))
                    .await?;
                let handoff = self
                    .begin_transfer_settlement(
                        &transfer,
                        &occurrence,
                        &acting,
                        expected_revision,
                        expected_remaining_quantity,
                        canonical_quantity,
                        &request_id,
                        now,
                    )
                    .await?;
                outcome.created = Some(handoff.uid);
            }
            Action::CompensateTransferOccurrenceSettlement {
                settlement,
                request_id,
                person,
            } => {
                let acting = self
                    .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                    .await?;
                self.require_permission(actor.as_deref(), "record:update")
                    .await?;
                self.require_permission(actor.as_deref(), "transfer:update")
                    .await?;
                if let Some(replayed) =
                    store::transfers::occurrence_settlement_compensation_for_request(
                        &self.store.pool,
                        &request_id,
                    )
                    .await?
                {
                    if replayed.correction.settlement_uid != settlement
                        || replayed.correction.owner_person_uid != acting
                    {
                        return Err(transfer_request_id_conflict());
                    }
                    outcome.created = Some(replayed.correction.uid);
                    return Ok(ControlFlow::Break(outcome));
                }
                if store::transfers::phase5_correction_request_for_request(
                    &self.store.pool,
                    &request_id,
                )
                .await?
                .is_some()
                {
                    return Err(transfer_request_id_conflict());
                }
                let slice =
                    store::transfers::occurrence_settlement_slice(&self.store.pool, &settlement)
                        .await?
                        .ok_or_else(|| EngineError::Conflict {
                            code: "transfer_settlement_missing",
                            message: "the settlement slice does not exist".into(),
                        })?;
                if slice.owner_person_uid != acting {
                    return Err(EngineError::Conflict {
                        code: "transfer_settlement_compensation_not_owner",
                        message:
                            "only the settlement owner may reverse its private Record application"
                                .into(),
                    });
                }
                if store::transfers::occurrence_settlement_compensation_for_settlement(
                    &self.store.pool,
                    &settlement,
                )
                .await?
                .is_some()
                {
                    return Err(EngineError::Conflict {
                        code: "transfer_settlement_already_compensated",
                        message:
                            "this settlement's private Record application was already compensated"
                                .into(),
                    });
                }
                let signer = self
                    .transfer_person_signer(&acting, verified_authorship.as_ref())
                    .await?;
                let commit = store::transfers::compensate_occurrence_settlement(
                    &self.store.pool,
                    store::transfers::OccurrenceSettlementCompensationInput {
                        settlement_uid: settlement,
                        idempotency_key: request_id,
                        actor_person_uid: acting,
                        authorization_intent_uid: verified_authorship
                            .as_ref()
                            .map(|value| value.intent_uid.clone()),
                    },
                    now,
                    |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                )
                .await?;
                match commit {
                    store::transfers::OccurrenceSettlementCompensationCommit::Committed(
                        committed,
                    ) => {
                        outcome.created = Some(committed.correction.uid);
                        let effects = store::transfer_effects::correction_facts(
                            &self.store.pool,
                            &committed.correction.original_application_fact_uid,
                        )
                        .await?;
                        outcome.facts = self.publish_committed_fact(committed.fact);
                        for effect in effects {
                            outcome.facts.extend(self.publish_committed_fact(effect));
                        }
                    }
                    store::transfers::OccurrenceSettlementCompensationCommit::Replayed(
                        replayed,
                    ) => {
                        outcome.created = Some(replayed.correction.uid);
                    }
                }
            }
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
