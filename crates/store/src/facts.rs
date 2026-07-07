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
    Ok(sqlx::query("SELECT hash FROM fact ORDER BY rowid DESC LIMIT 1")
        .fetch_optional(&mut **tx)
        .await?
        .map(|r| r.get::<String, _>("hash"))
        .unwrap_or_else(|| "genesis".to_string()))
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

pub async fn for_record(
    pool: &SqlitePool,
    record_uid: &str,
    limit: i64,
) -> Result<Vec<Fact>, StoreError> {
    Ok(sqlx::query("SELECT * FROM fact WHERE record_uid = ? ORDER BY rowid DESC LIMIT ?")
        .bind(record_uid)
        .bind(limit)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(map_fact)
        .collect())
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
