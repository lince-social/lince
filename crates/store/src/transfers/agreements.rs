//! Signed changes to a party's agreement level at a specific Transfer revision.

use chrono::{DateTime, Utc};
use nucleus::{
    Cause, Fact, NewFact, new_uid,
    transfer::{TransferAgreementTransitionEvidence, TransferRevisionEvidence},
};
use sqlx::{Row, Sqlite, SqlitePool, Transaction, sqlite::SqliteRow};

use super::{
    ensure_request_not_used_by_invitation_event, ensure_request_not_used_by_transfer_revision,
    validate_request_key,
};
use crate::{
    StoreError, action_intents::link_pending_fact, exact::zero, facts, records::bump_quantity,
};

pub(super) mod coalitions;
pub(super) mod readiness;

use coalitions::{
    AgreementCoalitionRow, agreement_coalition, agreement_coalition_on, excluded, freeze_if_ready,
};

/// A party's requested agreement level and the revision it reviewed.
#[derive(Debug, Clone)]
pub struct AgreementTransitionInput {
    pub transfer_uid: String,
    pub expected_revision: u64,
    pub idempotency_key: String,
    pub person_uid: String,
    pub to_level: u8,
    pub authorization_intent_uid: Option<String>,
}

/// The stored transition and its evidence reference.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgreementTransitionEventRow {
    pub uid: String,
    pub transfer_uid: String,
    pub revision: u64,
    pub party_uid: String,
    pub person_uid: String,
    pub from_level: u8,
    pub to_level: u8,
    pub fact_uid: String,
    pub idempotency_key: String,
    pub created_at: String,
}

/// A transition and its fact, including the coalition it froze, if any.
#[derive(Debug, Clone)]
pub struct AgreementTransitionOutcome {
    pub event: AgreementTransitionEventRow,
    pub fact: Fact,
    pub coalition: Option<AgreementCoalitionRow>,
}

/// A newly applied transition, the original result of a replay, or a stale revision.
#[derive(Debug, Clone)]
pub enum AgreementTransitionCommit {
    Committed(AgreementTransitionOutcome),
    Replayed(AgreementTransitionOutcome),
    Stale {
        transfer_uid: String,
        current_revision: u64,
    },
}

fn map_agreement_event(row: SqliteRow) -> AgreementTransitionEventRow {
    AgreementTransitionEventRow {
        uid: row.get("uid"),
        transfer_uid: row.get("transfer_uid"),
        revision: row.get::<i64, _>("revision").cast_unsigned(),
        party_uid: row.get("party_uid"),
        person_uid: row.get("person_uid"),
        from_level: row.get("from_level"),
        to_level: row.get("to_level"),
        fact_uid: row.get("fact_uid"),
        idempotency_key: row.get("idempotency_key"),
        created_at: row.get("created_at"),
    }
}

/// Lists transitions for one revision or the Transfer's complete agreement history.
///
/// # Errors
///
/// Returns an error if the revision exceeds SQLite's range or the query fails.
pub async fn agreement_events(
    pool: &SqlitePool,
    transfer_uid: &str,
    revision: Option<u64>,
) -> Result<Vec<AgreementTransitionEventRow>, StoreError> {
    let query = if let Some(revision) = revision {
        let revision = i64::try_from(revision)
            .map_err(|_| sqlx::Error::Protocol("transfer revision exceeds SQLite range".into()))?;
        sqlx::query(
            "SELECT * FROM transfer_agreement_event
             WHERE transfer_uid = ? AND revision = ? ORDER BY created_at, uid",
        )
        .bind(transfer_uid)
        .bind(revision)
    } else {
        sqlx::query(
            "SELECT * FROM transfer_agreement_event
             WHERE transfer_uid = ? ORDER BY revision, created_at, uid",
        )
        .bind(transfer_uid)
    };
    query.map(map_agreement_event).fetch_all(pool).await
}

