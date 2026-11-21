// non-lean-yet

use super::super::{
    Action, ActionOutcome, ControlFlow, DateTime, DispatchOutcome, Engine, EngineError, Utc,
    VerifiedActionAuthorship, evaluate_transfer_application_formula, invalid_dispatch,
    transfer_request_id_conflict, validate_transfer_application_formula,
};

impl Engine {
    pub(in super::super) async fn dispatch_transfers_applications(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
        verified_authorship: Option<VerifiedActionAuthorship>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
            Action::CompensateTransferApplication {
                application,
                request_id,
                person,
            } => {
                let acting = self
                    .transfer_action_person(actor.as_deref(), Some(&person), Some(&person))
                    .await?;
                self.require_permission(actor.as_deref(), "record:update")
                    .await?;
                let signer = self
                    .transfer_person_signer(&acting, verified_authorship.as_ref())
                    .await?
                    .ok_or_else(|| {
                        EngineError::Forbidden(
                            "private corrections require the owner's signing key".into(),
                        )
                    })?;
                let (fact, replayed) = store::transfer_accounting::compensate(
                    &self.store.pool,
                    &application,
                    &acting,
                    &request_id,
                    now,
                    |hash| Some(signer.sign_hash(hash)),
                )
                .await?;
                outcome.created = Some(fact.uid.clone());
                if !replayed {
                    let effects = store::transfer_effects::correction_facts(
                        &self.store.pool,
                        fact.cause.uid.as_deref().unwrap_or_default(),
                    )
                    .await?;
                    outcome.facts = self.publish_committed_fact(fact);
                    for effect in effects {
                        outcome.facts.extend(self.publish_committed_fact(effect));
                    }
                }
            }
            Action::SetTransferPrivateApplicationPolicy {
                transfer,
                exchange,
                mut effects,
                expected_version,
                request_id,
                person,
            } => {
                let acting = self
                    .transfer_action_person(actor.as_deref(), Some(&person), Some(&person))
                    .await?;
                let transfer = match self
                    .readable_remote_transfer(&transfer, Some(&acting))
                    .await?
                {
                    Some(uid) => uid,
                    None => self.resolve(&transfer).await?,
                };
                let rows = protein::execute_for_with_signer(
                    &self.store,
                    &protein::Protein {
                        source: protein::Source::Transfer,
                        filter: vec![protein::Predicate::UidEq(transfer.clone())],
                        fields: None,
                        include: Default::default(),
                        aggregate: None,
                        order: Vec::new(),
                        limit: None,
                    },
                    actor.as_deref(),
                    Some(&acting),
                )
                .await?;
                let owns_route = rows.iter().any(|row| {
                    row["promises"].as_array().is_some_and(|promises| {
                        promises.iter().any(|promise| {
                            promise["exchange"] == exchange
                                && (promise["giver"] == acting || promise["receiver"] == acting)
                        })
                    })
                });
                if !owns_route {
                    return Err(EngineError::Forbidden(
                        "private policies require an available exchange involving this Person"
                            .into(),
                    ));
                }
                for effect in &mut effects {
                    effect.record = self.resolve(&effect.record).await?;
                    self.reject_direct_transfer_record_mutation(&effect.record)
                        .await?;
                }
                let signer = self
                    .transfer_person_signer(&acting, verified_authorship.as_ref())
                    .await?
                    .ok_or_else(|| {
                        EngineError::Forbidden(
                            "private policies require the owner's signing key".into(),
                        )
                    })?;
                let policy = store::transfer_accounting::set_policy(
                    &self.store.pool,
                    store::transfer_accounting::PolicyInput {
                        transfer,
                        exchange,
                        person: acting,
                        effects,
                        expected_version,
                        request_id,
                    },
                    now,
                    &signer.key_id,
                    &signer.public_key_b64(),
                    |hash| Some(signer.sign_hash(hash)),
                )
                .await?;
                outcome.created = Some(policy.uid);
            }
            Action::SetTransferOccurrenceApplicationFormula {
                occurrence,
                request_id,
                person,
                formula,
            } => {
                let acting = self
                    .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                    .await?;
                self.require_permission(actor.as_deref(), "transfer:update")
                    .await?;
                let formula = validate_transfer_application_formula(&formula)?;
                if let Some(replayed) =
                    store::transfers::occurrence_application_formula_for_request(
                        &self.store.pool,
                        &request_id,
                    )
                    .await?
                {
                    let formula_hash =
                        nucleus::transfer::occurrence_application_formula_hash(&formula);
                    if replayed.event.occurrence_uid != occurrence
                        || replayed.event.receiver_person_uid != acting
                        || replayed.event.formula_hash != formula_hash
                    {
                        return Err(transfer_request_id_conflict());
                    }
                    outcome.created = Some(replayed.event.uid);
                    return Ok(ControlFlow::Break(outcome));
                }
                if store::transfers::phase4_request_for_request(&self.store.pool, &request_id)
                    .await?
                    .is_some()
                {
                    return Err(transfer_request_id_conflict());
                }
                let occurrence_row = store::transfers::occurrence(&self.store.pool, &occurrence)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(occurrence.clone()))?;
                if occurrence_row.receiver_person_uid != acting {
                    return Err(EngineError::Forbidden(
                        "only the occurrence receiver may set its private application formula"
                            .into(),
                    ));
                }
                evaluate_transfer_application_formula(&formula, occurrence_row.quantity)?;
                let signer = self
                    .transfer_person_signer(&acting, verified_authorship.as_ref())
                    .await?;
                let commit = store::transfers::set_occurrence_application_formula(
                    &self.store.pool,
                    store::transfers::OccurrenceApplicationFormulaInput {
                        occurrence_uid: occurrence,
                        idempotency_key: request_id,
                        actor_person_uid: acting,
                        formula,
                        authorization_intent_uid: verified_authorship
                            .as_ref()
                            .map(|value| value.intent_uid.clone()),
                    },
                    now,
                    |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                )
                .await?;
                match commit {
                    store::transfers::OccurrenceApplicationFormulaCommit::Committed(committed) => {
                        outcome.created = Some(committed.event.uid);
                        outcome.facts = self.publish_committed_fact(committed.fact);
                    }
                    store::transfers::OccurrenceApplicationFormulaCommit::Replayed(replayed) => {
                        outcome.created = Some(replayed.event.uid);
                    }
                }
            }
            Action::ApplyTransferApplication {
                expected_effects_hash,
                transfer,
                handoff,
                local_record,
                expected_formula_hash,
                expected_formula_version,
                expected_local_delta,
                expected_local_cumulative_before,
                request_id,
                person,
            } => {
                let acting = self
                    .transfer_action_person(actor.as_deref(), Some(&person), Some(&person))
                    .await?;
                let remote = store::transfer_delivery::application_effect_handoff(
                    &self.store.pool,
                    &handoff,
                )
                .await?
                .ok_or_else(|| EngineError::UnknownRecord(handoff.clone()))?;
                if remote.reference_uid.is_empty() {
                    self.require_transfer_origin_authority(&remote.transfer_uid)
                        .await?;
                }
                let previous = store::transfer_delivery::local_application_for_request(
                    &self.store.pool,
                    &request_id,
                )
                .await?;
                if remote.transfer_uid != transfer
                    || remote.participant_person_uid != acting
                    || (remote.state != "pending" && previous.is_none())
                {
                    return Err(EngineError::Conflict {
                        code: "transfer_remote_application_not_pending",
                        message: "application handoff is not pending for this Person".into(),
                    });
                }
                let local_record = self.resolve(&local_record).await?;
                self.reject_direct_transfer_record_mutation(&local_record)
                    .await?;
                self.require_permission(actor.as_deref(), "record:update")
                    .await?;
                let exchange = self
                    .application_handoff_exchange(&remote, previous.is_some())
                    .await?;
                let (
                    formula,
                    formula_version,
                    formula_hash,
                    local_before,
                    local_after,
                    local_delta,
                ) = if let Some(previous) = &previous {
                    if store::transfer_effects::reviewed_hash(
                        &self.store.pool,
                        &previous.application_fact_uid,
                    )
                    .await?
                        != expected_effects_hash
                    {
                        return Err(transfer_request_id_conflict());
                    }
                    (
                        previous.application_formula.clone(),
                        previous.application_formula_version,
                        previous.application_formula_hash.clone(),
                        previous.local_cumulative_before,
                        previous.local_cumulative_after,
                        previous.local_delta,
                    )
                } else {
                    let applied = store::transfer_accounting::applied(
                        &self.store.pool,
                        &remote.occurrence_uid,
                        &acting,
                    )
                    .await?;
                    if !applied
                        .canonical
                        .exact_numeric_cmp(nucleus::transfer::application::amount(
                            remote.canonical_cumulative_before,
                        )?)
                        .is_eq()
                    {
                        return Err(EngineError::Conflict {
                            code: "transfer_remote_application_out_of_order",
                            message: "apply the earlier slice before this receipt".into(),
                        });
                    }
                    let application = store::transfer_accounting::effective(
                        &self.store.pool,
                        &store::transfer_accounting::Binding {
                            transfer: &transfer,
                            exchange: &exchange,
                            occurrence: Some(&remote.occurrence_uid),
                            person: &acting,
                            record: Some(&local_record),
                            unit: remote.canonical_unit_uid.as_deref(),
                            outgoing: remote.application_direction < 0,
                        },
                    )
                    .await?;
                    let canonical_after = store::exact::sum_exact([
                        applied.canonical,
                        nucleus::transfer::application::amount(remote.canonical_quantity)?,
                    ])?;
                    let group = store::transfer_effects::quote(
                        &self.store.pool,
                        &store::transfer_accounting::Binding {
                            transfer: &transfer,
                            exchange: &exchange,
                            occurrence: Some(&remote.occurrence_uid),
                            person: &acting,
                            record: Some(&local_record),
                            unit: remote.canonical_unit_uid.as_deref(),
                            outgoing: remote.application_direction < 0,
                        },
                        canonical_after,
                    )
                    .await?;
                    store::transfer_effects::require_review(
                        group.as_ref(),
                        expected_effects_hash.as_deref(),
                    )?;
                    let (delta, after) = if let Some(group) = &group {
                        (group.effects[0].delta, group.effects[0].cumulative_after)
                    } else {
                        store::transfer_accounting::calculate(
                            &application.formula,
                            canonical_after,
                            applied.local,
                        )?
                    };
                    (
                        application.formula,
                        application.version,
                        application.formula_hash,
                        applied.local.to_f64(),
                        after.to_f64(),
                        delta.to_f64(),
                    )
                };
                if formula_hash != expected_formula_hash
                    || formula_version != expected_formula_version
                {
                    return Err(EngineError::Conflict {
                        code: "transfer_remote_application_formula_stale",
                        message: "private application policy or units changed after review".into(),
                    });
                }
                if local_delta != expected_local_delta
                    || local_before != expected_local_cumulative_before
                {
                    return Err(EngineError::Conflict {
                        code: "transfer_remote_application_preview_stale",
                        message: "private changes changed after review".into(),
                    });
                }
                let signer = self
                    .transfer_person_signer(&acting, verified_authorship.as_ref())
                    .await?
                    .ok_or_else(|| EngineError::Conflict {
                        code: "missing_person_signer",
                        message:
                            "application attestation requires the participant Person signing key"
                                .into(),
                    })?;
                let commit = store::transfer_delivery::apply_transfer_locally(
                    &self.store.pool,
                    store::transfer_delivery::NewLocalTransferApplication {
                        expected_effects_hash,
                        exchange_uid: exchange,
                        handoff_uid: handoff.clone(),
                        participant_person_uid: acting.clone(),
                        local_record_uid: local_record,
                        local_delta,
                        local_cumulative_before: local_before,
                        local_cumulative_after: local_after,
                        application_formula: formula,
                        application_formula_hash: formula_hash.clone(),
                        application_formula_version: formula_version,
                        authorization_intent_uid: verified_authorship
                            .as_ref()
                            .map(|value| value.intent_uid.clone()),
                        request_id: request_id.clone(),
                    },
                    now,
                    |hash| Some(signer.sign_hash(hash)),
                )
                .await
                .map_err(crate::transfer_counterparty::application_error)?;
                if !remote.reference_uid.is_empty() {
                    let mut attestation =
                        nucleus::transfer_delivery::TransferApplicationAttestationV1 {
                            version: nucleus::transfer_delivery::TRANSFER_ENVELOPE_VERSION,
                            attestation_uid: format!("taa:{}", commit.application.uid),
                            origin_organ_uid: remote.origin_organ_uid.clone(),
                            participant_organ_uid: remote.participant_organ_uid.clone(),
                            participant_person_uid: acting,
                            transfer_uid: remote.transfer_uid,
                            occurrence_uid: remote.occurrence_uid,
                            settlement_slice_uid: remote.settlement_slice_uid,
                            origin_revision: remote.origin_revision,
                            canonical_slice_hash: remote.canonical_slice_hash,
                            formula_commitment: formula_hash,
                            formula_version: formula_version.to_string(),
                            application_fact_uid: commit.fact.uid.clone(),
                            applied_at: commit.application.created_at.clone(),
                            key_id: signer.key_id.clone(),
                            signature: String::new(),
                        };
                    attestation.signature = signer.sign_bytes(&attestation.signing_bytes());
                    attestation
                        .validate_shape()
                        .map_err(EngineError::Consequence)?;
                    store::transfer_delivery::enqueue_application_attestation(
                        &self.store.pool,
                        &handoff,
                        &remote.reference_uid,
                        &remote.origin_organ_uid,
                        &attestation,
                        now,
                    )
                    .await?;
                }
                if remote.reference_uid.is_empty() {
                    self.prepare_counterparty_applications(&transfer, now)
                        .await?;
                }
                outcome.created = Some(commit.application.uid);
                if !commit.replayed {
                    let effects =
                        store::transfer_effects::facts(&self.store.pool, &commit.fact.uid).await?;
                    outcome.facts = self.publish_committed_fact(commit.fact);
                    for fact in effects {
                        outcome.facts.extend(self.publish_committed_fact(fact));
                    }
                }
            }
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
