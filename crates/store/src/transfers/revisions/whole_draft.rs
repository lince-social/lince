use std::collections::HashSet;

use chrono::{DateTime, Utc};
use nucleus::transfer::{AgreementGuard, OpenPromiseReusePolicy, karma::Guard};
use sqlx::{Row, SqlitePool};

use super::{
    RevisionAuthorization, RevisionCommit, RevisionWrite, append_revision,
    cancel_stale_item_delivery, fact_has_transfer_revision_action, snapshot::validated_snapshot,
};
use crate::{
    StoreError, records,
    transfers::{
        PromiseSuccessorInput, TransferDependencyInput, TransferLocationSnapshot,
        correction_link_for_request,
        dependencies::{
            replace_dependencies, validate_dependency_inputs, validate_transfer_parent,
        },
        ensure_request_not_used_by_invitation_event,
        item_terms::validate_location,
        karma_snapshot, open_claim_pair_for_request, promise_successor_for_request,
    },
};

mod promises;

/// The replacement public and private terms for a Transfer.
#[derive(Debug, Clone)]
pub struct TransferDraftTermsInput {
    pub public_history: Option<nucleus::transfer::public_history::Terms>,
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

/// A promise to keep, update, or add in the revised draft.
#[derive(Debug, Clone)]
pub struct DraftPromiseRevisionInput {
    pub uid: Option<String>,
    pub item: Option<nucleus::transfer::disclosure::TransferItem>,
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

/// A replacement draft with its expected revision, dependencies, and retained invitations.
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

/// Replaces the draft terms and records a revision.
///
/// # Errors
///
/// Returns an error for invalid terms, changed source data, conflicting replay,
/// missing authorization, or a storage failure.
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
        RevisionKind::Revise,
        None,
        sign,
    )
    .await
}

/// Revises the draft after checking the reviewed state and stock basis.
///
/// # Errors
///
/// Returns an error if the review is stale or the revision cannot be committed.
pub async fn revise_whole_draft_guarded<F>(
    pool: &SqlitePool,
    input: WholeDraftRevisionInput,
    expected_state: &Guard,
    stock_basis: (&str, &str),
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
        RevisionKind::Revise,
        Some(RevisionGuards {
            agreement: None,
            state: Some(expected_state),
            stock_basis: Some(stock_basis),
        }),
        sign,
    )
    .await
}

/// Creates signed terms for a legacy revision-zero Transfer.
///
/// # Errors
///
/// Returns an error for a nonzero expected revision or a failed revision.
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
        RevisionKind::Adopt,
        None,
        sign,
    )
    .await
}

/// Records revised terms as a counteroffer.
///
/// # Errors
///
/// Returns an error if the counteroffer is invalid or cannot be stored.
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
        RevisionKind::Counteroffer,
        None,
        sign,
    )
    .await
}

/// Records the published terms of the draft.
///
/// # Errors
///
/// Returns an error if the terms cannot be validated or stored.
pub async fn publish_whole_draft<F>(
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
        RevisionKind::Publish,
        None,
        sign,
    )
    .await
}

/// Publishes the terms after checking the supplied state and agreement guards.
///
/// # Errors
///
/// Returns an error if a guard changed or publication fails.
pub async fn publish_whole_draft_guarded<F>(
    pool: &SqlitePool,
    input: WholeDraftRevisionInput,
    expected: Option<&AgreementGuard>,
    expected_state: Option<&Guard>,
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
        RevisionKind::Publish,
        Some(RevisionGuards {
            agreement: expected,
            state: expected_state,
            stock_basis: None,
        }),
        sign,
    )
    .await
}

/// Records a successor for the reviewed predecessor promise.
///
/// # Errors
///
/// Returns an error for missing or invalid lineage or a failed revision.
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
        RevisionKind::Reopen,
        None,
        sign,
    )
    .await
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RevisionKind {
    Revise,
    Adopt,
    Counteroffer,
    Publish,
    Reopen,
}

impl RevisionKind {
    const fn action(self) -> &'static str {
        match self {
            Self::Revise => "revise-transfer-draft",
            Self::Adopt => "adopt-legacy-transfer-draft",
            Self::Counteroffer => "counteroffer-transfer-draft",
            Self::Publish => "publish-transfer",
            Self::Reopen => "reopen-transfer-promise",
        }
    }
}

struct RevisionGuards<'a> {
    agreement: Option<&'a AgreementGuard>,
    state: Option<&'a Guard>,
    stock_basis: Option<(&'a str, &'a str)>,
}

