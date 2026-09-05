use chrono::{DateTime, Utc};
pub use nucleus::transfer::TransferLocationSnapshot;
use nucleus::transfer::{
    OccurrenceClaimRole, OpenPromiseReusePolicy, TransferActivationEvidence,
    TransferAgreementTransitionEvidence, TransferDependencyEdge, TransferDependencyNode,
    TransferDependencyScope, TransferDependencyUpstreamKind, TransferInvitationLifecycleEvidence,
    TransferOccurrenceApplicationEvidence, TransferOccurrenceClaimEvidence,
    TransferOccurrenceDisputeEvidence, TransferOccurrenceSettlementApplicationEvidence,
    TransferOccurrenceSettlementCompensationEvidence, TransferOccurrenceSettlementEvidence,
    TransferOccurrenceSnapshot, TransferRemainderPolicy, TransferRevisionDependency,
    TransferRevisionEvidence, TransferRevisionInvitation, TransferRevisionParty,
    TransferRevisionPromise, TransferRevisionSnapshot, TransferRevisionTerms,
    TransferSourceGroupLoserEvidence, TransferSourceGroupResultEvidence,
};
use nucleus::{Cause, Fact, NewFact, PromiseState, RecordKind};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use std::collections::HashSet;

use crate::StoreError;
use crate::records::{self, NewRecord};

#[derive(Debug, Clone)]
pub struct TransferRow {
    pub record_uid: String,
    pub revision: i64,
    pub agreement_type: String,
    pub agreement_pct: Option<i64>,
    pub settlement: String,
    pub visibility: String,
    pub max_proximity: Option<i64>,
    pub satiation: Option<String>,
    pub parent_uid: Option<String>,
    pub source_uid: Option<String>,
    pub reserve_default: Option<String>,
    pub active: bool,
    pub require_confirmation: bool,
    pub default_place: Option<TransferLocationSnapshot>,
}

pub struct NewTransfer<'a> {
    pub slug: Option<&'a str>,
    pub head: &'a str,
    pub agreement_type: &'a str,
    pub agreement_pct: Option<i64>,
    pub satiation: Option<&'a str>,
    pub source_uid: Option<&'a str>,
    pub reserve_default: Option<&'a str>,
    pub require_confirmation: bool,
}

#[derive(Debug, Clone)]
pub struct DraftPromise {
    pub uid: Option<String>,
    pub record_uid: Option<String>,
    pub concept_uid: Option<String>,
    pub unit_uid: Option<String>,
    pub person_uid: Option<String>,
    pub open: bool,
    pub delta: f64,
    pub window_start: Option<String>,
    pub window_end: Option<String>,
    pub location: Option<TransferLocationSnapshot>,
    pub condition: Option<String>,
    pub reserve_from: String,
    pub open_reuse_policy: OpenPromiseReusePolicy,
}

#[derive(Debug, Clone)]
pub struct NewTransferCorrectionLink {
    pub kind: String,
    pub source_transfer_uid: String,
    pub source_occurrence_uid: String,
    pub source_revision: u64,
    pub canonical_quantity: f64,
}

#[derive(Debug, Clone)]
pub struct TransferCorrectionLinkRow {
    pub uid: String,
    pub kind: String,
    pub source_transfer_uid: String,
    pub source_occurrence_uid: String,
    pub created_transfer_uid: String,
    pub source_revision: u64,
    pub canonical_quantity: f64,
    pub actor_person_uid: String,
    pub fact_uid: String,
    pub idempotency_key: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferDependencyInput {
    pub uid: Option<String>,
    pub scope: TransferDependencyScope,
    pub promise_uid: Option<String>,
    pub upstream_kind: TransferDependencyUpstreamKind,
    pub upstream_uid: String,
    pub required_state: String,
}

#[derive(Debug, Clone)]
pub struct NewTransferDraft {
    pub idempotency_key: String,
    pub slug: Option<String>,
    pub head: String,
    pub agreement_type: String,
    pub agreement_pct: Option<i64>,
    pub satiation: Option<String>,
    pub parent_uid: Option<String>,
    pub source_uid: Option<String>,
    pub visibility: String,
    pub max_proximity: Option<i64>,
    pub reserve_default: String,
    pub require_confirmation: bool,
    pub default_place: Option<TransferLocationSnapshot>,
    pub creator_person: String,
    pub invitees: Vec<String>,
    pub promises: Vec<DraftPromise>,
    pub dependencies: Vec<TransferDependencyInput>,
    pub organ_uid: Option<String>,
    pub evidence_action: String,
    pub correction: Option<NewTransferCorrectionLink>,
    pub authorization_intent_uid: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TransferDraftTermsInput {
    pub slug: Option<String>,
    pub head: String,
    pub agreement_type: String,
    pub agreement_pct: Option<i64>,
    pub settlement: String,
    pub visibility: String,
    pub max_proximity: Option<i64>,
    pub satiation: Option<String>,
    pub parent_uid: Option<String>,
    pub source_uid: Option<String>,
    pub reserve_default: String,
    pub require_confirmation: bool,
    pub default_place: Option<TransferLocationSnapshot>,
}

#[derive(Debug, Clone)]
pub struct DraftPromiseRevisionInput {
    pub uid: Option<String>,
    pub source_promise_uid: Option<String>,
    pub record_uid: Option<String>,
    pub concept_uid: Option<String>,
    pub unit_uid: Option<String>,
    pub person_uid: Option<String>,
    pub open: bool,
    pub delta: f64,
    pub window_start: Option<String>,
    pub window_end: Option<String>,
    pub location: Option<TransferLocationSnapshot>,
    pub condition: Option<String>,
    pub reserve_from: String,
    pub open_reuse_policy: OpenPromiseReusePolicy,
}

#[derive(Debug, Clone)]
pub struct WholeDraftRevisionInput {
    pub transfer_uid: String,
    pub expected_revision: u64,
    pub idempotency_key: String,
    pub creator_person_uid: String,
    pub proposal_author_person_uid: String,
    pub terms: TransferDraftTermsInput,
    pub retained_invitation_uids: Vec<String>,
    pub promises: Vec<DraftPromiseRevisionInput>,
    pub dependencies: Vec<TransferDependencyInput>,
    pub successor: Option<PromiseSuccessorInput>,
    pub authorization_intent_uid: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PromiseSuccessorInput {
    pub predecessor_promise_uid: String,
    pub successor_promise_uid: String,
}

#[derive(Debug, Clone)]
pub struct PromiseSuccessorRow {
    pub uid: String,
    pub transfer_uid: String,
    pub predecessor_promise_uid: String,
    pub successor_promise_uid: String,
    pub revision: u64,
    pub actor_person_uid: String,
    pub fact_uid: String,
    pub idempotency_key: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct CreatedTransferDraft {
    pub transfer_uid: String,
    pub revision: u64,
    pub replayed: bool,
    pub party_uids: Vec<String>,
    pub invitation_uids: Vec<String>,
    pub promise_uids: Vec<String>,
    pub fact: Fact,
    pub invitation_event_facts: Vec<Fact>,
}

#[derive(Debug, Clone)]
pub struct PromiseRevisionInput {
    pub transfer_uid: String,
    pub promise_uid: String,
    pub expected_revision: u64,
    pub idempotency_key: String,
    pub record_uid: String,
    pub person_uid: String,
    pub delta: f64,
    pub window_end: Option<String>,
    pub condition: Option<String>,
    pub reserve_from: String,
}

#[derive(Debug, Clone)]
pub enum RevisionCommit {
    Committed { revision: u64, fact: Fact },
    Replayed { revision: u64, fact: Fact },
    Stale { current_revision: u64 },
}

pub async fn revision_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<(String, u64, String)>, StoreError> {
    Ok(sqlx::query(
        "SELECT tr.transfer_uid, tr.revision, f.payload
         FROM transfer_revision tr JOIN fact f ON f.uid = tr.fact_uid
         WHERE tr.idempotency_key = ?",
    )
    .bind(request_id.trim())
    .fetch_optional(pool)
    .await?
    .map(|row| -> Result<_, StoreError> {
        let payload: String = row.get("payload");
        let evidence: TransferRevisionEvidence = serde_json::from_str(&payload)
            .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
        Ok((
            row.get("transfer_uid"),
            row.get::<i64, _>("revision") as u64,
            evidence.action,
        ))
    })
    .transpose()?)
}

pub async fn revise_whole_draft<F>(
    pool: &SqlitePool,
    input: WholeDraftRevisionInput,
    now: DateTime<Utc>,
    fact_actor: Option<String>,
    sign: F,
) -> Result<RevisionCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    commit_whole_draft(
        pool,
        input,
        now,
        fact_actor,
        "revise-transfer-draft",
        false,
        true,
        sign,
    )
    .await
}

pub async fn adopt_legacy_draft<F>(
    pool: &SqlitePool,
    input: WholeDraftRevisionInput,
    now: DateTime<Utc>,
    fact_actor: Option<String>,
    sign: F,
) -> Result<RevisionCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    if input.expected_revision != 0 {
        return Err(sqlx::Error::Protocol(
            "legacy Transfer adoption requires expected revision 0".into(),
        ));
    }
    commit_whole_draft(
        pool,
        input,
        now,
        fact_actor,
        "adopt-legacy-transfer-draft",
        true,
        true,
        sign,
    )
    .await
}

pub async fn counteroffer_whole_draft<F>(
    pool: &SqlitePool,
    input: WholeDraftRevisionInput,
    now: DateTime<Utc>,
    fact_actor: Option<String>,
    sign: F,
) -> Result<RevisionCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    commit_whole_draft(
        pool,
        input,
        now,
        fact_actor,
        "counteroffer-transfer-draft",
        false,
        false,
        sign,
    )
    .await
}

pub async fn reopen_promise_revision<F>(
    pool: &SqlitePool,
    input: WholeDraftRevisionInput,
    now: DateTime<Utc>,
    fact_actor: Option<String>,
    sign: F,
) -> Result<RevisionCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    if input.successor.is_none() {
        return Err(sqlx::Error::Protocol(
            "reopen promise revision requires successor lineage".into(),
        ));
    }
    commit_whole_draft(
        pool,
        input,
        now,
        fact_actor,
        "reopen-transfer-promise",
        false,
        false,
        sign,
    )
    .await
}

