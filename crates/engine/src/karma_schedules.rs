use chrono::{DateTime, Utc};
use nucleus::karma::rule_field::{RuleConsequence, RuleFieldKind, RuleIdentity};
use nucleus::karma::scheduled_change::{BoundaryInput, DateInput, Purpose};
use nucleus::karma::{
    Cadence, Carry, ConditionBinding, Gate, LocalTimeResolution, ReferenceKind, Slug, TimestampMs,
    TypedUid,
};
use store::karma_fields::{Field, Selection};
use store::karma_schedules::{self as schedules, Schedule};
use store::recurrence::{Recurrence, RuleCondition};
use store::sqlx::Row;

use crate::{Engine, EngineError, actions::ActionOutcome};

fn invalid(message: impl ToString) -> EngineError {
    EngineError::Conflict {
        code: "karma_schedule_invalid",
        message: message.to_string(),
    }
}

fn fingerprint(value: &serde_json::Value) -> Result<String, EngineError> {
    Ok(
        nucleus::karma::canonical_hash("lince.karma-schedule-request.v1", value)
            .map_err(invalid)?
            .as_str()
            .into(),
    )
}

fn request(value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.len() > 200 {
        return Err(invalid("Use a request ID of 1–200 bytes"));
    }
    Ok(())
}

impl Engine {
    pub(crate) async fn preview_schedule_dates(
        &self,
        date: nucleus::karma::CivilDateTime,
        timezone: nucleus::karma::TimeZoneId,
        gap: nucleus::karma::GapPolicy,
        fold: nucleus::karma::FoldPolicy,
        actor: Option<&str>,
    ) -> Result<serde_json::Value, EngineError> {
        self.require_permission(actor, "frequency:read").await?;
        let config = self
            .configured_karma_runtime()
            .or_else(|error| match error {
                EngineError::Conflict {
                    code: "karma_runtime_unconfigured",
                    ..
                } => crate::karma_runtime::KarmaDeadlineDirectorConfig::for_host(
                    "schedule-date-preview".into(),
                ),
                other => Err(other),
            })?;
        let mut choices = Vec::new();
        for revision in config.provider_revisions() {
            let input = DateInput::Local {
                date,
                timezone: timezone.clone(),
                tzdb: revision.clone(),
                gap,
                fold,
            };
            if let Ok(at) = self.schedule_date(&input, nucleus::execution::now()) {
                choices.push(serde_json::json!({"date":input, "at_ms":at.timestamp_millis(), "rules":revision.version.as_str()}));
            }
        }
        if choices.is_empty() {
            return Err(invalid(
                "No installed timezone rules resolve this date with the selected gap/fold choices",
            ));
        }
        Ok(serde_json::Value::Array(choices))
    }
    async fn schedule_access(
        &self,
        value: &Schedule,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        for boundary in &value.boundaries {
            let rule = store::recurrence::get(&self.store.pool, &boundary.rule)
                .await?
                .ok_or_else(|| invalid("Schedule Rule is missing"))?;
            self.refuse_unreadable_karma_inputs(actor, &[crate::karma_transfer_effects::target(&rule).into()]).await?;
        }
        Ok(())
    }

    pub(crate) async fn inspect_karma_schedules(
        &self,
        uid: Option<&str>,
        actor: Option<&str>,
    ) -> Result<serde_json::Value, EngineError> {
        self.require_permission(actor, "frequency:read").await?;
        let values = match uid {
            Some(uid) => vec![
                schedules::get(&self.store.pool, uid)
                    .await?
                    .ok_or_else(|| invalid("Schedule not found"))?,
            ],
            None => schedules::list(&self.store.pool).await?,
        };
        let mut readable = Vec::new();
        for value in values {
            match self.schedule_access(&value, actor).await {
                Ok(()) => readable.push(value),
                Err(error) if uid.is_some() => return Err(error),
                Err(_) => {}
            }
        }
        serde_json::to_value(readable).map_err(EngineError::Json)
    }

