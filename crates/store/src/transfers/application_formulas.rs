use std::fmt;

use chrono::{DateTime, Utc};
use nucleus::{
    Cause, Fact, NewFact,
    transfer::{TransferOccurrenceApplicationEvidence, occurrence_application_formula_hash},
};
use sqlx::{Row, SqlitePool, sqlite::SqliteRow};

use super::{ensure_phase4_request_unused, occurrence, validate_request_key};
use crate::StoreError;

/// A receiver's request to revise their private occurrence formula.
#[derive(Clone)]
pub struct OccurrenceApplicationFormulaInput {
    pub occurrence_uid: String,
    pub idempotency_key: String,
    pub actor_person_uid: String,
    pub formula: String,
    pub authorization_intent_uid: Option<String>,
}

impl fmt::Debug for OccurrenceApplicationFormulaInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OccurrenceApplicationFormulaInput")
            .field("occurrence_uid", &self.occurrence_uid)
            .field("idempotency_key", &self.idempotency_key)
            .field("actor_person_uid", &self.actor_person_uid)
            .finish_non_exhaustive()
    }
}

/// Public evidence for a private formula revision.
#[derive(Debug, Clone)]
pub struct OccurrenceApplicationEventRow {
    pub uid: String,
    pub occurrence_uid: String,
    pub receiver_person_uid: String,
    pub formula_hash: String,
    pub version: u64,
    pub fact_uid: String,
    pub idempotency_key: String,
    pub created_at: String,
}

/// The receiver's current private formula and the event that authored it.
#[derive(Clone)]
pub struct OccurrenceApplicationPolicyRow {
    pub occurrence_uid: String,
    pub receiver_person_uid: String,
    pub formula: String,
    pub formula_hash: String,
    pub version: u64,
    pub latest_event_uid: String,
    pub updated_at: String,
}

impl fmt::Debug for OccurrenceApplicationPolicyRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OccurrenceApplicationPolicyRow")
            .field("occurrence_uid", &self.occurrence_uid)
            .field("receiver_person_uid", &self.receiver_person_uid)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

/// The request's event and fact, alongside the current policy.
#[derive(Debug, Clone)]
pub struct OccurrenceApplicationFormulaOutcome {
    pub transfer_uid: String,
    pub event: OccurrenceApplicationEventRow,
    pub fact: Fact,
    pub policy: OccurrenceApplicationPolicyRow,
}

/// Whether a formula was written or found by a previous request key.
#[derive(Debug, Clone)]
pub enum OccurrenceApplicationFormulaCommit {
    Committed(OccurrenceApplicationFormulaOutcome),
    Replayed(OccurrenceApplicationFormulaOutcome),
}

fn map_occurrence_application_event(row: SqliteRow) -> OccurrenceApplicationEventRow {
    OccurrenceApplicationEventRow {
        uid: row.get("uid"),
        occurrence_uid: row.get("occurrence_uid"),
        receiver_person_uid: row.get("receiver_person_uid"),
        formula_hash: row.get("formula_hash"),
        version: row.get::<i64, _>("version").cast_unsigned(),
        fact_uid: row.get("fact_uid"),
        idempotency_key: row.get("idempotency_key"),
        created_at: row.get("created_at"),
    }
}

fn map_occurrence_application_policy(row: SqliteRow) -> OccurrenceApplicationPolicyRow {
    OccurrenceApplicationPolicyRow {
        occurrence_uid: row.get("occurrence_uid"),
        receiver_person_uid: row.get("receiver_person_uid"),
        formula: row.get("formula"),
        formula_hash: row.get("formula_hash"),
        version: row.get::<i64, _>("version").cast_unsigned(),
        latest_event_uid: row.get("latest_event_uid"),
        updated_at: row.get("updated_at"),
    }
}

/// Reads formula evidence in version order.
///
/// # Errors
///
/// Returns a storage error if the event history cannot be read.
pub async fn occurrence_application_events(
    pool: &SqlitePool,
    occurrence_uid: &str,
) -> Result<Vec<OccurrenceApplicationEventRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM transfer_occurrence_application_event
         WHERE occurrence_uid = ? ORDER BY version",
    )
    .bind(occurrence_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_occurrence_application_event)
    .collect())
}

/// Finds the event for a request key, ignoring surrounding whitespace.
///
/// # Errors
///
/// Returns a storage error if the event cannot be read.
pub async fn occurrence_application_event_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<OccurrenceApplicationEventRow>, StoreError> {
    Ok(
        sqlx::query(
            "SELECT * FROM transfer_occurrence_application_event WHERE idempotency_key = ?",
        )
        .bind(request_id.trim())
        .fetch_optional(pool)
        .await?
        .map(map_occurrence_application_event),
    )
}

/// Reads the private formula for the given occurrence and receiver.
///
/// # Errors
///
/// Returns a storage error if the policy cannot be read.
pub async fn occurrence_application_policy(
    pool: &SqlitePool,
    occurrence_uid: &str,
    receiver_person_uid: &str,
) -> Result<Option<OccurrenceApplicationPolicyRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM transfer_occurrence_application_policy
         WHERE occurrence_uid = ? AND receiver_person_uid = ?",
    )
    .bind(occurrence_uid)
    .bind(receiver_person_uid)
    .fetch_optional(pool)
    .await?
    .map(map_occurrence_application_policy))
}

