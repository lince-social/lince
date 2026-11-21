use nucleus::trust_analysis::custody::{Acknowledgment, Offer, forwarding::SourceMode};
use serde::Serialize;
use serde_json::{Value, from_str, to_value};
use sqlx::{Row, Sqlite, Transaction, query, query_scalar, sqlite::SqliteRow};

use super::invalid;
use crate::StoreError;

/// A queued forwarding job and its current delivery status.
#[derive(Debug, Clone, Serialize)]
pub struct Job {
    /// Queue sequence used for ordering and pagination.
    pub sequence: i64,
    /// Record that authorized the forwarding.
    pub record: String,
    /// Operator who reviewed the forwarding.
    pub owner: String,
    /// Record containing or identifying the source evidence.
    pub source: String,
    /// How the artifact becomes available locally.
    pub mode: SourceMode,
    /// Forwarding policy revision accepted by the review.
    pub policy_version: u64,
    /// Reviewed delivery offer.
    pub offer: Offer,
    /// Pinned destination used for delivery.
    pub endpoint: String,
    /// Stored queue state.
    pub state: String,
    /// Number of claimed delivery attempts.
    pub attempts: u8,
    /// Last observed byte progress for this attempt.
    pub sent_bytes: u32,
    /// Bounded diagnostic describing the delivery state.
    pub error: Option<String>,
    /// Receipt proving the custodian accepted the offer, if available.
    pub acknowledgment: Option<Acknowledgment>,
}

/// Reads the forwarding job stored under the control Record.
/// # Errors
/// Returns an error if the job is absent or cannot be read or decoded.
pub async fn get_on(tx: &mut Transaction<'_, Sqlite>, record: &str) -> Result<Job, StoreError> {
    query("SELECT sequence,record_uid,owner_person_uid,source_record_uid,source_mode,policy_version,offer_json,endpoint,state,attempts,sent_bytes,error,acknowledgment_json FROM record_evidence_forwarding WHERE record_uid=?")
        .bind(record)
        .try_map(decode)
        .fetch_one(&mut **tx)
        .await
}

/// Lists up to four ready jobs, prioritizing control evidence and preserving queue order.
/// # Errors
/// Returns an error if the queue cannot be read.
pub async fn pending_on(
    tx: &mut Transaction<'_, Sqlite>,
    now: i64,
) -> Result<Vec<String>, StoreError> {
    query_scalar("SELECT record_uid FROM record_evidence_forwarding WHERE state='pending' AND attempts<3 AND next_attempt<=? ORDER BY control DESC,sequence LIMIT 4")
        .bind(now)
        .fetch_all(&mut **tx)
        .await
}

/// Lists up to 33 jobs for the operator, newest first before the optional sequence.
/// # Errors
/// Returns an error if the queue cannot be read, decoded, or encoded.
pub async fn page_on(
    tx: &mut Transaction<'_, Sqlite>,
    owner: &str,
    before: Option<i64>,
) -> Result<Vec<Value>, StoreError> {
    query("SELECT sequence,record_uid,owner_person_uid,source_record_uid,source_mode,policy_version,offer_json,endpoint,state,attempts,sent_bytes,error,acknowledgment_json FROM record_evidence_forwarding WHERE owner_person_uid=? AND sequence<? ORDER BY sequence DESC LIMIT 33")
        .bind(owner)
        .bind(before.unwrap_or(i64::MAX))
        .try_map(|row| to_value(decode(row)?).map_err(invalid))
        .fetch_all(&mut **tx)
        .await
}

fn decode(row: SqliteRow) -> Result<Job, StoreError> {
    Ok(Job {
        sequence: row.try_get("sequence")?,
        record: row.try_get("record_uid")?,
        owner: row.try_get("owner_person_uid")?,
        source: row.try_get("source_record_uid")?,
        mode: match row.try_get::<&str, _>("source_mode")? {
            "serving" => SourceMode::Serving,
            "custody" => SourceMode::Custody,
            _ => return Err(invalid("Unknown evidence delivery source")),
        },
        policy_version: u64::try_from(row.try_get::<i64, _>("policy_version")?).map_err(invalid)?,
        offer: from_str(row.try_get("offer_json")?).map_err(invalid)?,
        endpoint: row.try_get("endpoint")?,
        state: row.try_get("state")?,
        attempts: u8::try_from(row.try_get::<i64, _>("attempts")?).map_err(invalid)?,
        sent_bytes: u32::try_from(row.try_get::<i64, _>("sent_bytes")?).map_err(invalid)?,
        error: row.try_get("error")?,
        acknowledgment: row
            .try_get::<Option<&str>, _>("acknowledgment_json")?
            .map(|body| from_str(body).map_err(invalid))
            .transpose()?,
    })
}
