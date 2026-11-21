use chrono::{DateTime, Utc};
use nucleus::{
    Cause, Fact, NewFact,
    transfer::{OccurrenceClaimRole, TransferOccurrenceClaimEvidence},
};
use sqlx::{Row, Sqlite, SqlitePool, Transaction, sqlite::SqliteRow};

use super::{
    TransferOccurrenceRow, ensure_phase4_request_unused, occurrence, validate_request_key,
};
use crate::StoreError;

mod bulk;
pub use bulk::{
    BulkOccurrenceClaimCommit, BulkOccurrenceClaimFailure, BulkOccurrenceClaimInput,
    BulkOccurrenceClaimOutcome, ReviewedBulkOccurrenceClaim, complete_occurrence_claims_bulk,
    phase6_bulk_request_for_request,
};

/// A giver or receiver's request to assert or clear their claim.
#[derive(Debug, Clone)]
pub struct OccurrenceClaimInput {
    pub occurrence_uid: String,
    pub idempotency_key: String,
    pub actor_person_uid: String,
    pub role: OccurrenceClaimRole,
    pub asserted: bool,
    pub authorization_intent_uid: Option<String>,
}

/// An immutable claim event.
#[derive(Debug, Clone)]
pub struct OccurrenceClaimEventRow {
    pub uid: String,
    pub occurrence_uid: String,
    pub role: OccurrenceClaimRole,
    pub asserted: bool,
    pub actor_person_uid: String,
    pub fact_uid: String,
    pub idempotency_key: String,
    pub created_at: String,
}

/// The requested event and fact alongside the current occurrence.
#[derive(Debug, Clone)]
pub struct OccurrenceClaimOutcome {
    pub transfer_uid: String,
    pub event: OccurrenceClaimEventRow,
    pub fact: Fact,
    pub occurrence: TransferOccurrenceRow,
}

/// Whether the request recorded a new claim or replayed an existing event.
#[derive(Debug, Clone)]
pub enum OccurrenceClaimCommit {
    Committed(OccurrenceClaimOutcome),
    Replayed(OccurrenceClaimOutcome),
}

fn map_occurrence_claim_event(row: SqliteRow) -> Result<OccurrenceClaimEventRow, StoreError> {
    let role: &str = row.get("role");
    Ok(OccurrenceClaimEventRow {
        uid: row.get("uid"),
        occurrence_uid: row.get("occurrence_uid"),
        role: OccurrenceClaimRole::parse(role).ok_or_else(|| {
            sqlx::Error::Protocol(format!("unknown occurrence claim role `{role}`"))
        })?,
        asserted: row.get::<i64, _>("asserted") != 0,
        actor_person_uid: row.get("actor_person_uid"),
        fact_uid: row.get("fact_uid"),
        idempotency_key: row.get("idempotency_key"),
        created_at: row.get("created_at"),
    })
}

/// Reads the occurrence's claim history, ordered by time and event ID.
///
/// # Errors
///
/// Returns an error if storage fails or a stored role is invalid.
pub async fn occurrence_claim_events(
    pool: &SqlitePool,
    occurrence_uid: &str,
) -> Result<Vec<OccurrenceClaimEventRow>, StoreError> {
    sqlx::query(
        "SELECT * FROM transfer_occurrence_claim_event
         WHERE occurrence_uid = ? ORDER BY created_at, uid",
    )
    .bind(occurrence_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_occurrence_claim_event)
    .collect()
}

/// Finds a claim event by request key, ignoring surrounding whitespace.
///
/// # Errors
///
/// Returns an error if storage fails or the stored role is invalid.
pub async fn occurrence_claim_event_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<OccurrenceClaimEventRow>, StoreError> {
    sqlx::query("SELECT * FROM transfer_occurrence_claim_event WHERE idempotency_key = ?")
        .bind(request_id.trim())
        .fetch_optional(pool)
        .await?
        .map(map_occurrence_claim_event)
        .transpose()
}

