use super::*;

impl Engine {
    pub(super) fn dispatch_part_6(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
        verified_authorship: Option<VerifiedActionAuthorship>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = DispatchOutcome> + Send + '_>> {
        Box::pin(async move {
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
                    let occurrence_row =
                        store::transfers::occurrence(&self.store.pool, &occurrence)
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
                        store::transfers::OccurrenceApplicationFormulaCommit::Committed(
                            committed,
                        ) => {
                            outcome.created = Some(committed.event.uid);
                            outcome.facts = self.publish_committed_fact(committed.fact);
                        }
                        store::transfers::OccurrenceApplicationFormulaCommit::Replayed(
                            replayed,
                        ) => {
                            outcome.created = Some(replayed.event.uid);
                        }
                    }
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
                    if !expected_remaining_quantity.is_finite() || !expected_local_delta.is_finite()
                    {
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
                            || slice.application_formula_version
                                != expected_application_formula_version
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
                        || store::transfers::phase4_request_for_request(
                            &self.store.pool,
                            &request_id,
                        )
                        .await?
                        .is_some()
                    {
                        return Err(transfer_request_id_conflict());
                    }

                    let occurrence_row =
                        store::transfers::occurrence(&self.store.pool, &occurrence)
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

                    let progress = store::transfers::occurrence_settlement_progress(
                        &self.store.pool,
                        &occurrence,
                    )
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
                Action::ConfigureTransferDelivery {
                    transfer,
                    recipient_person,
                    recipient_organ,
                    person,
                    request_id,
                    mode,
                } => {
                    let transfer = self.resolve(&transfer).await?;
                    self.require_transfer_editor(&transfer, actor.as_deref())
                        .await?;
                    let recipient_person = self.resolve(&recipient_person).await?;
                    let recipient_organ = self.resolve(&recipient_organ).await?;
                    let derived = self.transfer_creator_person(&transfer).await?;
                    let acting = self
                        .transfer_action_person(actor.as_deref(), person.as_deref(), Some(&derived))
                        .await?;
                    let (fact_uid, facts) = self
                        .append_transfer_delivery_evidence(
                            &transfer,
                            &acting,
                            &request_id,
                            "configure",
                            serde_json::json!({
                                "action": "configure-transfer-delivery",
                                "request_id": request_id,
                                "recipient_person": recipient_person,
                                "recipient_organ": recipient_organ,
                                "mode": mode.as_str(),
                            }),
                            now,
                            verified_authorship.as_ref(),
                        )
                        .await?;
                    let policy = self
                        .create_transfer_delivery_policy(
                            &transfer,
                            &recipient_person,
                            &recipient_organ,
                            &acting,
                            &fact_uid,
                            &request_id,
                            mode,
                            now,
                        )
                        .await?;
                    outcome.created = Some(policy.uid);
                    outcome.facts = facts;
                }
                Action::SetTransferDeliveryMode {
                    transfer,
                    delivery,
                    expected_revision,
                    person,
                    request_id,
                    mode,
                } => {
                    let transfer = self.resolve(&transfer).await?;
                    self.require_transfer_editor(&transfer, actor.as_deref())
                        .await?;
                    let derived = self.transfer_creator_person(&transfer).await?;
                    let acting = self
                        .transfer_action_person(actor.as_deref(), person.as_deref(), Some(&derived))
                        .await?;
                    let (fact_uid, facts) = self
                        .append_transfer_delivery_evidence(
                            &transfer,
                            &acting,
                            &request_id,
                            "set-mode",
                            serde_json::json!({
                                "action": "set-transfer-delivery-mode",
                                "request_id": request_id,
                                "delivery": delivery,
                                "expected_revision": expected_revision,
                                "mode": mode.as_str(),
                            }),
                            now,
                            verified_authorship.as_ref(),
                        )
                        .await?;
                    self.change_transfer_delivery_mode(
                        &transfer,
                        &delivery,
                        expected_revision,
                        &acting,
                        &fact_uid,
                        &request_id,
                        mode,
                        now,
                    )
                    .await?;
                    outcome.created = Some(delivery);
                    outcome.facts = facts;
                }
                Action::RevokeTransferDelivery {
                    transfer,
                    delivery,
                    expected_revision,
                    person,
                    request_id,
                } => {
                    let transfer = self.resolve(&transfer).await?;
                    self.require_transfer_editor(&transfer, actor.as_deref())
                        .await?;
                    let derived = self.transfer_creator_person(&transfer).await?;
                    let acting = self
                        .transfer_action_person(actor.as_deref(), person.as_deref(), Some(&derived))
                        .await?;
                    let (fact_uid, facts) = self
                        .append_transfer_delivery_evidence(
                            &transfer,
                            &acting,
                            &request_id,
                            "revoke",
                            serde_json::json!({
                                "action": "revoke-transfer-delivery",
                                "request_id": request_id,
                                "delivery": delivery,
                                "expected_revision": expected_revision,
                                "retains_received_evidence": true,
                            }),
                            now,
                            verified_authorship.as_ref(),
                        )
                        .await?;
                    self.revoke_transfer_delivery_policy(
                        &transfer,
                        &delivery,
                        expected_revision,
                        &acting,
                        &fact_uid,
                        &request_id,
                        now,
                    )
                    .await?;
                    outcome.created = Some(delivery);
                    outcome.facts = facts;
                }
                Action::EnqueueTransferDelivery {
                    transfer,
                    delivery,
                    person,
                    request_id,
                } => {
                    let operation = "enqueue";
                    let transfer = self.resolve(&transfer).await?;
                    self.require_transfer_editor(&transfer, actor.as_deref())
                        .await?;
                    let derived = self.transfer_creator_person(&transfer).await?;
                    let acting = self
                        .transfer_action_person(actor.as_deref(), person.as_deref(), Some(&derived))
                        .await?;
                    let (_, facts) = self
                        .append_transfer_delivery_evidence(
                            &transfer,
                            &acting,
                            &request_id,
                            operation,
                            serde_json::json!({
                                "action": format!("{operation}-transfer-delivery"),
                                "request_id": request_id,
                                "delivery": delivery,
                            }),
                            now,
                            verified_authorship.as_ref(),
                        )
                        .await?;
                    outcome.created = Some(
                        self.enqueue_transfer_delivery(&transfer, &delivery, &request_id, now)
                            .await?,
                    );
                    outcome.facts = facts;
                }
                Action::RetryTransferDelivery {
                    transfer,
                    delivery,
                    person,
                    request_id,
                } => {
                    let transfer = self.resolve(&transfer).await?;
                    self.require_transfer_editor(&transfer, actor.as_deref())
                        .await?;
                    let derived = self.transfer_creator_person(&transfer).await?;
                    let acting = self
                        .transfer_action_person(actor.as_deref(), person.as_deref(), Some(&derived))
                        .await?;
                    let (_, facts) = self
                        .append_transfer_delivery_evidence(
                            &transfer,
                            &acting,
                            &request_id,
                            "retry",
                            serde_json::json!({
                                "action": "retry-transfer-delivery",
                                "request_id": request_id,
                                "delivery": delivery,
                            }),
                            now,
                            verified_authorship.as_ref(),
                        )
                        .await?;
                    outcome.created = Some(
                        self.retry_transfer_delivery(&transfer, &delivery, &request_id, now)
                            .await?,
                    );
                    outcome.facts = facts;
                }
                Action::RefreshTransferDelivery {
                    transfer,
                    delivery,
                    person,
                    request_id,
                } => {
                    let local = store::organs::local(&self.store.pool)
                        .await?
                        .ok_or_else(|| EngineError::Consequence("no local Organ".into()))?;
                    let reference = store::sqlx::query_as::<_, (String, String, String, String)>(
                    "SELECT transfer_uid, recipient_organ_uid, recipient_person_uid, state FROM transfer_remote_reference WHERE uid = ?",
                )
                .bind(&delivery)
                .fetch_optional(&self.store.pool)
                .await?
                .ok_or_else(|| EngineError::UnknownRecord(delivery.clone()))?;
                    let acting = self
                        .transfer_action_person(
                            actor.as_deref(),
                            person.as_deref(),
                            Some(&reference.2),
                        )
                        .await?;
                    if reference.0 != transfer
                        || reference.1 != local.uid
                        || reference.2 != acting
                        || reference.3 != "active"
                    {
                        return Err(EngineError::Conflict {
                            code: "transfer_delivery_refresh_forbidden",
                            message:
                                "only an active local hosted/replica reference may be refreshed"
                                    .into(),
                        });
                    }
                    outcome.created = Some(
                        store::transfer_delivery::enqueue_pull(
                            &self.store.pool,
                            &delivery,
                            &request_id,
                            now,
                        )
                        .await?
                        .uid,
                    );
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
                            message: "private application policy or units changed after review"
                                .into(),
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
                        .ok_or_else(|| {
                            EngineError::Conflict {
                        code: "missing_person_signer",
                        message:
                            "application attestation requires the participant Person signing key"
                                .into(),
                    }
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
                            store::transfer_effects::facts(&self.store.pool, &commit.fact.uid)
                                .await?;
                        outcome.facts = self.publish_committed_fact(commit.fact);
                        for fact in effects {
                            outcome.facts.extend(self.publish_committed_fact(fact));
                        }
                    }
                }
                Action::ConfirmTransfer {
                    transfer,
                    confirmation,
                } => {
                    if transfer_phase_locked() {
                        return Err(EngineError::Conflict {
                            code: "transfer_phase_4_not_available",
                            message: "occurrence-specific confirmation is not available yet".into(),
                        });
                    }
                    if !matches!(confirmation.as_str(), "delivery" | "receipt") {
                        return Err(EngineError::Consequence(format!(
                            "confirmation must be `delivery` or `receipt`, not `{confirmation}`"
                        )));
                    }
                    let transfer = self.resolve(&transfer).await?;
                    self.require_transfer_participant(&transfer, actor.as_deref())
                        .await?;
                    outcome.facts = self
                        .append(
                            NewFact {
                                uid: None,
                                record_uid: transfer.clone(),
                                delta: nucleus::fact::zero_delta(),
                                at: None,
                                actor_uid: actor,
                                cause: Cause::settlement(transfer),
                                payload: Some(
                                    serde_json::json!({ "confirmation": confirmation }).to_string(),
                                ),
                            },
                            now,
                        )
                        .await?;
                }
                _ => {
                    return Err(EngineError::Consequence(
                        "Invalid internal action dispatch".into(),
                    ));
                }
            }
            Ok(ControlFlow::Continue(outcome))
        })
    }
}