/// Looks up the transition written for a request.
///
/// # Errors
///
/// Returns an error if the lookup fails.
pub async fn agreement_event_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<AgreementTransitionEventRow>, StoreError> {
    sqlx::query("SELECT * FROM transfer_agreement_event WHERE idempotency_key = ?")
        .bind(request_id.trim())
        .map(map_agreement_event)
        .fetch_optional(pool)
        .await
}

async fn agreement_outcome_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<AgreementTransitionOutcome>, StoreError> {
    let Some(event) = agreement_event_for_request(pool, request_id).await? else {
        return Ok(None);
    };
    let fact = facts::get(pool, &event.fact_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let coalition = agreement_coalition(pool, &event.transfer_uid, event.revision)
        .await?
        .filter(|coalition| coalition.frozen_by_event_uid == event.uid);
    Ok(Some(AgreementTransitionOutcome {
        event,
        fact,
        coalition,
    }))
}

/// Changes a party's agreement by one level and writes its signed evidence.
///
/// # Errors
///
/// Returns an error for conflicting requests, invalid transitions, missing evidence,
/// or storage failures.
pub async fn transition_agreement<F>(
    pool: &SqlitePool,
    input: AgreementTransitionInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<AgreementTransitionCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_id = validate_request_key(&input.idempotency_key)?;
    if let Some(outcome) = agreement_outcome_for_request(pool, request_id).await? {
        let event = &outcome.event;
        if event.transfer_uid != input.transfer_uid
            || event.revision != input.expected_revision
            || event.person_uid != input.person_uid
            || event.to_level != input.to_level
        {
            return Err(sqlx::Error::Protocol(
                "agreement request id was already used with different action targets".into(),
            ));
        }
        return Ok(AgreementTransitionCommit::Replayed(outcome));
    }
    if input.to_level > 2 {
        return Err(sqlx::Error::Protocol(
            "agreement level must be between 0 and 2".into(),
        ));
    }
    if input.expected_revision == 0 {
        return Err(sqlx::Error::Protocol(
            "legacy revision-0 Transfer must be signed before agreement".into(),
        ));
    }
    ensure_request_not_used_by_transfer_revision(pool, request_id).await?;
    ensure_request_not_used_by_invitation_event(pool, request_id).await?;

    let mut tx = crate::write_tx(pool).await?;
    let outcome = transition_agreement_on(&mut tx, input, now, &sign).await?;
    if matches!(&outcome, AgreementTransitionCommit::Stale { .. }) {
        tx.rollback().await?;
    } else {
        tx.commit().await?;
    }
    Ok(outcome)
}

pub(super) async fn transition_agreement_on<F>(
    tx: &mut Transaction<'_, Sqlite>,
    input: AgreementTransitionInput,
    now: DateTime<Utc>,
    sign: &F,
) -> Result<AgreementTransitionCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_id = validate_request_key(&input.idempotency_key)?;
    let expected_revision = i64::try_from(input.expected_revision)
        .map_err(|_| sqlx::Error::Protocol("transfer revision exceeds SQLite range".into()))?;
    let transfer = sqlx::query(
        "SELECT revision, agreement_type, agreement_pct FROM transfer WHERE record_uid = ?",
    )
    .bind(&input.transfer_uid)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(sqlx::Error::RowNotFound)?;
    let current_revision: i64 = transfer.get("revision");
    if current_revision != expected_revision {
        return Ok(AgreementTransitionCommit::Stale {
            transfer_uid: input.transfer_uid,
            current_revision: current_revision.cast_unsigned(),
        });
    }
    let signed_revision = sqlx::query(
        "SELECT f.hash, f.payload FROM transfer_revision tr
             JOIN fact f ON f.uid = tr.fact_uid
             WHERE tr.transfer_uid = ? AND tr.revision = ?
               AND (f.signature IS NOT NULL OR EXISTS(
                   SELECT 1 FROM fact_action_intent fai
                   JOIN signed_action_intent sai ON sai.uid = fai.intent_uid
                   WHERE fai.fact_uid = f.uid AND sai.status = 'committed'
               ))",
    )
    .bind(&input.transfer_uid)
    .bind(expected_revision)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| {
        sqlx::Error::Protocol("agreement requires an existing signed Transfer revision".into())
    })?;
    let revision_evidence: TransferRevisionEvidence =
        serde_json::from_str(signed_revision.get::<&str, _>("payload"))
            .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let participant = sqlx::query(
        "SELECT tp.uid, coalesce(ta.level, 0) AS level, person.organ_uid
         FROM transfer_party tp
         JOIN record person ON person.uid = tp.actor_uid AND person.kind = 'person'
         LEFT JOIN transfer_agreement ta ON ta.transfer_uid = tp.transfer_uid
             AND ta.party_uid = tp.uid AND ta.revision = ?
         WHERE tp.transfer_uid = ? AND tp.actor_uid = ?",
    )
    .bind(expected_revision)
    .bind(&input.transfer_uid)
    .bind(&input.person_uid)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| {
        StoreError::Protocol("only a Person party may change their own Transfer agreement".into())
    })?;
    let party_uid: String = participant.get("uid");
    let from_level: i64 = participant.get("level");
    let to_level = i64::from(input.to_level);
    if to_level.abs_diff(from_level) != 1 {
        return Err(sqlx::Error::Protocol(format!(
            "agreement must move one adjacent level from {from_level}, not to {to_level}"
        )));
    }
    let signed_party = revision_evidence
        .terms
        .parties
        .iter()
        .find(|party| party.uid == party_uid && party.person_uid == input.person_uid)
        .ok_or_else(|| {
            sqlx::Error::Protocol("agreement party is absent from the signed revision".into())
        })?;
    if to_level > from_level
        && signed_party.organ_uid.as_deref() != participant.get::<Option<&str>, _>("organ_uid")
    {
        return Err(StoreError::Protocol(
            "Person's Organ changed; review a new Transfer revision before advancing agreement"
                .into(),
        ));
    }
    let cancellation_pending: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM transfer_cancellation WHERE transfer_uid = ? AND revision = ? AND applied_fact_uid IS NULL)")
        .bind(&input.transfer_uid).bind(expected_revision).fetch_one(&mut **tx).await?;
    if to_level > from_level
        && !cancellation_pending
        && excluded(tx, &input.transfer_uid, expected_revision, &party_uid).await?
    {
        return Err(StoreError::Protocol(
            "percentage agreement coalition is frozen without this party".into(),
        ));
    }
    let event_uid = new_uid("tae");
    let at = now.to_rfc3339();
    sqlx::query(
        "INSERT INTO transfer_agreement (uid, transfer_uid, party_uid, level, at, revision, last_event_uid)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(transfer_uid, party_uid) DO UPDATE SET
             level = excluded.level, at = excluded.at, revision = excluded.revision,
             last_event_uid = excluded.last_event_uid",
    )
    .bind(new_uid("g"))
    .bind(&input.transfer_uid)
    .bind(&party_uid)
    .bind(to_level)
    .bind(&at)
    .bind(expected_revision)
    .bind(&event_uid)
    .execute(&mut **tx)
    .await?;
    let states = match (from_level, to_level) {
        (1, 2) => Some(("proposed", "agreed")),
        (2, 1) => Some(("agreed", "proposed")),
        _ => None,
    };
    if let Some((from, to)) = states {
        sqlx::query(
            "UPDATE promise SET state = ?, updated_at = ?
             WHERE transfer_uid = ? AND party_uid = ? AND state = ?",
        )
        .bind(to)
        .bind(&at)
        .bind(&input.transfer_uid)
        .bind(&input.person_uid)
        .bind(from)
        .execute(&mut **tx)
        .await?;
    }

    let evidence = TransferAgreementTransitionEvidence {
        action: "set-transfer-agreement-level".into(),
        idempotency_key: request_id.into(),
        transfer_uid: input.transfer_uid.clone(),
        revision: input.expected_revision,
        party_uid: party_uid.clone(),
        person_uid: input.person_uid.clone(),
        organ_uid: signed_party.organ_uid.clone(),
        revision_fact_hash: Some(signed_revision.get("hash")),
        from_level: from_level as u8,
        to_level: input.to_level,
    };
    let payload = serde_json::to_string(&evidence)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let previous_hash = facts::last_hash(tx).await?;
    let mut fact = nucleus::fact::seal(
        NewFact {
            uid: None,
            record_uid: input.transfer_uid.clone(),
            delta: zero(),
            at: None,
            actor_uid: Some(input.person_uid.clone()),
            cause: Cause::user_edit(),
            payload: Some(payload),
        },
        &previous_hash,
        now,
    );
    fact.signature = sign(&fact.hash);
    if fact.signature.is_none() && input.authorization_intent_uid.is_none() {
        return Err(sqlx::Error::Protocol(
            "agreement transition requires a signing key or verified Action intent".into(),
        ));
    }
    facts::insert(tx, &fact).await?;
    if let Some(intent_uid) = input.authorization_intent_uid.as_deref() {
        link_pending_fact(tx, intent_uid, &fact).await?;
    }
    sqlx::query(
        "INSERT INTO transfer_agreement_event
            (uid, transfer_uid, revision, party_uid, person_uid, from_level,
             to_level, fact_uid, idempotency_key, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&event_uid)
    .bind(&input.transfer_uid)
    .bind(expected_revision)
    .bind(&party_uid)
    .bind(&input.person_uid)
    .bind(from_level)
    .bind(to_level)
    .bind(&fact.uid)
    .bind(request_id)
    .bind(&at)
    .execute(&mut **tx)
    .await?;

    if from_level == 2 && to_level == 1 {
        sqlx::query(
            "UPDATE transfer_occurrence
             SET disputed = 1, system_disputed = 1,
                 system_dispute_fact_uid = ?, system_disputed_at = ?,
                 dispute_fact_uid = ?, disputed_at = ?
             WHERE transfer_uid = ?
               AND (giver_person_uid = ? OR receiver_person_uid = ?)",
        )
        .bind(&fact.uid)
        .bind(&at)
        .bind(&fact.uid)
        .bind(&at)
        .bind(&input.transfer_uid)
        .bind(&input.person_uid)
        .bind(&input.person_uid)
        .execute(&mut **tx)
        .await?;
    }

    if transfer.get::<&str, _>("agreement_type") == "percentage" && to_level == 2 {
        freeze_if_ready(
            tx,
            &input.transfer_uid,
            expected_revision,
            transfer
                .get::<Option<i64>, _>("agreement_pct")
                .unwrap_or(100),
            &event_uid,
            &at,
            cancellation_pending,
        )
        .await?;
    }
    bump_quantity(tx, &input.transfer_uid, zero(), &at).await?;
    crate::transfer_loans::accept_ready_on(tx, &input.transfer_uid, &fact, now).await?;
    let event = AgreementTransitionEventRow {
        uid: event_uid,
        transfer_uid: input.transfer_uid,
        revision: input.expected_revision,
        party_uid,
        person_uid: input.person_uid,
        from_level: from_level as u8,
        to_level: input.to_level,
        fact_uid: fact.uid.clone(),
        idempotency_key: request_id.into(),
        created_at: at,
    };
    let coalition = agreement_coalition_on(tx, &event.transfer_uid, event.revision)
        .await?
        .filter(|coalition| coalition.frozen_by_event_uid == event.uid);
    Ok(AgreementTransitionCommit::Committed(
        AgreementTransitionOutcome {
            event,
            fact,
            coalition,
        },
    ))
}
