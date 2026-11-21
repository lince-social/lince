use crate::{StoreError, write_tx};
use nucleus::{
    execution::now,
    new_uid,
    performance::{Span, add},
};
use serde_json::Value;
use sqlx::{Row, SqlitePool, query, query_scalar, sqlite::SqliteRow};

/// Persists a queued effect and returns its generated UID.
///
/// # Errors
///
/// Returns an error if the queue insert fails.
pub async fn queue_effect(
    pool: &SqlitePool,
    kind: &str,
    payload: &Value,
    origin_uid: Option<&str>,
) -> Result<String, StoreError> {
    let uid = new_uid("e");
    query(
        "INSERT INTO effect_queue (uid, kind, payload, origin_uid, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(kind)
    .bind(payload.to_string())
    .bind(origin_uid)
    .bind(now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(uid)
}

/// A persisted effect ready to be handled.
#[derive(Debug, Clone)]
pub struct EffectRow {
    /// Effect UID.
    pub uid: String,
    /// Effect type used for dispatch.
    pub kind: String,
    /// Decoded effect details.
    pub payload: Value,
    /// Originating Record UID, when recorded.
    pub origin_uid: Option<String>,
}

/// Reads the next queue page without filtering by lane.
///
/// # Errors
///
/// Returns an error if the queue read fails.
pub async fn due_effects(pool: &SqlitePool) -> Result<Vec<EffectRow>, StoreError> {
    due_effects_for_lane(pool, None).await
}

/// Routes queued effects to their dispatcher.
#[derive(Debug, Clone, Copy)]
pub enum EffectLane {
    /// Action effects.
    Actions,
    /// Commands and signals subject to the normal command limit.
    Commands,
    /// Saved commands explicitly exempt from the command limit.
    UnlimitedCommands,
}

/// Reads due effects for a lane, deferring later effects of the same occurrence.
///
/// # Errors
///
/// Returns an error if the queue read fails.
pub async fn due_effects_for_lane(
    pool: &SqlitePool,
    lane: Option<EffectLane>,
) -> Result<Vec<EffectRow>, StoreError> {
    Ok(due_effect_rows(pool, lane)
        .await?
        .iter()
        .filter_map(effect_row)
        .collect())
}

/// Reads the first decodable effect in the next queue page.
///
/// # Errors
///
/// Returns an error if the queue read fails.
pub async fn first_due_effect(pool: &SqlitePool) -> Result<Option<EffectRow>, StoreError> {
    Ok(due_effect_rows(pool, None)
        .await?
        .iter()
        .find_map(effect_row))
}

async fn due_effect_rows(
    pool: &SqlitePool,
    lane: Option<EffectLane>,
) -> Result<Vec<SqliteRow>, StoreError> {
    let _read = Span::new("effect-queue-read-await");
    let lane = lane.map(|lane| match lane {
        EffectLane::Actions => 0,
        EffectLane::Commands => 1,
        EffectLane::UnlimitedCommands => 2,
    });
    let rows = query(
        "SELECT effect.uid, effect.kind, effect.payload, effect.origin_uid
         FROM effect_queue effect
         WHERE status = 'queued'
           AND (?1 IS NULL OR ?1 = CASE
               WHEN kind = 'saved-command' AND COALESCE(json_extract(payload, '$.unlimited'),
                   json_extract(payload, '$.command_snapshot.unlimited'), 0) = 1 THEN 2
               WHEN kind IN ('command', 'signal', 'saved-command') THEN 1
               ELSE 0 END)
           AND (?1 IS NULL OR NOT EXISTS (
               SELECT 1 FROM effect_queue earlier
               WHERE earlier.rowid < effect.rowid AND earlier.status IN ('queued', 'running')
                 AND json_extract(earlier.payload, '$.rule') = json_extract(effect.payload, '$.rule')
                 AND json_extract(earlier.payload, '$.revision') = json_extract(effect.payload, '$.revision')
                 AND json_extract(earlier.payload, '$.occurrence.event_id') = json_extract(effect.payload, '$.occurrence.event_id')))
         ORDER BY effect.rowid LIMIT 64",
    )
        .bind(lane)
        .fetch_all(pool)
        .await?;
    add("effect-queue-rows-read", rows.len() as u64);
    Ok(rows)
}

fn effect_row(row: &SqliteRow) -> Option<EffectRow> {
    let _decode = Span::new("effect-queue-payload-decode");
    add("effect-queue-payload-decodes", 1);
    let payload: &str = row.try_get("payload").ok()?;
    Some(EffectRow {
        uid: row.try_get("uid").ok()?,
        kind: row.try_get("kind").ok()?,
        payload: serde_json::from_str(payload).ok()?,
        origin_uid: row.try_get("origin_uid").ok()?,
    })
}

/// Moves a queued effect to running if its occurrence has no predecessor pending.
///
/// # Errors
///
/// Returns an error if the queue update fails.
pub async fn claim_effect(pool: &SqlitePool, uid: &str) -> Result<bool, StoreError> {
    let claim = query("UPDATE effect_queue SET status = 'running' WHERE uid = ? AND status = 'queued' AND NOT EXISTS (SELECT 1 FROM effect_queue earlier WHERE earlier.rowid < effect_queue.rowid AND earlier.status IN ('queued', 'running') AND json_extract(earlier.payload, '$.rule') = json_extract(effect_queue.payload, '$.rule') AND json_extract(earlier.payload, '$.revision') = json_extract(effect_queue.payload, '$.revision') AND json_extract(earlier.payload, '$.occurrence.event_id') = json_extract(effect_queue.payload, '$.occurrence.event_id'))")
        .bind(uid).execute(pool).await?;
    Ok(claim.rows_affected() != 0)
}

/// Records an attempt and its outcome in one transaction.
///
/// # Errors
///
/// Returns an error if the status or outcome could not be committed.
pub async fn finish_effect(
    pool: &SqlitePool,
    uid: &str,
    ok: bool,
    result: &str,
) -> Result<(), StoreError> {
    let mut tx = write_tx(pool).await?;
    query(
        "UPDATE effect_queue SET status = ?, finished_at = ?, result = ?,
                attempts = attempts + 1 WHERE uid = ?",
    )
    .bind(if ok { "done" } else { "failed" })
    .bind(now().to_rfc3339())
    .bind(result)
    .bind(uid)
    .execute(&mut *tx)
    .await?;
    query("INSERT INTO karma_effect_outcome(effect_uid, attempt, status, result, at) SELECT uid, attempts, status, result, finished_at FROM effect_queue WHERE uid = ?")
        .bind(uid).execute(&mut *tx).await?;
    tx.commit().await
}

/// Counts completed notifications since a timestamp, excluding parked effects.
///
/// # Errors
///
/// Returns an error if the count query fails.
pub async fn notifies_delivered_since(
    pool: &SqlitePool,
    since_rfc3339: &str,
) -> Result<i64, StoreError> {
    query_scalar(
        "SELECT COUNT(1) FROM effect_queue
         WHERE kind = 'notify' AND status = 'done'
           AND result NOT LIKE 'parked%' AND finished_at >= ?",
    )
    .bind(since_rfc3339)
    .fetch_one(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Store;
    use serde_json::json;

    #[tokio::test]
    async fn first_due_effect_skips_a_malformed_legacy_payload() {
        let store = Store::open_memory().await.unwrap();
        query("INSERT INTO effect_queue(uid,kind,payload,created_at) VALUES ('legacy','notify','invalid','2000-01-01')")
            .execute(&store.pool)
            .await
            .unwrap();
        let uid = queue_effect(&store.pool, "notify", &json!({"text":"ready"}), None)
            .await
            .unwrap();

        let effect = first_due_effect(&store.pool).await.unwrap().unwrap();
        assert_eq!(effect.uid, uid);
    }
}
