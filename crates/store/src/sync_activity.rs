//! Diagnostic history and pending queues for sync.

mod encoding;
mod queues;

use crate::{StoreError, write_tx};
use nucleus::{
    performance::Span,
    sync::{Activity, Change, Retention, Summary},
};
use sqlx::{Row, Sqlite, SqlitePool, Transaction, query, query_as, query_scalar};

pub use queues::queues;

/// Maximum page length for each history or queue query.
pub const PAGE_SIZE: i64 = 100;

const MAX_ACTIVITY_BYTES: usize = 16 * 1024;
const PRUNE_QUERY: &str = "DELETE FROM sync_activity WHERE at <= ? - (SELECT seconds FROM sync_history_policy WHERE id = 1) OR seq < (SELECT seq FROM sync_activity ORDER BY seq DESC LIMIT 1 OFFSET (SELECT max_entries - 1 FROM sync_history_policy WHERE id = 1))";

#[cfg(test)]
mod tests;

/// Reads the retention policy currently applied to sync history.
///
/// # Errors
///
/// Returns an error for a failed policy query or invalid stored values.
pub async fn retention(pool: &SqlitePool) -> Result<Retention, StoreError> {
    let (enabled, seconds, max_entries): (bool, i64, i64) =
        query_as("SELECT enabled, seconds, max_entries FROM sync_history_policy WHERE id = 1")
            .fetch_one(pool)
            .await?;
    Ok(Retention {
        enabled,
        seconds: u64::try_from(seconds).map_err(protocol)?,
        max_entries: u32::try_from(max_entries).map_err(protocol)?,
    })
}

/// Stores a retention policy and immediately prunes rows outside its limits.
///
/// # Errors
///
/// Returns an error for invalid limits or a failed write transaction.
pub async fn set_retention(
    pool: &SqlitePool,
    policy: Retention,
    now: i64,
) -> Result<(), StoreError> {
    if !policy.valid() {
        return Err(protocol("Invalid sync history limits"));
    }
    let mut tx = write_tx(pool).await?;
    query("UPDATE sync_history_policy SET enabled = ?, seconds = ?, max_entries = ? WHERE id = 1")
        .bind(policy.enabled)
        .bind(i64::try_from(policy.seconds).map_err(protocol)?)
        .bind(i64::from(policy.max_entries))
        .execute(&mut *tx)
        .await?;
    prune_tx(&mut tx, now).await?;
    tx.commit().await
}

/// Records sync activity when history is enabled, then applies the retention policy.
///
/// # Errors
///
/// Returns an error for oversized or invalid activity or a failed storage operation.
pub async fn append(
    pool: &SqlitePool,
    activity: &Activity,
    summary: &Summary,
    now: i64,
) -> Result<(), StoreError> {
    let enabled: bool = query_scalar("SELECT enabled FROM sync_history_policy WHERE id = 1")
        .fetch_one(pool)
        .await?;
    if !enabled {
        return Ok(());
    }
    let (activity, summary) = {
        let _prepare = Span::new("sync-history-prepare");
        let activity = serde_json::to_string(activity).map_err(protocol)?;
        if activity.len() > MAX_ACTIVITY_BYTES {
            return Err(protocol("Sync description is too large"));
        }
        (activity, encoding::summary(summary)?)
    };
    let mut tx = {
        let _begin = Span::new("sync-history-begin-await");
        write_tx(pool).await?
    };
    {
        let _insert = Span::new("sync-history-insert-await");
        query("INSERT INTO sync_activity (at, activity, summary) SELECT ?, ?, ? WHERE (SELECT enabled FROM sync_history_policy WHERE id = 1)")
            .bind(now)
            .bind(activity)
            .bind(summary)
            .execute(&mut *tx)
            .await?;
    }
    {
        let _prune = Span::new("sync-history-prune-await");
        prune_tx(&mut tx, now).await?;
    }
    let _commit = Span::new("sync-history-commit-await");
    tx.commit().await
}

async fn prune_tx(tx: &mut Transaction<'_, Sqlite>, now: i64) -> Result<(), StoreError> {
    query(PRUNE_QUERY).bind(now).execute(&mut **tx).await?;
    Ok(())
}

/// Prunes history when the enabled policy has expired or excess rows.
///
/// # Errors
///
/// Returns an error if SQLite cannot read the policy or complete the write.
pub async fn prune(pool: &SqlitePool, now: i64) -> Result<(), StoreError> {
    let needed: bool = query_scalar("SELECT enabled AND (EXISTS(SELECT 1 FROM sync_activity WHERE at <= ? - seconds) OR EXISTS(SELECT 1 FROM sync_activity ORDER BY seq DESC LIMIT 1 OFFSET (SELECT max_entries FROM sync_history_policy WHERE id = 1))) FROM sync_history_policy WHERE id = 1")
        .bind(now).fetch_one(pool).await?;
    if !needed {
        return Ok(());
    }
    let mut tx = write_tx(pool).await?;
    prune_tx(&mut tx, now).await?;
    tx.commit().await
}

/// Removes all retained sync activity.
///
/// # Errors
///
/// Returns an error if the delete fails.
pub async fn clear(pool: &SqlitePool) -> Result<(), StoreError> {
    query("DELETE FROM sync_activity").execute(pool).await?;
    Ok(())
}

/// Reads at most one descending page before the supplied sequence and within the age limit.
///
/// # Errors
///
/// Returns an error if the query or retained activity decoding fails.
pub async fn recent(
    pool: &SqlitePool,
    before: Option<i64>,
    now: i64,
) -> Result<Vec<Change>, StoreError> {
    let rows = query(
        "SELECT seq, at, activity, summary FROM sync_activity
        WHERE seq < ? AND at > ? - (SELECT seconds FROM sync_history_policy WHERE id = 1)
        ORDER BY seq DESC LIMIT ?",
    )
    .bind(before.unwrap_or(i64::MAX))
    .bind(now)
    .bind(PAGE_SIZE)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(Change {
                seq: row.try_get("seq")?,
                at: row.try_get("at")?,
                activity: serde_json::from_str(row.try_get("activity")?).map_err(protocol)?,
                summary: serde_json::from_str(row.try_get("summary")?).map_err(protocol)?,
            })
        })
        .collect()
}

fn protocol(error: impl ToString) -> StoreError {
    StoreError::Protocol(error.to_string())
}
