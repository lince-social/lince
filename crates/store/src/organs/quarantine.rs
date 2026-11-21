use std::collections::HashMap;

use nucleus::{execution::now, new_uid, performance::add};
use serde_json::to_string;
use sqlx::{SqlitePool, query, query_as, query_scalar};

use crate::{
    StoreError, budget,
    contact_rate::{RateKind, spend},
    write_tx,
};

/// Maximum number of retained quarantine rows per contact.
pub const QUARANTINE_PER_CONTACT: i64 = 200;

/// Stores a refusal, then applies the row cap, refusal rate, and quarantine budget.
///
/// # Errors
/// Propagates errors saving, accounting for, or trimming the refused payload.
pub async fn quarantine(
    pool: &SqlitePool,
    from_organ: &str,
    reason: &str,
    payload: &str,
) -> Result<(), StoreError> {
    query(
        "INSERT INTO sync_quarantine (uid, from_organ, reason, payload, at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(new_uid("qr"))
    .bind(from_organ)
    .bind(reason)
    .bind(payload)
    .bind(now().to_rfc3339())
    .execute(pool)
    .await?;
    query(
        "DELETE FROM sync_quarantine
          WHERE from_organ = ?
            AND rowid NOT IN (SELECT rowid FROM sync_quarantine
                               WHERE from_organ = ?
                               ORDER BY rowid DESC LIMIT ?)",
    )
    .bind(from_organ)
    .bind(from_organ)
    .bind(QUARANTINE_PER_CONTACT)
    .execute(pool)
    .await?;
    spend(pool, from_organ, RateKind::Refusal).await?;
    trim_quarantine_to_budget(pool, from_organ).await?;
    Ok(())
}

async fn trim_quarantine_to_budget(pool: &SqlitePool, from_organ: &str) -> Result<(), StoreError> {
    let total = budget::total(pool).await?;
    let Some(quota) = budget::share(total, budget::Area::Quarantine) else {
        return Ok(());
    };
    let rows: Vec<(i64, i64)> = query_as(
        "SELECT rowid AS id, LENGTH(payload) AS n FROM sync_quarantine
          WHERE from_organ = ? ORDER BY rowid DESC",
    )
    .bind(from_organ)
    .fetch_all(pool)
    .await?;
    let evicted = budget::evict_plan(&rows, quota);
    if evicted.is_empty() {
        return Ok(());
    }
    let mut tx = write_tx(pool).await?;
    for id in evicted {
        query("DELETE FROM sync_quarantine WHERE rowid = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await
}

/// Lists a contact's quarantined rows newest first.
pub async fn quarantined_for(
    pool: &SqlitePool,
    from_organ: &str,
    limit: i64,
) -> Result<Vec<(String, String, String)>, StoreError> {
    query_as(
        "SELECT reason, payload, at FROM sync_quarantine
          WHERE from_organ = ? ORDER BY rowid DESC LIMIT ?",
    )
    .bind(from_organ)
    .bind(limit)
    .fetch_all(pool)
    .await
}

/// Lists the newest quarantined rows per selected contact, bounded per contact.
pub async fn quarantined_for_many(
    pool: &SqlitePool,
    contacts: &[&str],
    limit: i64,
) -> Result<HashMap<String, Vec<(String, String, String)>>, StoreError> {
    let mut output = HashMap::<String, Vec<(String, String, String)>>::new();
    if contacts.is_empty() || limit == 0 {
        return Ok(output);
    }
    let selected = to_string(contacts).map_err(|error| StoreError::Protocol(error.to_string()))?;
    let rows = query_as::<_, (String, String, String, String)>(
        "SELECT q.from_organ, q.reason, q.payload, q.at
         FROM (SELECT DISTINCT value FROM json_each(?1)) selected
         JOIN sync_quarantine q ON q.rowid IN (
             SELECT rowid FROM sync_quarantine WHERE from_organ = selected.value
             ORDER BY rowid DESC LIMIT ?2
         ) ORDER BY q.from_organ, q.rowid DESC",
    )
    .bind(selected)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    add("contact-presentation-quarantine-rows", rows.len() as u64);
    for (organ, reason, payload, at) in rows {
        output.entry(organ).or_default().push((reason, payload, at));
    }
    Ok(output)
}

/// Counts quarantined rows per contact, largest counts first.
pub async fn quarantine_by_contact(pool: &SqlitePool) -> Result<Vec<(String, i64)>, StoreError> {
    query_as(
        "SELECT from_organ, COUNT(1) AS n FROM sync_quarantine
          GROUP BY from_organ ORDER BY n DESC",
    )
    .fetch_all(pool)
    .await
}

/// Removes a contact's quarantined rows and returns the deletion count.
pub async fn clear_quarantine(pool: &SqlitePool, from_organ: &str) -> Result<u64, StoreError> {
    Ok(query("DELETE FROM sync_quarantine WHERE from_organ = ?")
        .bind(from_organ)
        .execute(pool)
        .await?
        .rows_affected())
}

/// Counts all retained quarantine rows.
pub async fn quarantine_count(pool: &SqlitePool) -> Result<i64, StoreError> {
    query_scalar("SELECT COUNT(1) FROM sync_quarantine")
        .fetch_one(pool)
        .await
}
