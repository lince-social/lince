use chrono::{DateTime, Utc};
use nucleus::karma::scheduled_change::{BoundaryInput, Purpose};
use serde::Serialize;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::{StoreError, karma_fields::invalid};

#[derive(Clone, Debug, Serialize)]
pub struct Boundary {
    pub uid: String,
    pub schedule: String,
    pub revision: i64,
    pub input: BoundaryInput,
    pub intended_at_ms: i64,
    pub frequency: String,
    pub rule: String,
    pub current: bool,
    pub status: String,
    pub event: Option<String>,
    pub rule_revision: Option<i64>,
    pub attempt: i64,
    pub reason: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Schedule {
    pub uid: String,
    pub name: String,
    pub revision: i64,
    pub cancelled: bool,
    pub actor: Option<String>,
    pub boundaries: Vec<Boundary>,
}

fn boundary(row: sqlx::sqlite::SqliteRow) -> Result<Boundary, StoreError> {
    Ok(Boundary {
        uid: row.get("uid"),
        schedule: row.get("schedule_uid"),
        revision: row.get("revision"),
        input: serde_json::from_str(&row.get::<String, _>("input"))
            .map_err(|error| invalid(&error.to_string()))?,
        intended_at_ms: row.get("intended_at_ms"),
        frequency: row.get("frequency_uid"),
        rule: row.get("rule_uid"),
        current: row.get("current"),
        status: row.get("status"),
        event: row.get("event_id"),
        rule_revision: row.get("rule_revision"),
        attempt: row.get("attempt"),
        reason: row.get("reason"),
        completed_at: row.get("completed_at"),
    })
}

pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Option<Schedule>, StoreError> {
    let mut tx = pool.begin().await?;
    let result = get_tx(&mut tx, uid).await?;
    tx.rollback().await?;
    Ok(result)
}

pub async fn get_tx(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
) -> Result<Option<Schedule>, StoreError> {
    let Some(row) = sqlx::query("SELECT * FROM karma_schedule WHERE uid = ?")
        .bind(uid)
        .fetch_optional(&mut **tx)
        .await?
    else {
        return Ok(None);
    };
    let boundaries = sqlx::query("SELECT * FROM karma_schedule_boundary WHERE schedule_uid = ? ORDER BY revision, intended_at_ms, uid")
        .bind(uid).fetch_all(&mut **tx).await?.into_iter().map(boundary).collect::<Result<_, _>>()?;
    Ok(Some(Schedule {
        uid: row.get("uid"),
        name: row.get("name"),
        revision: row.get("revision"),
        cancelled: row.get("cancelled"),
        actor: row.get("actor_uid"),
        boundaries,
    }))
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<Schedule>, StoreError> {
    let uids: Vec<String> = sqlx::query_scalar(
        "SELECT uid FROM karma_schedule ORDER BY created_at DESC, uid LIMIT 200",
    )
    .fetch_all(pool)
    .await?;
    let mut values = Vec::new();
    for uid in uids {
        if let Some(value) = get(pool, &uid).await? {
            values.push(value);
        }
    }
    Ok(values)
}

pub async fn for_rule(pool: &SqlitePool, uid: &str) -> Result<Option<Boundary>, StoreError> {
    sqlx::query("SELECT * FROM karma_schedule_boundary WHERE rule_uid = ?")
        .bind(uid)
        .fetch_optional(pool)
        .await?
        .map(boundary)
        .transpose()
}

pub async fn replay(
    tx: &mut Transaction<'_, Sqlite>,
    request: &str,
    fingerprint: &str,
) -> Result<Option<serde_json::Value>, StoreError> {
    let Some(row) =
        sqlx::query("SELECT fingerprint, result FROM karma_schedule_request WHERE request_id = ?")
            .bind(request)
            .fetch_optional(&mut **tx)
            .await?
    else {
        return Ok(None);
    };
    if row.get::<String, _>("fingerprint") != fingerprint {
        return Err(invalid(
            "This request ID was already used for a different schedule change",
        ));
    }
    Ok(Some(
        serde_json::from_str(&row.get::<String, _>("result"))
            .map_err(|error| invalid(&error.to_string()))?,
    ))
}

pub async fn remember(
    tx: &mut Transaction<'_, Sqlite>,
    request: &str,
    fingerprint: &str,
    uid: &str,
    result: &serde_json::Value,
) -> Result<(), StoreError> {
    sqlx::query("INSERT INTO karma_schedule_request VALUES (?, ?, ?, ?)")
        .bind(request)
        .bind(fingerprint)
        .bind(uid)
        .bind(result.to_string())
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn pause_rule(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    request: &str,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    let changed = sqlx::query("UPDATE recurrence SET state = 'paused', revision = revision + 1, updated_at = ? WHERE uid = ? AND state != 'paused'")
        .bind(now.to_rfc3339()).bind(uid).execute(&mut **tx).await?;
    if changed.rows_affected() != 0 {
        sqlx::query("INSERT INTO recurrence_revision(uid, recurrence_uid, revision, kind, consequences_json, condition_src, gate, carry, note, cadence_json, anchor_at, state, request_id, actor_uid, at, name, slug, bindings_json) SELECT ?, uid, revision, 'paused', consequences_json, condition_src, gate, carry, note, cadence_json, anchor_at, state, ?, actor_uid, updated_at, name, slug, bindings_json FROM recurrence WHERE uid = ?")
            .bind(nucleus::new_uid("recr")).bind(request).bind(uid).execute(&mut **tx).await?;
    }
    Ok(())
}

pub async fn reflect_rule_edit(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    crate::karma_stages::invalidate_children_tx(tx, uid, now).await?;
    crate::karma_commands::invalidate_children_tx(tx, uid).await?;
    let Some(row) = sqlx::query("SELECT b.* FROM karma_schedule_boundary b JOIN karma_schedule s ON s.uid = b.schedule_uid WHERE b.rule_uid = ? AND b.current = 1 AND s.cancelled = 0 AND b.status NOT IN ('retired', 'expired')")
        .bind(uid).fetch_optional(&mut **tx).await?
    else {
        return Ok(());
    };
    let boundary = boundary(row)?;
    let rule = sqlx::query("SELECT r.record_uid, r.consequences_json, r.state, record.slug FROM recurrence r JOIN record ON record.uid = r.record_uid WHERE r.uid = ?")
        .bind(uid).fetch_one(&mut **tx).await?;
    let mut input = boundary.input;
    input.consequences = serde_json::from_str(&rule.get::<String, _>("consequences_json"))
        .map_err(|error| invalid(&error.to_string()))?;
    let target: String = input.consequences.iter().find_map(nucleus::karma::Consequence::transfer_target).map(str::to_owned).unwrap_or_else(|| rule.get("record_uid"));
    if input.target != target
        && Some(input.target.as_str()) != rule.get::<Option<String>, _>("slug").as_deref()
    {
        input.target = target;
    }
    sqlx::query("UPDATE karma_schedule_boundary SET input = ?, reason = CASE WHEN status = 'pending' THEN ? ELSE reason END WHERE uid = ?")
        .bind(serde_json::to_string(&input).map_err(|error| invalid(&error.to_string()))?)
        .bind(if rule.get::<String, _>("state") == "paused" {Some("The underlying Rule is paused")} else {None})
        .bind(&boundary.uid).execute(&mut **tx).await?;
    sqlx::query("UPDATE karma_schedule SET revision = revision + 1, updated_at = ? WHERE uid = ?")
        .bind(now.to_rfc3339())
        .bind(&boundary.schedule)
        .execute(&mut **tx)
        .await?;
    let current = get_tx(tx, &boundary.schedule)
        .await?
        .ok_or_else(|| invalid("Scheduled change disappeared"))?;
    let mut inputs: Vec<_> = current
        .boundaries
        .into_iter()
        .filter(|value| value.current)
        .map(|value| value.input)
        .collect();
    inputs.sort_by_key(|input| usize::from(input.purpose == Purpose::End));
    sqlx::query("INSERT INTO karma_schedule_revision VALUES (?, ?, ?, ?)")
        .bind(&current.uid)
        .bind(current.revision)
        .bind(serde_json::to_string(&inputs).map_err(|error| invalid(&error.to_string()))?)
        .bind(now.to_rfc3339())
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn invalidate(
    tx: &mut Transaction<'_, Sqlite>,
    value: &Boundary,
    status: &str,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    crate::karma_commands::invalidate_children_tx(tx, &value.rule).await?;
    pause_rule(
        tx,
        &value.rule,
        &format!("schedule:{}:{status}", value.uid),
        now,
    )
    .await?;
    let effects = sqlx::query("SELECT uid, attempts FROM effect_queue WHERE json_extract(payload, '$.rule') = ? AND status = 'queued'")
        .bind(&value.rule).fetch_all(&mut **tx).await?;
    for effect in effects {
        let uid: String = effect.get("uid");
        sqlx::query("INSERT INTO karma_effect_outcome VALUES (?, ?, 'cancelled', ?, ?)")
            .bind(&uid)
            .bind(effect.get::<i64, _>("attempts") + 1)
            .bind(status)
            .bind(now.to_rfc3339())
            .execute(&mut **tx)
            .await?;
        sqlx::query("UPDATE effect_queue SET status = 'cancelled', attempts = attempts + 1, result = ?, finished_at = ? WHERE uid = ?")
            .bind(status).bind(now.to_rfc3339()).bind(&uid).execute(&mut **tx).await?;
    }
    sqlx::query("UPDATE karma_schedule_boundary SET current = 0, status = ?, reason = ?, completed_at = ? WHERE uid = ?")
        .bind(status).bind(status).bind(now.to_rfc3339()).bind(&value.uid).execute(&mut **tx).await?;
    Ok(())
}

pub async fn admit(
    pool: &SqlitePool,
    rule: &str,
    frequency: Option<&str>,
    intended_ms: i64,
    now: DateTime<Utc>,
) -> Result<bool, StoreError> {
    let Some(value) = for_rule(pool, rule).await? else {
        return Ok(true);
    };
    let group = get(pool, &value.schedule)
        .await?
        .ok_or_else(|| invalid("Scheduled change is missing"))?;
    if group.cancelled
        || !value.current
        || !matches!(
            value.status.as_str(),
            "pending" | "failed" | "blocked" | "applied"
        )
        || frequency != Some(value.frequency.as_str())
        || intended_ms != value.intended_at_ms
        || now.timestamp_millis() < intended_ms
    {
        return Ok(false);
    }
    let applied: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM karma_rule_application WHERE rule_uid = ? AND status = 'applied')",
    )
    .bind(rule)
    .fetch_one(pool)
    .await?;
    if applied {
        return Ok(false);
    }
    if value.input.purpose == Purpose::Start
        && group.boundaries.iter().any(|end| {
            end.current
                && end.input.purpose == Purpose::End
                && now.timestamp_millis() >= end.intended_at_ms
        })
    {
        let mut tx = crate::write_tx(pool).await?;
        pause_rule(
            &mut tx,
            rule,
            &format!("schedule:{}:expired", value.uid),
            now,
        )
        .await?;
        sqlx::query("UPDATE karma_schedule_boundary SET status = 'expired', reason = 'The range ended before its start could run', completed_at = ? WHERE uid = ?")
            .bind(now.to_rfc3339()).bind(&value.uid).execute(&mut *tx).await?;
        tx.commit().await?;
        return Ok(false);
    }
    Ok(true)
}

pub async fn refresh(pool: &SqlitePool, now: DateTime<Utc>) -> Result<bool, StoreError> {
    let values = sqlx::query("SELECT * FROM karma_schedule_boundary WHERE current = 1 AND status IN ('pending', 'failed', 'blocked', 'applied')")
        .fetch_all(pool).await?.into_iter().map(boundary).collect::<Result<Vec<_>, _>>()?;
    let mut changed = false;
    for value in values {
        let Some(application) = sqlx::query(
            "SELECT * FROM karma_rule_application WHERE rule_uid = ? ORDER BY rowid DESC LIMIT 1",
        )
        .bind(&value.rule)
        .fetch_optional(pool)
        .await?
        else {
            continue;
        };
        if application.get::<i64, _>("attempt") < value.attempt {
            continue;
        }
        let mut status: String = application.get("status");
        let mut reason: Option<String> = application.get("reason");
        if status == "applied" {
            let effects = sqlx::query("SELECT status, result FROM effect_queue WHERE json_extract(payload, '$.rule') = ? AND json_extract(payload, '$.occurrence.event_id') = ?")
                .bind(&value.rule).bind(application.get::<String, _>("event_id")).fetch_all(pool).await?;
            if let Some(failure) = effects.iter().find(|effect| {
                matches!(
                    effect.get::<String, _>("status").as_str(),
                    "failed" | "uncertain"
                )
            }) {
                status = "failed".into();
                reason = failure.get("result");
            } else if effects
                .iter()
                .all(|effect| effect.get::<String, _>("status") == "done")
            {
                let commands = sqlx::query("SELECT c.status, c.last_error FROM karma_transfer_command k JOIN transfer_remote_command c ON c.command_uid = k.command_uid WHERE k.rule_uid = ? AND json_extract(k.origin, '$.occurrence.event_id') = ? AND k.cancelled = 0")
                    .bind(&value.rule).bind(application.get::<String, _>("event_id")).fetch_all(pool).await?;
                if let Some(rejected) = commands.iter().find(|command| command.get::<String, _>("status") == "rejected") {
                    status = "failed".into();
                    reason = rejected.get("last_error");
                } else if commands.iter().all(|command| command.get::<String, _>("status") == "accepted") {
                    status = "retired".into();
                }
            }
        }
        let event: String = application.get("event_id");
        let attempt: i64 = application.get("attempt");
        let revision: i64 = application.get("rule_revision");
        if status != value.status
            || value.event.as_ref() != Some(&event)
            || attempt != value.attempt
        {
            let mut tx = crate::write_tx(pool).await?;
            if status == "retired" {
                pause_rule(
                    &mut tx,
                    &value.rule,
                    &format!("schedule:{}:retired", value.uid),
                    now,
                )
                .await?;
            }
            sqlx::query("UPDATE karma_schedule_boundary SET status = ?, reason = ?, event_id = ?, rule_revision = ?, attempt = ?, completed_at = ? WHERE uid = ?")
                .bind(&status).bind(reason).bind(event).bind(revision).bind(attempt).bind(if status == "retired" { Some(now.to_rfc3339()) } else { None }).bind(&value.uid).execute(&mut *tx).await?;
            tx.commit().await?;
            changed = true;
        }
    }
    Ok(changed)
}
