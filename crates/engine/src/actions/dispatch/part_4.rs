use super::*;

impl Engine {
    pub(super) fn dispatch_part_4(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
        verified_authorship: Option<VerifiedActionAuthorship>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = DispatchOutcome> + Send + '_>> {
        Box::pin(async move {
            let mut outcome = ActionOutcome::default();
            match action {
                Action::CreateTransferDraft {
                    request_id,
                    creator,
                    slug,
                    head,
                    agreement,
                    agreement_pct,
                    satiation,
                    parent,
                    source,
                    visibility,
                    max_proximity,
                    reserve_default,
                    require_confirmation,
                    default_place,
                    invitees,
                    promises,
                    dependencies,
                } => {
                    let mapped_creator = self.require_transfer_creator(actor.as_deref()).await?;
                    let visibility_actor = actor.clone();
                    let request_id = request_id.trim().to_string();
                    if request_id.is_empty() || request_id.chars().count() > 200 {
                        return Err(EngineError::Consequence(
                            "transfer request_id must contain 1 to 200 characters".into(),
                        ));
                    }
                    let creator_person = match (mapped_creator, creator) {
                        (Some(mapped), Some(token)) => {
                            let requested = self.resolve(token.trim()).await?;
                            if requested != mapped {
                                return Err(EngineError::Forbidden(
                                    "an authenticated transfer creator is derived from the session"
                                        .into(),
                                ));
                            }
                            mapped
                        }
                        (Some(mapped), None) => mapped,
                        (None, Some(token)) => {
                            let requested = self.resolve(token.trim()).await?;
                            let row = store::records::get(&self.store.pool, &requested)
                                .await?
                                .ok_or_else(|| EngineError::UnknownRecord(requested.clone()))?;
                            if row.kind != RecordKind::Person.as_str() {
                                return Err(EngineError::Consequence(
                                    "the local transfer creator must be a Person record".into(),
                                ));
                            }
                            requested
                        }
                        (None, None) => {
                            return Err(EngineError::Consequence(
                                "trusted local mode requires an explicit creator Person".into(),
                            ));
                        }
                    };
                    if let Some((replayed_transfer, _, replayed_action)) =
                        store::transfers::revision_for_request(&self.store.pool, &request_id)
                            .await?
                    {
                        let replayed_creator = store::transfers::creator_party_actor(
                            &self.store.pool,
                            &replayed_transfer,
                        )
                        .await?;
                        if replayed_action != "create-transfer-draft"
                            || replayed_creator.as_deref() != Some(creator_person.as_str())
                        {
                            return Err(EngineError::Conflict {
                                code: "transfer_request_id_conflict",
                                message:
                                    "transfer request id belongs to another creator or transfer"
                                        .into(),
                            });
                        }
                        outcome.created = Some(replayed_transfer);
                        return Ok(ControlFlow::Break(outcome));
                    }
                    let head = head.trim().to_string();
                    if head.is_empty() || head.chars().count() > 200 {
                        return Err(EngineError::Consequence(
                            "transfer title must contain 1 to 200 characters".into(),
                        ));
                    }
                    let slug = slug
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty());
                    if let Some(value) = slug.as_deref() {
                        if !nucleus::valid_slug(value) {
                            return Err(EngineError::Consequence(format!(
                                "invalid transfer slug `{value}`"
                            )));
                        }
                    }

                    match agreement {
                        nucleus::transfer::AgreementType::Percentage => {
                            if !agreement_pct.is_some_and(|pct| (1..=100).contains(&pct)) {
                                return Err(EngineError::Consequence(
                                    "percentage agreement requires a threshold from 1 to 100"
                                        .into(),
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
                    match visibility {
                        TransferVisibility::Proximity => {
                            if !max_proximity.is_some_and(|value| value > 0) {
                                return Err(EngineError::Consequence(
                                    "proximity visibility requires max_proximity greater than zero"
                                        .into(),
                                ));
                            }
                        }
                        _ if max_proximity.is_some() => {
                            return Err(EngineError::Consequence(
                                "max_proximity is valid only for proximity visibility".into(),
                            ));
                        }
                        _ => {}
                    }
                    if invitees.len() > 63 {
                        return Err(EngineError::Consequence(
                            "a transfer draft supports at most 63 invited people".into(),
                        ));
                    }
                    if promises.len() > 256 {
                        return Err(EngineError::Consequence(
                            "a transfer draft supports at most 256 promises".into(),
                        ));
                    }
                    let cell_reserve_default = self.transfer_reservation_cell_default().await?;
                    let effective_reserve_default = reserve_default.resolve(cell_reserve_default);

                    let parent_uid =
                        match parent.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                            Some(token) => {
                                let uid = self
                                    .resolve_transfer_visible_record(token, actor.as_deref())
                                    .await?;
                                let row = store::records::get(&self.store.pool, &uid)
                                    .await?
                                    .ok_or_else(|| EngineError::UnknownRecord(uid.clone()))?;
                                if row.kind != RecordKind::Transfer.as_str() {
                                    return Err(EngineError::Consequence(
                                        "a transfer parent must be another transfer".into(),
                                    ));
                                }
                                self.require_permission(actor.as_deref(), "transfer:update")
                                    .await?;
                                Some(uid)
                            }
                            None => None,
                        };
                    let source_uid =
                        match source.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                            Some(token) => Some(
                                self.resolve_transfer_visible_record(token, actor.as_deref())
                                    .await?,
                            ),
                            None => None,
                        };
                    if matches!(satiation, TransferSatiation::FirstCompletes)
                        && source_uid.is_none()
                    {
                        return Err(EngineError::Consequence(
                            "first_completes requires a source record shared with its siblings"
                                .into(),
                        ));
                    }

                    let mut seen_people = std::collections::HashSet::new();
                    seen_people.insert(creator_person.clone());
                    let mut invited_people = Vec::with_capacity(invitees.len());
                    for token in invitees {
                        let person_uid = self.resolve(token.trim()).await?;
                        let row = store::records::get(&self.store.pool, &person_uid)
                            .await?
                            .ok_or_else(|| EngineError::UnknownRecord(person_uid.clone()))?;
                        if row.kind != RecordKind::Person.as_str() {
                            return Err(EngineError::Consequence(format!(
                                "transfer invitee `{}` is not a Person record",
                                row.slug.as_deref().unwrap_or(&person_uid)
                            )));
                        }
                        if !seen_people.insert(person_uid.clone()) {
                            return Err(EngineError::Consequence(
                                "the creator or an invitee cannot appear twice".into(),
                            ));
                        }
                        invited_people.push(person_uid);
                    }

                    let mut draft_promises = Vec::with_capacity(promises.len());
                    for input in promises {
                        if !input.delta.is_finite() || input.delta == 0.0 {
                            return Err(EngineError::Consequence(
                                "every promise delta must be finite and non-zero".into(),
                            ));
                        }
                        if input.withdrawn {
                            return Err(EngineError::Consequence(
                                "new transfer promises cannot be withdrawn".into(),
                            ));
                        }
                        if let Some(uid) = input.uid.as_deref()
                            && store::misc::get_promise(&self.store.pool, uid)
                                .await?
                                .is_some()
                        {
                            return Err(EngineError::Conflict {
                                code: "transfer_promise_uid_conflict",
                                message: "a new Transfer promise uid is already in use".into(),
                            });
                        }
                        let item = self
                            .resolve_transfer_item(input.item, &creator_person)
                            .await?;
                        let (record_uid, concept_uid) = self
                            .resolve_transfer_item_source(
                                &input.record,
                                item.is_some(),
                                actor.as_deref(),
                            )
                            .await?;
                        let person_uid = if input.open {
                            if let Some(token) = input.party.as_deref() {
                                let submitted = self.resolve(token.trim()).await?;
                                if submitted != creator_person {
                                    return Err(EngineError::Consequence(
                                    "an OPEN promise is owned by the creating Person; its counterparty remains unnamed"
                                        .into(),
                                ));
                                }
                            }
                            Some(creator_person.clone())
                        } else {
                            let token = input.party.as_deref().ok_or_else(|| {
                                EngineError::Consequence(
                                    "a non-OPEN promise requires a reviewed Person".into(),
                                )
                            })?;
                            let person_uid = self.resolve(token.trim()).await?;
                            if !seen_people.contains(&person_uid) {
                                return Err(EngineError::Consequence(
                                "every promise Person must be the creator or a reviewed invitee"
                                    .into(),
                            ));
                            }
                            Some(person_uid)
                        };
                        if !input.open
                            && input.reuse_policy
                                == nucleus::transfer::OpenPromiseReusePolicy::Consume
                        {
                            return Err(EngineError::Consequence(
                                "reuse_policy applies only to an OPEN promise".into(),
                            ));
                        }
                        let unit_uid = self.resolve_concept_opt(input.unit).await?;
                        let (window_start, window_end) = normalize_transfer_window(
                            input.window_start,
                            input.window_end,
                            now,
                            None,
                        )?;
                        let location = normalize_transfer_place(input.place)?;
                        let condition = input
                            .condition
                            .map(|value| value.trim().to_string())
                            .filter(|value| !value.is_empty());
                        if let Some(value) = condition.as_deref() {
                            nucleus::expr::Expr::parse(value).map_err(|error| {
                                EngineError::Consequence(format!(
                                    "invalid promise condition: {error}"
                                ))
                            })?;
                        }
                        draft_promises.push(store::transfers::DraftPromise {
                            uid: Some(input.uid.unwrap_or_else(|| nucleus::new_uid("p"))),
                            item,
                            record_uid,
                            concept_uid,
                            unit_uid,
                            person_uid,
                            open: input.open,
                            delta: input.delta,
                            window_start,
                            window_end,
                            location,
                            condition,
                            reserve_from: input
                                .reserve_from
                                .unwrap_or(effective_reserve_default)
                                .resolve(effective_reserve_default)
                                .as_str()
                                .into(),
                            open_reuse_policy: input.reuse_policy,
                        });
                    }
                    let promise_uids = draft_promises
                        .iter()
                        .filter_map(|promise| promise.uid.clone())
                        .collect::<HashSet<_>>();
                    let dependencies = self
                        .resolve_transfer_dependencies(
                            None,
                            dependencies,
                            &promise_uids,
                            actor.as_deref(),
                        )
                        .await?;
                    if matches!(agreement, nucleus::transfer::AgreementType::Dependency)
                        && dependencies.is_empty()
                    {
                        return Err(EngineError::Consequence(
                            "dependency agreement requires at least one structured dependency"
                                .into(),
                        ));
                    }

                    let organ_uid = store::organs::local(&self.store.pool)
                        .await?
                        .map(|organ| organ.uid);
                    let signer = self
                        .transfer_person_signer(&creator_person, verified_authorship.as_ref())
                        .await?;
                    let created = store::transfers::create_draft(
                        &self.store.pool,
                        store::transfers::NewTransferDraft {
                            idempotency_key: request_id,
                            slug,
                            head,
                            agreement_type: agreement.as_str().into(),
                            agreement_pct: agreement_pct.map(i64::from),
                            satiation: satiation.as_option(),
                            parent_uid,
                            source_uid,
                            visibility: visibility.as_str().into(),
                            max_proximity: max_proximity.map(i64::from),
                            reserve_default: effective_reserve_default.as_str().into(),
                            require_confirmation,
                            default_place: normalize_transfer_place(default_place)?,
                            creator_person: creator_person.clone(),
                            invitees: invited_people,
                            promises: draft_promises,
                            dependencies,
                            organ_uid,
                            evidence_action: "create-transfer-draft".into(),
                            correction: None,
                            authorization_intent_uid: verified_authorship
                                .as_ref()
                                .map(|value| value.intent_uid.clone()),
                        },
                        now,
                        Some(creator_person),
                        visibility_actor,
                        |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                    )
                    .await?;
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
                    outcome.created = Some(created.transfer_uid);
                }
                Action::ReviseTransferPromise {
                    transfer,
                    promise,
                    expected_revision,
                    request_id,
                    terms,
                } => {
                    if transfer_phase_locked() {
                        return Err(EngineError::Conflict {
                        code: "transfer_draft_action_required",
                        message:
                            "use revise-transfer-draft so every public term is reviewed together"
                                .into(),
                    });
                    }
                    if !terms.delta.is_finite() || terms.delta == 0.0 {
                        return Err(EngineError::Consequence(
                            "promise delta must be finite and non-zero".into(),
                        ));
                    }
                    let transfer = self.resolve(&transfer).await?;
                    self.require_transfer_editor(&transfer, actor.as_deref())
                        .await?;
                    let promise = promise.trim().to_string();
                    let source = store::misc::get_promise(&self.store.pool, &promise)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(promise.clone()))?;
                    if source.transfer_uid.as_deref() != Some(transfer.as_str()) {
                        return Err(EngineError::Consequence(
                            "promise does not belong to the selected Transfer".into(),
                        ));
                    }
                    let record_uid = self.resolve(terms.record.trim()).await?;
                    if !self.may_read_record(actor.as_deref(), &record_uid).await? {
                        return Err(EngineError::Forbidden(
                            "Transfer source is unavailable".into(),
                        ));
                    }
                    let person_token = terms.party.as_deref().ok_or_else(|| {
                    EngineError::Consequence(
                        "a single-promise edit cannot turn a promise OPEN; use revise-transfer-draft"
                            .into(),
                    )
                })?;
                    let person_uid = self.resolve(person_token.trim()).await?;
                    let person = store::records::get(&self.store.pool, &person_uid)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(person_uid.clone()))?;
                    if person.kind != RecordKind::Person.as_str() {
                        return Err(EngineError::Consequence(
                            "promise Person must be a Person record".into(),
                        ));
                    }
                    let is_known =
                        store::transfers::party_for_actor(&self.store.pool, &transfer, &person_uid)
                            .await?
                            .is_some()
                            || store::transfers::invitations_for_transfer(
                                &self.store.pool,
                                &transfer,
                            )
                            .await?
                            .iter()
                            .any(|invitation| invitation.addressed_person_uid == person_uid);
                    if !is_known {
                        return Err(EngineError::Consequence(
                            "promise Person must be a participant or addressed invitee".into(),
                        ));
                    }
                    let window_end = terms
                        .window_end
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
                    let condition = terms
                        .condition
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty());
                    if let Some(value) = condition.as_deref() {
                        nucleus::expr::Expr::parse(value).map_err(|error| {
                            EngineError::Consequence(format!("invalid promise condition: {error}"))
                        })?;
                    }
                    let request_id = request_id.trim().to_string();
                    let signer = self.signer.lock().await.clone();
                    let fact_actor = actor
                        .clone()
                        .or_else(|| signer.as_ref().map(|value| value.actor_uid.clone()));
                    match store::transfers::revise_promise(
                        &self.store.pool,
                        store::transfers::PromiseRevisionInput {
                            transfer_uid: transfer,
                            item: self
                                .resolve_transfer_item(
                                    terms.item,
                                    fact_actor.as_deref().unwrap_or_default(),
                                )
                                .await?,
                            promise_uid: promise,
                            expected_revision,
                            idempotency_key: request_id,
                            record_uid,
                            person_uid,
                            delta: terms.delta,
                            window_end,
                            condition,
                            reserve_from: terms
                                .reserve_from
                                .map(|value| value.as_str().to_string())
                                .unwrap_or(source.reserve_from),
                        },
                        now,
                        fact_actor,
                        |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                    )
                    .await?
                    {
                        store::transfers::RevisionCommit::Committed { fact, .. } => {
                            outcome.facts = self.observe_committed_fact(fact, now).await?;
                        }
                        store::transfers::RevisionCommit::Replayed { .. } => {}
                        store::transfers::RevisionCommit::Stale { current_revision } => {
                            return Err(EngineError::Conflict {
                                code: "transfer_revision_stale",
                                message: format!(
                                    "expected transfer revision {expected_revision}, current revision is {current_revision}"
                                ),
                            });
                        }
                    }
                }
                Action::ReviseTransferDraft {
                    transfer,
                    expected_revision,
                    request_id,
                    draft,
                } => {
                    let transfer = self.resolve(&transfer).await?;
                    self.require_transfer_draft_creator(&transfer, actor.as_deref())
                        .await?;
                    if let Some((replayed_transfer, _, replayed_action)) =
                        store::transfers::revision_for_request(&self.store.pool, request_id.trim())
                            .await?
                    {
                        if replayed_transfer != transfer
                            || replayed_action != "revise-transfer-draft"
                        {
                            return Err(EngineError::Conflict {
                                code: "transfer_request_id_conflict",
                                message: "transfer request id belongs to another transfer".into(),
                            });
                        }
                        outcome.created = Some(transfer);
                        return Ok(ControlFlow::Break(outcome));
                    }
                    let current = store::transfers::get(&self.store.pool, &transfer)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(transfer.clone()))?;
                    if current.revision == 0 {
                        return Err(EngineError::Conflict {
                            code: "transfer_legacy_adoption_required",
                            message: "legacy revision-0 terms require explicit review and adoption"
                                .into(),
                        });
                    }
                    let creator_person = self
                        .resolve_transfer_draft_creator_person(
                            &transfer,
                            &draft.creator,
                            expected_revision,
                            actor.as_deref(),
                        )
                        .await?;
                    let mut input = self
                        .resolve_whole_transfer_draft(
                            transfer.clone(),
                            expected_revision,
                            request_id,
                            draft,
                            creator_person.clone(),
                            creator_person.clone(),
                            now,
                            false,
                            actor.as_deref(),
                        )
                        .await?;
                    input.authorization_intent_uid = verified_authorship
                        .as_ref()
                        .map(|value| value.intent_uid.clone());
                    let signer = self
                        .transfer_person_signer(&creator_person, verified_authorship.as_ref())
                        .await?;
                    let commit = store::transfers::revise_whole_draft(
                        &self.store.pool,
                        input,
                        now,
                        Some(creator_person),
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
                Action::AdoptTransferDraft {
                    transfer,
                    request_id,
                    draft,
                } => {
                    let transfer = self.resolve(&transfer).await?;
                    self.require_transfer_draft_creator(&transfer, actor.as_deref())
                        .await?;
                    if let Some((replayed_transfer, _, replayed_action)) =
                        store::transfers::revision_for_request(&self.store.pool, request_id.trim())
                            .await?
                    {
                        if replayed_transfer != transfer
                            || replayed_action != "adopt-legacy-transfer-draft"
                        {
                            return Err(EngineError::Conflict {
                                code: "transfer_request_id_conflict",
                                message: "transfer request id belongs to another transfer".into(),
                            });
                        }
                        outcome.created = Some(transfer);
                        return Ok(ControlFlow::Break(outcome));
                    }
                    let current = store::transfers::get(&self.store.pool, &transfer)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(transfer.clone()))?;
                    if current.revision != 0 {
                        return Err(EngineError::Conflict {
                            code: "transfer_already_revisioned",
                            message: format!(
                                "transfer is already sealed at revision {}",
                                current.revision
                            ),
                        });
                    }
                    let creator_person = self
                        .resolve_transfer_draft_creator_person(
                            &transfer,
                            &draft.creator,
                            0,
                            actor.as_deref(),
                        )
                        .await?;
                    let mut input = self
                        .resolve_whole_transfer_draft(
                            transfer.clone(),
                            0,
                            request_id,
                            draft,
                            creator_person.clone(),
                            creator_person.clone(),
                            now,
                            false,
                            actor.as_deref(),
                        )
                        .await?;
                    input.authorization_intent_uid = verified_authorship
                        .as_ref()
                        .map(|value| value.intent_uid.clone());
                    let signer = self
                        .transfer_person_signer(&creator_person, verified_authorship.as_ref())
                        .await?;
                    let commit = store::transfers::adopt_legacy_draft(
                        &self.store.pool,
                        input,
                        now,
                        Some(creator_person),
                        |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                    )
                    .await?;
                    self.apply_transfer_revision_commit(commit, 0, &transfer, &mut outcome)
                        .await?;
                }
                Action::AddressTransferInvitation {
                    transfer,
                    expected_revision,
                    request_id,
                    person,
                    expires_at,
                } => {
                    let transfer = self.resolve(&transfer).await?;
                    self.require_permission(actor.as_deref(), "transfer:update")
                        .await?;
                    let creator = self.transfer_creator_person(&transfer).await?;
                    let acting = self
                        .transfer_action_person(actor.as_deref(), None, Some(&creator))
                        .await?;
                    if acting != creator {
                        return Err(EngineError::Forbidden(
                            "only the transfer creator may address an invitation".into(),
                        ));
                    }
                    let addressed = self.resolve(person.trim()).await?;
                    if let Some(event) = store::transfers::invitation_event_for_request(
                        &self.store.pool,
                        request_id.trim(),
                    )
                    .await?
                    {
                        let replayed =
                            store::transfers::invitation(&self.store.pool, &event.invitation_uid)
                                .await?
                                .ok_or_else(|| {
                                    EngineError::Consequence("unknown transfer invitation".into())
                                })?;
                        if event.kind != "addressed"
                            || event.transfer_uid != transfer
                            || replayed.addressed_person_uid != addressed
                        {
                            return Err(transfer_request_id_conflict());
                        }
                        outcome.created = Some(event.invitation_uid);
                        return Ok(ControlFlow::Break(outcome));
                    }
                    reject_existing_transfer_revision_request(&self.store.pool, request_id.trim())
                        .await?;
                    let addressed_record = store::records::get(&self.store.pool, &addressed)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(addressed.clone()))?;
                    if addressed_record.kind != RecordKind::Person.as_str() {
                        return Err(EngineError::Consequence(
                            "transfer invitation addressee must be a Person record".into(),
                        ));
                    }
                    let signer = self
                        .transfer_person_signer(&acting, verified_authorship.as_ref())
                        .await?;
                    let commit = store::transfers::address_transfer_invitation(
                        &self.store.pool,
                        store::transfers::AddressTransferInvitationInput {
                            transfer_uid: transfer.clone(),
                            expected_revision,
                            idempotency_key: request_id.trim().to_string(),
                            addressed_person_uid: addressed,
                            invited_by_person_uid: acting,
                            expires_at: normalize_transfer_invitation_expiry(expires_at, now)?,
                        },
                        now,
                        |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                    )
                    .await?;
                    self.apply_transfer_invitation_commit(commit, expected_revision, &mut outcome)?;
                }
                Action::AcceptTransferInvitation {
                    invitation,
                    expected_revision,
                    request_id,
                    transfer,
                    person,
                } => {
                    let invitation_row =
                        store::transfers::invitation(&self.store.pool, &invitation)
                            .await?
                            .ok_or_else(|| {
                                EngineError::Consequence("unknown transfer invitation".into())
                            })?;
                    self.require_permission(actor.as_deref(), "transfer:update")
                        .await?;
                    if let Some(transfer) = transfer.as_deref() {
                        let transfer = self.resolve(transfer).await?;
                        if transfer != invitation_row.transfer_uid {
                            return Err(transfer_request_id_conflict());
                        }
                    }
                    let acting = self
                        .transfer_action_person(
                            actor.as_deref(),
                            person.as_deref(),
                            Some(&invitation_row.addressed_person_uid),
                        )
                        .await?;
                    if acting != invitation_row.addressed_person_uid {
                        return Err(EngineError::Forbidden(
                            "only the addressed Person may accept an invitation".into(),
                        ));
                    }
                    if let Some(created) = transfer_invitation_replay(
                        &self.store.pool,
                        request_id.trim(),
                        &invitation,
                        "accepted",
                    )
                    .await?
                    {
                        outcome.created = Some(created);
                        return Ok(ControlFlow::Break(outcome));
                    }
                    let signer = self
                        .transfer_person_signer(&acting, verified_authorship.as_ref())
                        .await?;
                    let commit = store::transfers::accept_transfer_invitation(
                        &self.store.pool,
                        store::transfers::InvitationTransitionInput {
                            invitation_uid: invitation,
                            expected_revision,
                            idempotency_key: request_id.trim().to_string(),
                            actor_person_uid: Some(acting),
                            expires_at: None,
                        },
                        now,
                        |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                    )
                    .await?;
                    self.apply_transfer_invitation_commit(commit, expected_revision, &mut outcome)?;
                }
                Action::RejectTransferInvitation {
                    invitation,
                    request_id,
                    transfer,
                    person,
                } => {
                    let invitation_row =
                        store::transfers::invitation(&self.store.pool, &invitation)
                            .await?
                            .ok_or_else(|| {
                                EngineError::Consequence("unknown transfer invitation".into())
                            })?;
                    self.require_permission(actor.as_deref(), "transfer:update")
                        .await?;
                    if let Some(transfer) = transfer.as_deref() {
                        let transfer = self.resolve(transfer).await?;
                        if transfer != invitation_row.transfer_uid {
                            return Err(transfer_request_id_conflict());
                        }
                    }
                    let acting = self
                        .transfer_action_person(
                            actor.as_deref(),
                            person.as_deref(),
                            Some(&invitation_row.addressed_person_uid),
                        )
                        .await?;
                    if acting != invitation_row.addressed_person_uid {
                        return Err(EngineError::Forbidden(
                            "only the addressed Person may reject an invitation".into(),
                        ));
                    }
                    store::offers::refuse(
                        &self.store.pool,
                        store::offers::OfferKind::Transfer,
                        &invitation_row.transfer_uid,
                        &invitation_row.addressed_person_uid,
                    )
                    .await?;
                    if let Some(created) = transfer_invitation_replay(
                        &self.store.pool,
                        request_id.trim(),
                        &invitation,
                        "rejected",
                    )
                    .await?
                    {
                        outcome.created = Some(created);
                        return Ok(ControlFlow::Break(outcome));
                    }
                    let signer = self
                        .transfer_person_signer(&acting, verified_authorship.as_ref())
                        .await?;
                    let commit = store::transfers::reject_transfer_invitation(
                        &self.store.pool,
                        store::transfers::InvitationTransitionInput {
                            invitation_uid: invitation,
                            expected_revision: 0,
                            idempotency_key: request_id.trim().to_string(),
                            actor_person_uid: Some(acting),
                            expires_at: None,
                        },
                        now,
                        |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                    )
                    .await?;
                    self.apply_transfer_invitation_commit(commit, 0, &mut outcome)?;
                }
                Action::WithdrawTransferInvitation {
                    invitation,
                    expected_revision,
                    request_id,
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
                            "only the transfer creator may change an invitation".into(),
                        ));
                    }
                    if let Some(created) = transfer_invitation_replay(
                        &self.store.pool,
                        request_id.trim(),
                        &invitation,
                        "withdrawn",
                    )
                    .await?
                    {
                        outcome.created = Some(created);
                        return Ok(ControlFlow::Break(outcome));
                    }
                    let signer = self
                        .transfer_person_signer(&acting, verified_authorship.as_ref())
                        .await?;
                    let input = store::transfers::InvitationTransitionInput {
                        invitation_uid: invitation,
                        expected_revision,
                        idempotency_key: request_id.trim().to_string(),
                        actor_person_uid: Some(acting),
                        expires_at: None,
                    };
                    let commit = store::transfers::withdraw_transfer_invitation(
                        &self.store.pool,
                        input,
                        now,
                        |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
                    )
                    .await?;
                    self.apply_transfer_invitation_commit(commit, expected_revision, &mut outcome)?;
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
