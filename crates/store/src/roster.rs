//! Storage for the identity floor: signed Cell rosters, key successions and
//! revocation certificates (Ontology §11).
//!
//! This module STORES; it does not verify. Every signature check lives in
//! `engine::roster`, so there is exactly one place that decides whether a key
//! is allowed to speak for an Organ.

use chrono::Utc;
use sqlx::{Row, SqlitePool};

use crate::StoreError;

#[derive(Debug, Clone)]
pub struct StoredRoster {
    pub organ_uid: String,
    pub root_key: String,
    pub version: i64,
    pub not_after: String,
    pub payload: String,
    pub signature: String,
}

fn map(row: sqlx::sqlite::SqliteRow) -> StoredRoster {
    StoredRoster {
        organ_uid: row.get("organ_uid"),
        root_key: row.get("root_key"),
        version: row.get("version"),
        not_after: row.get("not_after"),
        payload: row.get("payload"),
        signature: row.get("signature"),
    }
}

pub async fn get(pool: &SqlitePool, organ_uid: &str) -> Result<Option<StoredRoster>, StoreError> {
    Ok(
        sqlx::query("SELECT * FROM organ_roster WHERE organ_uid = ?")
            .bind(organ_uid)
            .fetch_optional(pool)
            .await?
            .map(map),
    )
}

/// Store a roster the caller has ALREADY verified. Monotonic by version, so a
/// replayed older roster cannot re-add a device that was revoked.
pub async fn put(pool: &SqlitePool, roster: &StoredRoster) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO organ_roster
           (organ_uid, root_key, version, not_after, payload, signature, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(organ_uid) DO UPDATE SET
           root_key = excluded.root_key,
           version = excluded.version,
           not_after = excluded.not_after,
           payload = excluded.payload,
           signature = excluded.signature,
           updated_at = excluded.updated_at
         WHERE excluded.version > organ_roster.version",
    )
    .bind(&roster.organ_uid)
    .bind(&roster.root_key)
    .bind(roster.version)
    .bind(&roster.not_after)
    .bind(&roster.payload)
    .bind(&roster.signature)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

/// Issue an enrolment token by storing only its hash.
pub async fn put_enrolment_token(
    pool: &SqlitePool,
    token_hash: &str,
    expires_at: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT OR REPLACE INTO enrolment_token (token_hash, expires_at, used_at, created_at)
         VALUES (?, ?, NULL, ?)",
    )
    .bind(token_hash)
    .bind(expires_at)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

/// Consume a token: valid only if it exists, has not expired, and has not been
/// used. The UPDATE is the claim, so two devices racing the same token cannot
/// both succeed — `rows_affected` is the winner's answer.
pub async fn redeem_enrolment_token(
    pool: &SqlitePool,
    token_hash: &str,
) -> Result<bool, StoreError> {
    let now = Utc::now().to_rfc3339();
    let affected = sqlx::query(
        "UPDATE enrolment_token SET used_at = ?
          WHERE token_hash = ? AND used_at IS NULL AND expires_at > ?",
    )
    .bind(&now)
    .bind(token_hash)
    .bind(&now)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(affected > 0)
}

pub async fn record_succession(
    pool: &SqlitePool,
    organ_uid: &str,
    old_key: &str,
    new_key: &str,
    signature: &str,
    created_at: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT OR IGNORE INTO identity_succession
           (organ_uid, old_key, new_key, signature, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(organ_uid)
    .bind(old_key)
    .bind(new_key)
    .bind(signature)
    .bind(created_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// Every `(old_key, new_key)` edge held for an Organ — the succession chain a
/// new key must connect to.
pub async fn successions(
    pool: &SqlitePool,
    organ_uid: &str,
) -> Result<Vec<(String, String)>, StoreError> {
    Ok(
        sqlx::query("SELECT old_key, new_key FROM identity_succession WHERE organ_uid = ?")
            .bind(organ_uid)
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|row| (row.get("old_key"), row.get("new_key")))
            .collect(),
    )
}

pub async fn record_revocation(
    pool: &SqlitePool,
    organ_uid: &str,
    revoked_key: &str,
    signature: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT OR IGNORE INTO identity_revocation
           (organ_uid, revoked_key, signature, created_at)
         VALUES (?, ?, ?, ?)",
    )
    .bind(organ_uid)
    .bind(revoked_key)
    .bind(signature)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn is_revoked(pool: &SqlitePool, organ_uid: &str, key: &str) -> Result<bool, StoreError> {
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM identity_revocation WHERE organ_uid = ? AND revoked_key = ?",
    )
    .bind(organ_uid)
    .bind(key)
    .fetch_one(pool)
    .await?
        > 0)
}

/// Revocation certificates this Organ has published about its own keys.
pub async fn revocations_of(
    pool: &SqlitePool,
    organ_uid: &str,
) -> Result<Vec<(String, String)>, StoreError> {
    Ok(
        sqlx::query("SELECT revoked_key, signature FROM identity_revocation WHERE organ_uid = ?")
            .bind(organ_uid)
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|row| (row.get("revoked_key"), row.get("signature")))
            .collect(),
    )
}
