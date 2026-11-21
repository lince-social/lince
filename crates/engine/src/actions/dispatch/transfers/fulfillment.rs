// non-lean-yet

use super::super::{
    Action, ActionOutcome, ControlFlow, DateTime, DispatchOutcome, Engine, EngineError, Utc,
    VerifiedActionAuthorship, invalid_dispatch, transfer_phase_locked,
    transfer_request_id_conflict,
};

impl Engine {
    pub(in super::super) async fn dispatch_transfers_fulfillment(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
        verified_authorship: Option<VerifiedActionAuthorship>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
            Action::ActivateTransfer { transfer } => {
                if transfer_phase_locked() {
                    return Err(EngineError::Conflict {
                        code: "transfer_phase_4_not_available",
                        message: "transfer activation is not available before occurrence modeling"
                            .into(),
                    });
                }
                let transfer = self.resolve(&transfer).await?;
                self.require_transfer_editor(&transfer, actor.as_deref())
                    .await?;
                let activated = crate::transfer::activate_promises(&self.store, &transfer).await?;
                outcome.facts = self
                    .annotate(
                        transfer,
                        actor,
                        serde_json::json!({
                            "action": "activate-transfer",
                            "activated_promises": activated,
                        }),
                        now,
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
                original_authorities,
            } => {
                let transfer = self.resolve(&transfer).await?;
                self.require_permission(actor.as_deref(), "transfer:update")
                    .await?;
                let acting = self
                    .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                    .await?;
                let promise = promise.trim().to_string();
                let original_authority_records =
                    nucleus::transfer::receipt::original_authority_records(&original_authorities)
                        .map_err(EngineError::Consequence)?;
                if let Some(replayed) = store::transfers::occurrences_for_activation_request(
                    &self.store.pool,
                    &request_id,
                )
                .await?
                {
                    let original: nucleus::transfer::TransferActivationEvidence =
                        serde_json::from_str(replayed.fact.payload.as_deref().ok_or_else(
                            || {
                                EngineError::Consequence(
                                    "Original activation evidence is unavailable".into(),
                                )
                            },
                        )?)?;
                    if replayed.transfer_uid != transfer
                        || replayed.revision != expected_revision
                        || replayed.actor_person_uid != acting
                        || replayed.occurrences.len() != 1
                        || replayed.occurrences[0].promise_uid != promise
                        || original.original_authority_records != original_authority_records
                    {
                        return Err(transfer_request_id_conflict());
                    }
                    outcome.created = Some(replayed.occurrences[0].uid.clone());
                    return Ok(ControlFlow::Break(outcome));
                }
                self.check_transfer_participation_records(
                    &original_authority_records,
                    actor.as_deref(),
                    now,
                )
                .await?;
                if store::transfers::phase4_request_for_request(&self.store.pool, &request_id)
                    .await?
                    .is_some()
                {
                    return Err(transfer_request_id_conflict());
                }
                if store::transfers::source_group_state_for_transfer(&self.store.pool, &transfer)
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
                        "occurrence activation requires an accepted Transfer participant".into(),
                    ));
                }
                self.require_verified_transfer_revision(&transfer, expected_revision)
                    .await?;
                let ready =
                    protein::transfer_ready_promises_for_person(&self.store, &transfer, &acting)
                        .await?;
                if !ready.contains(&promise) {
                    return Err(EngineError::Conflict {
                        code: "transfer_promise_not_ready",
                        message: "the selected promise is not policy-ready for the acting Person"
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
                let (expected, expected_state) = crate::karma_transfer_actions::ACTIVATION_GUARD
                    .try_with(Clone::clone)
                    .unwrap_or_default();
                let commit = store::transfers::activate_occurrences_guarded(
                    &self.store.pool,
                    store::transfers::ActivateOccurrencesInput {
                        original_authority_records,
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
            Action::DesignateTransferExecutor {
                transfer_uid,
                cell_uid,
            } => {
                let transfer = self.resolve(&transfer_uid).await?;
                self.require_transfer_editor(&transfer, actor.as_deref())
                    .await?;
                self.require_transfer_state_writer("transfer", &[&transfer])
                    .await?;
                let local = store::cells::local(&self.store.pool)
                    .await?
                    .ok_or_else(|| {
                        EngineError::Consequence("Transfer delivery requires a writing Cell".into())
                    })?;
                if cell_uid.as_ref().is_some_and(|cell| cell != &local.uid) {
                    return Err(EngineError::Conflict {
                        code: "transfer_origin_cell_required",
                        message:
                            "Transfer delivery and its queue stay on the Transfer's writing Cell"
                                .into(),
                    });
                }
                store::executor::designate(&self.store.pool, &transfer, Some(&local.uid)).await?;
                outcome.warnings.push("This Transfer's writing Cell delivers it. Deliveries wait while that Cell is offline.".into());
            }
            Action::InspectTransferKarma { transfer, person } => {
                outcome.data = Some(
                    Box::pin(self.inspect_transfer_karma(
                        &transfer,
                        person.as_deref(),
                        actor.as_deref(),
                        now,
                    ))
                    .await?,
                );
            }
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
