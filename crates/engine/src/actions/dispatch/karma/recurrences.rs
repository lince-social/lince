// non-lean-yet

use super::super::{
    Action, ActionOutcome, ControlFlow, DateTime, DispatchOutcome, Engine, EngineError, Utc,
    invalid_dispatch, parse_instant_field, parse_optional_instant, parse_rule_condition,
};

impl Engine {
    pub(in super::super) async fn dispatch_karma_recurrences(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
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
                let condition = self
                    .canonical_condition_for(condition, actor.as_deref())
                    .await?;
                let mut declared_condition = parse_rule_condition(condition, gate, carry)?;
                if let Some(condition) = &mut declared_condition {
                    condition.bindings =
                        store::karma_bindings::resolve(&self.store.pool, &condition.source, &[])
                            .await?;
                }
                let mut effect_bindings = declared_condition
                    .as_ref()
                    .map_or_else(Vec::new, |condition| condition.bindings.clone());
                let declared = self
                    .bind_transfer_effects(declared.as_slice().to_vec(), &mut effect_bindings, &[])
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
                let condition = self
                    .canonical_condition_for(condition, actor.as_deref())
                    .await?;
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
                if !paused {
                    let rule = store::recurrence::get(&self.store.pool, &recurrence)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(recurrence.clone()))?;
                    Box::pin(self.validate_automatic_rule(
                        &rule.consequences,
                        rule.condition.as_ref(),
                        actor.as_deref(),
                    ))
                    .await?;
                    self.authorize_karma_rule(&rule, actor.as_deref()).await?;
                }
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
                                nucleus::karma::Consequence::CaptureEntry { concept, .. } => {
                                    nucleus::karma::Consequence::CaptureEntry {
                                        amount: value,
                                        concept,
                                    }
                                }
                                other => other,
                            })
                            .collect(),
                    )
                    .map_err(|error| EngineError::Consequence(error.to_string()))?;
                }
                if let Some(note) = note {
                    rule.note = Some(note);
                }
                rule.actor_uid = actor.clone();
                outcome.facts =
                    Box::pin(self.apply_rule_occurrence(&rule, parse_instant_field(&due_at)?, now))
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
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
