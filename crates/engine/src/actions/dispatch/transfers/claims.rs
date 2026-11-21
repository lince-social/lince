// non-lean-yet

use super::super::{
    Action, ActionOutcome, ControlFlow, DateTime, DispatchOutcome, Engine, EngineError,
    PromiseState, TransferOccurrenceClaimRole, Utc, VerifiedActionAuthorship, invalid_dispatch,
    normalize_transfer_place, normalize_transfer_window, transfer_request_id_conflict,
};

impl Engine {
    pub(in super::super) async fn dispatch_transfers_claims(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
        verified_authorship: Option<VerifiedActionAuthorship>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
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
                if !replaying && (source.state != PromiseState::Open || source.party_uid.is_none())
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
                    store::transfers::invitations_for_transfer(&self.store.pool, &transfer).await?;
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
                if let Some(token) = terms.party.as_deref().map(str::trim)
                    && token != claimant
                {
                    let party = self.resolve(token).await?;
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
                if let Some(replayed) =
                    store::transfers::occurrence_claim_for_request(&self.store.pool, &request_id)
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
                if let Some(replayed) =
                    store::transfers::occurrence_dispute_for_request(&self.store.pool, &request_id)
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
                let occurrence_row = store::transfers::occurrence(&self.store.pool, &occurrence)
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
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