async fn commit_whole_draft<F>(
    pool: &SqlitePool,
    input: WholeDraftRevisionInput,
    now: DateTime<Utc>,
    fact_actor: Option<String>,
    action: &'static str,
    allow_legacy: bool,
    mark_submitting_party_as_creator: bool,
    sign: F,
) -> Result<RevisionCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_key = input.idempotency_key.trim();
    let expected_revision = i64::try_from(input.expected_revision)
        .map_err(|_| sqlx::Error::Protocol("transfer revision exceeds SQLite range".into()))?;
    validate_revision_request(&input)?;
    ensure_request_not_used_by_invitation_event(pool, request_key).await?;
    let mut tx = crate::write_tx(pool).await?;

    if let Some(row) = sqlx::query(
        "SELECT transfer_uid, revision, fact_uid FROM transfer_revision
         WHERE idempotency_key = ?",
    )
    .bind(request_key)
    .fetch_optional(&mut *tx)
    .await?
    {
        let existing_transfer: String = row.get("transfer_uid");
        if existing_transfer != input.transfer_uid {
            return Err(sqlx::Error::Protocol(
                "transfer request id was already used for another transfer".into(),
            ));
        }
        let revision = row.get::<i64, _>("revision") as u64;
        let fact_uid: String = row.get("fact_uid");
        tx.rollback().await?;
        let correction = correction_link_for_request(pool, request_key).await?;
        let successor = promise_successor_for_request(pool, request_key).await?;
        let open_claim = open_claim_pair_for_request(pool, request_key).await?;
        match (&input.successor, successor.as_ref()) {
            (Some(requested), Some(existing))
                if correction.is_none()
                    && open_claim.is_none()
                    && existing.transfer_uid == input.transfer_uid
                    && existing.predecessor_promise_uid == requested.predecessor_promise_uid
                    && existing.successor_promise_uid == requested.successor_promise_uid => {}
            (None, None) if correction.is_none() && open_claim.is_none() => {}
            _ => {
                return Err(sqlx::Error::Protocol(
                    "transfer request id was replayed as a different action".into(),
                ));
            }
        }
        let fact = crate::facts::get(pool, &fact_uid)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
        if !fact_has_transfer_revision_action(&fact, action) {
            return Err(sqlx::Error::Protocol(
                "transfer request id was replayed as a different revision action".into(),
            ));
        }
        return Ok(RevisionCommit::Replayed { revision, fact });
    }

    if let Some(successor) = &input.successor {
        let predecessor = sqlx::query(
            "SELECT transfer_uid, party_uid, state, window_end
             FROM promise WHERE uid = ?",
        )
        .bind(&successor.predecessor_promise_uid)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
        let state: String = predecessor.get("state");
        let expired = matches!(state.as_str(), "proposed" | "agreed")
            && predecessor
                .get::<Option<String>, _>("window_end")
                .as_deref()
                .and_then(|window_end| DateTime::parse_from_rfc3339(window_end).ok())
                .is_some_and(|window_end| window_end.with_timezone(&Utc) <= now);
        let successor_terms = input.promises.iter().find(|promise| {
            promise.uid.as_deref() == Some(successor.successor_promise_uid.as_str())
        });
        if predecessor.get::<String, _>("transfer_uid") != input.transfer_uid
            || predecessor.get::<Option<String>, _>("party_uid").as_deref()
                != Some(input.proposal_author_person_uid.as_str())
            || (!matches!(state.as_str(), "broken" | "withdrawn") && !expired)
            || successor_terms.is_none_or(|terms| {
                terms.source_promise_uid.as_deref()
                    != Some(successor.predecessor_promise_uid.as_str())
                    || terms.person_uid.as_deref()
                        != Some(input.proposal_author_person_uid.as_str())
            })
        {
            return Err(sqlx::Error::Protocol(
                "promise successor authority or lineage is invalid".into(),
            ));
        }
    } else if input
        .promises
        .iter()
        .any(|promise| promise.source_promise_uid.is_some())
    {
        return Err(sqlx::Error::Protocol(
            "promise source lineage requires a reviewed successor operation".into(),
        ));
    }

    if expected_revision == 0 && !allow_legacy {
        return Err(sqlx::Error::Protocol(
            "legacy revision-0 Transfer requires explicit adoption".into(),
        ));
    }
    let revision = expected_revision
        .checked_add(1)
        .ok_or_else(|| sqlx::Error::Protocol("transfer revision overflow".into()))?;
    validate_transfer_parent(
        &mut tx,
        &input.transfer_uid,
        input.terms.parent_uid.as_deref(),
    )
    .await?;
    let at = now.to_rfc3339();
    let location = input.terms.default_place.as_ref();
    let advanced = sqlx::query(
        "UPDATE transfer SET
            agreement_type = ?, agreement_pct = ?, settlement = ?, visibility = ?,
            max_proximity = ?, satiation = ?, parent_uid = ?, source_uid = ?,
            reserve_default = ?, require_confirmation = ?,
            default_location_lat = ?, default_location_lon = ?,
            default_location_address = ?, revision = ?
         WHERE record_uid = ? AND revision = ?",
    )
    .bind(&input.terms.agreement_type)
    .bind(input.terms.agreement_pct)
    .bind(&input.terms.settlement)
    .bind(&input.terms.visibility)
    .bind(input.terms.max_proximity)
    .bind(&input.terms.satiation)
    .bind(&input.terms.parent_uid)
    .bind(&input.terms.source_uid)
    .bind(&input.terms.reserve_default)
    .bind(input.terms.require_confirmation as i64)
    .bind(location.and_then(|value| value.lat))
    .bind(location.and_then(|value| value.lon))
    .bind(location.and_then(|value| value.address.as_deref()))
    .bind(revision)
    .bind(&input.transfer_uid)
    .bind(expected_revision)
    .execute(&mut *tx)
    .await?;
    if advanced.rows_affected() == 0 {
        let current_revision: i64 =
            sqlx::query_scalar("SELECT revision FROM transfer WHERE record_uid = ?")
                .bind(&input.transfer_uid)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(sqlx::Error::RowNotFound)?;
        if current_revision == 0 && !allow_legacy {
            return Err(sqlx::Error::Protocol(
                "legacy revision-0 Transfer requires explicit adoption".into(),
            ));
        }
        tx.rollback().await?;
        return Ok(RevisionCommit::Stale {
            current_revision: current_revision as u64,
        });
    }
    sqlx::query("UPDATE record SET slug = ?, head = ?, updated_at = ? WHERE uid = ?")
        .bind(&input.terms.slug)
        .bind(&input.terms.head)
        .bind(&at)
        .bind(&input.transfer_uid)
        .execute(&mut *tx)
        .await?;
    let submitting_party_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM transfer_party
         WHERE transfer_uid = ? AND actor_uid = ?)",
    )
    .bind(&input.transfer_uid)
    .bind(&input.creator_person_uid)
    .fetch_one(&mut *tx)
    .await?;
    if !submitting_party_exists {
        return Err(sqlx::Error::Protocol(
            "Transfer revision submitter must already be a party".into(),
        ));
    }
    if mark_submitting_party_as_creator {
        sqlx::query(
            "UPDATE transfer_party SET kind = 'participant'
             WHERE transfer_uid = ? AND kind = 'creator' AND actor_uid != ?",
        )
        .bind(&input.transfer_uid)
        .bind(&input.creator_person_uid)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE transfer_party SET kind = 'creator'
             WHERE transfer_uid = ? AND actor_uid = ?",
        )
        .bind(&input.transfer_uid)
        .bind(&input.creator_person_uid)
        .execute(&mut *tx)
        .await?;
    }

    let pending_invitation_uids: HashSet<String> = sqlx::query_scalar(
        "SELECT uid FROM transfer_invitation
         WHERE transfer_uid = ? AND status = 'pending'",
    )
    .bind(&input.transfer_uid)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();
    let retained_invitation_uids: HashSet<&str> = input
        .retained_invitation_uids
        .iter()
        .map(String::as_str)
        .collect();
    if retained_invitation_uids
        .iter()
        .any(|uid| !pending_invitation_uids.contains(*uid))
    {
        return Err(sqlx::Error::Protocol(
            "draft contains an invitation that is not pending on this Transfer".into(),
        ));
    }
    for uid in pending_invitation_uids {
        if !retained_invitation_uids.contains(uid.as_str()) {
            sqlx::query(
                "UPDATE transfer_invitation
                 SET status = 'withdrawn', updated_at = ?
                 WHERE uid = ? AND status = 'pending'",
            )
            .bind(&at)
            .bind(uid)
            .execute(&mut *tx)
            .await?;
        }
    }

    let editable_promise_uids: HashSet<String> = sqlx::query_scalar(
        "SELECT uid FROM promise
         WHERE transfer_uid = ? AND state IN ('open', 'proposed', 'agreed')",
    )
    .bind(&input.transfer_uid)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();
    let retained_promise_uids: HashSet<&str> = input
        .promises
        .iter()
        .filter_map(|promise| promise.uid.as_deref())
        .collect();
    for uid in retained_promise_uids
        .iter()
        .copied()
        .filter(|uid| !editable_promise_uids.contains(*uid))
    {
        let existing: Option<(String, String)> =
            sqlx::query_as("SELECT transfer_uid, state FROM promise WHERE uid = ?")
                .bind(uid)
                .fetch_optional(&mut *tx)
                .await?;
        if let Some((owner, state)) = existing {
            return Err(sqlx::Error::Protocol(format!(
                "promise `{uid}` already belongs to Transfer `{owner}` in state `{state}`"
            )));
        }
    }
    for uid in &editable_promise_uids {
        if !retained_promise_uids.contains(uid.as_str()) {
            sqlx::query(
                "UPDATE promise SET state = 'withdrawn', revision = ?, updated_at = ?
                 WHERE uid = ? AND transfer_uid = ?",
            )
            .bind(revision)
            .bind(&at)
            .bind(uid)
            .bind(&input.transfer_uid)
            .execute(&mut *tx)
            .await?;
        }
    }

    for promise in &input.promises {
        let owner = promise.person_uid.as_deref().ok_or_else(|| {
            sqlx::Error::Protocol("every Transfer promise requires an owning Person".into())
        })?;
        let existing_open_owner: Option<String> = if let Some(uid) = promise.uid.as_deref() {
            sqlx::query_scalar(
                "SELECT party_uid FROM promise
                 WHERE uid = ? AND transfer_uid = ? AND state = 'open'",
            )
            .bind(uid)
            .bind(&input.transfer_uid)
            .fetch_optional(&mut *tx)
            .await?
            .flatten()
        } else {
            None
        };
        if let Some(existing_owner) = existing_open_owner.as_deref() {
            if !promise.open || existing_owner != owner {
                return Err(sqlx::Error::Protocol(
                    "retained OPEN promise must keep its original proposer and OPEN state".into(),
                ));
            }
        } else if promise.open && owner != input.proposal_author_person_uid {
            return Err(sqlx::Error::Protocol(
                "new OPEN promise must belong to the Person signing this revision".into(),
            ));
        }
        let state = if promise.open {
            PromiseState::Open
        } else {
            PromiseState::Proposed
        };
        let location = promise.location.as_ref();
        if let Some(uid) = promise
            .uid
            .as_deref()
            .filter(|uid| editable_promise_uids.contains(*uid))
        {
            sqlx::query(
                "UPDATE promise SET record_uid = ?, concept_uid = ?, unit_uid = ?,
                    party_uid = ?, delta = ?, window_start = ?, window_end = ?,
                    location_lat = ?, location_lon = ?, location_address = ?,
                    condition = ?, reserve_from = ?, open_reuse_policy = ?, state = ?,
                    revision = ?, updated_at = ?
                 WHERE uid = ? AND transfer_uid = ?",
            )
            .bind(&promise.record_uid)
            .bind(&promise.concept_uid)
            .bind(&promise.unit_uid)
            .bind(&promise.person_uid)
            .bind(promise.delta)
            .bind(&promise.window_start)
            .bind(&promise.window_end)
            .bind(location.and_then(|value| value.lat))
            .bind(location.and_then(|value| value.lon))
            .bind(location.and_then(|value| value.address.as_deref()))
            .bind(&promise.condition)
            .bind(&promise.reserve_from)
            .bind(promise.open_reuse_policy.as_str())
            .bind(state.as_str())
            .bind(revision)
            .bind(&at)
            .bind(uid)
            .bind(&input.transfer_uid)
            .execute(&mut *tx)
            .await?;
        } else {
            let uid = promise.uid.clone().unwrap_or_else(|| nucleus::new_uid("p"));
            sqlx::query(
                "INSERT INTO promise
                    (uid, source_promise_uid, record_uid, concept_uid, unit_uid, party_uid, delta,
                     window_start, window_end, location_lat, location_lon,
                     location_address, condition, transfer_uid, reserve_from,
                     open_reuse_policy, state, revision, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(uid)
            .bind(&promise.source_promise_uid)
            .bind(&promise.record_uid)
            .bind(&promise.concept_uid)
            .bind(&promise.unit_uid)
            .bind(&promise.person_uid)
            .bind(promise.delta)
            .bind(&promise.window_start)
            .bind(&promise.window_end)
            .bind(location.and_then(|value| value.lat))
            .bind(location.and_then(|value| value.lon))
            .bind(location.and_then(|value| value.address.as_deref()))
            .bind(&promise.condition)
            .bind(&input.transfer_uid)
            .bind(&promise.reserve_from)
            .bind(promise.open_reuse_policy.as_str())
            .bind(state.as_str())
            .bind(revision)
            .bind(&at)
            .bind(&at)
            .execute(&mut *tx)
            .await?;
        }
    }

    replace_dependencies(&mut tx, &input.transfer_uid, revision, &input.dependencies).await?;

    sqlx::query(
        "UPDATE transfer_agreement SET level = 0, revision = ?, at = ?
         WHERE transfer_uid = ?",
    )
    .bind(revision)
    .bind(&at)
    .bind(&input.transfer_uid)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "DELETE FROM visibility_rule
         WHERE target_uid = ? AND subject_kind = 'public'",
    )
    .bind(&input.transfer_uid)
    .execute(&mut *tx)
    .await?;
    if input.terms.visibility == "public" {
        sqlx::query(
            "INSERT INTO visibility_rule
                (uid, subject_kind, subject_uid, target_uid, grant_level)
             VALUES (?, 'public', NULL, ?, 'visible')",
        )
        .bind(nucleus::new_uid("v"))
        .bind(&input.transfer_uid)
        .execute(&mut *tx)
        .await?;
    }

    let snapshot = revision_snapshot(&mut tx, &input.transfer_uid).await?;
    let evidence = TransferRevisionEvidence {
        action: action.into(),
        idempotency_key: Some(request_key.into()),
        previous_revision: Some(input.expected_revision),
        terms: snapshot,
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
            actor_uid: fact_actor,
            cause: Cause::user_edit(),
            payload: Some(payload),
        },
        &previous_hash,
        now,
    );
    fact.signature = sign(&fact.hash);
    if fact.signature.is_none() && input.authorization_intent_uid.is_none() {
        return Err(sqlx::Error::Protocol(
            "Transfer revision requires a signing key or verified Action intent".into(),
        ));
    }
    crate::facts::insert(&mut tx, &fact).await?;
    if let Some(intent_uid) = input.authorization_intent_uid.as_deref() {
        crate::action_intents::link_pending_fact(&mut tx, intent_uid, &fact).await?;
    }
    sqlx::query(
        "INSERT INTO transfer_revision
            (transfer_uid, revision, fact_uid, idempotency_key, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&input.transfer_uid)
    .bind(revision)
    .bind(&fact.uid)
    .bind(request_key)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    if let Some(successor) = &input.successor {
        sqlx::query(
            "INSERT INTO transfer_promise_successor
                (uid, transfer_uid, predecessor_promise_uid, successor_promise_uid,
                 revision, actor_person_uid, fact_uid, idempotency_key, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(nucleus::new_uid("tps"))
        .bind(&input.transfer_uid)
        .bind(&successor.predecessor_promise_uid)
        .bind(&successor.successor_promise_uid)
        .bind(revision)
        .bind(&input.proposal_author_person_uid)
        .bind(&fact.uid)
        .bind(request_key)
        .bind(&at)
        .execute(&mut *tx)
        .await?;
    }
    crate::records::bump_quantity(&mut tx, &input.transfer_uid, crate::exact::zero(), &at).await?;
    tx.commit().await?;

    Ok(RevisionCommit::Committed {
        revision: revision as u64,
        fact,
    })
}

fn validate_revision_request(input: &WholeDraftRevisionInput) -> Result<(), StoreError> {
    let request_key = input.idempotency_key.trim();
    if request_key.is_empty() || request_key.chars().count() > 200 {
        return Err(sqlx::Error::Protocol(
            "transfer revision idempotency key must contain 1 to 200 characters".into(),
        ));
    }
    if let Some(slug) = input.terms.slug.as_deref()
        && !nucleus::valid_slug(slug)
    {
        return Err(sqlx::Error::Protocol(format!("invalid slug `{slug}`")));
    }
    validate_location(input.terms.default_place.as_ref())?;
    let invitation_count = input
        .retained_invitation_uids
        .iter()
        .collect::<HashSet<_>>()
        .len();
    if invitation_count != input.retained_invitation_uids.len() {
        return Err(sqlx::Error::Protocol(
            "draft repeats an invitation uid".into(),
        ));
    }
    let promise_uid_count = input
        .promises
        .iter()
        .filter_map(|promise| promise.uid.as_ref())
        .collect::<HashSet<_>>()
        .len();
    if promise_uid_count != input.promises.iter().filter(|p| p.uid.is_some()).count() {
        return Err(sqlx::Error::Protocol("draft repeats a promise uid".into()));
    }
    for promise in &input.promises {
        if promise.record_uid.is_none() && promise.concept_uid.is_none() {
            return Err(sqlx::Error::Protocol(
                "draft promise requires a Record or concept".into(),
            ));
        }
        if !promise.delta.is_finite() || promise.delta == 0.0 {
            return Err(sqlx::Error::Protocol(
                "draft promise delta must be finite and non-zero".into(),
            ));
        }
        validate_location(promise.location.as_ref())?;
    }
    validate_dependency_inputs(&input.dependencies)?;
    Ok(())
}

fn validate_dependency_inputs(dependencies: &[TransferDependencyInput]) -> Result<(), StoreError> {
    let supplied_uids: HashSet<&str> = dependencies
        .iter()
        .filter_map(|dependency| dependency.uid.as_deref())
        .collect();
    if supplied_uids.len()
        != dependencies
            .iter()
            .filter(|value| value.uid.is_some())
            .count()
    {
        return Err(sqlx::Error::Protocol(
            "transfer draft repeats a dependency uid".into(),
        ));
    }
    let mut terms = HashSet::new();
    for dependency in dependencies {
        let upstream_uid = dependency.upstream_uid.trim();
        if upstream_uid.is_empty() {
            return Err(sqlx::Error::Protocol(
                "transfer dependency requires an upstream uid".into(),
            ));
        }
        if dependency.scope == TransferDependencyScope::Promise
            && dependency
                .promise_uid
                .as_deref()
                .map_or(true, str::is_empty)
        {
            return Err(sqlx::Error::Protocol(
                "promise-scoped dependency requires a promise uid".into(),
            ));
        }
        if dependency.scope == TransferDependencyScope::Transfer && dependency.promise_uid.is_some()
        {
            return Err(sqlx::Error::Protocol(
                "Transfer-scoped dependency cannot name a scoped promise".into(),
            ));
        }
        let required_state = normalized_dependency_state(&dependency.required_state)?;
        if !terms.insert((
            dependency.scope,
            dependency.promise_uid.as_deref(),
            dependency.upstream_kind,
            upstream_uid,
            required_state,
        )) {
            return Err(sqlx::Error::Protocol(
                "transfer draft repeats identical dependency terms".into(),
            ));
        }
    }
    Ok(())
}

fn normalized_dependency_state(value: &str) -> Result<&str, StoreError> {
    let value = value.trim();
    let value = if value.is_empty() { "kept" } else { value };
    PromiseState::parse(value)
        .map(|state| state.as_str())
        .ok_or_else(|| sqlx::Error::Protocol(format!("unknown dependency state `{value}`")))
}

async fn replace_dependencies(
    tx: &mut Transaction<'_, Sqlite>,
    transfer_uid: &str,
    revision: i64,
    dependencies: &[TransferDependencyInput],
) -> Result<(), StoreError> {
    validate_dependency_inputs(dependencies)?;
    for dependency in dependencies {
        if let Some(uid) = dependency.uid.as_deref() {
            let owner: Option<String> =
                sqlx::query_scalar("SELECT transfer_uid FROM transfer_dependency WHERE uid = ?")
                    .bind(uid)
                    .fetch_optional(&mut **tx)
                    .await?;
            if owner.as_deref().is_some_and(|owner| owner != transfer_uid) {
                return Err(sqlx::Error::Protocol(
                    "dependency uid belongs to another Transfer".into(),
                ));
            }
        }
        if let Some(promise_uid) = dependency.promise_uid.as_deref() {
            let scoped_here: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM promise
                 WHERE uid = ? AND transfer_uid = ? AND state != 'withdrawn')",
            )
            .bind(promise_uid)
            .bind(transfer_uid)
            .fetch_one(&mut **tx)
            .await?;
            if !scoped_here {
                return Err(sqlx::Error::Protocol(format!(
                    "dependency scoped promise `{promise_uid}` is not current on this Transfer"
                )));
            }
        }
        let upstream_uid = dependency.upstream_uid.trim();
        let upstream_exists: bool = match dependency.upstream_kind {
            TransferDependencyUpstreamKind::Transfer => {
                if upstream_uid == transfer_uid {
                    return Err(sqlx::Error::Protocol(
                        "Transfer cannot depend directly on itself".into(),
                    ));
                }
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM transfer WHERE record_uid = ?)")
                    .bind(upstream_uid)
                    .fetch_one(&mut **tx)
                    .await?
            }
            TransferDependencyUpstreamKind::Promise => {
                if dependency.promise_uid.as_deref() == Some(upstream_uid) {
                    return Err(sqlx::Error::Protocol(
                        "promise cannot depend directly on itself".into(),
                    ));
                }
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM promise WHERE uid = ?)")
                    .bind(upstream_uid)
                    .fetch_one(&mut **tx)
                    .await?
            }
        };
        if !upstream_exists {
            return Err(sqlx::Error::Protocol(format!(
                "unknown {} dependency upstream `{upstream_uid}`",
                dependency.upstream_kind.as_str()
            )));
        }
    }

    validate_dependency_dag(tx, transfer_uid, dependencies).await?;

    sqlx::query("DELETE FROM transfer_dependency WHERE transfer_uid = ?")
        .bind(transfer_uid)
        .execute(&mut **tx)
        .await?;
    for dependency in dependencies {
        sqlx::query(
            "INSERT INTO transfer_dependency
                (uid, transfer_uid, revision, scope, promise_uid, upstream_kind,
                 upstream_uid, required_state)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(
            dependency
                .uid
                .clone()
                .unwrap_or_else(|| nucleus::new_uid("td")),
        )
        .bind(transfer_uid)
        .bind(revision)
        .bind(dependency.scope.as_str())
        .bind(&dependency.promise_uid)
        .bind(dependency.upstream_kind.as_str())
        .bind(dependency.upstream_uid.trim())
        .bind(normalized_dependency_state(&dependency.required_state)?)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn validate_transfer_parent(
    tx: &mut Transaction<'_, Sqlite>,
    transfer_uid: &str,
    parent_uid: Option<&str>,
) -> Result<(), StoreError> {
    let Some(mut cursor) = parent_uid
        .map(str::trim)
        .filter(|uid| !uid.is_empty())
        .map(str::to_owned)
    else {
        return Ok(());
    };
    let mut seen = HashSet::from([transfer_uid.to_owned()]);
    loop {
        if !seen.insert(cursor.clone()) {
            return Err(sqlx::Error::Protocol(format!(
                "Transfer parent cycle reaches `{cursor}`"
            )));
        }
        let row: Option<Option<String>> =
            sqlx::query_scalar("SELECT parent_uid FROM transfer WHERE record_uid = ?")
                .bind(&cursor)
                .fetch_optional(&mut **tx)
                .await?;
        let Some(parent) = row else {
            return Err(sqlx::Error::Protocol(format!(
                "unknown Transfer parent `{cursor}`"
            )));
        };
        let Some(next) = parent else {
            return Ok(());
        };
        cursor = next;
    }
}

async fn validate_dependency_dag(
    tx: &mut Transaction<'_, Sqlite>,
    transfer_uid: &str,
    replacement: &[TransferDependencyInput],
) -> Result<(), StoreError> {
    let rows = sqlx::query(
        "SELECT transfer_uid, scope, promise_uid, upstream_kind, upstream_uid
         FROM transfer_dependency WHERE transfer_uid != ?",
    )
    .bind(transfer_uid)
    .fetch_all(&mut **tx)
    .await?;
    let mut edges = Vec::with_capacity(rows.len() + replacement.len());
    for row in rows {
        let scope: String = row.get("scope");
        let upstream_kind: String = row.get("upstream_kind");
        let downstream = if scope == "transfer" {
            TransferDependencyNode::Transfer(row.get("transfer_uid"))
        } else {
            TransferDependencyNode::Promise(row.get("promise_uid"))
        };
        let upstream_uid: String = row.get("upstream_uid");
        let upstream = if upstream_kind == "transfer" {
            TransferDependencyNode::Transfer(upstream_uid)
        } else {
            TransferDependencyNode::Promise(upstream_uid)
        };
        edges.push(TransferDependencyEdge {
            upstream,
            downstream,
        });
    }
    for dependency in replacement {
        let downstream = match dependency.scope {
            TransferDependencyScope::Transfer => {
                TransferDependencyNode::Transfer(transfer_uid.to_owned())
            }
            TransferDependencyScope::Promise => TransferDependencyNode::Promise(
                dependency
                    .promise_uid
                    .clone()
                    .expect("validated promise-scoped dependency"),
            ),
        };
        let upstream = match dependency.upstream_kind {
            TransferDependencyUpstreamKind::Transfer => {
                TransferDependencyNode::Transfer(dependency.upstream_uid.trim().to_owned())
            }
            TransferDependencyUpstreamKind::Promise => {
                TransferDependencyNode::Promise(dependency.upstream_uid.trim().to_owned())
            }
        };
        edges.push(TransferDependencyEdge {
            upstream,
            downstream,
        });
    }
    if let Err(nodes) = nucleus::transfer::transfer_dependency_order(&edges) {
        let nodes = nodes
            .into_iter()
            .map(|node| match node {
                TransferDependencyNode::Transfer(uid) => format!("transfer:{uid}"),
                TransferDependencyNode::Promise(uid) => format!("promise:{uid}"),
            })
            .collect::<Vec<_>>()
            .join(", ");
        return Err(sqlx::Error::Protocol(format!(
            "Transfer dependency cycle contains {nodes}"
        )));
    }
    Ok(())
}

fn validate_location(location: Option<&TransferLocationSnapshot>) -> Result<(), StoreError> {
    let Some(location) = location else {
        return Ok(());
    };
    if location.lat.is_some() != location.lon.is_some() {
        return Err(sqlx::Error::Protocol(
            "location latitude and longitude must be provided together".into(),
        ));
    }
    if let (Some(lat), Some(lon)) = (location.lat, location.lon)
        && (!lat.is_finite()
            || !lon.is_finite()
            || !(-90.0..=90.0).contains(&lat)
            || !(-180.0..=180.0).contains(&lon))
    {
        return Err(sqlx::Error::Protocol(
            "location coordinates are outside valid latitude/longitude bounds".into(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferInvitationStatus {
    Pending,
    Accepted,
    Rejected,
    Withdrawn,
    Expired,
}

impl TransferInvitationStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Withdrawn => "withdrawn",
            Self::Expired => "expired",
        }
    }

    fn from_db(value: &str) -> Result<Self, StoreError> {
        match value {
            "pending" => Ok(Self::Pending),
            "accepted" => Ok(Self::Accepted),
            "rejected" => Ok(Self::Rejected),
            "withdrawn" => Ok(Self::Withdrawn),
            "expired" => Ok(Self::Expired),
            _ => Err(sqlx::Error::Protocol(format!(
                "unknown transfer invitation status `{value}`"
            ))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TransferInvitationRow {
    pub uid: String,
    pub transfer_uid: String,
    pub addressed_person_uid: String,
    pub invited_by_person_uid: String,
    pub status: TransferInvitationStatus,
    pub attempt: u64,
    pub party_uid: Option<String>,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub struct NewTransferInvitation<'a> {
    pub transfer_uid: &'a str,
    pub addressed_person_uid: &'a str,
    pub invited_by_person_uid: &'a str,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct AcceptedTransferInvitation {
    pub invitation: TransferInvitationRow,
    pub party_uid: String,
}

#[derive(Debug, Clone)]
pub struct AddressTransferInvitationInput {
    pub transfer_uid: String,
    pub expected_revision: u64,
    pub idempotency_key: String,
    pub addressed_person_uid: String,
    pub invited_by_person_uid: String,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct InvitationTransitionInput {
    pub invitation_uid: String,
    pub expected_revision: u64,
    pub idempotency_key: String,
    pub actor_person_uid: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct InvitationCommitOutcome {
    pub transfer_uid: String,
    pub invitation: TransferInvitationRow,
    pub party_uid: Option<String>,
    pub revision: Option<u64>,
    pub revision_fact: Option<Fact>,
    pub event_fact: Fact,
}

#[derive(Debug, Clone)]
pub enum InvitationCommit {
    Committed(InvitationCommitOutcome),
    Replayed(InvitationCommitOutcome),
    Stale {
        transfer_uid: String,
        current_revision: u64,
    },
}

#[derive(Debug, Clone)]
pub struct OpenPromiseClaimInput {
    pub transfer_uid: String,
    pub source_promise_uid: String,
    pub expected_revision: u64,
    pub idempotency_key: String,
    pub claimant_person_uid: String,
    pub record_uid: Option<String>,
    pub concept_uid: Option<String>,
    pub unit_uid: Option<String>,
    pub delta: f64,
    pub window_start: Option<String>,
    pub window_end: Option<String>,
    pub location: Option<TransferLocationSnapshot>,
    pub condition: Option<String>,
    pub reserve_from: String,
    pub authorization_intent_uid: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OpenPromiseClaimOutcome {
    pub transfer_uid: String,
    pub revision: u64,
    pub fact: Fact,
    pub party_uid: String,
    pub proposer_promise_uid: String,
    pub promise_uid: String,
    pub source_promise_uid: String,
    pub consumed: bool,
}

#[derive(Debug, Clone)]
pub struct OpenPromiseClaimPairRow {
    pub uid: String,
    pub transfer_uid: String,
    pub source_promise_uid: String,
    pub source_record_uid: String,
    pub proposer_promise_uid: String,
    pub claimant_promise_uid: String,
    pub proposer_person_uid: String,
    pub claimant_person_uid: String,
    pub revision: u64,
    pub reuse_policy: OpenPromiseReusePolicy,
    pub idempotency_key: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub enum OpenPromiseClaimCommit {
    Committed(OpenPromiseClaimOutcome),
    Replayed(OpenPromiseClaimOutcome),
    Stale {
        transfer_uid: String,
        current_revision: u64,
    },
}

#[derive(Debug, Clone)]
pub struct AgreementTransitionInput {
    pub transfer_uid: String,
    pub expected_revision: u64,
    pub idempotency_key: String,
    pub person_uid: String,
    pub to_level: u8,
    pub authorization_intent_uid: Option<String>,
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct AgreementCoalitionRow {
    pub transfer_uid: String,
    pub revision: u64,
    pub threshold_pct: u8,
    pub eligible_count: usize,
    pub frozen_by_event_uid: String,
    pub frozen_at: String,
    pub party_uids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AgreementTransitionOutcome {
    pub event: AgreementTransitionEventRow,
    pub fact: Fact,
    pub coalition: Option<AgreementCoalitionRow>,
}

#[derive(Debug, Clone)]
pub enum AgreementTransitionCommit {
    Committed(AgreementTransitionOutcome),
    Replayed(AgreementTransitionOutcome),
    Stale {
        transfer_uid: String,
        current_revision: u64,
    },
}

#[derive(Debug, Clone)]
pub struct AgreementPartyLevelRow {
    pub party_uid: String,
    pub person_uid: String,
    pub level: u8,
    pub revision: u64,
}

#[derive(Debug, Clone)]
pub struct AgreementPromiseReadinessRow {
    pub uid: String,
    pub record_uid: Option<String>,
    pub concept_uid: Option<String>,
    pub unit_uid: Option<String>,
    pub person_uid: Option<String>,
    pub delta: f64,
    pub window_start: Option<String>,
    pub window_end: Option<String>,
    pub location: Option<TransferLocationSnapshot>,
    pub state: String,
    pub revision: u64,
}

#[derive(Debug, Clone)]
pub struct TransferAgreementReadinessInput {
    pub transfer_uid: String,
    pub revision: u64,
    pub agreement_type: String,
    pub agreement_pct: Option<u8>,
    pub parties: Vec<AgreementPartyLevelRow>,
    pub promises: Vec<AgreementPromiseReadinessRow>,
    pub dependencies: Vec<TransferRevisionDependency>,
    pub coalition: Option<AgreementCoalitionRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OccurrenceActivationInput {
    pub promise_uid: String,
    pub opposite_promise_uid: Option<String>,
    pub giver_person_uid: String,
    pub receiver_person_uid: String,
}

#[derive(Debug, Clone)]
pub struct ActivateOccurrencesInput {
    pub transfer_uid: String,
    pub expected_revision: u64,
    pub idempotency_key: String,
    pub actor_person_uid: String,
    pub occurrences: Vec<OccurrenceActivationInput>,
    pub authorization_intent_uid: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TransferOccurrenceRow {
    pub uid: String,
    pub transfer_uid: String,
    pub revision: u64,
    pub promise_uid: String,
    pub exchange_path_uid: String,
    pub opposite_promise_uid: Option<String>,
    pub activation_event_uid: String,
    pub activation_fact_uid: String,
    pub record_uid: Option<String>,
    pub concept_uid: Option<String>,
    pub unit_uid: Option<String>,
    pub quantity: f64,
    pub giver_person_uid: String,
    pub receiver_person_uid: String,
    pub window_start: Option<String>,
    pub window_end: Option<String>,
    pub location: Option<TransferLocationSnapshot>,
    pub delivery_claimed: bool,
    pub receipt_claimed: bool,
    pub disputed: bool,
    pub system_disputed: bool,
    pub system_dispute_fact_uid: Option<String>,
    pub system_disputed_at: Option<String>,
    pub dispute_fact_uid: Option<String>,
    pub disputed_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct OccurrenceActivationOutcome {
    pub transfer_uid: String,
    pub revision: u64,
    pub actor_person_uid: String,
    pub event_uid: String,
    pub fact: Fact,
    pub occurrences: Vec<TransferOccurrenceRow>,
}

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

#[derive(Debug, Clone)]
pub struct OccurrenceClaimInput {
    pub occurrence_uid: String,
    pub idempotency_key: String,
    pub actor_person_uid: String,
    pub role: OccurrenceClaimRole,
    pub asserted: bool,
    pub authorization_intent_uid: Option<String>,
}

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

#[derive(Debug, Clone)]
pub struct OccurrenceClaimOutcome {
    pub transfer_uid: String,
    pub event: OccurrenceClaimEventRow,
    pub fact: Fact,
    pub occurrence: TransferOccurrenceRow,
}

#[derive(Debug, Clone)]
pub enum OccurrenceClaimCommit {
    Committed(OccurrenceClaimOutcome),
    Replayed(OccurrenceClaimOutcome),
}

#[derive(Debug, Clone)]
pub struct ReviewedBulkOccurrenceClaim {
    pub occurrence_uid: String,
    pub transfer_uid: String,
    pub expected_revision: u64,
    pub role: OccurrenceClaimRole,
    pub expected_delivery_claimed: bool,
    pub expected_receipt_claimed: bool,
}

#[derive(Debug, Clone)]
pub struct BulkOccurrenceClaimInput {
    pub idempotency_key: String,
    pub actor_person_uid: String,
    pub review_token: String,
    pub items: Vec<ReviewedBulkOccurrenceClaim>,
    pub authorization_intent_uid: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BulkOccurrenceClaimFailure {
    pub occurrence_uid: String,
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct BulkOccurrenceClaimOutcome {
    pub uid: String,
    pub facts: Vec<Fact>,
}

#[derive(Debug, Clone)]
pub enum BulkOccurrenceClaimCommit {
    Committed(BulkOccurrenceClaimOutcome),
    Replayed(BulkOccurrenceClaimOutcome),
    Rejected(Vec<BulkOccurrenceClaimFailure>),
}

#[derive(Debug, Clone)]
pub struct OccurrenceApplicationFormulaInput {
    pub occurrence_uid: String,
    pub idempotency_key: String,
    pub actor_person_uid: String,
    pub formula: String,
    pub authorization_intent_uid: Option<String>,
}

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

#[derive(Debug, Clone)]
pub struct OccurrenceApplicationPolicyRow {
    pub occurrence_uid: String,
    pub receiver_person_uid: String,
    pub formula: String,
    pub formula_hash: String,
    pub version: u64,
    pub latest_event_uid: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct OccurrenceApplicationFormulaOutcome {
    pub transfer_uid: String,
    pub event: OccurrenceApplicationEventRow,
    pub fact: Fact,
    pub policy: OccurrenceApplicationPolicyRow,
}

#[derive(Debug, Clone)]
pub enum OccurrenceApplicationFormulaCommit {
    Committed(OccurrenceApplicationFormulaOutcome),
    Replayed(OccurrenceApplicationFormulaOutcome),
}

#[derive(Debug, Clone)]
pub struct OccurrenceSettlementInput {
    pub occurrence_uid: String,
    pub idempotency_key: String,
    pub actor_person_uid: String,
    pub canonical_quantity: f64,
    pub local_record_uid: String,
    pub local_delta: f64,
    pub local_cumulative_after: f64,
    pub application_formula: String,
    pub application_formula_version: u64,
    pub remainder_policy: TransferRemainderPolicy,
    pub authorization_intent_uid: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OccurrenceSettlementSliceRow {
    pub uid: String,
    pub occurrence_uid: String,
    pub transfer_uid: String,
    pub promise_uid: String,
    pub owner_person_uid: String,
    pub canonical_quantity: f64,
    pub canonical_unit_uid: Option<String>,
    pub cumulative_before: f64,
    pub cumulative_after: f64,
    pub remaining_after: f64,
    pub evidence_fact_uid: String,
    pub application_fact_uid: String,
    pub local_record_uid: String,
    pub local_delta: f64,
    pub local_cumulative_before: f64,
    pub local_cumulative_after: f64,
    pub application_formula: String,
    pub application_formula_hash: String,
    pub application_formula_version: u64,
    pub remainder_policy: TransferRemainderPolicy,
    pub idempotency_key: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct OccurrenceSettlementProgress {
    pub occurrence_uid: String,
    pub canonical_quantity: f64,
    pub settled_quantity: f64,
    pub remaining_quantity: f64,
    pub partially_settled: bool,
    pub settled: bool,
    pub slices: Vec<OccurrenceSettlementSliceRow>,
}

#[derive(Debug, Clone)]
pub struct OccurrenceSettlementOutcome {
    pub transfer_uid: String,
    pub slice: OccurrenceSettlementSliceRow,
    pub evidence_fact: Fact,
    pub application_fact: Fact,
    pub progress: OccurrenceSettlementProgress,
    pub satiation_facts: Vec<Fact>,
}

#[derive(Debug, Clone)]
pub enum OccurrenceSettlementCommit {
    Committed(OccurrenceSettlementOutcome),
    Replayed(OccurrenceSettlementOutcome),
}

#[derive(Debug, Clone)]
pub struct TransferSourceGroupResultRow {
    pub uid: String,
    pub source_uid: String,
    pub policy: String,
    pub winner_transfer_uid: String,
    pub winner_revision: u64,
    pub settlement_uid: String,
    pub fact_uid: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct TransferSourceGroupLoserRow {
    pub uid: String,
    pub result_uid: String,
    pub transfer_uid: String,
    pub transfer_revision: u64,
    pub fact_uid: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct TransferSourceGroupState {
    pub result: TransferSourceGroupResultRow,
    pub loser: Option<TransferSourceGroupLoserRow>,
    pub satiated: bool,
}

#[derive(Debug, Clone)]
pub struct OccurrenceSettlementCompensationInput {
    pub settlement_uid: String,
    pub idempotency_key: String,
    pub actor_person_uid: String,
    pub authorization_intent_uid: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OccurrenceSettlementCompensationRow {
    pub uid: String,
    pub settlement_uid: String,
    pub occurrence_uid: String,
    pub transfer_uid: String,
    pub owner_person_uid: String,
    pub original_application_fact_uid: String,
    pub compensation_fact_uid: String,
    pub local_record_uid: String,
    pub inverse_delta: f64,
    pub idempotency_key: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct OccurrenceSettlementCompensationOutcome {
    pub correction: OccurrenceSettlementCompensationRow,
    pub fact: Fact,
}

#[derive(Debug, Clone)]
pub enum OccurrenceSettlementCompensationCommit {
    Committed(OccurrenceSettlementCompensationOutcome),
    Replayed(OccurrenceSettlementCompensationOutcome),
}

#[derive(Debug, Clone)]
pub struct OccurrenceDisputeInput {
    pub occurrence_uid: String,
    pub idempotency_key: String,
    pub actor_person_uid: String,
    pub disputed: bool,
    pub authorization_intent_uid: Option<String>,
}

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

#[derive(Debug, Clone)]
pub struct OccurrenceDisputeOutcome {
    pub event: OccurrenceDisputeEventRow,
    pub fact: Fact,
    pub current_disputed: bool,
}

#[derive(Debug, Clone)]
pub enum OccurrenceDisputeCommit {
    Committed(OccurrenceDisputeOutcome),
    Replayed(OccurrenceDisputeOutcome),
}

fn fact_has_transfer_revision_action(fact: &Fact, expected_action: &str) -> bool {
    fact.payload
        .as_deref()
        .and_then(|payload| serde_json::from_str::<TransferRevisionEvidence>(payload).ok())
        .is_some_and(|evidence| evidence.action == expected_action)
}

pub async fn create_draft<F>(
    pool: &SqlitePool,
    draft: NewTransferDraft,
    now: DateTime<Utc>,
    fact_actor: Option<String>,
    visibility_actor: Option<String>,
    sign: F,
) -> Result<CreatedTransferDraft, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_key = draft.idempotency_key.trim();
    if request_key.is_empty() || request_key.chars().count() > 200 {
        return Err(sqlx::Error::Protocol(
            "transfer draft request key must contain 1 to 200 characters".into(),
        ));
    }
    ensure_request_not_used_by_invitation_event(pool, request_key).await?;
    let now_string = now.to_rfc3339();
    let mut tx = crate::write_tx(pool).await?;
    if let Some(row) = sqlx::query(
        "SELECT transfer_uid, revision, fact_uid FROM transfer_revision
         WHERE idempotency_key = ?",
    )
    .bind(request_key)
    .fetch_optional(&mut *tx)
    .await?
    {
        let transfer_uid: String = row.get("transfer_uid");
        let revision = row.get::<i64, _>("revision") as u64;
        let fact_uid: String = row.get("fact_uid");
        let party_uids = sqlx::query_scalar(
            "SELECT uid FROM transfer_party WHERE transfer_uid = ? ORDER BY uid",
        )
        .bind(&transfer_uid)
        .fetch_all(&mut *tx)
        .await?;
        let invitation_uids = sqlx::query_scalar(
            "SELECT uid FROM transfer_invitation WHERE transfer_uid = ? ORDER BY uid",
        )
        .bind(&transfer_uid)
        .fetch_all(&mut *tx)
        .await?;
        let promise_uids =
            sqlx::query_scalar("SELECT uid FROM promise WHERE transfer_uid = ? ORDER BY uid")
                .bind(&transfer_uid)
                .fetch_all(&mut *tx)
                .await?;
        tx.rollback().await?;
        let correction = correction_link_for_request(pool, request_key).await?;
        let successor = promise_successor_for_request(pool, request_key).await?;
        let open_claim = open_claim_pair_for_request(pool, request_key).await?;
        match (&draft.correction, correction.as_ref()) {
            (Some(requested), Some(existing))
                if successor.is_none()
                    && open_claim.is_none()
                    && existing.kind == requested.kind
                    && existing.source_transfer_uid == requested.source_transfer_uid
                    && existing.source_occurrence_uid == requested.source_occurrence_uid
                    && existing.source_revision == requested.source_revision
                    && (existing.canonical_quantity - requested.canonical_quantity).abs()
                        < 1e-9
                    && existing.created_transfer_uid == transfer_uid
                    && existing.actor_person_uid == draft.creator_person => {}
            (None, None) if successor.is_none() && open_claim.is_none() => {}
            _ => {
                return Err(sqlx::Error::Protocol(
                    "transfer request id was replayed as a different action".into(),
                ));
            }
        }
        let fact = crate::facts::get(pool, &fact_uid)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
        if !fact_has_transfer_revision_action(&fact, &draft.evidence_action) {
            return Err(sqlx::Error::Protocol(
                "transfer request id was replayed as a different revision action".into(),
            ));
        }
        let event_fact_uids: Vec<String> = sqlx::query_scalar(
            "SELECT fact_uid FROM transfer_invitation_event
             WHERE transfer_uid = ? AND kind = 'addressed' AND revision = ?
             ORDER BY created_at, uid",
        )
        .bind(&transfer_uid)
        .bind(revision as i64)
        .fetch_all(pool)
        .await?;
        let mut invitation_event_facts = Vec::with_capacity(event_fact_uids.len());
        for uid in event_fact_uids {
            invitation_event_facts.push(
                crate::facts::get(pool, &uid)
                    .await?
                    .ok_or(sqlx::Error::RowNotFound)?,
            );
        }
        return Ok(CreatedTransferDraft {
            transfer_uid,
            revision,
            replayed: true,
            party_uids,
            invitation_uids,
            promise_uids,
            fact,
            invitation_event_facts,
        });
    }
    if let Some(slug) = draft.slug.as_deref()
        && !nucleus::valid_slug(slug)
    {
        return Err(sqlx::Error::Protocol(format!("invalid slug `{slug}`")));
    }
    validate_location(draft.default_place.as_ref())?;
    let supplied_promise_uids = draft
        .promises
        .iter()
        .filter_map(|promise| promise.uid.as_ref())
        .collect::<HashSet<_>>();
    if supplied_promise_uids.len() != draft.promises.iter().filter(|p| p.uid.is_some()).count() {
        return Err(sqlx::Error::Protocol(
            "transfer draft repeats a promise uid".into(),
        ));
    }
    for promise in &draft.promises {
        if promise.record_uid.is_none() && promise.concept_uid.is_none() {
            return Err(sqlx::Error::Protocol(
                "transfer draft promise requires a Record or concept".into(),
            ));
        }
        if !promise.delta.is_finite() || promise.delta == 0.0 {
            return Err(sqlx::Error::Protocol(
                "transfer draft promise delta must be finite and non-zero".into(),
            ));
        }
        validate_location(promise.location.as_ref())?;
    }
    validate_dependency_inputs(&draft.dependencies)?;
    let transfer_uid = nucleus::new_uid("r");
    validate_transfer_parent(&mut tx, &transfer_uid, draft.parent_uid.as_deref()).await?;
    sqlx::query(
        "INSERT INTO record
            (uid, slug, kind, head, body, quantity_mantissa, quantity_scale, organ_uid, created_at, updated_at)
         VALUES (?, ?, ?, ?, '', '0', 0, ?, ?, ?)",
    )
    .bind(&transfer_uid)
    .bind(&draft.slug)
    .bind(RecordKind::Transfer.as_str())
    .bind(&draft.head)
    .bind(&draft.organ_uid)
    .bind(&now_string)
    .bind(&now_string)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO transfer
            (record_uid, agreement_type, agreement_pct, visibility, max_proximity,
             satiation, parent_uid, source_uid, reserve_default, require_confirmation,
             default_location_lat, default_location_lon, default_location_address, revision)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1)",
    )
    .bind(&transfer_uid)
    .bind(&draft.agreement_type)
    .bind(draft.agreement_pct)
    .bind(&draft.visibility)
    .bind(draft.max_proximity)
    .bind(&draft.satiation)
    .bind(&draft.parent_uid)
    .bind(&draft.source_uid)
    .bind(&draft.reserve_default)
    .bind(draft.require_confirmation as i64)
    .bind(draft.default_place.as_ref().and_then(|value| value.lat))
    .bind(draft.default_place.as_ref().and_then(|value| value.lon))
    .bind(
        draft
            .default_place
            .as_ref()
            .and_then(|value| value.address.as_deref()),
    )
    .execute(&mut *tx)
    .await?;

    let party_uid = nucleus::new_uid("y");
    sqlx::query(
        "INSERT INTO transfer_party (uid, transfer_uid, actor_uid, kind)
         VALUES (?, ?, ?, 'creator')",
    )
    .bind(&party_uid)
    .bind(&transfer_uid)
    .bind(&draft.creator_person)
    .execute(&mut *tx)
    .await?;
    let party_uids = vec![party_uid];

    let mut invitation_uids = Vec::with_capacity(draft.invitees.len());
    for addressed_person_uid in &draft.invitees {
        let invitation_uid = nucleus::new_uid("ti");
        sqlx::query(
            "INSERT INTO transfer_invitation
                (uid, transfer_uid, addressed_person_uid, invited_by_person_uid,
                 status, created_at, updated_at)
             VALUES (?, ?, ?, ?, 'pending', ?, ?)",
        )
        .bind(&invitation_uid)
        .bind(&transfer_uid)
        .bind(addressed_person_uid)
        .bind(&draft.creator_person)
        .bind(&now_string)
        .bind(&now_string)
        .execute(&mut *tx)
        .await?;
        invitation_uids.push(invitation_uid);
    }

    let mut promise_uids = Vec::with_capacity(draft.promises.len());
    for promise in &draft.promises {
        if promise.person_uid.is_none() {
            return Err(sqlx::Error::Protocol(
                "every Transfer promise requires an owning Person".into(),
            ));
        }
        if promise.open && promise.person_uid.as_deref() != Some(draft.creator_person.as_str()) {
            return Err(sqlx::Error::Protocol(
                "a newly created OPEN promise must belong to the Transfer creator".into(),
            ));
        }
        let promise_uid = promise.uid.clone().unwrap_or_else(|| nucleus::new_uid("p"));
        sqlx::query(
            "INSERT INTO promise
                (uid, record_uid, concept_uid, unit_uid, delta, window_start, window_end,
                 party_uid, state, condition, transfer_uid, reserve_from, revision,
                 location_lat, location_lon, location_address, open_reuse_policy,
                 created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&promise_uid)
        .bind(&promise.record_uid)
        .bind(&promise.concept_uid)
        .bind(&promise.unit_uid)
        .bind(promise.delta)
        .bind(&promise.window_start)
        .bind(&promise.window_end)
        .bind(&promise.person_uid)
        .bind(if promise.open {
            PromiseState::Open.as_str()
        } else {
            PromiseState::Proposed.as_str()
        })
        .bind(&promise.condition)
        .bind(&transfer_uid)
        .bind(&promise.reserve_from)
        .bind(promise.location.as_ref().and_then(|value| value.lat))
        .bind(promise.location.as_ref().and_then(|value| value.lon))
        .bind(
            promise
                .location
                .as_ref()
                .and_then(|value| value.address.as_deref()),
        )
        .bind(promise.open_reuse_policy.as_str())
        .bind(&now_string)
        .bind(&now_string)
        .execute(&mut *tx)
        .await?;
        promise_uids.push(promise_uid);
    }

    replace_dependencies(&mut tx, &transfer_uid, 1, &draft.dependencies).await?;

    if let Some(subject) = visibility_actor.as_deref() {
        sqlx::query(
            "INSERT INTO visibility_rule
                (uid, subject_kind, subject_uid, target_uid, grant_level)
             VALUES (?, 'actor', ?, ?, 'visible')",
        )
        .bind(nucleus::new_uid("v"))
        .bind(subject)
        .bind(&transfer_uid)
        .execute(&mut *tx)
        .await?;
    }
    if draft.visibility == "public" {
        sqlx::query(
            "INSERT INTO visibility_rule
                (uid, subject_kind, subject_uid, target_uid, grant_level)
             VALUES (?, 'public', NULL, ?, 'visible')",
        )
        .bind(nucleus::new_uid("v"))
        .bind(&transfer_uid)
        .execute(&mut *tx)
        .await?;
    }

    let snapshot = revision_snapshot(&mut tx, &transfer_uid).await?;
    let evidence = TransferRevisionEvidence {
        action: draft.evidence_action.clone(),
        idempotency_key: Some(request_key.into()),
        previous_revision: None,
        terms: snapshot,
    };
    let payload = serde_json::to_string(&evidence)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let previous_hash = crate::facts::last_hash(&mut tx).await?;
    let mut fact = nucleus::fact::seal(
        NewFact {
            uid: None,
            record_uid: transfer_uid.clone(),
            delta: crate::exact::one(),
            at: None,
            actor_uid: fact_actor,
            cause: Cause::user_edit(),
            payload: Some(payload),
        },
        &previous_hash,
        now,
    );
    fact.signature = sign(&fact.hash);
    if fact.signature.is_none() && draft.authorization_intent_uid.is_none() {
        return Err(sqlx::Error::Protocol(
            "Transfer draft requires a signing key or verified Action intent".into(),
        ));
    }
    crate::facts::insert(&mut tx, &fact).await?;
    if let Some(intent_uid) = draft.authorization_intent_uid.as_deref() {
        crate::action_intents::link_pending_fact(&mut tx, intent_uid, &fact).await?;
    }
    sqlx::query(
        "INSERT INTO transfer_revision
            (transfer_uid, revision, fact_uid, idempotency_key, created_at)
         VALUES (?, 1, ?, ?, ?)",
    )
    .bind(&transfer_uid)
    .bind(&fact.uid)
    .bind(request_key)
    .bind(&now_string)
    .execute(&mut *tx)
    .await?;
    if let Some(correction) = &draft.correction {
        if !matches!(correction.kind.as_str(), "remainder" | "reversal")
            || !correction.canonical_quantity.is_finite()
            || correction.canonical_quantity <= 0.0
        {
            return Err(sqlx::Error::Protocol(
                "invalid Transfer correction lineage".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO transfer_correction_link
                (uid, kind, source_transfer_uid, source_occurrence_uid,
                 created_transfer_uid, source_revision, canonical_quantity,
                 actor_person_uid, fact_uid, idempotency_key, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(nucleus::new_uid("tcl"))
        .bind(&correction.kind)
        .bind(&correction.source_transfer_uid)
        .bind(&correction.source_occurrence_uid)
        .bind(&transfer_uid)
        .bind(correction.source_revision as i64)
        .bind(correction.canonical_quantity)
        .bind(&draft.creator_person)
        .bind(&fact.uid)
        .bind(request_key)
        .bind(&now_string)
        .execute(&mut *tx)
        .await?;
    }
    let mut invitation_event_facts = Vec::with_capacity(invitation_uids.len());
    for invitation_uid in &invitation_uids {
        let event_request = format!("{request_key}:invitation:{invitation_uid}:addressed");
        invitation_event_facts.push(
            insert_invitation_event(
                &mut tx,
                NewInvitationEvent {
                    invitation_uid,
                    transfer_uid: &transfer_uid,
                    attempt: 1,
                    kind: "addressed",
                    from_status: None,
                    to_status: "pending",
                    actor_uid: Some(&draft.creator_person),
                    revision: Some(1),
                    request_id: &event_request,
                },
                now,
                &sign,
            )
            .await?,
        );
    }
    crate::records::bump_quantity(&mut tx, &transfer_uid, crate::exact::one(), &now_string).await?;

    tx.commit().await?;
    Ok(CreatedTransferDraft {
        transfer_uid,
        revision: 1,
        replayed: false,
        party_uids,
        invitation_uids,
        promise_uids,
        fact,
        invitation_event_facts,
    })
}

pub async fn revise_promise<F>(
    pool: &SqlitePool,
    input: PromiseRevisionInput,
    now: DateTime<Utc>,
    fact_actor: Option<String>,
    sign: F,
) -> Result<RevisionCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_key = input.idempotency_key.trim();
    if request_key.is_empty() || request_key.chars().count() > 200 {
        return Err(sqlx::Error::Protocol(
            "transfer revision idempotency key must contain 1 to 200 characters".into(),
        ));
    }
    let expected_revision = i64::try_from(input.expected_revision)
        .map_err(|_| sqlx::Error::Protocol("transfer revision exceeds SQLite range".into()))?;
    let mut tx = crate::write_tx(pool).await?;

    if let Some(row) = sqlx::query(
        "SELECT transfer_uid, revision, fact_uid FROM transfer_revision
         WHERE idempotency_key = ?",
    )
    .bind(request_key)
    .fetch_optional(&mut *tx)
    .await?
    {
        let existing_transfer: String = row.get("transfer_uid");
        if existing_transfer != input.transfer_uid {
            return Err(sqlx::Error::Protocol(
                "transfer request id was already used for another transfer".into(),
            ));
        }
        let revision = row.get::<i64, _>("revision") as u64;
        let fact_uid: String = row.get("fact_uid");
        if correction_link_for_request(pool, request_key)
            .await?
            .is_some()
            || promise_successor_for_request(pool, request_key)
                .await?
                .is_some()
            || open_claim_pair_for_request(pool, request_key)
                .await?
                .is_some()
        {
            return Err(sqlx::Error::Protocol(
                "transfer request id was replayed as a different action".into(),
            ));
        }
        tx.rollback().await?;
        let fact = crate::facts::get(pool, &fact_uid)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
        if !fact_has_transfer_revision_action(&fact, "revise-transfer-promise") {
            return Err(sqlx::Error::Protocol(
                "transfer request id was replayed as a different revision action".into(),
            ));
        }
        return Ok(RevisionCommit::Replayed { revision, fact });
    }

    ensure_request_not_used_by_invitation_event(pool, request_key).await?;

    let promise = sqlx::query("SELECT state FROM promise WHERE uid = ? AND transfer_uid = ?")
        .bind(&input.promise_uid)
        .bind(&input.transfer_uid)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let state: String = promise.get("state");
    if !matches!(
        PromiseState::parse(&state),
        Some(PromiseState::Proposed | PromiseState::Agreed)
    ) {
        return Err(sqlx::Error::Protocol(format!(
            "promise {} cannot be revised from state {state}",
            input.promise_uid
        )));
    }

    let advanced = sqlx::query(
        "UPDATE transfer SET revision = revision + 1
         WHERE record_uid = ? AND revision = ?",
    )
    .bind(&input.transfer_uid)
    .bind(expected_revision)
    .execute(&mut *tx)
    .await?;
    if advanced.rows_affected() == 0 {
        let current_revision: i64 =
            sqlx::query_scalar("SELECT revision FROM transfer WHERE record_uid = ?")
                .bind(&input.transfer_uid)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(sqlx::Error::RowNotFound)?;
        tx.rollback().await?;
        return Ok(RevisionCommit::Stale {
            current_revision: current_revision as u64,
        });
    }

    let revision = expected_revision + 1;
    let now_string = now.to_rfc3339();
    sqlx::query(
        "UPDATE promise
         SET record_uid = ?, concept_uid = NULL, party_uid = ?, delta = ?, window_end = ?,
             condition = ?, reserve_from = ?,
             state = CASE WHEN state = 'agreed' THEN 'proposed' ELSE state END,
             revision = ?, updated_at = ?
         WHERE uid = ? AND transfer_uid = ?",
    )
    .bind(&input.record_uid)
    .bind(&input.person_uid)
    .bind(input.delta)
    .bind(&input.window_end)
    .bind(&input.condition)
    .bind(&input.reserve_from)
    .bind(revision)
    .bind(&now_string)
    .bind(&input.promise_uid)
    .bind(&input.transfer_uid)
    .execute(&mut *tx)
    .await?;
    reset_agreements_for_revision(&mut tx, &input.transfer_uid, revision, now).await?;

    let snapshot = revision_snapshot(&mut tx, &input.transfer_uid).await?;
    let evidence = TransferRevisionEvidence {
        action: "revise-transfer-promise".into(),
        idempotency_key: Some(request_key.into()),
        previous_revision: Some(input.expected_revision),
        terms: snapshot,
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
            actor_uid: fact_actor,
            cause: Cause::user_edit(),
            payload: Some(payload),
        },
        &previous_hash,
        now,
    );
    fact.signature = sign(&fact.hash);
    crate::facts::insert(&mut tx, &fact).await?;
    sqlx::query(
        "INSERT INTO transfer_revision
            (transfer_uid, revision, fact_uid, idempotency_key, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&input.transfer_uid)
    .bind(revision)
    .bind(&fact.uid)
    .bind(request_key)
    .bind(&now_string)
    .execute(&mut *tx)
    .await?;
    crate::records::bump_quantity(
        &mut tx,
        &input.transfer_uid,
        crate::exact::zero(),
        &now_string,
    )
    .await?;
    tx.commit().await?;

    Ok(RevisionCommit::Committed {
        revision: revision as u64,
        fact,
    })
}

async fn revision_snapshot(
    tx: &mut Transaction<'_, Sqlite>,
    transfer_uid: &str,
) -> Result<TransferRevisionSnapshot, StoreError> {
    let row = sqlx::query(
        "SELECT t.*, r.slug, r.head
         FROM transfer t JOIN record r ON r.uid = t.record_uid
         WHERE t.record_uid = ?",
    )
    .bind(transfer_uid)
    .fetch_one(&mut **tx)
    .await?;
    let revision = row.get::<i64, _>("revision") as u64;
    let parties = sqlx::query(
        "SELECT uid, actor_uid, kind FROM transfer_party
         WHERE transfer_uid = ? ORDER BY uid",
    )
    .bind(transfer_uid)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(|row| TransferRevisionParty {
        uid: row.get("uid"),
        person_uid: row.get("actor_uid"),
        kind: row.get("kind"),
    })
    .collect();
    let invitations = sqlx::query(
        "SELECT uid, attempt, addressed_person_uid, invited_by_person_uid, status, expires_at
         FROM transfer_invitation WHERE transfer_uid = ? ORDER BY uid",
    )
    .bind(transfer_uid)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(|row| TransferRevisionInvitation {
        uid: row.get("uid"),
        attempt: row.get::<i64, _>("attempt") as u64,
        addressed_person_uid: row.get("addressed_person_uid"),
        invited_by_person_uid: row.get("invited_by_person_uid"),
        status: row.get("status"),
        expires_at: row.get("expires_at"),
    })
    .collect();
    let promises = sqlx::query(
        "SELECT uid, source_promise_uid, revision, record_uid, concept_uid, unit_uid, party_uid, delta,
                window_start, window_end, location_lat, location_lon, location_address,
                condition, reserve_from, state, open_reuse_policy
         FROM promise WHERE transfer_uid = ? ORDER BY uid",
    )
    .bind(transfer_uid)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
        .map(|row| TransferRevisionPromise {
            uid: row.get("uid"),
            source_promise_uid: row.get("source_promise_uid"),
        revision: row.get::<i64, _>("revision") as u64,
        record_uid: row.get("record_uid"),
        concept_uid: row.get("concept_uid"),
        unit_uid: row.get("unit_uid"),
        person_uid: row.get("party_uid"),
        delta: row.get("delta"),
        window_start: row.get("window_start"),
        window_end: row.get("window_end"),
        location: location_snapshot(
            row.get("location_lat"),
            row.get("location_lon"),
            row.get("location_address"),
        ),
        condition: row.get("condition"),
        reserve_from: row.get("reserve_from"),
        state: row.get("state"),
        open_reuse_policy: OpenPromiseReusePolicy::parse(
            row.get::<String, _>("open_reuse_policy").as_str(),
        )
        .unwrap_or_default(),
    })
    .collect();
    let dependencies = sqlx::query(
        "SELECT uid, scope, promise_uid, upstream_kind, upstream_uid, required_state
         FROM transfer_dependency WHERE transfer_uid = ? ORDER BY uid",
    )
    .bind(transfer_uid)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(|row| -> Result<TransferRevisionDependency, StoreError> {
        let scope: String = row.get("scope");
        let upstream_kind: String = row.get("upstream_kind");
        Ok(TransferRevisionDependency {
            uid: row.get("uid"),
            scope: TransferDependencyScope::parse(&scope).ok_or_else(|| {
                sqlx::Error::Protocol(format!("unknown dependency scope `{scope}`"))
            })?,
            promise_uid: row.get("promise_uid"),
            upstream_kind: TransferDependencyUpstreamKind::parse(&upstream_kind).ok_or_else(
                || {
                    sqlx::Error::Protocol(format!(
                        "unknown dependency upstream kind `{upstream_kind}`"
                    ))
                },
            )?,
            upstream_uid: row.get("upstream_uid"),
            required_state: row.get("required_state"),
        })
    })
    .collect::<Result<Vec<_>, _>>()?;
    let mut snapshot = TransferRevisionSnapshot {
        revision,
        transfer: TransferRevisionTerms {
            uid: row.get("record_uid"),
            slug: row.get("slug"),
            head: row.get("head"),
            agreement_type: row.get("agreement_type"),
            agreement_pct: row.get("agreement_pct"),
            settlement: row.get("settlement"),
            visibility: row.get("visibility"),
            max_proximity: row.get("max_proximity"),
            satiation: row.get("satiation"),
            parent_uid: row.get("parent_uid"),
            source_uid: row.get("source_uid"),
            reserve_default: row.get("reserve_default"),
            require_confirmation: row.get::<i64, _>("require_confirmation") != 0,
            default_place: location_snapshot(
                row.get("default_location_lat"),
                row.get("default_location_lon"),
                row.get("default_location_address"),
            ),
        },
        parties,
        invitations,
        promises,
        dependencies,
    };
    snapshot.canonicalize();
    Ok(snapshot)
}

fn location_snapshot(
    lat: Option<f64>,
    lon: Option<f64>,
    address: Option<String>,
) -> Option<TransferLocationSnapshot> {
    if lat.is_none() && lon.is_none() && address.is_none() {
        None
    } else {
        Some(TransferLocationSnapshot { lat, lon, address })
    }
}

pub async fn create(pool: &SqlitePool, new: NewTransfer<'_>) -> Result<String, StoreError> {
    let rec = records::create(
        pool,
        NewRecord {
            slug: new.slug,
            kind: RecordKind::Transfer,
            head: new.head,
            body: "",
            quantity: crate::exact::zero(),
        },
    )
    .await?;
    sqlx::query(
        "INSERT INTO transfer (record_uid, agreement_type, agreement_pct, satiation, source_uid,
                               reserve_default, require_confirmation)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&rec.uid)
    .bind(new.agreement_type)
    .bind(new.agreement_pct)
    .bind(new.satiation)
    .bind(new.source_uid)
    .bind(new.reserve_default)
    .bind(new.require_confirmation as i64)
    .execute(pool)
    .await?;
    Ok(rec.uid)
}

#[derive(Debug, Clone)]
pub struct TransferListRow {
    pub transfer: TransferRow,
    pub slug: Option<String>,
    pub head: String,
}

pub async fn list_all(pool: &SqlitePool) -> Result<Vec<TransferListRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT t.*, r.slug, r.head, r.quantity_mantissa, r.quantity_scale FROM transfer t
           JOIN record r ON r.uid = t.record_uid
          ORDER BY r.created_at",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| TransferListRow {
        transfer: TransferRow {
            record_uid: r.get("record_uid"),
            revision: r.get("revision"),
            agreement_type: r.get("agreement_type"),
            agreement_pct: r.get("agreement_pct"),
            settlement: r.get("settlement"),
            visibility: r.get("visibility"),
            max_proximity: r.get("max_proximity"),
            satiation: r.get("satiation"),
            parent_uid: r.get("parent_uid"),
            source_uid: r.get("source_uid"),
            reserve_default: r.get("reserve_default"),
            active: r.get::<String, _>("quantity_mantissa") != "0",
            require_confirmation: r.get::<i64, _>("require_confirmation") != 0,
            default_place: location_snapshot(
                r.get("default_location_lat"),
                r.get("default_location_lon"),
                r.get("default_location_address"),
            ),
        },
        slug: r.get("slug"),
        head: r.get("head"),
    })
    .collect())
}

pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Option<TransferRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT t.*, r.quantity_mantissa, r.quantity_scale FROM transfer t JOIN record r ON r.uid = t.record_uid
         WHERE t.record_uid = ?",
    )
    .bind(uid)
    .fetch_optional(pool)
    .await?
    .map(|r| TransferRow {
        record_uid: r.get("record_uid"),
        revision: r.get("revision"),
        agreement_type: r.get("agreement_type"),
        agreement_pct: r.get("agreement_pct"),
        settlement: r.get("settlement"),
        visibility: r.get("visibility"),
        max_proximity: r.get("max_proximity"),
        satiation: r.get("satiation"),
        parent_uid: r.get("parent_uid"),
        source_uid: r.get("source_uid"),
        reserve_default: r.get("reserve_default"),
        active: r.get::<String, _>("quantity_mantissa") != "0",
        require_confirmation: r.get::<i64, _>("require_confirmation") != 0,
        default_place: location_snapshot(
            r.get("default_location_lat"),
            r.get("default_location_lon"),
            r.get("default_location_address"),
        ),
    }))
}

pub async fn add_party(
    pool: &SqlitePool,
    transfer_uid: &str,
    actor_uid: &str,
) -> Result<String, StoreError> {
    let uid = nucleus::new_uid("y");
    sqlx::query("INSERT INTO transfer_party (uid, transfer_uid, actor_uid) VALUES (?, ?, ?)")
        .bind(&uid)
        .bind(transfer_uid)
        .bind(actor_uid)
        .execute(pool)
        .await?;
    Ok(uid)
}

pub async fn creator_party_actor(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<Option<String>, StoreError> {
    sqlx::query_scalar(
        "SELECT actor_uid FROM transfer_party
         WHERE transfer_uid = ? AND kind = 'creator' LIMIT 1",
    )
    .bind(transfer_uid)
    .fetch_optional(pool)
    .await
}

pub async fn party_levels(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<Vec<(String, String, i64)>, StoreError> {
    Ok(sqlx::query(
        "SELECT p.uid, p.actor_uid, COALESCE(a.level, 0) AS level
         FROM transfer_party p
         JOIN transfer t ON t.record_uid = p.transfer_uid
         LEFT JOIN transfer_agreement a
           ON a.party_uid = p.uid
          AND a.transfer_uid = p.transfer_uid
          AND a.revision = t.revision
         WHERE p.transfer_uid = ?",
    )
    .bind(transfer_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| (r.get("uid"), r.get("actor_uid"), r.get("level")))
    .collect())
}

pub async fn party_actor(
    pool: &SqlitePool,
    transfer_uid: &str,
    party_uid: &str,
) -> Result<Option<String>, StoreError> {
    sqlx::query_scalar("SELECT actor_uid FROM transfer_party WHERE transfer_uid = ? AND uid = ?")
        .bind(transfer_uid)
        .bind(party_uid)
        .fetch_optional(pool)
        .await
}

pub async fn party_for_actor(
    pool: &SqlitePool,
    transfer_uid: &str,
    actor_uid: &str,
) -> Result<Option<String>, StoreError> {
    sqlx::query_scalar("SELECT uid FROM transfer_party WHERE transfer_uid = ? AND actor_uid = ?")
        .bind(transfer_uid)
        .bind(actor_uid)
        .fetch_optional(pool)
        .await
}

fn map_invitation(row: SqliteRow) -> Result<TransferInvitationRow, StoreError> {
    Ok(TransferInvitationRow {
        uid: row.get("uid"),
        transfer_uid: row.get("transfer_uid"),
        addressed_person_uid: row.get("addressed_person_uid"),
        invited_by_person_uid: row.get("invited_by_person_uid"),
        status: TransferInvitationStatus::from_db(row.get::<String, _>("status").as_str())?,
        attempt: row.get::<i64, _>("attempt") as u64,
        party_uid: row.get("party_uid"),
        expires_at: row.get("expires_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

pub async fn invitation(
    pool: &SqlitePool,
    uid: &str,
) -> Result<Option<TransferInvitationRow>, StoreError> {
    sqlx::query("SELECT * FROM transfer_invitation WHERE uid = ?")
        .bind(uid)
        .fetch_optional(pool)
        .await?
        .map(map_invitation)
        .transpose()
}

pub async fn invitations_for_transfer(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<Vec<TransferInvitationRow>, StoreError> {
    sqlx::query(
        "SELECT * FROM transfer_invitation
         WHERE transfer_uid = ? ORDER BY created_at, uid",
    )
    .bind(transfer_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_invitation)
    .collect()
}

pub async fn pending_invitations_for_person(
    pool: &SqlitePool,
    person_uid: &str,
) -> Result<Vec<TransferInvitationRow>, StoreError> {
    sqlx::query(
        "SELECT * FROM transfer_invitation
         WHERE addressed_person_uid = ? AND status = 'pending'
         ORDER BY created_at, uid",
    )
    .bind(person_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_invitation)
    .collect()
}

#[derive(Debug, Clone)]
pub struct TransferInvitationEventRow {
    pub uid: String,
    pub invitation_uid: String,
    pub transfer_uid: String,
    pub attempt: u64,
    pub kind: String,
    pub actor_uid: Option<String>,
    pub revision: Option<u64>,
    pub fact_uid: String,
    pub idempotency_key: String,
    pub created_at: String,
}

fn map_invitation_event(row: SqliteRow) -> TransferInvitationEventRow {
    TransferInvitationEventRow {
        uid: row.get("uid"),
        invitation_uid: row.get("invitation_uid"),
        transfer_uid: row.get("transfer_uid"),
        attempt: row.get::<i64, _>("attempt") as u64,
        kind: row.get("kind"),
        actor_uid: row.get("actor_uid"),
        revision: row
            .get::<Option<i64>, _>("revision")
            .map(|value| value as u64),
        fact_uid: row.get("fact_uid"),
        idempotency_key: row.get("idempotency_key"),
        created_at: row.get("created_at"),
    }
}

pub async fn invitation_events(
    pool: &SqlitePool,
    invitation_uid: &str,
) -> Result<Vec<TransferInvitationEventRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM transfer_invitation_event
         WHERE invitation_uid = ? ORDER BY attempt, created_at, uid",
    )
    .bind(invitation_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_invitation_event)
    .collect())
}

pub async fn invitation_event_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<TransferInvitationEventRow>, StoreError> {
    Ok(
        sqlx::query("SELECT * FROM transfer_invitation_event WHERE idempotency_key = ?")
            .bind(request_id.trim())
            .fetch_optional(pool)
            .await?
            .map(map_invitation_event),
    )
}

fn map_open_claim_pair(row: SqliteRow) -> Result<OpenPromiseClaimPairRow, StoreError> {
    let reuse_policy: String = row.get("reuse_policy");
    Ok(OpenPromiseClaimPairRow {
        uid: row.get("uid"),
        transfer_uid: row.get("transfer_uid"),
        source_promise_uid: row.get("source_promise_uid"),
        source_record_uid: row.get("source_record_uid"),
        proposer_promise_uid: row.get("proposer_promise_uid"),
        claimant_promise_uid: row.get("claimant_promise_uid"),
        proposer_person_uid: row.get("proposer_person_uid"),
        claimant_person_uid: row.get("claimant_person_uid"),
        revision: row.get::<i64, _>("revision") as u64,
        reuse_policy: OpenPromiseReusePolicy::parse(&reuse_policy).ok_or_else(|| {
            sqlx::Error::Protocol(format!("unknown OPEN reuse policy `{reuse_policy}`"))
        })?,
        idempotency_key: row.get("idempotency_key"),
        created_at: row.get("created_at"),
    })
}

pub async fn open_claim_pairs_for_source(
    pool: &SqlitePool,
    source_promise_uid: &str,
) -> Result<Vec<OpenPromiseClaimPairRow>, StoreError> {
    sqlx::query(
        "SELECT * FROM transfer_open_claim_pair
         WHERE source_promise_uid = ? ORDER BY rowid",
    )
    .bind(source_promise_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_open_claim_pair)
    .collect()
}

pub async fn open_claim_pair_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<OpenPromiseClaimPairRow>, StoreError> {
    sqlx::query("SELECT * FROM transfer_open_claim_pair WHERE idempotency_key = ?")
        .bind(request_id.trim())
        .fetch_optional(pool)
        .await?
        .map(map_open_claim_pair)
        .transpose()
}

pub async fn open_claim_target_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<(String, String, String, String)>, StoreError> {
    Ok(open_claim_pair_for_request(pool, request_id)
        .await?
        .map(|pair| {
            (
                pair.transfer_uid,
                pair.source_promise_uid,
                pair.claimant_person_uid,
                pair.claimant_promise_uid,
            )
        }))
}

fn validate_request_key(value: &str) -> Result<&str, StoreError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 200 {
        return Err(sqlx::Error::Protocol(
            "transfer request id must contain 1 to 200 characters".into(),
        ));
    }
    Ok(value)
}

fn invitation_status_for_event(kind: &str) -> Result<TransferInvitationStatus, StoreError> {
    match kind {
        "addressed" | "reopened" => Ok(TransferInvitationStatus::Pending),
        "accepted" => Ok(TransferInvitationStatus::Accepted),
        "rejected" => Ok(TransferInvitationStatus::Rejected),
        "withdrawn" => Ok(TransferInvitationStatus::Withdrawn),
        "expired" => Ok(TransferInvitationStatus::Expired),
        value => Err(sqlx::Error::Protocol(format!(
            "unknown transfer invitation event kind `{value}`"
        ))),
    }
}

async fn invitation_commit_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<InvitationCommitOutcome>, StoreError> {
    let Some(event_row) =
        sqlx::query("SELECT * FROM transfer_invitation_event WHERE idempotency_key = ?")
            .bind(request_id)
            .fetch_optional(pool)
            .await?
    else {
        return Ok(None);
    };
    let event = map_invitation_event(event_row);
    let mut invitation = invitation(pool, &event.invitation_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    invitation.status = invitation_status_for_event(&event.kind)?;
    invitation.attempt = event.attempt;
    if invitation.status != TransferInvitationStatus::Accepted {
        invitation.party_uid = None;
    }
    let event_fact = crate::facts::get(pool, &event.fact_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let revision_fact = if let Some(revision) = event.revision {
        revision_fact(pool, &event.transfer_uid, revision).await?
    } else {
        None
    };
    Ok(Some(InvitationCommitOutcome {
        transfer_uid: event.transfer_uid,
        party_uid: invitation.party_uid.clone(),
        invitation,
        revision: event.revision,
        revision_fact,
        event_fact,
    }))
}

async fn ensure_request_not_used_by_transfer_revision(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<(), StoreError> {
    let used: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM transfer_revision WHERE idempotency_key = ?)
             OR EXISTS(SELECT 1 FROM transfer_agreement_event WHERE idempotency_key = ?)
             OR EXISTS(SELECT 1 FROM transfer_open_claim_pair WHERE idempotency_key = ?)
             OR EXISTS(SELECT 1 FROM transfer_phase4_request WHERE idempotency_key = ?)
             OR EXISTS(SELECT 1 FROM transfer_phase5_request WHERE idempotency_key = ?)
             OR EXISTS(SELECT 1 FROM transfer_phase5_correction_request WHERE idempotency_key = ?)
             OR EXISTS(SELECT 1 FROM transfer_phase6_bulk_request WHERE idempotency_key = ?)
             OR EXISTS(SELECT 1 FROM transfer_correction_link WHERE idempotency_key = ?)
             OR EXISTS(SELECT 1 FROM transfer_promise_successor WHERE idempotency_key = ?)",
    )
    .bind(request_id)
    .bind(request_id)
    .bind(request_id)
    .bind(request_id)
    .bind(request_id)
    .bind(request_id)
    .bind(request_id)
    .bind(request_id)
    .bind(request_id)
    .fetch_one(pool)
    .await?;
    if used {
        return Err(sqlx::Error::Protocol(
            "transfer request id was already used by another action".into(),
        ));
    }
    Ok(())
}

async fn ensure_request_not_used_by_invitation_event(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<(), StoreError> {
    let used: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM transfer_invitation_event WHERE idempotency_key = ?)
             OR EXISTS(SELECT 1 FROM transfer_agreement_event WHERE idempotency_key = ?)
             OR EXISTS(SELECT 1 FROM transfer_open_claim_pair WHERE idempotency_key = ?)
             OR EXISTS(SELECT 1 FROM transfer_phase4_request WHERE idempotency_key = ?)
             OR EXISTS(SELECT 1 FROM transfer_phase5_request WHERE idempotency_key = ?)
             OR EXISTS(SELECT 1 FROM transfer_phase5_correction_request WHERE idempotency_key = ?)
             OR EXISTS(SELECT 1 FROM transfer_phase6_bulk_request WHERE idempotency_key = ?)
             OR EXISTS(SELECT 1 FROM transfer_correction_link WHERE idempotency_key = ?)
             OR EXISTS(SELECT 1 FROM transfer_promise_successor WHERE idempotency_key = ?)",
    )
    .bind(request_id)
    .bind(request_id)
    .bind(request_id)
    .bind(request_id)
    .bind(request_id)
    .bind(request_id)
    .bind(request_id)
    .bind(request_id)
    .bind(request_id)
    .fetch_one(pool)
    .await?;
    if used {
        return Err(sqlx::Error::Protocol(
            "transfer request id was already used by another action".into(),
        ));
    }
    Ok(())
}

async fn advance_transfer_revision(
    tx: &mut Transaction<'_, Sqlite>,
    transfer_uid: &str,
    expected_revision: u64,
) -> Result<Result<i64, u64>, StoreError> {
    let expected = i64::try_from(expected_revision)
        .map_err(|_| sqlx::Error::Protocol("transfer revision exceeds SQLite range".into()))?;
    let revision = expected
        .checked_add(1)
        .ok_or_else(|| sqlx::Error::Protocol("transfer revision overflow".into()))?;
    let result =
        sqlx::query("UPDATE transfer SET revision = ? WHERE record_uid = ? AND revision = ?")
            .bind(revision)
            .bind(transfer_uid)
            .bind(expected)
            .execute(&mut **tx)
            .await?;
    if result.rows_affected() != 0 {
        return Ok(Ok(revision));
    }
    let current: i64 = sqlx::query_scalar("SELECT revision FROM transfer WHERE record_uid = ?")
        .bind(transfer_uid)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    Ok(Err(current as u64))
}

async fn insert_revision_fact<F>(
    tx: &mut Transaction<'_, Sqlite>,
    transfer_uid: &str,
    revision: i64,
    previous_revision: u64,
    request_id: &str,
    action: &str,
    actor_uid: Option<String>,
    now: DateTime<Utc>,
    sign: &F,
) -> Result<Fact, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let snapshot = revision_snapshot(tx, transfer_uid).await?;
    let evidence = TransferRevisionEvidence {
        action: action.into(),
        idempotency_key: Some(request_id.into()),
        previous_revision: Some(previous_revision),
        terms: snapshot,
    };
    let payload = serde_json::to_string(&evidence)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let previous_hash = crate::facts::last_hash(tx).await?;
    let mut fact = nucleus::fact::seal(
        NewFact {
            uid: None,
            record_uid: transfer_uid.into(),
            delta: crate::exact::zero(),
            at: None,
            actor_uid,
            cause: Cause::user_edit(),
            payload: Some(payload),
        },
        &previous_hash,
        now,
    );
    fact.signature = sign(&fact.hash);
    crate::facts::insert(tx, &fact).await?;
    sqlx::query(
        "INSERT INTO transfer_revision
            (transfer_uid, revision, fact_uid, idempotency_key, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(transfer_uid)
    .bind(revision)
    .bind(&fact.uid)
    .bind(request_id)
    .bind(now.to_rfc3339())
    .execute(&mut **tx)
    .await?;
    Ok(fact)
}

struct NewInvitationEvent<'a> {
    invitation_uid: &'a str,
    transfer_uid: &'a str,
    attempt: u64,
    kind: &'a str,
    from_status: Option<&'a str>,
    to_status: &'a str,
    actor_uid: Option<&'a str>,
    revision: Option<u64>,
    request_id: &'a str,
}

async fn insert_invitation_event<F>(
    tx: &mut Transaction<'_, Sqlite>,
    event: NewInvitationEvent<'_>,
    now: DateTime<Utc>,
    sign: &F,
) -> Result<Fact, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let evidence = TransferInvitationLifecycleEvidence {
        action: format!("{}-transfer-invitation", event.kind),
        idempotency_key: event.request_id.into(),
        invitation_uid: event.invitation_uid.into(),
        transfer_uid: event.transfer_uid.into(),
        attempt: event.attempt,
        from_status: event.from_status.map(str::to_owned),
        to_status: event.to_status.into(),
        actor_person_uid: event.actor_uid.map(str::to_owned),
        revision: event.revision,
    };
    let payload = serde_json::to_string(&evidence)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let previous_hash = crate::facts::last_hash(tx).await?;
    let mut fact = nucleus::fact::seal(
        NewFact {
            uid: None,
            record_uid: event.transfer_uid.into(),
            delta: crate::exact::zero(),
            at: None,
            actor_uid: event.actor_uid.map(str::to_owned),
            cause: Cause::user_edit(),
            payload: Some(payload),
        },
        &previous_hash,
        now,
    );
    fact.signature = sign(&fact.hash);
    crate::facts::insert(tx, &fact).await?;
    sqlx::query(
        "INSERT INTO transfer_invitation_event
            (uid, invitation_uid, transfer_uid, attempt, kind, actor_uid,
             revision, fact_uid, idempotency_key, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(nucleus::new_uid("tie"))
    .bind(event.invitation_uid)
    .bind(event.transfer_uid)
    .bind(
        i64::try_from(event.attempt)
            .map_err(|_| sqlx::Error::Protocol("invitation attempt exceeds SQLite range".into()))?,
    )
    .bind(event.kind)
    .bind(event.actor_uid)
    .bind(event.revision.map(|value| value as i64))
    .bind(&fact.uid)
    .bind(event.request_id)
    .bind(now.to_rfc3339())
    .execute(&mut **tx)
    .await?;
    Ok(fact)
}

async fn reset_agreements_for_revision(
    tx: &mut Transaction<'_, Sqlite>,
    transfer_uid: &str,
    revision: i64,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE transfer_agreement SET level = 0, revision = ?, at = ?
         WHERE transfer_uid = ?",
    )
    .bind(revision)
    .bind(now.to_rfc3339())
    .bind(transfer_uid)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE promise SET state = 'proposed', updated_at = ?
         WHERE transfer_uid = ? AND state = 'agreed'",
    )
    .bind(now.to_rfc3339())
    .bind(transfer_uid)
    .execute(&mut **tx)
    .await?;
    sqlx::query("UPDATE transfer_dependency SET revision = ? WHERE transfer_uid = ?")
        .bind(revision)
        .bind(transfer_uid)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn address_transfer_invitation<F>(
    pool: &SqlitePool,
    input: AddressTransferInvitationInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<InvitationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_id = validate_request_key(&input.idempotency_key)?;
    if let Some(outcome) = invitation_commit_for_request(pool, request_id).await? {
        if outcome.transfer_uid != input.transfer_uid {
            return Err(sqlx::Error::Protocol(
                "transfer request id was already used for another transfer".into(),
            ));
        }
        return Ok(InvitationCommit::Replayed(outcome));
    }
    ensure_request_not_used_by_transfer_revision(pool, request_id).await?;
    let mut tx = crate::write_tx(pool).await?;
    for (role, person_uid) in [
        ("addressed", input.addressed_person_uid.as_str()),
        ("inviting", input.invited_by_person_uid.as_str()),
    ] {
        let valid: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM record WHERE uid = ? AND kind = 'person')",
        )
        .bind(person_uid)
        .fetch_one(&mut *tx)
        .await?;
        if !valid {
            return Err(sqlx::Error::Protocol(format!(
                "{role} transfer invitation identity `{person_uid}` is not a Person"
            )));
        }
    }
    let existing: Option<(String, String)> = sqlx::query_as(
        "SELECT uid, status FROM transfer_invitation
         WHERE transfer_uid = ? AND addressed_person_uid = ? LIMIT 1",
    )
    .bind(&input.transfer_uid)
    .bind(&input.addressed_person_uid)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((uid, status)) = existing {
        return Err(sqlx::Error::Protocol(format!(
            "invitation `{uid}` already represents this Person with status `{status}`; reopen it instead"
        )));
    }
    let already_party: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM transfer_party
         WHERE transfer_uid = ? AND actor_uid = ?)",
    )
    .bind(&input.transfer_uid)
    .bind(&input.addressed_person_uid)
    .fetch_one(&mut *tx)
    .await?;
    if already_party {
        return Err(sqlx::Error::Protocol(
            "a current transfer party cannot be invited".into(),
        ));
    }
    let revision =
        match advance_transfer_revision(&mut tx, &input.transfer_uid, input.expected_revision)
            .await?
        {
            Ok(revision) => revision,
            Err(current_revision) => {
                tx.rollback().await?;
                return Ok(InvitationCommit::Stale {
                    transfer_uid: input.transfer_uid,
                    current_revision,
                });
            }
        };
    let invitation_uid = nucleus::new_uid("ti");
    let at = now.to_rfc3339();
    sqlx::query(
        "INSERT INTO transfer_invitation
            (uid, transfer_uid, addressed_person_uid, invited_by_person_uid,
             status, party_uid, expires_at, attempt, created_at, updated_at)
         VALUES (?, ?, ?, ?, 'pending', NULL, ?, 1, ?, ?)",
    )
    .bind(&invitation_uid)
    .bind(&input.transfer_uid)
    .bind(&input.addressed_person_uid)
    .bind(&input.invited_by_person_uid)
    .bind(input.expires_at.map(|value| value.to_rfc3339()))
    .bind(&at)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    reset_agreements_for_revision(&mut tx, &input.transfer_uid, revision, now).await?;
    let revision_fact = insert_revision_fact(
        &mut tx,
        &input.transfer_uid,
        revision,
        input.expected_revision,
        request_id,
        "address-transfer-invitation",
        Some(input.invited_by_person_uid.clone()),
        now,
        &sign,
    )
    .await?;
    let event_fact = insert_invitation_event(
        &mut tx,
        NewInvitationEvent {
            invitation_uid: &invitation_uid,
            transfer_uid: &input.transfer_uid,
            attempt: 1,
            kind: "addressed",
            from_status: None,
            to_status: "pending",
            actor_uid: Some(&input.invited_by_person_uid),
            revision: Some(revision as u64),
            request_id,
        },
        now,
        &sign,
    )
    .await?;
    crate::records::bump_quantity(&mut tx, &input.transfer_uid, crate::exact::zero(), &at).await?;
    let invitation = map_invitation(
        sqlx::query("SELECT * FROM transfer_invitation WHERE uid = ?")
            .bind(&invitation_uid)
            .fetch_one(&mut *tx)
            .await?,
    )?;
    tx.commit().await?;
    Ok(InvitationCommit::Committed(InvitationCommitOutcome {
        transfer_uid: input.transfer_uid,
        invitation,
        party_uid: None,
        revision: Some(revision as u64),
        revision_fact: Some(revision_fact),
        event_fact,
    }))
}

pub async fn accept_transfer_invitation<F>(
    pool: &SqlitePool,
    input: InvitationTransitionInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<InvitationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_id = validate_request_key(&input.idempotency_key)?;
    if let Some(outcome) = invitation_commit_for_request(pool, request_id).await? {
        return Ok(InvitationCommit::Replayed(outcome));
    }
    ensure_request_not_used_by_transfer_revision(pool, request_id).await?;
    let actor_uid = input.actor_person_uid.as_deref().ok_or_else(|| {
        sqlx::Error::Protocol("accepting an invitation requires a Person actor".into())
    })?;
    let mut tx = crate::write_tx(pool).await?;
    let pending =
        sqlx::query("SELECT * FROM transfer_invitation WHERE uid = ? AND status = 'pending'")
            .bind(&input.invitation_uid)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| sqlx::Error::Protocol("invitation is not pending".into()))?;
    let pending = map_invitation(pending)?;
    if pending.addressed_person_uid != actor_uid {
        return Err(sqlx::Error::Protocol(
            "only the addressed Person may accept an invitation".into(),
        ));
    }
    if pending
        .expires_at
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .is_some_and(|value| value.with_timezone(&Utc) <= now)
    {
        return Err(sqlx::Error::Protocol(
            "expired invitation must be expired and reopened before acceptance".into(),
        ));
    }
    let revision =
        match advance_transfer_revision(&mut tx, &pending.transfer_uid, input.expected_revision)
            .await?
        {
            Ok(revision) => revision,
            Err(current_revision) => {
                tx.rollback().await?;
                return Ok(InvitationCommit::Stale {
                    transfer_uid: pending.transfer_uid,
                    current_revision,
                });
            }
        };
    let party_uid = nucleus::new_uid("y");
    sqlx::query(
        "INSERT INTO transfer_party (uid, transfer_uid, actor_uid, kind)
         VALUES (?, ?, ?, 'participant')",
    )
    .bind(&party_uid)
    .bind(&pending.transfer_uid)
    .bind(actor_uid)
    .execute(&mut *tx)
    .await?;
    let at = now.to_rfc3339();
    sqlx::query(
        "UPDATE transfer_invitation SET status = 'accepted', party_uid = ?, updated_at = ?
         WHERE uid = ? AND status = 'pending'",
    )
    .bind(&party_uid)
    .bind(&at)
    .bind(&input.invitation_uid)
    .execute(&mut *tx)
    .await?;
    reset_agreements_for_revision(&mut tx, &pending.transfer_uid, revision, now).await?;
    sqlx::query(
        "INSERT INTO transfer_agreement (uid, transfer_uid, party_uid, level, at, revision)
         VALUES (?, ?, ?, 0, ?, ?)",
    )
    .bind(nucleus::new_uid("g"))
    .bind(&pending.transfer_uid)
    .bind(&party_uid)
    .bind(&at)
    .bind(revision)
    .execute(&mut *tx)
    .await?;
    let revision_fact = insert_revision_fact(
        &mut tx,
        &pending.transfer_uid,
        revision,
        input.expected_revision,
        request_id,
        "accept-transfer-invitation",
        Some(actor_uid.into()),
        now,
        &sign,
    )
    .await?;
    let event_fact = insert_invitation_event(
        &mut tx,
        NewInvitationEvent {
            invitation_uid: &pending.uid,
            transfer_uid: &pending.transfer_uid,
            attempt: pending.attempt,
            kind: "accepted",
            from_status: Some("pending"),
            to_status: "accepted",
            actor_uid: Some(actor_uid),
            revision: Some(revision as u64),
            request_id,
        },
        now,
        &sign,
    )
    .await?;
    crate::records::bump_quantity(&mut tx, &pending.transfer_uid, crate::exact::zero(), &at)
        .await?;
    let invitation = map_invitation(
        sqlx::query("SELECT * FROM transfer_invitation WHERE uid = ?")
            .bind(&pending.uid)
            .fetch_one(&mut *tx)
            .await?,
    )?;
    tx.commit().await?;
    Ok(InvitationCommit::Committed(InvitationCommitOutcome {
        transfer_uid: pending.transfer_uid,
        invitation,
        party_uid: Some(party_uid),
        revision: Some(revision as u64),
        revision_fact: Some(revision_fact),
        event_fact,
    }))
}

pub async fn withdraw_transfer_invitation<F>(
    pool: &SqlitePool,
    input: InvitationTransitionInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<InvitationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_id = validate_request_key(&input.idempotency_key)?;
    if let Some(outcome) = invitation_commit_for_request(pool, request_id).await? {
        return Ok(InvitationCommit::Replayed(outcome));
    }
    ensure_request_not_used_by_transfer_revision(pool, request_id).await?;
    let actor_uid = input.actor_person_uid.as_deref().ok_or_else(|| {
        sqlx::Error::Protocol("withdrawing an invitation requires a Person actor".into())
    })?;
    let mut tx = crate::write_tx(pool).await?;
    let pending =
        sqlx::query("SELECT * FROM transfer_invitation WHERE uid = ? AND status = 'pending'")
            .bind(&input.invitation_uid)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| sqlx::Error::Protocol("invitation is not pending".into()))?;
    let pending = map_invitation(pending)?;
    let creator: Option<String> = sqlx::query_scalar(
        "SELECT actor_uid FROM transfer_party
         WHERE transfer_uid = ? AND kind = 'creator' LIMIT 1",
    )
    .bind(&pending.transfer_uid)
    .fetch_optional(&mut *tx)
    .await?;
    if creator.as_deref() != Some(actor_uid) {
        return Err(sqlx::Error::Protocol(
            "only the Transfer creator may withdraw an invitation".into(),
        ));
    }
    let revision =
        match advance_transfer_revision(&mut tx, &pending.transfer_uid, input.expected_revision)
            .await?
        {
            Ok(revision) => revision,
            Err(current_revision) => {
                tx.rollback().await?;
                return Ok(InvitationCommit::Stale {
                    transfer_uid: pending.transfer_uid,
                    current_revision,
                });
            }
        };
    let at = now.to_rfc3339();
    sqlx::query(
        "UPDATE transfer_invitation
         SET status = 'withdrawn', party_uid = NULL, updated_at = ?
         WHERE uid = ? AND status = 'pending'",
    )
    .bind(&at)
    .bind(&pending.uid)
    .execute(&mut *tx)
    .await?;
    reset_agreements_for_revision(&mut tx, &pending.transfer_uid, revision, now).await?;
    let revision_fact = insert_revision_fact(
        &mut tx,
        &pending.transfer_uid,
        revision,
        input.expected_revision,
        request_id,
        "withdraw-transfer-invitation",
        Some(actor_uid.into()),
        now,
        &sign,
    )
    .await?;
    let event_fact = insert_invitation_event(
        &mut tx,
        NewInvitationEvent {
            invitation_uid: &pending.uid,
            transfer_uid: &pending.transfer_uid,
            attempt: pending.attempt,
            kind: "withdrawn",
            from_status: Some("pending"),
            to_status: "withdrawn",
            actor_uid: Some(actor_uid),
            revision: Some(revision as u64),
            request_id,
        },
        now,
        &sign,
    )
    .await?;
    crate::records::bump_quantity(&mut tx, &pending.transfer_uid, crate::exact::zero(), &at)
        .await?;
    let invitation = map_invitation(
        sqlx::query("SELECT * FROM transfer_invitation WHERE uid = ?")
            .bind(&pending.uid)
            .fetch_one(&mut *tx)
            .await?,
    )?;
    tx.commit().await?;
    Ok(InvitationCommit::Committed(InvitationCommitOutcome {
        transfer_uid: pending.transfer_uid,
        invitation,
        party_uid: None,
        revision: Some(revision as u64),
        revision_fact: Some(revision_fact),
        event_fact,
    }))
}

pub async fn reopen_transfer_invitation<F>(
    pool: &SqlitePool,
    input: InvitationTransitionInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<InvitationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_id = validate_request_key(&input.idempotency_key)?;
    if let Some(outcome) = invitation_commit_for_request(pool, request_id).await? {
        return Ok(InvitationCommit::Replayed(outcome));
    }
    ensure_request_not_used_by_transfer_revision(pool, request_id).await?;
    let actor_uid = input.actor_person_uid.as_deref().ok_or_else(|| {
        sqlx::Error::Protocol("reopening an invitation requires a Person actor".into())
    })?;
    let mut tx = crate::write_tx(pool).await?;
    let closed = sqlx::query(
        "SELECT * FROM transfer_invitation
         WHERE uid = ? AND status IN ('rejected', 'withdrawn', 'expired')",
    )
    .bind(&input.invitation_uid)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| sqlx::Error::Protocol("invitation is not reopenable".into()))?;
    let closed = map_invitation(closed)?;
    let creator: Option<String> = sqlx::query_scalar(
        "SELECT actor_uid FROM transfer_party
         WHERE transfer_uid = ? AND kind = 'creator' LIMIT 1",
    )
    .bind(&closed.transfer_uid)
    .fetch_optional(&mut *tx)
    .await?;
    if creator.as_deref() != Some(actor_uid) {
        return Err(sqlx::Error::Protocol(
            "only the Transfer creator may reopen an invitation".into(),
        ));
    }
    let already_party: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM transfer_party
         WHERE transfer_uid = ? AND actor_uid = ?)",
    )
    .bind(&closed.transfer_uid)
    .bind(&closed.addressed_person_uid)
    .fetch_one(&mut *tx)
    .await?;
    if already_party {
        return Err(sqlx::Error::Protocol(
            "a current transfer party cannot have an invitation reopened".into(),
        ));
    }
    let revision =
        match advance_transfer_revision(&mut tx, &closed.transfer_uid, input.expected_revision)
            .await?
        {
            Ok(revision) => revision,
            Err(current_revision) => {
                tx.rollback().await?;
                return Ok(InvitationCommit::Stale {
                    transfer_uid: closed.transfer_uid,
                    current_revision,
                });
            }
        };
    let attempt = closed
        .attempt
        .checked_add(1)
        .ok_or_else(|| sqlx::Error::Protocol("invitation attempt overflow".into()))?;
    let at = now.to_rfc3339();
    sqlx::query(
        "UPDATE transfer_invitation
         SET status = 'pending', party_uid = NULL, expires_at = ?, attempt = ?,
             invited_by_person_uid = ?, updated_at = ?
         WHERE uid = ? AND status IN ('rejected', 'withdrawn', 'expired')",
    )
    .bind(input.expires_at.map(|value| value.to_rfc3339()))
    .bind(
        i64::try_from(attempt)
            .map_err(|_| sqlx::Error::Protocol("invitation attempt exceeds SQLite range".into()))?,
    )
    .bind(actor_uid)
    .bind(&at)
    .bind(&closed.uid)
    .execute(&mut *tx)
    .await?;
    reset_agreements_for_revision(&mut tx, &closed.transfer_uid, revision, now).await?;
    let revision_fact = insert_revision_fact(
        &mut tx,
        &closed.transfer_uid,
        revision,
        input.expected_revision,
        request_id,
        "reopen-transfer-invitation",
        Some(actor_uid.into()),
        now,
        &sign,
    )
    .await?;
    let event_fact = insert_invitation_event(
        &mut tx,
        NewInvitationEvent {
            invitation_uid: &closed.uid,
            transfer_uid: &closed.transfer_uid,
            attempt,
            kind: "reopened",
            from_status: Some(closed.status.as_str()),
            to_status: "pending",
            actor_uid: Some(actor_uid),
            revision: Some(revision as u64),
            request_id,
        },
        now,
        &sign,
    )
    .await?;
    crate::records::bump_quantity(&mut tx, &closed.transfer_uid, crate::exact::zero(), &at).await?;
    let invitation = map_invitation(
        sqlx::query("SELECT * FROM transfer_invitation WHERE uid = ?")
            .bind(&closed.uid)
            .fetch_one(&mut *tx)
            .await?,
    )?;
    tx.commit().await?;
    Ok(InvitationCommit::Committed(InvitationCommitOutcome {
        transfer_uid: closed.transfer_uid,
        invitation,
        party_uid: None,
        revision: Some(revision as u64),
        revision_fact: Some(revision_fact),
        event_fact,
    }))
}

async fn close_transfer_invitation_signed<F>(
    pool: &SqlitePool,
    input: InvitationTransitionInput,
    status: TransferInvitationStatus,
    now: DateTime<Utc>,
    sign: F,
) -> Result<InvitationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    debug_assert!(matches!(
        status,
        TransferInvitationStatus::Rejected | TransferInvitationStatus::Expired
    ));
    let request_id = validate_request_key(&input.idempotency_key)?;
    if let Some(outcome) = invitation_commit_for_request(pool, request_id).await? {
        return Ok(InvitationCommit::Replayed(outcome));
    }
    ensure_request_not_used_by_transfer_revision(pool, request_id).await?;
    let mut tx = crate::write_tx(pool).await?;
    let pending =
        sqlx::query("SELECT * FROM transfer_invitation WHERE uid = ? AND status = 'pending'")
            .bind(&input.invitation_uid)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| sqlx::Error::Protocol("invitation is not pending".into()))?;
    let pending = map_invitation(pending)?;
    match status {
        TransferInvitationStatus::Rejected => {
            if input.actor_person_uid.as_deref() != Some(&pending.addressed_person_uid) {
                return Err(sqlx::Error::Protocol(
                    "only the addressed Person may reject an invitation".into(),
                ));
            }
        }
        TransferInvitationStatus::Expired => {
            if input.actor_person_uid.is_some() {
                return Err(sqlx::Error::Protocol(
                    "automatic invitation expiry must not attribute a Person actor".into(),
                ));
            }
            let due = pending
                .expires_at
                .as_deref()
                .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
                .is_some_and(|value| value.with_timezone(&Utc) <= now);
            if !due {
                return Err(sqlx::Error::Protocol(
                    "invitation is not due to expire".into(),
                ));
            }
        }
        _ => unreachable!(),
    }
    let at = now.to_rfc3339();
    let transition = sqlx::query(
        "UPDATE transfer_invitation SET status = ?, party_uid = NULL, updated_at = ?
         WHERE uid = ? AND status = 'pending'",
    )
    .bind(status.as_str())
    .bind(&at)
    .bind(&pending.uid)
    .execute(&mut *tx)
    .await?;
    if transition.rows_affected() != 1 {
        return Err(sqlx::Error::Protocol("invitation is not pending".into()));
    }
    let event_fact = insert_invitation_event(
        &mut tx,
        NewInvitationEvent {
            invitation_uid: &pending.uid,
            transfer_uid: &pending.transfer_uid,
            attempt: pending.attempt,
            kind: status.as_str(),
            from_status: Some("pending"),
            to_status: status.as_str(),
            actor_uid: input.actor_person_uid.as_deref(),
            revision: None,
            request_id,
        },
        now,
        &sign,
    )
    .await?;
    crate::records::bump_quantity(&mut tx, &pending.transfer_uid, crate::exact::zero(), &at)
        .await?;
    let invitation = map_invitation(
        sqlx::query("SELECT * FROM transfer_invitation WHERE uid = ?")
            .bind(&pending.uid)
            .fetch_one(&mut *tx)
            .await?,
    )?;
    tx.commit().await?;
    Ok(InvitationCommit::Committed(InvitationCommitOutcome {
        transfer_uid: pending.transfer_uid,
        invitation,
        party_uid: None,
        revision: None,
        revision_fact: None,
        event_fact,
    }))
}

pub async fn reject_transfer_invitation<F>(
    pool: &SqlitePool,
    input: InvitationTransitionInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<InvitationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    close_transfer_invitation_signed(pool, input, TransferInvitationStatus::Rejected, now, sign)
        .await
}

pub async fn expire_transfer_invitation<F>(
    pool: &SqlitePool,
    input: InvitationTransitionInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<InvitationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    close_transfer_invitation_signed(pool, input, TransferInvitationStatus::Expired, now, sign)
        .await
}

pub async fn due_transfer_invitations(
    pool: &SqlitePool,
    now: DateTime<Utc>,
) -> Result<Vec<TransferInvitationRow>, StoreError> {
    sqlx::query(
        "SELECT * FROM transfer_invitation
         WHERE status = 'pending' AND expires_at IS NOT NULL AND expires_at <= ?
         ORDER BY expires_at, uid",
    )
    .bind(now.to_rfc3339())
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_invitation)
    .collect()
}

async fn open_claim_for_request(
    pool: &SqlitePool,
    input: &OpenPromiseClaimInput,
    request_id: &str,
) -> Result<Option<OpenPromiseClaimOutcome>, StoreError> {
    let Some(pair) = open_claim_pair_for_request(pool, request_id).await? else {
        return Ok(None);
    };
    if pair.transfer_uid != input.transfer_uid
        || pair.source_promise_uid != input.source_promise_uid
        || pair.claimant_person_uid != input.claimant_person_uid
        || input.expected_revision.checked_add(1) != Some(pair.revision)
    {
        return Err(sqlx::Error::Protocol(
            "OPEN claim request id was already used with different targets".into(),
        ));
    }
    let fact = revision_fact(pool, &pair.transfer_uid, pair.revision)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let payload = fact
        .payload
        .as_deref()
        .ok_or_else(|| sqlx::Error::Protocol("OPEN claim revision has no signed payload".into()))?;
    let evidence: TransferRevisionEvidence =
        serde_json::from_str(payload).map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    if evidence.action != "claim-open-transfer-promise" {
        return Err(sqlx::Error::Protocol(
            "OPEN claim request id belongs to another action".into(),
        ));
    }
    let claimant = evidence
        .terms
        .promises
        .iter()
        .find(|promise| promise.uid == pair.claimant_promise_uid)
        .ok_or_else(|| {
            sqlx::Error::Protocol("signed OPEN claim lacks its claimant promise".into())
        })?;
    let proposer = evidence
        .terms
        .promises
        .iter()
        .find(|promise| promise.uid == pair.proposer_promise_uid)
        .ok_or_else(|| {
            sqlx::Error::Protocol("signed OPEN claim lacks its proposer promise".into())
        })?;
    if claimant.source_promise_uid.as_deref() != Some(pair.source_promise_uid.as_str())
        || claimant.record_uid != input.record_uid
        || claimant.concept_uid != input.concept_uid
        || claimant.unit_uid != input.unit_uid
        || claimant.person_uid.as_deref() != Some(input.claimant_person_uid.as_str())
        || claimant.delta != input.delta
        || claimant.window_start != input.window_start
        || claimant.window_end != input.window_end
        || claimant.location != input.location
        || claimant.condition != input.condition
        || claimant.reserve_from != input.reserve_from
        || proposer.record_uid.as_deref() != Some(pair.source_record_uid.as_str())
        || proposer.concept_uid != input.concept_uid
        || proposer.unit_uid != input.unit_uid
        || proposer.person_uid.as_deref() != Some(pair.proposer_person_uid.as_str())
        || proposer.delta != -input.delta
        || proposer.window_start != input.window_start
        || proposer.window_end != input.window_end
        || proposer.location != input.location
        || proposer.condition != input.condition
        || proposer.reserve_from != input.reserve_from
    {
        return Err(sqlx::Error::Protocol(
            "OPEN claim request id was replayed with different refined terms".into(),
        ));
    }
    let party_uid: String = sqlx::query_scalar(
        "SELECT uid FROM transfer_party WHERE transfer_uid = ? AND actor_uid = ?",
    )
    .bind(&input.transfer_uid)
    .bind(&input.claimant_person_uid)
    .fetch_optional(pool)
    .await?
    .ok_or(sqlx::Error::RowNotFound)?;
    Ok(Some(OpenPromiseClaimOutcome {
        transfer_uid: pair.transfer_uid,
        revision: pair.revision,
        fact,
        party_uid,
        proposer_promise_uid: pair.proposer_promise_uid,
        promise_uid: pair.claimant_promise_uid,
        source_promise_uid: pair.source_promise_uid,
        consumed: pair.reuse_policy == OpenPromiseReusePolicy::Consume,
    }))
}

pub async fn claim_open_promise<F>(
    pool: &SqlitePool,
    input: OpenPromiseClaimInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<OpenPromiseClaimCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_id = validate_request_key(&input.idempotency_key)?;
    if let Some(outcome) = open_claim_for_request(pool, &input, request_id).await? {
        return Ok(OpenPromiseClaimCommit::Replayed(outcome));
    }
    ensure_request_not_used_by_invitation_event(pool, request_id).await?;
    if input.record_uid.is_none() && input.concept_uid.is_none() {
        return Err(sqlx::Error::Protocol(
            "claimed promise requires a Record or concept".into(),
        ));
    }
    if !input.delta.is_finite() || input.delta == 0.0 {
        return Err(sqlx::Error::Protocol(
            "claimed promise delta must be finite and non-zero".into(),
        ));
    }
    validate_location(input.location.as_ref())?;
    let mut tx = crate::write_tx(pool).await?;
    let is_person: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record WHERE uid = ? AND kind = 'person')")
            .bind(&input.claimant_person_uid)
            .fetch_one(&mut *tx)
            .await?;
    if !is_person {
        return Err(sqlx::Error::Protocol(
            "OPEN claimant identity is not a Person".into(),
        ));
    }
    let pending_invitation: Option<String> = sqlx::query_scalar(
        "SELECT uid FROM transfer_invitation
         WHERE transfer_uid = ? AND addressed_person_uid = ? AND status = 'pending' LIMIT 1",
    )
    .bind(&input.transfer_uid)
    .bind(&input.claimant_person_uid)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(invitation_uid) = pending_invitation {
        return Err(sqlx::Error::Protocol(format!(
            "invitation `{invitation_uid}` must be accepted before claiming an OPEN promise"
        )));
    }
    let source = sqlx::query(
        "SELECT record_uid, party_uid, delta, open_reuse_policy FROM promise
         WHERE uid = ? AND transfer_uid = ? AND state = 'open'
           AND party_uid IS NOT NULL",
    )
    .bind(&input.source_promise_uid)
    .bind(&input.transfer_uid)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| sqlx::Error::Protocol("source promise is not OPEN".into()))?;
    let reuse_policy =
        OpenPromiseReusePolicy::parse(source.get::<String, _>("open_reuse_policy").as_str())
            .ok_or_else(|| sqlx::Error::Protocol("OPEN promise has invalid reuse policy".into()))?;
    let proposer_person_uid: String = source.get("party_uid");
    let proposer_record_uid: String = source
        .get::<Option<String>, _>("record_uid")
        .ok_or_else(|| sqlx::Error::Protocol("OPEN source requires a concrete Record".into()))?;
    let source_delta: f64 = source.get("delta");
    if !source_delta.is_finite() || source_delta == 0.0 {
        return Err(sqlx::Error::Protocol(
            "OPEN source direction must be finite and non-zero".into(),
        ));
    }
    if proposer_person_uid == input.claimant_person_uid {
        return Err(sqlx::Error::Protocol(
            "OPEN proposer cannot claim their own proposal".into(),
        ));
    }
    if source_delta.signum() == input.delta.signum() {
        return Err(sqlx::Error::Protocol(
            "OPEN claimant delta must oppose the proposal direction".into(),
        ));
    }
    let proposer_party_uid: Option<String> = sqlx::query_scalar(
        "SELECT uid FROM transfer_party WHERE transfer_uid = ? AND actor_uid = ?",
    )
    .bind(&input.transfer_uid)
    .bind(&proposer_person_uid)
    .fetch_optional(&mut *tx)
    .await?;
    if proposer_party_uid.is_none() {
        return Err(sqlx::Error::Protocol(
            "OPEN proposer is not a current Transfer party".into(),
        ));
    }
    let existing_party: Option<String> = sqlx::query_scalar(
        "SELECT uid FROM transfer_party WHERE transfer_uid = ? AND actor_uid = ?",
    )
    .bind(&input.transfer_uid)
    .bind(&input.claimant_person_uid)
    .fetch_optional(&mut *tx)
    .await?;
    if existing_party.is_none() {
        let visibility: String =
            sqlx::query_scalar("SELECT visibility FROM transfer WHERE record_uid = ?")
                .bind(&input.transfer_uid)
                .fetch_one(&mut *tx)
                .await?;
        if visibility != "public" {
            return Err(sqlx::Error::Protocol(
                "unknown Person may claim an OPEN promise only on a public Transfer".into(),
            ));
        }
    }
    let revision =
        match advance_transfer_revision(&mut tx, &input.transfer_uid, input.expected_revision)
            .await?
        {
            Ok(revision) => revision,
            Err(current_revision) => {
                tx.rollback().await?;
                return Ok(OpenPromiseClaimCommit::Stale {
                    transfer_uid: input.transfer_uid,
                    current_revision,
                });
            }
        };
    let at = now.to_rfc3339();
    let party_uid = if let Some(uid) = existing_party {
        uid
    } else {
        let uid = nucleus::new_uid("y");
        sqlx::query(
            "INSERT INTO transfer_party (uid, transfer_uid, actor_uid, kind)
             VALUES (?, ?, ?, 'participant')",
        )
        .bind(&uid)
        .bind(&input.transfer_uid)
        .bind(&input.claimant_person_uid)
        .execute(&mut *tx)
        .await?;
        uid
    };
    let location = input.location.as_ref();
    let (proposer_promise_uid, consumed) = match reuse_policy {
        OpenPromiseReusePolicy::Consume => {
            let updated = sqlx::query(
                "UPDATE promise SET record_uid = ?, concept_uid = ?, unit_uid = ?,
                    party_uid = ?, delta = ?, window_start = ?, window_end = ?,
                    location_lat = ?, location_lon = ?, location_address = ?,
                    condition = ?, reserve_from = ?, state = 'proposed', revision = ?,
                    updated_at = ?
                 WHERE uid = ? AND transfer_uid = ? AND state = 'open'
                   AND party_uid = ?",
            )
            .bind(&proposer_record_uid)
            .bind(&input.concept_uid)
            .bind(&input.unit_uid)
            .bind(&proposer_person_uid)
            .bind(-input.delta)
            .bind(&input.window_start)
            .bind(&input.window_end)
            .bind(location.and_then(|value| value.lat))
            .bind(location.and_then(|value| value.lon))
            .bind(location.and_then(|value| value.address.as_deref()))
            .bind(&input.condition)
            .bind(&input.reserve_from)
            .bind(revision)
            .bind(&at)
            .bind(&input.source_promise_uid)
            .bind(&input.transfer_uid)
            .bind(&proposer_person_uid)
            .execute(&mut *tx)
            .await?;
            if updated.rows_affected() != 1 {
                return Err(sqlx::Error::Protocol(
                    "source promise is no longer OPEN".into(),
                ));
            }
            (input.source_promise_uid.clone(), true)
        }
        OpenPromiseReusePolicy::Duplicate => {
            let uid = nucleus::new_uid("p");
            sqlx::query(
                "INSERT INTO promise
                    (uid, source_promise_uid, record_uid, concept_uid, unit_uid,
                     party_uid, delta, window_start, window_end, location_lat,
                     location_lon, location_address, condition, transfer_uid,
                     reserve_from, open_reuse_policy, state, revision, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                         'proposed', ?, ?, ?)",
            )
            .bind(&uid)
            .bind(&input.source_promise_uid)
            .bind(&proposer_record_uid)
            .bind(&input.concept_uid)
            .bind(&input.unit_uid)
            .bind(&proposer_person_uid)
            .bind(-input.delta)
            .bind(&input.window_start)
            .bind(&input.window_end)
            .bind(location.and_then(|value| value.lat))
            .bind(location.and_then(|value| value.lon))
            .bind(location.and_then(|value| value.address.as_deref()))
            .bind(&input.condition)
            .bind(&input.transfer_uid)
            .bind(&input.reserve_from)
            .bind(reuse_policy.as_str())
            .bind(revision)
            .bind(&at)
            .bind(&at)
            .execute(&mut *tx)
            .await?;
            (uid, false)
        }
    };
    let claimant_promise_uid = nucleus::new_uid("p");
    sqlx::query(
        "INSERT INTO promise
            (uid, source_promise_uid, record_uid, concept_uid, unit_uid,
             party_uid, delta, window_start, window_end, location_lat,
             location_lon, location_address, condition, transfer_uid,
             reserve_from, open_reuse_policy, state, revision, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                 'proposed', ?, ?, ?)",
    )
    .bind(&claimant_promise_uid)
    .bind(&input.source_promise_uid)
    .bind(&input.record_uid)
    .bind(&input.concept_uid)
    .bind(&input.unit_uid)
    .bind(&input.claimant_person_uid)
    .bind(input.delta)
    .bind(&input.window_start)
    .bind(&input.window_end)
    .bind(location.and_then(|value| value.lat))
    .bind(location.and_then(|value| value.lon))
    .bind(location.and_then(|value| value.address.as_deref()))
    .bind(&input.condition)
    .bind(&input.transfer_uid)
    .bind(&input.reserve_from)
    .bind(reuse_policy.as_str())
    .bind(revision)
    .bind(&at)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    reset_agreements_for_revision(&mut tx, &input.transfer_uid, revision, now).await?;
    sqlx::query(
        "INSERT INTO transfer_agreement (uid, transfer_uid, party_uid, level, at, revision)
         VALUES (?, ?, ?, 0, ?, ?)
         ON CONFLICT(transfer_uid, party_uid) DO UPDATE SET
             level = 0, at = excluded.at, revision = excluded.revision",
    )
    .bind(nucleus::new_uid("g"))
    .bind(&input.transfer_uid)
    .bind(&party_uid)
    .bind(&at)
    .bind(revision)
    .execute(&mut *tx)
    .await?;
    let fact = insert_revision_fact(
        &mut tx,
        &input.transfer_uid,
        revision,
        input.expected_revision,
        request_id,
        "claim-open-transfer-promise",
        Some(input.claimant_person_uid.clone()),
        now,
        &sign,
    )
    .await?;
    if fact.signature.is_none() && input.authorization_intent_uid.is_none() {
        return Err(sqlx::Error::Protocol(
            "OPEN claim requires a signing key or verified Action intent".into(),
        ));
    }
    if let Some(intent_uid) = input.authorization_intent_uid.as_deref() {
        crate::action_intents::link_pending_fact(&mut tx, intent_uid, &fact).await?;
    }
    let pair_uid = nucleus::new_uid("tcp");
    sqlx::query(
        "INSERT INTO transfer_open_claim_pair
            (uid, transfer_uid, source_promise_uid, source_record_uid, proposer_promise_uid,
             claimant_promise_uid, proposer_person_uid, claimant_person_uid,
             revision, reuse_policy, idempotency_key, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&pair_uid)
    .bind(&input.transfer_uid)
    .bind(&input.source_promise_uid)
    .bind(&proposer_record_uid)
    .bind(&proposer_promise_uid)
    .bind(&claimant_promise_uid)
    .bind(&proposer_person_uid)
    .bind(&input.claimant_person_uid)
    .bind(revision)
    .bind(reuse_policy.as_str())
    .bind(request_id)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    crate::records::bump_quantity(&mut tx, &input.transfer_uid, crate::exact::zero(), &at).await?;
    tx.commit().await?;
    Ok(OpenPromiseClaimCommit::Committed(OpenPromiseClaimOutcome {
        transfer_uid: input.transfer_uid,
        revision: revision as u64,
        fact,
        party_uid,
        proposer_promise_uid,
        promise_uid: claimant_promise_uid,
        source_promise_uid: input.source_promise_uid,
        consumed,
    }))
}

pub async fn create_invitation(
    pool: &SqlitePool,
    new: NewTransferInvitation<'_>,
    now: DateTime<Utc>,
) -> Result<TransferInvitationRow, StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    for (role, person_uid) in [
        ("addressed", new.addressed_person_uid),
        ("inviting", new.invited_by_person_uid),
    ] {
        let is_person = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM record WHERE uid = ? AND kind = 'person')",
        )
        .bind(person_uid)
        .fetch_one(&mut *tx)
        .await?;
        if !is_person {
            return Err(sqlx::Error::Protocol(format!(
                "{role} transfer invitation identity `{person_uid}` is not a Person"
            )));
        }
    }

    let already_party = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
            SELECT 1 FROM transfer_party WHERE transfer_uid = ? AND actor_uid = ?
         )",
    )
    .bind(new.transfer_uid)
    .bind(new.addressed_person_uid)
    .fetch_one(&mut *tx)
    .await?;
    if already_party {
        return Err(sqlx::Error::Protocol(format!(
            "Person `{}` is already a party of transfer `{}`",
            new.addressed_person_uid, new.transfer_uid
        )));
    }

    let uid = nucleus::new_uid("ti");
    let at = now.to_rfc3339();
    let expires_at = new.expires_at.map(|value| value.to_rfc3339());
    sqlx::query(
        "INSERT INTO transfer_invitation
            (uid, transfer_uid, addressed_person_uid, invited_by_person_uid,
             status, expires_at, created_at, updated_at)
         VALUES (?, ?, ?, ?, 'pending', ?, ?, ?)",
    )
    .bind(&uid)
    .bind(new.transfer_uid)
    .bind(new.addressed_person_uid)
    .bind(new.invited_by_person_uid)
    .bind(expires_at)
    .bind(&at)
    .bind(&at)
    .execute(&mut *tx)
    .await?;

    let row = sqlx::query("SELECT * FROM transfer_invitation WHERE uid = ?")
        .bind(&uid)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    map_invitation(row)
}

pub async fn accept_invitation(
    pool: &SqlitePool,
    uid: &str,
    now: DateTime<Utc>,
) -> Result<Option<AcceptedTransferInvitation>, StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    let Some(pending) =
        sqlx::query("SELECT * FROM transfer_invitation WHERE uid = ? AND status = 'pending'")
            .bind(uid)
            .fetch_optional(&mut *tx)
            .await?
    else {
        return Ok(None);
    };
    let pending = map_invitation(pending)?;

    let party_uid = nucleus::new_uid("y");
    sqlx::query("INSERT INTO transfer_party (uid, transfer_uid, actor_uid) VALUES (?, ?, ?)")
        .bind(&party_uid)
        .bind(&pending.transfer_uid)
        .bind(&pending.addressed_person_uid)
        .execute(&mut *tx)
        .await?;

    let transition = sqlx::query(
        "UPDATE transfer_invitation
         SET status = 'accepted', party_uid = ?, updated_at = ?
         WHERE uid = ? AND status = 'pending'",
    )
    .bind(&party_uid)
    .bind(now.to_rfc3339())
    .bind(uid)
    .execute(&mut *tx)
    .await?;
    if transition.rows_affected() != 1 {
        tx.rollback().await?;
        return Ok(None);
    }
    let invitation = sqlx::query("SELECT * FROM transfer_invitation WHERE uid = ?")
        .bind(uid)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;

    Ok(Some(AcceptedTransferInvitation {
        invitation: map_invitation(invitation)?,
        party_uid,
    }))
}

async fn close_pending_invitation(
    pool: &SqlitePool,
    uid: &str,
    status: TransferInvitationStatus,
    now: DateTime<Utc>,
) -> Result<Option<TransferInvitationRow>, StoreError> {
    debug_assert!(matches!(
        status,
        TransferInvitationStatus::Rejected
            | TransferInvitationStatus::Withdrawn
            | TransferInvitationStatus::Expired
    ));
    let result = sqlx::query(
        "UPDATE transfer_invitation SET status = ?, updated_at = ?
         WHERE uid = ? AND status = 'pending'",
    )
    .bind(status.as_str())
    .bind(now.to_rfc3339())
    .bind(uid)
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Ok(None);
    }
    invitation(pool, uid).await
}

pub async fn reject_invitation(
    pool: &SqlitePool,
    uid: &str,
    now: DateTime<Utc>,
) -> Result<Option<TransferInvitationRow>, StoreError> {
    close_pending_invitation(pool, uid, TransferInvitationStatus::Rejected, now).await
}

pub async fn withdraw_invitation(
    pool: &SqlitePool,
    uid: &str,
    now: DateTime<Utc>,
) -> Result<Option<TransferInvitationRow>, StoreError> {
    close_pending_invitation(pool, uid, TransferInvitationStatus::Withdrawn, now).await
}

pub async fn expire_invitation(
    pool: &SqlitePool,
    uid: &str,
    now: DateTime<Utc>,
) -> Result<Option<TransferInvitationRow>, StoreError> {
    close_pending_invitation(pool, uid, TransferInvitationStatus::Expired, now).await
}

fn map_agreement_event(row: SqliteRow) -> AgreementTransitionEventRow {
    AgreementTransitionEventRow {
        uid: row.get("uid"),
        transfer_uid: row.get("transfer_uid"),
        revision: row.get::<i64, _>("revision") as u64,
        party_uid: row.get("party_uid"),
        person_uid: row.get("person_uid"),
        from_level: row.get::<i64, _>("from_level") as u8,
        to_level: row.get::<i64, _>("to_level") as u8,
        fact_uid: row.get("fact_uid"),
        idempotency_key: row.get("idempotency_key"),
        created_at: row.get("created_at"),
    }
}

pub async fn agreement_events(
    pool: &SqlitePool,
    transfer_uid: &str,
    revision: Option<u64>,
) -> Result<Vec<AgreementTransitionEventRow>, StoreError> {
    let rows = if let Some(revision) = revision {
        let revision = i64::try_from(revision)
            .map_err(|_| sqlx::Error::Protocol("transfer revision exceeds SQLite range".into()))?;
        sqlx::query(
            "SELECT * FROM transfer_agreement_event
             WHERE transfer_uid = ? AND revision = ? ORDER BY created_at, uid",
        )
        .bind(transfer_uid)
        .bind(revision)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            "SELECT * FROM transfer_agreement_event
             WHERE transfer_uid = ? ORDER BY revision, created_at, uid",
        )
        .bind(transfer_uid)
        .fetch_all(pool)
        .await?
    };
    Ok(rows.into_iter().map(map_agreement_event).collect())
}

pub async fn agreement_event_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<AgreementTransitionEventRow>, StoreError> {
    Ok(
        sqlx::query("SELECT * FROM transfer_agreement_event WHERE idempotency_key = ?")
            .bind(request_id.trim())
            .fetch_optional(pool)
            .await?
            .map(map_agreement_event),
    )
}

pub async fn agreement_coalition(
    pool: &SqlitePool,
    transfer_uid: &str,
    revision: u64,
) -> Result<Option<AgreementCoalitionRow>, StoreError> {
    let revision = i64::try_from(revision)
        .map_err(|_| sqlx::Error::Protocol("transfer revision exceeds SQLite range".into()))?;
    let Some(row) = sqlx::query(
        "SELECT * FROM transfer_agreement_coalition
         WHERE transfer_uid = ? AND revision = ?",
    )
    .bind(transfer_uid)
    .bind(revision)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };
    let party_uids = sqlx::query_scalar(
        "SELECT party_uid FROM transfer_agreement_coalition_member
         WHERE transfer_uid = ? AND revision = ? ORDER BY party_uid",
    )
    .bind(transfer_uid)
    .bind(revision)
    .fetch_all(pool)
    .await?;
    Ok(Some(AgreementCoalitionRow {
        transfer_uid: row.get("transfer_uid"),
        revision: row.get::<i64, _>("revision") as u64,
        threshold_pct: row.get::<i64, _>("threshold_pct") as u8,
        eligible_count: row.get::<i64, _>("eligible_count") as usize,
        frozen_by_event_uid: row.get("frozen_by_event_uid"),
        frozen_at: row.get("frozen_at"),
        party_uids,
    }))
}

