//! Transfer repository (blueprint VIII.1): a transfer is a record
//! (kind='transfer', quantity = active) whose items ARE promises.

use chrono::Utc;
use nucleus::RecordKind;
use sqlx::{Row, SqlitePool};

use crate::records::{self, NewRecord};
use crate::StoreError;

#[derive(Debug, Clone)]
pub struct TransferRow {
    pub record_uid: String,
    pub agreement_type: String,
    pub agreement_pct: Option<i64>,
    pub settlement: String,
    pub visibility: String,
    pub satiation: Option<String>,
    pub parent_uid: Option<String>,
    pub source_uid: Option<String>,
    pub active: bool,
}

pub struct NewTransfer<'a> {
    pub slug: Option<&'a str>,
    pub head: &'a str,
    pub agreement_type: &'a str,
    pub agreement_pct: Option<i64>,
    pub satiation: Option<&'a str>,
    pub source_uid: Option<&'a str>,
}

pub async fn create(pool: &SqlitePool, new: NewTransfer<'_>) -> Result<String, StoreError> {
    let rec = records::create(
        pool,
        NewRecord {
            slug: new.slug,
            kind: RecordKind::Transfer,
            head: new.head,
            body: "",
            quantity: 1.0,
        },
    )
    .await?;
    sqlx::query(
        "INSERT INTO transfer (record_uid, agreement_type, agreement_pct, satiation, source_uid)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&rec.uid)
    .bind(new.agreement_type)
    .bind(new.agreement_pct)
    .bind(new.satiation)
    .bind(new.source_uid)
    .execute(pool)
    .await?;
    Ok(rec.uid)
}

pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Option<TransferRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT t.*, r.quantity FROM transfer t JOIN record r ON r.uid = t.record_uid
         WHERE t.record_uid = ?",
    )
    .bind(uid)
    .fetch_optional(pool)
    .await?
    .map(|r| TransferRow {
        record_uid: r.get("record_uid"),
        agreement_type: r.get("agreement_type"),
        agreement_pct: r.get("agreement_pct"),
        settlement: r.get("settlement"),
        visibility: r.get("visibility"),
        satiation: r.get("satiation"),
        parent_uid: r.get("parent_uid"),
        source_uid: r.get("source_uid"),
        active: r.get::<f64, _>("quantity") != 0.0,
    }))
}

pub async fn add_party(
    pool: &SqlitePool,
    transfer_uid: &str,
    actor_uid: &str,
) -> Result<String, StoreError> {
    let uid = nucleus::new_uid("y");
    sqlx::query("INSERT INTO transfer_party (uid, transfer_uid, actor_uid) VALUES (?, ?, ?)")
        .bind(&uid)
        .bind(transfer_uid)
        .bind(actor_uid)
        .execute(pool)
        .await?;
    Ok(uid)
}

/// (party_uid, actor_uid, agreement level) for every party; level 0 when the
/// party never agreed or was invalidated.
pub async fn party_levels(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<Vec<(String, String, i64)>, StoreError> {
    Ok(sqlx::query(
        "SELECT p.uid, p.actor_uid, COALESCE(a.level, 0) AS level
         FROM transfer_party p
         LEFT JOIN transfer_agreement a ON a.party_uid = p.uid AND a.transfer_uid = p.transfer_uid
         WHERE p.transfer_uid = ?",
    )
    .bind(transfer_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| (r.get("uid"), r.get("actor_uid"), r.get("level")))
    .collect())
}

pub async fn set_agreement(
    pool: &SqlitePool,
    transfer_uid: &str,
    party_uid: &str,
    level: i64,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO transfer_agreement (uid, transfer_uid, party_uid, level, at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(transfer_uid, party_uid) DO UPDATE SET level = excluded.level, at = excluded.at",
    )
    .bind(nucleus::new_uid("g"))
    .bind(transfer_uid)
    .bind(party_uid)
    .bind(level)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

/// Counteroffers are edits: editing a bundled promise drops every party's
/// agreement back to 0 (blueprint VIII.1).
pub async fn invalidate_agreements(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE transfer_agreement SET level = 0 WHERE transfer_uid = ?")
        .bind(transfer_uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn promises_of(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<Vec<crate::misc::PromiseRow>, StoreError> {
    Ok(sqlx::query("SELECT * FROM promise WHERE transfer_uid = ?")
        .bind(transfer_uid)
        .fetch_all(pool)
        .await?
        .into_iter()
        .filter_map(crate::misc::map_promise_pub)
        .collect())
}

/// Sibling transfers duplicated from the same source (satiation, VIII.3).
pub async fn siblings_of_source(
    pool: &SqlitePool,
    source_uid: &str,
    except: &str,
) -> Result<Vec<String>, StoreError> {
    Ok(sqlx::query("SELECT record_uid FROM transfer WHERE source_uid = ? AND record_uid != ?")
        .bind(source_uid)
        .bind(except)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|r| r.get("record_uid"))
        .collect())
}

/// Promises with a condition that are waiting to activate (chains/spectators).
pub async fn conditional_pending(
    pool: &SqlitePool,
) -> Result<Vec<crate::misc::PromiseRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM promise WHERE condition IS NOT NULL AND state IN ('proposed', 'agreed')",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .filter_map(crate::misc::map_promise_pub)
    .collect())
}
