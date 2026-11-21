//! Origin-side handoff requests and their state transitions.

use chrono::{DateTime, Utc};
use nucleus::{
    new_uid,
    transfer_delivery::TransferApplicationHandoffState::{
        self, Accepted, Compensated, Pending, Rejected,
    },
};
use sqlx::{Row, SqlitePool, sqlite::SqliteRow};

use crate::{
    StoreError,
    transfer_delivery::validation::{ensure_request_unused, protocol},
};

/// The origin's current handoff state for a settlement slice.
#[derive(Debug, Clone)]
pub struct ApplicationHandoffRow {
    pub uid: String,
    pub state: TransferApplicationHandoffState,
    pub origin_organ_uid: String,
    pub participant_organ_uid: String,
    pub participant_person_uid: String,
    pub transfer_uid: String,
    pub occurrence_uid: String,
    pub settlement_slice_uid: String,
    pub origin_revision: u64,
    pub canonical_slice_hash: String,
    pub attestation_uid: Option<String>,
}

fn map_handoff(row: SqliteRow) -> Result<ApplicationHandoffRow, StoreError> {
    let state = match row.get("state") {
        "pending" => Pending,
        "accepted" => Accepted,
        "rejected" => Rejected,
        "compensated" => Compensated,
        _ => {
            return Err(protocol(
                "stored Transfer application handoff state is invalid",
            ));
        }
    };
    Ok(ApplicationHandoffRow {
        uid: row.get("uid"),
        state,
        origin_organ_uid: row.get("origin_organ_uid"),
        participant_organ_uid: row.get("participant_organ_uid"),
        participant_person_uid: row.get("participant_person_uid"),
        transfer_uid: row.get("transfer_uid"),
        occurrence_uid: row.get("occurrence_uid"),
        settlement_slice_uid: row.get("settlement_slice_uid"),
        origin_revision: row.get::<i64, _>("origin_revision").cast_unsigned(),
        canonical_slice_hash: row.get("canonical_slice_hash"),
        attestation_uid: row.get("attestation_uid"),
    })
}

/// Finds an origin-side handoff by UID.
///
/// # Errors
///
/// Returns an error if the query fails or the stored state is invalid.
pub async fn application_handoff(
    pool: &SqlitePool,
    uid: &str,
) -> Result<Option<ApplicationHandoffRow>, StoreError> {
    sqlx::query("SELECT * FROM transfer_application_handoff WHERE uid = ?")
        .bind(uid)
        .try_map(map_handoff)
        .fetch_optional(pool)
        .await
}

/// Identifies the participant and canonical slice for a new handoff.
#[derive(Debug)]
pub struct NewApplicationHandoff<'a> {
    pub origin_organ_uid: &'a str,
    pub participant_organ_uid: &'a str,
    pub participant_person_uid: &'a str,
    pub transfer_uid: &'a str,
    pub occurrence_uid: &'a str,
    pub settlement_slice_uid: &'a str,
    pub origin_revision: u64,
    pub canonical_slice_hash: &'a str,
    pub request_id: &'a str,
    pub fact_uid: Option<&'a str>,
}

/// Creates a pending handoff or returns the current state on exact replay.
///
/// # Errors
///
/// Returns an error if the request conflicts, the input violates storage
/// constraints, or a write fails.
pub async fn create_application_handoff(
    pool: &SqlitePool,
    input: NewApplicationHandoff<'_>,
    now: DateTime<Utc>,
) -> Result<ApplicationHandoffRow, StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    if let Some(row) = sqlx::query(
        "SELECT h.*, e.kind AS event_kind, e.fact_uid AS event_fact FROM transfer_application_handoff_event e
         JOIN transfer_application_handoff h ON h.uid = e.handoff_uid WHERE e.request_id = ?",
    )
    .bind(input.request_id)
    .fetch_optional(&mut *tx)
    .await?
    {
        let matching_event = row.get::<&str, _>("event_kind") == "pending"
            && row.get::<Option<&str>, _>("event_fact") == input.fact_uid;
        let existing = map_handoff(row)?;
        if matching_event
            && existing.origin_organ_uid == input.origin_organ_uid
            && existing.participant_organ_uid == input.participant_organ_uid
            && existing.participant_person_uid == input.participant_person_uid
            && existing.transfer_uid == input.transfer_uid
            && existing.occurrence_uid == input.occurrence_uid
            && existing.settlement_slice_uid == input.settlement_slice_uid
            && existing.origin_revision == input.origin_revision
            && existing.canonical_slice_hash == input.canonical_slice_hash
        {
            tx.rollback().await?;
            return Ok(existing);
        }
        return Err(protocol("application handoff request id was replayed with changed contents"));
    }
    ensure_request_unused(&mut tx, input.request_id).await?;
    let uid = new_uid("tah");
    let at = now.to_rfc3339();
    sqlx::query(
        "INSERT INTO transfer_application_handoff
         (uid, origin_organ_uid, participant_organ_uid, participant_person_uid, transfer_uid,
          occurrence_uid, settlement_slice_uid, origin_revision, canonical_slice_hash,
          state, request_id, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?, ?)",
    )
    .bind(&uid)
    .bind(input.origin_organ_uid)
    .bind(input.participant_organ_uid)
    .bind(input.participant_person_uid)
    .bind(input.transfer_uid)
    .bind(input.occurrence_uid)
    .bind(input.settlement_slice_uid)
    .bind(input.origin_revision.cast_signed())
    .bind(input.canonical_slice_hash)
    .bind(input.request_id)
    .bind(&at)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO transfer_application_handoff_event
         (uid, handoff_uid, kind, from_state, to_state, fact_uid, request_id, created_at)
         VALUES (?, ?, 'pending', NULL, 'pending', ?, ?, ?)",
    )
    .bind(new_uid("tahe"))
    .bind(&uid)
    .bind(input.fact_uid)
    .bind(input.request_id)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    application_handoff(pool, &uid)
        .await?
        .ok_or(StoreError::RowNotFound)
}

