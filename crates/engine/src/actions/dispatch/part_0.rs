use super::*;

impl Engine {
    pub(super) fn dispatch_part_0(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
        verified_authorship: Option<VerifiedActionAuthorship>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = DispatchOutcome> + Send + '_>> {
        Box::pin(async move {
            let mut outcome = ActionOutcome::default();
            match action {
                Action::Social { request } => {
                    outcome = self.social_command(request, actor.as_deref(), now).await?;
                }
                Action::ChangeRecord { request } => {
                    outcome = self.change_record(request, actor.as_deref()).await?;
                }
                Action::CreateRecordWithTags {
                    head,
                    body,
                    quantity,
                    tags,
                } => {
                    outcome = self
                        .create_tagged_record(head, body, quantity, tags, actor, now)
                        .await?;
                }
                Action::CreateRecordDraft { draft } => {
                    outcome = self.create_record_draft(draft, actor, now).await?;
                }
                Action::CreateCustomComponent { head, body } => {
                    outcome = self.create_custom_component(head, body, actor, now).await?;
                }
                Action::CreateRecord {
                    slug,
                    kind,
                    head,
                    body,
                    quantity,
                } => {
                    let rec = store::records::create(
                        &self.store.pool,
                        store::records::NewRecord {
                            slug: slug.as_deref(),
                            kind,
                            head: &head,
                            body: &body,
                            quantity: store::exact::zero(),
                        },
                    )
                    .await?;
                    outcome.facts = self
                        .append(
                            NewFact {
                                actor_uid: actor,
                                ..NewFact::quantity_f64(
                                    rec.uid.clone(),
                                    quantity,
                                    Cause::user_edit(),
                                )
                            },
                            now,
                        )
                        .await?;
                    outcome.created = Some(rec.uid);
                }
                Action::PreviewAreaTransition {
                    target,
                    changes,
                    constraints,
                } => {
                    outcome.data = Some(
                        serde_json::to_value(
                            self.preview_area_transition(target, changes, constraints)
                                .await?,
                        )
                        .map_err(EngineError::Json)?,
                    );
                }
                Action::ApplyAreaTransition {
                    request_id,
                    preview,
                } => {
                    outcome = self
                        .apply_area_transition(request_id, preview, actor, now)
                        .await?;
                }
                Action::SetQuantityExact { target, amount } => {
                    outcome = self
                        .change_record(
                            crate::record_change::Request {
                                id: nucleus::new_uid("op"),
                                record_uid: self.resolve(&target).await?,
                                mutation: crate::record_change::Mutation::Quantity {
                                    value: amount,
                                },
                            },
                            actor.as_deref(),
                        )
                        .await?;
                }
                Action::PresentComponent { target, component } => {
                    outcome = self
                        .present_component(target, component, actor.as_deref())
                        .await?;
                }
                Action::ActivateFiote {
                    target,
                    value,
                    request_id,
                } => {
                    outcome = self
                        .activate_fiote(
                            target,
                            value,
                            request_id,
                            serde_json::json!({"kind":"manual"}),
                            actor.as_deref(),
                        )
                        .await?;
                }
                Action::InspectFioteActivations { target } => {
                    let target = self.resolve(&target).await?;
                    self.refuse_unreadable(actor.as_deref(), std::slice::from_ref(&target))
                        .await?;
                    outcome.data = Some(
                        serde_json::json!({"activations":self.fiote_activations(&target).await?,"limit":256}),
                    );
                }
                Action::ReportFioteChild {
                    parent,
                    thread,
                    task,
                    state,
                } => {
                    let _guard = self.fiote_config_lock.lock().await;
                    let parent = self.resolve(&parent).await?;
                    let thread = self.resolve(&thread).await?;
                    self.refuse_unreadable(actor.as_deref(), &[parent.clone(), thread.clone()])
                        .await?;
                    if parent == thread
                        || task.trim().is_empty()
                        || task.len() > 4096
                        || !["working", "waiting", "stopped", "finished", "interrupted"]
                            .contains(&state.as_str())
                    {
                        return Err(EngineError::Consequence(
                            "Choose distinct parent/child threads, a task and a valid child state"
                                .into(),
                        ));
                    }
                    for uid in [&parent, &thread] {
                        if store::records::get(&self.store.pool, uid)
                            .await?
                            .is_none_or(|row| row.kind != "thread")
                        {
                            return Err(EngineError::Consequence(
                                "Child reporting requires conversation threads".into(),
                            ));
                        }
                    }
                    let mut ancestor = parent.clone();
                    let mut depth = 0;
                    while let Some(value) = store::records::get_extension(
                        &self.store.pool,
                        &ancestor,
                        "lince.fiote-child",
                    )
                    .await?
                    {
                        depth += 1;
                        if depth > 32 || value["parent"] == thread {
                            return Err(EngineError::Consequence(
                                "Child sessions cannot form a loop or exceed 32 ancestors".into(),
                            ));
                        }
                        let Some(next) = value["parent"].as_str() else {
                            break;
                        };
                        ancestor = next.into();
                    }
                    store::records::set_extension(
                        &self.store.pool,
                        &thread,
                        "lince.fiote-child",
                        &serde_json::json!({"parent":parent,"task":task,"state":state}),
                    )
                    .await?;
                    outcome.data =
                        Some(serde_json::json!({"reported":thread,"starts_child":false}));
                }
                Action::DiscardTransferDraft {
                    transfer,
                    person,
                    expected_revision,
                    request_id,
                } => {
                    outcome = self
                        .discard_transfer_draft(
                            transfer,
                            person,
                            expected_revision,
                            request_id,
                            actor.as_deref(),
                            verified_authorship.as_ref(),
                        )
                        .await?;
                }
                Action::SetQuantity { target, value } => {
                    if !value.is_finite() {
                        return Err(EngineError::Consequence("Quantity must be finite".into()));
                    }
                    outcome = self
                        .change_record(
                            crate::record_change::Request {
                                id: nucleus::new_uid("op"),
                                record_uid: self.resolve(&target).await?,
                                mutation: crate::record_change::Mutation::Quantity {
                                    value: store::exact::from_f64(value).to_string(),
                                },
                            },
                            actor.as_deref(),
                        )
                        .await?;
                }
                Action::TransitionRecord {
                    subject,
                    retract,
                    assert,
                    quantity,
                } => {
                    let subject_uid = self.resolve(&subject).await?;
                    self.reject_direct_transfer_record_mutation(&subject_uid)
                        .await?;
                    let mut retract_uids = Vec::new();
                    for concept in retract {
                        let uid = store::concepts::resolve(&self.store.pool, &concept)
                            .await?
                            .ok_or_else(|| EngineError::UnknownRecord(concept))?;
                        if !retract_uids.contains(&uid) {
                            retract_uids.push(uid);
                        }
                    }
                    let mut assert_uids = Vec::new();
                    for concept in assert {
                        let uid = store::concepts::resolve(&self.store.pool, &concept)
                            .await?
                            .ok_or_else(|| EngineError::UnknownRecord(concept))?;
                        if !assert_uids.contains(&uid) {
                            assert_uids.push(uid);
                        }
                    }
                    retract_uids.retain(|uid| !assert_uids.contains(uid));

                    let signer = self.signer.lock().await.clone();
                    let mut tx = store::write_tx(&self.store.pool).await?;
                    store::assertions::transition_unary(
                        &mut tx,
                        &subject_uid,
                        &retract_uids,
                        &assert_uids,
                        actor.as_deref(),
                    )
                    .await?;
                    let fact = if let Some(value) = quantity {
                        let current =
                            store::records::quantity_in_transaction(&mut tx, &subject_uid)
                                .await?
                                .ok_or_else(|| EngineError::UnknownRecord(subject_uid.clone()))?;
                        let target = store::exact::from_f64(value);
                        if target == current {
                            None
                        } else {
                            crate::append::append_one_in_transaction(
                                &mut tx,
                                NewFact::quantity(
                                    subject_uid.clone(),
                                    store::exact::difference(target, current)?,
                                    Cause::user_edit(),
                                ),
                                now,
                                signer.as_ref(),
                            )
                            .await?
                        }
                    } else {
                        None
                    };
                    tx.commit().await?;
                    if let Some(fact) = fact {
                        outcome.facts = self.observe_committed_fact(fact, now).await?;
                    }
                }
                Action::CaptureEntry {
                    target,
                    amount,
                    concept,
                    note,
                    at,
                    request_id,
                } => {
                    let request_id = request_id
                        .map(|id| id.trim().to_string())
                        .filter(|id| !id.is_empty())
                        .unwrap_or_else(|| nucleus::new_uid("req"));
                    if store::entries::replayed(&self.store.pool, &request_id)
                        .await?
                        .is_some()
                    {
                        return Ok(ControlFlow::Break(outcome));
                    }
                    let uid = self.resolve(&target).await?;
                    self.reject_direct_transfer_record_mutation(&uid).await?;
                    let delta =
                        nucleus::DecimalValue::parse_inferred(amount.trim()).map_err(|_| {
                            EngineError::Conflict {
                                code: "entry_amount_invalid",
                                message: format!("`{amount}` is not an exact decimal amount"),
                            }
                        })?;
                    let occurred_at = match at.as_deref() {
                        Some(text) => Some(
                            chrono::DateTime::parse_from_rfc3339(text)
                                .map_err(|_| EngineError::Conflict {
                                    code: "entry_at_invalid",
                                    message: format!("`{text}` is not an RFC3339 instant"),
                                })?
                                .with_timezone(&Utc),
                        ),
                        None => None,
                    };
                    let concept_uid = self.resolve_concept_opt(concept).await?;
                    outcome.facts = self
                        .append(
                            NewFact {
                                actor_uid: actor,
                                at: occurred_at,
                                ..NewFact::quantity(uid, delta, Cause::user_edit())
                            },
                            now,
                        )
                        .await?;
                    if let Some(fact) = outcome.facts.first() {
                        store::ledger::classify_fact(
                            &self.store.pool,
                            &fact.uid,
                            concept_uid.as_deref(),
                            fact.actor_uid.as_deref(),
                            note.as_deref(),
                        )
                        .await?;
                        let commit = store::entries::create(
                            &self.store.pool,
                            store::entries::NewEntry {
                                record_uid: &fact.record_uid,
                                amount: delta,
                                note: note.as_deref(),
                                occurred_at: fact.at,
                                fact_uid: &fact.uid,
                                request_id: &request_id,
                                actor_uid: fact.actor_uid.as_deref(),
                            },
                            now,
                        )
                        .await?;
                        outcome.created = Some(commit.entry().uid.clone());
                    }
                }
                Action::CreateFrequency {
                    slug,
                    head,
                    every,
                    anchor_at,
                    request_id,
                } => {
                    let request_id = request_id
                        .map(|id| id.trim().to_string())
                        .filter(|id| !id.is_empty())
                        .unwrap_or_else(|| nucleus::new_uid("req"));
                    let anchor = parse_optional_instant(anchor_at.as_deref())?.unwrap_or(now);
                    let head = head.unwrap_or_default();
                    let frequency = store::frequency::create(
                        &self.store.pool,
                        store::frequency::NewFrequency {
                            slug: &slug,
                            head: &head,
                            every,
                            anchor_at: anchor,
                            request_id: &request_id,
                            actor_uid: actor.as_deref(),
                        },
                        now,
                    )
                    .await
                    .map_err(|error| EngineError::Conflict {
                        code: "frequency_invalid",
                        message: error.to_string(),
                    })?;
                    outcome.created = Some(frequency.uid);
                }
                Action::DeleteFrequency { frequency } => {
                    let found = store::frequency::resolve(&self.store.pool, &frequency)
                        .await?
                        .ok_or_else(|| EngineError::Conflict {
                            code: "frequency_unknown",
                            message: format!("nothing here is called {frequency}"),
                        })?;
                    store::frequency::delete(&self.store.pool, &found.uid)
                        .await
                        .map_err(|error| EngineError::Conflict {
                            code: "frequency_in_use",
                            message: error.to_string(),
                        })?;
                }
                Action::PreviewKarmaReading { source } => {
                    outcome.data = Some(
                        Box::pin(self.preview_karma_reading(&source, actor.as_deref(), now))
                            .await?,
                    );
                }
                Action::PreviewKarmaProposal { .. } => {
                    return Err(crate::karma_preview::invalid(
                        "Preview must use the isolated Simulation service",
                    ));
                }
                Action::InspectKarmaRuleHistory { rule, limit } => {
                    outcome.data = Some(
                        self.inspect_karma_history(&rule, limit, actor.as_deref())
                            .await?,
                    );
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
                Action::PreviewKarmaHabit { input } => {
                    outcome.data = Some(
                        serde_json::to_value(
                            self.preview_karma_habit(input, actor.as_deref(), now)
                                .await?,
                        )
                        .map_err(EngineError::Json)?,
                    );
                }
                Action::ImportKarmaHabit {
                    input,
                    expected_preview,
                    request_id,
                } => {
                    let imported = Box::pin(self.import_karma_habit(
                        input,
                        expected_preview,
                        request_id,
                        actor.as_deref(),
                        now,
                    ))
                    .await?;
                    outcome.created = imported
                        .objects
                        .iter()
                        .find(|object| object.kind == crate::karma_habits::Kind::Record)
                        .map(|object| object.uid.clone());
                    outcome.data = Some(serde_json::to_value(imported).map_err(EngineError::Json)?);
                }
                Action::SaveKarmaSchedule {
                    schedule,
                    expected_revision,
                    name,
                    boundaries,
                    request_id,
                } => {
                    outcome = Box::pin(self.save_karma_schedule(
                        schedule,
                        expected_revision,
                        name,
                        boundaries,
                        request_id,
                        actor.as_deref(),
                        now,
                    ))
                    .await?;
                    self.notify_karma_deadline_change();
                }
                Action::InspectKarmaSchedules { schedule } => {
                    outcome.data = Some(
                        self.inspect_karma_schedules(schedule.as_deref(), actor.as_deref())
                            .await?,
                    );
                }
                Action::PreviewKarmaScheduleDates {
                    date,
                    timezone,
                    gap,
                    fold,
                } => {
                    outcome.data = Some(
                        self.preview_schedule_dates(date, timezone, gap, fold, actor.as_deref())
                            .await?,
                    );
                }
                Action::CancelKarmaSchedule {
                    schedule,
                    expected_revision,
                    request_id,
                } => {
                    outcome = Box::pin(self.cancel_karma_schedule(
                        schedule,
                        expected_revision,
                        request_id,
                        actor.as_deref(),
                        now,
                    ))
                    .await?;
                    self.notify_karma_deadline_change();
                }
                Action::RetryKarmaSchedule {
                    schedule,
                    expected_revision,
                    boundary,
                    request_id,
                } => {
                    outcome = Box::pin(self.retry_karma_schedule(
                        schedule,
                        expected_revision,
                        boundary,
                        request_id,
                        actor.as_deref(),
                        now,
                    ))
                    .await?;
                    self.notify_karma_deadline_change();
                }
                Action::SaveKarmaRule {
                    identity,
                    rule,
                    expected_revision,
                    fields,
                    request_id,
                } => {
                    outcome = Box::pin(self.save_karma_rule(
                        rule,
                        expected_revision,
                        fields,
                        identity,
                        request_id,
                        actor.as_deref(),
                        now,
                    ))
                    .await?;
                }
                Action::ReviseKarmaField {
                    field,
                    expected_revision,
                    source,
                    request_id,
                } => {
                    outcome = Box::pin(self.revise_karma_field(
                        field,
                        expected_revision,
                        source,
                        request_id,
                        actor.as_deref(),
                        now,
                    ))
                    .await?;
                }
                Action::CreateRecurrence {
                    target,
                    consequences,
                    condition,
                    gate,
                    carry,
                    note,
                    cadence,
                    anchor_at,
                    request_id,
                } => {
                    let request_id = request_id
                        .map(|id| id.trim().to_string())
                        .filter(|id| !id.is_empty())
                        .unwrap_or_else(|| nucleus::new_uid("req"));
                    let declared = self.resolve_consequences(consequences).await?;
                    let uid = match self
                        .transfer_rule_anchor(&declared, actor.as_deref())
                        .await?
                    {
                        Some(anchor) => {
                            if self.resolve_karma_transfer(&target).await?.as_str()
                                != declared
                                    .iter()
                                    .find_map(nucleus::karma::Consequence::transfer_target)
                                    .unwrap_or("")
                            {
                                return Err(EngineError::Consequence(
                                    "Rule target differs from its Transfer consequence".into(),
                                ));
                            }
                            anchor
                        }
                        None => {
                            let uid = self.resolve(&target).await?;
                            self.authorize_rule_target(&uid, actor.as_deref()).await?;
                            uid
                        }
                    };
                    let condition = self.canonical_condition(condition).await?;
                    let mut declared_condition = parse_rule_condition(condition, gate, carry)?;
                    if let Some(condition) = &mut declared_condition {
                        condition.bindings = store::karma_bindings::resolve(
                            &self.store.pool,
                            &condition.source,
                            &[],
                        )
                        .await?;
                    }
                    let mut effect_bindings = declared_condition
                        .as_ref()
                        .map_or_else(Vec::new, |condition| condition.bindings.clone());
                    let declared = self
                        .bind_transfer_effects(
                            declared.as_slice().to_vec(),
                            &mut effect_bindings,
                            &[],
                        )
                        .await?;
                    if let Some(condition) = &mut declared_condition {
                        condition.bindings = effect_bindings;
                    }
                    Box::pin(self.validate_automatic_rule(
                        &declared,
                        declared_condition.as_ref(),
                        actor.as_deref(),
                    ))
                    .await?;
                    let anchor = parse_optional_instant(anchor_at.as_deref())?.unwrap_or(now);
                    let commit = store::recurrence::create(
                        &self.store.pool,
                        store::recurrence::NewRecurrence {
                            record_uid: &uid,
                            consequences: declared,
                            condition: declared_condition,
                            note: note.as_deref(),
                            cadence,
                            anchor_at: anchor,
                            request_id: &request_id,
                            actor_uid: actor.as_deref(),
                        },
                        now,
                    )
                    .await
                    .map_err(|error| EngineError::Conflict {
                        code: "recurrence_invalid",
                        message: error.to_string(),
                    })?;
                    outcome.created = Some(commit.rule().uid.clone());
                }
                Action::ReviseRecurrence {
                    recurrence,
                    expected_revision,
                    request_id,
                    consequences,
                    condition,
                    gate,
                    carry,
                    note,
                    cadence,
                    anchor_at,
                } => {
                    let current = store::recurrence::get(&self.store.pool, &recurrence)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(recurrence.clone()))?;
                    let declared = self.resolve_consequences(consequences).await?;
                    self.authorize_karma_rule(&current, actor.as_deref())
                        .await?;
                    if let Some(anchor) = self
                        .transfer_rule_anchor(&declared, actor.as_deref())
                        .await?
                        && anchor != current.record_uid
                    {
                        return Err(EngineError::Consequence(
                            "Use the Rule editor to select another Transfer target".into(),
                        ));
                    }
                    let condition = self.canonical_condition(condition).await?;
                    let mut declared_condition = parse_rule_condition(condition, gate, carry)?;
                    if let Some(condition) = &mut declared_condition {
                        condition.bindings = store::karma_bindings::resolve(
                            &self.store.pool,
                            &condition.source,
                            current
                                .condition
                                .as_ref()
                                .map_or(&[], |old| old.bindings.as_slice()),
                        )
                        .await?;
                    }
                    let mut effect_bindings = declared_condition
                        .as_ref()
                        .map_or_else(Vec::new, |condition| condition.bindings.clone());
                    let declared = self
                        .bind_transfer_effects(
                            declared.as_slice().to_vec(),
                            &mut effect_bindings,
                            current
                                .condition
                                .as_ref()
                                .map_or(&[], |condition| condition.bindings.as_slice()),
                        )
                        .await?;
                    if let Some(condition) = &mut declared_condition {
                        condition.bindings = effect_bindings;
                    }
                    Box::pin(self.validate_automatic_rule(
                        &declared,
                        declared_condition.as_ref(),
                        actor.as_deref(),
                    ))
                    .await?;
                    let anchor = match parse_optional_instant(anchor_at.as_deref())? {
                        Some(value) => value,
                        None => parse_instant_field(&current.anchor_at)?,
                    };
                    store::recurrence::revise(
                        &self.store.pool,
                        store::recurrence::ReviseRecurrence {
                            recurrence_uid: &recurrence,
                            expected_revision,
                            consequences: declared,
                            condition: declared_condition,
                            note: note.as_deref(),
                            cadence,
                            anchor_at: anchor,
                            request_id: &request_id,
                            actor_uid: actor.as_deref(),
                        },
                        now,
                    )
                    .await
                    .map_err(|error| EngineError::Conflict {
                        code: "recurrence_revision_stale",
                        message: error.to_string(),
                    })?;
                }
                Action::DeleteRecurrence { recurrence } => {
                    let rule = store::recurrence::get(&self.store.pool, &recurrence)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(recurrence.clone()))?;
                    self.refuse_unreadable_karma_inputs(
                        actor.as_deref(),
                        &[crate::karma_transfer_effects::target(&rule).into()],
                    )
                    .await?;
                    if store::karma_schedules::for_rule(&self.store.pool, &rule.uid)
                        .await?
                        .is_some()
                    {
                        return Err(EngineError::Conflict {
                        code: "karma_schedule_retained_rule",
                        message: "Cancel the scheduled change to stop its remaining work. Its Rule and outcomes are kept in history.".into(),
                    });
                    }
                    store::recurrence::delete(&self.store.pool, &rule.uid).await?;
                }
                Action::SetRecurrencePaused {
                    recurrence,
                    expected_revision,
                    request_id,
                    paused,
                } => {
                    store::recurrence::set_state(
                        &self.store.pool,
                        &recurrence,
                        expected_revision,
                        paused,
                        &request_id,
                        actor.as_deref(),
                        now,
                    )
                    .await
                    .map_err(|error| EngineError::Conflict {
                        code: "recurrence_revision_stale",
                        message: error.to_string(),
                    })?;
                }
                Action::ApplyRecurrenceOccurrence {
                    recurrence,
                    due_at,
                    amount,
                    note,
                } => {
                    let mut rule = store::recurrence::get(&self.store.pool, &recurrence)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(recurrence.clone()))?;
                    if let Some(amount) = amount {
                        let value = nucleus::DecimalValue::parse_inferred(&amount)
                            .map_err(|error| EngineError::Consequence(error.to_string()))?;
                        rule.consequences = nucleus::karma::Consequences::new(
                            rule.consequences
                                .iter()
                                .cloned()
                                .map(|consequence| match consequence {
                                    nucleus::karma::Consequence::CaptureEntry {
                                        concept, ..
                                    } => nucleus::karma::Consequence::CaptureEntry {
                                        amount: value,
                                        concept,
                                    },
                                    other => other,
                                })
                                .collect(),
                        )
                        .map_err(|error| EngineError::Consequence(error.to_string()))?;
                    }
                    if let Some(note) = note {
                        rule.note = Some(note);
                    }
                    outcome.facts = Box::pin(self.apply_rule_occurrence(
                        &rule,
                        parse_instant_field(&due_at)?,
                        now,
                    ))
                    .await?;
                }
                Action::SkipRecurrenceOccurrence {
                    recurrence,
                    due_at,
                    note,
                } => {
                    let due = parse_instant_field(&due_at)?;
                    store::recurrence::skip(
                        &self.store.pool,
                        &recurrence,
                        due,
                        note.as_deref(),
                        actor.as_deref(),
                        now,
                    )
                    .await?;
                }
                Action::UnskipRecurrenceOccurrence { recurrence, due_at } => {
                    let due = parse_instant_field(&due_at)?;
                    store::recurrence::unskip(&self.store.pool, &recurrence, due).await?;
                }
                Action::ReviseEntry {
                    entry,
                    expected_revision,
                    request_id,
                    amount,
                    note,
                    at,
                } => {
                    if store::entries::replayed(&self.store.pool, &request_id)
                        .await?
                        .is_some()
                    {
                        return Ok(ControlFlow::Break(outcome));
                    }
                    let current = store::entries::get(&self.store.pool, &entry)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(entry.clone()))?;
                    if current.is_void() {
                        return Err(EngineError::Conflict {
                            code: "entry_void",
                            message: "a voided entry cannot be revised".to_string(),
                        });
                    }
                    if current.revision != expected_revision {
                        return Err(EngineError::Conflict {
                            code: "entry_revision_stale",
                            message: "this entry was changed by someone else".to_string(),
                        });
                    }
                    let delta =
                        nucleus::DecimalValue::parse_inferred(amount.trim()).map_err(|_| {
                            EngineError::Conflict {
                                code: "entry_amount_invalid",
                                message: format!("`{amount}` is not an exact decimal amount"),
                            }
                        })?;
                    let occurred_at = match at.as_deref() {
                        Some(text) => chrono::DateTime::parse_from_rfc3339(text)
                            .map_err(|_| EngineError::Conflict {
                                code: "entry_at_invalid",
                                message: format!("`{text}` is not an RFC3339 instant"),
                            })?
                            .with_timezone(&Utc),
                        None => chrono::DateTime::parse_from_rfc3339(&current.occurred_at)
                            .map_err(|_| EngineError::Conflict {
                                code: "entry_at_invalid",
                                message: "stored entry instant is unreadable".to_string(),
                            })?
                            .with_timezone(&Utc),
                    };

                    let amount_changed = delta
                        .aligned_sub(current.amount)
                        .is_none_or(|difference| !difference.is_zero());
                    let moved =
                        amount_changed || store::facts::instant(occurred_at) != current.occurred_at;
                    let (compensated, replacement) = if moved {
                        let old_fact_uid =
                            current
                                .fact_uid
                                .clone()
                                .ok_or_else(|| EngineError::Conflict {
                                    code: "entry_fact_missing",
                                    message: "this entry has no Fact to correct".to_string(),
                                })?;
                        let old_fact = store::facts::get(&self.store.pool, &old_fact_uid)
                            .await?
                            .ok_or_else(|| EngineError::UnknownRecord(old_fact_uid.clone()))?;
                        let concept_uid =
                            store::ledger::fact_concept(&self.store.pool, &old_fact_uid).await?;

                        let mut appended = self
                            .append(
                                NewFact {
                                    uid: None,
                                    record_uid: old_fact.record_uid.clone(),
                                    delta: store::exact::negate(old_fact.delta)?,
                                    at: Some(old_fact.at),
                                    actor_uid: actor.clone(),
                                    cause: Cause {
                                        kind: CauseKind::Compensation,
                                        uid: Some(old_fact_uid.clone()),
                                    },
                                    payload: None,
                                },
                                now,
                            )
                            .await?;
                        let compensation_uid = appended.first().map(|f| f.uid.clone());
                        let replacement_facts = self
                            .append(
                                NewFact {
                                    actor_uid: actor.clone(),
                                    at: Some(occurred_at),
                                    ..NewFact::quantity(
                                        old_fact.record_uid.clone(),
                                        delta,
                                        Cause::user_edit(),
                                    )
                                },
                                now,
                            )
                            .await?;
                        let replacement_uid = replacement_facts.first().map(|f| f.uid.clone());
                        for uid in [&compensation_uid, &replacement_uid].into_iter().flatten() {
                            store::ledger::classify_fact(
                                &self.store.pool,
                                uid,
                                concept_uid.as_deref(),
                                actor.as_deref(),
                                None,
                            )
                            .await?;
                        }
                        appended.extend(replacement_facts);
                        outcome.facts = appended;
                        (compensation_uid, replacement_uid)
                    } else {
                        (None, None)
                    };

                    store::entries::revise(
                        &self.store.pool,
                        store::entries::ReviseEntry {
                            entry_uid: &entry,
                            expected_revision,
                            amount: delta,
                            note: note.as_deref(),
                            occurred_at,
                            compensated_fact_uid: compensated.as_deref(),
                            replacement_fact_uid: replacement.as_deref(),
                            request_id: &request_id,
                            actor_uid: actor.as_deref(),
                        },
                        now,
                    )
                    .await?;
                }
                Action::VoidEntry {
                    entry,
                    expected_revision,
                    request_id,
                } => {
                    if store::entries::replayed(&self.store.pool, &request_id)
                        .await?
                        .is_some()
                    {
                        return Ok(ControlFlow::Break(outcome));
                    }
                    let current = store::entries::get(&self.store.pool, &entry)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(entry.clone()))?;
                    if current.is_void() {
                        return Err(EngineError::Conflict {
                            code: "entry_already_void",
                            message: "this entry is already void".to_string(),
                        });
                    }
                    if current.revision != expected_revision {
                        return Err(EngineError::Conflict {
                            code: "entry_revision_stale",
                            message: "this entry was changed by someone else".to_string(),
                        });
                    }

                    let mut compensation_uid = None;
                    if let Some(old_fact_uid) = current.fact_uid.clone() {
                        let old_fact = store::facts::get(&self.store.pool, &old_fact_uid)
                            .await?
                            .ok_or_else(|| EngineError::UnknownRecord(old_fact_uid.clone()))?;
                        if !old_fact.delta.is_zero() {
                            let concept_uid =
                                store::ledger::fact_concept(&self.store.pool, &old_fact_uid)
                                    .await?;
                            outcome.facts = self
                                .append(
                                    NewFact {
                                        uid: None,
                                        record_uid: old_fact.record_uid.clone(),
                                        delta: store::exact::negate(old_fact.delta)?,
                                        at: Some(old_fact.at),
                                        actor_uid: actor.clone(),
                                        cause: Cause {
                                            kind: CauseKind::Compensation,
                                            uid: Some(old_fact_uid.clone()),
                                        },
                                        payload: None,
                                    },
                                    now,
                                )
                                .await?;
                            if let Some(fact) = outcome.facts.first() {
                                compensation_uid = Some(fact.uid.clone());
                                store::ledger::classify_fact(
                                    &self.store.pool,
                                    &fact.uid,
                                    concept_uid.as_deref(),
                                    actor.as_deref(),
                                    None,
                                )
                                .await?;
                            }
                        }
                    }

                    store::entries::void(
                        &self.store.pool,
                        store::entries::VoidEntry {
                            entry_uid: &entry,
                            expected_revision,
                            compensated_fact_uid: compensation_uid.as_deref(),
                            request_id: &request_id,
                            actor_uid: actor.as_deref(),
                        },
                        now,
                    )
                    .await?;
                }
                Action::ClassifyFact {
                    fact,
                    concept,
                    note,
                } => {
                    if store::facts::get(&self.store.pool, &fact).await?.is_none() {
                        return Err(EngineError::UnknownRecord(fact));
                    }
                    let concept_uid = self.resolve_concept_opt(concept).await?;
                    store::ledger::classify_fact(
                        &self.store.pool,
                        &fact,
                        concept_uid.as_deref(),
                        actor.as_deref(),
                        note.as_deref(),
                    )
                    .await?;
                }
                Action::AddQuantityGroupExact { changes } => {
                    if changes.is_empty() || changes.len() > 32 {
                        return Err(EngineError::Consequence(
                            "choose between one and 32 Record changes".into(),
                        ));
                    }
                    let mut records = std::collections::BTreeSet::new();
                    let mut facts = Vec::new();
                    for (target, delta) in changes {
                        let uid = self.resolve(&target).await?;
                        self.reject_direct_transfer_record_mutation(&uid).await?;
                        if !records.insert(uid.clone()) {
                            return Err(EngineError::Consequence(
                                "a Record may appear only once in a group".into(),
                            ));
                        }
                        facts.push(NewFact {
                            uid: None,
                            record_uid: uid,
                            delta,
                            at: None,
                            actor_uid: actor.clone(),
                            cause: Cause::user_edit(),
                            payload: None,
                        });
                    }
                    let signer = self.signer.lock().await.clone();
                    let facts =
                        crate::append::append_all(&self.store, facts, now, signer.as_ref()).await?;
                    for fact in facts {
                        outcome
                            .facts
                            .extend(self.observe_committed_fact(fact, now).await?);
                    }
                }
                Action::AddQuantityExact { target, delta } => {
                    let uid = self.resolve(&target).await?;
                    self.reject_direct_transfer_record_mutation(&uid).await?;
                    if delta.mantissa() != 0 {
                        outcome.facts = self
                            .append(
                                NewFact {
                                    uid: None,
                                    record_uid: uid,
                                    delta,
                                    at: None,
                                    actor_uid: actor,
                                    cause: Cause::user_edit(),
                                    payload: None,
                                },
                                now,
                            )
                            .await?;
                    }
                }
                Action::AddQuantity { target, delta } => {
                    let uid = self.resolve(&target).await?;
                    self.reject_direct_transfer_record_mutation(&uid).await?;
                    if delta != 0.0 {
                        outcome.facts = self
                            .append(
                                NewFact {
                                    actor_uid: actor,
                                    ..NewFact::quantity_f64(uid, delta, Cause::user_edit())
                                },
                                now,
                            )
                            .await?;
                    }
                }
                Action::Activate { target } => {
                    return Box::pin(self.act(Action::SetQuantity { target, value: 1.0 }, actor))
                        .await
                        .map(ControlFlow::Break);
                }
                Action::Deactivate { target } => {
                    return Box::pin(self.act(Action::SetQuantity { target, value: 0.0 }, actor))
                        .await
                        .map(ControlFlow::Break);
                }
                Action::DeleteRecord { target } => {
                    let uid = self.resolve(&target).await?;
                    self.reject_direct_transfer_record_mutation(&uid).await?;
                    self.check_delete_permission(&uid, actor.as_deref()).await?;
                    if store::karma::frequencies::get_handle(&self.store.pool, &uid)
                        .await?
                        .is_some()
                    {
                        store::frequency::delete(&self.store.pool, &uid)
                            .await
                            .map_err(|error| EngineError::Conflict {
                                code: "frequency_in_use",
                                message: error.to_string(),
                            })?;
                        return Ok(ControlFlow::Break(outcome));
                    }
                    let old_slug = store::records::get(&self.store.pool, &uid)
                        .await?
                        .and_then(|r| r.slug);
                    outcome.facts = self
                        .annotate(
                            uid.clone(),
                            actor,
                            serde_json::json!({ "deleted": true, "slug": old_slug }),
                            now,
                        )
                        .await?;
                    store::records::mark_deleted(&self.store.pool, &uid).await?;
                }
                Action::EditRecordText { target, head, body } => {
                    let uid = self.resolve(&target).await?;
                    outcome.facts = self
                        .write_record_text_as(
                            &uid,
                            head.as_deref(),
                            body.as_deref(),
                            actor.as_deref(),
                        )
                        .await?;
                }
                Action::SetSlug { target, slug } => {
                    outcome = self
                        .change_record(
                            crate::record_change::Request {
                                id: nucleus::new_uid("op"),
                                record_uid: self.resolve(&target).await?,
                                mutation: crate::record_change::Mutation::Slug {
                                    value: slug.filter(|value| !value.is_empty()),
                                },
                            },
                            actor.as_deref(),
                        )
                        .await?;
                }
                Action::SetUnit { target, unit } => {
                    let uid = self.resolve(&target).await?;
                    let unit_uid = self.resolve_concept_opt(unit).await?;
                    store::records::set_unit(&self.store.pool, &uid, unit_uid.as_deref()).await?;
                    outcome.facts = self
                        .annotate(uid, actor, serde_json::json!({ "unit": unit_uid }), now)
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
