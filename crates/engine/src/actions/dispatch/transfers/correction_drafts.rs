// non-lean-yet

use super::super::{
    Action, ActionOutcome, ControlFlow, DateTime, DispatchOutcome, Engine, EngineError, Utc,
    VerifiedActionAuthorship, invalid_dispatch, transfer_request_id_conflict,
    transfer_settlement_values_match,
};

impl Engine {
    pub(in super::super) async fn dispatch_transfers_correction_drafts(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
        verified_authorship: Option<VerifiedActionAuthorship>,
    ) -> DispatchOutcome {
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
                let occurrence_row = store::transfers::occurrence(&self.store.pool, &occurrence)
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
                let source_promise =
                    store::transfers::promises_of(&self.store.pool, &occurrence_row.transfer_uid)
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
                let progress =
                    store::transfers::occurrence_settlement_progress(&self.store.pool, &occurrence)
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
                            message: "reversal must use the exact canonical occurrence quantity"
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
                            message: "reviewed remainder is no longer the exact partial remainder"
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
                        public_history: None,
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
                            open_reuse_policy: nucleus::transfer::OpenPromiseReusePolicy::Duplicate,
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
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
