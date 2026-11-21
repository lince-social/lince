//! Evidence custody policies, commitments, retained artifacts, and expiry.

mod control;
mod policy;

use std::fmt::{self, Debug, Formatter};

use nucleus::trust_analysis::{
    custody::{self, Acknowledgment, Offer},
    serving::Artifact,
};
use serde_json::{Value, from_str, json, to_string};
use sqlx::{Sqlite, Transaction, query, query_as, query_scalar};

pub(crate) use control::{Control, control_on};
pub use policy::{PolicyState, policy_on, set_policy_on};

use crate::{StoreError, evidence_forwarding};

fn invalid(error: impl ToString) -> StoreError {
    StoreError::Protocol(error.to_string())
}

/// Removes expired payloads and controls, returning their deleted counts.
/// # Errors
/// Returns an error if forwarding or custody cleanup cannot complete.
pub async fn cleanup_on(
    tx: &mut Transaction<'_, Sqlite>,
    now: i64,
) -> Result<(u64, u64), StoreError> {
    evidence_forwarding::cleanup_on(tx, now).await?;
    let payloads = query("DELETE FROM evidence_custody_payload WHERE NOT EXISTS(SELECT 1 FROM evidence_custody_commitment c WHERE c.artifact_hash=evidence_custody_payload.artifact_hash AND c.retained_until>?)")
        .bind(now)
        .execute(&mut **tx)
        .await?
        .rows_affected();
    let controls = query("DELETE FROM evidence_custody_commitment WHERE control_until<=?")
        .bind(now)
        .execute(&mut **tx)
        .await?
        .rows_affected();
    Ok((payloads, controls))
}

/// Reads the acknowledgment already recorded for the exact offer.
/// # Errors
/// Returns an error if the offer hash or stored acknowledgment cannot be read.
pub async fn acknowledgment_on(
    tx: &mut Transaction<'_, Sqlite>,
    offer: &Offer,
) -> Result<Option<Acknowledgment>, StoreError> {
    acknowledgment_for_hash(tx, offer.hash().map_err(invalid)?.as_str()).await
}

async fn acknowledgment_for_hash(
    tx: &mut Transaction<'_, Sqlite>,
    hash: &str,
) -> Result<Option<Acknowledgment>, StoreError> {
    let row: Option<String> = query_scalar(
        "SELECT acknowledgment_json FROM evidence_custody_commitment WHERE offer_hash=?",
    )
    .bind(hash)
    .fetch_optional(&mut **tx)
    .await?;
    row.map(|body| from_str(&body).map_err(invalid)).transpose()
}

/// Inputs for retaining an offered artifact under the current custody policy.
pub struct Retain<'a> {
    /// Local operator who authorized custody.
    pub person: &'a str,
    /// Custody policy revision observed when reviewing the offer.
    pub expected_policy: u64,
    /// Signed custody offer to accept.
    pub offer: &'a Offer,
    /// Exact artifact offered for retention.
    pub artifact: &'a Artifact,
    /// Acknowledgment bound to the offer.
    pub acknowledgment: &'a Acknowledgment,
    /// Current Unix time in seconds.
    pub now: i64,
}

impl Debug for Retain<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Retain")
            .field("expected_policy", &self.expected_policy)
            .field("now", &self.now)
            .finish_non_exhaustive()
    }
}

