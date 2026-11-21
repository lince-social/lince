use chrono::{DateTime, Utc};
use nucleus::{Cause, Fact, NewFact, transfer::TransferOccurrenceDisputeEvidence};
use sqlx::{Row, SqlitePool, sqlite::SqliteRow};

use super::{ensure_phase4_request_unused, occurrence, validate_request_key};
use crate::StoreError;

/// A participant's request to assert or clear their dispute.
#[derive(Debug, Clone)]
pub struct OccurrenceDisputeInput {
    pub occurrence_uid: String,
    /// The request key. Reuse requires the same occurrence, participant, and decision.
    pub idempotency_key: String,
    pub actor_person_uid: String,
    pub disputed: bool,
    /// Verified Action intent used when the fact is unsigned.
    pub authorization_intent_uid: Option<String>,
}

/// One immutable decision in an occurrence's dispute history.
#[derive(Debug, Clone)]
pub struct OccurrenceDisputeEventRow {
    pub uid: String,
    pub occurrence_uid: String,
    pub transfer_uid: String,
    pub actor_person_uid: String,
    pub disputed: bool,
    pub fact_uid: String,
    pub idempotency_key: String,
    pub created_at: String,
}

/// The recorded decision and the occurrence's current dispute state.
#[derive(Debug, Clone)]
pub struct OccurrenceDisputeOutcome {
    pub event: OccurrenceDisputeEventRow,
    pub fact: Fact,
    /// Includes other participants' decisions and any system dispute.
    pub current_disputed: bool,
}

/// Whether the operation wrote a new decision or found an existing one.
#[derive(Debug, Clone)]
pub enum OccurrenceDisputeCommit {
    Committed(OccurrenceDisputeOutcome),
    Replayed(OccurrenceDisputeOutcome),
}

fn map_occurrence_dispute_event(row: SqliteRow) -> OccurrenceDisputeEventRow {
    OccurrenceDisputeEventRow {
        uid: row.get("uid"),
        occurrence_uid: row.get("occurrence_uid"),
        transfer_uid: row.get("transfer_uid"),
        actor_person_uid: row.get("actor_person_uid"),
        disputed: row.get::<i64, _>("asserted") != 0,
        fact_uid: row.get("fact_uid"),
        idempotency_key: row.get("idempotency_key"),
        created_at: row.get("created_at"),
    }
}

/// Returns the decision history in insertion order.
///
/// # Errors
///
/// Returns a storage error if the history cannot be read.
pub async fn occurrence_dispute_events(
    pool: &SqlitePool,
    occurrence_uid: &str,
) -> Result<Vec<OccurrenceDisputeEventRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM transfer_occurrence_dispute_event
         WHERE occurrence_uid = ? ORDER BY rowid",
    )
    .bind(occurrence_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_occurrence_dispute_event)
    .collect())
}

