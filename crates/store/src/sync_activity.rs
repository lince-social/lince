use crate::StoreError;
use nucleus::sync::{Activity, Change, Direction, Instance, Outcome, Pending, Retention, Summary};
use sqlx::{Row, SqlitePool};

pub const PAGE_SIZE: i64 = 100;

pub async fn retention(pool: &SqlitePool) -> Result<Retention, StoreError> {
    let row = sqlx::query("SELECT seconds, max_entries FROM sync_history_policy WHERE id = 1")
        .fetch_one(pool)
        .await?;
    Ok(Retention {
        seconds: row.get::<i64, _>("seconds") as u64,
        max_entries: row.get::<i64, _>("max_entries") as u32,
    })
}

pub async fn set_retention(
    pool: &SqlitePool,
    policy: Retention,
    now: i64,
) -> Result<(), StoreError> {
    if !policy.valid() {
        return Err(StoreError::Protocol("Invalid sync history limits".into()));
    }
    let mut tx = crate::write_tx(pool).await?;
    sqlx::query("UPDATE sync_history_policy SET seconds = ?, max_entries = ? WHERE id = 1")
        .bind(policy.seconds as i64)
        .bind(i64::from(policy.max_entries))
        .execute(&mut *tx)
        .await?;
    prune_tx(&mut tx, now).await?;
    tx.commit().await
}

pub async fn append(
    pool: &SqlitePool,
    activity: &Activity,
    summary: &Summary,
    now: i64,
) -> Result<(), StoreError> {
    let mut summary = summary.clone();
    summary.subjects.truncate(32);
    for subject in &mut summary.subjects {
        *subject = subject.chars().take(256).collect();
    }
    summary.message = summary
        .message
        .map(|message| message.chars().take(1024).collect());
    let activity =
        serde_json::to_string(activity).map_err(|error| StoreError::Protocol(error.to_string()))?;
    if activity.len() > 16 * 1024 {
        return Err(StoreError::Protocol("Sync description is too large".into()));
    }
    let summary =
        serde_json::to_string(&summary).map_err(|error| StoreError::Protocol(error.to_string()))?;
    let mut tx = crate::write_tx(pool).await?;
    sqlx::query("INSERT INTO sync_activity (at, activity, summary) VALUES (?, ?, ?)")
        .bind(now)
        .bind(activity)
        .bind(summary)
        .execute(&mut *tx)
        .await?;
    prune_tx(&mut tx, now).await?;
    tx.commit().await
}

async fn prune_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    now: i64,
) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM sync_activity WHERE at <= ? - (SELECT seconds FROM sync_history_policy WHERE id = 1)
        OR seq NOT IN (SELECT seq FROM sync_activity ORDER BY seq DESC LIMIT (SELECT max_entries FROM sync_history_policy WHERE id = 1))")
        .bind(now).execute(&mut **tx).await?;
    Ok(())
}

pub async fn prune(pool: &SqlitePool, now: i64) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    prune_tx(&mut tx, now).await?;
    tx.commit().await
}

pub async fn clear(pool: &SqlitePool) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM sync_activity")
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn recent(
    pool: &SqlitePool,
    before: Option<i64>,
    now: i64,
) -> Result<Vec<Change>, StoreError> {
    let rows = sqlx::query(
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
                seq: row.get("seq"),
                at: row.get("at"),
                activity: serde_json::from_str(row.get("activity"))
                    .map_err(|error| StoreError::Protocol(error.to_string()))?,
                summary: serde_json::from_str(row.get("summary"))
                    .map_err(|error| StoreError::Protocol(error.to_string()))?,
            })
        })
        .collect()
}

pub async fn queues(pool: &SqlitePool) -> Result<(Vec<Pending>, u64, u64), StoreError> {
    let mut tx = pool.begin().await?;
    let outgoing: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sync_outbox")
        .fetch_one(&mut *tx)
        .await?;
    let held: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sync_quarantine")
        .fetch_one(&mut *tx)
        .await?;
    let rows = sqlx::query(
        "SELECT q.contact_organ, q.seq, q.uid, q.field, q.attempts, o.replica_root
        FROM sync_outbox q LEFT JOIN sync_op o ON o.seq = q.seq ORDER BY q.seq LIMIT ?",
    )
    .bind(PAGE_SIZE)
    .fetch_all(&mut *tx)
    .await?;
    let mut pending: Vec<_> = rows
        .into_iter()
        .map(|row| {
            let organ: String = row.get("contact_organ");
            let root: Option<String> = row.get("replica_root");
            Pending {
                id: format!("outbox:{organ}:{}", row.get::<i64, _>("seq")),
                instance: Instance::peer(&organ, root.as_deref()),
                direction: Direction::Outgoing,
                outcome: Outcome::Pending,
                subject: Some(row.get("uid")),
                field: Some(row.get("field")),
                attempts: row.get::<i64, _>("attempts").max(0) as u64,
                message: None,
            }
        })
        .collect();
    let rows = sqlx::query(
        "SELECT uid, from_organ, reason FROM sync_quarantine ORDER BY rowid DESC LIMIT ?",
    )
    .bind(PAGE_SIZE)
    .fetch_all(&mut *tx)
    .await?;
    pending.extend(rows.into_iter().map(|row| Pending {
        id: format!("quarantine:{}", row.get::<String, _>("uid")),
        instance: Instance::peer(row.get("from_organ"), None),
        direction: Direction::Incoming,
        outcome: Outcome::Conflict,
        subject: None,
        field: None,
        attempts: 0,
        message: Some(row.get::<String, _>("reason").chars().take(1024).collect()),
    }));
    tx.commit().await?;
    Ok((pending, outgoing as u64, held as u64))
}
