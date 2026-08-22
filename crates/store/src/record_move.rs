use crate::StoreError;
use sqlx::{Row, SqlitePool};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Move {
    pub record_uid: String,
    pub contact_organ: String,
    pub started_at: String,
    pub handed_over_at: Option<String>,
}

fn map(row: sqlx::sqlite::SqliteRow) -> Move {
    Move {
        record_uid: row.get("record_uid"),
        contact_organ: row.get("contact_organ"),
        started_at: row.get("started_at"),
        handed_over_at: row.get("handed_over_at"),
    }
}

pub async fn begin(
    pool: &SqlitePool,
    record_uid: &str,
    contact_organ: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO record_move (record_uid, contact_organ, started_at) VALUES (?, ?, ?)
         ON CONFLICT (record_uid) DO UPDATE SET contact_organ = excluded.contact_organ,
                                                started_at = excluded.started_at,
                                                handed_over_at = NULL",
    )
    .bind(record_uid)
    .bind(contact_organ)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn pending(pool: &SqlitePool) -> Result<Vec<Move>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM record_move WHERE handed_over_at IS NULL ORDER BY started_at, record_uid",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map)
    .collect())
}

pub async fn to_contact(
    pool: &SqlitePool,
    contact_organ: &str,
) -> Result<HashSet<String>, StoreError> {
    Ok(sqlx::query_scalar::<_, String>(
        "SELECT record_uid FROM record_move WHERE contact_organ = ?",
    )
    .bind(contact_organ)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect())
}

pub async fn of_record(pool: &SqlitePool, record_uid: &str) -> Result<Option<Move>, StoreError> {
    Ok(
        sqlx::query("SELECT * FROM record_move WHERE record_uid = ?")
            .bind(record_uid)
            .fetch_optional(pool)
            .await?
            .map(map),
    )
}

pub async fn forget(pool: &SqlitePool, record_uid: &str) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM record_move WHERE record_uid = ?")
        .bind(record_uid)
        .execute(pool)
        .await?;
    Ok(())
}

const OP_SEQS_OF_RECORD: &str = "SELECT seq FROM sync_op WHERE replica_root IS NULL AND (
        (tbl = 'record' AND uid = ?1)
     OR (tbl = 'record_assertion' AND uid IN (
            SELECT uid FROM record_assertion WHERE subject_uid = ?1 OR object_uid = ?1))
     OR (tbl = 'fact' AND uid IN (SELECT uid FROM fact WHERE record_uid = ?1)))";

pub async fn last_op_seq(pool: &SqlitePool, record_uid: &str) -> Result<i64, StoreError> {
    let sql = format!("SELECT COALESCE(MAX(seq), 0) AS seq FROM ({OP_SEQS_OF_RECORD})");
    Ok(sqlx::query(&sql)
        .bind(record_uid)
        .fetch_one(pool)
        .await?
        .get("seq"))
}

pub async fn still_queued(
    pool: &SqlitePool,
    contact_organ: &str,
    record_uid: &str,
) -> Result<bool, StoreError> {
    let sql = format!(
        "SELECT COUNT(1) AS n FROM sync_outbox
          WHERE contact_organ = ?2 AND seq IN ({OP_SEQS_OF_RECORD})"
    );
    let count: i64 = sqlx::query(&sql)
        .bind(record_uid)
        .bind(contact_organ)
        .fetch_one(pool)
        .await?
        .get("n");
    Ok(count > 0)
}

pub async fn mark_handed_over(pool: &SqlitePool, record_uid: &str) -> Result<(), StoreError> {
    sqlx::query("UPDATE record_move SET handed_over_at = ? WHERE record_uid = ?")
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(record_uid)
        .execute(pool)
        .await?;
    Ok(())
}
