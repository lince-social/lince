use super::{PAGE_SIZE, encoding::prefix, protocol};
use crate::StoreError;
use nucleus::sync::{Direction, Instance, Outcome, Pending};
use sqlx::{Row, SqlitePool, query, query_scalar};

const REASON_LIMIT: usize = 1024;

/// Reads pending outgoing and quarantined operations with their total counts.
///
/// # Errors
///
/// Returns an error if the read transaction or queue decoding fails.
pub async fn queues(pool: &SqlitePool) -> Result<(Vec<Pending>, u64, u64), StoreError> {
    let mut tx = pool.begin().await?;
    let outgoing: i64 = query_scalar("SELECT COUNT(*) FROM sync_outbox")
        .fetch_one(&mut *tx)
        .await?;
    let held: i64 = query_scalar("SELECT COUNT(*) FROM sync_quarantine")
        .fetch_one(&mut *tx)
        .await?;
    let outgoing = u64::try_from(outgoing).map_err(protocol)?;
    let held = u64::try_from(held).map_err(protocol)?;
    let rows = query(
        "SELECT q.contact_organ, q.seq, q.uid, q.field, q.attempts, o.replica_root
        FROM sync_outbox q LEFT JOIN sync_op o ON o.seq = q.seq ORDER BY q.seq LIMIT ?",
    )
    .bind(PAGE_SIZE)
    .fetch_all(&mut *tx)
    .await?;
    let mut pending = Vec::with_capacity(rows.len());
    for row in rows {
        let organ: &str = row.try_get("contact_organ")?;
        let root: Option<&str> = row.try_get("replica_root")?;
        let seq: i64 = row.try_get("seq")?;
        pending.push(Pending {
            id: format!("outbox:{organ}:{seq}"),
            instance: Instance::peer(organ, root),
            direction: Direction::Outgoing,
            outcome: Outcome::Pending,
            subject: Some(row.try_get("uid")?),
            field: Some(row.try_get("field")?),
            attempts: u64::try_from(row.try_get::<i64, _>("attempts")?).unwrap_or_default(),
            message: None,
        });
    }
    let rows =
        query("SELECT uid, from_organ, reason FROM sync_quarantine ORDER BY rowid DESC LIMIT ?")
            .bind(PAGE_SIZE)
            .fetch_all(&mut *tx)
            .await?;
    pending.reserve(rows.len());
    for row in rows {
        let uid: &str = row.try_get("uid")?;
        let reason: &str = row.try_get("reason")?;
        pending.push(Pending {
            id: format!("quarantine:{uid}"),
            instance: Instance::peer(row.try_get("from_organ")?, None),
            direction: Direction::Incoming,
            outcome: Outcome::Conflict,
            subject: None,
            field: None,
            attempts: 0,
            message: Some(prefix(reason, REASON_LIMIT).to_owned()),
        });
    }
    tx.commit().await?;
    Ok((pending, outgoing, held))
}
