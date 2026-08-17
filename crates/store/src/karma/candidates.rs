use chrono::{DateTime, TimeZone, Utc};
use nucleus::karma::{
    CandidateReviewAction, CandidateReviewEvidence, CandidateReviewEvidenceSchema, CandidateRoute,
    CandidateStatus, CanonicalHash, KarmaCandidateProposal, ReferenceKind, TimestampMs, TypedUid,
    canonical_hash, canonical_json_bytes,
};
use nucleus::{Cause, CauseKind, Fact, NewFact};
use serde::{Deserialize, Serialize};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::StoreError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KarmaCandidateRow {
    pub candidate_hash: CanonicalHash,
    pub proposal: KarmaCandidateProposal,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KarmaCandidateStateRow {
    pub candidate_hash: CanonicalHash,
    pub state_revision: u64,
    pub status: CandidateStatus,
    pub snoozed_until: Option<TimestampMs>,
    pub current_event_hash: Option<CanonicalHash>,
    pub actor_person_uid: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct RespondCandidateInput {
    pub request_id: String,
    pub candidate_hash: CanonicalHash,
    pub expected_state_revision: u64,
    pub response: CandidateReviewAction,
    pub actor_person_uid: Option<String>,
    /// K5.2: accepting an `act` proposal *and* authorizing it are one commit.
    /// Left empty, acceptance stays inert exactly as it was in K4.3. Naming a
    /// grant makes this transaction check authority and reserve budget too, and
    /// exactly one grant may be named — never a union of matching grants.
    pub authorizing_grant_uid: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CandidateReviewCommit {
    Committed {
        state: KarmaCandidateStateRow,
        fact: Fact,
        intent: Option<CanonicalHash>,
        /// The intent's own lifecycle Fact, committed in the same transaction.
        intent_fact: Option<Fact>,
    },
    Replayed {
        state: KarmaCandidateStateRow,
        fact: Fact,
        intent: Option<CanonicalHash>,
    },
    Stale {
        current_state_revision: u64,
    },
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<KarmaCandidateRow>, StoreError> {
    sqlx::query("SELECT * FROM karma_candidate ORDER BY created_at, candidate_hash")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(map_row)
        .collect()
}

pub async fn get_state(
    pool: &SqlitePool,
    candidate_hash: &CanonicalHash,
) -> Result<Option<KarmaCandidateStateRow>, StoreError> {
    let row = sqlx::query("SELECT * FROM karma_candidate_state WHERE candidate_hash = ?")
        .bind(candidate_hash.as_str())
        .fetch_optional(pool)
        .await?;
    row.map(map_state).transpose()
}

pub async fn list_states(pool: &SqlitePool) -> Result<Vec<KarmaCandidateStateRow>, StoreError> {
    sqlx::query("SELECT * FROM karma_candidate_state ORDER BY updated_at, candidate_hash")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(map_state)
        .collect()
}

pub async fn respond<F>(
    pool: &SqlitePool,
    input: RespondCandidateInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<CandidateReviewCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    if input.request_id.is_empty()
        || input.request_id.len() > 200
        || input.request_id.trim() != input.request_id
        || input.request_id.chars().any(char::is_control)
    {
        return Err(protocol(
            "candidate review request id must contain 1 to 200 trimmed non-control bytes",
        ));
    }
    if let Some(actor_person_uid) = &input.actor_person_uid {
        TypedUid::new(ReferenceKind::Person, actor_person_uid.clone())
            .map_err(|error| protocol(format!("invalid candidate review actor: {error}")))?;
    }
    if input.expected_state_revision == 0 {
        return Err(protocol("candidate review state revision must be positive"));
    }
    let now = canonical_time(now)?;
    let now_timestamp = TimestampMs::from_millis(now.timestamp_millis()).map_err(boundary)?;
    if let CandidateReviewAction::Snooze { until } = &input.response
        && *until <= now_timestamp
    {
        return Err(protocol("candidate snooze instant must be in the future"));
    }
    let fingerprint = canonical_hash(
        "karma.candidate-review-request.v1",
        &CandidateReviewFingerprint {
            candidate_hash: &input.candidate_hash,
            expected_state_revision: input.expected_state_revision,
            response: &input.response,
            actor_person_uid: input.actor_person_uid.as_deref(),
            authorizing_grant_uid: input.authorizing_grant_uid.as_deref(),
        },
    )
    .map_err(boundary)?;
    let at = now.to_rfc3339();
    let mut tx = crate::write_tx(pool).await?;
    if let Some(row) = sqlx::query(
        "SELECT request.payload_hash AS global_payload_hash,
                review.payload_hash, review.result_json, review.fact_uid
         FROM karma_candidate_review_request review
         JOIN karma_request request ON request.request_id = review.request_id
         WHERE review.request_id = ? AND request.family = 'candidate-review'",
    )
    .bind(&input.request_id)
    .fetch_optional(&mut *tx)
    .await?
    {
        if row.get::<String, _>("payload_hash") != fingerprint.as_str()
            || row.get::<String, _>("global_payload_hash") != fingerprint.as_str()
        {
            return Err(protocol(
                "candidate review request id was replayed with different content",
            ));
        }
        let result_json: String = row.get("result_json");
        let state: KarmaCandidateStateRow =
            serde_json::from_str(&result_json).map_err(json_protocol)?;
        if canonical_string(&state)? != result_json {
            return Err(protocol("stored candidate review replay result is invalid"));
        }
        let fact_uid: String = row.get("fact_uid");
        let fact = crate::facts::get_in_transaction(&mut tx, &fact_uid)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
        // A candidate has at most one intent, so the replay reports the same one
        // the original commit created rather than minting a second.
        let intent = intent_for_candidate_tx(&mut tx, &input.candidate_hash).await?;
        tx.rollback().await?;
        return Ok(CandidateReviewCommit::Replayed {
            state,
            fact,
            intent,
        });
    }
    let candidate = get_candidate_tx(&mut tx, &input.candidate_hash)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let current = get_state_tx(&mut tx, &input.candidate_hash)
        .await?
        .ok_or_else(|| protocol("candidate current state is missing"))?;
    if current.state_revision != input.expected_state_revision {
        tx.rollback().await?;
        return Ok(CandidateReviewCommit::Stale {
            current_state_revision: current.state_revision,
        });
    }
    let status = input.response.resulting_status();
    if current.status == status {
        return Err(protocol(
            "candidate review must change status or replay the original request",
        ));
    }
    let state_revision = current
        .state_revision
        .checked_add(1)
        .ok_or_else(|| protocol("candidate state revision overflowed"))?;
    let evidence = CandidateReviewEvidence {
        schema: CandidateReviewEvidenceSchema::V1,
        request_id: input.request_id.clone(),
        candidate_hash: input.candidate_hash.clone(),
        actor_person_uid: input.actor_person_uid.clone(),
        previous_state_revision: current.state_revision,
        state_revision,
        action: input.response.clone(),
        status,
    };
    let evidence_json = canonical_string(&evidence)?;
    let event_hash =
        canonical_hash("karma.candidate-review-event.v1", &evidence).map_err(boundary)?;
    sqlx::query(
        "INSERT INTO karma_request (request_id, family, payload_hash, created_at)
         VALUES (?, 'candidate-review', ?, ?)",
    )
    .bind(&input.request_id)
    .bind(fingerprint.as_str())
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    let previous = crate::facts::last_hash(&mut tx).await?;
    let mut fact = nucleus::fact::seal(
        NewFact {
            uid: None,
            record_uid: candidate.proposal.program_uid.clone(),
            delta: crate::exact::zero(),
            at: Some(now),
            actor_uid: input.actor_person_uid.clone(),
            cause: Cause {
                kind: CauseKind::Action,
                uid: Some(input.request_id.clone()),
            },
            payload: Some(evidence_json.clone()),
        },
        &previous,
        now,
    );
    fact.signature = sign(&fact.hash);
    crate::facts::insert(&mut tx, &fact).await?;
    crate::records::bump_quantity(
        &mut tx,
        &candidate.proposal.program_uid,
        crate::exact::zero(),
        &at,
    )
    .await?;
    let snoozed_until = match &input.response {
        CandidateReviewAction::Snooze { until } => Some(*until),
        CandidateReviewAction::Accept | CandidateReviewAction::Dismiss => None,
    };
    sqlx::query(
        "INSERT INTO karma_candidate_review_event
            (event_hash, candidate_hash, state_revision, previous_event_hash,
             request_id, action, status, snoozed_until, actor_person_uid,
             evidence_json, fact_uid, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(event_hash.as_str())
    .bind(input.candidate_hash.as_str())
    .bind(sql_i64(state_revision, "candidate state revision")?)
    .bind(
        current
            .current_event_hash
            .as_ref()
            .map(CanonicalHash::as_str),
    )
    .bind(&input.request_id)
    .bind(input.response.action_name())
    .bind(candidate_status_name(status))
    .bind(snoozed_until.map(|value| value.to_string()))
    .bind(&input.actor_person_uid)
    .bind(&evidence_json)
    .bind(&fact.uid)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    let updated = sqlx::query(
        "UPDATE karma_candidate_state
         SET state_revision = ?, status = ?, snoozed_until = ?, current_event_hash = ?,
             actor_person_uid = ?, updated_at = ?
         WHERE candidate_hash = ? AND state_revision = ?",
    )
    .bind(sql_i64(state_revision, "candidate state revision")?)
    .bind(candidate_status_name(status))
    .bind(snoozed_until.map(|value| value.to_string()))
    .bind(event_hash.as_str())
    .bind(&input.actor_person_uid)
    .bind(&at)
    .bind(input.candidate_hash.as_str())
    .bind(sql_i64(current.state_revision, "candidate state revision")?)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(protocol("candidate review lost state CAS serialization"));
    }
    let state = get_state_tx(&mut tx, &input.candidate_hash)
        .await?
        .expect("reviewed candidate state exists");
    sqlx::query(
        "INSERT INTO karma_candidate_review_request
            (request_id, payload_hash, candidate_hash, expected_state_revision,
             result_state_revision, result_json, event_hash, fact_uid, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&input.request_id)
    .bind(fingerprint.as_str())
    .bind(input.candidate_hash.as_str())
    .bind(sql_i64(
        input.expected_state_revision,
        "candidate state revision",
    )?)
    .bind(sql_i64(state_revision, "candidate state revision")?)
    .bind(canonical_string(&state)?)
    .bind(event_hash.as_str())
    .bind(&fact.uid)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    // Accepting and authorizing are one commit: there is no window in which a
    // proposal is accepted but its authority was never checked.
    let (intent, intent_fact) = match &input.authorizing_grant_uid {
        None => (None, None),
        Some(grant_uid) => {
            if status != CandidateStatus::Accepted {
                return Err(protocol(
                    "only an accepted candidate can name an authorizing Karma grant",
                ));
            }
            let actor = input
                .actor_person_uid
                .as_deref()
                .ok_or_else(|| protocol("authorizing a Karma intent requires an acting Person"))?;
            let intent = crate::karma::intents::authorize_accepted_candidate_tx(
                &mut tx,
                &input.candidate_hash,
                &candidate.proposal,
                grant_uid,
                &input.request_id,
                actor,
                now,
                &at,
            )
            .await?;
            let intent_hash = intent.intent_hash().map_err(boundary)?;
            let intent_fact = crate::karma::intents::append_intent_fact(
                &mut tx,
                &candidate.proposal.program_uid,
                &input.request_id,
                actor,
                &intent,
                now,
                &sign,
            )
            .await?;
            (Some(intent_hash), Some(intent_fact))
        }
    };
    tx.commit().await?;
    Ok(CandidateReviewCommit::Committed {
        state,
        fact,
        intent,
        intent_fact,
    })
}

async fn intent_for_candidate_tx(
    tx: &mut Transaction<'_, Sqlite>,
    candidate_hash: &CanonicalHash,
) -> Result<Option<CanonicalHash>, StoreError> {
    let stored: Option<String> =
        sqlx::query_scalar("SELECT intent_hash FROM karma_intent WHERE candidate_hash = ?")
            .bind(candidate_hash.as_str())
            .fetch_optional(&mut **tx)
            .await?;
    stored
        .map(|value| CanonicalHash::parse(value).map_err(boundary))
        .transpose()
}

pub(crate) async fn insert_tx(
    tx: &mut Transaction<'_, Sqlite>,
    proposal: &KarmaCandidateProposal,
    at: &str,
) -> Result<KarmaCandidateRow, StoreError> {
    proposal.validate().map_err(boundary)?;
    let candidate_hash = proposal.candidate_hash().map_err(boundary)?;
    let proposal_json = canonical_string(proposal)?;
    let inserted = sqlx::query(
        "INSERT OR IGNORE INTO karma_candidate
            (candidate_hash, source_run_hash, occurrence_hash, program_uid,
             program_revision_hash, node_id, output_port, route, template,
             status, proposal_json, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'proposed', ?, ?)",
    )
    .bind(candidate_hash.as_str())
    .bind(proposal.source_run_hash.as_str())
    .bind(proposal.occurrence_hash.as_str())
    .bind(&proposal.program_uid)
    .bind(proposal.program_revision_hash.as_str())
    .bind(proposal.node_id.as_str())
    .bind(proposal.output.as_str())
    .bind(route_name(proposal.route))
    .bind(proposal.template.as_str())
    .bind(proposal_json)
    .bind(at)
    .execute(&mut **tx)
    .await?
    .rows_affected()
        == 1;
    let row = sqlx::query(
        "SELECT * FROM karma_candidate
         WHERE source_run_hash = ? AND node_id = ? AND output_port = ?",
    )
    .bind(proposal.source_run_hash.as_str())
    .bind(proposal.node_id.as_str())
    .bind(proposal.output.as_str())
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| protocol("inserted Karma candidate is missing"))?;
    let row = map_row(row)?;
    if !inserted && (row.candidate_hash != candidate_hash || row.proposal != *proposal) {
        return Err(protocol(
            "Karma run node output already has a different candidate proposal",
        ));
    }
    sqlx::query(
        "INSERT OR IGNORE INTO karma_candidate_state
            (candidate_hash, state_revision, status, snoozed_until,
             current_event_hash, actor_person_uid, updated_at)
         VALUES (?, 1, 'proposed', NULL, NULL, NULL, ?)",
    )
    .bind(candidate_hash.as_str())
    .bind(at)
    .execute(&mut **tx)
    .await?;
    Ok(row)
}

#[derive(Serialize)]
struct CandidateReviewFingerprint<'a> {
    candidate_hash: &'a CanonicalHash,
    expected_state_revision: u64,
    response: &'a CandidateReviewAction,
    actor_person_uid: Option<&'a str>,
    /// Part of the fingerprint: replaying the same request id against a
    /// different grant is a different request and must be refused.
    #[serde(skip_serializing_if = "Option::is_none")]
    authorizing_grant_uid: Option<&'a str>,
}

async fn get_candidate_tx(
    tx: &mut Transaction<'_, Sqlite>,
    candidate_hash: &CanonicalHash,
) -> Result<Option<KarmaCandidateRow>, StoreError> {
    let row = sqlx::query("SELECT * FROM karma_candidate WHERE candidate_hash = ?")
        .bind(candidate_hash.as_str())
        .fetch_optional(&mut **tx)
        .await?;
    row.map(map_row).transpose()
}

async fn get_state_tx(
    tx: &mut Transaction<'_, Sqlite>,
    candidate_hash: &CanonicalHash,
) -> Result<Option<KarmaCandidateStateRow>, StoreError> {
    let row = sqlx::query("SELECT * FROM karma_candidate_state WHERE candidate_hash = ?")
        .bind(candidate_hash.as_str())
        .fetch_optional(&mut **tx)
        .await?;
    row.map(map_state).transpose()
}

fn map_state(row: sqlx::sqlite::SqliteRow) -> Result<KarmaCandidateStateRow, StoreError> {
    let status = parse_candidate_status(&row.get::<String, _>("status"))?;
    let snoozed_until = row
        .get::<Option<String>, _>("snoozed_until")
        .map(|value| TimestampMs::parse_canonical(&value))
        .transpose()
        .map_err(boundary)?;
    if (status == CandidateStatus::Snoozed) != snoozed_until.is_some() {
        return Err(protocol("stored candidate snooze projection is invalid"));
    }
    Ok(KarmaCandidateStateRow {
        candidate_hash: parse_hash(row.get("candidate_hash"))?,
        state_revision: rust_u64(row.get("state_revision"), "candidate state revision")?,
        status,
        snoozed_until,
        current_event_hash: row
            .get::<Option<String>, _>("current_event_hash")
            .map(CanonicalHash::parse)
            .transpose()
            .map_err(boundary)?,
        actor_person_uid: row.get("actor_person_uid"),
        updated_at: row.get("updated_at"),
    })
}

fn map_row(row: sqlx::sqlite::SqliteRow) -> Result<KarmaCandidateRow, StoreError> {
    let candidate_hash = parse_hash(row.get("candidate_hash"))?;
    let proposal_json: String = row.get("proposal_json");
    let proposal: KarmaCandidateProposal =
        serde_json::from_str(&proposal_json).map_err(json_protocol)?;
    proposal.validate().map_err(boundary)?;
    if canonical_string(&proposal)? != proposal_json
        || proposal.candidate_hash().map_err(boundary)? != candidate_hash
        || proposal.source_run_hash.as_str() != row.get::<String, _>("source_run_hash")
        || proposal.occurrence_hash.as_str() != row.get::<String, _>("occurrence_hash")
        || proposal.program_uid != row.get::<String, _>("program_uid")
        || proposal.program_revision_hash.as_str() != row.get::<String, _>("program_revision_hash")
        || proposal.node_id.as_str() != row.get::<String, _>("node_id")
        || proposal.output.as_str() != row.get::<String, _>("output_port")
        || route_name(proposal.route) != row.get::<String, _>("route")
        || proposal.template.as_str() != row.get::<String, _>("template")
        || proposal.status != CandidateStatus::Proposed
        || row.get::<String, _>("status") != "proposed"
    {
        return Err(protocol("stored Karma candidate proposal is invalid"));
    }
    Ok(KarmaCandidateRow {
        candidate_hash,
        proposal,
        created_at: row.get("created_at"),
    })
}

fn route_name(route: CandidateRoute) -> &'static str {
    match route {
        CandidateRoute::Observe => "observe",
        CandidateRoute::Recommend => "recommend",
        CandidateRoute::Draft => "draft",
        CandidateRoute::Ask => "ask",
        CandidateRoute::Act => "act",
    }
}

fn candidate_status_name(status: CandidateStatus) -> &'static str {
    match status {
        CandidateStatus::Proposed => "proposed",
        CandidateStatus::Accepted => "accepted",
        CandidateStatus::Dismissed => "dismissed",
        CandidateStatus::Snoozed => "snoozed",
        CandidateStatus::Edited
        | CandidateStatus::Muted
        | CandidateStatus::Stale
        | CandidateStatus::Expired => "unsupported",
    }
}

fn parse_candidate_status(value: &str) -> Result<CandidateStatus, StoreError> {
    match value {
        "proposed" => Ok(CandidateStatus::Proposed),
        "accepted" => Ok(CandidateStatus::Accepted),
        "dismissed" => Ok(CandidateStatus::Dismissed),
        "snoozed" => Ok(CandidateStatus::Snoozed),
        _ => Err(protocol("stored candidate status is invalid")),
    }
}

fn canonical_time(value: DateTime<Utc>) -> Result<DateTime<Utc>, StoreError> {
    Utc.timestamp_millis_opt(value.timestamp_millis())
        .single()
        .ok_or_else(|| protocol("candidate review time is outside the supported range"))
}

fn canonical_string<T: Serialize>(value: &T) -> Result<String, StoreError> {
    String::from_utf8(canonical_json_bytes(value).map_err(boundary)?)
        .map_err(|error| protocol(error.to_string()))
}

fn parse_hash(value: String) -> Result<CanonicalHash, StoreError> {
    CanonicalHash::parse(value).map_err(boundary)
}

fn sql_i64(value: u64, name: &str) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| protocol(format!("{name} exceeds SQLite integer range")))
}

fn rust_u64(value: i64, name: &str) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|_| protocol(format!("stored {name} is invalid")))
}

fn boundary(error: nucleus::karma::KarmaBoundaryError) -> StoreError {
    protocol(error.to_string())
}

fn json_protocol(error: serde_json::Error) -> StoreError {
    protocol(error.to_string())
}

fn protocol(message: impl Into<String>) -> StoreError {
    sqlx::Error::Protocol(message.into())
}
