// non-lean-yet

use super::super::{
    Action, ActionOutcome, ControlFlow, DateTime, DispatchOutcome, Engine, EngineError,
    PromiseState, Utc, VerifiedActionAuthorship, invalid_dispatch, normalize_transfer_window,
    transfer_request_id_conflict,
};

impl Engine {
    pub(in super::super) async fn dispatch_transfers_amendments(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
        verified_authorship: Option<VerifiedActionAuthorship>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
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
                        transfer: transfer.clone(),
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
                if let Err(error) = self.location_stop_ended_transfer(&transfer).await {
                    tracing::debug!(%error,"Location stop will be retried by the location worker");
                }
            }
            Action::ReopenTransferPromise {
                transfer,
                promise,
                expected_revision,
                request_id,
                person,
                window_end,
                open,
            } => {
                let transfer = self.resolve(&transfer).await?;
                let acting = self
                    .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                    .await?;
                self.require_permission(actor.as_deref(), "transfer:update")
                    .await?;
                if let Some(existing) =
                    store::transfers::promise_successor_for_request(&self.store.pool, &request_id)
                        .await?
                {
                    let requested_window_end = window_end
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(|value| DateTime::parse_from_rfc3339(value).map(|_| value.to_string()))
                        .transpose()
                        .map_err(|_| EngineError::Conflict {
                            code: "transfer_window_invalid",
                            message: "reopened promise window end is not RFC 3339".into(),
                        })?;
                    let existing_successor =
                        store::transfers::promises_of(&self.store.pool, &transfer)
                            .await?
                            .into_iter()
                            .find(|candidate| candidate.uid == existing.successor_promise_uid)
                            .ok_or_else(|| EngineError::Conflict {
                                code: "transfer_promise_successor_missing",
                                message: "reopened promise successor no longer exists".into(),
                            })?;
                    if existing.transfer_uid != transfer
                        || existing.predecessor_promise_uid != promise
                        || existing.actor_person_uid != acting
                        || existing.revision != expected_revision.saturating_add(1)
                        || (existing_successor.state == PromiseState::Open) != open
                        || existing_successor.window_end != requested_window_end
                    {
                        return Err(transfer_request_id_conflict());
                    }
                    outcome.created = Some(existing.successor_promise_uid);
                    return Ok(ControlFlow::Break(outcome));
                }
                let transfer_row = store::transfers::get(&self.store.pool, &transfer)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(transfer.clone()))?;
                if transfer_row.revision as u64 != expected_revision {
                    return Err(EngineError::Conflict {
                        code: "transfer_revision_stale",
                        message: format!(
                            "expected transfer revision {expected_revision}, current revision is {}",
                            transfer_row.revision
                        ),
                    });
                }
                let transfer_record = store::records::get(&self.store.pool, &transfer)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(transfer.clone()))?;
                let current_promises =
                    store::transfers::promises_of(&self.store.pool, &transfer).await?;
                let predecessor = current_promises
                    .iter()
                    .find(|candidate| candidate.uid == promise)
                    .ok_or_else(|| EngineError::Conflict {
                        code: "transfer_promise_missing",
                        message: "predecessor promise no longer exists".into(),
                    })?;
                let predecessor_expired =
                    matches!(
                        predecessor.state,
                        PromiseState::Proposed | PromiseState::Agreed
                    ) && predecessor.window_end.as_deref().is_some_and(|window_end| {
                        DateTime::parse_from_rfc3339(window_end)
                            .is_ok_and(|window_end| window_end.with_timezone(&Utc) <= now)
                    });
                if !matches!(
                    predecessor.state,
                    PromiseState::Broken | PromiseState::Withdrawn
                ) && !predecessor_expired
                {
                    return Err(EngineError::Conflict {
                        code: "transfer_promise_not_reopenable",
                        message:
                            "only a broken, withdrawn, or expired proposed promise may be reopened"
                                .into(),
                    });
                }
                if predecessor.party_uid.as_deref() != Some(acting.as_str())
                    || store::transfers::party_for_actor(&self.store.pool, &transfer, &acting)
                        .await?
                        .is_none()
                {
                    return Err(EngineError::Forbidden(
                        "only the predecessor promise owner may reopen it".into(),
                    ));
                }
                let remaining =
                    match store::transfers::occurrence_for_promise(&self.store.pool, &promise)
                        .await?
                    {
                        Some(occurrence) => store::transfers::occurrence_settlement_progress(
                            &self.store.pool,
                            &occurrence.uid,
                        )
                        .await?
                        .map(|progress| progress.remaining_quantity)
                        .unwrap_or(occurrence.quantity),
                        None => predecessor.delta.abs(),
                    };
                if !remaining.is_finite() || remaining <= 0.0 {
                    return Err(EngineError::Conflict {
                        code: "transfer_promise_fully_settled",
                        message: "a fully settled promise has no remainder to reopen".into(),
                    });
                }
                let (_, successor_window_end) =
                    normalize_transfer_window(None, window_end, now, None)?;
                let current_fact =
                    store::transfers::revision_fact(&self.store.pool, &transfer, expected_revision)
                        .await?
                        .ok_or_else(|| EngineError::Conflict {
                            code: "transfer_revision_evidence_missing",
                            message: "current signed Transfer revision is unavailable".into(),
                        })?;
                let current_evidence: nucleus::transfer::TransferRevisionEvidence =
                    serde_json::from_str(current_fact.payload.as_deref().unwrap_or("")).map_err(
                        |_| EngineError::Conflict {
                            code: "transfer_revision_evidence_invalid",
                            message: "current signed Transfer revision evidence is invalid".into(),
                        },
                    )?;
                let signed_by_uid = current_evidence
                    .terms
                    .promises
                    .iter()
                    .map(|promise| (promise.uid.as_str(), promise))
                    .collect::<std::collections::HashMap<_, _>>();
                let mut promises = current_promises
                    .iter()
                    .filter(|promise| {
                        promise.uid != predecessor.uid
                            && matches!(
                                promise.state,
                                PromiseState::Open | PromiseState::Proposed | PromiseState::Agreed
                            )
                    })
                    .map(|promise| {
                        let signed = signed_by_uid.get(promise.uid.as_str()).copied();
                        store::transfers::DraftPromiseRevisionInput {
                            uid: Some(promise.uid.clone()),
                            item: signed.and_then(|promise| promise.item.clone()),
                            source_promise_uid: signed
                                .and_then(|promise| promise.source_promise_uid.clone()),
                            record_uid: promise.record_uid.clone(),
                            concept_uid: promise.concept_uid.clone(),
                            unit_uid: promise.unit_uid.clone(),
                            person_uid: promise.party_uid.clone(),
                            open: promise.state == PromiseState::Open,
                            delta: promise.delta,
                            window_start: promise.window_start.clone(),
                            window_end: promise.window_end.clone(),
                            location: promise.location.clone(),
                            condition: promise.condition.clone(),
                            reserve_from: promise.reserve_from.clone(),
                            open_reuse_policy: promise.open_reuse_policy,
                        }
                    })
                    .collect::<Vec<_>>();
                let successor_uid = nucleus::new_uid("p");
                let mut successor_item = signed_by_uid
                    .get(predecessor.uid.as_str())
                    .and_then(|promise| promise.item.clone());
                if let Some(exchange) = successor_item
                    .as_mut()
                    .and_then(|item| item.exchange.as_mut())
                {
                    exchange.uid = nucleus::new_uid("exchange");
                }
                promises.push(store::transfers::DraftPromiseRevisionInput {
                    uid: Some(successor_uid.clone()),
                    item: successor_item,
                    source_promise_uid: Some(promise.clone()),
                    record_uid: predecessor.record_uid.clone(),
                    concept_uid: predecessor.concept_uid.clone(),
                    unit_uid: predecessor.unit_uid.clone(),
                    person_uid: Some(acting.clone()),
                    open,
                    delta: predecessor.delta.signum() * remaining,
                    window_start: None,
                    window_end: successor_window_end,
                    location: predecessor.location.clone(),
                    condition: predecessor.condition.clone(),
                    reserve_from: predecessor.reserve_from.clone(),
                    open_reuse_policy: predecessor.open_reuse_policy,
                });
                let dependencies = store::transfers::dependencies_of(&self.store.pool, &transfer)
                    .await?
                    .into_iter()
                    .map(|dependency| store::transfers::TransferDependencyInput {
                        uid: Some(dependency.uid),
                        scope: dependency.scope,
                        promise_uid: dependency.promise_uid.map(|scoped| {
                            if scoped == promise {
                                successor_uid.clone()
                            } else {
                                scoped
                            }
                        }),
                        upstream_kind: dependency.upstream_kind,
                        upstream_uid: dependency.upstream_uid,
                        required_state: dependency.required_state,
                    })
                    .collect();
                let retained_invitation_uids =
                    store::transfers::invitations_for_transfer(&self.store.pool, &transfer)
                        .await?
                        .into_iter()
                        .filter(|invitation| {
                            invitation.status == store::transfers::TransferInvitationStatus::Pending
                        })
                        .map(|invitation| invitation.uid)
                        .collect();
                let signer = self
                    .transfer_person_signer(&acting, verified_authorship.as_ref())
                    .await?;
                let commit = store::transfers::reopen_promise_revision(
                    &self.store.pool,
                    store::transfers::WholeDraftRevisionInput {
                        transfer_uid: transfer.clone(),
                        expected_revision,
                        idempotency_key: request_id,
                        creator_person_uid: acting.clone(),
                        proposal_author_person_uid: acting.clone(),
                        terms: store::transfers::TransferDraftTermsInput {
                            public_history: store::transfers::public_history(
                                &self.store.pool,
                                &transfer,
                            )
                            .await?,
                            slug: transfer_record.slug,
                            head: transfer_record.head,
                            agreement_type: transfer_row.agreement_type,
                            agreement_pct: transfer_row.agreement_pct,
                            settlement: transfer_row.settlement,
                            visibility: transfer_row.visibility,
                            max_proximity: transfer_row.max_proximity,
                            satiation: transfer_row.satiation,
                            parent_uid: transfer_row.parent_uid,
                            source_uid: transfer_row.source_uid,
                            reserve_default: transfer_row
                                .reserve_default
                                .unwrap_or_else(|| "none".into()),
                            require_confirmation: transfer_row.require_confirmation,
                            default_place: transfer_row.default_place,
                        },
                        retained_invitation_uids,
                        promises,
                        dependencies,
                        successor: Some(store::transfers::PromiseSuccessorInput {
                            predecessor_promise_uid: promise,
                            successor_promise_uid: successor_uid.clone(),
                        }),
                        authorization_intent_uid: verified_authorship
                            .as_ref()
                            .map(|value| value.intent_uid.clone()),
                    },
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
                outcome.created = Some(successor_uid);
            }
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
