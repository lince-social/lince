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

/// Which Organ, of those whose signed roster we hold, names this Cell —
/// `None` when no roster we hold mentions it.
///
/// The reverse of the membership question, and it exists because the dedup key
/// on the op log is `(actor_cell, hlc)`: an op claiming a Cell that belongs to
/// somebody ELSE can pre-occupy that key and make the real op arrive later and
/// be dropped as an already-seen duplicate. Knowing who a Cell belongs to is
/// what turns that into a refusal.
///
/// Scanned in Rust rather than with SQLite's JSON functions: the rosters we
/// hold number one per contact, the payload is the SIGNED blob and must stay
/// exactly as signed, and an unparseable one is skipped rather than failing
/// the whole question — a roster we cannot read tells us nothing about who
/// owns a Cell, which is the same position as holding no roster at all.
pub async fn organ_holding_cell(
    pool: &SqlitePool,
    cell_uid: &str,
) -> Result<Option<String>, StoreError> {
    let rows = sqlx::query("SELECT organ_uid, payload FROM organ_roster")
        .fetch_all(pool)
        .await?;
    for row in rows {
        let payload: String = row.get("payload");
        let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&payload) else {
            continue;
        };
        let named = parsed
            .get("cells")
            .and_then(|cells| cells.as_array())
            .is_some_and(|cells| {
                cells.iter().any(|cell| {
                    cell.get("cell_uid").and_then(|uid| uid.as_str()) == Some(cell_uid)
                })
            });
        if named {
            return Ok(Some(row.get("organ_uid")));
        }
    }
    Ok(None)
}

/// Flatten THIS Cell's capabilities out of a roster, so the database can
/// enforce them (Ontology §11, C4).
///
/// Called whenever a roster for our OWN Organ is stored, from either
/// direction: publishing one here, or adopting one signed elsewhere. A Cell
/// that is not named in it ends up with an empty set, which is the correct
/// reading — an absent capability set grants nothing, and a Cell removed from
/// the roster has been revoked.
///
/// A projection, never the source of truth. The signed blob is.
pub async fn project_local_capabilities(
    pool: &SqlitePool,
    capabilities: &[String],
) -> Result<(), StoreError> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM local_capability")
        .execute(&mut *tx)
        .await?;
    for capability in capabilities {
        sqlx::query("INSERT OR IGNORE INTO local_capability (capability) VALUES (?)")
            .bind(capability)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Store the signed public directory record, replacing any earlier one.
///
/// The caller has already signed it; this module still only stores.
pub async fn put_public_packet(
    pool: &SqlitePool,
    organ_uid: &str,
    packet: &[u8],
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO organ_public_record (organ_uid, packet, updated_at)
         VALUES (?, ?, ?)
         ON CONFLICT(organ_uid) DO UPDATE SET
           packet = excluded.packet,
           updated_at = excluded.updated_at",
    )
    .bind(organ_uid)
    .bind(packet)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn public_packet(
    pool: &SqlitePool,
    organ_uid: &str,
) -> Result<Option<Vec<u8>>, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT packet FROM organ_public_record WHERE organ_uid = ?")
            .bind(organ_uid)
            .fetch_optional(pool)
            .await?,
    )
}

/// Stop publishing. Called when the last front door goes away, so switching a
/// Cell off the public tier stops the broadcast instead of leaving the timer
/// re-announcing an address that is no longer meant to be public.
pub async fn clear_public_packet(pool: &SqlitePool, organ_uid: &str) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM organ_public_record WHERE organ_uid = ?")
        .bind(organ_uid)
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

/// Whether an enrolment token is outstanding: issued, unused, unexpired.
///
/// This is a DOOR POLICY, not a lookup. A device being enrolled is not yet a
/// contact of anything, so it arrives at the thread door as a stranger — and
/// requiring the owner to also switch on "accept unknown Organs" just to add
/// their own phone would conflate two unrelated decisions and leave a door
/// open long after the phone was added. Instead the door opens exactly while
/// the owner has asked for a code, and closes when it is used or expires.
pub async fn enrolment_is_open(pool: &SqlitePool) -> Result<bool, StoreError> {
    let now = Utc::now().to_rfc3339();
    let outstanding: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM enrolment_token WHERE used_at IS NULL AND expires_at > ?",
    )
    .bind(&now)
    .fetch_one(pool)
    .await?;
    Ok(outstanding > 0)
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

/// One succession edge with everything a peer needs to verify it for itself:
/// the signature and the `created_at` are both inside the signed payload, so
/// neither can be dropped on the way out.
pub struct SuccessionRow {
    pub old_key: String,
    pub new_key: String,
    pub signature: String,
    pub created_at: String,
}

/// Every succession an Organ has signed about its OWN keys, for publishing.
/// `successions()` above is the local chain-walk view and deliberately carries
/// no signature — nothing verifies a chain it already holds.
pub async fn published_successions(
    pool: &SqlitePool,
    organ_uid: &str,
) -> Result<Vec<SuccessionRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT old_key, new_key, signature, created_at
           FROM identity_succession WHERE organ_uid = ? ORDER BY created_at",
    )
    .bind(organ_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| SuccessionRow {
        old_key: row.get("old_key"),
        new_key: row.get("new_key"),
        signature: row.get("signature"),
        created_at: row.get("created_at"),
    })
    .collect())
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
