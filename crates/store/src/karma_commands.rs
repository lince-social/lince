use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::{StoreError, karma_fields::invalid};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Origin {
    pub rule: String,
    pub revision: i64,
    pub request: String,
    pub occurrence: nucleus::simulation::RuleOccurrence,
    pub evaluation: serde_json::Value,
}

#[derive(Clone, Debug, Serialize)]
pub struct State {
    pub origin: Origin,
    pub cancelled: bool,
    pub reason: Option<String>,
    pub dispatched_at: Option<String>,
}

pub async fn get(pool: &SqlitePool, command: &str) -> Result<Option<State>, StoreError> {
    sqlx::query("SELECT * FROM karma_transfer_command WHERE command_uid = ?")
        .bind(command)
        .fetch_optional(pool)
        .await?
        .map(|row| {
            Ok(State {
                origin: serde_json::from_str(&row.get::<String, _>("origin"))
                    .map_err(|error| invalid(&error.to_string()))?,
                cancelled: row.get("cancelled"),
                reason: row.get("reason"),
                dispatched_at: row.get("dispatched_at"),
            })
        })
        .transpose()
}

pub async fn save_tx(
    tx: &mut Transaction<'_, Sqlite>,
    command: &str,
    origin: &Origin,
) -> Result<(), StoreError> {
    let parent = sqlx::query("SELECT revision, state FROM recurrence WHERE uid = ?")
        .bind(&origin.rule)
        .fetch_optional(&mut **tx)
        .await?;
    if parent.is_none_or(|row| {
        row.get::<i64, _>("revision") != origin.revision
            || row.get::<String, _>("state") == "paused"
    }) {
        return Err(invalid(
            "The Rule changed before its remote command was queued",
        ));
    }
    let original: Option<String> =
        sqlx::query_scalar("SELECT origin FROM karma_transfer_command WHERE command_uid = ?")
            .bind(command)
            .fetch_optional(&mut **tx)
            .await?;
    if let Some(original) = original {
        let original: Origin =
            serde_json::from_str(&original).map_err(|error| invalid(&error.to_string()))?;
        if original != *origin {
            return Err(invalid("Remote automation origin changed on retry"));
        }
        return Ok(());
    }
    sqlx::query("INSERT INTO karma_transfer_command(command_uid, rule_uid, rule_revision, origin) VALUES (?, ?, ?, ?)")
        .bind(command).bind(&origin.rule).bind(origin.revision).bind(serde_json::to_string(origin).map_err(|error| invalid(&error.to_string()))?).execute(&mut **tx).await?;
    Ok(())
}

pub async fn invalidate_children_tx(
    tx: &mut Transaction<'_, Sqlite>,
    rule: &str,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE karma_transfer_command SET cancelled = 1, reason = 'The parent Rule changed or paused before dispatch' WHERE rule_uid = ? AND dispatched_at IS NULL AND cancelled = 0")
        .bind(rule).execute(&mut **tx).await?;
    Ok(())
}

pub async fn cancel(pool: &SqlitePool, command: &str, reason: &str) -> Result<(), StoreError> {
    sqlx::query("UPDATE karma_transfer_command SET cancelled = 1, reason = ? WHERE command_uid = ? AND dispatched_at IS NULL AND cancelled = 0")
        .bind(reason).bind(command).execute(pool).await?;
    Ok(())
}

pub async fn mark_dispatched(
    pool: &SqlitePool,
    command: &str,
    now: DateTime<Utc>,
) -> Result<bool, StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    let changed = sqlx::query("UPDATE karma_transfer_command SET dispatched_at = COALESCE(dispatched_at, ?) WHERE command_uid = ? AND cancelled = 0 AND (dispatched_at IS NOT NULL OR EXISTS(SELECT 1 FROM recurrence r WHERE r.uid = karma_transfer_command.rule_uid AND r.revision = karma_transfer_command.rule_revision AND r.state != 'paused'))")
        .bind(now.to_rfc3339()).bind(command).execute(&mut *tx).await?;
    let dispatched = changed.rows_affected() == 1;
    if dispatched {
        tx.commit().await?;
    } else {
        tx.rollback().await?;
    }
    Ok(dispatched)
}
