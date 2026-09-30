use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::{StoreError, karma_fields::invalid, karma_schedules};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Origin {
    pub parent_rule: String,
    pub parent_revision: i64,
    pub position: usize,
    pub change_uid: String,
    pub due_at_ms: i64,
    pub evaluation: serde_json::Value,
    pub occurrence: nucleus::simulation::RuleOccurrence,
}

pub async fn for_rule(pool: &SqlitePool, rule: &str) -> Result<Option<Origin>, StoreError> {
    let value: Option<String> = sqlx::query_scalar("SELECT s.origin FROM karma_transfer_stage s JOIN karma_schedule_boundary b ON b.uid = s.boundary_uid WHERE b.rule_uid = ?")
        .bind(rule).fetch_optional(pool).await?;
    value
        .map(|value| serde_json::from_str(&value).map_err(|error| invalid(&error.to_string())))
        .transpose()
}

pub async fn existing(pool: &SqlitePool, origin: &Origin) -> Result<Option<String>, StoreError> {
    Ok(sqlx::query_scalar("SELECT b.schedule_uid FROM karma_transfer_stage s JOIN karma_schedule_boundary b ON b.uid = s.boundary_uid WHERE s.parent_rule_uid = ? AND s.parent_revision = ? AND s.position = ? AND s.change_uid = ?")
        .bind(&origin.parent_rule).bind(origin.parent_revision).bind(i64::try_from(origin.position).map_err(|error| invalid(&error.to_string()))?).bind(&origin.change_uid).fetch_optional(pool).await?)
}

pub async fn save_tx(
    tx: &mut Transaction<'_, Sqlite>,
    boundary: &str,
    origin: &Origin,
) -> Result<(), StoreError> {
    let parent = sqlx::query("SELECT revision, state FROM recurrence WHERE uid = ?")
        .bind(&origin.parent_rule)
        .fetch_optional(&mut **tx)
        .await?;
    if parent.is_none_or(|parent| {
        parent.get::<i64, _>("revision") != origin.parent_revision
            || parent.get::<String, _>("state") == "paused"
    }) {
        return Err(invalid(
            "The parent Rule changed before its stage was saved",
        ));
    }
    sqlx::query("INSERT INTO karma_transfer_stage(boundary_uid, parent_rule_uid, parent_revision, position, change_uid, origin) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(boundary).bind(&origin.parent_rule).bind(origin.parent_revision).bind(i64::try_from(origin.position).map_err(|error| invalid(&error.to_string()))?).bind(&origin.change_uid).bind(serde_json::to_string(origin).map_err(|error| invalid(&error.to_string()))?).execute(&mut **tx).await?;
    Ok(())
}

pub async fn invalidate_children_tx(
    tx: &mut Transaction<'_, Sqlite>,
    parent: &str,
    now: DateTime<Utc>,
) -> Result<bool, StoreError> {
    let schedules: Vec<String> = sqlx::query_scalar("SELECT DISTINCT b.schedule_uid FROM karma_transfer_stage s JOIN karma_schedule_boundary b ON b.uid = s.boundary_uid WHERE s.parent_rule_uid = ? AND b.current = 1 AND b.status NOT IN ('retired', 'expired')")
        .bind(parent).fetch_all(&mut **tx).await?;
    for uid in &schedules {
        if let Some(schedule) = karma_schedules::get_tx(tx, uid).await? {
            for boundary in &schedule.boundaries {
                if boundary.current && !matches!(boundary.status.as_str(), "retired" | "expired") {
                    karma_schedules::invalidate(tx, boundary, "parent-changed", now).await?;
                }
            }
            sqlx::query("UPDATE karma_schedule SET cancelled = 1, revision = revision + 1, updated_at = ? WHERE uid = ?")
                .bind(now.to_rfc3339()).bind(uid).execute(&mut **tx).await?;
        }
    }
    Ok(!schedules.is_empty())
}

pub async fn cancel(
    pool: &SqlitePool,
    rule: &str,
    reason: &str,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    let Some(boundary) = karma_schedules::for_rule(pool, rule).await? else {
        return Ok(());
    };
    let mut tx = crate::write_tx(pool).await?;
    karma_schedules::invalidate(&mut tx, &boundary, reason, now).await?;
    sqlx::query("UPDATE karma_schedule SET cancelled = 1, revision = revision + 1, updated_at = ? WHERE uid = ? AND cancelled = 0")
        .bind(now.to_rfc3339()).bind(&boundary.schedule).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
