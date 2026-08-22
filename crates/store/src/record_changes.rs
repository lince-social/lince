use crate::StoreError;
use chrono::{DateTime, Duration, Utc};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

pub const RETENTION_DAYS: i64 = 7;

pub const MAX_PER_RECORD: i64 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    Local,
    Remote,
}

impl Cause {
    pub fn as_str(self) -> &'static str {
        match self {
            Cause::Local => "local",
            Cause::Remote => "remote",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "local" => Some(Cause::Local),
            "remote" => Some(Cause::Remote),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub seq: i64,
    pub record_uid: String,
    pub field: String,
    pub cause: Cause,
    pub winner_organ: Option<String>,
    pub displaced: Option<String>,
    pub displaced_local: bool,
    pub at: String,
}

pub async fn note_local(
    pool: &SqlitePool,
    record_uid: &str,
    field: &str,
) -> Result<(), StoreError> {
    insert(pool, record_uid, field, Cause::Local, None, None, false).await
}

pub async fn note_remote_win(
    pool: &SqlitePool,
    record_uid: &str,
    field: &str,
    winner_organ: &str,
    displaced: Option<&str>,
    displaced_local: bool,
) -> Result<(), StoreError> {
    insert(
        pool,
        record_uid,
        field,
        Cause::Remote,
        Some(winner_organ),
        displaced,
        displaced_local,
    )
    .await
}

async fn insert(
    pool: &SqlitePool,
    record_uid: &str,
    field: &str,
    cause: Cause,
    winner_organ: Option<&str>,
    displaced: Option<&str>,
    displaced_local: bool,
) -> Result<(), StoreError> {
    let now = Utc::now();
    sqlx::query(
        "INSERT INTO record_change
             (record_uid, field, cause, winner_organ, displaced, displaced_local, at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(record_uid)
    .bind(field)
    .bind(cause.as_str())
    .bind(winner_organ)
    .bind(displaced)
    .bind(i64::from(displaced_local))
    .bind(now.to_rfc3339())
    .execute(pool)
    .await?;
    prune_record(pool, record_uid).await?;
    prune_before(pool, now - Duration::days(RETENTION_DAYS)).await?;
    Ok(())
}

pub async fn note_local_tx(
    tx: &mut Transaction<'_, Sqlite>,
    record_uid: &str,
    field: &str,
) -> Result<(), StoreError> {
    let now = Utc::now();
    sqlx::query(
        "INSERT INTO record_change
             (record_uid, field, cause, winner_organ, displaced, displaced_local, at)
         VALUES (?, ?, 'local', NULL, NULL, 0, ?)",
    )
    .bind(record_uid)
    .bind(field)
    .bind(now.to_rfc3339())
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "DELETE FROM record_change
          WHERE record_uid = ?
            AND seq NOT IN (
                SELECT seq FROM record_change
                 WHERE record_uid = ?
                 ORDER BY seq DESC
                 LIMIT ?
            )",
    )
    .bind(record_uid)
    .bind(record_uid)
    .bind(MAX_PER_RECORD)
    .execute(&mut **tx)
    .await?;
    sqlx::query("DELETE FROM record_change WHERE at < ?")
        .bind((now - Duration::days(RETENTION_DAYS)).to_rfc3339())
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn prune_before(pool: &SqlitePool, cutoff: DateTime<Utc>) -> Result<u64, StoreError> {
    Ok(sqlx::query("DELETE FROM record_change WHERE at < ?")
        .bind(cutoff.to_rfc3339())
        .execute(pool)
        .await?
        .rows_affected())
}

async fn prune_record(pool: &SqlitePool, record_uid: &str) -> Result<u64, StoreError> {
    Ok(sqlx::query(
        "DELETE FROM record_change
          WHERE record_uid = ?
            AND seq NOT IN (
                SELECT seq FROM record_change
                 WHERE record_uid = ?
                 ORDER BY seq DESC
                 LIMIT ?
            )",
    )
    .bind(record_uid)
    .bind(record_uid)
    .bind(MAX_PER_RECORD)
    .execute(pool)
    .await?
    .rows_affected())
}

pub async fn recent(
    pool: &SqlitePool,
    record_uid: &str,
    limit: i64,
) -> Result<Vec<Change>, StoreError> {
    let rows = sqlx::query(
        "SELECT seq, record_uid, field, cause, winner_organ, displaced, displaced_local, at
           FROM record_change
          WHERE record_uid = ?
          ORDER BY seq DESC
          LIMIT ?",
    )
    .bind(record_uid)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            Some(Change {
                seq: r.get("seq"),
                record_uid: r.get("record_uid"),
                field: r.get("field"),
                cause: Cause::parse(r.get::<String, _>("cause").as_str())?,
                winner_organ: r.get("winner_organ"),
                displaced: r.get("displaced"),
                displaced_local: r.get::<i64, _>("displaced_local") != 0,
                at: r.get("at"),
            })
        })
        .collect())
}