/// Records custody of the exact artifact and returns its durable acknowledgment.
/// # Errors
/// Returns an error if the offer, artifact, authorization, policy, quota, or transaction is invalid.
pub async fn retain_on(
    tx: &mut Transaction<'_, Sqlite>,
    input: Retain<'_>,
    sign: impl Fn(&str) -> Option<String>,
) -> Result<Acknowledgment, StoreError> {
    let Retain {
        person,
        expected_policy,
        offer,
        artifact,
        acknowledgment,
        now,
    } = input;
    offer.validate(now).map_err(invalid)?;
    offer.accepts(artifact, now).map_err(invalid)?;
    if !acknowledgment.matches(offer) {
        return Err(invalid(
            "Acknowledgment does not bind the exact custody offer",
        ));
    }
    let offer_hash = offer.hash().map_err(invalid)?;
    if let Some(stored) = acknowledgment_for_hash(tx, offer_hash.as_str()).await? {
        return Ok(stored);
    }
    cleanup_on(tx, now).await?;
    let state = policy_on(tx, person).await?;
    if state.version != expected_policy
        || !state.policy.enabled
        || offer.retained_until > now.saturating_add(i64::from(state.policy.retention_seconds))
    {
        return Err(invalid(
            "Custody policy changed, disabled or rejects retention",
        ));
    }
    let (items, bytes): (i64, i64) = query_as("SELECT COUNT(*),COALESCE(SUM(byte_count),0) FROM evidence_custody_commitment WHERE owner_person_uid=? AND retained_until>?")
        .bind(person)
        .bind(now)
        .fetch_one(&mut **tx)
        .await?;
    let (all_items, all_bytes): (i64, i64) = query_as("SELECT (SELECT COUNT(*) FROM evidence_custody_commitment),(SELECT COALESCE(SUM(length(body)),0) FROM evidence_custody_payload)")
        .fetch_one(&mut **tx)
        .await?;
    let exists: bool =
        query_scalar("SELECT EXISTS(SELECT 1 FROM evidence_custody_payload WHERE artifact_hash=?)")
            .bind(offer.artifact.as_str())
            .fetch_one(&mut **tx)
            .await?;
    let offer_bytes = i64::from(offer.bytes);
    let quota = i64::try_from(state.policy.storage_bytes).map_err(|_| capacity_error())?;
    let global_bytes = i64::try_from(custody::MAX_STORAGE_BYTES).map_err(|_| capacity_error())?;
    if items >= i64::from(state.policy.items)
        || bytes.saturating_add(offer_bytes) > quota
        || all_items >= i64::from(custody::MAX_ITEMS)
        || all_bytes.saturating_add(if exists { 0 } else { offer_bytes }) > global_bytes
    {
        return Err(capacity_error());
    }
    let record = control_on(
        tx,
        Control {
            person,
            actor: &offer.custodian,
            head: "Evidence retention acknowledgment",
            payload: &json!({"kind":"custody_acknowledgment","acknowledgment":acknowledgment}),
            now,
            intent: None,
        },
        sign,
    )
    .await?;
    query("INSERT OR IGNORE INTO evidence_custody_payload(artifact_hash,kind,body,created_at) VALUES(?,?,?,?)")
        .bind(offer.artifact.as_str())
        .bind(offer.source_kind.as_str())
        .bind(artifact.bytes().map_err(invalid)?)
        .bind(now)
        .execute(&mut **tx)
        .await?;
    query("INSERT INTO evidence_custody_commitment(offer_hash,record_uid,owner_person_uid,sender_organ_uid,artifact_hash,byte_count,retained_until,control_until,offer_json,acknowledgment_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
        .bind(offer_hash.as_str())
        .bind(record)
        .bind(person)
        .bind(&offer.sender)
        .bind(offer.artifact.as_str())
        .bind(offer.bytes)
        .bind(offer.retained_until)
        .bind(offer.retained_until.saturating_add(custody::CONTROL_SECONDS))
        .bind(to_string(offer).map_err(invalid)?)
        .bind(to_string(acknowledgment).map_err(invalid)?)
        .bind(now)
        .execute(&mut **tx)
        .await?;
    Ok(acknowledgment.clone())
}

/// Lists retained commitments newest first before the optional sequence limit.
/// # Errors
/// Returns an error if the inventory cannot be read.
pub async fn inventory_on(
    tx: &mut Transaction<'_, Sqlite>,
    person: &str,
    before: Option<i64>,
) -> Result<Vec<Value>, StoreError> {
    let rows: Vec<(i64, String, String, i64, i64, i64)> = query_as("SELECT sequence,record_uid,artifact_hash,byte_count,retained_until,control_until FROM evidence_custody_commitment WHERE owner_person_uid=? AND sequence<? ORDER BY sequence DESC LIMIT 33")
        .bind(person)
        .bind(before.unwrap_or(i64::MAX))
        .fetch_all(&mut **tx)
        .await?;
    Ok(rows.into_iter().map(|(sequence, record, hash, bytes, until, control)| {
        json!({"sequence":sequence,"record":record,"hash":hash,"bytes":bytes,"retained_until":until,"control_until":control})
    }).collect())
}

fn capacity_error() -> StoreError {
    invalid("Custody capacity is full; existing commitments are retained")
}
