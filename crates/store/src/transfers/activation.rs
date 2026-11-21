use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use nucleus::{
    Cause, Fact, NewFact,
    transfer::{
        AgreementGuard, TransferActivationEvidence, TransferRevisionEvidence, karma::Guard,
        receipt::original_authority_records,
    },
};
use sqlx::{Row, SqlitePool};

use super::{
    TransferOccurrenceRow, ensure_phase4_request_unused, karma_snapshot,
    occurrence_rows::occurrences_for_activation_event, participation, validate_request_key,
};
use crate::StoreError;

mod paths;

/// The promise and participants that form one activation path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OccurrenceActivationInput {
    pub promise_uid: String,
    pub opposite_promise_uid: Option<String>,
    pub giver_person_uid: String,
    pub receiver_person_uid: String,
}

/// An activation request for a specific signed Transfer revision.
#[derive(Debug, Clone)]
pub struct ActivateOccurrencesInput {
    pub transfer_uid: String,
    pub expected_revision: u64,
    pub idempotency_key: String,
    pub actor_person_uid: String,
    pub occurrences: Vec<OccurrenceActivationInput>,
    pub original_authority_records: Vec<String>,
    pub authorization_intent_uid: Option<String>,
}

/// The activation event, signed evidence, and resulting occurrences.
#[derive(Debug, Clone)]
pub struct OccurrenceActivationOutcome {
    pub transfer_uid: String,
    pub revision: u64,
    pub actor_person_uid: String,
    pub event_uid: String,
    pub fact: Fact,
    pub occurrences: Vec<TransferOccurrenceRow>,
}

/// Whether activation succeeded, replayed, or found a superseded revision or source.
#[derive(Debug, Clone)]
pub enum OccurrenceActivationCommit {
    Committed(OccurrenceActivationOutcome),
    Replayed(OccurrenceActivationOutcome),
    Stale {
        transfer_uid: String,
        current_revision: u64,
    },
    Satiated {
        winner_transfer_uid: String,
    },
}

/// Finds an activation by request key and returns its occurrences in UID order.
///
/// # Errors
///
/// Returns a storage error if the event or its evidence cannot be read.
pub async fn occurrences_for_activation_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<OccurrenceActivationOutcome>, StoreError> {
    let Some(event) = sqlx::query(
        "SELECT uid, transfer_uid, revision, actor_person_uid, fact_uid
         FROM transfer_activation_event
         WHERE idempotency_key = ?",
    )
    .bind(request_id.trim())
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };
    let event_uid: String = event.get("uid");
    let transfer_uid: String = event.get("transfer_uid");
    let revision = event.get::<i64, _>("revision").cast_unsigned();
    let actor_person_uid: String = event.get("actor_person_uid");
    let fact_uid: String = event.get("fact_uid");
    let occurrences = occurrences_for_activation_event(pool, &event_uid).await?;
    let fact = crate::facts::get(pool, &fact_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    Ok(Some(OccurrenceActivationOutcome {
        transfer_uid,
        revision,
        actor_person_uid,
        event_uid,
        fact,
        occurrences,
    }))
}

/// Activates the requested paths after checking the current revision and agreements.
///
/// # Errors
///
/// Returns the same errors as [`activate_occurrences_guarded`].
pub async fn activate_occurrences<F>(
    pool: &SqlitePool,
    input: ActivateOccurrencesInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<OccurrenceActivationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    activate_occurrences_guarded(pool, input, None, None, now, sign).await
}

