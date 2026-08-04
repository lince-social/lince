//! Logins granted to contact Organs (Ontology §11 "live mode").
//!
//! A login here is a BINDING, not a credential: the iroh handshake already
//! proved which Organ is on the connection, so what remains to decide is which
//! Person they act as. Every read they then make is gated by that Person's
//! visibility, which is why granting one is a named, revocable thing rather
//! than a door.

use chrono::Utc;
use sqlx::{Row, SqlitePool};

use crate::StoreError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Login {
    pub organ_uid: String,
    pub person_uid: String,
    pub created_at: String,
}

fn map(row: sqlx::sqlite::SqliteRow) -> Login {
    Login {
        organ_uid: row.get("organ_uid"),
        person_uid: row.get("person_uid"),
        created_at: row.get("created_at"),
    }
}

/// Bind a contact Organ to the Person it acts as. Re-granting moves the
/// binding rather than adding a second one — an Organ acts as exactly one
/// Person, or its writes could not be told apart.
pub async fn grant(pool: &SqlitePool, organ_uid: &str, person_uid: &str) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO organ_login (organ_uid, person_uid, created_at) VALUES (?, ?, ?)
         ON CONFLICT(organ_uid) DO UPDATE SET person_uid = excluded.person_uid",
    )
    .bind(organ_uid)
    .bind(person_uid)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

/// The Person an Organ acts as, or `None` — which is a refusal, not an error:
/// a contact with no login granted simply has no live session available.
pub async fn person_for_organ(
    pool: &SqlitePool,
    organ_uid: &str,
) -> Result<Option<String>, StoreError> {
    sqlx::query_scalar("SELECT person_uid FROM organ_login WHERE organ_uid = ?")
        .bind(organ_uid)
        .fetch_optional(pool)
        .await
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<Login>, StoreError> {
    Ok(sqlx::query("SELECT * FROM organ_login ORDER BY created_at")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(map)
        .collect())
}

/// Take the login back. One row, one delete — the guarantee §12 asks of
/// revocation: local, immediate, and not a request the other side may decline.
pub async fn revoke(pool: &SqlitePool, organ_uid: &str) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM organ_login WHERE organ_uid = ?")
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}
