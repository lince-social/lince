// non-lean-yet

use super::super::{
    Action, ActionOutcome, Cause, ControlFlow, DateTime, DispatchOutcome, Engine, EngineError,
    NewFact, PromiseState, RecordKind, Utc, VerifiedActionAuthorship, invalid_dispatch,
    transfer_phase_locked, transfer_request_id_conflict,
};

impl Engine {
    pub(in super::super) async fn dispatch_transfers_agreements(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
        verified_authorship: Option<VerifiedActionAuthorship>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
            Action::CreateTransfer {
                slug,
                head,
                agreement,
                agreement_pct,
                satiation,
                mut source,
                reserve_default,
                require_confirmation,
            } => {
                if transfer_phase_locked() {
                    return Err(EngineError::Conflict {
                        code: "transfer_draft_action_required",
                        message: "use create-transfer-draft so the initial terms are atomic and revisioned".into(),
                    });
                }
                if head.trim().is_empty() || head.chars().count() > 200 {
                    return Err(EngineError::Consequence(
                        "transfer title must contain 1 to 200 characters".into(),
                    ));
                }
                let agreement_kind = nucleus::transfer::AgreementType::parse(&agreement)
                    .ok_or_else(|| {
                        EngineError::Consequence(format!(
                            "unknown transfer agreement `{agreement}`"
                        ))
                    })?;
                match agreement_kind {
                    nucleus::transfer::AgreementType::Percentage => {
                        if !agreement_pct.is_some_and(|pct| (1..=100).contains(&pct)) {
                            return Err(EngineError::Consequence(
                                "percentage agreement requires a threshold from 1 to 100".into(),
                            ));
                        }
                    }
                    _ if agreement_pct.is_some() => {
                        return Err(EngineError::Consequence(
                            "agreement_pct is valid only for percentage agreement".into(),
                        ));
                    }
                    _ => {}
                }
                if satiation
                    .as_deref()
                    .is_some_and(|value| value != "first_completes")
                {
                    return Err(EngineError::Consequence(
                        "satiation must be `first_completes` or omitted".into(),
                    ));
                }
                if satiation.as_deref() == Some("first_completes") && source.is_none() {
                    return Err(EngineError::Consequence(
                        "first_completes requires a source record".into(),
                    ));
                }
                if reserve_default.as_deref().is_some_and(|value| {
                    !matches!(value, "none" | "proposed" | "agreed" | "active")
                }) {
                    return Err(EngineError::Consequence(
                        "reserve_default must be none, proposed, agreed, active, or omitted".into(),
                    ));
                }
                if let Some(token) = source.as_deref() {
                    source = Some(self.resolve(token).await?);
                }
                let creator_person = self.require_transfer_creator(actor.as_deref()).await?;
                let visibility_actor = actor.clone();
                let transfer = store::transfers::create(
                    &self.store.pool,
                    store::transfers::NewTransfer {
                        slug: slug.as_deref(),
                        head: &head,
                        agreement_type: &agreement,
                        agreement_pct,
                        satiation: satiation.as_deref(),
                        source_uid: source.as_deref(),
                        reserve_default: reserve_default.as_deref(),
                        require_confirmation,
                    },
                )
                .await?;
                if let Some(person) = creator_person {
                    store::transfers::add_party(&self.store.pool, &transfer, &person).await?;
                }
                if let Some(subject) = visibility_actor.as_deref() {
                    store::visibility::grant(&self.store.pool, "actor", Some(subject), &transfer)
                        .await?;
                }
                outcome.facts = self
                    .append(
                        NewFact {
                            actor_uid: actor,
                            ..NewFact::quantity(
                                transfer.clone(),
                                store::exact::one(),
                                Cause::user_edit(),
                            )
                        },
                        now,
                    )
                    .await?;
                outcome.created = Some(transfer);
            }
            Action::AddParty {
                transfer,
                actor: person,
            } => {
                if transfer_phase_locked() {
                    return Err(EngineError::Conflict {
                        code: "transfer_invitation_required",
                        message:
                            "a Person becomes a transfer party only by accepting an invitation"
                                .into(),
                    });
                }
                let transfer = self.resolve(&transfer).await?;
                self.require_transfer_editor(&transfer, actor.as_deref())
                    .await?;
                let person = self.resolve(&person).await?;
                let actor_record = store::records::get(&self.store.pool, &person)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(person.clone()))?;
                if actor_record.kind != RecordKind::Person.as_str() {
                    return Err(EngineError::Consequence(
                        "transfer parties must be person records".into(),
                    ));
                }
                let party =
                    store::transfers::add_party(&self.store.pool, &transfer, &person).await?;
                outcome.facts = self
                    .annotate(
                        transfer,
                        actor,
                        serde_json::json!({
                            "action": "add-party",
                            "party": party,
                            "person": person,
                        }),
                        now,
                    )
                    .await?;
                outcome.created = Some(party);
            }
            Action::AddPromiseToTransfer {
                transfer,
                record,
                delta,
                party,
                window_end,
                condition,
            } => {
                if transfer_phase_locked() {
                    return Err(EngineError::Conflict {
                        code: "transfer_phase_1_not_available",
                        message: "adding a promise requires the revision-safe draft edit workflow"
                            .into(),
                    });
                }
                if !delta.is_finite() || delta == 0.0 {
                    return Err(EngineError::Consequence(
                        "promise delta must be finite and non-zero".into(),
                    ));
                }
                let transfer = self.resolve(&transfer).await?;
                self.require_transfer_editor(&transfer, actor.as_deref())
                    .await?;
                let record = self.resolve(&record).await?;
                let party = self.resolve(&party).await?;
                let party_record = store::records::get(&self.store.pool, &party)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(party.clone()))?;
                if party_record.kind != RecordKind::Person.as_str() {
                    return Err(EngineError::Consequence(
                        "promise parties must be person records".into(),
                    ));
                }
                let window_end = window_end
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty());
                if let Some(value) = window_end.as_deref() {
                    let parsed = DateTime::parse_from_rfc3339(value).map_err(|_| {
                        EngineError::Consequence(format!(
                            "promise window `{value}` must be an RFC3339 date and time"
                        ))
                    })?;
                    if parsed.with_timezone(&Utc) <= now {
                        return Err(EngineError::Consequence(
                            "promise window must end in the future".into(),
                        ));
                    }
                }
                let condition = condition
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty());
                if let Some(value) = condition.as_deref() {
                    nucleus::expr::Expr::parse(value).map_err(|error| {
                        EngineError::Consequence(format!("invalid promise condition: {error}"))
                    })?;
                }
                let promise = store::misc::insert_promise(
                    &self.store.pool,
                    store::misc::NewPromise {
                        record_uid: Some(record.clone()),
                        delta,
                        window_end,
                        party_uid: Some(party.clone()),
                        state: Some(PromiseState::Proposed),
                        condition,
                        transfer_uid: Some(transfer.clone()),
                        ..Default::default()
                    },
                )
                .await?;
                outcome.facts = self
                    .annotate_many(
                        vec![transfer, record],
                        actor,
                        serde_json::json!({
                            "action": "add-promise-to-transfer",
                            "promise": promise,
                            "party": party,
                            "delta": delta,
                        }),
                        now,
                    )
                    .await?;
                outcome.created = Some(promise);
            }
            Action::AgreeTransfer { .. } => {
                return Err(EngineError::Conflict {
                    code: "signed_transfer_agreement_action_required",
                    message: "use set-transfer-agreement-level with expected_revision, request_id, and the acting Person"
                        .into(),
                });
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
                        "agreement may be authored only by an accepted Transfer participant".into(),
                    ));
                }
                if store::transfers::revision_for_request(&self.store.pool, &request_id)
                    .await?
                    .is_some()
                    || store::transfers::invitation_event_for_request(&self.store.pool, &request_id)
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
                        current_revision, ..
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
                            "transfer_agreement_source_changed" | "transfer_state_source_changed"
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
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
