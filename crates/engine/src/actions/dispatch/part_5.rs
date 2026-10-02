use super::*;

impl Engine {
    pub(super) fn dispatch_part_5(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
        verified_authorship: Option<VerifiedActionAuthorship>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = DispatchOutcome> + Send + '_>> {
        Box::pin(async move {
            let mut outcome = ActionOutcome::default();
            match action {
                Action::ReopenTransferInvitation {
                    invitation,
                    expected_revision,
                    request_id,
                    expires_at,
                } => {
                    let invitation_row =
                        store::transfers::invitation(&self.store.pool, &invitation)
                            .await?
                            .ok_or_else(|| {
                                EngineError::Consequence("unknown transfer invitation".into())
                            })?;
                    self.require_permission(actor.as_deref(), "transfer:update")
                        .await?;
                    let creator = self
                        .transfer_creator_person(&invitation_row.transfer_uid)
                        .await?;
                    let acting = self
                        .transfer_action_person(actor.as_deref(), None, Some(&creator))
                        .await?;
                    if acting != creator {
                        return Err(EngineError::Forbidden(
                            "only the transfer creator may reopen an invitation".into(),
                        ));
                    }
                    if let Some(created) = transfer_invitation_replay(
                        &self.store.pool,
                        request_id.trim(),
                        &invitation,
                        "reopened",
                    )
                    .await?
                    {
                        outcome.created = Some(created);
                        return Ok(ControlFlow::Break(outcome));
                    }
                    let signer = self
                        .transfer_person_signer(&acting, verified_authorship.as_ref())
                        .await?;
                    let commit = store::transfers::reopen_transfer_invitation(
                        &self.store.pool,
                        store::transfers::InvitationTransitionInput {
                            invitation_uid: invitation,
                            expected_revision,
                            idempotency_key: request_id.trim().to_string(),
                            actor_person_uid: Some(acting),
                            expires_at: normalize_transfer_invitation_expiry(expires_at, now)?,
                        },
                        now,
                        |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                    )
                    .await?;
                    self.apply_transfer_invitation_commit(commit, expected_revision, &mut outcome)?;
                }
                Action::CounterofferTransfer {
                    transfer,
                    expected_revision,
                    request_id,
                    person,
                    draft,
                } => {
                    let transfer = self.resolve(&transfer).await?;
                    let acting = self
                        .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                        .await?;
                    if store::transfers::phase4_request_for_request(&self.store.pool, &request_id)
                        .await?
                        .is_some()
                    {
                        return Err(transfer_request_id_conflict());
                    }
                    self.require_permission(actor.as_deref(), "transfer:update")
                        .await?;
                    let request_id = request_id.trim().to_string();
                    if request_id.is_empty() || request_id.chars().count() > 200 {
                        return Err(EngineError::Consequence(
                            "transfer request_id must contain 1 to 200 characters".into(),
                        ));
                    }
                    if store::transfers::party_for_actor(&self.store.pool, &transfer, &acting)
                        .await?
                        .is_none()
                    {
                        return Err(EngineError::Forbidden(
                            "a counteroffer requires an accepted transfer participant".into(),
                        ));
                    }
                    if let Some((replayed_transfer, _, replayed_action)) =
                        store::transfers::revision_for_request(&self.store.pool, request_id.trim())
                            .await?
                    {
                        if replayed_transfer != transfer
                            || replayed_action != "counteroffer-transfer-draft"
                        {
                            return Err(transfer_request_id_conflict());
                        }
                        outcome.created = Some(transfer);
                        return Ok(ControlFlow::Break(outcome));
                    }
                    if store::transfers::invitation_event_for_request(
                        &self.store.pool,
                        request_id.trim(),
                    )
                    .await?
                    .is_some()
                    {
                        return Err(transfer_request_id_conflict());
                    }
                    let creator = self.transfer_creator_person(&transfer).await?;
                    let submitted_creator = if draft.creator.trim().is_empty() {
                        creator.clone()
                    } else {
                        self.resolve(draft.creator.trim()).await?
                    };
                    if submitted_creator != creator {
                        return Err(EngineError::Conflict {
                            code: "transfer_creator_immutable",
                            message: "transfer creator Person cannot change in a counteroffer"
                                .into(),
                        });
                    }
                    let mut input = self
                        .resolve_whole_transfer_draft(
                            transfer.clone(),
                            expected_revision,
                            request_id,
                            draft,
                            creator,
                            acting.clone(),
                            now,
                            true,
                            actor.as_deref().or_else(|| {
                                verified_authorship
                                    .as_ref()
                                    .map(|authorship| authorship.person_uid.as_str())
                            }),
                        )
                        .await?;
                    input.authorization_intent_uid = verified_authorship
                        .as_ref()
                        .map(|value| value.intent_uid.clone());
                    let signer = self
                        .transfer_person_signer(&acting, verified_authorship.as_ref())
                        .await?;
                    let commit = store::transfers::counteroffer_whole_draft(
                        &self.store.pool,
                        input,
                        now,
                        Some(acting),
                        |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                    )
                    .await?;
                    self.apply_transfer_revision_commit(
                        commit,
                        expected_revision,
                        &transfer,
                        &mut outcome,
                    )
                    .await?;
                }
                Action::ClaimOpenTransferPromise {
                    transfer,
                    promise,
                    expected_revision,
                    request_id,
                    person,
                    terms,
                } => {
                    let transfer = self.resolve(&transfer).await?;
                    self.require_permission(actor.as_deref(), "transfer:update")
                        .await?;
                    let claimant = self
                        .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                        .await?;
                    let replaying = if let Some((replayed_transfer, _, replayed_action)) =
                        store::transfers::revision_for_request(&self.store.pool, request_id.trim())
                            .await?
                    {
                        if replayed_transfer != transfer
                            || replayed_action != "claim-open-transfer-promise"
                        {
                            return Err(transfer_request_id_conflict());
                        }
                        let Some((target_transfer, target_source, target_claimant, _)) =
                            store::transfers::open_claim_target_for_request(
                                &self.store.pool,
                                request_id.trim(),
                            )
                            .await?
                        else {
                            return Err(transfer_request_id_conflict());
                        };
                        if target_transfer != transfer
                            || target_source != promise
                            || target_claimant != claimant
                        {
                            return Err(transfer_request_id_conflict());
                        }
                        true
                    } else {
                        false
                    };
                    if !replaying
                        && store::transfers::invitation_event_for_request(
                            &self.store.pool,
                            request_id.trim(),
                        )
                        .await?
                        .is_some()
                    {
                        return Err(transfer_request_id_conflict());
                    }
                    let transfer_row = store::transfers::get(&self.store.pool, &transfer)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(transfer.clone()))?;
                    let source = store::misc::get_promise(&self.store.pool, &promise)
                        .await?
                        .filter(|source| source.transfer_uid.as_deref() == Some(transfer.as_str()))
                        .ok_or_else(|| {
                            EngineError::Consequence("unknown OPEN transfer promise".into())
                        })?;
                    if !replaying
                        && (source.state != PromiseState::Open || source.party_uid.is_none())
                    {
                        return Err(EngineError::Conflict {
                            code: "transfer_promise_not_open",
                            message: "only a current OPEN promise may be claimed".into(),
                        });
                    }
                    if !replaying && source.party_uid.as_deref() == Some(claimant.as_str()) {
                        return Err(EngineError::Conflict {
                            code: "transfer_open_self_claim_forbidden",
                            message: "an OPEN proposer cannot claim their own proposal".into(),
                        });
                    }
                    let invitations =
                        store::transfers::invitations_for_transfer(&self.store.pool, &transfer)
                            .await?;
                    if !replaying
                        && invitations.iter().any(|invitation| {
                            invitation.addressed_person_uid == claimant
                                && invitation.status
                                    == store::transfers::TransferInvitationStatus::Pending
                        })
                    {
                        return Err(EngineError::Conflict {
                        code: "transfer_invitation_acceptance_required",
                        message: "an addressed Person must accept the invitation before claiming an OPEN promise"
                            .into(),
                    });
                    }
                    let participant =
                        store::transfers::party_for_actor(&self.store.pool, &transfer, &claimant)
                            .await?
                            .is_some();
                    if !replaying && !participant && transfer_row.visibility != "public" {
                        return Err(EngineError::Forbidden(
                            "a non-participant may claim only a public OPEN promise".into(),
                        ));
                    }
                    if terms.withdrawn {
                        return Err(EngineError::Consequence(
                            "an OPEN claim cannot withdraw its refined promise".into(),
                        ));
                    }
                    if terms.open {
                        return Err(EngineError::Consequence(
                            "an OPEN claim must create a concrete claimant promise".into(),
                        ));
                    }
                    if terms.reuse_policy != source.open_reuse_policy {
                        return Err(EngineError::Conflict {
                            code: "transfer_open_reuse_policy_mismatch",
                            message: "the claim must use the OPEN proposal's signed reuse policy"
                                .into(),
                        });
                    }
                    if terms
                        .party
                        .as_deref()
                        .is_some_and(|token| token.trim() != claimant)
                    {
                        let party = self.resolve(terms.party.as_deref().unwrap().trim()).await?;
                        if party != claimant {
                            return Err(EngineError::Consequence(
                                "a claimed promise belongs to the claiming Person".into(),
                            ));
                        }
                    }
                    if !terms.delta.is_finite() || terms.delta == 0.0 {
                        return Err(EngineError::Consequence(
                            "claimed promise delta must be finite and non-zero".into(),
                        ));
                    }
                    if source.delta.signum() == terms.delta.signum() {
                        return Err(EngineError::Conflict {
                        code: "transfer_open_claim_direction_mismatch",
                        message: "claimant quantity must have the opposite direction from the OPEN proposal"
                            .into(),
                    });
                    }
                    let (record_uid, concept_uid) = self
                        .resolve_transfer_item_source(
                            &terms.record,
                            store::transfers::item_of(&self.store.pool, &source.uid)
                                .await?
                                .is_some(),
                            actor.as_deref().or_else(|| {
                                verified_authorship
                                    .as_ref()
                                    .map(|value| value.person_uid.as_str())
                            }),
                        )
                        .await?;
                    let concept_uid = concept_uid.or_else(|| source.concept_uid.clone());
                    let unit_uid = self.resolve_concept_opt(terms.unit).await?;
                    let replay_window_end = terms.window_end.clone();
                    let (window_start, window_end) = normalize_transfer_window(
                        terms.window_start,
                        terms.window_end,
                        now,
                        if replaying {
                            replay_window_end.as_deref()
                        } else {
                            source.window_end.as_deref()
                        },
                    )?;
                    let condition = terms
                        .condition
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty());
                    if let Some(value) = condition.as_deref() {
                        nucleus::expr::Expr::parse(value).map_err(|error| {
                            EngineError::Consequence(format!("invalid promise condition: {error}"))
                        })?;
                    }
                    let signer = if replaying {
                        None
                    } else {
                        self.transfer_person_signer(&claimant, verified_authorship.as_ref())
                            .await?
                    };
                    let commit = store::transfers::claim_open_promise(
                        &self.store.pool,
                        store::transfers::OpenPromiseClaimInput {
                            transfer_uid: transfer,
                            source_promise_uid: promise,
                            expected_revision,
                            idempotency_key: request_id.trim().to_string(),
                            claimant_person_uid: claimant,
                            record_uid,
                            concept_uid,
                            unit_uid,
                            delta: terms.delta,
                            window_start,
                            window_end,
                            location: normalize_transfer_place(terms.place)?,
                            condition,
                            reserve_from: terms
                                .reserve_from
                                .map(|value| value.as_str().to_string())
                                .unwrap_or(source.reserve_from),
                            authorization_intent_uid: verified_authorship
                                .as_ref()
                                .map(|value| value.intent_uid.clone()),
                        },
                        now,
                        |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                    )
                    .await?;
                    self.apply_open_promise_claim_commit(commit, expected_revision, &mut outcome)?;
                }
                Action::SetTransferAgreementLevel {
                    transfer,
                    expected_revision,
                    request_id,
                    person,
                    level,
                } => {
                    if level > 2 {
                        return Err(EngineError::Consequence(
                            "agreement level must be 0, 1, or 2".into(),
                        ));
                    }
                    let transfer = self.resolve(&transfer).await?;
                    self.require_permission(actor.as_deref(), "transfer:update")
                        .await?;
                    let acting = self
                        .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                        .await?;
                    if let Some(event) =
                        store::transfers::agreement_event_for_request(&self.store.pool, &request_id)
                            .await?
                    {
                        if event.transfer_uid != transfer
                            || event.revision != expected_revision
                            || event.person_uid != acting
                            || event.to_level != level
                        {
                            return Err(transfer_request_id_conflict());
                        }
                        outcome.created = Some(event.uid);
                        return Ok(ControlFlow::Break(outcome));
                    }
                    if store::transfers::party_for_actor(&self.store.pool, &transfer, &acting)
                        .await?
                        .is_none()
                    {
                        return Err(EngineError::Forbidden(
                            "agreement may be authored only by an accepted Transfer participant"
                                .into(),
                        ));
                    }
                    if store::transfers::revision_for_request(&self.store.pool, &request_id)
                        .await?
                        .is_some()
                        || store::transfers::invitation_event_for_request(
                            &self.store.pool,
                            &request_id,
                        )
                        .await?
                        .is_some()
                    {
                        return Err(transfer_request_id_conflict());
                    }
                    self.require_verified_transfer_revision(&transfer, expected_revision)
                        .await?;
                    let signer = self
                        .transfer_person_signer(&acting, verified_authorship.as_ref())
                        .await?;
                    let commit = store::transfers::transition_agreement(
                        &self.store.pool,
                        store::transfers::AgreementTransitionInput {
                            transfer_uid: transfer.clone(),
                            expected_revision,
                            idempotency_key: request_id,
                            person_uid: acting,
                            to_level: level,
                            authorization_intent_uid: verified_authorship
                                .as_ref()
                                .map(|value| value.intent_uid.clone()),
                        },
                        now,
                        |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                    )
                    .await?;
                    match commit {
                        store::transfers::AgreementTransitionCommit::Committed(committed) => {
                            outcome.facts = self.publish_committed_fact(committed.fact);
                            outcome.created = Some(committed.event.uid);
                        }
                        store::transfers::AgreementTransitionCommit::Replayed(replayed) => {
                            outcome.created = Some(replayed.event.uid);
                        }
                        store::transfers::AgreementTransitionCommit::Stale {
                            current_revision,
                            ..
                        } => {
                            return Err(EngineError::Conflict {
                                code: "transfer_revision_stale",
                                message: format!(
                                    "expected transfer revision {expected_revision}, current revision is {current_revision}"
                                ),
                            });
                        }
                    }
                }
                Action::AssignTransferAgreementLevel {
                    transfer,
                    expected_revision,
                    request_id,
                    person,
                    level,
                    expected,
                    expected_state,
                } => {
                    let transfer = self.resolve(&transfer).await?;
                    self.require_permission(actor.as_deref(), "transfer:update")
                        .await?;
                    let acting = self
                        .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                        .await?;
                    self.require_verified_transfer_revision(&transfer, expected_revision)
                        .await?;
                    let signer = self
                        .transfer_person_signer(&acting, verified_authorship.as_ref())
                        .await?;
                    let result = store::transfers::assign_agreement(
                        &self.store.pool,
                        store::transfers::AgreementTargetInput {
                            transition: store::transfers::AgreementTransitionInput {
                                transfer_uid: transfer.clone(),
                                expected_revision,
                                idempotency_key: request_id,
                                person_uid: acting.clone(),
                                to_level: level,
                                authorization_intent_uid: verified_authorship
                                    .as_ref()
                                    .map(|value| value.intent_uid.clone()),
                            },
                            expected,
                            expected_state,
                        },
                        now,
                        |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                    )
                    .await
                    .map_err(|error| match error {
                        store::StoreError::Protocol(ref message)
                            if matches!(
                                message.as_str(),
                                "transfer_agreement_source_changed"
                                    | "transfer_state_source_changed"
                            ) =>
                        {
                            crate::karma_transfer_actions::guard_error(error)
                        }
                        store::StoreError::Protocol(ref message)
                            if matches!(
                                message.as_str(),
                                "transfer_revision_stale"
                                    | "transfer_agreement_target_request_conflict"
                            ) =>
                        {
                            EngineError::Conflict {
                                code: match message.as_str() {
                                    "transfer_agreement_source_changed" => {
                                        "transfer_agreement_source_changed"
                                    }
                                    "transfer_revision_stale" => "transfer_revision_stale",
                                    _ => "transfer_request_id_conflict",
                                },
                                message: message.clone(),
                            }
                        }
                        other => EngineError::Store(other),
                    })?;
                    outcome.created = result.changes.last().map(|change| change.uid.clone());
                    outcome.data = Some(
                        serde_json::json!({"transfer":transfer,"person":acting,"before":result.before,"level":result.level,"changes":result.changes}),
                    );
                    if !result.replayed {
                        for fact in result.facts {
                            outcome.facts.extend(self.publish_committed_fact(fact));
                        }
                    }
                }
                Action::PublishTransfer {
                    transfer,
                    expected_revision,
                    request_id,
                    person,
                    expected,
                    expected_state,
                } => {
                    outcome = self
                        .publish_transfer_action(
                            transfer,
                            expected_revision,
                            request_id,
                            person,
                            expected,
                            expected_state,
                            actor.as_deref(),
                            now,
                            verified_authorship.as_ref(),
                        )
                        .await?;
                }
                Action::ActivateTransferFulfillment {
                    transfer,
                    promise,
                    fulfillment,
                    expected_revision,
                    request_id,
                    person,
                    expected,
                    expected_state,
                } => {
                    if request_id.trim().is_empty() || request_id.len() > 200 {
                        return Err(EngineError::Consequence(
                            "Use a request identity of 1–200 bytes".into(),
                        ));
                    }
                    outcome = self
                        .activate_transfer_fulfillment_action(
                            transfer,
                            promise,
                            fulfillment,
                            expected_revision,
                            person,
                            expected,
                            expected_state,
                            actor.as_deref(),
                            now,
                            verified_authorship.as_ref(),
                        )
                        .await?;
                }
                Action::ActivateTransferOccurrence {
                    transfer,
                    promise,
                    expected_revision,
                    request_id,
                    person,
                } => {
                    let transfer = self.resolve(&transfer).await?;
                    self.require_permission(actor.as_deref(), "transfer:update")
                        .await?;
                    let acting = self
                        .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                        .await?;
                    let promise = promise.trim().to_string();
                    if let Some(replayed) = store::transfers::occurrences_for_activation_request(
                        &self.store.pool,
                        &request_id,
                    )
                    .await?
                    {
                        if replayed.transfer_uid != transfer
                            || replayed.revision != expected_revision
                            || replayed.actor_person_uid != acting
                            || replayed.occurrences.len() != 1
                            || replayed.occurrences[0].promise_uid != promise
                        {
                            return Err(transfer_request_id_conflict());
                        }
                        outcome.created = Some(replayed.occurrences[0].uid.clone());
                        return Ok(ControlFlow::Break(outcome));
                    }
                    if store::transfers::phase4_request_for_request(&self.store.pool, &request_id)
                        .await?
                        .is_some()
                    {
                        return Err(transfer_request_id_conflict());
                    }
                    if store::transfers::source_group_state_for_transfer(
                        &self.store.pool,
                        &transfer,
                    )
                    .await?
                    .is_some_and(|state| state.satiated)
                    {
                        return Err(EngineError::Conflict {
                        code: "transfer_satiated",
                        message:
                            "another Transfer already completed this first-completes source group"
                                .into(),
                    });
                    }
                    if store::transfers::party_for_actor(&self.store.pool, &transfer, &acting)
                        .await?
                        .is_none()
                    {
                        return Err(EngineError::Forbidden(
                            "occurrence activation requires an accepted Transfer participant"
                                .into(),
                        ));
                    }
                    self.require_verified_transfer_revision(&transfer, expected_revision)
                        .await?;
                    let ready = protein::transfer_ready_promises_for_person(
                        &self.store,
                        &transfer,
                        &acting,
                    )
                    .await?;
                    if !ready.contains(&promise) {
                        return Err(EngineError::Conflict {
                            code: "transfer_promise_not_ready",
                            message:
                                "the selected promise is not policy-ready for the acting Person"
                                    .into(),
                        });
                    }
                    let readiness =
                        store::transfers::agreement_readiness_input(&self.store.pool, &transfer)
                            .await?;
                    if readiness.revision != expected_revision {
                        return Err(EngineError::Conflict {
                            code: "transfer_revision_stale",
                            message: format!(
                                "expected transfer revision {expected_revision}, current revision is {}",
                                readiness.revision
                            ),
                        });
                    }
                    let (opposite_promise_uid, giver_person_uid, receiver_person_uid) =
                    protein::transfer_occurrence_roles(&readiness, &promise).map_err(|code| {
                        EngineError::Conflict {
                            code,
                            message: "the selected promise does not have one unambiguous directed counterparty"
                                .into(),
                        }
                    })?;
                    let signer = self
                        .transfer_person_signer(&acting, verified_authorship.as_ref())
                        .await?;
                    let (expected, expected_state) =
                        crate::karma_transfer_actions::ACTIVATION_GUARD
                            .try_with(Clone::clone)
                            .unwrap_or_default();
                    let commit = store::transfers::activate_occurrences_guarded(
                        &self.store.pool,
                        store::transfers::ActivateOccurrencesInput {
                            transfer_uid: transfer,
                            expected_revision,
                            idempotency_key: request_id,
                            actor_person_uid: acting,
                            occurrences: vec![store::transfers::OccurrenceActivationInput {
                                promise_uid: promise,
                                opposite_promise_uid,
                                giver_person_uid,
                                receiver_person_uid,
                            }],
                            authorization_intent_uid: verified_authorship
                                .as_ref()
                                .map(|value| value.intent_uid.clone()),
                        },
                        expected.as_ref(),
                        expected_state.as_ref(),
                        now,
                        |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                    )
                    .await
                    .map_err(crate::karma_transfer_actions::guard_error)?;
                    match commit {
                        store::transfers::OccurrenceActivationCommit::Committed(committed) => {
                            outcome.created = committed
                                .occurrences
                                .first()
                                .map(|occurrence| occurrence.uid.clone());
                            outcome.facts = self.publish_committed_fact(committed.fact);
                        }
                        store::transfers::OccurrenceActivationCommit::Replayed(replayed) => {
                            outcome.created = replayed
                                .occurrences
                                .first()
                                .map(|occurrence| occurrence.uid.clone());
                        }
                        store::transfers::OccurrenceActivationCommit::Stale {
                            current_revision,
                            ..
                        } => {
                            return Err(EngineError::Conflict {
                                code: "transfer_revision_stale",
                                message: format!(
                                    "expected transfer revision {expected_revision}, current revision is {current_revision}"
                                ),
                            });
                        }
                        store::transfers::OccurrenceActivationCommit::Satiated {
                            winner_transfer_uid,
                        } => {
                            return Err(EngineError::Conflict {
                                code: "transfer_satiated",
                                message: format!(
                                    "Transfer cannot activate because `{winner_transfer_uid}` completed its first-completes source group"
                                ),
                            });
                        }
                    }
                }
                Action::SetTransferOccurrenceClaim {
                    occurrence,
                    request_id,
                    person,
                    role,
                    claimed,
                } => {
                    let acting = self
                        .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                        .await?;
                    self.require_permission(actor.as_deref(), "transfer:update")
                        .await?;
                    let role = match role {
                        TransferOccurrenceClaimRole::Delivery => {
                            nucleus::transfer::OccurrenceClaimRole::Delivery
                        }
                        TransferOccurrenceClaimRole::Receipt => {
                            nucleus::transfer::OccurrenceClaimRole::Receipt
                        }
                    };
                    if let Some(replayed) = store::transfers::occurrence_claim_for_request(
                        &self.store.pool,
                        &request_id,
                    )
                    .await?
                    {
                        if replayed.event.occurrence_uid != occurrence
                            || replayed.event.actor_person_uid != acting
                            || replayed.event.role != role
                            || replayed.event.asserted != claimed
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
                    let signer = self
                        .transfer_person_signer(&acting, verified_authorship.as_ref())
                        .await?;
                    let commit = store::transfers::set_occurrence_claim(
                        &self.store.pool,
                        store::transfers::OccurrenceClaimInput {
                            occurrence_uid: occurrence,
                            idempotency_key: request_id,
                            actor_person_uid: acting,
                            role,
                            asserted: claimed,
                            authorization_intent_uid: verified_authorship
                                .as_ref()
                                .map(|value| value.intent_uid.clone()),
                        },
                        now,
                        |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                    )
                    .await?;
                    match commit {
                        store::transfers::OccurrenceClaimCommit::Committed(committed) => {
                            outcome.created = Some(committed.event.uid);
                            outcome.facts = self.publish_committed_fact(committed.fact);
                        }
                        store::transfers::OccurrenceClaimCommit::Replayed(replayed) => {
                            outcome.created = Some(replayed.event.uid);
                        }
                    }
                }
                Action::CompleteTransferOccurrenceClaimsBulk {
                    request_id,
                    person,
                    review_token,
                    items,
                } => {
                    let acting = self
                        .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                        .await?;
                    self.require_permission(actor.as_deref(), "transfer:update")
                        .await?;
                    let items = items
                        .into_iter()
                        .map(|item| store::transfers::ReviewedBulkOccurrenceClaim {
                            occurrence_uid: item.occurrence,
                            transfer_uid: item.transfer,
                            expected_revision: item.expected_revision,
                            role: match item.role {
                                TransferOccurrenceClaimRole::Delivery => {
                                    nucleus::transfer::OccurrenceClaimRole::Delivery
                                }
                                TransferOccurrenceClaimRole::Receipt => {
                                    nucleus::transfer::OccurrenceClaimRole::Receipt
                                }
                            },
                            expected_delivery_claimed: item.expected_delivery_claimed,
                            expected_receipt_claimed: item.expected_receipt_claimed,
                        })
                        .collect();
                    let signer = self
                        .transfer_person_signer(&acting, verified_authorship.as_ref())
                        .await?;
                    let commit = store::transfers::complete_occurrence_claims_bulk(
                        &self.store.pool,
                        store::transfers::BulkOccurrenceClaimInput {
                            idempotency_key: request_id,
                            actor_person_uid: acting,
                            review_token,
                            items,
                            authorization_intent_uid: verified_authorship
                                .as_ref()
                                .map(|value| value.intent_uid.clone()),
                        },
                        now,
                        |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                    )
                    .await?;
                    match commit {
                        store::transfers::BulkOccurrenceClaimCommit::Committed(committed) => {
                            outcome.created = Some(committed.uid);
                            for fact in committed.facts {
                                outcome.facts.extend(self.publish_committed_fact(fact));
                            }
                        }
                        store::transfers::BulkOccurrenceClaimCommit::Replayed(replayed) => {
                            outcome.created = Some(replayed.uid);
                        }
                        store::transfers::BulkOccurrenceClaimCommit::Rejected(failures) => {
                            let message = failures
                                .into_iter()
                                .map(|failure| {
                                    format!(
                                        "{} [{}]: {}",
                                        failure.occurrence_uid, failure.code, failure.message
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join("; ");
                            return Err(EngineError::Conflict {
                                code: "transfer_bulk_preflight_failed",
                                message,
                            });
                        }
                    }
                }
                Action::SetTransferOccurrenceDispute {
                    occurrence,
                    request_id,
                    person,
                    disputed,
                } => {
                    let acting = self
                        .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                        .await?;
                    self.require_permission(actor.as_deref(), "transfer:update")
                        .await?;
                    if let Some(replayed) = store::transfers::occurrence_dispute_for_request(
                        &self.store.pool,
                        &request_id,
                    )
                    .await?
                    {
                        if replayed.event.occurrence_uid != occurrence
                            || replayed.event.actor_person_uid != acting
                            || replayed.event.disputed != disputed
                        {
                            return Err(transfer_request_id_conflict());
                        }
                        outcome.created = Some(replayed.event.uid);
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
                    let occurrence_row =
                        store::transfers::occurrence(&self.store.pool, &occurrence)
                            .await?
                            .ok_or_else(|| EngineError::Conflict {
                                code: "transfer_occurrence_missing",
                                message: "the occurrence does not exist".into(),
                            })?;
                    if occurrence_row.giver_person_uid != acting
                        && occurrence_row.receiver_person_uid != acting
                    {
                        return Err(EngineError::Conflict {
                            code: "transfer_occurrence_dispute_not_participant",
                            message: "only the occurrence giver or receiver may assert a dispute"
                                .into(),
                        });
                    }
                    let signer = self
                        .transfer_person_signer(&acting, verified_authorship.as_ref())
                        .await?;
                    let commit = store::transfers::set_occurrence_dispute(
                        &self.store.pool,
                        store::transfers::OccurrenceDisputeInput {
                            occurrence_uid: occurrence,
                            idempotency_key: request_id,
                            actor_person_uid: acting,
                            disputed,
                            authorization_intent_uid: verified_authorship
                                .as_ref()
                                .map(|value| value.intent_uid.clone()),
                        },
                        now,
                        |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                    )
                    .await?;
                    match commit {
                        store::transfers::OccurrenceDisputeCommit::Committed(committed) => {
                            outcome.created = Some(committed.event.uid);
                            outcome.facts = self.publish_committed_fact(committed.fact);
                        }
                        store::transfers::OccurrenceDisputeCommit::Replayed(replayed) => {
                            outcome.created = Some(replayed.event.uid);
                        }
                    }
                }
                Action::SetTransferChildRequirement {
                    transfer,
                    child,
                    required,
                    expected_revision,
                    request_id,
                    person,
                } => {
                    let transfer = self.resolve(&transfer).await?;
                    let child = self
                        .resolve_transfer_visible_record(&child, actor.as_deref())
                        .await?;
                    let acting = self
                        .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                        .await?;
                    self.require_verified_transfer_revision(&transfer, expected_revision)
                        .await?;
                    let signer = self
                        .transfer_person_signer(&acting, verified_authorship.as_ref())
                        .await?;
                    let fact = store::transfer_children::set(
                        &self.store.pool,
                        store::transfer_children::Input {
                            transfer,
                            child,
                            required,
                            expected_revision,
                            person: acting,
                            request_id,
                            authorization_intent_uid: verified_authorship
                                .as_ref()
                                .map(|value| value.intent_uid.clone()),
                        },
                        now,
                        |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                    )
                    .await?;
                    if let Some(fact) = fact {
                        outcome.facts = self.publish_committed_fact(fact);
                    }
                }
                Action::ProposeTransferLoanExtension {
                    transfer,
                    exchange,
                    until,
                    expected_revision,
                    request_id,
                    person,
                } => {
                    let acting = self
                        .transfer_action_person(actor.as_deref(), Some(&person), Some(&person))
                        .await?;
                    let transfer = self.resolve(&transfer).await?;
                    self.require_transfer_origin_authority(&transfer).await?;
                    self.require_verified_transfer_revision(&transfer, expected_revision)
                        .await?;
                    let signer = self
                        .transfer_person_signer(&acting, verified_authorship.as_ref())
                        .await?;
                    let result = store::transfer_loans::extend(
                        &self.store.pool,
                        store::transfer_loans::Extension {
                            transfer,
                            exchange,
                            until,
                            expected_revision,
                            request_id,
                            person: acting,
                            authorization_intent_uid: verified_authorship
                                .as_ref()
                                .map(|value| value.intent_uid.clone()),
                        },
                        now,
                        |hash| signer.as_ref().map(|signer| signer.sign_hash(hash)),
                    )
                    .await?;
                    outcome.created = Some(result.uid);
                    if let Some(fact) = result.fact {
                        outcome.facts = self.publish_committed_fact(fact);
                    }
                }
                Action::ProposeTransferCancellation {
                    transfer,
                    occurrence,
                    expected_revision,
                    expected_remaining_quantity,
                    request_id,
                    person,
                } => {
                    let transfer = self.resolve(&transfer).await?;
                    let acting = self
                        .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                        .await?;
                    self.require_verified_transfer_revision(&transfer, expected_revision)
                        .await?;
                    let signer = self
                        .transfer_person_signer(&acting, verified_authorship.as_ref())
                        .await?;
                    let result = store::transfer_cancellations::propose(
                        &self.store.pool,
                        store::transfer_cancellations::Proposal {
                            transfer,
                            occurrence,
                            expected_revision,
                            quantity: expected_remaining_quantity,
                            person: acting,
                            request_id,
                            authorization_intent_uid: verified_authorship
                                .as_ref()
                                .map(|value| value.intent_uid.clone()),
                        },
                        now,
                        |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                    )
                    .await?;
                    outcome.created = Some(result.uid);
                    if let Some(fact) = result.fact {
                        outcome.facts = self.publish_committed_fact(fact);
                    }
                }
                Action::ApplyTransferCancellation {
                    transfer,
                    cancellation,
                    expected_revision,
                    request_id,
                    person,
                } => {
                    let transfer = self.resolve(&transfer).await?;
                    let acting = self
                        .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                        .await?;
                    self.require_verified_transfer_revision(&transfer, expected_revision)
                        .await?;
                    let signer = self
                        .transfer_person_signer(&acting, verified_authorship.as_ref())
                        .await?;
                    let result = store::transfer_cancellations::apply(
                        &self.store.pool,
                        store::transfer_cancellations::Application {
                            transfer,
                            cancellation,
                            expected_revision,
                            person: acting,
                            request_id,
                            authorization_intent_uid: verified_authorship
                                .as_ref()
                                .map(|value| value.intent_uid.clone()),
                        },
                        now,
                        |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                    )
                    .await?;
                    outcome.created = Some(result.uid);
                    if let Some(fact) = result.fact {
                        outcome.facts = self.publish_committed_fact(fact);
                    }
                }
                Action::SetRecordStockLimit {
                    record,
                    person,
                    minimum,
                    expected_version,
                    request_id,
                } => {
                    let acting = self
                        .transfer_action_person(actor.as_deref(), Some(&person), Some(&person))
                        .await?;
                    let record = self.resolve(&record).await?;
                    let signer = self
                        .transfer_person_signer(&acting, verified_authorship.as_ref())
                        .await?
                        .ok_or_else(|| {
                            EngineError::Forbidden(
                                "stock limits require the owner's signing key".into(),
                            )
                        })?;
                    outcome.created = Some(
                        store::transfer_stock::set(
                            &self.store.pool,
                            store::transfer_stock::Input {
                                record,
                                person: acting,
                                minimum,
                                expected_version,
                                request_id,
                            },
                            now,
                            &signer.key_id,
                            &signer.public_key_b64(),
                            |hash| Some(signer.sign_hash(hash)),
                        )
                        .await?,
                    );
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