    fn schedule_date(
        &self,
        input: &DateInput,
        now: DateTime<Utc>,
    ) -> Result<DateTime<Utc>, EngineError> {
        let ms = match input {
            DateInput::Instant { at_ms } => *at_ms,
            DateInput::After { milliseconds } => now
                .timestamp_millis()
                .checked_add(i64::try_from(*milliseconds).map_err(invalid)?)
                .ok_or_else(|| invalid("Elapsed date is out of range"))?,
            DateInput::Local {
                date,
                timezone,
                tzdb,
                gap,
                fold,
            } => {
                let config = self
                    .configured_karma_runtime()
                    .or_else(|error| match error {
                        EngineError::Conflict {
                            code: "karma_runtime_unconfigured",
                            ..
                        } => crate::karma_runtime::KarmaDeadlineDirectorConfig::for_host(
                            "schedule-date".into(),
                        ),
                        other => Err(other),
                    })?;
                let provider = config.provider(tzdb).ok_or_else(|| {
                    invalid("Install the selected timezone artifact before saving this local date")
                })?;
                match provider.resolve_local(timezone, *date).map_err(invalid)? {
                    LocalTimeResolution::Unique { instant } => instant.as_millis(),
                    LocalTimeResolution::Gap {
                        first_valid_after, ..
                    } if *gap == nucleus::karma::GapPolicy::ShiftForward => {
                        first_valid_after.as_millis()
                    }
                    LocalTimeResolution::Fold { first, .. }
                        if *fold == nucleus::karma::FoldPolicy::First =>
                    {
                        first.as_millis()
                    }
                    LocalTimeResolution::Fold { second, .. }
                        if *fold == nucleus::karma::FoldPolicy::Second =>
                    {
                        second.as_millis()
                    }
                    LocalTimeResolution::Gap { .. } => {
                        return Err(invalid(
                            "This local date does not exist; choose another date or shift forward",
                        ));
                    }
                    LocalTimeResolution::Fold { .. } => {
                        return Err(invalid(
                            "This local date occurs twice; select its first or second occurrence",
                        ));
                    }
                }
            }
        };
        DateTime::from_timestamp_millis(ms).ok_or_else(|| invalid("Date is out of range"))
    }

    pub(crate) async fn save_karma_schedule(
        &self,
        uid: Option<String>,
        expected: Option<i64>,
        name: String,
        inputs: Vec<BoundaryInput>,
        request_id: String,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        Box::pin(self.save_karma_schedule_with_stage(uid, expected, name, inputs, request_id, actor, now, None)).await
    }