async fn commit_whole_draft<F>(
    pool: &SqlitePool,
    input: WholeDraftRevisionInput,
    now: DateTime<Utc>,
    fact_actor: Option<String>,
    kind: RevisionKind,
    guards: Option<RevisionGuards<'_>>,
    sign: F,
) -> Result<RevisionCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let action = kind.action();
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

    if let Some(guards) = guards {
        if let Some((record, expected_basis)) = guards.stock_basis {
            let balance = crate::transfer_balances::read_on(&mut tx, record).await?;
            let unit: Option<String> = sqlx::query_scalar(
                "SELECT unit_uid FROM record WHERE uid=? AND deleted_at IS NULL",
            )
            .bind(record)
            .fetch_one(&mut *tx)
            .await?;
            let current = nucleus::karma::canonical_hash(
                "lince.stock-policy-basis.v1",
                &serde_json::json!({"balance":balance,"unit":unit}),
            )
            .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
            if current.as_str() != expected_basis {
                return Err(sqlx::Error::Protocol(
                    "stock policy inputs changed; refresh the offer".into(),
                ));
            }
        }
        karma_snapshot::check_on(&mut tx, &input.transfer_uid, guards.state).await?;
        karma_snapshot::check_participant_on(
            &mut tx,
            &input.transfer_uid,
            &input.proposal_author_person_uid,
            guards.agreement,
        )
        .await?;
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

    if expected_revision == 0 && kind != RevisionKind::Adopt {
        return Err(sqlx::Error::Protocol(
            "legacy revision-0 Transfer requires explicit adoption".into(),
        ));
    }
    let revision = expected_revision
        .checked_add(1)
        .ok_or_else(|| sqlx::Error::Protocol("transfer revision overflow".into()))?;
    let old_parent: Option<String> =
        sqlx::query_scalar("SELECT parent_uid FROM transfer WHERE record_uid = ?")
            .bind(&input.transfer_uid)
            .fetch_one(&mut *tx)
            .await?;
    if old_parent != input.terms.parent_uid {
        for parent in old_parent.iter().chain(input.terms.parent_uid.iter()) {
            crate::transfer_children::require_participant(
                &mut tx,
                parent,
                &input.proposal_author_person_uid,
            )
            .await?;
        }
    }
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
        if current_revision == 0 && kind != RevisionKind::Adopt {
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
    records::set_transfer_public_history_on(
        &mut tx,
        &input.transfer_uid,
        input.terms.public_history.as_ref(),
    )
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
    if matches!(kind, RevisionKind::Revise | RevisionKind::Adopt) {
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

    promises::replace(
        &mut tx,
        &input.transfer_uid,
        &input.promises,
        &input.proposal_author_person_uid,
        revision,
        &at,
    )
    .await?;
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

    let snapshot = validated_snapshot(&mut tx, &input.transfer_uid).await?;
    let fact = append_revision(
        &mut tx,
        snapshot,
        RevisionWrite {
            transfer_uid: &input.transfer_uid,
            revision,
            previous: Some(input.expected_revision),
            request_key,
            action,
            actor: fact_actor,
            delta: crate::exact::zero(),
            authorization: RevisionAuthorization::Required {
                intent: input.authorization_intent_uid.as_deref(),
                missing: "Transfer revision requires a signing key or verified Action intent",
            },
        },
        now,
        &at,
        &sign,
    )
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
    cancel_stale_item_delivery(&mut tx, &input.transfer_uid, revision).await?;
    let mut parent_facts = Vec::new();
    if old_parent != input.terms.parent_uid {
        if let Some(parent) = &old_parent {
            sqlx::query(
                "DELETE FROM transfer_child_requirement WHERE parent_uid = ? AND child_uid = ?",
            )
            .bind(parent)
            .bind(&input.transfer_uid)
            .execute(&mut *tx)
            .await?;
        }
        for parent in old_parent.iter().chain(input.terms.parent_uid.iter()) {
            let key = nucleus::fact::sha256_hex(
                format!("child-membership:{request_key}:{parent}").as_bytes(),
            );
            parent_facts.push(
                crate::transfer_children::amend(
                    &mut tx,
                    parent,
                    &input.proposal_author_person_uid,
                    &key,
                    input.authorization_intent_uid.as_deref(),
                    now,
                    &sign,
                )
                .await?,
            );
        }
    }
    tx.commit().await?;

    Ok(RevisionCommit::Committed {
        revision: revision as u64,
        fact,
        parent_facts,
    })
}

fn validate_revision_request(input: &WholeDraftRevisionInput) -> Result<(), StoreError> {
    if let Some(terms) = &input.terms.public_history {
        terms.validate().map_err(sqlx::Error::Protocol)?;
    }
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
        if promise.record_uid.is_none() && promise.concept_uid.is_none() && promise.item.is_none() {
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