/// Finds a recorded decision by its request key, ignoring surrounding whitespace.
///
/// # Errors
///
/// Returns a storage error if the decision or its related fact or occurrence
/// cannot be read.
pub async fn occurrence_dispute_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<OccurrenceDisputeOutcome>, StoreError> {
    let Some(event) =
        sqlx::query("SELECT * FROM transfer_occurrence_dispute_event WHERE idempotency_key = ?")
            .bind(request_id.trim())
            .fetch_optional(pool)
            .await?
            .map(map_occurrence_dispute_event)
    else {
        return Ok(None);
    };
    let fact = crate::facts::get(pool, &event.fact_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let current_disputed = occurrence(pool, &event.occurrence_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?
        .disputed;
    Ok(Some(OccurrenceDisputeOutcome {
        event,
        fact,
        current_disputed,
    }))
}

/// Records a participant decision and updates the occurrence in one transaction.
///
/// # Errors
///
/// Returns an error for an invalid or conflicting request, a missing occurrence,
/// a caller outside the occurrence's participants, missing authorization, or a
/// storage failure.
pub async fn set_occurrence_dispute<F>(
    pool: &SqlitePool,
    input: OccurrenceDisputeInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<OccurrenceDisputeCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_id = validate_request_key(&input.idempotency_key)?;
    if let Some(outcome) = occurrence_dispute_for_request(pool, request_id).await? {
        if outcome.event.occurrence_uid != input.occurrence_uid
            || outcome.event.actor_person_uid != input.actor_person_uid
            || outcome.event.disputed != input.disputed
        {
            return Err(sqlx::Error::Protocol(
                "dispute request id was already used with different values".into(),
            ));
        }
        return Ok(OccurrenceDisputeCommit::Replayed(outcome));
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
    let giver_person_uid: &str = occurrence.get("giver_person_uid");
    let receiver_person_uid: &str = occurrence.get("receiver_person_uid");
    if input.actor_person_uid != giver_person_uid && input.actor_person_uid != receiver_person_uid {
        return Err(sqlx::Error::Protocol(
            "only an occurrence giver or receiver may set a dispute".into(),
        ));
    }

    let event_uid = nucleus::new_uid("tde");
    let evidence = TransferOccurrenceDisputeEvidence {
        action: "set-transfer-occurrence-dispute".into(),
        idempotency_key: request_id.into(),
        transfer_uid: transfer_uid.into(),
        occurrence_uid: input.occurrence_uid.clone(),
        actor_person_uid: input.actor_person_uid.clone(),
        disputed: input.disputed,
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
            cause: Cause::settlement(input.occurrence_uid.clone()),
            payload: Some(payload),
        },
        &previous_hash,
        now,
    );
    fact.signature = sign(&fact.hash);
    if fact.signature.is_none() && input.authorization_intent_uid.is_none() {
        return Err(sqlx::Error::Protocol(
            "occurrence dispute requires a signing key or verified Action intent".into(),
        ));
    }
    crate::facts::insert(&mut tx, &fact).await?;
    if let Some(intent_uid) = input.authorization_intent_uid.as_deref() {
        crate::action_intents::link_pending_fact(&mut tx, intent_uid, &fact).await?;
    }

    let at = now.to_rfc3339();
    sqlx::query(
        "INSERT INTO transfer_phase5_correction_request
            (idempotency_key, kind, target_uid, fact_uid, created_at)
         VALUES (?, 'dispute', ?, ?, ?)",
    )
    .bind(request_id)
    .bind(&input.occurrence_uid)
    .bind(&fact.uid)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO transfer_occurrence_dispute_event
            (uid, occurrence_uid, transfer_uid, actor_person_uid, asserted,
             fact_uid, idempotency_key, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&event_uid)
    .bind(&input.occurrence_uid)
    .bind(transfer_uid)
    .bind(&input.actor_person_uid)
    .bind(i64::from(input.disputed))
    .bind(&fact.uid)
    .bind(request_id)
    .bind(&at)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "WITH active AS (
             SELECT latest.fact_uid, latest.created_at
             FROM transfer_occurrence_dispute_event latest
             WHERE latest.occurrence_uid = ?1 AND latest.asserted = 1
               AND NOT EXISTS (
                   SELECT 1 FROM transfer_occurrence_dispute_event newer
                   WHERE newer.occurrence_uid = latest.occurrence_uid
                     AND newer.actor_person_uid = latest.actor_person_uid
                     AND newer.rowid > latest.rowid
               )
             ORDER BY latest.rowid DESC LIMIT 1
         )
         UPDATE transfer_occurrence
         SET (disputed, dispute_fact_uid, disputed_at) = (
             SELECT 1, fact_uid, created_at FROM active
             UNION ALL
             SELECT system_disputed,
                    CASE WHEN system_disputed THEN system_dispute_fact_uid END,
                    CASE WHEN system_disputed THEN system_disputed_at END
             WHERE NOT EXISTS (SELECT 1 FROM active)
         )
         WHERE uid = ?1",
    )
    .bind(&input.occurrence_uid)
    .execute(&mut *tx)
    .await?;
    crate::records::bump_quantity(&mut tx, transfer_uid, crate::exact::zero(), &at).await?;
    tx.commit().await?;

    let outcome = occurrence_dispute_for_request(pool, request_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    Ok(OccurrenceDisputeCommit::Committed(outcome))
}
