use chrono::{DateTime, TimeZone, Utc};
use nucleus::karma::{
    CanonicalHash, DelegationGrantRevision, DelegationGrantSpec, DelegationSignature,
    GrantAuthorityDecision, GrantAuthorityDenial, GrantAuthorityRequest, GrantMutationAction,
    GrantMutationEvidence, GrantMutationEvidenceSchema, GrantRevisionChange, GrantStatus,
    ReferenceKind, Slug, TypedUid, canonical_hash, canonical_json_bytes,
};
use nucleus::{Cause, CauseKind, Fact, NewFact, RecordKind};
use serde::{Deserialize, Serialize};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::StoreError;

const REQUEST_HASH_DOMAIN: &str = "karma.grant-request.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantHandleRow {
    pub record_uid: String,
    pub slug: String,
    pub handle_revision: u64,
    pub status: GrantStatus,
    pub head_revision_hash: CanonicalHash,
    pub active_revision_hash: Option<CanonicalHash>,
    pub principal_person_uid: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantRevisionRow {
    pub revision_hash: CanonicalHash,
    pub grant_uid: String,
    pub revision: DelegationGrantRevision,
    pub signature: DelegationSignature,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct CreateGrantInput {
    pub request_id: String,
    pub slug: Slug,
    pub grant: DelegationGrantSpec,
    pub actor_person_uid: String,
}

#[derive(Debug, Clone)]
pub struct NarrowGrantInput {
    pub request_id: String,
    pub grant_uid: String,
    pub expected_handle_revision: u64,
    pub grant: DelegationGrantSpec,
    pub actor_person_uid: String,
}

#[derive(Debug, Clone)]
pub struct ActivateGrantInput {
    pub request_id: String,
    pub grant_uid: String,
    pub expected_handle_revision: u64,
    pub revision_hash: CanonicalHash,
    pub actor_person_uid: String,
}

#[derive(Debug, Clone)]
pub struct RevokeGrantInput {
    pub request_id: String,
    pub grant_uid: String,
    pub expected_handle_revision: u64,
    pub actor_person_uid: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GrantMutationCommit {
    Committed { handle: GrantHandleRow, fact: Fact },
    Replayed { handle: GrantHandleRow, fact: Fact },
    Stale { current_handle_revision: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantAuthorityEvaluation {
    pub grant_uid: String,
    pub handle_revision: Option<u64>,
    pub revision_hash: Option<CanonicalHash>,
    pub request_hash: CanonicalHash,
    pub decision: GrantAuthorityDecision,
}

pub async fn create<F>(
    pool: &SqlitePool,
    input: CreateGrantInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<GrantMutationCommit, StoreError>
where
    F: Fn(&str) -> Option<DelegationSignature> + Send + Sync,
{
    validate_request_id(&input.request_id)?;
    let principal = person(&input.actor_person_uid)?;
    let revision =
        DelegationGrantRevision::new(principal.clone(), input.grant).map_err(boundary)?;
    let revision_hash = revision.revision_hash().map_err(boundary)?;
    let fingerprint = request_hash(&RequestFingerprint {
        action: GrantMutationAction::Create,
        slug: Some(&input.slug),
        grant_uid: None,
        expected_handle_revision: None,
        revision_hash: Some(&revision_hash),
        actor_person_uid: &input.actor_person_uid,
    })?;
    let now = canonical_time(now)?;
    let at = now.to_rfc3339();
    let mut tx = pool.begin().await?;
    if let Some(commit) = replay_request(&mut tx, &input.request_id, &fingerprint).await? {
        tx.rollback().await?;
        return Ok(commit);
    }
    validate_program_scope(&mut tx, &revision).await?;
    let revision_signature = require_signature(&sign, revision_hash.as_str(), &principal)?;
    let grant_uid = nucleus::new_uid("r");
    let origin_organ_uid: Option<String> = sqlx::query_scalar(
        "SELECT uid FROM record WHERE slug = ? AND kind = ? AND deleted_at IS NULL LIMIT 1",
    )
    .bind(crate::organs::LOCAL_ORGAN_SLUG)
    .bind(RecordKind::Organ.as_str())
    .fetch_optional(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO record
            (uid, slug, kind, head, body, quantity_mantissa, quantity_scale, created_at, updated_at, organ_uid)
         VALUES (?, ?, ?, ?, '', '0', 0, ?, ?, ?)",
    )
    .bind(&grant_uid)
    .bind(input.slug.as_str())
    .bind(RecordKind::Grant.as_str())
    .bind(&revision.spec.purpose)
    .bind(&at)
    .bind(&at)
    .bind(origin_organ_uid)
    .execute(&mut *tx)
    .await?;
    insert_revision(
        &mut tx,
        &grant_uid,
        &revision,
        &revision_hash,
        &revision_signature,
        &at,
    )
    .await?;
    sqlx::query(
        "INSERT INTO karma_grant
            (record_uid, handle_revision, status, head_revision_hash,
             active_revision_hash, principal_person_uid, created_at, updated_at)
         VALUES (?, 1, 'draft', ?, NULL, ?, ?, ?)",
    )
    .bind(&grant_uid)
    .bind(revision_hash.as_str())
    .bind(&input.actor_person_uid)
    .bind(&at)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    finish_mutation(
        tx,
        &input.request_id,
        GrantMutationAction::Create,
        &fingerprint,
        &grant_uid,
        None,
        1,
        Some(&revision_hash),
        false,
        false,
        now,
        &sign,
    )
    .await
}

pub async fn narrow<F>(
    pool: &SqlitePool,
    input: NarrowGrantInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<GrantMutationCommit, StoreError>
where
    F: Fn(&str) -> Option<DelegationSignature> + Send + Sync,
{
    validate_request_id(&input.request_id)?;
    validate_grant_uid(&input.grant_uid)?;
    let principal = person(&input.actor_person_uid)?;
    let replacement =
        DelegationGrantRevision::new(principal.clone(), input.grant).map_err(boundary)?;
    let revision_hash = replacement.revision_hash().map_err(boundary)?;
    let fingerprint = request_hash(&RequestFingerprint {
        action: GrantMutationAction::Narrow,
        slug: None,
        grant_uid: Some(&input.grant_uid),
        expected_handle_revision: Some(input.expected_handle_revision),
        revision_hash: Some(&revision_hash),
        actor_person_uid: &input.actor_person_uid,
    })?;
    let now = canonical_time(now)?;
    let at = now.to_rfc3339();
    let mut tx = pool.begin().await?;
    if let Some(commit) = replay_request(&mut tx, &input.request_id, &fingerprint).await? {
        tx.rollback().await?;
        return Ok(commit);
    }
    let current = get_handle_in_tx(&mut tx, &input.grant_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    if current.handle_revision != input.expected_handle_revision {
        tx.rollback().await?;
        return Ok(GrantMutationCommit::Stale {
            current_handle_revision: current.handle_revision,
        });
    }
    if current.principal_person_uid != input.actor_person_uid {
        return Err(protocol("only the grant principal may narrow the grant"));
    }
    if current.status == GrantStatus::Revoked {
        return Err(protocol("revoked Karma grants cannot be revised"));
    }
    let previous = get_revision_in_tx(&mut tx, &input.grant_uid, &current.head_revision_hash)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    if previous
        .revision
        .compare_replacement(&replacement)
        .map_err(boundary)?
        != GrantRevisionChange::Narrowing
    {
        return Err(protocol(
            "narrow Karma grant accepts only a strict authority subset",
        ));
    }
    validate_program_scope(&mut tx, &replacement).await?;
    let revision_signature = require_signature(&sign, revision_hash.as_str(), &principal)?;
    insert_revision(
        &mut tx,
        &input.grant_uid,
        &replacement,
        &revision_hash,
        &revision_signature,
        &at,
    )
    .await?;
    let next = next_revision(current.handle_revision)?;
    let active = (current.status == GrantStatus::Active).then_some(revision_hash.as_str());
    let updated = sqlx::query(
        "UPDATE karma_grant
         SET handle_revision = ?, head_revision_hash = ?, active_revision_hash = ?, updated_at = ?
         WHERE record_uid = ? AND handle_revision = ? AND status != 'revoked'",
    )
    .bind(sql_revision(next)?)
    .bind(revision_hash.as_str())
    .bind(active)
    .bind(&at)
    .bind(&input.grant_uid)
    .bind(sql_revision(current.handle_revision)?)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() != 1 {
        return stale(tx, &input.grant_uid).await;
    }
    finish_mutation(
        tx,
        &input.request_id,
        GrantMutationAction::Narrow,
        &fingerprint,
        &input.grant_uid,
        Some(input.expected_handle_revision),
        next,
        Some(&revision_hash),
        current.status == GrantStatus::Active,
        false,
        now,
        &sign,
    )
    .await
}

pub async fn activate<F>(
    pool: &SqlitePool,
    input: ActivateGrantInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<GrantMutationCommit, StoreError>
where
    F: Fn(&str) -> Option<DelegationSignature> + Send + Sync,
{
    mutate_status(
        pool,
        input.request_id,
        input.grant_uid,
        input.expected_handle_revision,
        Some(input.revision_hash),
        input.actor_person_uid,
        now,
        sign,
    )
    .await
}

pub async fn revoke<F>(
    pool: &SqlitePool,
    input: RevokeGrantInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<GrantMutationCommit, StoreError>
where
    F: Fn(&str) -> Option<DelegationSignature> + Send + Sync,
{
    mutate_status(
        pool,
        input.request_id,
        input.grant_uid,
        input.expected_handle_revision,
        None,
        input.actor_person_uid,
        now,
        sign,
    )
    .await
}

pub async fn get_handle(
    pool: &SqlitePool,
    grant_uid: &str,
) -> Result<Option<GrantHandleRow>, StoreError> {
    let row = sqlx::query(
        "SELECT grant.*, record.slug, record.kind FROM karma_grant grant
         JOIN record ON record.uid = grant.record_uid
         WHERE grant.record_uid = ? AND record.deleted_at IS NULL",
    )
    .bind(grant_uid)
    .fetch_optional(pool)
    .await?;
    row.map(map_handle).transpose()
}

pub async fn list_handles(pool: &SqlitePool) -> Result<Vec<GrantHandleRow>, StoreError> {
    sqlx::query(
        "SELECT grant.*, record.slug, record.kind FROM karma_grant grant
         JOIN record ON record.uid = grant.record_uid
         WHERE record.deleted_at IS NULL ORDER BY grant.created_at, grant.record_uid",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_handle)
    .collect()
}

pub async fn get_revision(
    pool: &SqlitePool,
    grant_uid: &str,
    revision_hash: &CanonicalHash,
) -> Result<Option<GrantRevisionRow>, StoreError> {
    let row =
        sqlx::query("SELECT * FROM karma_grant_revision WHERE grant_uid = ? AND revision_hash = ?")
            .bind(grant_uid)
            .bind(revision_hash.as_str())
            .fetch_optional(pool)
            .await?;
    row.map(map_revision).transpose()
}

pub async fn list_revisions(pool: &SqlitePool) -> Result<Vec<GrantRevisionRow>, StoreError> {
    sqlx::query("SELECT * FROM karma_grant_revision ORDER BY grant_uid, created_at, revision_hash")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(map_revision)
        .collect()
}

pub async fn evaluate_active(
    pool: &SqlitePool,
    grant_uid: &str,
    request: &GrantAuthorityRequest,
) -> Result<GrantAuthorityEvaluation, StoreError> {
    validate_grant_uid(grant_uid)?;
    request.validate().map_err(boundary)?;
    let request_hash = request.request_hash().map_err(boundary)?;
    let Some(handle) = get_handle(pool, grant_uid).await? else {
        return Ok(GrantAuthorityEvaluation {
            grant_uid: grant_uid.to_string(),
            handle_revision: None,
            revision_hash: None,
            request_hash,
            decision: GrantAuthorityDecision::denied(GrantAuthorityDenial::GrantMissing),
        });
    };
    let denial = match handle.status {
        GrantStatus::Draft => Some(GrantAuthorityDenial::GrantInactive),
        GrantStatus::Revoked => Some(GrantAuthorityDenial::GrantRevoked),
        GrantStatus::Active => None,
    };
    if let Some(denial) = denial {
        return Ok(GrantAuthorityEvaluation {
            grant_uid: grant_uid.to_string(),
            handle_revision: Some(handle.handle_revision),
            revision_hash: handle.active_revision_hash,
            request_hash,
            decision: GrantAuthorityDecision::denied(denial),
        });
    }
    let revision_hash = handle
        .active_revision_hash
        .ok_or_else(|| protocol("active Karma grant is missing its active revision"))?;
    let revision = get_revision(pool, grant_uid, &revision_hash)
        .await?
        .ok_or_else(|| protocol("active Karma grant revision is missing"))?;
    Ok(GrantAuthorityEvaluation {
        grant_uid: grant_uid.to_string(),
        handle_revision: Some(handle.handle_revision),
        revision_hash: Some(revision_hash),
        request_hash,
        decision: revision.revision.evaluate(request),
    })
}

#[allow(clippy::too_many_arguments)]
async fn mutate_status<F>(
    pool: &SqlitePool,
    request_id: String,
    grant_uid: String,
    expected_handle_revision: u64,
    selected_revision: Option<CanonicalHash>,
    actor_person_uid: String,
    now: DateTime<Utc>,
    sign: F,
) -> Result<GrantMutationCommit, StoreError>
where
    F: Fn(&str) -> Option<DelegationSignature> + Send + Sync,
{
    validate_request_id(&request_id)?;
    validate_grant_uid(&grant_uid)?;
    let principal = person(&actor_person_uid)?;
    let action = if selected_revision.is_some() {
        GrantMutationAction::Activate
    } else {
        GrantMutationAction::Revoke
    };
    let fingerprint = request_hash(&RequestFingerprint {
        action,
        slug: None,
        grant_uid: Some(&grant_uid),
        expected_handle_revision: Some(expected_handle_revision),
        revision_hash: selected_revision.as_ref(),
        actor_person_uid: &actor_person_uid,
    })?;
    let now = canonical_time(now)?;
    let at = now.to_rfc3339();
    let mut tx = pool.begin().await?;
    if let Some(commit) = replay_request(&mut tx, &request_id, &fingerprint).await? {
        tx.rollback().await?;
        return Ok(commit);
    }
    let current = get_handle_in_tx(&mut tx, &grant_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    if current.handle_revision != expected_handle_revision {
        tx.rollback().await?;
        return Ok(GrantMutationCommit::Stale {
            current_handle_revision: current.handle_revision,
        });
    }
    if current.principal_person_uid != actor_person_uid || principal.as_str() != actor_person_uid {
        return Err(protocol("only the grant principal may change grant status"));
    }
    if current.status == GrantStatus::Revoked {
        return Err(protocol("revoked Karma grants cannot change status"));
    }
    if let Some(revision_hash) = &selected_revision {
        if &current.head_revision_hash != revision_hash {
            return Err(protocol(
                "only the current grant head revision may be activated",
            ));
        }
        if current.active_revision_hash.as_ref() == Some(revision_hash) {
            return Err(protocol("Karma grant revision is already active"));
        }
    }
    let next = next_revision(current.handle_revision)?;
    let status = if selected_revision.is_some() {
        GrantStatus::Active
    } else {
        GrantStatus::Revoked
    };
    let updated = sqlx::query(
        "UPDATE karma_grant
         SET handle_revision = ?, status = ?, active_revision_hash = ?, updated_at = ?
         WHERE record_uid = ? AND handle_revision = ? AND status != 'revoked'",
    )
    .bind(sql_revision(next)?)
    .bind(status.as_str())
    .bind(selected_revision.as_ref().map(CanonicalHash::as_str))
    .bind(&at)
    .bind(&grant_uid)
    .bind(sql_revision(current.handle_revision)?)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() != 1 {
        return stale(tx, &grant_uid).await;
    }
    finish_mutation(
        tx,
        &request_id,
        action,
        &fingerprint,
        &grant_uid,
        Some(expected_handle_revision),
        next,
        selected_revision.as_ref(),
        current.status == GrantStatus::Active,
        // No intent may outlive the consent that authorized it. Cancelling in
        // the same transaction makes revocation deterministic rather than a race
        // against whoever reads the intent next, and releases the budget.
        status == GrantStatus::Revoked,
        now,
        &sign,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn finish_mutation<F>(
    mut tx: Transaction<'_, Sqlite>,
    request_id: &str,
    action: GrantMutationAction,
    fingerprint: &CanonicalHash,
    grant_uid: &str,
    expected_handle_revision: Option<u64>,
    result_handle_revision: u64,
    revision_hash: Option<&CanonicalHash>,
    was_active: bool,
    cancel_intents: bool,
    now: DateTime<Utc>,
    sign: &F,
) -> Result<GrantMutationCommit, StoreError>
where
    F: Fn(&str) -> Option<DelegationSignature> + Send + Sync,
{
    let handle = get_handle_in_tx(&mut tx, grant_uid)
        .await?
        .expect("mutated Karma grant exists");
    let evidence = GrantMutationEvidence {
        schema: GrantMutationEvidenceSchema::V1,
        request_id: request_id.to_string(),
        action,
        grant_uid: grant_uid.to_string(),
        handle_revision: result_handle_revision,
        status: handle.status,
        head_revision_hash: handle.head_revision_hash.clone(),
        active_revision_hash: handle.active_revision_hash.clone(),
        principal_person_uid: person(&handle.principal_person_uid)?,
    };
    let previous = crate::facts::last_hash(&mut tx).await?;
    // Quantity tracks live authority, exactly as a Program's quantity tracks live
    // activation: the transition moves it, not the action name. Revoking a draft that
    // never authorized anything is a no-op, so it must not push the Record negative.
    let delta = crate::exact::integer(match (was_active, handle.status == GrantStatus::Active) {
        (false, true) => 1,
        (true, false) => -1,
        _ => 0,
    });
    let mut fact = nucleus::fact::seal(
        NewFact {
            uid: None,
            record_uid: grant_uid.to_string(),
            delta,
            at: Some(now),
            actor_uid: Some(handle.principal_person_uid.clone()),
            cause: Cause {
                kind: CauseKind::Action,
                uid: Some(request_id.to_string()),
            },
            payload: Some(canonical_string(&evidence)?),
        },
        &previous,
        now,
    );
    // The lifecycle Fact is signed by the same Person who signed the revision, over the
    // Fact's own chain hash. Fact hashes are bare hex, not canonical `sha256:` handles.
    fact.signature =
        Some(require_signature(sign, &fact.hash, &evidence.principal_person_uid)?.signature);
    crate::facts::insert(&mut tx, &fact).await?;
    crate::records::bump_quantity(&mut tx, grant_uid, delta, &now.to_rfc3339()).await?;
    insert_request(
        &mut tx,
        request_id,
        action,
        fingerprint,
        grant_uid,
        expected_handle_revision,
        result_handle_revision,
        &handle,
        revision_hash,
        &fact.uid,
        &now.to_rfc3339(),
    )
    .await?;
    if cancel_intents {
        // Deliberately after the request row exists: a cancelled intent points at
        // the request that caused it, and a cause must be recorded before the
        // transitions that cite it.
        crate::karma::intents::cancel_for_grant_tx(
            &mut tx,
            grant_uid,
            request_id,
            &handle.principal_person_uid,
            "karma_grant_revoked",
            &now.to_rfc3339(),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(GrantMutationCommit::Committed { handle, fact })
}

async fn validate_program_scope(
    tx: &mut Transaction<'_, Sqlite>,
    revision: &DelegationGrantRevision,
) -> Result<(), StoreError> {
    let program_uid = revision.spec.program_uid.as_str();
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM karma_program WHERE record_uid = ?)")
            .bind(program_uid)
            .fetch_one(&mut **tx)
            .await?;
    if !exists {
        return Err(protocol("Karma grant Program does not exist"));
    }
    if let nucleus::karma::GrantProgramRevisionScope::Exact { revision_hash } =
        &revision.spec.program_revision
    {
        let belongs: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM karma_program_revision
             WHERE program_uid = ? AND revision_hash = ?)",
        )
        .bind(program_uid)
        .bind(revision_hash.as_str())
        .fetch_one(&mut **tx)
        .await?;
        if !belongs {
            return Err(protocol(
                "exact Karma grant revision does not belong to its Program",
            ));
        }
    }
    Ok(())
}

async fn insert_revision(
    tx: &mut Transaction<'_, Sqlite>,
    grant_uid: &str,
    revision: &DelegationGrantRevision,
    revision_hash: &CanonicalHash,
    signature: &DelegationSignature,
    at: &str,
) -> Result<(), StoreError> {
    let revision_json = canonical_string(revision)?;
    sqlx::query(
        "INSERT INTO karma_grant_revision
            (revision_hash, grant_uid, schema_name, revision_json, principal_person_uid,
             signer_key_id, revision_signature, created_at)
         VALUES (?, ?, 'karma.grant.v1', ?, ?, ?, ?, ?)",
    )
    .bind(revision_hash.as_str())
    .bind(grant_uid)
    .bind(revision_json)
    .bind(revision.principal_person_uid.as_str())
    .bind(&signature.key_id)
    .bind(&signature.signature)
    .bind(at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_request(
    tx: &mut Transaction<'_, Sqlite>,
    request_id: &str,
    action: GrantMutationAction,
    payload_hash: &CanonicalHash,
    grant_uid: &str,
    expected_handle_revision: Option<u64>,
    result_handle_revision: u64,
    handle: &GrantHandleRow,
    revision_hash: Option<&CanonicalHash>,
    fact_uid: &str,
    at: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO karma_request (request_id, family, payload_hash, created_at)
         VALUES (?, 'grant', ?, ?)",
    )
    .bind(request_id)
    .bind(payload_hash.as_str())
    .bind(at)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO karma_grant_request
            (request_id, action, payload_hash, grant_uid, expected_handle_revision,
             result_handle_revision, result_json, revision_hash, fact_uid, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(request_id)
    .bind(action_name(action))
    .bind(payload_hash.as_str())
    .bind(grant_uid)
    .bind(expected_handle_revision.map(sql_revision).transpose()?)
    .bind(sql_revision(result_handle_revision)?)
    .bind(canonical_string(handle)?)
    .bind(revision_hash.map(CanonicalHash::as_str))
    .bind(fact_uid)
    .bind(at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn replay_request(
    tx: &mut Transaction<'_, Sqlite>,
    request_id: &str,
    payload_hash: &CanonicalHash,
) -> Result<Option<GrantMutationCommit>, StoreError> {
    let Some(row) = sqlx::query(
        "SELECT request.payload_hash AS global_payload_hash,
                grant_request.payload_hash, grant_request.grant_uid,
                grant_request.result_handle_revision, grant_request.result_json,
                grant_request.fact_uid
         FROM karma_grant_request grant_request
         JOIN karma_request request ON request.request_id = grant_request.request_id
         WHERE grant_request.request_id = ? AND request.family = 'grant'",
    )
    .bind(request_id)
    .fetch_optional(&mut **tx)
    .await?
    else {
        return Ok(None);
    };
    if row.get::<String, _>("payload_hash") != payload_hash.as_str()
        || row.get::<String, _>("global_payload_hash") != payload_hash.as_str()
    {
        return Err(protocol(
            "Karma Grant request id was replayed with a different payload",
        ));
    }
    let result_json: String = row.get("result_json");
    let handle: GrantHandleRow = serde_json::from_str(&result_json).map_err(json_protocol)?;
    if canonical_string(&handle)? != result_json
        || handle.record_uid != row.get::<String, _>("grant_uid")
        || sql_revision(handle.handle_revision)? != row.get::<i64, _>("result_handle_revision")
    {
        return Err(protocol("stored Karma Grant request result is invalid"));
    }
    let fact_uid: String = row.get("fact_uid");
    let fact = crate::facts::get_in_transaction(tx, &fact_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    Ok(Some(GrantMutationCommit::Replayed { handle, fact }))
}

async fn stale(
    mut tx: Transaction<'_, Sqlite>,
    grant_uid: &str,
) -> Result<GrantMutationCommit, StoreError> {
    let current: i64 =
        sqlx::query_scalar("SELECT handle_revision FROM karma_grant WHERE record_uid = ?")
            .bind(grant_uid)
            .fetch_one(&mut *tx)
            .await?;
    tx.rollback().await?;
    Ok(GrantMutationCommit::Stale {
        current_handle_revision: rust_revision(current)?,
    })
}

pub(crate) async fn get_handle_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    grant_uid: &str,
) -> Result<Option<GrantHandleRow>, StoreError> {
    let row = sqlx::query(
        "SELECT grant.*, record.slug, record.kind FROM karma_grant grant
         JOIN record ON record.uid = grant.record_uid
         WHERE grant.record_uid = ? AND record.deleted_at IS NULL",
    )
    .bind(grant_uid)
    .fetch_optional(&mut **tx)
    .await?;
    row.map(map_handle).transpose()
}

pub(crate) async fn get_revision_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    grant_uid: &str,
    revision_hash: &CanonicalHash,
) -> Result<Option<GrantRevisionRow>, StoreError> {
    let row =
        sqlx::query("SELECT * FROM karma_grant_revision WHERE grant_uid = ? AND revision_hash = ?")
            .bind(grant_uid)
            .bind(revision_hash.as_str())
            .fetch_optional(&mut **tx)
            .await?;
    row.map(map_revision).transpose()
}

fn map_handle(row: sqlx::sqlite::SqliteRow) -> Result<GrantHandleRow, StoreError> {
    if row.get::<String, _>("kind") != RecordKind::Grant.as_str() {
        return Err(protocol(
            "Karma Grant sidecar is attached to a non-Grant Record",
        ));
    }
    let status = GrantStatus::parse(&row.get::<String, _>("status"))
        .ok_or_else(|| protocol("stored Karma Grant status is invalid"))?;
    let active_revision_hash = row
        .get::<Option<String>, _>("active_revision_hash")
        .map(parse_hash)
        .transpose()?;
    if (status == GrantStatus::Active) != active_revision_hash.is_some() {
        return Err(protocol("stored Karma Grant activation is invalid"));
    }
    Ok(GrantHandleRow {
        record_uid: row.get("record_uid"),
        slug: row
            .get::<Option<String>, _>("slug")
            .ok_or_else(|| protocol("Karma Grant Record must retain a slug"))?,
        handle_revision: rust_revision(row.get("handle_revision"))?,
        status,
        head_revision_hash: parse_hash(row.get("head_revision_hash"))?,
        active_revision_hash,
        principal_person_uid: row.get("principal_person_uid"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn map_revision(row: sqlx::sqlite::SqliteRow) -> Result<GrantRevisionRow, StoreError> {
    if row.get::<String, _>("schema_name") != "karma.grant.v1" {
        return Err(protocol("stored Karma Grant schema is unsupported"));
    }
    let revision_hash = parse_hash(row.get("revision_hash"))?;
    let revision_json: String = row.get("revision_json");
    let revision: DelegationGrantRevision =
        serde_json::from_str(&revision_json).map_err(json_protocol)?;
    revision.validate().map_err(boundary)?;
    if canonical_string(&revision)? != revision_json
        || revision.revision_hash().map_err(boundary)? != revision_hash
        || revision.principal_person_uid.as_str() != row.get::<String, _>("principal_person_uid")
    {
        return Err(protocol("stored Karma Grant revision is invalid"));
    }
    let signature = DelegationSignature {
        signer_person_uid: revision.principal_person_uid.clone(),
        key_id: row.get("signer_key_id"),
        signature: row.get("revision_signature"),
    };
    signature
        .validate_for(&revision.principal_person_uid)
        .map_err(boundary)?;
    Ok(GrantRevisionRow {
        revision_hash,
        grant_uid: row.get("grant_uid"),
        revision,
        signature,
        created_at: row.get("created_at"),
    })
}

#[derive(Serialize)]
struct RequestFingerprint<'a> {
    action: GrantMutationAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    slug: Option<&'a Slug>,
    #[serde(skip_serializing_if = "Option::is_none")]
    grant_uid: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_handle_revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    revision_hash: Option<&'a CanonicalHash>,
    actor_person_uid: &'a str,
}

fn require_signature<F>(
    sign: &F,
    hash: &str,
    principal: &TypedUid,
) -> Result<DelegationSignature, StoreError>
where
    F: Fn(&str) -> Option<DelegationSignature> + Send + Sync,
{
    let signature =
        sign(hash).ok_or_else(|| protocol("Karma Grant mutation requires the principal signer"))?;
    signature.validate_for(principal).map_err(boundary)?;
    Ok(signature)
}

fn request_hash(value: &RequestFingerprint<'_>) -> Result<CanonicalHash, StoreError> {
    canonical_hash(REQUEST_HASH_DOMAIN, value).map_err(boundary)
}

fn validate_request_id(value: &str) -> Result<(), StoreError> {
    if value.is_empty()
        || value.len() > 200
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        Err(protocol(
            "Karma Grant request id must contain 1 to 200 trimmed non-control bytes",
        ))
    } else {
        Ok(())
    }
}

fn person(value: &str) -> Result<TypedUid, StoreError> {
    TypedUid::new(ReferenceKind::Person, value.to_string())
        .map_err(|error| protocol(format!("invalid Karma Grant principal: {error}")))
}

fn validate_grant_uid(value: &str) -> Result<(), StoreError> {
    TypedUid::new(ReferenceKind::Grant, value.to_string())
        .map(|_| ())
        .map_err(|error| protocol(format!("invalid Karma Grant uid: {error}")))
}

fn canonical_time(value: DateTime<Utc>) -> Result<DateTime<Utc>, StoreError> {
    Utc.timestamp_millis_opt(value.timestamp_millis())
        .single()
        .ok_or_else(|| protocol("Karma Grant timestamp is outside the supported range"))
}

fn canonical_string<T: Serialize>(value: &T) -> Result<String, StoreError> {
    String::from_utf8(canonical_json_bytes(value).map_err(boundary)?)
        .map_err(|error| protocol(error.to_string()))
}

fn parse_hash(value: String) -> Result<CanonicalHash, StoreError> {
    CanonicalHash::parse(value).map_err(boundary)
}

fn next_revision(value: u64) -> Result<u64, StoreError> {
    value
        .checked_add(1)
        .ok_or_else(|| protocol("Karma Grant handle revision overflowed"))
}

fn sql_revision(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| protocol("Karma Grant revision exceeds SQLite range"))
}

fn rust_revision(value: i64) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|_| protocol("stored Karma Grant revision is invalid"))
}

fn action_name(action: GrantMutationAction) -> &'static str {
    match action {
        GrantMutationAction::Create => "create",
        GrantMutationAction::Narrow => "narrow",
        GrantMutationAction::Activate => "activate",
        GrantMutationAction::Revoke => "revoke",
    }
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
