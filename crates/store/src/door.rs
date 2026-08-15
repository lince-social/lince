//! The front door's inbox (Ontology §11 "Front-door mechanics", cluster C3).
//!
//! A front-door Cell — the always-on VPS a stranger's "add me in Lince"
//! reaches — holds `relay_capabilities()`: no write, no karma, no represent.
//! It therefore cannot bind a contact, cannot log an op, and cannot accept on
//! the owner's behalf. What it CAN do is hold the request until a Cell that
//! can decide comes and asks for it.
//!
//! Local-only, never in the op log. Putting it in the log would mean the door
//! writing into the identity, which is the one thing its capability set exists
//! to prevent.

use chrono::Utc;
use sqlx::{Row, SqlitePool};

use crate::StoreError;

/// How many held requests a door keeps. A queue nobody empties is a queue that
/// fills a disk, and the owner deciding on the hundredth stranger matters less
/// than the door still working.
pub const MAX_DOOR_REQUESTS: i64 = 500;

#[derive(Debug, Clone, PartialEq)]
pub struct DoorRequest {
    pub uid: String,
    pub node_id: String,
    pub organ_uid: String,
    /// The Introduction verbatim, as JSON.
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

/// Hold a stranger's request. Re-knocking REPLACES rather than accumulates:
/// the newest attempt carries the freshest addresses, and a peer retrying on a
/// timer must not be able to grow this table.
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
    // Oldest first, because the newest knock is the one still being waited on.
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

/// What is waiting, oldest first — the order a person would work through them.
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

/// Drop requests a deciding Cell has taken. Acknowledged rather than deleted
/// on read: a personal Cell that dies mid-decision must find them still there.
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
