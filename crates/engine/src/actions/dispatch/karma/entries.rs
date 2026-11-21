// non-lean-yet

use super::super::{
    Action, ActionOutcome, Cause, CauseKind, ControlFlow, DateTime, DispatchOutcome, Engine,
    EngineError, NewFact, Utc, invalid_dispatch, parse_optional_instant,
};

impl Engine {
    pub(in super::super) async fn dispatch_karma_entries(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
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
                let delta = nucleus::DecimalValue::parse_inferred(amount.trim()).map_err(|_| {
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
                let delta = nucleus::DecimalValue::parse_inferred(amount.trim()).map_err(|_| {
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
                            store::ledger::fact_concept(&self.store.pool, &old_fact_uid).await?;
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
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
