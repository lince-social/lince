//! Fact repository (blueprint Part II).

use chrono::{DateTime, TimeDelta, Utc};
use nucleus::{Cause, CauseKind, Fact};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::StoreError;

pub async fn exists(tx: &mut Transaction<'_, Sqlite>, uid: &str) -> Result<bool, StoreError> {
    Ok(sqlx::query("SELECT 1 FROM fact WHERE uid = ?")
        .bind(uid)
        .fetch_optional(&mut **tx)
        .await?
        .is_some())
}

/// Head of the Cell's hash chain; "genesis" for an empty ledger.
pub async fn last_hash(tx: &mut Transaction<'_, Sqlite>) -> Result<String, StoreError> {
    Ok(
        sqlx::query("SELECT hash FROM fact ORDER BY rowid DESC LIMIT 1")
            .fetch_optional(&mut **tx)
            .await?
            .map(|r| r.get::<String, _>("hash"))
            .unwrap_or_else(|| "genesis".to_string()),
    )
}

pub async fn insert(tx: &mut Transaction<'_, Sqlite>, f: &Fact) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO fact (uid, record_uid, delta, at, actor_uid, cause_kind, cause_uid,
                           payload, prev_hash, hash, signature)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&f.uid)
    .bind(&f.record_uid)
    .bind(f.delta)
    .bind(f.at.to_rfc3339())
    .bind(&f.actor_uid)
    .bind(f.cause.kind.as_str())
    .bind(&f.cause.uid)
    .bind(&f.payload)
    .bind(&f.prev_hash)
    .bind(&f.hash)
    .bind(&f.signature)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn map_fact(r: sqlx::sqlite::SqliteRow) -> Fact {
    let at: String = r.get("at");
    let cause_kind: String = r.get("cause_kind");
    Fact {
        uid: r.get("uid"),
        record_uid: r.get("record_uid"),
        delta: r.get("delta"),
        at: DateTime::parse_from_rfc3339(&at)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        actor_uid: r.get("actor_uid"),
        cause: Cause {
            kind: CauseKind::parse(&cause_kind).unwrap_or(CauseKind::UserEdit),
            uid: r.get("cause_uid"),
        },
        payload: r.get("payload"),
        prev_hash: r.get("prev_hash"),
        hash: r.get("hash"),
        signature: r.get("signature"),
    }
}

/// Fetch a single sealed fact by uid (the target of a compensation/undo).
pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Option<Fact>, StoreError> {
    Ok(sqlx::query("SELECT * FROM fact WHERE uid = ?")
        .bind(uid)
        .fetch_optional(pool)
        .await?
        .map(map_fact))
}

/// The actor_uid of a record's EARLIEST fact (rowid order — creation, not the
/// latest edit). Used for "who created this" (record ownership checks,
/// message/thread sender resolution) — a record's own row carries no creator
/// column, only its facts do.
pub async fn creator_uid(
    pool: &SqlitePool,
    record_uid: &str,
) -> Result<Option<String>, StoreError> {
    Ok(
        sqlx::query("SELECT actor_uid FROM fact WHERE record_uid = ? ORDER BY rowid ASC LIMIT 1")
            .bind(record_uid)
            .fetch_optional(pool)
            .await?
            .and_then(|r| r.get::<Option<String>, _>("actor_uid")),
    )
}

pub async fn for_record(
    pool: &SqlitePool,
    record_uid: &str,
    limit: i64,
) -> Result<Vec<Fact>, StoreError> {
    Ok(
        sqlx::query("SELECT * FROM fact WHERE record_uid = ? ORDER BY rowid DESC LIMIT ?")
            .bind(record_uid)
            .bind(limit)
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(map_fact)
            .collect(),
    )
}

/// `sum(@x, <window>)` (blueprint II.1): net delta over the trailing window.
pub async fn sum_window(
    pool: &SqlitePool,
    record_uid: &str,
    window_secs: i64,
    now: DateTime<Utc>,
) -> Result<f64, StoreError> {
    let cutoff = (now - TimeDelta::seconds(window_secs)).to_rfc3339();
    let row = sqlx::query(
        "SELECT COALESCE(SUM(delta), 0.0) AS s FROM fact WHERE record_uid = ? AND at >= ?",
    )
    .bind(record_uid)
    .bind(cutoff)
    .fetch_one(pool)
    .await?;
    Ok(row.get::<f64, _>("s"))
}

/// `sum_pos(@x, <window>)`: only the inflows (positive deltas) of the window.
pub async fn sum_pos_window(
    pool: &SqlitePool,
    record_uid: &str,
    window_secs: i64,
    now: DateTime<Utc>,
) -> Result<f64, StoreError> {
    let cutoff = (now - TimeDelta::seconds(window_secs)).to_rfc3339();
    let row = sqlx::query(
        "SELECT COALESCE(SUM(delta), 0.0) AS s FROM fact
          WHERE record_uid = ? AND at >= ? AND delta > 0",
    )
    .bind(record_uid)
    .bind(cutoff)
    .fetch_one(pool)
    .await?;
    Ok(row.get::<f64, _>("s"))
}

/// `sum_neg(@x, <window>)`: only the outflows (negative deltas); the sum is
/// returned as-is (a negative number, or 0 when there were none).
pub async fn sum_neg_window(
    pool: &SqlitePool,
    record_uid: &str,
    window_secs: i64,
    now: DateTime<Utc>,
) -> Result<f64, StoreError> {
    let cutoff = (now - TimeDelta::seconds(window_secs)).to_rfc3339();
    let row = sqlx::query(
        "SELECT COALESCE(SUM(delta), 0.0) AS s FROM fact
          WHERE record_uid = ? AND at >= ? AND delta < 0",
    )
    .bind(record_uid)
    .bind(cutoff)
    .fetch_one(pool)
    .await?;
    Ok(row.get::<f64, _>("s"))
}

