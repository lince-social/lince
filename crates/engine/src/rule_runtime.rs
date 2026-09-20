use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use nucleus::karma::{
    Condition, DefinitionStatus, FrequencyAst, KarmaOccurrenceSource, NodeOperation, Slug,
    TimestampMs, TriggerSource,
};
use nucleus::{Cause, Fact, NewFact};
use store::karma::frequencies::{
    self, ActivateFrequencyInput, CreateFrequencyInput, FrequencyMutationCommit,
};

use crate::karma_runtime::KarmaDeadlineDirectorConfig;
use crate::{Engine, EngineError};

const VALUE_DEPENDENCY_LIMIT: usize = 4;

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
}

tokio::task_local! {
    pub(crate) static RULE_EVENT: RuleEvent;
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
            id: format!(
                "frequency:{}:{}",
                frequency.as_deref().unwrap_or(&rule.uid),
                due.timestamp_millis()
            ),
            frequency,
            at: due,
        };
        let mut facts = Box::pin(self.execute_rule_event(rule, &event, now)).await?;
        for fact in &facts {
            let _ = self.bus.send(fact.clone());
        }
        facts.extend(
            self.run_rule_reactions(
                facts.iter().map(|fact| fact.record_uid.clone()).collect(),
                event.id,
                now,
            )
            .await?,
        );
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
        self.require_permission(actor, "record:update").await?;
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
            let mut reads = Vec::new();
            for token in Condition::parse(&condition.source)
                .map_err(|error| invalid(error.to_string()))?
                .reads()
            {
                if token.func == "freq" {
                    reads.push(self.resolve_frequency_uid(&token.slug).await?);
                } else if token.func == nucleus::expr::ASSERTION {
                    let concept = store::concepts::resolve(&self.store.pool, &token.slug)
                        .await?
                        .ok_or_else(|| invalid("condition concept is missing"))?;
                    reads.extend(
                        store::ledger::records_with_concept(&self.store.pool, &concept).await?,
                    );
                } else {
                    for name in token.slug.split('|') {
                        if let Some(record) =
                            store::records::resolve(&self.store.pool, name).await?
                        {
                            reads.push(record.uid);
                        }
                    }
                }
            }
            self.refuse_unreadable(actor, &reads).await?;
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
                if steps > 256 {
                    return Err(EngineError::Conflict {
                        code: "karma_reaction_limit",
                        message: "Rule reactions exceeded 256 evaluations; inspect the rule cycle."
                            .into(),
                    });
                }
                let rule = &index.rules[&uid];
                let event = RuleEvent {
                    id: event_id.clone(),
                    frequency: None,
                    at: now,
                };
                let facts = match Box::pin(self.execute_rule_event(rule, &event, now)).await {
                    Ok(facts) => facts,
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
        let index = Box::pin(self.rule_dependencies()).await?;
        let mut result = Vec::new();
        for _ in 0..64 {
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
            let activation_hash = match &occurrence.envelope.source {
                KarmaOccurrenceSource::ScheduleTick { tick, .. } => &tick.activation_hash,
                KarmaOccurrenceSource::ScheduleCoalesced { batch, .. } => &batch.activation_hash,
                KarmaOccurrenceSource::CalendarTick { tick, .. } => &tick.activation_hash,
                KarmaOccurrenceSource::CalendarCoalesced { batch, .. } => &batch.activation_hash,
            };
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
                    id: format!("frequency:{uid}:{}", occurrence.logical_at.as_millis()),
                    frequency: Some(uid.to_string()),
                    at: DateTime::from_timestamp_millis(occurrence.logical_at.as_millis())
                        .ok_or_else(|| invalid("invalid occurrence time"))?,
                };
                for rule_uid in index.frequencies.get(uid).into_iter().flatten() {
                    let rule = &index.rules[rule_uid];
                    let updated = crate::actions::parse_instant_field(&rule.updated_at)?;
                    if updated > event.at {
                        continue;
                    }
                    match Box::pin(self.execute_rule_event(rule, &event, now)).await {
                        Ok(facts) => {
                            for fact in &facts {
                                let _ = self.bus.send(fact.clone());
                            }
                            let changed =
                                facts.iter().map(|fact| fact.record_uid.clone()).collect();
                            result.extend(facts);
                            result.extend(
                                self.run_rule_reactions(
                                    changed,
                                    format!("reaction:{}:{rule_uid}", event.id),
                                    now,
                                )
                                .await?,
                            );
                        }
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
        Ok(result)
    }

    async fn record_rule_failure(
        &self,
        rule: &store::recurrence::Recurrence,
        event: &RuleEvent,
        error: &EngineError,
        now: DateTime<Utc>,
    ) -> Result<(), EngineError> {
        store::sqlx::query(
            "INSERT OR IGNORE INTO karma_rule_application VALUES (?, ?, ?, 'failed', ?, ?, ?, ?)",
        )
        .bind(&event.id)
        .bind(&rule.uid)
        .bind(rule.revision)
        .bind(error.to_string())
        .bind(now.to_rfc3339())
        .bind(event.at.to_rfc3339())
        .bind(&event.frequency)
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
        let current = store::recurrence::get(&self.store.pool, &rule.uid).await?;
        if current.is_none_or(|current| current.is_paused() || current.revision != rule.revision) {
            return Ok(Vec::new());
        }
        let consumed: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM karma_rule_application WHERE event_id = ? AND rule_uid = ? AND rule_revision = ?)")
            .bind(&event.id).bind(&rule.uid).bind(rule.revision).fetch_one(&self.store.pool).await?;
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
        Box::pin(self.validate_automatic_rule(
            &rule.consequences,
            rule.condition.as_ref(),
            rule.actor_uid.as_deref(),
        ))
        .await?;
        self.reject_direct_transfer_record_mutation(&rule.record_uid)
            .await?;
        self.authorize_action(
            &crate::actions::Action::SetQuantityExact {
                target: rule.record_uid.clone(),
                amount: "0".into(),
            },
            rule.actor_uid.as_deref(),
        )
        .await?;
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
                    store::sqlx::query("INSERT OR IGNORE INTO karma_rule_application VALUES (?, ?, ?, 'blocked', NULL, ?, ?, ?)")
                        .bind(&event.id).bind(&rule.uid).bind(rule.revision).bind(now.to_rfc3339()).bind(event.at.to_rfc3339()).bind(&event.frequency).execute(&self.store.pool).await?;
                    return Ok(Vec::new());
                }
            },
        };
        let signer = self.signer.lock().await.clone();
        let mut tx = store::write_tx(&self.store.pool).await?;
        let inserted = store::sqlx::query("INSERT OR IGNORE INTO karma_rule_application VALUES (?, ?, ?, 'applied', NULL, ?, ?, ?)")
            .bind(&event.id).bind(&rule.uid).bind(rule.revision).bind(now.to_rfc3339()).bind(event.at.to_rfc3339()).bind(&event.frequency).execute(&mut *tx).await?;
        if inserted.rows_affected() == 0 {
            return Ok(Vec::new());
        }
        let mut facts = Vec::new();
        for (position, consequence) in rule.consequences.iter().enumerate() {
            use nucleus::karma::Consequence;
            let current = store::sqlx::query("SELECT quantity_mantissa, quantity_scale FROM record WHERE uid = ? AND deleted_at IS NULL")
                .bind(&rule.record_uid).fetch_optional(&mut *tx).await?.ok_or_else(|| invalid("rule target is missing"))?;
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
                    queue_effect_tx(&mut tx, &format!("{}:{}:{}:{position}", event.id, rule.uid, rule.revision), "action", serde_json::json!({"action": action, "actor": rule.actor_uid, "rule": rule.uid, "revision": rule.revision}), &rule.record_uid, now).await?;
                    None
                }
                Consequence::RunCommand { command } => {
                    queue_effect_tx(&mut tx, &format!("{}:{}:{}:{position}", event.id, rule.uid, rule.revision), "command", serde_json::json!({"command": command, "actor": rule.actor_uid, "rule": rule.uid, "revision": rule.revision}), &rule.record_uid, now).await?;
                    None
                }
                Consequence::Notify { message } => {
                    queue_effect_tx(
                        &mut tx,
                        &format!("{}:{}:{}:{position}", event.id, rule.uid, rule.revision),
                        "notify",
                        serde_json::json!({"message": message, "carried": carried}),
                        &rule.record_uid,
                        now,
                    )
                    .await?;
                    None
                }
                _ => {
                    queue_effect_tx(&mut tx, &format!("{}:{}:{}:{position}", event.id, rule.uid, rule.revision), "consequence", serde_json::json!({"consequence": consequence, "carried": carried, "actor": rule.actor_uid, "rule": rule.uid, "revision": rule.revision}), &rule.record_uid, now).await?;
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
                    facts.push(fact);
                }
            }
        }
        tx.commit().await?;
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
                for token in Condition::parse(&condition.source)
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
                for token in Condition::parse(&condition.source)
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
