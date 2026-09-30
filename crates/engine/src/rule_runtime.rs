use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use nucleus::karma::{
    DefinitionStatus, FrequencyAst, KarmaOccurrenceSource, NodeOperation, Slug, TimestampMs,
    TriggerSource,
};
use nucleus::{Cause, Fact, NewFact};
use store::sqlx::Row;
use store::karma::frequencies::{
    self, ActivateFrequencyInput, CreateFrequencyInput, FrequencyMutationCommit,
};

use crate::karma_runtime::KarmaDeadlineDirectorConfig;
use crate::{Engine, EngineError};

const VALUE_DEPENDENCY_LIMIT: usize = 4;

pub(crate) async fn execution_checkpoint(evaluation: bool) -> Result<(), EngineError> {
    if let Some(control) = nucleus::execution::current().and_then(|execution| execution.control()) {
        control.wait_running().await;
        control.checkpoint(evaluation).map_err(EngineError::ExecutionLimit)?;
    }
    Ok(())
}

#[derive(Clone, Default)]
pub struct RuleIndex {
    pub(crate) rules: BTreeMap<String, store::recurrence::Recurrence>,
    pub(crate) records: BTreeMap<String, BTreeSet<String>>,
    pub(crate) concept_members: BTreeMap<String, BTreeSet<String>>,
    pub(crate) frequencies: BTreeMap<String, BTreeSet<String>>,
    pub(crate) concepts: BTreeMap<String, BTreeSet<String>>,
    pub(crate) signals: BTreeMap<String, BTreeSet<String>>,
    pub(crate) used: BTreeSet<String>,
}

#[derive(Clone)]
pub struct RuleEvent {
    pub id: String,
    pub frequency: Option<String>,
    pub at: DateTime<Utc>,
    pub attempt: i64,
}

tokio::task_local! {
    pub(crate) static RULE_EVENT: RuleEvent;
    pub(crate) static EFFECT_OCCURRENCE: nucleus::simulation::RuleOccurrence;
    pub(crate) static RECEIVED_PARENT: String;
}

pub fn frequency_pulse(uid: &str) -> bool {
    RULE_EVENT
        .try_with(|event| event.frequency.as_deref() == Some(uid))
        .unwrap_or(false)
}

fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::Conflict {
        code: "karma_rule_invalid",
        message: message.into(),
    }
}

impl Engine {
    pub(crate) async fn check_control_quantities(&self, cell: &str, control: &nucleus::execution::control::Control, uid: &str, now: DateTime<Utc>) -> Result<(), EngineError> {
        let Some(record) = store::records::get(&self.store.pool, uid).await? else { return Ok(()) };
        let unit = record.unit_uid.map(|uid| nucleus::karma::TypedUid::new(nucleus::karma::ReferenceKind::Unit, uid))
            .transpose().map_err(|error| invalid(error.to_string()))?;
        let mut quantity = nucleus::simulation::Quantity { value: record.quantity, unit };
        control.check_quantity(cell, uid, &quantity).map_err(EngineError::ExecutionLimit)?;
        if control.checks_available(cell, uid) {
            for adjustment in store::transfer_loans::adjustments(&self.store.pool, now.timestamp_millis()).await? {
                if adjustment.record == uid {
                    if adjustment.unit_changed { return Ok(()) }
                    quantity.value = store::exact::sum_exact([quantity.value, adjustment.delta])?;
                }
            }
            control.check_available_quantity(cell, uid, &quantity).map_err(EngineError::ExecutionLimit)?;
        }
        Ok(())
    }

