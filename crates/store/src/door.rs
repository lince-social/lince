use chrono::Utc;
use sqlx::{Row, SqlitePool};

use crate::StoreError;

pub const MAX_DOOR_REQUESTS: i64 = 500;

#[derive(Debug, Clone, PartialEq)]
pub struct DoorRequest {
    pub uid: String,
    pub node_id: String,
    pub organ_uid: String,
    pub intro: String,
    pub received_at: String,
}

fn map(row: sqlx::sqlite::SqliteRow) -> DoorRequest {
    DoorRequest {
        uid: row.get("uid"),
        node_id: row.get("node_id"),
        organ_uid: row.get("organ_uid"),
        intro: row.get("intro"),
        received_at: row.get("received_at"),
    }
}

pub async fn hold(
    pool: &SqlitePool,
    node_id: &str,
    organ_uid: &str,
    intro: &str,
) -> Result<(), StoreError> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO door_request (uid, node_id, organ_uid, intro, received_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(node_id) DO UPDATE SET
           organ_uid = excluded.organ_uid,
           intro = excluded.intro,
           received_at = excluded.received_at",
    )
    .bind(nucleus::new_uid("d"))
    .bind(node_id)
    .bind(organ_uid)
    .bind(intro)
    .bind(&now)
    .execute(pool)
    .await?;
    sqlx::query(
        "DELETE FROM door_request WHERE uid IN (
           SELECT uid FROM door_request ORDER BY received_at DESC LIMIT -1 OFFSET ?
         )",
    )
    .bind(MAX_DOOR_REQUESTS)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn held(pool: &SqlitePool, limit: i64) -> Result<Vec<DoorRequest>, StoreError> {
    Ok(
        sqlx::query("SELECT * FROM door_request ORDER BY received_at ASC LIMIT ?")
            .bind(limit)
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(map)
            .collect(),
    )
}

pub async fn release(pool: &SqlitePool, uids: &[String]) -> Result<(), StoreError> {
    for uid in uids {
        sqlx::query("DELETE FROM door_request WHERE uid = ?")
            .bind(uid)
            .execute(pool)
            .await?;
    }
    Ok(())
}

pub async fn count(pool: &SqlitePool) -> Result<i64, StoreError> {
    Ok(sqlx::query_scalar("SELECT COUNT(*) FROM door_request")
        .fetch_one(pool)
        .await?)
}