/// Activates the paths atomically, checking any supplied agreement and state guards.
///
/// # Errors
///
/// Returns an error for an invalid request, changed guard, unauthorized or
/// unready path, missing signed evidence, or storage failure.
pub async fn activate_occurrences_guarded<F>(
    pool: &SqlitePool,
    input: ActivateOccurrencesInput,
    expected: Option<&AgreementGuard>,
    expected_state: Option<&Guard>,
    now: DateTime<Utc>,
    sign: F,
) -> Result<OccurrenceActivationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_id = validate_request_key(&input.idempotency_key)?;
    let original_authority_records = original_authority_records(&input.original_authority_records)
        .map_err(sqlx::Error::Protocol)?;
    if let Some(outcome) = occurrences_for_activation_request(pool, request_id).await? {
        let original: TransferActivationEvidence =
            serde_json::from_str(outcome.fact.payload.as_deref().ok_or_else(|| {
                sqlx::Error::Protocol("missing original activation evidence".into())
            })?)
            .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
        let mut replayed: HashMap<_, _> = outcome
            .occurrences
            .iter()
            .map(|row| (row.promise_uid.as_str(), row))
            .collect();
        let exact_terms = input.occurrences.iter().all(|requested| {
            replayed
                .remove(requested.promise_uid.as_str())
                .is_some_and(|existing| {
                    existing.opposite_promise_uid == requested.opposite_promise_uid
                        && existing.giver_person_uid == requested.giver_person_uid
                        && existing.receiver_person_uid == requested.receiver_person_uid
                })
        }) && replayed.is_empty();
        if outcome.transfer_uid != input.transfer_uid
            || outcome.revision != input.expected_revision
            || outcome.actor_person_uid != input.actor_person_uid
            || !exact_terms
            || original.original_authority_records != original_authority_records
        {
            return Err(sqlx::Error::Protocol(
                "activation request id was already used with different targets".into(),
            ));
        }
        return Ok(OccurrenceActivationCommit::Replayed(outcome));
    }
    if input.occurrences.is_empty() {
        return Err(sqlx::Error::Protocol(
            "occurrence activation requires at least one promise".into(),
        ));
    }
    let mut unique = HashSet::with_capacity(input.occurrences.len());
    if !input
        .occurrences
        .iter()
        .all(|item| unique.insert(item.promise_uid.as_str()))
    {
        return Err(sqlx::Error::Protocol(
            "occurrence activation repeats a promise".into(),
        ));
    }
    ensure_phase4_request_unused(pool, request_id).await?;
    let expected_revision = i64::try_from(input.expected_revision)
        .map_err(|_| sqlx::Error::Protocol("transfer revision exceeds SQLite range".into()))?;
    let mut tx = crate::write_tx(pool).await?;
    karma_snapshot::check_on(&mut tx, &input.transfer_uid, expected_state).await?;
    karma_snapshot::check_participant_on(
        &mut tx,
        &input.transfer_uid,
        &input.actor_person_uid,
        expected,
    )
    .await?;
    let (current_revision, agreement_type): (i64, String) =
        sqlx::query_as("SELECT revision, agreement_type FROM transfer WHERE record_uid = ?")
            .bind(&input.transfer_uid)
            .fetch_one(&mut *tx)
            .await?;
    if current_revision != expected_revision {
        tx.rollback().await?;
        return Ok(OccurrenceActivationCommit::Stale {
            transfer_uid: input.transfer_uid,
            current_revision: current_revision.cast_unsigned(),
        });
    }
    let satiated_by: Option<String> = sqlx::query_scalar(
        "SELECT result.transfer_uid FROM transfer sibling
         JOIN transfer_source_group_result result
           ON result.source_uid = sibling.source_uid
          AND result.policy = sibling.satiation
         WHERE sibling.record_uid = ? AND result.transfer_uid != sibling.record_uid",
    )
    .bind(&input.transfer_uid)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(winner_transfer_uid) = satiated_by {
        tx.rollback().await?;
        return Ok(OccurrenceActivationCommit::Satiated {
            winner_transfer_uid,
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
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| {
        sqlx::Error::Protocol(
            "occurrence activation requires an existing signed Transfer revision".into(),
        )
    })?;
    let revision_evidence: TransferRevisionEvidence =
        serde_json::from_str(signed_revision.get::<&str, _>("payload"))
            .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let original_authorities = participation::read_on(
        &mut tx,
        &original_authority_records,
        &input,
        &revision_evidence,
        signed_revision.get("hash"),
        now,
    )
    .await?;
    let party = sqlx::query(
        "SELECT tp.uid, coalesce(ta.level, 0) AS level
         FROM transfer_party tp
         JOIN record person ON person.uid = tp.actor_uid AND person.kind = 'person'
         LEFT JOIN transfer_agreement ta ON ta.transfer_uid = tp.transfer_uid
              AND ta.party_uid = tp.uid AND ta.revision = ?
         WHERE tp.transfer_uid = ? AND tp.actor_uid = ?",
    )
    .bind(expected_revision)
    .bind(&input.transfer_uid)
    .bind(&input.actor_person_uid)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| {
        sqlx::Error::Protocol("occurrence activation requires an accepted Person party".into())
    })?;
    if agreement_type != "dependency" && party.get::<i64, _>("level") != 2 {
        return Err(sqlx::Error::Protocol(
            "occurrence activation requires the acting Person's current agreement".into(),
        ));
    }
    if agreement_type == "percentage" {
        let member: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM transfer_agreement_coalition_member
             WHERE transfer_uid = ? AND revision = ? AND party_uid = ?)",
        )
        .bind(&input.transfer_uid)
        .bind(expected_revision)
        .bind(party.get::<&str, _>("uid"))
        .fetch_one(&mut *tx)
        .await?;
        if !member {
            return Err(sqlx::Error::Protocol(
                "acting Person is outside the frozen percentage coalition".into(),
            ));
        }
    }

    let readiness = crate::transfer_agreement::read_on(&mut tx, &input.transfer_uid)
        .await?
        .0;
    if input.occurrences.iter().any(|occurrence| {
        !readiness
            .promises
            .get(&occurrence.promise_uid)
            .is_some_and(|promise| promise.ready)
    }) {
        return Err(sqlx::Error::Protocol(
            "current parent, party or dependency agreement is not ready".into(),
        ));
    }
    let at = now.to_rfc3339();
    let activation_event_uid = nucleus::new_uid("tac");
    let occurrences = paths::prepare(
        &mut tx,
        &input,
        expected_revision,
        &agreement_type,
        &revision_evidence,
        &at,
    )
    .await?;
    let evidence = TransferActivationEvidence {
        action: "activate-transfer-occurrence".into(),
        idempotency_key: request_id.into(),
        transfer_uid: input.transfer_uid.clone(),
        revision: input.expected_revision,
        actor_person_uid: input.actor_person_uid.clone(),
        revision_fact_hash: Some(signed_revision.get("hash")),
        original_authorities,
        original_authority_records,
        occurrences,
    };
    let payload = serde_json::to_string(&evidence)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let previous_hash = crate::facts::last_hash(&mut tx).await?;
    let mut fact = nucleus::fact::seal(
        NewFact {
            uid: None,
            record_uid: input.transfer_uid.clone(),
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
            "occurrence activation requires a signing key or verified Action intent".into(),
        ));
    }
    crate::facts::insert(&mut tx, &fact).await?;
    if let Some(intent_uid) = input.authorization_intent_uid.as_deref() {
        crate::action_intents::link_pending_fact(&mut tx, intent_uid, &fact).await?;
    }
    sqlx::query(
        "INSERT INTO transfer_phase4_request
            (idempotency_key, kind, target_uid, fact_uid, created_at)
         VALUES (?, 'activation', ?, ?, ?)",
    )
    .bind(request_id)
    .bind(&input.transfer_uid)
    .bind(&fact.uid)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO transfer_activation_event
            (uid, transfer_uid, revision, actor_person_uid, fact_uid,
             idempotency_key, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&activation_event_uid)
    .bind(&input.transfer_uid)
    .bind(expected_revision)
    .bind(&input.actor_person_uid)
    .bind(&fact.uid)
    .bind(request_id)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    for snapshot in &evidence.occurrences {
        let location = snapshot.location.as_ref();
        sqlx::query(
            "INSERT INTO transfer_occurrence
                (uid, transfer_uid, revision, promise_uid, exchange_path_uid,
                 activation_event_uid, record_uid, concept_uid, unit_uid,
                 quantity, giver_person_uid, receiver_person_uid, window_start,
                 window_end, location_lat, location_lon, location_address, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&snapshot.uid)
        .bind(&input.transfer_uid)
        .bind(expected_revision)
        .bind(&snapshot.promise_uid)
        .bind(&snapshot.exchange_path_uid)
        .bind(&activation_event_uid)
        .bind(&snapshot.record_uid)
        .bind(&snapshot.concept_uid)
        .bind(&snapshot.unit_uid)
        .bind(snapshot.quantity)
        .bind(&snapshot.giver_person_uid)
        .bind(&snapshot.receiver_person_uid)
        .bind(&snapshot.window_start)
        .bind(&snapshot.window_end)
        .bind(location.and_then(|value| value.lat))
        .bind(location.and_then(|value| value.lon))
        .bind(location.and_then(|value| value.address.as_deref()))
        .bind(&at)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE promise SET state = 'active', updated_at = ?
             WHERE uid = ? AND transfer_uid = ? AND (state = 'agreed' OR
               (state = 'proposed' AND EXISTS(SELECT 1 FROM transfer WHERE record_uid = ? AND agreement_type = 'dependency')))",
        )
        .bind(&at)
        .bind(&snapshot.promise_uid)
        .bind(&input.transfer_uid)
        .bind(&input.transfer_uid)
        .execute(&mut *tx)
        .await?;
    }
    crate::records::bump_quantity(&mut tx, &input.transfer_uid, crate::exact::zero(), &at).await?;
    tx.commit().await?;
    let outcome = occurrences_for_activation_request(pool, request_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    Ok(OccurrenceActivationCommit::Committed(outcome))
}