/// End-lagged window: net delta over `[now - end_lag - window, now - end_lag)`.
/// `end_lag = 0` degenerates to `sum_window` (with an exclusive upper bound).
pub async fn sum_window_lagged(
    pool: &SqlitePool,
    record_uid: &str,
    window_secs: i64,
    end_lag_secs: i64,
    now: DateTime<Utc>,
) -> Result<f64, StoreError> {
    let end = now - TimeDelta::seconds(end_lag_secs);
    let start = (end - TimeDelta::seconds(window_secs)).to_rfc3339();
    let end = end.to_rfc3339();
    let row = sqlx::query(
        "SELECT COALESCE(SUM(delta), 0.0) AS s FROM fact
          WHERE record_uid = ? AND at >= ? AND at < ?",
    )
    .bind(record_uid)
    .bind(start)
    .bind(end)
    .fetch_one(pool)
    .await?;
    Ok(row.get::<f64, _>("s"))
}

/// Set (upsert) the retention horizon for a record kind (blueprint II.2).
pub async fn set_retention(
    pool: &SqlitePool,
    kind: &str,
    horizon_seconds: i64,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO retention_policy (kind, horizon_seconds) VALUES (?, ?)
         ON CONFLICT(kind) DO UPDATE SET horizon_seconds = excluded.horizon_seconds",
    )
    .bind(kind)
    .bind(horizon_seconds)
    .execute(pool)
    .await?;
    Ok(())
}

/// All retention policies as `(kind, horizon_seconds)`.
pub async fn retention_policies(pool: &SqlitePool) -> Result<Vec<(String, i64)>, StoreError> {
    Ok(
        sqlx::query("SELECT kind, horizon_seconds FROM retention_policy")
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|r| (r.get("kind"), r.get("horizon_seconds")))
            .collect(),
    )
}

/// The record's most recent checkpoint fact and its rowid (compaction bound).
pub async fn last_checkpoint(
    pool: &SqlitePool,
    record_uid: &str,
) -> Result<Option<(i64, Fact)>, StoreError> {
    Ok(sqlx::query(
        "SELECT rowid, * FROM fact
          WHERE record_uid = ? AND cause_kind = 'checkpoint'
          ORDER BY rowid DESC LIMIT 1",
    )
    .bind(record_uid)
    .fetch_optional(pool)
    .await?
    .map(|r| (r.get::<i64, _>("rowid"), map_fact(r))))
}

/// Facts of a record eligible for compaction: strictly before the checkpoint
/// row AND older than the cutoff time. Ordered by rowid (chain order).
pub async fn archivable_before(
    pool: &SqlitePool,
    record_uid: &str,
    checkpoint_rowid: i64,
    cutoff: DateTime<Utc>,
) -> Result<Vec<Fact>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM fact
          WHERE record_uid = ? AND rowid < ? AND at < ?
          ORDER BY rowid",
    )
    .bind(record_uid)
    .bind(checkpoint_rowid)
    .bind(cutoff.to_rfc3339())
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_fact)
    .collect())
}

/// Delete archived facts by uid, in one transaction. The quantity cache is
/// untouched on purpose: the deltas live on, folded into the checkpoint level.
pub async fn delete_by_uids(pool: &SqlitePool, uids: &[String]) -> Result<u64, StoreError> {
    let mut tx = pool.begin().await?;
    let mut deleted = 0;
    for uid in uids {
        deleted += sqlx::query("DELETE FROM fact WHERE uid = ?")
            .bind(uid)
            .execute(&mut *tx)
            .await?
            .rows_affected();
    }
    tx.commit().await?;
    Ok(deleted)
}

/// Facts across all records, optionally since a cutoff, in chain order —
/// the `source: fact` Protein's raw feed.
pub async fn list_since(
    pool: &SqlitePool,
    since_rfc3339: Option<&str>,
    limit: i64,
) -> Result<Vec<Fact>, StoreError> {
    let rows = match since_rfc3339 {
        Some(since) => {
            sqlx::query("SELECT * FROM fact WHERE at >= ? ORDER BY rowid LIMIT ?")
                .bind(since)
                .bind(limit)
                .fetch_all(pool)
                .await?
        }
        None => {
            sqlx::query("SELECT * FROM fact ORDER BY rowid LIMIT ?")
                .bind(limit)
                .fetch_all(pool)
                .await?
        }
    };
    Ok(rows.into_iter().map(map_fact).collect())
}

/// Hours since the most recent fact on a record; None when it has none.
pub async fn hours_since_last(
    pool: &SqlitePool,
    record_uid: &str,
    now: DateTime<Utc>,
) -> Result<Option<f64>, StoreError> {
    let last: Option<String> =
        sqlx::query("SELECT at FROM fact WHERE record_uid = ? ORDER BY rowid DESC LIMIT 1")
            .bind(record_uid)
            .fetch_optional(pool)
            .await?
            .map(|r| r.get("at"));
    Ok(last.and_then(|at| {
        DateTime::parse_from_rfc3339(&at)
            .ok()
            .map(|d| (now - d.with_timezone(&Utc)).num_seconds() as f64 / 3600.0)
    }))
}