/// A guarded state change and its associated evidence or rejection reason.
#[derive(Debug)]
pub struct ApplicationHandoffTransition<'a> {
    pub handoff_uid: &'a str,
    pub expected_state: TransferApplicationHandoffState,
    pub to_state: TransferApplicationHandoffState,
    pub attestation_uid: Option<&'a str>,
    pub fact_uid: Option<&'a str>,
    pub reason_code: Option<&'a str>,
    pub request_id: &'a str,
}

/// Accepts, rejects, or compensates a handoff and records the transition.
///
/// # Errors
///
/// Returns an error if the request conflicts, the transition is invalid or
/// stale, an acceptance lacks an attestation, or storage fails.
pub async fn transition_application_handoff(
    pool: &SqlitePool,
    input: ApplicationHandoffTransition<'_>,
    now: DateTime<Utc>,
) -> Result<ApplicationHandoffRow, StoreError> {
    let valid = matches!(
        (input.expected_state, input.to_state),
        (Pending, Accepted | Rejected) | (Accepted, Compensated)
    );
    if !valid || (input.to_state == Accepted && input.attestation_uid.is_none()) {
        return Err(protocol("invalid application handoff transition"));
    }
    let mut tx = crate::write_tx(pool).await?;
    if let Some(row) = sqlx::query(
        "SELECT h.*, e.kind AS event_kind, e.from_state AS event_from_state,
                e.to_state AS event_to_state, e.attestation_uid AS event_attestation_uid,
                e.fact_uid AS event_fact_uid, e.reason_code AS event_reason_code
         FROM transfer_application_handoff_event e
         JOIN transfer_application_handoff h ON h.uid = e.handoff_uid WHERE e.request_id = ?",
    )
    .bind(input.request_id)
    .fetch_optional(&mut *tx)
    .await?
    {
        let exact = row.get::<&str, _>("uid") == input.handoff_uid
            && row.get::<&str, _>("event_kind") == input.to_state.as_str()
            && row.get::<Option<&str>, _>("event_from_state")
                == Some(input.expected_state.as_str())
            && row.get::<&str, _>("event_to_state") == input.to_state.as_str()
            && row.get::<Option<&str>, _>("event_attestation_uid") == input.attestation_uid
            && row.get::<Option<&str>, _>("event_fact_uid") == input.fact_uid
            && row.get::<Option<&str>, _>("event_reason_code") == input.reason_code;
        if !exact {
            return Err(protocol(
                "application handoff request id was replayed as another transition",
            ));
        }
        let existing = map_handoff(row)?;
        tx.rollback().await?;
        return Ok(existing);
    }
    ensure_request_unused(&mut tx, input.request_id).await?;
    let at = now.to_rfc3339();
    let result = sqlx::query(
        "UPDATE transfer_application_handoff SET state = ?, attestation_uid = COALESCE(?, attestation_uid),
          updated_at = ? WHERE uid = ? AND state = ?",
    )
    .bind(input.to_state.as_str())
    .bind(input.attestation_uid)
    .bind(&at)
    .bind(input.handoff_uid)
    .bind(input.expected_state.as_str())
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() != 1 {
        return Err(protocol("application handoff state is stale"));
    }
    sqlx::query(
        "INSERT INTO transfer_application_handoff_event
         (uid, handoff_uid, kind, from_state, to_state, attestation_uid, fact_uid,
          reason_code, request_id, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(new_uid("tahe"))
    .bind(input.handoff_uid)
    .bind(input.to_state.as_str())
    .bind(input.expected_state.as_str())
    .bind(input.to_state.as_str())
    .bind(input.attestation_uid)
    .bind(input.fact_uid)
    .bind(input.reason_code)
    .bind(input.request_id)
    .bind(at)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    application_handoff(pool, input.handoff_uid)
        .await?
        .ok_or(StoreError::RowNotFound)
}