async fn agreement_outcome_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<AgreementTransitionOutcome>, StoreError> {
    let Some(event) = agreement_event_for_request(pool, request_id).await? else {
        return Ok(None);
    };
    let fact = crate::facts::get(pool, &event.fact_uid)
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

    let expected_revision = i64::try_from(input.expected_revision)
        .map_err(|_| sqlx::Error::Protocol("transfer revision exceeds SQLite range".into()))?;
    let mut tx = crate::write_tx(pool).await?;
    let transfer = sqlx::query(
        "SELECT revision, agreement_type, agreement_pct FROM transfer WHERE record_uid = ?",
    )
    .bind(&input.transfer_uid)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(sqlx::Error::RowNotFound)?;
    let current_revision: i64 = transfer.get("revision");
    if current_revision != expected_revision {
        tx.rollback().await?;
        return Ok(AgreementTransitionCommit::Stale {
            transfer_uid: input.transfer_uid,
            current_revision: current_revision as u64,
        });
    }
    let signed_revision_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM transfer_revision tr
             JOIN fact f ON f.uid = tr.fact_uid
             WHERE tr.transfer_uid = ? AND tr.revision = ?
               AND (f.signature IS NOT NULL OR EXISTS(
                   SELECT 1 FROM fact_action_intent fai
                   JOIN signed_action_intent sai ON sai.uid = fai.intent_uid
                   WHERE fai.fact_uid = f.uid AND sai.status = 'committed'
               ))
         )",
    )
    .bind(&input.transfer_uid)
    .bind(expected_revision)
    .fetch_one(&mut *tx)
    .await?;
    if !signed_revision_exists {
        return Err(sqlx::Error::Protocol(
            "agreement requires an existing signed Transfer revision".into(),
        ));
    }
    let party_uid: String = sqlx::query_scalar(
        "SELECT tp.uid FROM transfer_party tp
         JOIN record person ON person.uid = tp.actor_uid AND person.kind = 'person'
         WHERE tp.transfer_uid = ? AND tp.actor_uid = ?",
    )
    .bind(&input.transfer_uid)
    .bind(&input.person_uid)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| {
        sqlx::Error::Protocol("only a Person party may change their own Transfer agreement".into())
    })?;
    let from_level: i64 = sqlx::query_scalar(
        "SELECT level FROM transfer_agreement
         WHERE transfer_uid = ? AND party_uid = ? AND revision = ?",
    )
    .bind(&input.transfer_uid)
    .bind(&party_uid)
    .bind(expected_revision)
    .fetch_optional(&mut *tx)
    .await?
    .unwrap_or(0);
    let to_level = i64::from(input.to_level);
    if (to_level - from_level).abs() != 1 {
        return Err(sqlx::Error::Protocol(format!(
            "agreement must move one adjacent level from {from_level}, not to {to_level}"
        )));
    }
    if to_level > from_level {
        let coalition_frozen: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM transfer_agreement_coalition
             WHERE transfer_uid = ? AND revision = ?)",
        )
        .bind(&input.transfer_uid)
        .bind(expected_revision)
        .fetch_one(&mut *tx)
        .await?;
        if coalition_frozen {
            let member: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM transfer_agreement_coalition_member
                 WHERE transfer_uid = ? AND revision = ? AND party_uid = ?)",
            )
            .bind(&input.transfer_uid)
            .bind(expected_revision)
            .bind(&party_uid)
            .fetch_one(&mut *tx)
            .await?;
            if !member {
                return Err(sqlx::Error::Protocol(
                    "percentage agreement coalition is frozen without this party".into(),
                ));
            }
        }
    }
    let at = now.to_rfc3339();
    sqlx::query(
        "INSERT INTO transfer_agreement (uid, transfer_uid, party_uid, level, at, revision)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(transfer_uid, party_uid) DO UPDATE SET
             level = excluded.level, at = excluded.at, revision = excluded.revision",
    )
    .bind(nucleus::new_uid("g"))
    .bind(&input.transfer_uid)
    .bind(&party_uid)
    .bind(to_level)
    .bind(&at)
    .bind(expected_revision)
    .execute(&mut *tx)
    .await?;
    if from_level == 1 && to_level == 2 {
        sqlx::query(
            "UPDATE promise SET state = 'agreed', updated_at = ?
             WHERE transfer_uid = ? AND party_uid = ? AND state = 'proposed'",
        )
        .bind(&at)
        .bind(&input.transfer_uid)
        .bind(&input.person_uid)
        .execute(&mut *tx)
        .await?;
    } else if from_level == 2 && to_level == 1 {
        sqlx::query(
            "UPDATE promise SET state = 'proposed', updated_at = ?
             WHERE transfer_uid = ? AND party_uid = ? AND state = 'agreed'",
        )
        .bind(&at)
        .bind(&input.transfer_uid)
        .bind(&input.person_uid)
        .execute(&mut *tx)
        .await?;
    }

    let evidence = TransferAgreementTransitionEvidence {
        action: "set-transfer-agreement-level".into(),
        idempotency_key: request_id.into(),
        transfer_uid: input.transfer_uid.clone(),
        revision: input.expected_revision,
        party_uid: party_uid.clone(),
        person_uid: input.person_uid.clone(),
        from_level: from_level as u8,
        to_level: input.to_level,
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
    crate::facts::insert(&mut tx, &fact).await?;
    if let Some(intent_uid) = input.authorization_intent_uid.as_deref() {
        crate::action_intents::link_pending_fact(&mut tx, intent_uid, &fact).await?;
    }
    let event_uid = nucleus::new_uid("tae");
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
    .execute(&mut *tx)
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
        .execute(&mut *tx)
        .await?;
    }

    let agreement_type: String = transfer.get("agreement_type");
    if agreement_type == "percentage" && to_level == 2 {
        let pct: i64 = transfer
            .get::<Option<i64>, _>("agreement_pct")
            .unwrap_or(100);
        if !(1..=100).contains(&pct) {
            return Err(sqlx::Error::Protocol(
                "percentage agreement requires a threshold between 1 and 100".into(),
            ));
        }
        let frozen: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM transfer_agreement_coalition
             WHERE transfer_uid = ? AND revision = ?)",
        )
        .bind(&input.transfer_uid)
        .bind(expected_revision)
        .fetch_one(&mut *tx)
        .await?;
        if !frozen {
            let eligible: i64 =
                sqlx::query_scalar("SELECT count(*) FROM transfer_party WHERE transfer_uid = ?")
                    .bind(&input.transfer_uid)
                    .fetch_one(&mut *tx)
                    .await?;
            let committed: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM transfer_agreement
                 WHERE transfer_uid = ? AND revision = ? AND level = 2",
            )
            .bind(&input.transfer_uid)
            .bind(expected_revision)
            .fetch_one(&mut *tx)
            .await?;
            if eligible > 0 && committed * 100 >= eligible * pct {
                sqlx::query(
                    "INSERT INTO transfer_agreement_coalition
                        (transfer_uid, revision, threshold_pct, eligible_count,
                         frozen_by_event_uid, frozen_at)
                     VALUES (?, ?, ?, ?, ?, ?)",
                )
                .bind(&input.transfer_uid)
                .bind(expected_revision)
                .bind(pct)
                .bind(eligible)
                .bind(&event_uid)
                .bind(&at)
                .execute(&mut *tx)
                .await?;
                sqlx::query(
                    "INSERT INTO transfer_agreement_coalition_member
                        (transfer_uid, revision, party_uid)
                     SELECT transfer_uid, revision, party_uid
                     FROM transfer_agreement
                     WHERE transfer_uid = ? AND revision = ? AND level = 2",
                )
                .bind(&input.transfer_uid)
                .bind(expected_revision)
                .execute(&mut *tx)
                .await?;
            }
        }
    }
    crate::records::bump_quantity(&mut tx, &input.transfer_uid, crate::exact::zero(), &at).await?;
    tx.commit().await?;

    let outcome = agreement_outcome_for_request(pool, request_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    Ok(AgreementTransitionCommit::Committed(outcome))
}