    pub(crate) async fn save_karma_schedule_with_stage(
        &self,
        uid: Option<String>,
        expected: Option<i64>,
        name: String,
        inputs: Vec<BoundaryInput>,
        request_id: String,
        actor: Option<&str>,
        now: DateTime<Utc>,
        stage: Option<&store::karma_stages::Origin>,
    ) -> Result<ActionOutcome, EngineError> {
        request(&request_id)?;
        self.require_permission(
            actor,
            if uid.is_some() {
                "frequency:update"
            } else {
                "frequency:create"
            },
        )
        .await?;
        if name.trim().is_empty() || name.len() > 256 || name.chars().any(char::is_control) {
            return Err(invalid("Use a schedule name of 1–256 bytes"));
        }
        let purposes: Vec<_> = inputs.iter().map(|input| input.purpose).collect();
        if purposes != [Purpose::Once] && purposes != [Purpose::Start, Purpose::End] {
            return Err(invalid("Use one change or an ordered start/end pair"));
        }
        let hash = fingerprint(
            &serde_json::json!({"action":"save", "uid":uid, "revision":expected, "name":name, "inputs":inputs, "actor":actor, "stage":stage}),
        )?;
        let mut read = self.store.pool.begin().await?;
        let replay = schedules::replay(&mut read, &request_id, &hash).await?;
        read.rollback().await?;
        if let Some(data) = replay {
            let value = schedules::get(
                &self.store.pool,
                data["uid"]
                    .as_str()
                    .ok_or_else(|| invalid("Invalid schedule receipt"))?,
            )
            .await?
            .ok_or_else(|| invalid("Schedule not found"))?;
            self.schedule_access(&value, actor).await?;
            return Ok(ActionOutcome {
                created: Some(value.uid),
                data: Some(data),
                ..Default::default()
            });
        }
        let old = match &uid {
            Some(uid) => Some(
                schedules::get(&self.store.pool, uid)
                    .await?
                    .ok_or_else(|| invalid("Schedule not found"))?,
            ),
            None => None,
        };
        if let Some(old) = &old {
            self.schedule_access(old, actor).await?;
            if expected != Some(old.revision) || old.cancelled {
                return Err(invalid(
                    "Schedule changed or was cancelled; refresh before saving",
                ));
            }
            if old
                .boundaries
                .iter()
                .filter(|boundary| boundary.current)
                .any(|boundary| !purposes.contains(&boundary.input.purpose))
            {
                return Err(invalid(
                    "Keep this schedule's one-change or range shape; create a new schedule to change it",
                ));
            }
        } else if expected.is_some() {
            return Err(invalid("A new schedule has no previous revision"));
        }
        let mut prepared = Vec::new();
        for input in &inputs {
            let previous = old.as_ref().and_then(|old| {
                old.boundaries
                    .iter()
                    .find(|boundary| boundary.current && boundary.input.purpose == input.purpose)
            });
            if let Some(previous) = previous
                && previous.input == *input
            {
                prepared.push((
                    input.clone(),
                    previous.intended_at_ms,
                    previous.input.target.clone(),
                    None,
                ));
                continue;
            }
            if let Some(previous) = previous {
                let applied: bool = store::sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM karma_rule_application WHERE rule_uid = ? AND status = 'applied')",
                )
                .bind(&previous.rule)
                .fetch_one(&self.store.pool)
                .await?;
                if applied || matches!(previous.status.as_str(), "retired" | "expired") {
                    return Err(invalid(
                        "This boundary already ran or expired; inspect or retry its pending outcome, and create a new schedule for another change",
                    ));
                }
            }
            let date = match previous.filter(|previous| previous.input.date == input.date) {
                Some(previous) => DateTime::from_timestamp_millis(previous.intended_at_ms)
                    .ok_or_else(|| invalid("Saved date is out of range"))?,
                None => self.schedule_date(&input.date, now)?,
            };
            if date.timestamp_millis() < now.timestamp_millis() {
                return Err(invalid(
                    "Choose a future date; overdue dates belong to recovery of saved work",
                ));
            }
            let consequences = self
                .bind_transfer_effects(input.consequences.clone(), &mut Vec::new(), &[])
                .await?;
            let target = match self.transfer_rule_anchor(&consequences, actor).await? {
                Some(anchor) => {
                    let logical = self.resolve_karma_transfer(&input.target).await?;
                    if consequences.iter().any(|effect| effect.transfer_target() != Some(logical.as_str())) {
                        return Err(invalid("Scheduled Transfer consequences must name the selected target"));
                    }
                    anchor
                }
                None => {
                    let target = if let Some(previous) = previous.filter(|previous| previous.input.target == input.target) {
                        store::recurrence::get(&self.store.pool, &previous.rule).await?.ok_or_else(|| invalid("Schedule Rule is missing"))?.record_uid
                    } else { self.resolve(&input.target).await? };
                    self.authorize_rule_target(&target, actor).await?;
                    target
                }
            };
            Box::pin(self.validate_automatic_rule(&consequences, None, actor)).await?;
            prepared.push((
                input.clone(),
                date.timestamp_millis(),
                target,
                Some(consequences),
            ));
        }
        if prepared.len() == 2 && prepared[0].1 >= prepared[1].1 {
            return Err(invalid("The start must precede the end"));
        }
        let uid = uid.unwrap_or_else(|| nucleus::new_uid("ks"));
        let revision = old.as_ref().map_or(1, |old| old.revision + 1);
        let signer = self.signer.lock().await.clone();
        let mut tx = store::write_tx(&self.store.pool).await?;
        if let Some(data) = schedules::replay(&mut tx, &request_id, &hash).await? {
            return Ok(ActionOutcome {
                created: Some(uid),
                data: Some(data),
                ..Default::default()
            });
        }
        if let Some(old) = &old {
            let current = schedules::get_tx(&mut tx, &uid)
                .await?
                .ok_or_else(|| invalid("Schedule disappeared"))?;
            if current.revision != old.revision || current.cancelled {
                return Err(invalid("Schedule changed; refresh before saving"));
            }
            sqlx_update_group(&mut tx, &uid, &name, revision, now).await?;
            for boundary in &old.boundaries {
                if boundary.current
                    && !prepared.iter().any(|(input, _, _, consequences)| {
                        input.purpose == boundary.input.purpose && consequences.is_none()
                    })
                {
                    schedules::invalidate(&mut tx, boundary, "superseded", now).await?;
                }
            }
        } else {
            store::sqlx::query("INSERT INTO karma_schedule(uid, name, revision, actor_uid, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)")
                .bind(&uid).bind(name.trim()).bind(revision).bind(actor).bind(now.to_rfc3339()).bind(now.to_rfc3339()).execute(&mut *tx).await?;
        }
        store::sqlx::query("INSERT INTO karma_schedule_revision VALUES (?, ?, ?, ?)")
            .bind(&uid)
            .bind(revision)
            .bind(serde_json::to_string(&inputs).map_err(EngineError::Json)?)
            .bind(now.to_rfc3339())
            .execute(&mut *tx)
            .await?;
        let mut facts = Vec::new();
        for (input, ms, target, consequences) in prepared {
            let Some(consequences) = consequences else {
                continue;
            };
            let boundary = nucleus::new_uid("ksb");
            let frequency_uid = nucleus::new_uid("r");
            let slug = format!(
                "schedule.{}.{}.{}",
                uid.to_ascii_lowercase().replace('_', "-"),
                revision,
                input.purpose.as_str()
            );
            let ast = nucleus::karma::simple_frequency::frequency_from_cadence(
                Slug::new(&slug).map_err(invalid)?,
                format!("{}: {}", name.trim(), input.purpose.as_str()),
                &Cadence::once(),
                TimestampMs::from_millis(ms).map_err(invalid)?,
            )
            .map_err(invalid)?;
            let commit = store::karma::frequencies::create_identified_tx(
                &mut tx,
                store::karma::frequencies::CreateFrequencyInput {
                    request_id: format!("schedule:{boundary}"),
                    frequency: ast,
                    owner_person_uid: actor.map(str::to_owned),
                    actor_person_uid: actor.map(str::to_owned),
                },
                Some(&frequency_uid),
                None,
                now,
                |hash| signer.as_ref().map(|signer| signer.sign_hash(hash)),
            )
            .await?;
            if let store::karma::frequencies::FrequencyMutationCommit::Committed { fact, .. } =
                commit
            {
                facts.push(fact);
            }
            let source = format!("freq(@{slug})");
            let rule = Recurrence {
                uid: nucleus::new_uid("rec"),
                record_uid: target.clone(),
                consequences: consequences.clone(),
                condition: Some(RuleCondition {
                    source: source.clone(),
                    bindings: vec![ConditionBinding {
                        reading: "freq".into(),
                        authored: slug.clone(),
                        target: TypedUid::new(ReferenceKind::Frequency, &frequency_uid)
                            .map_err(invalid)?,
                    }],
                    gate: Gate::NonZero,
                    carry: Carry::Value,
                }),
                note: None,
                cadence: Cadence::once(),
                anchor_at: DateTime::from_timestamp_millis(ms)
                    .ok_or_else(|| invalid("Date is out of range"))?
                    .to_rfc3339(),
                state: "active".into(),
                revision: 0,
                actor_uid: actor.map(str::to_owned),
                created_at: now.to_rfc3339(),
                updated_at: now.to_rfc3339(),
            };
            let sources = [
                source,
                Gate::NonZero.as_text(),
                RuleConsequence {
                    target: consequences.iter().find_map(nucleus::karma::Consequence::transfer_target).unwrap_or(&target).into(),
                    consequences: consequences.as_slice().to_vec(),
                }
                .as_text(),
            ];
            let fields: Vec<_> = RuleFieldKind::ALL
                .into_iter()
                .zip(sources)
                .map(|(kind, source)| Selection {
                    field: Field {
                        uid: nucleus::new_uid("kf"),
                        kind,
                        source,
                        revision: 1,
                    },
                    fresh: true,
                })
                .collect();
            store::karma_fields::save_rule_tx(
                &mut tx,
                &rule,
                &fields,
                Some(&RuleIdentity {
                    name: format!("{}: {}", name.trim(), input.purpose.as_str()),
                    slug: format!("{slug}.rule"),
                }),
                &format!("schedule-rule:{boundary}"),
                now,
            )
            .await?;
            store::sqlx::query("INSERT INTO karma_schedule_boundary(uid, schedule_uid, revision, purpose, input, intended_at_ms, frequency_uid, rule_uid, current, status) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, 'pending')")
                .bind(&boundary).bind(&uid).bind(revision).bind(input.purpose.as_str()).bind(serde_json::to_string(&input).map_err(EngineError::Json)?).bind(ms).bind(frequency_uid).bind(rule.uid).execute(&mut *tx).await?;
            if let Some(stage) = stage { store::karma_stages::save_tx(&mut tx, &boundary, stage).await?; }
        }
        let data = serde_json::to_value(
            schedules::get_tx(&mut tx, &uid)
                .await?
                .ok_or_else(|| invalid("Schedule disappeared"))?,
        )
        .map_err(EngineError::Json)?;
        schedules::remember(&mut tx, &request_id, &hash, &uid, &data).await?;
        tx.commit().await?;
        for fact in &facts {
            let _ = self.bus.send(fact.clone());
        }
        Ok(ActionOutcome {
            created: Some(uid),
            facts,
            data: Some(data),
            ..Default::default()
        })
    }

    pub(crate) async fn cancel_karma_schedule(
        &self,
        uid: String,
        expected: i64,
        request_id: String,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        request(&request_id)?;
        self.require_permission(actor, "frequency:update").await?;
        let value = schedules::get(&self.store.pool, &uid)
            .await?
            .ok_or_else(|| invalid("Schedule not found"))?;
        self.schedule_access(&value, actor).await?;
        let hash = fingerprint(
            &serde_json::json!({"action":"cancel", "uid":uid, "revision":expected, "actor":actor}),
        )?;
        let mut tx = store::write_tx(&self.store.pool).await?;
        if let Some(data) = schedules::replay(&mut tx, &request_id, &hash).await? {
            return Ok(ActionOutcome {
                data: Some(data),
                ..Default::default()
            });
        }
        let value = schedules::get_tx(&mut tx, &uid)
            .await?
            .ok_or_else(|| invalid("Schedule disappeared"))?;
        if value.revision != expected || value.cancelled {
            return Err(invalid("Schedule changed; refresh before cancelling"));
        }
        for boundary in &value.boundaries {
            if boundary.current && !matches!(boundary.status.as_str(), "retired" | "expired") {
                schedules::invalidate(&mut tx, boundary, "cancelled", now).await?;
            }
        }
        store::sqlx::query("UPDATE karma_schedule SET cancelled = 1, revision = revision + 1, updated_at = ? WHERE uid = ?").bind(now.to_rfc3339()).bind(&uid).execute(&mut *tx).await?;
        let data = serde_json::to_value(
            schedules::get_tx(&mut tx, &uid)
                .await?
                .ok_or_else(|| invalid("Schedule disappeared"))?,
        )
        .map_err(EngineError::Json)?;
        schedules::remember(&mut tx, &request_id, &hash, &uid, &data).await?;
        tx.commit().await?;
        Ok(ActionOutcome {
            data: Some(data),
            ..Default::default()
        })
    }

    pub(crate) async fn retry_karma_schedule(
        &self,
        uid: String,
        expected: i64,
        boundary_uid: String,
        request_id: String,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        request(&request_id)?;
        self.require_permission(actor, "frequency:update").await?;
        let value = schedules::get(&self.store.pool, &uid)
            .await?
            .ok_or_else(|| invalid("Schedule not found"))?;
        self.schedule_access(&value, actor).await?;
        let hash = fingerprint(
            &serde_json::json!({"action":"retry", "uid":uid, "revision":expected, "boundary":boundary_uid, "actor":actor}),
        )?;
        let mut tx = store::write_tx(&self.store.pool).await?;
        if let Some(data) = schedules::replay(&mut tx, &request_id, &hash).await? {
            return Ok(ActionOutcome {
                data: Some(data),
                ..Default::default()
            });
        }
        let current = schedules::get_tx(&mut tx, &uid)
            .await?
            .ok_or_else(|| invalid("Schedule disappeared"))?;
        if current.cancelled || current.revision != expected {
            return Err(invalid("Schedule changed; refresh before retrying"));
        }
        let boundary = current
            .boundaries
            .iter()
            .find(|boundary| boundary.uid == boundary_uid && boundary.current)
            .ok_or_else(|| invalid("Current boundary not found"))?
            .clone();
        let rule_state = store::sqlx::query("SELECT revision, state FROM recurrence WHERE uid = ?")
            .bind(&boundary.rule)
            .fetch_one(&mut *tx)
            .await?;
        if boundary.rule_revision != Some(rule_state.get("revision"))
            || rule_state.get::<String, _>("state") == "paused"
        {
            return Err(invalid(
                "The Rule changed or was paused after the failure; edit the schedule instead of retrying old work",
            ));
        }
        if boundary.event.is_none() {
            return Err(invalid("Failed boundary has no occurrence"));
        }
        if !matches!(boundary.status.as_str(), "failed" | "blocked") {
            return Err(invalid("Only a failed or blocked boundary can be retried"));
        }
        let effects = store::sqlx::query(
            "SELECT uid, status FROM effect_queue WHERE json_extract(payload, '$.rule') = ?",
        )
        .bind(&boundary.rule)
        .fetch_all(&mut *tx)
        .await?;
        if effects.iter().any(|effect| {
            matches!(
                effect.get::<String, _>("status").as_str(),
                "uncertain" | "running" | "queued"
            )
        }) {
            return Err(invalid(
                "An interrupted effect has an uncertain result; inspect it before retrying",
            ));
        }
        let application_failed = effects.is_empty();
        if application_failed {
            store::sqlx::query("UPDATE karma_schedule_boundary SET attempt = attempt + 1, status = 'pending', reason = NULL WHERE uid = ?").bind(&boundary.uid).execute(&mut *tx).await?;
        } else {
            store::sqlx::query("UPDATE effect_queue SET status = 'queued', finished_at = NULL, result = NULL WHERE json_extract(payload, '$.rule') = ? AND status = 'failed'").bind(&boundary.rule).execute(&mut *tx).await?;
            store::sqlx::query("UPDATE karma_schedule_boundary SET status = 'applied', reason = NULL WHERE uid = ?").bind(&boundary.uid).execute(&mut *tx).await?;
        }
        let data = serde_json::json!({"uid":uid, "boundary":boundary.uid, "event":boundary.event, "retry":true});
        schedules::remember(&mut tx, &request_id, &hash, &uid, &data).await?;
        tx.commit().await?;
        let mut outcome = ActionOutcome {
            data: Some(data),
            ..Default::default()
        };
        if application_failed {
            outcome.facts =
                Box::pin(crate::as_one_firing(self.recover_schedule_retries(now))).await?;
        }
        schedules::refresh(&self.store.pool, now).await?;
        self.effects_changed
            .send_modify(|revision| *revision = revision.wrapping_add(1));
        Ok(outcome)
    }
}

async fn sqlx_update_group(
    tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
    uid: &str,
    name: &str,
    revision: i64,
    now: DateTime<Utc>,
) -> Result<(), EngineError> {
    store::sqlx::query(
        "UPDATE karma_schedule SET name = ?, revision = ?, updated_at = ? WHERE uid = ?",
    )
    .bind(name.trim())
    .bind(revision)
    .bind(now.to_rfc3339())
    .bind(uid)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
