use super::*;

impl Engine {
    pub(super) fn dispatch_part_8(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
        verified_authorship: Option<VerifiedActionAuthorship>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = DispatchOutcome> + Send + '_>> {
        Box::pin(async move {
            let mut outcome = ActionOutcome::default();
            match action {
                correction_action @ (Action::CreateTransferRemainderDraft { .. }
                | Action::CreateReversingTransferDraft { .. }) => {
                    let (
                        occurrence,
                        expected_revision,
                        expected_remaining_quantity,
                        request_id,
                        person,
                        reversing,
                    ) = match correction_action {
                        Action::CreateTransferRemainderDraft {
                            occurrence,
                            expected_revision,
                            expected_remaining_quantity,
                            request_id,
                            person,
                        } => (
                            occurrence,
                            expected_revision,
                            expected_remaining_quantity,
                            request_id,
                            person,
                            false,
                        ),
                        Action::CreateReversingTransferDraft {
                            occurrence,
                            expected_revision,
                            canonical_quantity,
                            request_id,
                            person,
                        } => (
                            occurrence,
                            expected_revision,
                            canonical_quantity,
                            request_id,
                            person,
                            true,
                        ),
                        _ => unreachable!("matched Transfer correction draft action"),
                    };
                    let kind = if reversing { "reversal" } else { "remainder" };
                    let acting = self
                        .transfer_action_person(actor.as_deref(), person.as_deref(), None)
                        .await?;
                    self.require_permission(actor.as_deref(), "transfer:update")
                        .await?;
                    if let Some(existing) =
                        store::transfers::correction_link_for_request(&self.store.pool, &request_id)
                            .await?
                    {
                        if existing.kind != kind
                            || existing.source_occurrence_uid != occurrence
                            || existing.source_revision != expected_revision
                            || existing.actor_person_uid != acting
                            || !transfer_settlement_values_match(
                                existing.canonical_quantity,
                                expected_remaining_quantity,
                            )
                        {
                            return Err(transfer_request_id_conflict());
                        }
                        outcome.created = Some(existing.created_transfer_uid);
                        return Ok(ControlFlow::Break(outcome));
                    }
                    let occurrence_row =
                        store::transfers::occurrence(&self.store.pool, &occurrence)
                            .await?
                            .ok_or_else(|| EngineError::Conflict {
                                code: "transfer_occurrence_missing",
                                message: "correction occurrence no longer exists".into(),
                            })?;
                    let source_transfer =
                        store::transfers::get(&self.store.pool, &occurrence_row.transfer_uid)
                            .await?
                            .ok_or_else(|| {
                                EngineError::UnknownRecord(occurrence_row.transfer_uid.clone())
                            })?;
                    if source_transfer.revision as u64 != expected_revision {
                        return Err(EngineError::Conflict {
                            code: "transfer_revision_stale",
                            message: format!(
                                "expected transfer revision {expected_revision}, current revision is {}",
                                source_transfer.revision
                            ),
                        });
                    }
                    let source_record =
                        store::records::get(&self.store.pool, &source_transfer.record_uid)
                            .await?
                            .ok_or_else(|| {
                                EngineError::UnknownRecord(source_transfer.record_uid.clone())
                            })?;
                    let source_promise = store::transfers::promises_of(
                        &self.store.pool,
                        &occurrence_row.transfer_uid,
                    )
                    .await?
                    .into_iter()
                    .find(|promise| promise.uid == occurrence_row.promise_uid)
                    .ok_or_else(|| EngineError::Conflict {
                        code: "transfer_promise_missing",
                        message: "correction source promise no longer exists".into(),
                    })?;
                    if !reversing && source_promise.party_uid.as_deref() != Some(acting.as_str()) {
                        return Err(EngineError::Forbidden(
                            "only the source-promise owner may create its correction draft".into(),
                        ));
                    }
                    let progress = store::transfers::occurrence_settlement_progress(
                        &self.store.pool,
                        &occurrence,
                    )
                    .await?
                    .ok_or_else(|| EngineError::Conflict {
                        code: "transfer_occurrence_missing",
                        message: "correction occurrence no longer exists".into(),
                    })?;
                    let quantity = if reversing {
                        if self
                            .transfer_creator_person(&occurrence_row.transfer_uid)
                            .await?
                            != acting
                        {
                            return Err(EngineError::Forbidden(
                                "only the original Transfer creator may propose a reversal".into(),
                            ));
                        }
                        if !expected_remaining_quantity.is_finite()
                            || expected_remaining_quantity <= 0.0
                            || !transfer_settlement_values_match(
                                expected_remaining_quantity,
                                occurrence_row.quantity,
                            )
                        {
                            return Err(EngineError::Conflict {
                                code: "transfer_reversal_quantity_stale",
                                message:
                                    "reversal must use the exact canonical occurrence quantity"
                                        .into(),
                            });
                        }
                        expected_remaining_quantity
                    } else {
                        if store::transfers::effective_occurrence_remainder_policy(
                            &self.store.pool,
                            &occurrence,
                            &acting,
                        )
                        .await?
                            != nucleus::transfer::TransferRemainderPolicy::LocalDraft
                        {
                            return Err(EngineError::Conflict {
                                code: "transfer_remainder_policy_changed",
                                message: "occurrence remainder policy is not local_draft".into(),
                            });
                        }
                        if progress.settled_quantity <= 0.0
                            || progress.remaining_quantity <= 0.0
                            || !transfer_settlement_values_match(
                                expected_remaining_quantity,
                                progress.remaining_quantity,
                            )
                        {
                            return Err(EngineError::Conflict {
                                code: "transfer_remainder_stale",
                                message:
                                    "reviewed remainder is no longer the exact partial remainder"
                                        .into(),
                            });
                        }
                        progress.remaining_quantity
                    };
                    let delta = if reversing {
                        if occurrence_row.giver_person_uid == acting {
                            quantity
                        } else if occurrence_row.receiver_person_uid == acting {
                            -quantity
                        } else {
                            return Err(EngineError::Forbidden(
                                "reversal creator must be an occurrence participant".into(),
                            ));
                        }
                    } else {
                        source_promise.delta.signum() * quantity
                    };
                    let (correction_record_uid, correction_concept_uid) = if !reversing
                        || source_promise.party_uid.as_deref() == Some(acting.as_str())
                    {
                        (
                            occurrence_row.record_uid.clone(),
                            occurrence_row.concept_uid.clone(),
                        )
                    } else if occurrence_row.concept_uid.is_some() {
                        (None, occurrence_row.concept_uid.clone())
                    } else {
                        return Err(EngineError::Conflict {
                        code: "transfer_reversal_private_record_without_concept",
                        message: "reversal needs a canonical concept when the source Record belongs to another participant"
                            .into(),
                    });
                    };
                    let organ_uid = store::organs::local(&self.store.pool)
                        .await?
                        .map(|organ| organ.uid);
                    let signer = self
                        .transfer_person_signer(&acting, verified_authorship.as_ref())
                        .await?;
                    let mut correction_item =
                        store::transfers::item_of(&self.store.pool, &occurrence_row.promise_uid)
                            .await?;
                    let mut correction_invitees = Vec::new();
                    if let Some(exchange) = correction_item
                        .as_mut()
                        .and_then(|item| item.exchange.as_mut())
                    {
                        exchange.uid = nucleus::new_uid("exchange");
                        if reversing {
                            std::mem::swap(&mut exchange.giver, &mut exchange.receiver);
                        }
                        let other = if exchange.giver == acting {
                            &exchange.receiver
                        } else {
                            &exchange.giver
                        };
                        correction_invitees.push(other.clone());
                    }
                    let created = store::transfers::create_draft(
                        &self.store.pool,
                        store::transfers::NewTransferDraft {
                            idempotency_key: request_id,
                            slug: None,
                            head: format!(
                                "{}: {}",
                                if reversing { "Reversal" } else { "Remainder" },
                                source_record.head
                            ),
                            agreement_type: nucleus::transfer::AgreementType::Full.as_str().into(),
                            agreement_pct: None,
                            satiation: None,
                            parent_uid: None,
                            source_uid: None,
                            visibility: "hidden".into(),
                            max_proximity: None,
                            reserve_default: "none".into(),
                            require_confirmation: source_transfer.require_confirmation,
                            default_place: occurrence_row.location.clone(),
                            creator_person: acting.clone(),
                            invitees: correction_invitees,
                            promises: vec![store::transfers::DraftPromise {
                                uid: None,
                                item: correction_item,
                                record_uid: correction_record_uid,
                                concept_uid: correction_concept_uid,
                                unit_uid: occurrence_row.unit_uid.clone(),
                                person_uid: Some(acting.clone()),
                                open: false,
                                delta,
                                window_start: None,
                                window_end: None,
                                location: occurrence_row.location.clone(),
                                condition: None,
                                reserve_from: "none".into(),
                                open_reuse_policy:
                                    nucleus::transfer::OpenPromiseReusePolicy::Duplicate,
                            }],
                            dependencies: Vec::new(),
                            organ_uid,
                            evidence_action: format!("create-transfer-{kind}-draft"),
                            correction: Some(store::transfers::NewTransferCorrectionLink {
                                kind: kind.into(),
                                source_transfer_uid: occurrence_row.transfer_uid,
                                source_occurrence_uid: occurrence,
                                source_revision: expected_revision,
                                canonical_quantity: quantity,
                            }),
                            authorization_intent_uid: verified_authorship
                                .as_ref()
                                .map(|value| value.intent_uid.clone()),
                        },
                        now,
                        Some(acting),
                        actor.clone(),
                        |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                    )
                    .await?;
                    outcome.created = Some(created.transfer_uid);
                    if !created.replayed {
                        outcome.facts = self.publish_committed_fact(created.fact);
                        for fact in created
                            .invitation_event_facts
                            .into_iter()
                            .chain(created.parent_facts)
                        {
                            outcome.facts.extend(self.publish_committed_fact(fact));
                        }
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
                    if let Some(existing) = store::transfers::promise_successor_for_request(
                        &self.store.pool,
                        &request_id,
                    )
                    .await?
                    {
                        let requested_window_end = window_end
                            .as_deref()
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .map(|value| {
                                DateTime::parse_from_rfc3339(value).map(|_| value.to_string())
                            })
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
                    let transfer_record =
                        store::records::get(&self.store.pool, &transfer)
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
                    let current_fact = store::transfers::revision_fact(
                        &self.store.pool,
                        &transfer,
                        expected_revision,
                    )
                    .await?
                    .ok_or_else(|| EngineError::Conflict {
                        code: "transfer_revision_evidence_missing",
                        message: "current signed Transfer revision is unavailable".into(),
                    })?;
                    let current_evidence: nucleus::transfer::TransferRevisionEvidence =
                        serde_json::from_str(current_fact.payload.as_deref().unwrap_or(""))
                            .map_err(|_| EngineError::Conflict {
                                code: "transfer_revision_evidence_invalid",
                                message: "current signed Transfer revision evidence is invalid"
                                    .into(),
                            })?;
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
                                    PromiseState::Open
                                        | PromiseState::Proposed
                                        | PromiseState::Agreed
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
                    let dependencies =
                        store::transfers::dependencies_of(&self.store.pool, &transfer)
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
                                invitation.status
                                    == store::transfers::TransferInvitationStatus::Pending
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
                    let slice = store::transfers::occurrence_settlement_slice(
                        &self.store.pool,
                        &settlement,
                    )
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
                Action::DeclareEquivalence { a, b } => {
                    let a = store::concepts::resolve(&self.store.pool, &a)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(a))?;
                    let b = store::concepts::resolve(&self.store.pool, &b)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(b))?;
                    store::concepts::declare_equivalence(
                        &self.store.pool,
                        &a,
                        &b,
                        actor.as_deref(),
                    )
                    .await?;
                }
                Action::RenameRole {
                    role,
                    expected_revision,
                    name,
                } => {
                    self.change_role(actor.as_deref(), role, expected_revision, Some(&name))
                        .await?;
                }
                Action::DeleteRole {
                    role,
                    expected_revision,
                } => {
                    self.change_role(actor.as_deref(), role, expected_revision, None)
                        .await?;
                }
                Action::CreateRole { name } => {
                    self.require_permission(actor.as_deref(), "role:create")
                        .await?;
                    let role_id = store::auth::ensure_role(&self.store.pool, &name).await?;
                    outcome.created = Some(role_id.to_string());
                }
                Action::CreateUser {
                    username,
                    name,
                    password,
                    role,
                } => {
                    self.require_permission(actor.as_deref(), "user:create")
                        .await?;
                    let role_id = store::auth::role_by_name(&self.store.pool, &role)
                        .await?
                        .ok_or_else(|| {
                            EngineError::Consequence(format!("unknown role `{role}`"))
                        })?;
                    let password_hash = self
                        .passwords
                        .hash(
                            crate::private_password::PasswordInput::new(password.into_bytes())
                                .map_err(|error| EngineError::Consequence(error.to_string()))?,
                        )
                        .await
                        .map_err(|error| EngineError::Consequence(error.to_string()))?;
                    let person_uid = store::auth::create_person_login(
                        &self.store.pool,
                        &name,
                        &username,
                        password_hash.as_phc(),
                        role_id,
                    )
                    .await?;
                    outcome.facts = self
                        .append(
                            NewFact {
                                actor_uid: actor,
                                ..NewFact::quantity_f64(person_uid.clone(), 0.0, Cause::user_edit())
                            },
                            now,
                        )
                        .await?;
                    outcome.created = Some(person_uid);
                }
                Action::UpdateUser {
                    user,
                    username,
                    name,
                    password,
                } => {
                    self.change_login(actor.as_deref(), &user, Some((username, name, password)))
                        .await?;
                }
                Action::DeleteUser { user } => {
                    self.change_login(actor.as_deref(), &user, None).await?;
                }
                Action::AssignRole { user, role } => {
                    self.require_permission(actor.as_deref(), "user:assign_role")
                        .await?;
                    let person = self.resolve(&user).await?;
                    let role_id = store::auth::role_by_name(&self.store.pool, &role)
                        .await?
                        .ok_or_else(|| {
                            EngineError::Consequence(format!("unknown role `{role}`"))
                        })?;
                    if !store::people::is_active(&self.store.pool, &person).await? {
                        return Err(EngineError::Consequence(format!(
                            "`{user}` is not an active Person"
                        )));
                    }
                    store::auth::set_user_role(&self.store.pool, &person, role_id).await?;
                }
                Action::SetPersonReadFilter { person, filter } => {
                    self.require_permission(actor.as_deref(), "user:update")
                        .await?;
                    let person_uid = self.resolve(&person).await?;
                    let record = store::records::get(&self.store.pool, &person_uid)
                        .await?
                        .ok_or_else(|| {
                            EngineError::Consequence(format!("no such Person `{person}`"))
                        })?;
                    if record.kind != "person" {
                        return Err(EngineError::Consequence(format!(
                            "`{person}` is a {} record, not a Person",
                            record.kind
                        )));
                    }
                    self.set_read_filter(&person_uid, filter.as_ref()).await?;
                }
                Action::SetRoleReadRules {
                    role,
                    rules,
                    expected_revision,
                } => {
                    self.require_permission(actor.as_deref(), "role:update")
                        .await?;
                    self.require_permission(actor.as_deref(), "permission:assign")
                        .await?;
                    if let Some(actor) = actor.as_deref()
                        && self.actor_user(actor).await?.role != "admin"
                    {
                        return Err(EngineError::Forbidden(
                            "Only admins may change role read rules".into(),
                        ));
                    }
                    if role == "admin" {
                        return Err(EngineError::Forbidden(
                            "Admin access cannot be restricted here".into(),
                        ));
                    }
                    rules.validate()?;
                    let mut pending = vec![&rules.allow, &rules.block];
                    while let Some(predicate) = pending.pop() {
                        match predicate {
                            protein::Predicate::All(children)
                            | protein::Predicate::Any(children) => pending.extend(children),
                            protein::Predicate::Not(child) => pending.push(child),
                            protein::Predicate::ConceptIn(uid) => {
                                if store::concepts::resolve(&self.store.pool, uid)
                                    .await?
                                    .is_none()
                                {
                                    return Err(EngineError::Consequence(
                                        "Choose an existing tag".into(),
                                    ));
                                }
                            }
                            _ => unreachable!(),
                        }
                    }
                    let id = store::auth::role_by_name(&self.store.pool, &role)
                        .await?
                        .ok_or_else(|| EngineError::Consequence("Unknown role".into()))?;
                    let previous = store::role_policies::get(&self.store.pool, id).await?;
                    if previous.as_ref().map_or(0, |row| row.revision) != expected_revision {
                        return Err(EngineError::Conflict {
                            code: "role_policy_changed",
                            message: "The role rules changed. Reload them before saving.".into(),
                        });
                    }
                    let mut policy = if let Some(value) = previous.and_then(|row| row.policy) {
                        serde_json::from_value::<protein::authority::RolePolicy>(value)
                            .map_err(|error| EngineError::Consequence(error.to_string()))?
                    } else {
                        protein::authority::RolePolicy {
                            read: protein::Predicate::All(vec![]),
                            grants: vec![],
                        }
                    };
                    policy.read = rules.predicate();
                    let value = serde_json::to_value(policy)
                        .map_err(|error| EngineError::Consequence(error.to_string()))?;
                    store::role_policies::set(&self.store.pool, id, &value, expected_revision)
                        .await?;
                }
                Action::SetPersonStanding {
                    person,
                    active,
                    note,
                } => {
                    self.require_permission(actor.as_deref(), "user:update")
                        .await?;
                    let person_uid = self.resolve(&person).await?;
                    let record = store::records::get(&self.store.pool, &person_uid)
                        .await?
                        .ok_or_else(|| {
                            EngineError::Consequence(format!("no such Person `{person}`"))
                        })?;
                    if record.kind != "person" {
                        return Err(EngineError::Consequence(format!(
                            "`{person}` is a {} record, not a Person",
                            record.kind
                        )));
                    }
                    if actor.as_deref() == Some(person_uid.as_str()) && !active {
                        return Err(EngineError::Consequence(
                            "you cannot deactivate yourself — ask another admin".into(),
                        ));
                    }
                    if !active {
                        let admins = store::auth::admins(&self.store.pool).await?;
                        if admins.iter().any(|admin| admin == &person_uid) {
                            let mut others = 0;
                            for admin in admins.iter().filter(|admin| *admin != &person_uid) {
                                if store::people::is_active(&self.store.pool, admin).await? {
                                    others += 1;
                                }
                            }
                            if others == 0 {
                                return Err(EngineError::Consequence(
                                "this is the last active admin — make someone else an admin first"
                                    .into(),
                            ));
                            }
                        }
                    }
                    if active {
                        store::people::reactivate(&self.store.pool, &person_uid).await?;
                    } else {
                        store::people::deactivate(
                            &self.store.pool,
                            &person_uid,
                            &now.to_rfc3339(),
                            note.as_deref(),
                        )
                        .await?;
                    }
                    outcome.created = Some(person_uid);
                }
                Action::GrantPermission { role, permission } => {
                    self.require_permission(actor.as_deref(), "permission:assign")
                        .await?;
                    let (subject, action_name) = permission.split_once(':').ok_or_else(|| {
                        EngineError::Consequence(format!("bad permission `{permission}`"))
                    })?;
                    let role_id = store::auth::role_by_name(&self.store.pool, &role)
                        .await?
                        .ok_or_else(|| {
                            EngineError::Consequence(format!("unknown role `{role}`"))
                        })?;
                    let permission_id =
                        store::auth::ensure_permission(&self.store.pool, subject, action_name)
                            .await?;
                    store::auth::grant(&self.store.pool, role_id, permission_id).await?;
                }
                Action::RevokePermission { role, permission } => {
                    self.require_permission(actor.as_deref(), "permission:assign")
                        .await?;
                    let (subject, action_name) = permission.split_once(':').ok_or_else(|| {
                        EngineError::Consequence(format!("bad permission `{permission}`"))
                    })?;
                    let role_id = store::auth::role_by_name(&self.store.pool, &role)
                        .await?
                        .ok_or_else(|| {
                            EngineError::Consequence(format!("unknown role `{role}`"))
                        })?;
                    let permission_id =
                        store::auth::ensure_permission(&self.store.pool, subject, action_name)
                            .await?;
                    store::auth::revoke(&self.store.pool, role_id, permission_id).await?;
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