    pub(crate) async fn apply_rule_occurrence(
        &self,
        rule: &store::recurrence::Recurrence,
        due: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Vec<Fact>, EngineError> {
        let _guard = self.rule_execution.lock().await;
        let index = Box::pin(self.rule_dependencies()).await?;
        let frequency = index
            .frequencies
            .iter()
            .find_map(|(uid, rules)| rules.contains(&rule.uid).then(|| uid.clone()));
        let boundary = match &frequency {
            Some(uid) => self.frequency_has_boundary(uid, due).await?,
            None => {
                let anchor = crate::actions::parse_instant_field(&rule.anchor_at)?;
                let end = due
                    .checked_add_signed(chrono::TimeDelta::nanoseconds(1))
                    .ok_or_else(|| invalid("occurrence date is out of range"))?;
                rule.cadence
                    .between(anchor, due, end)
                    .map_err(|error| invalid(error.to_string()))?
                    .dates
                    .contains(&due)
            }
        };
        if !boundary {
            return Err(invalid("this date is not a boundary of the rule Frequency"));
        }
        let event = RuleEvent {
            attempt: 0,
            id: format!(
                "frequency:{}:{}",
                frequency.as_deref().unwrap_or(&rule.uid),
                due.timestamp_millis()
            ),
            frequency,
            at: due,
        };
        let mut facts = match Box::pin(self.execute_rule_event(rule, &event, now)).await {
            Ok(facts) => facts,
            Err(error @ EngineError::ExecutionLimit(_)) => return Err(error),
            Err(error) => {
                if store::karma_schedules::for_rule(&self.store.pool, &rule.uid).await?.is_some() {
                    self.record_rule_failure(rule, &event, &error, now).await?;
                    store::karma_schedules::refresh(&self.store.pool, now).await?;
                }
                return Err(error);
            }
        };
        for fact in &facts {
            let _ = self.bus.send(fact.clone());
        }
        let roots: BTreeMap<_, _> = facts.iter().map(|fact| (fact.record_uid.clone(), fact.uid.clone())).collect();
        for (record, parent) in roots {
            facts.extend(self.run_rule_reactions(vec![record], parent, now).await?);
        }
        if store::karma_schedules::refresh(&self.store.pool, now).await? { self.notify_karma_deadline_change(); }
        Ok(facts)
    }

    async fn frequency_has_boundary(
        &self,
        uid: &str,
        due: DateTime<Utc>,
    ) -> Result<bool, EngineError> {
        let handle = frequencies::get_handle(&self.store.pool, uid)
            .await?
            .ok_or_else(|| invalid("rule Frequency is missing"))?;
        let compiled = match handle.active_activation_hash {
            Some(hash) => frequencies::get_activation(&self.store.pool, &hash)
                .await?
                .ok_or_else(|| invalid("rule activation is missing"))?
                .epoch
                .compiled()
                .clone(),
            None => {
                frequencies::get_revision(&self.store.pool, &handle.head_revision_hash)
                    .await?
                    .ok_or_else(|| invalid("rule Frequency revision is missing"))?
                    .default_compiled
            }
        };
        if due.timestamp_subsec_nanos() % 1_000_000 != 0 {
            return Ok(false);
        }
        match compiled.schedule {
            nucleus::karma::CompiledSchedule::Elapsed { schedule } => {
                let elapsed =
                    i128::from(due.timestamp_millis()) - i128::from(schedule.anchor().as_millis());
                Ok(elapsed >= 0 && elapsed % i128::from(schedule.interval_ms()) == 0)
            }
            nucleus::karma::CompiledSchedule::Calendar { schedule } => {
                let config = match self.configured_karma_runtime() {
                    Ok(config) => config,
                    Err(EngineError::Conflict {
                        code: "karma_runtime_unconfigured",
                        ..
                    }) => KarmaDeadlineDirectorConfig::for_host("manual-rule-boundary".into())?,
                    Err(error) => return Err(error),
                };
                let provider = config
                    .provider(&schedule.tzdb)
                    .ok_or_else(|| invalid("the Frequency timezone provider is not installed"))?;
                let mut previous = None;
                for _ in 0..65_536 {
                    let next = schedule
                        .next_after(provider, previous)
                        .map_err(|error| invalid(error.to_string()))?;
                    let Some(boundary) = next.boundary else {
                        return Ok(false);
                    };
                    if boundary.intended_at.as_millis() >= due.timestamp_millis() {
                        return Ok(boundary.intended_at.as_millis() == due.timestamp_millis());
                    }
                    previous = Some(boundary);
                }
                Err(invalid(
                    "manual calendar boundary search exceeded its work limit",
                ))
            }
        }
    }

    pub(crate) async fn validate_automatic_rule(
        &self,
        consequences: &nucleus::karma::Consequences,
        condition: Option<&store::recurrence::RuleCondition>,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        if self.transfer_rule_anchor(consequences, actor).await?.is_none() {
            self.require_permission(actor, "record:update").await?;
        }
        if consequences.iter().any(|effect| {
            matches!(
                effect,
                nucleus::karma::Consequence::RunCommand { .. }
                    | nucleus::karma::Consequence::RunAction { .. }
            )
        }) {
            self.require_permission(actor, "organ:update").await?;
        }
        if let Some(condition) = condition {
            self.karma_condition_records(condition.parsed().map_err(|error| invalid(error.to_string()))?, actor).await?;
        }
        Ok(())
    }

    pub(crate) async fn react_to_event(
        &self,
        changed: Vec<String>,
        event_id: String,
        now: DateTime<Utc>,
    ) -> Result<Vec<Fact>, EngineError> {
        if crate::already_firing() {
            return Ok(Vec::new());
        }
        let _guard = self.rule_execution.lock().await;
        crate::as_one_firing(self.run_rule_reactions(changed, event_id, now)).await
    }

    async fn run_rule_reactions(
        &self,
        changed: Vec<String>,
        event_id: String,
        now: DateTime<Utc>,
    ) -> Result<Vec<Fact>, EngineError> {
        let index = Box::pin(self.rule_dependencies()).await?;
        let mut queue = VecDeque::from([(changed, event_id)]);
        let mut result = Vec::new();
        let mut steps = 0;
        while let Some((changed, event_id)) = queue.pop_front() {
            execution_checkpoint(false).await?;
            let mut readers = BTreeSet::new();
            for uid in changed {
                readers.extend(index.records.get(&uid).into_iter().flatten().cloned());
                readers.extend(
                    index
                        .concept_members
                        .get(&uid)
                        .into_iter()
                        .flatten()
                        .cloned(),
                );
                for (concept, rules) in &index.concepts {
                    if store::ledger::records_with_concept(&self.store.pool, concept)
                        .await?
                        .contains(&uid)
                    {
                        readers.extend(rules.iter().cloned());
                    }
                }
            }
            for uid in readers {
                steps += 1;
                if steps > 256 && nucleus::execution::current().and_then(|execution| execution.control()).is_none() {
                    return Err(EngineError::Conflict {
                        code: "karma_reaction_limit",
                        message: "Rule reactions exceeded 256 evaluations; inspect the rule cycle."
                            .into(),
                    });
                }
                let rule = &index.rules[&uid];
                let event = RuleEvent {
                    attempt: 0,
                    id: event_id.clone(),
                    frequency: None,
                    at: now,
                };
                let facts = match Box::pin(self.execute_rule_event(rule, &event, now)).await {
                    Ok(facts) => facts,
                    Err(error @ EngineError::ExecutionLimit(_)) => return Err(error),
                    Err(error) => {
                        self.record_rule_failure(rule, &event, &error, now).await?;
                        tracing::warn!(rule = rule.uid, %error, "Karma rule refused a Record change");
                        continue;
                    }
                };
                for fact in &facts {
                    let _ = self.bus.send(fact.clone());
                    queue.push_back((vec![fact.record_uid.clone()], fact.uid.clone()));
                }
                result.extend(facts);
            }
        }
        Ok(result)
    }

    pub(crate) async fn has_rule_occurrences(&self) -> Result<bool, EngineError> {
        Ok(store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM karma_occurrence WHERE cell_sequence >= (SELECT next_sequence FROM karma_rule_progress WHERE singleton = 1))")
            .fetch_one(&self.store.pool).await?)
    }