/// Reads a claim's event and fact with the current occurrence.
///
/// # Errors
///
/// Returns an error if the event or its related data cannot be read.
pub async fn occurrence_claim_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<OccurrenceClaimOutcome>, StoreError> {
    let Some(event) = occurrence_claim_event_for_request(pool, request_id).await? else {
        return Ok(None);
    };
    let occurrence = occurrence(pool, &event.occurrence_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let fact = crate::facts::get(pool, &event.fact_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    Ok(Some(OccurrenceClaimOutcome {
        transfer_uid: occurrence.transfer_uid.clone(),
        event,
        fact,
        occurrence,
    }))
}

/// Records a claim and updates the flag for its role in one transaction.
///
/// # Errors
///
/// Returns an error for an invalid or conflicting request, an unauthorized
/// actor, a missing occurrence, missing authorization, or a storage failure.
pub async fn set_occurrence_claim<F>(
    pool: &SqlitePool,
    input: OccurrenceClaimInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<OccurrenceClaimCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_id = validate_request_key(&input.idempotency_key)?;
    if let Some(outcome) = occurrence_claim_for_request(pool, request_id).await? {
        if outcome.event.occurrence_uid != input.occurrence_uid
            || outcome.event.actor_person_uid != input.actor_person_uid
            || outcome.event.role != input.role
            || outcome.event.asserted != input.asserted
        {
            return Err(sqlx::Error::Protocol(
                "occurrence claim request id was already used with different targets".into(),
            ));
        }
        return Ok(OccurrenceClaimCommit::Replayed(outcome));
    }
    ensure_phase4_request_unused(pool, request_id).await?;
    let mut tx = crate::write_tx(pool).await?;
    let occurrence = sqlx::query(
        "SELECT transfer_uid, giver_person_uid, receiver_person_uid
         FROM transfer_occurrence WHERE uid = ?",
    )
    .bind(&input.occurrence_uid)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(sqlx::Error::RowNotFound)?;
    let transfer_uid: &str = occurrence.get("transfer_uid");
    let (person_column, party) = match input.role {
        OccurrenceClaimRole::Delivery => ("giver_person_uid", "giver"),
        OccurrenceClaimRole::Receipt => ("receiver_person_uid", "receiver"),
    };
    if occurrence.get::<&str, _>(person_column) != input.actor_person_uid {
        return Err(sqlx::Error::Protocol(format!(
            "{} claim belongs only to the occurrence {party}",
            input.role.as_str(),
        )));
    }
    let evidence = TransferOccurrenceClaimEvidence {
        action: "set-transfer-occurrence-claim".into(),
        idempotency_key: request_id.into(),
        transfer_uid: transfer_uid.into(),
        occurrence_uid: input.occurrence_uid.clone(),
        role: input.role,
        asserted: input.asserted,
        actor_person_uid: input.actor_person_uid.clone(),
    };
    let previous_hash = crate::facts::last_hash(&mut tx).await?;
    let mut fact = seal_claim(&evidence, &previous_hash, now)?;
    fact.signature = sign(&fact.hash);
    if fact.signature.is_none() && input.authorization_intent_uid.is_none() {
        return Err(sqlx::Error::Protocol(
            "occurrence claim requires a signing key or verified Action intent".into(),
        ));
    }
    let at = now.to_rfc3339();
    insert_claim(
        &mut tx,
        &evidence,
        &fact,
        &at,
        input.authorization_intent_uid.as_deref(),
    )
    .await?;
    crate::records::bump_quantity(&mut tx, transfer_uid, crate::exact::zero(), &at).await?;
    tx.commit().await?;
    let outcome = occurrence_claim_for_request(pool, request_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    Ok(OccurrenceClaimCommit::Committed(outcome))
}

fn seal_claim(
    evidence: &TransferOccurrenceClaimEvidence,
    previous_hash: &str,
    now: DateTime<Utc>,
) -> Result<Fact, StoreError> {
    let payload = serde_json::to_string(evidence)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    Ok(nucleus::fact::seal(
        NewFact {
            uid: None,
            record_uid: evidence.transfer_uid.clone(),
            delta: crate::exact::zero(),
            at: None,
            actor_uid: Some(evidence.actor_person_uid.clone()),
            cause: Cause::user_edit(),
            payload: Some(payload),
        },
        previous_hash,
        now,
    ))
}

async fn insert_claim(
    tx: &mut Transaction<'_, Sqlite>,
    evidence: &TransferOccurrenceClaimEvidence,
    fact: &Fact,
    at: &str,
    intent: Option<&str>,
) -> Result<String, StoreError> {
    crate::facts::insert(tx, fact).await?;
    if let Some(intent) = intent {
        crate::action_intents::link_pending_fact(tx, intent, fact).await?;
    }
    sqlx::query(
        "INSERT INTO transfer_phase4_request
            (idempotency_key, kind, target_uid, fact_uid, created_at)
         VALUES (?, 'claim', ?, ?, ?)",
    )
    .bind(&evidence.idempotency_key)
    .bind(&evidence.occurrence_uid)
    .bind(&fact.uid)
    .bind(at)
    .execute(&mut **tx)
    .await?;

    let uid = nucleus::new_uid("tce");
    sqlx::query(
        "INSERT INTO transfer_occurrence_claim_event
            (uid, occurrence_uid, role, asserted, actor_person_uid, fact_uid,
             idempotency_key, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(&evidence.occurrence_uid)
    .bind(evidence.role.as_str())
    .bind(evidence.asserted)
    .bind(&evidence.actor_person_uid)
    .bind(&fact.uid)
    .bind(&evidence.idempotency_key)
    .bind(at)
    .execute(&mut **tx)
    .await?;

    let update = match evidence.role {
        OccurrenceClaimRole::Delivery => {
            "UPDATE transfer_occurrence SET delivery_claimed = ? WHERE uid = ?"
        }
        OccurrenceClaimRole::Receipt => {
            "UPDATE transfer_occurrence SET receipt_claimed = ? WHERE uid = ?"
        }
    };
    sqlx::query(update)
        .bind(evidence.asserted)
        .bind(&evidence.occurrence_uid)
        .execute(&mut **tx)
        .await?;
    Ok(uid)
}
