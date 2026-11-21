use chrono::{DateTime, TimeDelta};
use nucleus::execution::now;
use serde_json::{Value, json};
use sqlx::{Row, SqlitePool, query, query_scalar, sqlite::SqliteRow};

use super::OfferKind;
use crate::StoreError;

/// Length of the refusal window in days.
pub const REFUSAL_WINDOW_DAYS: i64 = 30;

/// A locally recorded refusal and its expiry.
#[derive(Debug, Clone)]
pub struct Refusal {
    /// Stored offer category.
    pub kind: String,
    /// Subject of the refused offer.
    pub subject_uid: String,
    /// Peer or addressee that made the offer.
    pub other_party: String,
    /// RFC 3339 time of the most recent refusal.
    pub at: String,
    /// RFC 3339 expiry of the refusal window.
    pub until: String,
}

/// Stores a refusal or renews the existing window for the same offer and party.
/// # Errors
/// Returns an error if the expiry cannot be represented or the refusal cannot be saved.
pub async fn refuse(
    pool: &SqlitePool,
    kind: OfferKind,
    subject_uid: &str,
    other_party: &str,
) -> Result<(), StoreError> {
    let now = now();
    let until = now
        .checked_add_signed(TimeDelta::days(REFUSAL_WINDOW_DAYS))
        .ok_or_else(|| {
            StoreError::Protocol("Offer refusal expiry is outside the supported range".into())
        })?;
    query(
        "INSERT INTO offer_refusal (kind, subject_uid, other_party, at, until)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(kind, subject_uid, other_party)
         DO UPDATE SET at = excluded.at, until = excluded.until",
    )
    .bind(kind.as_str())
    .bind(subject_uid)
    .bind(other_party)
    .bind(now.to_rfc3339())
    .bind(until.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

/// Reports whether the exact offer and party have a current refusal.
/// # Errors
/// Returns an error if the refusal cannot be read.
pub async fn standing_refusal(
    pool: &SqlitePool,
    kind: OfferKind,
    subject_uid: &str,
    other_party: &str,
) -> Result<bool, StoreError> {
    let until: Option<String> = query_scalar(
        "SELECT until FROM offer_refusal
          WHERE kind = ? AND subject_uid = ? AND other_party = ?",
    )
    .bind(kind.as_str())
    .bind(subject_uid)
    .bind(other_party)
    .fetch_optional(pool)
    .await?;
    Ok(until
        .is_some_and(|until| DateTime::parse_from_rfc3339(&until).is_ok_and(|when| when > now())))
}

/// Reports whether any offer of this category from the party has a current refusal.
/// # Errors
/// Returns an error if the refusal rows cannot be read.
pub async fn refused_by_party(
    pool: &SqlitePool,
    kind: OfferKind,
    other_party: &str,
) -> Result<bool, StoreError> {
    let rows: Vec<String> =
        query_scalar("SELECT until FROM offer_refusal WHERE kind = ? AND other_party = ?")
            .bind(kind.as_str())
            .bind(other_party)
            .fetch_all(pool)
            .await?;
    let now = now();
    Ok(rows
        .iter()
        .any(|until| DateTime::parse_from_rfc3339(until).is_ok_and(|when| when > now)))
}

/// Lists locally recorded refusals, including expired entries.
/// # Errors
/// Returns an error if the refusal rows cannot be read or decoded.
pub async fn refusals(pool: &SqlitePool) -> Result<Vec<Refusal>, StoreError> {
    query(
        "SELECT kind, subject_uid, other_party, at, until FROM offer_refusal
          ORDER BY at DESC, subject_uid",
    )
    .try_map(|row: SqliteRow| {
        Ok(Refusal {
            kind: row.try_get("kind")?,
            subject_uid: row.try_get("subject_uid")?,
            other_party: row.try_get("other_party")?,
            at: row.try_get("at")?,
            until: row.try_get("until")?,
        })
    })
    .fetch_all(pool)
    .await
}

/// Clears the refusal for the exact offer and party.
/// # Errors
/// Returns an error if the refusal cannot be deleted.
pub async fn forget_refusal(
    pool: &SqlitePool,
    kind: OfferKind,
    subject_uid: &str,
    other_party: &str,
) -> Result<(), StoreError> {
    query(
        "DELETE FROM offer_refusal
          WHERE kind = ? AND subject_uid = ? AND other_party = ?",
    )
    .bind(kind.as_str())
    .bind(subject_uid)
    .bind(other_party)
    .execute(pool)
    .await?;
    Ok(())
}

/// Saves the latest local outcome for the offer and party.
/// # Errors
/// Returns an error if the outcome cannot be saved.
pub async fn remember_outcome(
    pool: &SqlitePool,
    kind: OfferKind,
    subject: &str,
    party: &str,
    outcome: &str,
) -> Result<(), StoreError> {
    query("INSERT INTO offer_local_outcome(kind,subject_uid,other_party,outcome,at) VALUES (?,?,?,?,?) ON CONFLICT(kind,subject_uid,other_party) DO UPDATE SET outcome=excluded.outcome,at=excluded.at")
        .bind(kind.as_str())
        .bind(subject)
        .bind(party)
        .bind(outcome)
        .bind(now().to_rfc3339())
        .execute(pool)
        .await?;
    Ok(())
}

/// Returns up to 100 local outcomes in descending timestamp order.
/// # Errors
/// Returns an error if the outcomes cannot be read or decoded.
pub async fn local_outcomes(pool: &SqlitePool) -> Result<Vec<Value>, StoreError> {
    query("SELECT * FROM offer_local_outcome ORDER BY at DESC LIMIT 100")
        .try_map(|row: SqliteRow| {
            Ok(json!({
                "kind": row.try_get::<String, _>("kind")?,
                "subject_uid": row.try_get::<String, _>("subject_uid")?,
                "other_party": row.try_get::<String, _>("other_party")?,
                "outcome": row.try_get::<String, _>("outcome")?,
                "at": row.try_get::<String, _>("at")?,
            }))
        })
        .fetch_all(pool)
        .await
}