    pub(crate) async fn process_rule_occurrences(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<Fact>, EngineError> {
        let _guard = self.rule_execution.lock().await;
        crate::as_one_firing(self.process_rule_occurrences_inner(now)).await
    }

    async fn process_rule_occurrences_inner(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<Fact>, EngineError> {
        let mut result = Box::pin(self.recover_schedule_retries(now)).await?;
        store::karma_schedules::refresh(&self.store.pool, now).await?;
        let index = Box::pin(self.rule_dependencies()).await?;
        for _ in 0..64 {
            execution_checkpoint(false).await?;
            let sequence: i64 = store::sqlx::query_scalar(
                "SELECT next_sequence FROM karma_rule_progress WHERE singleton = 1",
            )
            .fetch_one(&self.store.pool)
            .await?;
            let Some(occurrence) =
                store::karma::occurrences::get_by_cell_sequence(&self.store.pool, sequence as u64)
                    .await?
            else {
                break;
            };
            let (activation_hash, schedule_occurrence_hash) = match &occurrence.envelope.source {
                KarmaOccurrenceSource::ScheduleTick {
                    tick,
                    schedule_occurrence_hash,
                } => (&tick.activation_hash, schedule_occurrence_hash),
                KarmaOccurrenceSource::ScheduleCoalesced {
                    batch,
                    schedule_occurrence_hash,
                } => (&batch.activation_hash, schedule_occurrence_hash),
                KarmaOccurrenceSource::CalendarTick {
                    tick,
                    schedule_occurrence_hash,
                } => (&tick.activation_hash, schedule_occurrence_hash),
                KarmaOccurrenceSource::CalendarCoalesced {
                    batch,
                    schedule_occurrence_hash,
                } => (&batch.activation_hash, schedule_occurrence_hash),
            };
            let observed_at =
                store::karma::schedules::get_occurrence(&self.store.pool, schedule_occurrence_hash)
                    .await?
                    .ok_or_else(|| invalid("occurrence has no scheduler emission"))?
                    .occurrence
                    .observed_at();
            let activation = frequencies::get_activation(&self.store.pool, activation_hash)
                .await?
                .ok_or_else(|| invalid("occurrence has no Frequency activation"))?;
            let uid = activation.epoch.frequency_uid();
            let active = frequencies::get_handle(&self.store.pool, uid)
                .await?
                .is_some_and(|handle| {
                    handle.active_activation_hash.as_ref() == Some(activation_hash)
                        && handle.status == DefinitionStatus::Active
                });
            if active {
                let event = RuleEvent {
                    attempt: 0,
                    id: format!("frequency:{uid}:{}", occurrence.logical_at.as_millis()),
                    frequency: Some(uid.to_string()),
                    at: DateTime::from_timestamp_millis(occurrence.logical_at.as_millis())
                        .ok_or_else(|| invalid("invalid occurrence time"))?,
                };
                for rule_uid in index.frequencies.get(uid).into_iter().flatten() {
                    let rule = &index.rules[rule_uid];
                    let updated = crate::actions::parse_instant_field(&rule.updated_at)?;
                    if updated.timestamp_millis() > observed_at.as_millis() {
                        continue;
                    }
                    match Box::pin(self.execute_rule_event(rule, &event, now)).await {
                        Ok(facts) => {
                            for fact in &facts {
                                let _ = self.bus.send(fact.clone());
                            }
                            let roots: BTreeMap<_, _> = facts.iter().map(|fact| (fact.record_uid.clone(), fact.uid.clone())).collect();
                            result.extend(facts);
                            for (record, parent) in roots {
                                result.extend(self.run_rule_reactions(vec![record], parent, now).await?);
                            }
                        }
                        Err(error @ EngineError::ExecutionLimit(_)) => return Err(error),
                        Err(error) => {
                            self.record_rule_failure(rule, &event, &error, now).await?;
                            tracing::warn!(rule = rule.uid, %error, "Karma rule refused its occurrence");
                        }
                    }
                }
                for signal in index.signals.get(uid).into_iter().flatten() {
                    let row = store::misc::list_signals(&self.store.pool)
                        .await?
                        .into_iter()
                        .find(|row| &row.record_uid == signal);
                    if let Some(row) = row {
                        let mut tx = store::write_tx(&self.store.pool).await?;
                        queue_effect_tx(&mut tx, &format!("signal:{}:{}", signal, event.id), "signal", serde_json::json!({"command": row.source, "signal": signal, "actor": row.actor_uid}), signal, now).await?;
                        tx.commit().await?;
                        self.effects_changed
                            .send_modify(|revision| *revision = revision.wrapping_add(1));
                    }
                }
            }
            store::sqlx::query("UPDATE karma_rule_progress SET next_sequence = ? WHERE singleton = 1 AND next_sequence = ?")
                .bind(sequence + 1).bind(sequence).execute(&self.store.pool).await?;
        }
        if store::karma_schedules::refresh(&self.store.pool, now).await? { self.notify_karma_deadline_change(); }
        Ok(result)
    }

    pub(crate) async fn recover_schedule_retries(&self, now: DateTime<Utc>) -> Result<Vec<Fact>, EngineError> {
        let values = store::sqlx::query("SELECT * FROM karma_schedule_boundary WHERE current = 1 AND status = 'pending' AND attempt > 0 ORDER BY intended_at_ms, uid")
            .fetch_all(&self.store.pool).await?;
        let mut facts = Vec::new();
        for value in values {
            let uid: String = value.get("rule_uid");
            let Some(rule) = store::recurrence::get(&self.store.pool, &uid).await? else { continue };
            if Some(rule.revision) != value.get::<Option<i64>, _>("rule_revision") || rule.is_paused() { continue }
            let event = RuleEvent { id: value.get("event_id"), frequency: value.get("frequency_uid"), at: DateTime::from_timestamp_millis(value.get("intended_at_ms")).ok_or_else(|| invalid("Invalid scheduled occurrence date"))?, attempt: value.get("attempt") };
            match Box::pin(self.execute_rule_event(&rule, &event, now)).await {
                Ok(applied) => {
                    for fact in &applied { let _ = self.bus.send(fact.clone()); }
                    let roots: BTreeMap<_, _> = applied.iter().map(|fact| (fact.record_uid.clone(), fact.uid.clone())).collect();
                    facts.extend(applied);
                    for (record, parent) in roots { facts.extend(self.run_rule_reactions(vec![record], parent, now).await?); }
                }
                Err(error @ EngineError::ExecutionLimit(_)) => return Err(error),
                Err(error) => self.record_rule_failure(&rule, &event, &error, now).await?,
            }
        }
        Ok(facts)
    }

    pub(crate) async fn record_rule_failure(
        &self,
        rule: &store::recurrence::Recurrence,
        event: &RuleEvent,
        error: &EngineError,
        now: DateTime<Utc>,
    ) -> Result<(), EngineError> {
        store::sqlx::query(
            "INSERT OR IGNORE INTO karma_rule_application(event_id, rule_uid, rule_revision, status, reason, at, intended_at, frequency_uid, attempt) VALUES (?, ?, ?, 'failed', ?, ?, ?, ?, ?)",
        )
        .bind(&event.id)
        .bind(&rule.uid)
        .bind(rule.revision)
        .bind(error.to_string())
        .bind(now.to_rfc3339())
        .bind(event.at.to_rfc3339())
        .bind(&event.frequency)
        .bind(event.attempt)
        .execute(&self.store.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn execute_rule_event(
        &self,
        rule: &store::recurrence::Recurrence,
        event: &RuleEvent,
        now: DateTime<Utc>,
    ) -> Result<Vec<Fact>, EngineError> {
        crate::karma_transfers::scope(rule.actor_uid.as_deref(), self.execute_rule_event_with_reads(rule, event, now)).await
    }

    async fn execute_rule_event_with_reads(
        &self,
        rule: &store::recurrence::Recurrence,
        event: &RuleEvent,
        now: DateTime<Utc>,
    ) -> Result<Vec<Fact>, EngineError> {
        let mut evidence = self.capture_rule_evidence(rule).await;
        if let Some(inputs) = &mut evidence.inputs
            && let Some(frequency) = &event.frequency
            && !inputs.contains(frequency) {
            inputs.push(frequency.clone());
        }
        crate::karma_history::CAPTURE.scope(std::cell::RefCell::new(evidence), async {
            let result = Box::pin(self.execute_rule_event_inner(rule, event, now)).await;
            if result.as_ref().err().is_some_and(|error| !matches!(error, EngineError::ExecutionLimit(_))) {
                let mut connection = self.store.pool.acquire().await?;
                crate::karma_history::persist(&mut connection, rule, event).await?;
            }
            result
        }).await
    }

    async fn execute_rule_event_inner(
        &self,
        rule: &store::recurrence::Recurrence,
        event: &RuleEvent,
        now: DateTime<Utc>,
    ) -> Result<Vec<Fact>, EngineError> {
        let current = store::recurrence::get(&self.store.pool, &rule.uid).await?;
        if current.is_none_or(|current| current.is_paused() || current.revision != rule.revision) {
            return Ok(Vec::new());
        }
        if !store::karma_schedules::admit(&self.store.pool, &rule.uid, event.frequency.as_deref(), event.at.timestamp_millis(), now).await? { return Ok(Vec::new()) }
        let consumed: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM karma_rule_application WHERE event_id = ? AND rule_uid = ? AND rule_revision = ? AND (attempt = ? OR status = 'applied'))")
            .bind(&event.id).bind(&rule.uid).bind(rule.revision).bind(event.attempt).fetch_one(&self.store.pool).await?;
        if consumed {
            return Ok(Vec::new());
        }
        if event.frequency.is_some() {
            let skipped: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM recurrence_skip WHERE recurrence_uid = ? AND due_at = ?)")
                .bind(&rule.uid).bind(store::facts::instant(event.at)).fetch_one(&self.store.pool).await?;
            if skipped {
                return Ok(Vec::new());
            }
        }
        let stage = self.transfer_stage_evaluation(rule, now).await?;
        if stage.is_some() && let Some(execution) = nucleus::execution::current()
            && let Some(control) = execution.control()
            && let Some(cell) = execution.cell()
            && let Some(origin) = store::karma_stages::for_rule(&self.store.pool, &rule.uid).await? {
            control.link_effect(cell, &event.id, &origin.occurrence);
        }
        execution_checkpoint(true).await?;
        Box::pin(self.validate_automatic_rule(
            &rule.consequences,
            rule.condition.as_ref(),
            rule.actor_uid.as_deref(),
        ))
        .await?;
        self.authorize_karma_rule(rule, rule.actor_uid.as_deref()).await?;
        let carried = match &rule.condition {
            None => None,
            Some(condition) => match RULE_EVENT
                .scope(
                    event.clone(),
                    Box::pin(self.evaluate_rule_condition(condition, event.at, now)),
                )
                .await?
            {
                Some(value) => Some(value),
                None => {
                    let mut tx = store::write_tx(&self.store.pool).await?;
                    store::sqlx::query("INSERT OR IGNORE INTO karma_rule_application(event_id, rule_uid, rule_revision, status, reason, at, intended_at, frequency_uid, attempt) VALUES (?, ?, ?, 'blocked', NULL, ?, ?, ?, ?)")
                        .bind(&event.id).bind(&rule.uid).bind(rule.revision).bind(now.to_rfc3339()).bind(event.at.to_rfc3339()).bind(&event.frequency).bind(event.attempt).execute(&mut *tx).await?;
                    crate::karma_history::persist(&mut tx, rule, event).await?;
                    tx.commit().await?;
                    return Ok(Vec::new());
                }
            },
        };
        let mut transfer_effects = Vec::with_capacity(rule.consequences.len());
        for consequence in &rule.consequences {
            let mut prepared = self.prepare_transfer_effect(consequence, carried, rule.actor_uid.as_deref()).await?;
            if let Some(original) = &stage && let Some(prepared) = &mut prepared {
                if prepared.transfer != original.transfer || prepared.person != original.person {
                    return Err(invalid("A guarded stage must retain its Transfer and acting Person"));
                }
                let level = prepared.level;
                *prepared = original.clone();
                prepared.level = level;
            }
            transfer_effects.push(prepared);
        }
        let signer = self.signer.lock().await.clone();
        let mut tx = store::write_tx(&self.store.pool).await?;
        let inserted = store::sqlx::query("INSERT OR IGNORE INTO karma_rule_application(event_id, rule_uid, rule_revision, status, reason, at, intended_at, frequency_uid, attempt) VALUES (?, ?, ?, 'applied', NULL, ?, ?, ?, ?)")
            .bind(&event.id).bind(&rule.uid).bind(rule.revision).bind(now.to_rfc3339()).bind(event.at.to_rfc3339()).bind(&event.frequency).bind(event.attempt).execute(&mut *tx).await?;
        if inserted.rows_affected() == 0 {
            return Ok(Vec::new());
        }
        let mut facts = Vec::new();
        let mut changes = Vec::new();
        let occurrence = nucleus::simulation::RuleOccurrence {
            rule_uid: rule.uid.clone(), revision: rule.revision.try_into().map_err(|_| invalid("negative Rule revision"))?, event_id: event.id.clone(),
            frequency: event.frequency.as_ref().map(|uid| nucleus::karma::TypedUid::new(nucleus::karma::ReferenceKind::Frequency, uid)).transpose().map_err(|error| invalid(error.to_string()))?,
            intended_at_ms: Some(event.at.timestamp_millis()),
        };
        let execution = nucleus::execution::current();
        let control = execution.as_ref().and_then(|execution| execution.control());
        for (position, consequence) in rule.consequences.iter().enumerate() {
            use nucleus::karma::Consequence;
            let current = store::sqlx::query("SELECT quantity_mantissa, quantity_scale, unit_uid FROM record WHERE uid = ? AND deleted_at IS NULL")
                .bind(&rule.record_uid).fetch_optional(&mut *tx).await?.ok_or_else(|| invalid("rule target is missing"))?;
            let unit: Option<String> = current.try_get("unit_uid")?;
            let current = store::exact::read_decimal(&current, "quantity")?;
            let delta = match consequence {
                Consequence::SetQuantity { value } => Some(store::exact::difference(
                    value
                        .or(carried)
                        .ok_or_else(|| invalid("set quantity needs a value"))?,
                    current,
                )?),
                Consequence::AddQuantity { delta } => Some(
                    delta
                        .or(carried)
                        .ok_or_else(|| invalid("add quantity needs a value"))?,
                ),
                Consequence::CaptureEntry { amount, concept } => {
                    let action = crate::actions::Action::CaptureEntry {
                        target: rule.record_uid.clone(),
                        amount: carried.unwrap_or(*amount).to_string(),
                        concept: concept.clone(),
                        note: rule.note.clone(),
                        at: Some(event.at.to_rfc3339()),
                        request_id: Some(format!(
                            "{}:{}:{}:{position}",
                            event.id, rule.uid, rule.revision
                        )),
                    };
                    queue_effect_tx(&mut tx, &format!("{}:{}:{}:{position}", event.id, rule.uid, rule.revision), "action", serde_json::json!({"action": action, "actor": rule.actor_uid, "rule": rule.uid, "revision": rule.revision, "occurrence": occurrence}), &rule.record_uid, now).await?;
                    None
                }
                Consequence::RunCommand { command } => {
                    queue_effect_tx(&mut tx, &format!("{}:{}:{}:{position}", event.id, rule.uid, rule.revision), "command", serde_json::json!({"command": command, "actor": rule.actor_uid, "rule": rule.uid, "revision": rule.revision, "occurrence": occurrence}), &rule.record_uid, now).await?;
                    None
                }
                Consequence::Notify { message } => {
                    queue_effect_tx(
                        &mut tx,
                        &format!("{}:{}:{}:{position}", event.id, rule.uid, rule.revision),
                        "notify",
                        serde_json::json!({"message": message, "carried": carried, "actor": rule.actor_uid, "rule": rule.uid, "revision": rule.revision, "occurrence": occurrence}),
                        &rule.record_uid,
                        now,
                    )
                    .await?;
                    None
                }
                _ => {
                    let request_id = format!("{}:{}:{}:{position}", event.id, rule.uid, rule.revision);
                    queue_effect_tx(&mut tx, &request_id, "consequence", serde_json::json!({"consequence": consequence, "carried": carried, "actor": rule.actor_uid, "rule": rule.uid, "revision": rule.revision, "occurrence": occurrence, "transfer_evaluation": transfer_effects[position], "request_id": request_id}), &rule.record_uid, now).await?;
                    None
                }
            };
            if let Some(delta) = delta
                && delta != store::exact::zero()
            {
                let mut new = NewFact::quantity(
                    rule.record_uid.clone(),
                    delta,
                    Cause::rule(rule.uid.clone()),
                );
                new.at = Some(event.at);
                new.actor_uid = rule.actor_uid.clone();
                new.payload = Some(serde_json::json!({"rule": rule.uid, "revision": rule.revision, "event": event.id, "intended_at": event.at, "consequence": position}).to_string());
                if let Some(fact) =
                    crate::append::append_one_in_transaction(&mut tx, new, now, signer.as_ref())
                        .await?
                {
                    if control.is_some() {
                        let unit = unit.map(|uid| nucleus::karma::TypedUid::new(nucleus::karma::ReferenceKind::Unit, uid)).transpose().map_err(|error| invalid(error.to_string()))?;
                        changes.push(nucleus::simulation::RuleChange {
                            record: nucleus::karma::TypedUid::new(nucleus::karma::ReferenceKind::Record, rule.record_uid.clone()).map_err(|error| invalid(error.to_string()))?,
                            fact: fact.uid.clone().try_into().map_err(invalid)?,
                            before: nucleus::simulation::Quantity { value: current, unit: unit.clone() },
                            after: nucleus::simulation::Quantity { value: store::exact::sum_exact([current, delta])?, unit },
                        });
                    }
                    facts.push(fact);
                }
            }
        }
        crate::karma_history::persist(&mut tx, rule, event).await?;
        tx.commit().await?;
        if let Some(control) = control && let Some(cell) = execution.as_ref().and_then(|execution| execution.cell()) {
            let consequences = rule.consequences.iter().map(|consequence| serde_json::to_value(consequence).map(|value| value["kind"].as_str().unwrap_or("consequence").to_owned())).collect::<Result<Vec<_>, _>>().map_err(EngineError::Json)?;
            control.record(nucleus::simulation::RuleStep { cell: cell.to_owned(), occurrence, changes, transfer_changes: Vec::new(), consequences, at_ms: now.timestamp_millis(), virtual_ms: control.now_ms() }).map_err(EngineError::ExecutionLimit)?;
            let changed: BTreeSet<_> = facts.iter().map(|fact| fact.record_uid.as_str()).collect();
            for uid in changed {
                if control.checks_available(cell, uid) {
                    self.check_control_quantities(cell, &control, uid, now).await?;
                }
            }
        }
        self.effects_changed
            .send_modify(|revision| *revision = revision.wrapping_add(1));
        self.query_changed
            .send_modify(|revision| *revision = revision.wrapping_add(1));
        Ok(facts)
    }

    pub(crate) async fn rule_dependencies(&self) -> Result<Arc<RuleIndex>, EngineError> {
        let revision = *self.karma_deadline_changed.borrow();
        let mut cached = self.rule_index.lock().await;
        if let Some((known, index)) = cached.as_ref()
            && *known == revision
        {
            return Ok(index.clone());
        }
        let mut index = RuleIndex::default();
        for rule in store::recurrence::all(&self.store.pool).await? {
            if rule.is_paused() {
                continue;
            }
            let target = store::records::get(&self.store.pool, &rule.record_uid).await?;
            if target.is_none() {
                continue;
            }
            if let Some(condition) = &rule.condition {
                for token in condition
                    .parsed()
                    .map_err(|e| invalid(e.to_string()))?
                    .reads()
                {
                    if token.func == "freq" {
                        let uid = self.resolve_frequency_uid(&token.slug).await?;
                        index
                            .frequencies
                            .entry(uid.clone())
                            .or_default()
                            .insert(rule.uid.clone());
                        index.used.insert(uid);
                    } else if nucleus::transfer::karma::is_reading(&token.func) {
                        for name in token.slug.split('|') {
                            let uid = match store::records::resolve(&self.store.pool, name).await? {
                                Some(record) => record.uid,
                                None => name.to_string(),
                            };
                            index.records.entry(uid).or_default().insert(rule.uid.clone());
                        }
                    } else if token.func == nucleus::expr::ASSERTION {
                        index
                            .concepts
                            .entry(token.slug.clone())
                            .or_default()
                            .insert(rule.uid.clone());
                    } else {
                        for name in token.slug.split('|') {
                            if let Some(record) =
                                store::records::resolve(&self.store.pool, name).await?
                            {
                                index
                                    .records
                                    .entry(record.uid)
                                    .or_default()
                                    .insert(rule.uid.clone());
                            }
                        }
                    }
                }
            } else {
                let frequency = self.bind_rule_frequency(&rule).await?;
                index
                    .frequencies
                    .entry(frequency.clone())
                    .or_default()
                    .insert(rule.uid.clone());
                index.used.insert(frequency);
            }
            index.rules.insert(rule.uid.clone(), rule);
        }
        for _ in 0..VALUE_DEPENDENCY_LIMIT {
            let mut expanded = index.records.clone();
            for rule in index.rules.values() {
                let Some(condition) = &rule.condition else {
                    continue;
                };
                for token in condition
                    .parsed()
                    .map_err(|error| invalid(error.to_string()))?
                    .reads()
                {
                    if token.func != "value" {
                        continue;
                    }
                    let Some(record) =
                        store::records::resolve(&self.store.pool, &token.slug).await?
                    else {
                        continue;
                    };
                    for dependency in index
                        .rules
                        .values()
                        .filter(|dependency| dependency.record_uid == record.uid)
                    {
                        for (record_uid, readers) in &index.records {
                            if readers.contains(&dependency.uid) {
                                expanded
                                    .entry(record_uid.clone())
                                    .or_default()
                                    .insert(rule.uid.clone());
                            }
                        }
                    }
                }
            }
            if expanded == index.records {
                break;
            }
            index.records = expanded;
        }
        for signal in store::misc::list_signals(&self.store.pool).await? {
            if !index.records.contains_key(&signal.record_uid) {
                continue;
            }
            let frequency = self.bind_signal_frequency(&signal).await?;
            index
                .signals
                .entry(frequency.clone())
                .or_default()
                .insert(signal.record_uid);
            index.used.insert(frequency);
        }
        for (concept, rules) in &index.concepts {
            for member in store::ledger::records_with_concept(&self.store.pool, concept).await? {
                index
                    .concept_members
                    .entry(member)
                    .or_default()
                    .extend(rules.iter().cloned());
            }
        }
        for handle in store::karma::programs::list_handles(&self.store.pool).await? {
            if handle.status != DefinitionStatus::Active
                || !store::karma::execution::executes(&self.store.pool, &handle.record_uid).await?
            {
                continue;
            }
            let local_executor: bool = store::sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM record_extension WHERE record_uid = ? AND namespace = 'lince.schedule.executor' AND json_extract(fds, '$.cell') IS NOT NULL AND json_extract(fds, '$.cell') IS NOT (SELECT uid FROM record WHERE slug = ? AND kind = 'device' LIMIT 1))")
                .bind(&handle.record_uid).bind(store::cells::LOCAL_CELL_SLUG).fetch_one(&self.store.pool).await?;
            if !local_executor {
                continue;
            }
            let Some(hash) = handle.active_revision_hash else {
                continue;
            };
            let Some(revision) =
                store::karma::programs::get_revision(&self.store.pool, &hash).await?
            else {
                continue;
            };
            for node in revision.program.nodes.values() {
                if let NodeOperation::Trigger {
                    source: TriggerSource::Frequency { frequency },
                    ..
                } = &node.operation
                {
                    index.used.insert(frequency.target.as_str().to_string());
                }
            }
        }
        let index = Arc::new(index);
        *cached = Some((revision, index.clone()));
        Ok(index)
    }

    pub(crate) async fn resolve_frequency_uid(&self, name: &str) -> Result<String, EngineError> {
        store::sqlx::query_scalar("SELECT k.record_uid FROM karma_frequency k JOIN record r ON r.uid = k.record_uid WHERE (r.slug = ? OR r.uid = ?) AND r.deleted_at IS NULL")
            .bind(name.trim_start_matches('@')).bind(name.trim_start_matches('@'))
            .fetch_optional(&self.store.pool).await?.ok_or_else(|| invalid(format!("unknown Frequency @{name}")))
    }

    async fn create_internal_frequency(
        &self,
        ast: FrequencyAst,
        key: String,
        now: DateTime<Utc>,
    ) -> Result<String, EngineError> {
        let commit = frequencies::create(
            &self.store.pool,
            CreateFrequencyInput {
                request_id: key,
                frequency: ast,
                owner_person_uid: None,
                actor_person_uid: None,
            },
            now,
            |_| None,
        )
        .await?;
        match commit {
            FrequencyMutationCommit::Committed { handle, .. }
            | FrequencyMutationCommit::Replayed { handle, .. } => Ok(handle.record_uid),
            FrequencyMutationCommit::Stale { .. } => {
                Err(invalid("frequency changed during binding"))
            }
        }
    }

    async fn bind_cadence(
        &self,
        cadence: &nucleus::karma::Cadence,
        anchor: DateTime<Utc>,
    ) -> Result<String, EngineError> {
        let mut ast = nucleus::karma::simple_frequency::frequency_from_cadence(
            Slug::new("recurring").map_err(|error| invalid(error.to_string()))?,
            "Recurring frequency".into(),
            cadence,
            TimestampMs::from_millis(anchor.timestamp_millis())
                .map_err(|error| invalid(error.to_string()))?,
        )
        .map_err(|error| invalid(error.to_string()))?;
        let compiled = ast
            .compile(&BTreeMap::new())
            .map_err(|error| invalid(error.to_string()))?;
        let hash =
            nucleus::karma::canonical_hash("lince.recurring-frequency.v1", &compiled.schedule)
                .map_err(|error| invalid(error.to_string()))?;
        ast.slug = Slug::new(format!(
            "cadence.{}",
            hash.as_str().trim_start_matches("sha256:")
        ))
        .map_err(|error| invalid(error.to_string()))?;
        self.create_internal_frequency(ast, format!("cadence:{}", hash.as_str()), anchor)
            .await
    }

    async fn bind_rule_frequency(
        &self,
        rule: &store::recurrence::Recurrence,
    ) -> Result<String, EngineError> {
        let existing: Option<String> = store::sqlx::query_scalar("SELECT frequency_uid FROM karma_rule_frequency WHERE recurrence_uid = ? AND rule_revision = ?")
            .bind(&rule.uid).bind(rule.revision).fetch_optional(&self.store.pool).await?;
        if let Some(uid) = existing {
            return Ok(uid);
        }
        let anchor = crate::actions::parse_instant_field(&rule.anchor_at)?;
        let uid = self.bind_cadence(&rule.cadence, anchor).await?;
        store::sqlx::query("INSERT INTO karma_rule_frequency VALUES (?, ?, ?) ON CONFLICT(recurrence_uid) DO UPDATE SET frequency_uid = excluded.frequency_uid, rule_revision = excluded.rule_revision")
            .bind(&rule.uid).bind(&uid).bind(rule.revision).execute(&self.store.pool).await?;
        Ok(uid)
    }

    async fn bind_signal_frequency(
        &self,
        signal: &store::misc::SignalRow,
    ) -> Result<String, EngineError> {
        let existing: Option<String> = store::sqlx::query_scalar(
            "SELECT frequency_uid FROM karma_signal_frequency WHERE signal_uid = ?",
        )
        .bind(&signal.record_uid)
        .fetch_optional(&self.store.pool)
        .await?;
        if let Some(uid) = existing {
            return Ok(uid);
        }
        let seconds = nucleus::parse_duration(&signal.schedule)
            .filter(|value| *value > 0)
            .ok_or_else(|| invalid("Signal needs a positive sampling period"))?;
        let record = store::records::get(&self.store.pool, &signal.record_uid)
            .await?
            .ok_or_else(|| invalid("Signal is missing"))?;
        let anchor = crate::actions::parse_instant_field(&record.created_at)?;
        let cadence = nucleus::karma::Cadence::every(nucleus::karma::CadenceStep {
            seconds: u32::try_from(seconds)
                .map_err(|_| invalid("Signal sampling period is too large"))?,
            ..Default::default()
        });
        let uid = self.bind_cadence(&cadence, anchor).await?;
        store::sqlx::query("INSERT INTO karma_signal_frequency VALUES (?, ?)")
            .bind(&signal.record_uid)
            .bind(&uid)
            .execute(&self.store.pool)
            .await?;
        Ok(uid)
    }

    pub(crate) async fn reconcile_rule_frequencies(
        &self,
        config: &KarmaDeadlineDirectorConfig,
        now: DateTime<Utc>,
    ) -> Result<BTreeSet<String>, EngineError> {
        let index = Box::pin(self.rule_dependencies()).await?;
        for uid in &index.used {
            let Some(handle) = frequencies::get_handle(&self.store.pool, uid).await? else {
                continue;
            };
            let usage: Option<i64> = store::sqlx::query_scalar(
                "SELECT enabled FROM karma_frequency_usage WHERE frequency_uid = ?",
            )
            .bind(uid)
            .fetch_optional(&self.store.pool)
            .await?;
            let auto_paused: Option<bool> = store::sqlx::query_scalar(
                "SELECT auto_paused FROM karma_frequency_usage WHERE frequency_uid = ?",
            )
            .bind(uid)
            .fetch_optional(&self.store.pool)
            .await?;
            if handle.status == DefinitionStatus::Proven
                || (usage == Some(0)
                    && auto_paused == Some(true)
                    && handle.status == DefinitionStatus::Paused)
            {
                let parameters = match &handle.latest_activation_hash {
                    Some(hash) => frequencies::get_activation(&self.store.pool, hash)
                        .await?
                        .map(|activation| activation.epoch.effective_parameters().clone())
                        .unwrap_or_default(),
                    None => BTreeMap::new(),
                };
                Box::pin(self.activate_karma_frequency(
                    ActivateFrequencyInput {
                        request_id: format!("frequency-reader:{uid}:{}", handle.handle_revision),
                        frequency_uid: uid.clone(),
                        expected_handle_revision: handle.handle_revision,
                        revision_hash: handle.head_revision_hash,
                        parameter_overrides: parameters,
                        actor_person_uid: None,
                    },
                    config,
                    now,
                ))
                .await?;
            }
            store::sqlx::query("INSERT INTO karma_frequency_usage (frequency_uid, enabled) VALUES (?, 1) ON CONFLICT(frequency_uid) DO UPDATE SET enabled = 1, auto_paused = 0")
                .bind(uid).execute(&self.store.pool).await?;
        }
        let known: Vec<String> = store::sqlx::query_scalar(
            "SELECT frequency_uid FROM karma_frequency_usage WHERE enabled = 1",
        )
        .fetch_all(&self.store.pool)
        .await?;
        for uid in known {
            if index.used.contains(&uid) {
                continue;
            }
            let mut auto_paused = false;
            if let Some(handle) = frequencies::get_handle(&self.store.pool, &uid).await?
                && handle.status == DefinitionStatus::Active
            {
                frequencies::pause(
                    &self.store.pool,
                    frequencies::PauseFrequencyInput {
                        request_id: format!("frequency-unused:{uid}:{}", handle.handle_revision),
                        frequency_uid: uid.clone(),
                        expected_handle_revision: handle.handle_revision,
                        actor_person_uid: None,
                    },
                    now,
                    |_| None,
                )
                .await?;
                auto_paused = true;
            }
            store::sqlx::query("UPDATE karma_frequency_usage SET enabled = 0, auto_paused = ? WHERE frequency_uid = ?").bind(auto_paused).bind(uid).execute(&self.store.pool).await?;
        }
        Ok(index.used.clone())
    }
}

pub(crate) async fn queue_effect_tx(
    tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
    request: &str,
    kind: &str,
    payload: serde_json::Value,
    origin: &str,
    now: DateTime<Utc>,
) -> Result<(), EngineError> {
    store::sqlx::query("INSERT OR IGNORE INTO effect_queue (uid, kind, payload, origin_uid, created_at, request_id) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(nucleus::new_uid("e")).bind(kind).bind(payload.to_string()).bind(origin).bind(now.to_rfc3339()).bind(request).execute(&mut **tx).await?;
    Ok(())
}
