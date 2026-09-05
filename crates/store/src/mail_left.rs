use chrono::Utc;
use sqlx::{Row, SqlitePool};

use crate::StoreError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeftMail {
    pub uid: String,
    pub carrier_organ: String,
    pub carrier_node: String,
    pub to_organ: String,
    pub left_at: String,
    pub expired_at: Option<String>,
}

fn map(row: sqlx::sqlite::SqliteRow) -> LeftMail {
    LeftMail {
        uid: row.get("uid"),
        carrier_organ: row.get("carrier_organ"),
        carrier_node: row.get("carrier_node"),
        to_organ: row.get("to_organ"),
        left_at: row.get("left_at"),
        expired_at: row.get("expired_at"),
    }
}

pub async fn record(
    pool: &SqlitePool,
    uid: &str,
    carrier_organ: &str,
    carrier_node: &str,
    to_organ: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT OR IGNORE INTO mail_left
           (uid, carrier_organ, carrier_node, to_organ, left_at, expired_at)
         VALUES (?, ?, ?, ?, ?, NULL)",
    )
    .bind(uid)
    .bind(carrier_organ)
    .bind(carrier_node)
    .bind(to_organ)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn carriers_to_ask(pool: &SqlitePool) -> Result<Vec<String>, StoreError> {
    Ok(
        sqlx::query("SELECT DISTINCT carrier_node FROM mail_left WHERE expired_at IS NULL")
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|row| row.get("carrier_node"))
            .collect(),
    )
}

pub async fn mark_expired(
    pool: &SqlitePool,
    carrier_node: &str,
    uid: &str,
    expired_at: &str,
) -> Result<bool, StoreError> {
    Ok(sqlx::query(
        "UPDATE mail_left SET expired_at = ?
          WHERE uid = ? AND carrier_node = ? AND expired_at IS NULL",
    )
    .bind(expired_at)
    .bind(uid)
    .bind(carrier_node)
    .execute(pool)
    .await?
    .rows_affected()
        > 0)
}

pub async fn expired(pool: &SqlitePool, limit: i64) -> Result<Vec<LeftMail>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM mail_left WHERE expired_at IS NOT NULL
          ORDER BY expired_at DESC, uid LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map)
    .collect())
}

pub async fn outstanding(pool: &SqlitePool) -> Result<i64, StoreError> {
    Ok(
        sqlx::query("SELECT COUNT(*) AS n FROM mail_left WHERE expired_at IS NULL")
            .fetch_one(pool)
            .await?
            .get("n"),
    )
}

pub async fn prune(pool: &SqlitePool, keep_days: i64) -> Result<u64, StoreError> {
    let cutoff = (Utc::now() - chrono::Duration::days(keep_days)).to_rfc3339();
    Ok(sqlx::query("DELETE FROM mail_left WHERE left_at < ?")
        .bind(&cutoff)
        .execute(pool)
        .await?
        .rows_affected())
}