/// Reads the request's event and fact with the receiver's current formula.
///
/// # Errors
///
/// Returns a storage error if a recorded event or its related data cannot be read.
pub async fn occurrence_application_formula_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<OccurrenceApplicationFormulaOutcome>, StoreError> {
    let Some(event) = occurrence_application_event_for_request(pool, request_id).await? else {
        return Ok(None);
    };
    let occurrence = occurrence(pool, &event.occurrence_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let fact = crate::facts::get(pool, &event.fact_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let policy =
        occurrence_application_policy(pool, &event.occurrence_uid, &event.receiver_person_uid)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
    Ok(Some(OccurrenceApplicationFormulaOutcome {
        transfer_uid: occurrence.transfer_uid,
        event,
        fact,
        policy,
    }))
}

/// Records a new formula version and replaces the receiver's current policy.
///
/// # Errors
///
/// Returns an error for an invalid or conflicting request, missing authorization,
/// a non-receiver actor, an exhausted version counter, or a storage failure.
pub async fn set_occurrence_application_formula<F>(
    pool: &SqlitePool,
    input: OccurrenceApplicationFormulaInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<OccurrenceApplicationFormulaCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_id = validate_request_key(&input.idempotency_key)?;
    let formula = input.formula.trim();
    if formula.is_empty() || formula.chars().count() > 2_000 {
        return Err(sqlx::Error::Protocol(
            "application formula must contain 1 to 2000 characters".into(),
        ));
    }
    let formula_hash = occurrence_application_formula_hash(formula);
    if let Some(outcome) = occurrence_application_formula_for_request(pool, request_id).await? {
        if outcome.event.occurrence_uid != input.occurrence_uid
            || outcome.event.receiver_person_uid != input.actor_person_uid
            || outcome.event.formula_hash != formula_hash
        {
            return Err(sqlx::Error::Protocol(
                "application request id was already used with different targets".into(),
            ));
        }
        return Ok(OccurrenceApplicationFormulaCommit::Replayed(outcome));
    }
    ensure_phase4_request_unused(pool, request_id).await?;
    let mut tx = crate::write_tx(pool).await?;
    let occurrence = sqlx::query(
        "SELECT transfer_uid, receiver_person_uid FROM transfer_occurrence WHERE uid = ?",
    )
    .bind(&input.occurrence_uid)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(sqlx::Error::RowNotFound)?;
    let transfer_uid: &str = occurrence.get("transfer_uid");
    let receiver_person_uid: &str = occurrence.get("receiver_person_uid");
    if receiver_person_uid != input.actor_person_uid {
        return Err(sqlx::Error::Protocol(
            "only the occurrence receiver may set its private application formula".into(),
        ));
    }
    let version: i64 = sqlx::query_scalar(
        "SELECT version FROM transfer_occurrence_application_policy WHERE occurrence_uid = ?",
    )
    .bind(&input.occurrence_uid)
    .fetch_optional(&mut *tx)
    .await?
    .unwrap_or(0_i64)
    .checked_add(1)
    .ok_or_else(|| sqlx::Error::Protocol("application formula version overflow".into()))?;
    let evidence = TransferOccurrenceApplicationEvidence {
        action: "set-transfer-occurrence-application-formula".into(),
        idempotency_key: request_id.into(),
        transfer_uid: transfer_uid.into(),
        occurrence_uid: input.occurrence_uid.clone(),
        receiver_person_uid: input.actor_person_uid.clone(),
        formula_hash: formula_hash.clone(),
        version: version.cast_unsigned(),
    };
    let payload = serde_json::to_string(&evidence)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let previous_hash = crate::facts::last_hash(&mut tx).await?;
    let mut fact = nucleus::fact::seal(
        NewFact {
            uid: None,
            record_uid: transfer_uid.into(),
            delta: crate::exact::zero(),
            at: None,
            actor_uid: Some(input.actor_person_uid.clone()),
            cause: Cause::user_edit(),
            payload: Some(payload),
        },
        &previous_hash,
        now,
    );
    fact.signature = sign(&fact.hash);
    if fact.signature.is_none() && input.authorization_intent_uid.is_none() {
        return Err(sqlx::Error::Protocol(
            "application formula change requires a signing key or verified Action intent".into(),
        ));
    }
    crate::facts::insert(&mut tx, &fact).await?;
    if let Some(intent_uid) = input.authorization_intent_uid.as_deref() {
        crate::action_intents::link_pending_fact(&mut tx, intent_uid, &fact).await?;
    }
    let at = now.to_rfc3339();
    sqlx::query(
        "INSERT INTO transfer_phase4_request
            (idempotency_key, kind, target_uid, fact_uid, created_at)
         VALUES (?, 'application', ?, ?, ?)",
    )
    .bind(request_id)
    .bind(&input.occurrence_uid)
    .bind(&fact.uid)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    let event_uid = nucleus::new_uid("tap");
    sqlx::query(
        "INSERT INTO transfer_occurrence_application_event
            (uid, occurrence_uid, receiver_person_uid, formula_hash, version,
             fact_uid, idempotency_key, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&event_uid)
    .bind(&input.occurrence_uid)
    .bind(&input.actor_person_uid)
    .bind(&formula_hash)
    .bind(version)
    .bind(&fact.uid)
    .bind(request_id)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO transfer_occurrence_application_policy
            (occurrence_uid, receiver_person_uid, formula, formula_hash,
             version, latest_event_uid, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(occurrence_uid) DO UPDATE SET
             receiver_person_uid = excluded.receiver_person_uid,
             formula = excluded.formula,
             formula_hash = excluded.formula_hash,
             version = excluded.version,
             latest_event_uid = excluded.latest_event_uid,
             updated_at = excluded.updated_at",
    )
    .bind(&input.occurrence_uid)
    .bind(&input.actor_person_uid)
    .bind(formula)
    .bind(&formula_hash)
    .bind(version)
    .bind(&event_uid)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    crate::records::bump_quantity(&mut tx, transfer_uid, crate::exact::zero(), &at).await?;
    tx.commit().await?;
    let outcome = occurrence_application_formula_for_request(pool, request_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    Ok(OccurrenceApplicationFormulaCommit::Committed(outcome))
}