fn map_dependency_row(row: SqliteRow) -> Result<TransferRevisionDependency, StoreError> {
    let scope: String = row.get("scope");
    let upstream_kind: String = row.get("upstream_kind");
    Ok(TransferRevisionDependency {
        uid: row.get("uid"),
        scope: TransferDependencyScope::parse(&scope)
            .ok_or_else(|| sqlx::Error::Protocol(format!("unknown dependency scope `{scope}`")))?,
        promise_uid: row.get("promise_uid"),
        upstream_kind: TransferDependencyUpstreamKind::parse(&upstream_kind).ok_or_else(|| {
            sqlx::Error::Protocol(format!(
                "unknown dependency upstream kind `{upstream_kind}`"
            ))
        })?,
        upstream_uid: row.get("upstream_uid"),
        required_state: row.get("required_state"),
    })
}

pub async fn dependencies_of(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<Vec<TransferRevisionDependency>, StoreError> {
    sqlx::query(
        "SELECT uid, scope, promise_uid, upstream_kind, upstream_uid, required_state
         FROM transfer_dependency WHERE transfer_uid = ? ORDER BY uid",
    )
    .bind(transfer_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_dependency_row)
    .collect()
}

pub async fn transfer_dependency_order(
    pool: &SqlitePool,
) -> Result<Vec<TransferDependencyNode>, StoreError> {
    let rows = sqlx::query(
        "SELECT transfer_uid, scope, promise_uid, upstream_kind, upstream_uid
         FROM transfer_dependency",
    )
    .fetch_all(pool)
    .await?;
    let edges = rows
        .into_iter()
        .map(|row| {
            let downstream = if row.get::<String, _>("scope") == "transfer" {
                TransferDependencyNode::Transfer(row.get("transfer_uid"))
            } else {
                TransferDependencyNode::Promise(row.get("promise_uid"))
            };
            let upstream_uid: String = row.get("upstream_uid");
            let upstream = if row.get::<String, _>("upstream_kind") == "transfer" {
                TransferDependencyNode::Transfer(upstream_uid)
            } else {
                TransferDependencyNode::Promise(upstream_uid)
            };
            TransferDependencyEdge {
                upstream,
                downstream,
            }
        })
        .collect::<Vec<_>>();
    nucleus::transfer::transfer_dependency_order(&edges).map_err(|cycle| {
        sqlx::Error::Protocol(format!(
            "persisted Transfer dependency cycle contains {cycle:?}"
        ))
    })
}

pub async fn dependency_readiness(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<Vec<nucleus::transfer::TransferDependencyReadiness>, StoreError> {
    let dependencies = dependencies_of(pool, transfer_uid).await?;
    let mut readiness = Vec::with_capacity(dependencies.len());
    for dependency in dependencies {
        let upstream = match dependency.upstream_kind {
            TransferDependencyUpstreamKind::Promise => {
                TransferDependencyNode::Promise(dependency.upstream_uid.clone())
            }
            TransferDependencyUpstreamKind::Transfer => {
                TransferDependencyNode::Transfer(dependency.upstream_uid.clone())
            }
        };
        let current_state = match dependency.upstream_kind {
            TransferDependencyUpstreamKind::Promise => {
                sqlx::query_scalar("SELECT state FROM promise WHERE uid = ?")
                    .bind(&dependency.upstream_uid)
                    .fetch_optional(pool)
                    .await?
            }
            TransferDependencyUpstreamKind::Transfer => {
                let states: Vec<String> = sqlx::query_scalar(
                    "SELECT DISTINCT state FROM promise
                     WHERE transfer_uid = ? AND state != 'withdrawn' ORDER BY state",
                )
                .bind(&dependency.upstream_uid)
                .fetch_all(pool)
                .await?;
                match states.as_slice() {
                    [] => None,
                    [state] => Some(state.clone()),
                    _ => Some(format!("mixed:{}", states.join(","))),
                }
            }
        };
        readiness.push(nucleus::transfer::TransferDependencyReadiness {
            dependency_uid: dependency.uid,
            upstream,
            required_state: dependency.required_state,
            current_state,
        });
    }
    Ok(nucleus::transfer::ordered_transfer_dependency_readiness(
        readiness,
    ))
}

pub async fn agreement_readiness_input(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<TransferAgreementReadinessInput, StoreError> {
    let transfer = sqlx::query(
        "SELECT revision, agreement_type, agreement_pct FROM transfer WHERE record_uid = ?",
    )
    .bind(transfer_uid)
    .fetch_optional(pool)
    .await?
    .ok_or(sqlx::Error::RowNotFound)?;
    let revision = transfer.get::<i64, _>("revision") as u64;
    let parties = sqlx::query(
        "SELECT tp.uid AS party_uid, tp.actor_uid AS person_uid,
                coalesce(ta.level, 0) AS level, coalesce(ta.revision, ?) AS revision
         FROM transfer_party tp
         LEFT JOIN transfer_agreement ta
           ON ta.transfer_uid = tp.transfer_uid AND ta.party_uid = tp.uid
              AND ta.revision = ?
         WHERE tp.transfer_uid = ? ORDER BY tp.uid",
    )
    .bind(revision as i64)
    .bind(revision as i64)
    .bind(transfer_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| AgreementPartyLevelRow {
        party_uid: row.get("party_uid"),
        person_uid: row.get("person_uid"),
        level: row.get::<i64, _>("level") as u8,
        revision: row.get::<i64, _>("revision") as u64,
    })
    .collect();
    let promises = sqlx::query(
        "SELECT uid, record_uid, concept_uid, unit_uid, party_uid, delta,
                window_start, window_end, location_lat, location_lon,
                location_address, state, revision
         FROM promise WHERE transfer_uid = ? AND state != 'withdrawn'
         ORDER BY uid",
    )
    .bind(transfer_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| AgreementPromiseReadinessRow {
        uid: row.get("uid"),
        record_uid: row.get("record_uid"),
        concept_uid: row.get("concept_uid"),
        unit_uid: row.get("unit_uid"),
        person_uid: row.get("party_uid"),
        delta: row.get("delta"),
        window_start: row.get("window_start"),
        window_end: row.get("window_end"),
        location: location_snapshot(
            row.get("location_lat"),
            row.get("location_lon"),
            row.get("location_address"),
        ),
        state: row.get("state"),
        revision: row.get::<i64, _>("revision") as u64,
    })
    .collect();
    let agreement_pct = transfer
        .get::<Option<i64>, _>("agreement_pct")
        .map(|value| {
            u8::try_from(value).map_err(|_| {
                sqlx::Error::Protocol("agreement percentage is outside the u8 range".into())
            })
        })
        .transpose()?;
    if agreement_pct.is_some_and(|value| value > 100) {
        return Err(sqlx::Error::Protocol(
            "agreement percentage must not exceed 100".into(),
        ));
    }
    Ok(TransferAgreementReadinessInput {
        transfer_uid: transfer_uid.into(),
        revision,
        agreement_type: transfer.get("agreement_type"),
        agreement_pct,
        parties,
        promises,
        dependencies: dependencies_of(pool, transfer_uid).await?,
        coalition: agreement_coalition(pool, transfer_uid, revision).await?,
    })
}

fn map_occurrence(row: SqliteRow) -> TransferOccurrenceRow {
    TransferOccurrenceRow {
        uid: row.get("uid"),
        transfer_uid: row.get("transfer_uid"),
        revision: row.get::<i64, _>("revision") as u64,
        promise_uid: row.get("promise_uid"),
        exchange_path_uid: row.get("exchange_path_uid"),
        opposite_promise_uid: row.get("opposite_promise_uid"),
        activation_event_uid: row.get("activation_event_uid"),
        activation_fact_uid: row.get("activation_fact_uid"),
        record_uid: row.get("record_uid"),
        concept_uid: row.get("concept_uid"),
        unit_uid: row.get("unit_uid"),
        quantity: row.get("quantity"),
        giver_person_uid: row.get("giver_person_uid"),
        receiver_person_uid: row.get("receiver_person_uid"),
        window_start: row.get("window_start"),
        window_end: row.get("window_end"),
        location: location_snapshot(
            row.get("location_lat"),
            row.get("location_lon"),
            row.get("location_address"),
        ),
        delivery_claimed: row.get::<i64, _>("delivery_claimed") != 0,
        receipt_claimed: row.get::<i64, _>("receipt_claimed") != 0,
        disputed: row.get::<i64, _>("disputed") != 0,
        system_disputed: row.get::<i64, _>("system_disputed") != 0,
        system_dispute_fact_uid: row.get("system_dispute_fact_uid"),
        system_disputed_at: row.get("system_disputed_at"),
        dispute_fact_uid: row.get("dispute_fact_uid"),
        disputed_at: row.get("disputed_at"),
        created_at: row.get("created_at"),
    }
}

const OCCURRENCE_SELECT: &str = "SELECT o.*,
            CASE WHEN path.primary_promise_uid = o.promise_uid
                 THEN path.opposite_promise_uid ELSE path.primary_promise_uid END
                 AS opposite_promise_uid,
            activation.fact_uid AS activation_fact_uid
     FROM transfer_occurrence o
     JOIN transfer_exchange_path path ON path.uid = o.exchange_path_uid
     JOIN transfer_activation_event activation ON activation.uid = o.activation_event_uid";

pub async fn occurrence(
    pool: &SqlitePool,
    occurrence_uid: &str,
) -> Result<Option<TransferOccurrenceRow>, StoreError> {
    let sql = format!("{OCCURRENCE_SELECT} WHERE o.uid = ?");
    Ok(sqlx::query(&sql)
        .bind(occurrence_uid)
        .fetch_optional(pool)
        .await?
        .map(map_occurrence))
}

pub async fn occurrences_of(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<Vec<TransferOccurrenceRow>, StoreError> {
    let sql = format!("{OCCURRENCE_SELECT} WHERE o.transfer_uid = ? ORDER BY o.created_at, o.uid");
    Ok(sqlx::query(&sql)
        .bind(transfer_uid)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(map_occurrence)
        .collect())
}

pub async fn occurrence_for_promise(
    pool: &SqlitePool,
    promise_uid: &str,
) -> Result<Option<TransferOccurrenceRow>, StoreError> {
    let sql = format!("{OCCURRENCE_SELECT} WHERE o.promise_uid = ?");
    Ok(sqlx::query(&sql)
        .bind(promise_uid)
        .fetch_optional(pool)
        .await?
        .map(map_occurrence))
}

pub async fn phase4_request_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<(String, String, String)>, StoreError> {
    Ok(sqlx::query(
        "SELECT kind, target_uid, fact_uid FROM transfer_phase4_request
         WHERE idempotency_key = ?",
    )
    .bind(request_id.trim())
    .fetch_optional(pool)
    .await?
    .map(|row| (row.get("kind"), row.get("target_uid"), row.get("fact_uid"))))
}

async fn activation_outcome_for_request(
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
    let revision = event.get::<i64, _>("revision") as u64;
    let actor_person_uid: String = event.get("actor_person_uid");
    let fact_uid: String = event.get("fact_uid");
    let sql = format!("{OCCURRENCE_SELECT} WHERE o.activation_event_uid = ? ORDER BY o.uid");
    let occurrences = sqlx::query(&sql)
        .bind(&event_uid)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(map_occurrence)
        .collect();
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

pub async fn occurrences_for_activation_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<OccurrenceActivationOutcome>, StoreError> {
    activation_outcome_for_request(pool, request_id).await
}

#[derive(Debug, Clone)]
struct ActivationPromiseTerms {
    uid: String,
    record_uid: Option<String>,
    concept_uid: Option<String>,
    unit_uid: Option<String>,
    person_uid: String,
    delta: f64,
    window_start: Option<String>,
    window_end: Option<String>,
    location: Option<TransferLocationSnapshot>,
    state: String,
}

fn map_activation_promise(row: SqliteRow) -> ActivationPromiseTerms {
    ActivationPromiseTerms {
        uid: row.get("uid"),
        record_uid: row.get("record_uid"),
        concept_uid: row.get("concept_uid"),
        unit_uid: row.get("unit_uid"),
        person_uid: row.get("party_uid"),
        delta: row.get("delta"),
        window_start: row.get("window_start"),
        window_end: row.get("window_end"),
        location: location_snapshot(
            row.get("location_lat"),
            row.get("location_lon"),
            row.get("location_address"),
        ),
        state: row.get("state"),
    }
}

async fn activation_promise(
    tx: &mut Transaction<'_, Sqlite>,
    transfer_uid: &str,
    promise_uid: &str,
) -> Result<ActivationPromiseTerms, StoreError> {
    sqlx::query(
        "SELECT uid, record_uid, concept_uid, unit_uid, party_uid, delta,
                window_start, window_end, location_lat, location_lon,
                location_address, state
         FROM promise WHERE uid = ? AND transfer_uid = ?",
    )
    .bind(promise_uid)
    .bind(transfer_uid)
    .fetch_optional(&mut **tx)
    .await?
    .map(map_activation_promise)
    .ok_or(sqlx::Error::RowNotFound)
}

fn promises_are_exact_opposites(
    promise: &ActivationPromiseTerms,
    opposite: &ActivationPromiseTerms,
) -> bool {
    let same_subject = match (&promise.concept_uid, &opposite.concept_uid) {
        (Some(left), Some(right)) => left == right,
        _ => promise.record_uid.is_some() && promise.record_uid == opposite.record_uid,
    };
    promise.uid != opposite.uid
        && same_subject
        && promise.unit_uid == opposite.unit_uid
        && (promise.delta + opposite.delta).abs() < 1e-9
        && promise.window_start == opposite.window_start
        && promise.window_end == opposite.window_end
        && promise.location == opposite.location
}

async fn ensure_phase4_request_unused(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<(), StoreError> {
    if phase4_request_for_request(pool, request_id)
        .await?
        .is_some()
        || revision_for_request(pool, request_id).await?.is_some()
        || invitation_event_for_request(pool, request_id)
            .await?
            .is_some()
        || agreement_event_for_request(pool, request_id)
            .await?
            .is_some()
        || phase5_request_for_request(pool, request_id)
            .await?
            .is_some()
        || phase5_correction_request_for_request(pool, request_id)
            .await?
            .is_some()
        || phase6_bulk_request_for_request(pool, request_id)
            .await?
            .is_some()
        || open_claim_pair_for_request(pool, request_id)
            .await?
            .is_some()
        || correction_link_for_request(pool, request_id)
            .await?
            .is_some()
        || promise_successor_for_request(pool, request_id)
            .await?
            .is_some()
    {
        return Err(sqlx::Error::Protocol(
            "transfer request id was already used by another action".into(),
        ));
    }
    Ok(())
}

pub async fn activate_occurrences<F>(
    pool: &SqlitePool,
    input: ActivateOccurrencesInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<OccurrenceActivationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_id = validate_request_key(&input.idempotency_key)?;
    if let Some(outcome) = activation_outcome_for_request(pool, request_id).await? {
        let requested = input
            .occurrences
            .iter()
            .map(|value| value.promise_uid.as_str())
            .collect::<HashSet<_>>();
        let replayed = outcome
            .occurrences
            .iter()
            .map(|value| value.promise_uid.as_str())
            .collect::<HashSet<_>>();
        let exact_terms = input.occurrences.iter().all(|requested| {
            outcome.occurrences.iter().any(|existing| {
                existing.promise_uid == requested.promise_uid
                    && existing.opposite_promise_uid == requested.opposite_promise_uid
                    && existing.giver_person_uid == requested.giver_person_uid
                    && existing.receiver_person_uid == requested.receiver_person_uid
            })
        });
        if outcome.transfer_uid != input.transfer_uid
            || outcome.revision != input.expected_revision
            || outcome.actor_person_uid != input.actor_person_uid
            || requested != replayed
            || input.occurrences.len() != outcome.occurrences.len()
            || !exact_terms
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
    let unique_promises = input
        .occurrences
        .iter()
        .map(|value| value.promise_uid.as_str())
        .collect::<HashSet<_>>();
    if unique_promises.len() != input.occurrences.len() {
        return Err(sqlx::Error::Protocol(
            "occurrence activation repeats a promise".into(),
        ));
    }
    ensure_phase4_request_unused(pool, request_id).await?;
    let expected_revision = i64::try_from(input.expected_revision)
        .map_err(|_| sqlx::Error::Protocol("transfer revision exceeds SQLite range".into()))?;
    let mut tx = crate::write_tx(pool).await?;
    let transfer =
        sqlx::query("SELECT revision, agreement_type FROM transfer WHERE record_uid = ?")
            .bind(&input.transfer_uid)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
    let current_revision: i64 = transfer.get("revision");
    if current_revision != expected_revision {
        tx.rollback().await?;
        return Ok(OccurrenceActivationCommit::Stale {
            transfer_uid: input.transfer_uid,
            current_revision: current_revision as u64,
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
    let signed_revision_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM transfer_revision tr
             JOIN fact f ON f.uid = tr.fact_uid
             WHERE tr.transfer_uid = ? AND tr.revision = ?
               AND (f.signature IS NOT NULL OR EXISTS(
                   SELECT 1 FROM fact_action_intent fai
                   JOIN signed_action_intent sai ON sai.uid = fai.intent_uid
                   WHERE fai.fact_uid = f.uid AND sai.status = 'committed'
               ))
         )",
    )
    .bind(&input.transfer_uid)
    .bind(expected_revision)
    .fetch_one(&mut *tx)
    .await?;
    if !signed_revision_exists {
        return Err(sqlx::Error::Protocol(
            "occurrence activation requires an existing signed Transfer revision".into(),
        ));
    }
    let revision_payload: String = sqlx::query_scalar(
        "SELECT f.payload FROM transfer_revision tr
         JOIN fact f ON f.uid = tr.fact_uid
         WHERE tr.transfer_uid = ? AND tr.revision = ?",
    )
    .bind(&input.transfer_uid)
    .bind(expected_revision)
    .fetch_one(&mut *tx)
    .await?;
    let revision_evidence: TransferRevisionEvidence = serde_json::from_str(&revision_payload)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
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
    if party.get::<i64, _>("level") != 2 {
        return Err(sqlx::Error::Protocol(
            "occurrence activation requires the acting Person's current agreement".into(),
        ));
    }
    if transfer.get::<String, _>("agreement_type") == "percentage" {
        let member: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM transfer_agreement_coalition_member
             WHERE transfer_uid = ? AND revision = ? AND party_uid = ?)",
        )
        .bind(&input.transfer_uid)
        .bind(expected_revision)
        .bind(party.get::<String, _>("uid"))
        .fetch_one(&mut *tx)
        .await?;
        if !member {
            return Err(sqlx::Error::Protocol(
                "acting Person is outside the frozen percentage coalition".into(),
            ));
        }
    }

    let at = now.to_rfc3339();
    let activation_event_uid = nucleus::new_uid("tac");
    let mut snapshots = Vec::with_capacity(input.occurrences.len());
    for requested in &input.occurrences {
        let promise =
            activation_promise(&mut tx, &input.transfer_uid, &requested.promise_uid).await?;
        if !revision_evidence
            .terms
            .promises
            .iter()
            .any(|signed| signed.uid == promise.uid && signed.state != "withdrawn")
        {
            return Err(sqlx::Error::Protocol(format!(
                "promise `{}` is not present in the signed current revision",
                promise.uid
            )));
        }
        if promise.state != PromiseState::Agreed.as_str()
            || promise.person_uid != input.actor_person_uid
        {
            return Err(sqlx::Error::Protocol(format!(
                "promise `{}` is not an agreed promise owned by the acting Person",
                promise.uid
            )));
        }
        if (promise.delta < 0.0 && requested.giver_person_uid != input.actor_person_uid)
            || (promise.delta > 0.0 && requested.receiver_person_uid != input.actor_person_uid)
            || !promise.delta.is_finite()
            || promise.delta == 0.0
        {
            return Err(sqlx::Error::Protocol(
                "occurrence direction does not match the acting Person's promise delta".into(),
            ));
        }
        let party_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM transfer_party WHERE transfer_uid = ?")
                .bind(&input.transfer_uid)
                .fetch_one(&mut *tx)
                .await?;
        let roles_exist: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM transfer_party tp
             JOIN record person ON person.uid = tp.actor_uid AND person.kind = 'person'
             WHERE tp.transfer_uid = ? AND tp.actor_uid IN (?, ?)",
        )
        .bind(&input.transfer_uid)
        .bind(&requested.giver_person_uid)
        .bind(&requested.receiver_person_uid)
        .fetch_one(&mut *tx)
        .await?;
        if requested.giver_person_uid == requested.receiver_person_uid {
            return Err(sqlx::Error::Protocol(
                "occurrence giver and receiver must be different People".into(),
            ));
        }
        if roles_exist != 2 {
            return Err(sqlx::Error::Protocol(
                "occurrence giver and receiver must be accepted Transfer parties".into(),
            ));
        }
        let agreed_roles: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM transfer_party tp
             JOIN transfer_agreement ta ON ta.transfer_uid = tp.transfer_uid
                AND ta.party_uid = tp.uid AND ta.revision = ? AND ta.level = 2
             WHERE tp.transfer_uid = ? AND tp.actor_uid IN (?, ?)",
        )
        .bind(expected_revision)
        .bind(&input.transfer_uid)
        .bind(&requested.giver_person_uid)
        .bind(&requested.receiver_person_uid)
        .fetch_one(&mut *tx)
        .await?;
        if agreed_roles != 2 {
            return Err(sqlx::Error::Protocol(
                "occurrence path requires current agreement from giver and receiver".into(),
            ));
        }

        let opposite = if let Some(uid) = requested.opposite_promise_uid.as_deref() {
            let opposite = activation_promise(&mut tx, &input.transfer_uid, uid).await?;
            if !revision_evidence
                .terms
                .promises
                .iter()
                .any(|signed| signed.uid == opposite.uid && signed.state != "withdrawn")
            {
                return Err(sqlx::Error::Protocol(format!(
                    "opposite promise `{}` is not present in the signed current revision",
                    opposite.uid
                )));
            }
            if !promises_are_exact_opposites(&promise, &opposite)
                || !matches!(
                    PromiseState::parse(&opposite.state),
                    Some(PromiseState::Agreed | PromiseState::Active)
                )
                || (promise.delta < 0.0 && opposite.person_uid != requested.receiver_person_uid)
                || (promise.delta > 0.0 && opposite.person_uid != requested.giver_person_uid)
            {
                return Err(sqlx::Error::Protocol(
                    "named opposite promise is not an exact opposite path".into(),
                ));
            }
            Some(opposite)
        } else {
            if party_count > 2 {
                return Err(sqlx::Error::Protocol(
                    "unmatched promise is ambiguous in a multi-party Transfer".into(),
                ));
            }
            None
        };
        let (primary_uid, opposite_uid) = if let Some(opposite) = opposite.as_ref() {
            if promise.uid < opposite.uid {
                (promise.uid.as_str(), Some(opposite.uid.as_str()))
            } else {
                (opposite.uid.as_str(), Some(promise.uid.as_str()))
            }
        } else {
            (promise.uid.as_str(), None)
        };
        let exchange_path_uid: String = if let Some(uid) = sqlx::query_scalar(
            "SELECT uid FROM transfer_exchange_path
             WHERE transfer_uid = ? AND revision = ? AND primary_promise_uid = ?
               AND opposite_promise_uid IS ?",
        )
        .bind(&input.transfer_uid)
        .bind(expected_revision)
        .bind(primary_uid)
        .bind(opposite_uid)
        .fetch_optional(&mut *tx)
        .await?
        {
            uid
        } else {
            let uid = nucleus::new_uid("tep");
            sqlx::query(
                "INSERT INTO transfer_exchange_path
                    (uid, transfer_uid, revision, primary_promise_uid,
                     opposite_promise_uid, created_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&uid)
            .bind(&input.transfer_uid)
            .bind(expected_revision)
            .bind(primary_uid)
            .bind(opposite_uid)
            .bind(&at)
            .execute(&mut *tx)
            .await?;
            uid
        };
        snapshots.push(TransferOccurrenceSnapshot {
            uid: nucleus::new_uid("toc"),
            promise_uid: promise.uid,
            exchange_path_uid,
            opposite_promise_uid: opposite.map(|value| value.uid),
            record_uid: promise.record_uid,
            concept_uid: promise.concept_uid,
            unit_uid: promise.unit_uid,
            quantity: promise.delta.abs(),
            giver_person_uid: requested.giver_person_uid.clone(),
            receiver_person_uid: requested.receiver_person_uid.clone(),
            window_start: promise.window_start,
            window_end: promise.window_end,
            location: promise.location,
        });
    }
    snapshots.sort_by(|left, right| left.uid.cmp(&right.uid));
    let evidence = TransferActivationEvidence {
        action: "activate-transfer-occurrence".into(),
        idempotency_key: request_id.into(),
        transfer_uid: input.transfer_uid.clone(),
        revision: input.expected_revision,
        actor_person_uid: input.actor_person_uid.clone(),
        occurrences: snapshots.clone(),
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
    for snapshot in &snapshots {
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
             WHERE uid = ? AND transfer_uid = ? AND state = 'agreed'",
        )
        .bind(&at)
        .bind(&snapshot.promise_uid)
        .bind(&input.transfer_uid)
        .execute(&mut *tx)
        .await?;
    }
    crate::records::bump_quantity(&mut tx, &input.transfer_uid, crate::exact::zero(), &at).await?;
    tx.commit().await?;
    let outcome = activation_outcome_for_request(pool, request_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    Ok(OccurrenceActivationCommit::Committed(outcome))
}

fn map_occurrence_claim_event(row: SqliteRow) -> Result<OccurrenceClaimEventRow, StoreError> {
    let role: String = row.get("role");
    Ok(OccurrenceClaimEventRow {
        uid: row.get("uid"),
        occurrence_uid: row.get("occurrence_uid"),
        role: OccurrenceClaimRole::parse(&role).ok_or_else(|| {
            sqlx::Error::Protocol(format!("unknown occurrence claim role `{role}`"))
        })?,
        asserted: row.get::<i64, _>("asserted") != 0,
        actor_person_uid: row.get("actor_person_uid"),
        fact_uid: row.get("fact_uid"),
        idempotency_key: row.get("idempotency_key"),
        created_at: row.get("created_at"),
    })
}

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

async fn occurrence_claim_outcome_for_request(
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

pub async fn occurrence_claim_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<OccurrenceClaimOutcome>, StoreError> {
    occurrence_claim_outcome_for_request(pool, request_id).await
}

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
    if let Some(outcome) = occurrence_claim_outcome_for_request(pool, request_id).await? {
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
    let transfer_uid: String = occurrence.get("transfer_uid");
    let expected_actor: String = match input.role {
        OccurrenceClaimRole::Delivery => occurrence.get("giver_person_uid"),
        OccurrenceClaimRole::Receipt => occurrence.get("receiver_person_uid"),
    };
    if expected_actor != input.actor_person_uid {
        return Err(sqlx::Error::Protocol(format!(
            "{} claim belongs only to the occurrence {}",
            input.role.as_str(),
            if input.role == OccurrenceClaimRole::Delivery {
                "giver"
            } else {
                "receiver"
            }
        )));
    }
    let evidence = TransferOccurrenceClaimEvidence {
        action: "set-transfer-occurrence-claim".into(),
        idempotency_key: request_id.into(),
        transfer_uid: transfer_uid.clone(),
        occurrence_uid: input.occurrence_uid.clone(),
        role: input.role,
        asserted: input.asserted,
        actor_person_uid: input.actor_person_uid.clone(),
    };
    let payload = serde_json::to_string(&evidence)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let previous_hash = crate::facts::last_hash(&mut tx).await?;
    let mut fact = nucleus::fact::seal(
        NewFact {
            uid: None,
            record_uid: transfer_uid.clone(),
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
            "occurrence claim requires a signing key or verified Action intent".into(),
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
         VALUES (?, 'claim', ?, ?, ?)",
    )
    .bind(request_id)
    .bind(&input.occurrence_uid)
    .bind(&fact.uid)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    let event_uid = nucleus::new_uid("tce");
    sqlx::query(
        "INSERT INTO transfer_occurrence_claim_event
            (uid, occurrence_uid, role, asserted, actor_person_uid, fact_uid,
             idempotency_key, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&event_uid)
    .bind(&input.occurrence_uid)
    .bind(input.role.as_str())
    .bind(input.asserted as i64)
    .bind(&input.actor_person_uid)
    .bind(&fact.uid)
    .bind(request_id)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    let column = match input.role {
        OccurrenceClaimRole::Delivery => "delivery_claimed",
        OccurrenceClaimRole::Receipt => "receipt_claimed",
    };
    let sql = format!("UPDATE transfer_occurrence SET {column} = ? WHERE uid = ?");
    sqlx::query(&sql)
        .bind(input.asserted as i64)
        .bind(&input.occurrence_uid)
        .execute(&mut *tx)
        .await?;
    crate::records::bump_quantity(&mut tx, &transfer_uid, crate::exact::zero(), &at).await?;
    tx.commit().await?;
    let outcome = occurrence_claim_outcome_for_request(pool, request_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    Ok(OccurrenceClaimCommit::Committed(outcome))
}

async fn bulk_occurrence_claim_outcome(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<(String, String, String, BulkOccurrenceClaimOutcome)>, StoreError> {
    let Some(row) = sqlx::query(
        "SELECT uid, actor_person_uid, review_token
         FROM transfer_phase6_bulk_request WHERE idempotency_key = ?",
    )
    .bind(request_id.trim())
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };
    let uid: String = row.get("uid");
    let fact_uids: Vec<String> = sqlx::query_scalar(
        "SELECT fact_uid FROM transfer_phase6_bulk_item
         WHERE bulk_uid = ? ORDER BY ordinal",
    )
    .bind(&uid)
    .fetch_all(pool)
    .await?;
    let mut facts = Vec::with_capacity(fact_uids.len());
    for fact_uid in fact_uids {
        facts.push(
            crate::facts::get(pool, &fact_uid)
                .await?
                .ok_or(sqlx::Error::RowNotFound)?,
        );
    }
    Ok(Some((
        row.get("actor_person_uid"),
        row.get("review_token"),
        uid.clone(),
        BulkOccurrenceClaimOutcome { uid, facts },
    )))
}

pub async fn phase6_bulk_request_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<(String, String, String)>, StoreError> {
    Ok(sqlx::query(
        "SELECT uid, actor_person_uid, review_token
         FROM transfer_phase6_bulk_request WHERE idempotency_key = ?",
    )
    .bind(request_id.trim())
    .fetch_optional(pool)
    .await?
    .map(|row| {
        (
            row.get("uid"),
            row.get("actor_person_uid"),
            row.get("review_token"),
        )
    }))
}

pub async fn complete_occurrence_claims_bulk<F>(
    pool: &SqlitePool,
    mut input: BulkOccurrenceClaimInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<BulkOccurrenceClaimCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_id = validate_request_key(&input.idempotency_key)?;
    input.items.sort_by(|left, right| {
        left.occurrence_uid
            .cmp(&right.occurrence_uid)
            .then_with(|| left.role.as_str().cmp(right.role.as_str()))
    });
    if input.items.is_empty() {
        return Ok(BulkOccurrenceClaimCommit::Rejected(vec![
            BulkOccurrenceClaimFailure {
                occurrence_uid: "*".into(),
                code: "empty_selection",
                message: "bulk completion requires at least one occurrence".into(),
            },
        ]));
    }
    let reviewed = input
        .items
        .iter()
        .map(|item| nucleus::transfer::TransferBulkClaimReviewItem {
            occurrence_uid: item.occurrence_uid.clone(),
            transfer_uid: item.transfer_uid.clone(),
            transfer_revision: item.expected_revision,
            role: item.role,
            delivery_claimed: item.expected_delivery_claimed,
            receipt_claimed: item.expected_receipt_claimed,
        })
        .collect::<Vec<_>>();
    let expected_token =
        nucleus::transfer::transfer_bulk_claim_review_token(&input.actor_person_uid, &reviewed);
    if input.review_token.trim() != expected_token {
        return Ok(BulkOccurrenceClaimCommit::Rejected(vec![
            BulkOccurrenceClaimFailure {
                occurrence_uid: "*".into(),
                code: "review_token_mismatch",
                message: "bulk review token does not match the actor and exact selection".into(),
            },
        ]));
    }
    if let Some((actor, token, _, outcome)) =
        bulk_occurrence_claim_outcome(pool, request_id).await?
    {
        if actor != input.actor_person_uid || token != expected_token {
            return Err(sqlx::Error::Protocol(
                "bulk completion request id was already used with another review".into(),
            ));
        }
        return Ok(BulkOccurrenceClaimCommit::Replayed(outcome));
    }
    ensure_phase4_request_unused(pool, request_id).await?;

    let mut failures = Vec::new();
    let mut seen = HashSet::new();
    let mut tx = crate::write_tx(pool).await?;
    for item in &input.items {
        if !seen.insert((item.occurrence_uid.as_str(), item.role)) {
            failures.push(BulkOccurrenceClaimFailure {
                occurrence_uid: item.occurrence_uid.clone(),
                code: "duplicate_item",
                message: "occurrence role appears more than once in the reviewed selection".into(),
            });
            continue;
        }
        let Some(row) = sqlx::query(
            "SELECT occurrence.transfer_uid, current_transfer.revision,
                    occurrence.giver_person_uid, occurrence.receiver_person_uid,
                    occurrence.delivery_claimed, occurrence.receipt_claimed,
                    occurrence.disputed, occurrence.system_disputed,
                    EXISTS(
                        SELECT 1 FROM transfer sibling
                        JOIN transfer_source_group_result result
                          ON result.source_uid = sibling.source_uid
                         AND result.policy = sibling.satiation
                        WHERE sibling.record_uid = occurrence.transfer_uid
                          AND result.transfer_uid != sibling.record_uid
                    ) AS satiated,
                    COALESCE((SELECT SUM(slice.canonical_quantity)
                              FROM transfer_occurrence_settlement_slice slice
                              WHERE slice.occurrence_uid = occurrence.uid), 0.0)
                        >= occurrence.quantity AS completed
             FROM transfer_occurrence occurrence
             JOIN transfer current_transfer
               ON current_transfer.record_uid = occurrence.transfer_uid
             WHERE occurrence.uid = ?",
        )
        .bind(&item.occurrence_uid)
        .fetch_optional(&mut *tx)
        .await?
        else {
            failures.push(BulkOccurrenceClaimFailure {
                occurrence_uid: item.occurrence_uid.clone(),
                code: "missing",
                message: "occurrence no longer exists".into(),
            });
            continue;
        };
        let current_transfer: String = row.get("transfer_uid");
        let current_revision = row.get::<i64, _>("revision") as u64;
        let delivery = row.get::<i64, _>("delivery_claimed") != 0;
        let receipt = row.get::<i64, _>("receipt_claimed") != 0;
        let expected_actor: String = match item.role {
            OccurrenceClaimRole::Delivery => row.get("giver_person_uid"),
            OccurrenceClaimRole::Receipt => row.get("receiver_person_uid"),
        };
        let blocker = if current_transfer != item.transfer_uid {
            Some(("transfer_changed", "occurrence belongs to another Transfer"))
        } else if current_revision != item.expected_revision {
            Some(("revision_stale", "signed Transfer revision changed"))
        } else if delivery != item.expected_delivery_claimed
            || receipt != item.expected_receipt_claimed
        {
            Some(("claim_state_stale", "occurrence claim state changed"))
        } else if expected_actor != input.actor_person_uid {
            Some(("unauthorized_role", "actor does not own the reviewed role"))
        } else if match item.role {
            OccurrenceClaimRole::Delivery => delivery,
            OccurrenceClaimRole::Receipt => receipt,
        } {
            Some((
                "already_completed",
                "actor's reviewed role is already claimed",
            ))
        } else if row.get::<i64, _>("disputed") != 0 || row.get::<i64, _>("system_disputed") != 0 {
            Some(("disputed", "occurrence is disputed"))
        } else if row.get::<i64, _>("satiated") != 0 {
            Some(("satiated", "Transfer lost its first-completes source group"))
        } else if row.get::<i64, _>("completed") != 0 {
            Some(("already_settled", "occurrence is already fully settled"))
        } else {
            None
        };
        if let Some((code, message)) = blocker {
            failures.push(BulkOccurrenceClaimFailure {
                occurrence_uid: item.occurrence_uid.clone(),
                code,
                message: message.into(),
            });
        }
    }
    if !failures.is_empty() {
        tx.rollback().await?;
        return Ok(BulkOccurrenceClaimCommit::Rejected(failures));
    }

    let bulk_uid = nucleus::new_uid("tbc");
    let at = now.to_rfc3339();
    sqlx::query(
        "INSERT INTO transfer_phase6_bulk_request
            (uid, idempotency_key, actor_person_uid, review_token, item_count, created_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&bulk_uid)
    .bind(request_id)
    .bind(&input.actor_person_uid)
    .bind(&expected_token)
    .bind(input.items.len() as i64)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    let mut facts = Vec::with_capacity(input.items.len());
    let mut previous_hash = crate::facts::last_hash(&mut tx).await?;
    for (ordinal, item) in input.items.iter().enumerate() {
        let item_request_id = format!("{request_id}/{ordinal}");
        let evidence = TransferOccurrenceClaimEvidence {
            action: "complete-transfer-occurrence-claims-bulk".into(),
            idempotency_key: item_request_id.clone(),
            transfer_uid: item.transfer_uid.clone(),
            occurrence_uid: item.occurrence_uid.clone(),
            role: item.role,
            asserted: true,
            actor_person_uid: input.actor_person_uid.clone(),
        };
        let payload = serde_json::to_string(&evidence)
            .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
        let mut fact = nucleus::fact::seal(
            NewFact {
                uid: None,
                record_uid: item.transfer_uid.clone(),
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
                "bulk occurrence completion requires a signing key or verified Action intent"
                    .into(),
            ));
        }
        crate::facts::insert(&mut tx, &fact).await?;
        if let Some(intent_uid) = input.authorization_intent_uid.as_deref() {
            crate::action_intents::link_pending_fact(&mut tx, intent_uid, &fact).await?;
        }
        sqlx::query(
            "INSERT INTO transfer_phase4_request
                (idempotency_key, kind, target_uid, fact_uid, created_at)
             VALUES (?, 'claim', ?, ?, ?)",
        )
        .bind(&item_request_id)
        .bind(&item.occurrence_uid)
        .bind(&fact.uid)
        .bind(&at)
        .execute(&mut *tx)
        .await?;
        let event_uid = nucleus::new_uid("tce");
        sqlx::query(
            "INSERT INTO transfer_occurrence_claim_event
                (uid, occurrence_uid, role, asserted, actor_person_uid, fact_uid,
                 idempotency_key, created_at)
             VALUES (?, ?, ?, 1, ?, ?, ?, ?)",
        )
        .bind(&event_uid)
        .bind(&item.occurrence_uid)
        .bind(item.role.as_str())
        .bind(&input.actor_person_uid)
        .bind(&fact.uid)
        .bind(&item_request_id)
        .bind(&at)
        .execute(&mut *tx)
        .await?;
        let column = match item.role {
            OccurrenceClaimRole::Delivery => "delivery_claimed",
            OccurrenceClaimRole::Receipt => "receipt_claimed",
        };
        let sql = format!("UPDATE transfer_occurrence SET {column} = 1 WHERE uid = ?");
        sqlx::query(&sql)
            .bind(&item.occurrence_uid)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO transfer_phase6_bulk_item
                (bulk_uid, ordinal, occurrence_uid, transfer_uid, transfer_revision,
                 role, expected_delivery_claimed, expected_receipt_claimed,
                 claim_event_uid, fact_uid)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&bulk_uid)
        .bind(ordinal as i64)
        .bind(&item.occurrence_uid)
        .bind(&item.transfer_uid)
        .bind(item.expected_revision as i64)
        .bind(item.role.as_str())
        .bind(item.expected_delivery_claimed as i64)
        .bind(item.expected_receipt_claimed as i64)
        .bind(&event_uid)
        .bind(&fact.uid)
        .execute(&mut *tx)
        .await?;
        crate::records::bump_quantity(&mut tx, &item.transfer_uid, crate::exact::zero(), &at)
            .await?;
        previous_hash = fact.hash.clone();
        facts.push(fact);
    }
    tx.commit().await?;
    Ok(BulkOccurrenceClaimCommit::Committed(
        BulkOccurrenceClaimOutcome {
            uid: bulk_uid,
            facts,
        },
    ))
}

fn map_occurrence_application_event(row: SqliteRow) -> OccurrenceApplicationEventRow {
    OccurrenceApplicationEventRow {
        uid: row.get("uid"),
        occurrence_uid: row.get("occurrence_uid"),
        receiver_person_uid: row.get("receiver_person_uid"),
        formula_hash: row.get("formula_hash"),
        version: row.get::<i64, _>("version") as u64,
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
        version: row.get::<i64, _>("version") as u64,
        latest_event_uid: row.get("latest_event_uid"),
        updated_at: row.get("updated_at"),
    }
}

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

async fn occurrence_application_outcome_for_request(
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

pub async fn occurrence_application_formula_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<OccurrenceApplicationFormulaOutcome>, StoreError> {
    occurrence_application_outcome_for_request(pool, request_id).await
}

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
    let formula_hash = nucleus::transfer::occurrence_application_formula_hash(formula);
    if let Some(outcome) = occurrence_application_outcome_for_request(pool, request_id).await? {
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
    let transfer_uid: String = occurrence.get("transfer_uid");
    let receiver_person_uid: String = occurrence.get("receiver_person_uid");
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
        transfer_uid: transfer_uid.clone(),
        occurrence_uid: input.occurrence_uid.clone(),
        receiver_person_uid: input.actor_person_uid.clone(),
        formula_hash: formula_hash.clone(),
        version: version as u64,
    };
    let payload = serde_json::to_string(&evidence)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let previous_hash = crate::facts::last_hash(&mut tx).await?;
    let mut fact = nucleus::fact::seal(
        NewFact {
            uid: None,
            record_uid: transfer_uid.clone(),
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
    crate::records::bump_quantity(&mut tx, &transfer_uid, crate::exact::zero(), &at).await?;
    tx.commit().await?;
    let outcome = occurrence_application_outcome_for_request(pool, request_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    Ok(OccurrenceApplicationFormulaCommit::Committed(outcome))
}

fn map_occurrence_settlement_slice(
    row: SqliteRow,
) -> Result<OccurrenceSettlementSliceRow, StoreError> {
    let remainder_policy: String = row.get("remainder_policy");
    Ok(OccurrenceSettlementSliceRow {
        uid: row.get("uid"),
        occurrence_uid: row.get("occurrence_uid"),
        transfer_uid: row.get("transfer_uid"),
        promise_uid: row.get("promise_uid"),
        owner_person_uid: row.get("owner_person_uid"),
        canonical_quantity: row.get("canonical_quantity"),
        canonical_unit_uid: row.get("canonical_unit_uid"),
        cumulative_before: row.get("cumulative_before"),
        cumulative_after: row.get("cumulative_after"),
        remaining_after: row.get("remaining_after"),
        evidence_fact_uid: row.get("evidence_fact_uid"),
        application_fact_uid: row.get("application_fact_uid"),
        local_record_uid: row.get("local_record_uid"),
        local_delta: row.get("local_delta"),
        local_cumulative_before: row.get("local_cumulative_before"),
        local_cumulative_after: row.get("local_cumulative_after"),
        application_formula: row.get("application_formula"),
        application_formula_hash: row.get("application_formula_hash"),
        application_formula_version: row.get::<i64, _>("application_formula_version") as u64,
        remainder_policy: TransferRemainderPolicy::parse(&remainder_policy).ok_or_else(|| {
            sqlx::Error::Protocol(format!(
                "unknown Transfer remainder policy `{remainder_policy}`"
            ))
        })?,
        idempotency_key: row.get("idempotency_key"),
        created_at: row.get("created_at"),
    })
}

pub async fn occurrence_settlement_slices(
    pool: &SqlitePool,
    occurrence_uid: &str,
) -> Result<Vec<OccurrenceSettlementSliceRow>, StoreError> {
    sqlx::query(
        "SELECT * FROM transfer_occurrence_settlement_slice
         WHERE occurrence_uid = ? ORDER BY created_at, uid",
    )
    .bind(occurrence_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_occurrence_settlement_slice)
    .collect()
}

pub async fn occurrence_settlement_slice(
    pool: &SqlitePool,
    settlement_uid: &str,
) -> Result<Option<OccurrenceSettlementSliceRow>, StoreError> {
    sqlx::query("SELECT * FROM transfer_occurrence_settlement_slice WHERE uid = ?")
        .bind(settlement_uid)
        .fetch_optional(pool)
        .await?
        .map(map_occurrence_settlement_slice)
        .transpose()
}

pub async fn occurrence_settlement_slice_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<OccurrenceSettlementSliceRow>, StoreError> {
    sqlx::query("SELECT * FROM transfer_occurrence_settlement_slice WHERE idempotency_key = ?")
        .bind(request_id.trim())
        .fetch_optional(pool)
        .await?
        .map(map_occurrence_settlement_slice)
        .transpose()
}

pub async fn phase5_request_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<(String, String, String)>, StoreError> {
    Ok(sqlx::query(
        "SELECT kind, target_uid, fact_uid FROM transfer_phase5_request
         WHERE idempotency_key = ?",
    )
    .bind(request_id.trim())
    .fetch_optional(pool)
    .await?
    .map(|row| (row.get("kind"), row.get("target_uid"), row.get("fact_uid"))))
}

pub async fn phase5_correction_request_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<(String, String, String)>, StoreError> {
    Ok(sqlx::query(
        "SELECT kind, target_uid, fact_uid FROM transfer_phase5_correction_request
         WHERE idempotency_key = ?",
    )
    .bind(request_id.trim())
    .fetch_optional(pool)
    .await?
    .map(|row| (row.get("kind"), row.get("target_uid"), row.get("fact_uid"))))
}

pub async fn occurrence_settlement_progress(
    pool: &SqlitePool,
    occurrence_uid: &str,
) -> Result<Option<OccurrenceSettlementProgress>, StoreError> {
    let Some(canonical_quantity) =
        sqlx::query_scalar::<_, f64>("SELECT quantity FROM transfer_occurrence WHERE uid = ?")
            .bind(occurrence_uid)
            .fetch_optional(pool)
            .await?
    else {
        return Ok(None);
    };
    let slices = occurrence_settlement_slices(pool, occurrence_uid).await?;
    let local_settled = slices.last().map_or(0.0, |slice| slice.cumulative_after);
    let remote_settled: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(d.canonical_quantity), 0.0)
         FROM transfer_application_handoff h
         JOIN transfer_application_handoff_detail d ON d.handoff_uid = h.uid
         WHERE h.occurrence_uid = ? AND h.state = 'accepted'",
    )
    .bind(occurrence_uid)
    .fetch_one(pool)
    .await?;
    let settled_quantity = local_settled + remote_settled;
    let remaining_quantity = (canonical_quantity - settled_quantity).max(0.0);
    Ok(Some(OccurrenceSettlementProgress {
        occurrence_uid: occurrence_uid.into(),
        canonical_quantity,
        settled_quantity,
        remaining_quantity,
        partially_settled: settled_quantity > 0.0 && remaining_quantity > 0.0,
        settled: settled_quantity > 0.0 && remaining_quantity == 0.0,
        slices,
    }))
}

async fn occurrence_settlement_outcome_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<OccurrenceSettlementOutcome>, StoreError> {
    let Some(slice) = occurrence_settlement_slice_for_request(pool, request_id).await? else {
        return Ok(None);
    };
    let evidence_fact = crate::facts::get(pool, &slice.evidence_fact_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let application_fact = crate::facts::get(pool, &slice.application_fact_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let progress = occurrence_settlement_progress(pool, &slice.occurrence_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let mut satiation_facts = Vec::new();
    if let Some(result) =
        sqlx::query("SELECT fact_uid FROM transfer_source_group_result WHERE settlement_uid = ?")
            .bind(&slice.uid)
            .fetch_optional(pool)
            .await?
    {
        let result_fact_uid: String = result.get("fact_uid");
        satiation_facts.push(
            crate::facts::get(pool, &result_fact_uid)
                .await?
                .ok_or(sqlx::Error::RowNotFound)?,
        );
        let loser_fact_uids: Vec<String> = sqlx::query_scalar(
            "SELECT loser.fact_uid FROM transfer_source_group_loser loser
             JOIN transfer_source_group_result result ON result.uid = loser.result_uid
             WHERE result.settlement_uid = ? ORDER BY loser.transfer_uid",
        )
        .bind(&slice.uid)
        .fetch_all(pool)
        .await?;
        for fact_uid in loser_fact_uids {
            satiation_facts.push(
                crate::facts::get(pool, &fact_uid)
                    .await?
                    .ok_or(sqlx::Error::RowNotFound)?,
            );
        }
    }
    Ok(Some(OccurrenceSettlementOutcome {
        transfer_uid: slice.transfer_uid.clone(),
        slice,
        evidence_fact,
        application_fact,
        progress,
        satiation_facts,
    }))
}

pub async fn occurrence_settlement_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<OccurrenceSettlementOutcome>, StoreError> {
    occurrence_settlement_outcome_for_request(pool, request_id).await
}

fn settlement_values_match(left: f64, right: f64) -> bool {
    let scale = left.abs().max(right.abs()).max(1.0);
    (left - right).abs() <= scale * 1e-9
}

pub async fn effective_occurrence_remainder_policy(
    pool: &SqlitePool,
    occurrence_uid: &str,
    owner_person_uid: &str,
) -> Result<TransferRemainderPolicy, StoreError> {
    if let Some(policy) = sqlx::query_scalar::<_, String>(
        "SELECT policy FROM transfer_occurrence_remainder_policy
         WHERE occurrence_uid = ? AND owner_person_uid = ?",
    )
    .bind(occurrence_uid)
    .bind(owner_person_uid)
    .fetch_optional(pool)
    .await?
    {
        return TransferRemainderPolicy::parse(&policy).ok_or_else(|| {
            sqlx::Error::Protocol(format!("unknown Transfer remainder policy `{policy}`"))
        });
    }
    crate::config::ensure_default(pool).await?;
    let policy: String =
        sqlx::query_scalar("SELECT transfer_remainder_policy FROM configuration WHERE id = 1")
            .fetch_one(pool)
            .await?;
    TransferRemainderPolicy::parse(&policy).ok_or_else(|| {
        sqlx::Error::Protocol(format!("unknown Transfer remainder policy `{policy}`"))
    })
}

pub async fn set_occurrence_remainder_policy(
    pool: &SqlitePool,
    occurrence_uid: &str,
    owner_person_uid: &str,
    policy: TransferRemainderPolicy,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    let source_owner: Option<String> = sqlx::query_scalar(
        "SELECT source.party_uid
         FROM transfer_occurrence occurrence
         JOIN promise source ON source.uid = occurrence.promise_uid
         WHERE occurrence.uid = ?",
    )
    .bind(occurrence_uid)
    .fetch_optional(pool)
    .await?;
    if source_owner.as_deref() != Some(owner_person_uid) {
        return Err(sqlx::Error::Protocol(
            "only the source-promise owner may set the occurrence remainder policy".into(),
        ));
    }
    sqlx::query(
        "INSERT INTO transfer_occurrence_remainder_policy
            (occurrence_uid, owner_person_uid, policy, updated_at)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(occurrence_uid) DO UPDATE SET
            owner_person_uid = excluded.owner_person_uid,
            policy = excluded.policy,
            updated_at = excluded.updated_at",
    )
    .bind(occurrence_uid)
    .bind(owner_person_uid)
    .bind(policy.as_str())
    .bind(now.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

async fn record_first_completes_result<F>(
    tx: &mut Transaction<'_, Sqlite>,
    transfer_uid: &str,
    settlement_uid: &str,
    actor_person_uid: &str,
    authorization_intent_uid: Option<&str>,
    now: DateTime<Utc>,
    sign: &F,
) -> Result<(), StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let Some(group) =
        sqlx::query("SELECT source_uid, satiation, revision FROM transfer WHERE record_uid = ?")
            .bind(transfer_uid)
            .fetch_optional(&mut **tx)
            .await?
    else {
        return Err(sqlx::Error::RowNotFound);
    };
    let source_uid: Option<String> = group.get("source_uid");
    let policy: Option<String> = group.get("satiation");
    if policy.as_deref() != Some("first_completes") || source_uid.is_none() {
        return Ok(());
    }
    let source_uid = source_uid.expect("checked source group");
    let fully_settled: bool = sqlx::query_scalar(
        "SELECT EXISTS(
                 SELECT 1 FROM promise
                 WHERE transfer_uid = ? AND state NOT IN ('open', 'withdrawn')
             )
             AND NOT EXISTS(
                 SELECT 1 FROM promise promise
                 LEFT JOIN transfer_occurrence occurrence
                   ON occurrence.promise_uid = promise.uid
                 WHERE promise.transfer_uid = ?
                   AND promise.state NOT IN ('open', 'withdrawn')
                   AND (occurrence.uid IS NULL OR COALESCE((
                       SELECT SUM(slice.canonical_quantity)
                       FROM transfer_occurrence_settlement_slice slice
                       WHERE slice.occurrence_uid = occurrence.uid
                   ), 0.0) < occurrence.quantity)
             )",
    )
    .bind(transfer_uid)
    .bind(transfer_uid)
    .fetch_one(&mut **tx)
    .await?;
    if !fully_settled {
        return Ok(());
    }
    let already_decided: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM transfer_source_group_result
         WHERE source_uid = ? AND policy = 'first_completes')",
    )
    .bind(&source_uid)
    .fetch_one(&mut **tx)
    .await?;
    if already_decided {
        return Ok(());
    }

    let winner_revision = group.get::<i64, _>("revision") as u64;
    let result_uid = nucleus::new_uid("tsg");
    let evidence = TransferSourceGroupResultEvidence {
        action: "complete-transfer-source-group".into(),
        source_uid: source_uid.clone(),
        policy: "first_completes".into(),
        winner_transfer_uid: transfer_uid.into(),
        winner_revision,
        settlement_uid: settlement_uid.into(),
        observed_by_person_uid: actor_person_uid.into(),
    };
    let payload = serde_json::to_string(&evidence)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let mut previous_hash = crate::facts::last_hash(tx).await?;
    let mut result_fact = nucleus::fact::seal(
        NewFact {
            uid: None,
            record_uid: transfer_uid.into(),
            delta: crate::exact::zero(),
            at: None,
            actor_uid: Some(actor_person_uid.into()),
            cause: Cause::settlement(settlement_uid),
            payload: Some(payload),
        },
        &previous_hash,
        now,
    );
    result_fact.signature = sign(&result_fact.hash);
    if result_fact.signature.is_none() && authorization_intent_uid.is_none() {
        return Err(sqlx::Error::Protocol(
            "source-group result requires a signing key or verified Action intent".into(),
        ));
    }
    crate::facts::insert(tx, &result_fact).await?;
    if let Some(intent_uid) = authorization_intent_uid {
        crate::action_intents::link_pending_fact(tx, intent_uid, &result_fact).await?;
    }
    let at = now.to_rfc3339();
    sqlx::query(
        "INSERT INTO transfer_source_group_result
            (uid, source_uid, policy, transfer_uid, transfer_revision,
             settlement_uid, fact_uid, created_at)
         VALUES (?, ?, 'first_completes', ?, ?, ?, ?, ?)",
    )
    .bind(&result_uid)
    .bind(&source_uid)
    .bind(transfer_uid)
    .bind(winner_revision as i64)
    .bind(settlement_uid)
    .bind(&result_fact.uid)
    .bind(&at)
    .execute(&mut **tx)
    .await?;
    previous_hash = result_fact.hash;

    let losers = sqlx::query(
        "SELECT record_uid, revision FROM transfer
         WHERE source_uid = ? AND satiation = 'first_completes' AND record_uid != ?
         ORDER BY record_uid",
    )
    .bind(&source_uid)
    .bind(transfer_uid)
    .fetch_all(&mut **tx)
    .await?;
    for loser in losers {
        let losing_transfer_uid: String = loser.get("record_uid");
        let losing_revision = loser.get::<i64, _>("revision") as u64;
        let loser_uid = nucleus::new_uid("tsl");
        let evidence = TransferSourceGroupLoserEvidence {
            action: "satiate-transfer-source-group-sibling".into(),
            source_uid: source_uid.clone(),
            policy: "first_completes".into(),
            winner_transfer_uid: transfer_uid.into(),
            losing_transfer_uid: losing_transfer_uid.clone(),
            losing_revision,
            result_uid: result_uid.clone(),
            observed_by_person_uid: actor_person_uid.into(),
        };
        let payload = serde_json::to_string(&evidence)
            .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
        let mut fact = nucleus::fact::seal(
            NewFact {
                uid: None,
                record_uid: losing_transfer_uid.clone(),
                delta: crate::exact::zero(),
                at: None,
                actor_uid: Some(actor_person_uid.into()),
                cause: Cause::settlement(settlement_uid),
                payload: Some(payload),
            },
            &previous_hash,
            now,
        );
        fact.signature = sign(&fact.hash);
        if fact.signature.is_none() && authorization_intent_uid.is_none() {
            return Err(sqlx::Error::Protocol(
                "source-group loser evidence requires a signing key or verified Action intent"
                    .into(),
            ));
        }
        crate::facts::insert(tx, &fact).await?;
        if let Some(intent_uid) = authorization_intent_uid {
            crate::action_intents::link_pending_fact(tx, intent_uid, &fact).await?;
        }
        sqlx::query(
            "INSERT INTO transfer_source_group_loser
                (uid, result_uid, transfer_uid, transfer_revision, fact_uid, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(loser_uid)
        .bind(&result_uid)
        .bind(&losing_transfer_uid)
        .bind(losing_revision as i64)
        .bind(&fact.uid)
        .bind(&at)
        .execute(&mut **tx)
        .await?;
        previous_hash = fact.hash;
    }
    Ok(())
}

pub async fn settle_occurrence<F>(
    pool: &SqlitePool,
    input: OccurrenceSettlementInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<OccurrenceSettlementCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_id = validate_request_key(&input.idempotency_key)?;
    let formula = input.application_formula.trim();
    if formula.is_empty() || formula.chars().count() > 2_000 {
        return Err(sqlx::Error::Protocol(
            "application formula must contain 1 to 2000 characters".into(),
        ));
    }
    if !input.canonical_quantity.is_finite() || input.canonical_quantity <= 0.0 {
        return Err(sqlx::Error::Protocol(
            "settlement canonical quantity must be finite and positive".into(),
        ));
    }
    if !input.local_delta.is_finite() || !input.local_cumulative_after.is_finite() {
        return Err(sqlx::Error::Protocol(
            "settlement local quantities must be finite".into(),
        ));
    }
    let formula_version = i64::try_from(input.application_formula_version).map_err(|_| {
        sqlx::Error::Protocol("application formula version exceeds SQLite range".into())
    })?;
    let formula_hash = nucleus::transfer::occurrence_application_formula_hash(formula);
    if let Some(outcome) = occurrence_settlement_outcome_for_request(pool, request_id).await? {
        let slice = &outcome.slice;
        if slice.occurrence_uid != input.occurrence_uid
            || slice.owner_person_uid != input.actor_person_uid
            || !settlement_values_match(slice.canonical_quantity, input.canonical_quantity)
            || slice.local_record_uid != input.local_record_uid
            || !settlement_values_match(slice.local_delta, input.local_delta)
            || !settlement_values_match(slice.local_cumulative_after, input.local_cumulative_after)
            || slice.application_formula_hash != formula_hash
            || slice.application_formula != formula
            || slice.application_formula_version != input.application_formula_version
            || slice.remainder_policy != input.remainder_policy
        {
            return Err(sqlx::Error::Protocol(
                "settlement request id was already used with different preview values".into(),
            ));
        }
        return Ok(OccurrenceSettlementCommit::Replayed(outcome));
    }
    if phase5_request_for_request(pool, request_id)
        .await?
        .is_some()
    {
        return Err(sqlx::Error::Protocol(
            "settlement request id exists without its immutable slice".into(),
        ));
    }
    ensure_phase4_request_unused(pool, request_id).await?;

    let local_organ_uid = crate::organs::local(pool).await?.map(|organ| organ.uid);
    let mut tx = crate::write_tx(pool).await?;
    let occurrence = sqlx::query(
        "SELECT occurrence.transfer_uid, occurrence.promise_uid,
                occurrence.quantity, occurrence.unit_uid, occurrence.record_uid,
                occurrence.delivery_claimed, occurrence.receipt_claimed,
                occurrence.disputed, source.party_uid, source.state,
                local_record.organ_uid, local_record.deleted_at
         FROM transfer_occurrence occurrence
         JOIN promise source ON source.uid = occurrence.promise_uid
         LEFT JOIN record local_record ON local_record.uid = occurrence.record_uid
         WHERE occurrence.uid = ?",
    )
    .bind(&input.occurrence_uid)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(sqlx::Error::RowNotFound)?;
    let transfer_uid: String = occurrence.get("transfer_uid");
    let promise_uid: String = occurrence.get("promise_uid");
    let canonical_total: f64 = occurrence.get("quantity");
    let canonical_unit_uid: Option<String> = occurrence.get("unit_uid");
    let source_record_uid: Option<String> = occurrence.get("record_uid");
    let source_owner: String = occurrence.get("party_uid");
    let promise_state: String = occurrence.get("state");
    let record_organ_uid: Option<String> = occurrence.get("organ_uid");
    let record_deleted_at: Option<String> = occurrence.get("deleted_at");

    if source_record_uid.as_deref() != Some(input.local_record_uid.as_str()) {
        return Err(sqlx::Error::Protocol(
            "settlement requires the concrete Record signed into the source promise".into(),
        ));
    }
    if source_owner != input.actor_person_uid {
        return Err(sqlx::Error::Protocol(
            "only the source-promise owner may settle this occurrence".into(),
        ));
    }
    if promise_state != PromiseState::Active.as_str() {
        return Err(sqlx::Error::Protocol(
            "only an active source promise may be settled".into(),
        ));
    }
    if occurrence.get::<i64, _>("delivery_claimed") == 0
        || occurrence.get::<i64, _>("receipt_claimed") == 0
    {
        return Err(sqlx::Error::Protocol(
            "settlement requires current delivery and receipt confirmation".into(),
        ));
    }
    if occurrence.get::<i64, _>("disputed") != 0 {
        return Err(sqlx::Error::Protocol(
            "a disputed occurrence cannot be settled".into(),
        ));
    }
    if record_deleted_at.is_some()
        || record_organ_uid
            .as_deref()
            .is_some_and(|origin| Some(origin) != local_organ_uid.as_deref())
    {
        return Err(sqlx::Error::Protocol(
            "settlement may mutate only a live Record originating in this Cell".into(),
        ));
    }

    if input.application_formula_version > 0 {
        let policy = sqlx::query(
            "SELECT formula, formula_hash, version
             FROM transfer_occurrence_application_policy WHERE occurrence_uid = ?",
        )
        .bind(&input.occurrence_uid)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| {
            sqlx::Error::Protocol(
                "versioned settlement formula has no occurrence application policy".into(),
            )
        })?;
        if policy.get::<String, _>("formula") != formula
            || policy.get::<String, _>("formula_hash") != formula_hash
            || policy.get::<i64, _>("version") != formula_version
        {
            return Err(sqlx::Error::Protocol(
                "settlement preview formula is no longer current".into(),
            ));
        }
    }

    let progress = sqlx::query(
        "SELECT COALESCE(SUM(canonical_quantity), 0.0) AS canonical_sum,
                COALESCE(SUM(local_delta), 0.0) AS local_sum
         FROM transfer_occurrence_settlement_slice WHERE occurrence_uid = ?",
    )
    .bind(&input.occurrence_uid)
    .fetch_one(&mut *tx)
    .await?;
    let cumulative_before: f64 = progress.get("canonical_sum");
    let local_cumulative_before: f64 = progress.get("local_sum");
    let remaining_before = canonical_total - cumulative_before;
    if remaining_before <= 0.0 || input.canonical_quantity > remaining_before {
        return Err(sqlx::Error::Protocol(
            "settlement slice exceeds the occurrence's remaining quantity".into(),
        ));
    }
    let cumulative_after = cumulative_before + input.canonical_quantity;
    let remaining_after = canonical_total - cumulative_after;
    let expected_local_after = local_cumulative_before + input.local_delta;
    if !settlement_values_match(expected_local_after, input.local_cumulative_after) {
        return Err(sqlx::Error::Protocol(
            "settlement local delta does not match the reviewed cumulative formula result".into(),
        ));
    }

    let settlement_uid = nucleus::new_uid("tss");
    let public_evidence = TransferOccurrenceSettlementEvidence {
        action: "settle-transfer-occurrence".into(),
        idempotency_key: request_id.into(),
        settlement_uid: settlement_uid.clone(),
        transfer_uid: transfer_uid.clone(),
        occurrence_uid: input.occurrence_uid.clone(),
        promise_uid: promise_uid.clone(),
        owner_person_uid: input.actor_person_uid.clone(),
        canonical_quantity: input.canonical_quantity,
        canonical_unit_uid: canonical_unit_uid.clone(),
        cumulative_before,
        cumulative_after,
        remaining_after,
        application_formula_hash: formula_hash.clone(),
        application_formula_version: input.application_formula_version,
        remainder_policy: input.remainder_policy,
    };
    let public_payload = serde_json::to_string(&public_evidence)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let previous_hash = crate::facts::last_hash(&mut tx).await?;
    let mut evidence_fact = nucleus::fact::seal(
        NewFact {
            uid: None,
            record_uid: transfer_uid.clone(),
            delta: crate::exact::zero(),
            at: None,
            actor_uid: Some(input.actor_person_uid.clone()),
            cause: Cause::settlement(settlement_uid.clone()),
            payload: Some(public_payload),
        },
        &previous_hash,
        now,
    );
    evidence_fact.signature = sign(&evidence_fact.hash);
    if evidence_fact.signature.is_none() && input.authorization_intent_uid.is_none() {
        return Err(sqlx::Error::Protocol(
            "occurrence settlement requires a signing key or verified Action intent".into(),
        ));
    }
    crate::facts::insert(&mut tx, &evidence_fact).await?;
    if let Some(intent_uid) = input.authorization_intent_uid.as_deref() {
        crate::action_intents::link_pending_fact(&mut tx, intent_uid, &evidence_fact).await?;
    }

    let application_evidence = TransferOccurrenceSettlementApplicationEvidence {
        action: "apply-transfer-occurrence-settlement".into(),
        settlement_uid: settlement_uid.clone(),
        transfer_uid: transfer_uid.clone(),
        occurrence_uid: input.occurrence_uid.clone(),
        promise_uid: promise_uid.clone(),
        owner_person_uid: input.actor_person_uid.clone(),
        evidence_fact_uid: evidence_fact.uid.clone(),
        application_formula_hash: formula_hash.clone(),
        application_formula_version: input.application_formula_version,
        local_cumulative_before,
        local_cumulative_after: input.local_cumulative_after,
    };
    let application_payload = serde_json::to_string(&application_evidence)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let mut application_fact = nucleus::fact::seal(
        NewFact {
            uid: None,
            record_uid: input.local_record_uid.clone(),
            delta: crate::exact::from_f64(input.local_delta),
            at: None,
            actor_uid: Some(input.actor_person_uid.clone()),
            cause: Cause::settlement(settlement_uid.clone()),
            payload: Some(application_payload),
        },
        &evidence_fact.hash,
        now,
    );
    application_fact.signature = sign(&application_fact.hash);
    if application_fact.signature.is_none() && input.authorization_intent_uid.is_none() {
        return Err(sqlx::Error::Protocol(
            "settlement application requires a signing key or verified Action intent".into(),
        ));
    }
    crate::facts::insert(&mut tx, &application_fact).await?;
    if let Some(intent_uid) = input.authorization_intent_uid.as_deref() {
        crate::action_intents::link_pending_fact(&mut tx, intent_uid, &application_fact).await?;
    }

    let at = now.to_rfc3339();
    sqlx::query(
        "INSERT INTO transfer_phase5_request
            (idempotency_key, kind, target_uid, fact_uid, created_at)
         VALUES (?, 'settlement', ?, ?, ?)",
    )
    .bind(request_id)
    .bind(&input.occurrence_uid)
    .bind(&evidence_fact.uid)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO transfer_occurrence_settlement_slice
            (uid, occurrence_uid, transfer_uid, promise_uid, owner_person_uid,
             canonical_quantity, canonical_unit_uid, cumulative_before,
             cumulative_after, remaining_after, evidence_fact_uid,
             application_fact_uid, local_record_uid, local_delta,
             local_cumulative_before, local_cumulative_after,
             application_formula, application_formula_hash,
             application_formula_version, remainder_policy, idempotency_key,
             created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&settlement_uid)
    .bind(&input.occurrence_uid)
    .bind(&transfer_uid)
    .bind(&promise_uid)
    .bind(&input.actor_person_uid)
    .bind(input.canonical_quantity)
    .bind(&canonical_unit_uid)
    .bind(cumulative_before)
    .bind(cumulative_after)
    .bind(remaining_after)
    .bind(&evidence_fact.uid)
    .bind(&application_fact.uid)
    .bind(&input.local_record_uid)
    .bind(input.local_delta)
    .bind(local_cumulative_before)
    .bind(input.local_cumulative_after)
    .bind(formula)
    .bind(&formula_hash)
    .bind(formula_version)
    .bind(input.remainder_policy.as_str())
    .bind(request_id)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    record_first_completes_result(
        &mut tx,
        &transfer_uid,
        &settlement_uid,
        &input.actor_person_uid,
        input.authorization_intent_uid.as_deref(),
        now,
        &sign,
    )
    .await?;
    if remaining_after == 0.0 {
        sqlx::query("UPDATE promise SET state = 'kept', updated_at = ? WHERE uid = ?")
            .bind(&at)
            .bind(&promise_uid)
            .execute(&mut *tx)
            .await?;
    }
    crate::records::bump_quantity(
        &mut tx,
        &input.local_record_uid,
        crate::exact::from_f64(input.local_delta),
        &at,
    )
    .await?;
    crate::records::bump_quantity(&mut tx, &transfer_uid, crate::exact::zero(), &at).await?;
    tx.commit().await?;

    let outcome = occurrence_settlement_outcome_for_request(pool, request_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    Ok(OccurrenceSettlementCommit::Committed(outcome))
}

fn map_occurrence_settlement_compensation(row: SqliteRow) -> OccurrenceSettlementCompensationRow {
    OccurrenceSettlementCompensationRow {
        uid: row.get("uid"),
        settlement_uid: row.get("settlement_uid"),
        occurrence_uid: row.get("occurrence_uid"),
        transfer_uid: row.get("transfer_uid"),
        owner_person_uid: row.get("owner_person_uid"),
        original_application_fact_uid: row.get("original_application_fact_uid"),
        compensation_fact_uid: row.get("compensation_fact_uid"),
        local_record_uid: row.get("local_record_uid"),
        inverse_delta: row.get("inverse_delta"),
        idempotency_key: row.get("idempotency_key"),
        created_at: row.get("created_at"),
    }
}

pub async fn occurrence_settlement_for_application_fact(
    pool: &SqlitePool,
    fact_uid: &str,
) -> Result<Option<OccurrenceSettlementSliceRow>, StoreError> {
    sqlx::query(
        "SELECT * FROM transfer_occurrence_settlement_slice
         WHERE application_fact_uid = ?",
    )
    .bind(fact_uid)
    .fetch_optional(pool)
    .await?
    .map(map_occurrence_settlement_slice)
    .transpose()
}

pub async fn occurrence_settlement_compensation_for_settlement(
    pool: &SqlitePool,
    settlement_uid: &str,
) -> Result<Option<OccurrenceSettlementCompensationRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM transfer_occurrence_settlement_compensation
         WHERE settlement_uid = ?",
    )
    .bind(settlement_uid)
    .fetch_optional(pool)
    .await?
    .map(map_occurrence_settlement_compensation))
}

pub async fn occurrence_settlement_compensation_for_fact(
    pool: &SqlitePool,
    fact_uid: &str,
) -> Result<Option<OccurrenceSettlementCompensationRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM transfer_occurrence_settlement_compensation
         WHERE compensation_fact_uid = ?",
    )
    .bind(fact_uid)
    .fetch_optional(pool)
    .await?
    .map(map_occurrence_settlement_compensation))
}

pub async fn occurrence_settlement_compensations(
    pool: &SqlitePool,
    occurrence_uid: &str,
) -> Result<Vec<OccurrenceSettlementCompensationRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM transfer_occurrence_settlement_compensation
         WHERE occurrence_uid = ? ORDER BY created_at, uid",
    )
    .bind(occurrence_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_occurrence_settlement_compensation)
    .collect())
}

async fn occurrence_settlement_compensation_outcome_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<OccurrenceSettlementCompensationOutcome>, StoreError> {
    let Some(correction) = sqlx::query(
        "SELECT * FROM transfer_occurrence_settlement_compensation
         WHERE idempotency_key = ?",
    )
    .bind(request_id.trim())
    .fetch_optional(pool)
    .await?
    .map(map_occurrence_settlement_compensation) else {
        return Ok(None);
    };
    let fact = crate::facts::get(pool, &correction.compensation_fact_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    Ok(Some(OccurrenceSettlementCompensationOutcome {
        correction,
        fact,
    }))
}

pub async fn occurrence_settlement_compensation_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<OccurrenceSettlementCompensationOutcome>, StoreError> {
    occurrence_settlement_compensation_outcome_for_request(pool, request_id).await
}

pub async fn compensate_occurrence_settlement<F>(
    pool: &SqlitePool,
    input: OccurrenceSettlementCompensationInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<OccurrenceSettlementCompensationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_id = validate_request_key(&input.idempotency_key)?;
    if let Some(outcome) =
        occurrence_settlement_compensation_outcome_for_request(pool, request_id).await?
    {
        if outcome.correction.settlement_uid != input.settlement_uid
            || outcome.correction.owner_person_uid != input.actor_person_uid
        {
            return Err(sqlx::Error::Protocol(
                "compensation request id was already used with different targets".into(),
            ));
        }
        return Ok(OccurrenceSettlementCompensationCommit::Replayed(outcome));
    }
    ensure_phase4_request_unused(pool, request_id).await?;

    let mut tx = crate::write_tx(pool).await?;
    let slice = sqlx::query("SELECT * FROM transfer_occurrence_settlement_slice WHERE uid = ?")
        .bind(&input.settlement_uid)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
        .and_then(map_occurrence_settlement_slice)?;
    if slice.owner_person_uid != input.actor_person_uid {
        return Err(sqlx::Error::Protocol(
            "only the settlement owner may compensate its private application".into(),
        ));
    }
    if sqlx::query(
        "SELECT 1 FROM transfer_occurrence_settlement_compensation
         WHERE settlement_uid = ?",
    )
    .bind(&input.settlement_uid)
    .fetch_optional(&mut *tx)
    .await?
    .is_some()
    {
        return Err(sqlx::Error::Protocol(
            "settlement application was already compensated".into(),
        ));
    }
    let correction_uid = nucleus::new_uid("tsc");
    let evidence = TransferOccurrenceSettlementCompensationEvidence {
        action: "compensate-transfer-occurrence-settlement".into(),
        idempotency_key: request_id.into(),
        compensation_uid: correction_uid.clone(),
        settlement_uid: slice.uid.clone(),
        transfer_uid: slice.transfer_uid.clone(),
        occurrence_uid: slice.occurrence_uid.clone(),
        owner_person_uid: input.actor_person_uid.clone(),
        original_application_fact_uid: slice.application_fact_uid.clone(),
    };
    let payload = serde_json::to_string(&evidence)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let previous_hash = crate::facts::last_hash(&mut tx).await?;
    let mut fact = nucleus::fact::seal(
        NewFact {
            uid: None,
            record_uid: slice.local_record_uid.clone(),
            delta: crate::exact::from_f64(-slice.local_delta),
            at: None,
            actor_uid: Some(input.actor_person_uid.clone()),
            cause: Cause {
                kind: nucleus::CauseKind::Compensation,
                uid: Some(slice.application_fact_uid.clone()),
            },
            payload: Some(payload),
        },
        &previous_hash,
        now,
    );
    fact.signature = sign(&fact.hash);
    if fact.signature.is_none() && input.authorization_intent_uid.is_none() {
        return Err(sqlx::Error::Protocol(
            "settlement compensation requires a signing key or verified Action intent".into(),
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
         VALUES (?, 'compensation', ?, ?, ?)",
    )
    .bind(request_id)
    .bind(&slice.uid)
    .bind(&fact.uid)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO transfer_occurrence_settlement_compensation
            (uid, settlement_uid, occurrence_uid, transfer_uid,
             owner_person_uid, original_application_fact_uid,
             compensation_fact_uid, local_record_uid, inverse_delta,
             idempotency_key, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&correction_uid)
    .bind(&slice.uid)
    .bind(&slice.occurrence_uid)
    .bind(&slice.transfer_uid)
    .bind(&slice.owner_person_uid)
    .bind(&slice.application_fact_uid)
    .bind(&fact.uid)
    .bind(&slice.local_record_uid)
    .bind(-slice.local_delta)
    .bind(request_id)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    crate::records::bump_quantity(
        &mut tx,
        &slice.local_record_uid,
        crate::exact::from_f64(-slice.local_delta),
        &at,
    )
    .await?;
    tx.commit().await?;

    let outcome = occurrence_settlement_compensation_outcome_for_request(pool, request_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    Ok(OccurrenceSettlementCompensationCommit::Committed(outcome))
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

async fn occurrence_dispute_outcome_for_request(
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

pub async fn occurrence_dispute_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<OccurrenceDisputeOutcome>, StoreError> {
    occurrence_dispute_outcome_for_request(pool, request_id).await
}

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
    if let Some(outcome) = occurrence_dispute_outcome_for_request(pool, request_id).await? {
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
    let transfer_uid: String = occurrence.get("transfer_uid");
    let giver_person_uid: String = occurrence.get("giver_person_uid");
    let receiver_person_uid: String = occurrence.get("receiver_person_uid");
    if input.actor_person_uid != giver_person_uid && input.actor_person_uid != receiver_person_uid {
        return Err(sqlx::Error::Protocol(
            "only an occurrence giver or receiver may set a dispute".into(),
        ));
    }

    let event_uid = nucleus::new_uid("tde");
    let evidence = TransferOccurrenceDisputeEvidence {
        action: "set-transfer-occurrence-dispute".into(),
        idempotency_key: request_id.into(),
        transfer_uid: transfer_uid.clone(),
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
            record_uid: transfer_uid.clone(),
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
    .bind(&transfer_uid)
    .bind(&input.actor_person_uid)
    .bind(i64::from(input.disputed))
    .bind(&fact.uid)
    .bind(request_id)
    .bind(&at)
    .execute(&mut *tx)
    .await?;

    let system_projection = sqlx::query(
        "SELECT system_disputed, system_dispute_fact_uid, system_disputed_at
         FROM transfer_occurrence WHERE uid = ?",
    )
    .bind(&input.occurrence_uid)
    .fetch_one(&mut *tx)
    .await?;
    let system_disputed = system_projection.get::<i64, _>("system_disputed") != 0;
    let active_dispute = sqlx::query(
        "SELECT latest.fact_uid, latest.created_at
         FROM transfer_occurrence_dispute_event latest
         WHERE latest.occurrence_uid = ? AND latest.asserted = 1
           AND NOT EXISTS (
               SELECT 1 FROM transfer_occurrence_dispute_event newer
               WHERE newer.occurrence_uid = latest.occurrence_uid
                 AND newer.actor_person_uid = latest.actor_person_uid
                 AND newer.rowid > latest.rowid
           )
         ORDER BY latest.rowid DESC LIMIT 1",
    )
    .bind(&input.occurrence_uid)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(active) = active_dispute {
        sqlx::query(
            "UPDATE transfer_occurrence
             SET disputed = 1, dispute_fact_uid = ?, disputed_at = ? WHERE uid = ?",
        )
        .bind(active.get::<String, _>("fact_uid"))
        .bind(active.get::<String, _>("created_at"))
        .bind(&input.occurrence_uid)
        .execute(&mut *tx)
        .await?;
    } else if system_disputed {
        sqlx::query(
            "UPDATE transfer_occurrence
             SET disputed = 1, dispute_fact_uid = ?, disputed_at = ? WHERE uid = ?",
        )
        .bind(system_projection.get::<Option<String>, _>("system_dispute_fact_uid"))
        .bind(system_projection.get::<Option<String>, _>("system_disputed_at"))
        .bind(&input.occurrence_uid)
        .execute(&mut *tx)
        .await?;
    } else {
        sqlx::query(
            "UPDATE transfer_occurrence
             SET disputed = 0, dispute_fact_uid = NULL, disputed_at = NULL WHERE uid = ?",
        )
        .bind(&input.occurrence_uid)
        .execute(&mut *tx)
        .await?;
    }
    crate::records::bump_quantity(&mut tx, &transfer_uid, crate::exact::zero(), &at).await?;
    tx.commit().await?;

    let outcome = occurrence_dispute_outcome_for_request(pool, request_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    Ok(OccurrenceDisputeCommit::Committed(outcome))
}

pub async fn revision_fact(
    pool: &SqlitePool,
    transfer_uid: &str,
    revision: u64,
) -> Result<Option<Fact>, StoreError> {
    let revision = i64::try_from(revision)
        .map_err(|_| sqlx::Error::Protocol("transfer revision exceeds SQLite range".into()))?;
    let Some(fact_uid) = sqlx::query_scalar::<_, String>(
        "SELECT fact_uid FROM transfer_revision WHERE transfer_uid = ? AND revision = ?",
    )
    .bind(transfer_uid)
    .bind(revision)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };
    crate::facts::get(pool, &fact_uid).await
}

pub async fn promises_of(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<Vec<crate::misc::PromiseRow>, StoreError> {
    Ok(sqlx::query("SELECT * FROM promise WHERE transfer_uid = ?")
        .bind(transfer_uid)
        .fetch_all(pool)
        .await?
        .into_iter()
        .filter_map(crate::misc::map_promise_pub)
        .collect())
}

pub async fn siblings_of_source(
    pool: &SqlitePool,
    source_uid: &str,
    except: &str,
) -> Result<Vec<String>, StoreError> {
    Ok(
        sqlx::query("SELECT record_uid FROM transfer WHERE source_uid = ? AND record_uid != ?")
            .bind(source_uid)
            .bind(except)
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|r| r.get("record_uid"))
            .collect(),
    )
}

fn map_transfer_correction_link(row: SqliteRow) -> TransferCorrectionLinkRow {
    TransferCorrectionLinkRow {
        uid: row.get("uid"),
        kind: row.get("kind"),
        source_transfer_uid: row.get("source_transfer_uid"),
        source_occurrence_uid: row.get("source_occurrence_uid"),
        created_transfer_uid: row.get("created_transfer_uid"),
        source_revision: row.get::<i64, _>("source_revision") as u64,
        canonical_quantity: row.get("canonical_quantity"),
        actor_person_uid: row.get("actor_person_uid"),
        fact_uid: row.get("fact_uid"),
        idempotency_key: row.get("idempotency_key"),
        created_at: row.get("created_at"),
    }
}

pub async fn correction_link_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<TransferCorrectionLinkRow>, StoreError> {
    Ok(
        sqlx::query("SELECT * FROM transfer_correction_link WHERE idempotency_key = ?")
            .bind(request_id.trim())
            .fetch_optional(pool)
            .await?
            .map(map_transfer_correction_link),
    )
}

pub async fn correction_links_for_transfer(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<Vec<TransferCorrectionLinkRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM transfer_correction_link
         WHERE source_transfer_uid = ? OR created_transfer_uid = ?
         ORDER BY created_at, uid",
    )
    .bind(transfer_uid)
    .bind(transfer_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_transfer_correction_link)
    .collect())
}

fn map_promise_successor(row: SqliteRow) -> PromiseSuccessorRow {
    PromiseSuccessorRow {
        uid: row.get("uid"),
        transfer_uid: row.get("transfer_uid"),
        predecessor_promise_uid: row.get("predecessor_promise_uid"),
        successor_promise_uid: row.get("successor_promise_uid"),
        revision: row.get::<i64, _>("revision") as u64,
        actor_person_uid: row.get("actor_person_uid"),
        fact_uid: row.get("fact_uid"),
        idempotency_key: row.get("idempotency_key"),
        created_at: row.get("created_at"),
    }
}

pub async fn promise_successor_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<PromiseSuccessorRow>, StoreError> {
    Ok(
        sqlx::query("SELECT * FROM transfer_promise_successor WHERE idempotency_key = ?")
            .bind(request_id.trim())
            .fetch_optional(pool)
            .await?
            .map(map_promise_successor),
    )
}

pub async fn promise_successors_for_transfer(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<Vec<PromiseSuccessorRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM transfer_promise_successor
         WHERE transfer_uid = ? ORDER BY created_at, uid",
    )
    .bind(transfer_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_promise_successor)
    .collect())
}

fn map_source_group_result(row: SqliteRow) -> TransferSourceGroupResultRow {
    TransferSourceGroupResultRow {
        uid: row.get("uid"),
        source_uid: row.get("source_uid"),
        policy: row.get("policy"),
        winner_transfer_uid: row.get("transfer_uid"),
        winner_revision: row.get::<i64, _>("transfer_revision") as u64,
        settlement_uid: row.get("settlement_uid"),
        fact_uid: row.get("fact_uid"),
        created_at: row.get("created_at"),
    }
}

fn map_source_group_loser(row: SqliteRow) -> TransferSourceGroupLoserRow {
    TransferSourceGroupLoserRow {
        uid: row.get("uid"),
        result_uid: row.get("result_uid"),
        transfer_uid: row.get("transfer_uid"),
        transfer_revision: row.get::<i64, _>("transfer_revision") as u64,
        fact_uid: row.get("fact_uid"),
        created_at: row.get("created_at"),
    }
}

pub async fn source_group_result(
    pool: &SqlitePool,
    source_uid: &str,
) -> Result<Option<TransferSourceGroupResultRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM transfer_source_group_result
         WHERE source_uid = ? AND policy = 'first_completes'",
    )
    .bind(source_uid)
    .fetch_optional(pool)
    .await?
    .map(map_source_group_result))
}

pub async fn source_group_state_for_transfer(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<Option<TransferSourceGroupState>, StoreError> {
    let Some(result_row) = sqlx::query(
        "SELECT result.* FROM transfer transfer
         JOIN transfer_source_group_result result
           ON result.source_uid = transfer.source_uid
          AND result.policy = transfer.satiation
         WHERE transfer.record_uid = ?",
    )
    .bind(transfer_uid)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };
    let result = map_source_group_result(result_row);
    let loser = sqlx::query(
        "SELECT * FROM transfer_source_group_loser
         WHERE result_uid = ? AND transfer_uid = ?",
    )
    .bind(&result.uid)
    .bind(transfer_uid)
    .fetch_optional(pool)
    .await?
    .map(map_source_group_loser);
    Ok(Some(TransferSourceGroupState {
        satiated: result.winner_transfer_uid != transfer_uid,
        result,
        loser,
    }))
}

pub async fn conditional_pending(
    pool: &SqlitePool,
) -> Result<Vec<crate::misc::PromiseRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM promise WHERE condition IS NOT NULL AND state IN ('proposed', 'agreed')",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .filter_map(crate::misc::map_promise_pub)
    .collect())
}
