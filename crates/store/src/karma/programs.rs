use chrono::{DateTime, TimeZone, Utc};
use nucleus::karma::{
    CanonicalHash, DefinitionStatus, ProgramAst, ProgramMutationAction, ProgramMutationEvidence,
    ProgramMutationEvidenceSchema, Proof, ProofStatus, ReferenceKind, TypedUid, canonical_hash,
    canonical_json_bytes, format_program, prove_program,
};
use nucleus::{Cause, CauseKind, Fact, NewFact, RecordKind};
use serde::{Deserialize, Serialize};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::StoreError;

const REQUEST_HASH_DOMAIN: &str = "karma.program-request.v1";
const PROGRAM_REVISION_HASH_DOMAIN: &str = "karma.program-revision.v1";
const MAX_REQUEST_ID_BYTES: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgramHandleRow {
    pub record_uid: String,
    pub slug: String,
    pub handle_revision: u64,
    pub status: DefinitionStatus,
    pub head_revision_hash: CanonicalHash,
    pub active_revision_hash: Option<CanonicalHash>,
    pub owner_person_uid: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramRevisionRow {
    pub revision_hash: CanonicalHash,
    pub program_uid: String,
    pub program: ProgramAst,
    pub canonical_dsl: String,
    pub proof: Proof,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct CreateProgramInput {
    pub request_id: String,
    pub program: ProgramAst,
    pub owner_person_uid: Option<String>,
    pub actor_person_uid: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ReviseProgramInput {
    pub request_id: String,
    pub program_uid: String,
    pub expected_handle_revision: u64,
    pub program: ProgramAst,
    pub actor_person_uid: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ActivateProgramInput {
    pub request_id: String,
    pub program_uid: String,
    pub expected_handle_revision: u64,
    pub revision_hash: CanonicalHash,
    pub actor_person_uid: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PauseProgramInput {
    pub request_id: String,
    pub program_uid: String,
    pub expected_handle_revision: u64,
    pub actor_person_uid: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProgramMutationCommit {
    Committed {
        handle: ProgramHandleRow,
        fact: Fact,
    },
    Replayed {
        handle: ProgramHandleRow,
        fact: Fact,
    },
    Stale {
        current_handle_revision: u64,
    },
}

pub async fn create<F>(
    pool: &SqlitePool,
    input: CreateProgramInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<ProgramMutationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    validate_request_id(&input.request_id)?;
    validate_person(input.owner_person_uid.as_deref(), "owner")?;
    validate_person(input.actor_person_uid.as_deref(), "actor")?;
    let prepared = PreparedRevision::new(&input.program)?;
    let fingerprint = request_hash(&RequestFingerprint {
        action: ProgramMutationAction::Create,
        program_uid: None,
        expected_handle_revision: None,
        revision_hash: Some(&prepared.revision_hash),
        owner_person_uid: input.owner_person_uid.as_deref(),
        actor_person_uid: input.actor_person_uid.as_deref(),
    })?;
    let now = canonical_time(now)?;
    let at = now.to_rfc3339();
    let mut tx = pool.begin().await?;
    if let Some(commit) = replay_request(&mut tx, &input.request_id, &fingerprint).await? {
        tx.rollback().await?;
        return Ok(commit);
    }

    let program_uid = nucleus::new_uid("r");
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
    .bind(&program_uid)
    .bind(input.program.slug.as_str())
    .bind(RecordKind::Program.as_str())
    .bind(&input.program.purpose)
    .bind(&at)
    .bind(&at)
    .bind(origin_organ_uid)
    .execute(&mut *tx)
    .await?;

    insert_revision(&mut tx, &program_uid, &prepared, &at).await?;
    let status = if prepared.proof.status == ProofStatus::Accepted {
        DefinitionStatus::Proven
    } else {
        DefinitionStatus::Draft
    };
    sqlx::query(
        "INSERT INTO karma_program
            (record_uid, handle_revision, status, head_revision_hash,
             active_revision_hash, owner_person_uid, created_at, updated_at)
         VALUES (?, 1, ?, ?, NULL, ?, ?, ?)",
    )
    .bind(&program_uid)
    .bind(status.as_str())
    .bind(prepared.revision_hash.as_str())
    .bind(&input.owner_person_uid)
    .bind(&at)
    .bind(&at)
    .execute(&mut *tx)
    .await?;

    let evidence = ProgramMutationEvidence {
        schema: ProgramMutationEvidenceSchema::V1,
        action: ProgramMutationAction::Create,
        request_id: input.request_id.clone(),
        program_uid: program_uid.clone(),
        actor_person_uid: input.actor_person_uid.clone(),
        previous_handle_revision: None,
        handle_revision: 1,
        previous_head_revision_hash: None,
        head_revision_hash: prepared.revision_hash.clone(),
        previous_active_revision_hash: None,
        active_revision_hash: None,
        status,
    };
    let fact = append_evidence_fact(
        &mut tx,
        &program_uid,
        crate::exact::zero(),
        &input.request_id,
        input.actor_person_uid,
        &evidence,
        now,
        &sign,
    )
    .await?;
    let handle = get_handle_tx(&mut tx, &program_uid)
        .await?
        .expect("new Program handle exists");
    insert_request(
        &mut tx,
        &input.request_id,
        ProgramMutationAction::Create,
        &fingerprint,
        &program_uid,
        None,
        1,
        &handle,
        Some(&prepared.revision_hash),
        &fact.uid,
        &at,
    )
    .await?;
    tx.commit().await?;
    Ok(ProgramMutationCommit::Committed { handle, fact })
}

pub async fn revise<F>(
    pool: &SqlitePool,
    input: ReviseProgramInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<ProgramMutationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    validate_request_id(&input.request_id)?;
    validate_program_uid(&input.program_uid)?;
    validate_person(input.actor_person_uid.as_deref(), "actor")?;
    let prepared = PreparedRevision::new(&input.program)?;
    let fingerprint = request_hash(&RequestFingerprint {
        action: ProgramMutationAction::Revise,
        program_uid: Some(&input.program_uid),
        expected_handle_revision: Some(input.expected_handle_revision),
        revision_hash: Some(&prepared.revision_hash),
        owner_person_uid: None,
        actor_person_uid: input.actor_person_uid.as_deref(),
    })?;
    let now = canonical_time(now)?;
    let at = now.to_rfc3339();
    let mut tx = pool.begin().await?;
    if let Some(commit) = replay_request(&mut tx, &input.request_id, &fingerprint).await? {
        tx.rollback().await?;
        return Ok(commit);
    }
    let current = get_handle_tx(&mut tx, &input.program_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    if current.handle_revision != input.expected_handle_revision {
        tx.rollback().await?;
        return Ok(ProgramMutationCommit::Stale {
            current_handle_revision: current.handle_revision,
        });
    }
    if current.status == DefinitionStatus::Retired {
        return Err(protocol("retired Karma Programs cannot be revised"));
    }
    if current.head_revision_hash == prepared.revision_hash {
        return Err(protocol(
            "Karma Program revision does not change canonical content",
        ));
    }
    let next_revision = checked_next_revision(current.handle_revision)?;
    insert_revision(&mut tx, &input.program_uid, &prepared, &at).await?;
    let status = if current.active_revision_hash.is_some() {
        DefinitionStatus::Active
    } else if current.status == DefinitionStatus::Paused {
        DefinitionStatus::Paused
    } else if prepared.proof.status == ProofStatus::Accepted {
        DefinitionStatus::Proven
    } else {
        DefinitionStatus::Draft
    };
    let updated = sqlx::query(
        "UPDATE karma_program
         SET handle_revision = ?, status = ?, head_revision_hash = ?, updated_at = ?
         WHERE record_uid = ? AND handle_revision = ? AND status != 'retired'",
    )
    .bind(sqlite_revision(next_revision)?)
    .bind(status.as_str())
    .bind(prepared.revision_hash.as_str())
    .bind(&at)
    .bind(&input.program_uid)
    .bind(sqlite_revision(current.handle_revision)?)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        let actual = current_revision_tx(&mut tx, &input.program_uid).await?;
        tx.rollback().await?;
        return Ok(ProgramMutationCommit::Stale {
            current_handle_revision: actual,
        });
    }
    sqlx::query("UPDATE record SET slug = ?, head = ?, updated_at = ? WHERE uid = ?")
        .bind(input.program.slug.as_str())
        .bind(&input.program.purpose)
        .bind(&at)
        .bind(&input.program_uid)
        .execute(&mut *tx)
        .await?;
    let evidence = ProgramMutationEvidence {
        schema: ProgramMutationEvidenceSchema::V1,
        action: ProgramMutationAction::Revise,
        request_id: input.request_id.clone(),
        program_uid: input.program_uid.clone(),
        actor_person_uid: input.actor_person_uid.clone(),
        previous_handle_revision: Some(current.handle_revision),
        handle_revision: next_revision,
        previous_head_revision_hash: Some(current.head_revision_hash),
        head_revision_hash: prepared.revision_hash.clone(),
        previous_active_revision_hash: current.active_revision_hash.clone(),
        active_revision_hash: current.active_revision_hash,
        status,
    };
    let fact = append_evidence_fact(
        &mut tx,
        &input.program_uid,
        crate::exact::zero(),
        &input.request_id,
        input.actor_person_uid,
        &evidence,
        now,
        &sign,
    )
    .await?;
    let handle = get_handle_tx(&mut tx, &input.program_uid)
        .await?
        .expect("revised Program exists");
    insert_request(
        &mut tx,
        &input.request_id,
        ProgramMutationAction::Revise,
        &fingerprint,
        &input.program_uid,
        Some(input.expected_handle_revision),
        next_revision,
        &handle,
        Some(&prepared.revision_hash),
        &fact.uid,
        &at,
    )
    .await?;
    tx.commit().await?;
    Ok(ProgramMutationCommit::Committed { handle, fact })
}

pub async fn activate<F>(
    pool: &SqlitePool,
    input: ActivateProgramInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<ProgramMutationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    validate_request_id(&input.request_id)?;
    validate_program_uid(&input.program_uid)?;
    validate_person(input.actor_person_uid.as_deref(), "actor")?;
    let fingerprint = request_hash(&RequestFingerprint {
        action: ProgramMutationAction::Activate,
        program_uid: Some(&input.program_uid),
        expected_handle_revision: Some(input.expected_handle_revision),
        revision_hash: Some(&input.revision_hash),
        owner_person_uid: None,
        actor_person_uid: input.actor_person_uid.as_deref(),
    })?;
    mutate_activation(
        pool,
        &input.request_id,
        &input.program_uid,
        input.expected_handle_revision,
        Some(&input.revision_hash),
        input.actor_person_uid,
        fingerprint,
        now,
        &sign,
    )
    .await
}

pub async fn pause<F>(
    pool: &SqlitePool,
    input: PauseProgramInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<ProgramMutationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    validate_request_id(&input.request_id)?;
    validate_program_uid(&input.program_uid)?;
    validate_person(input.actor_person_uid.as_deref(), "actor")?;
    let fingerprint = request_hash(&RequestFingerprint {
        action: ProgramMutationAction::Pause,
        program_uid: Some(&input.program_uid),
        expected_handle_revision: Some(input.expected_handle_revision),
        revision_hash: None,
        owner_person_uid: None,
        actor_person_uid: input.actor_person_uid.as_deref(),
    })?;
    mutate_activation(
        pool,
        &input.request_id,
        &input.program_uid,
        input.expected_handle_revision,
        None,
        input.actor_person_uid,
        fingerprint,
        now,
        &sign,
    )
    .await
}

pub async fn get_handle(
    pool: &SqlitePool,
    program_uid: &str,
) -> Result<Option<ProgramHandleRow>, StoreError> {
    let row = sqlx::query(
        "SELECT kp.*, record.slug, record.kind
         FROM karma_program kp JOIN record ON record.uid = kp.record_uid
         WHERE kp.record_uid = ? AND record.deleted_at IS NULL",
    )
    .bind(program_uid)
    .fetch_optional(pool)
    .await?;
    row.map(map_handle).transpose()
}

pub async fn list_handles(pool: &SqlitePool) -> Result<Vec<ProgramHandleRow>, StoreError> {
    sqlx::query(
        "SELECT kp.*, record.slug, record.kind
         FROM karma_program kp JOIN record ON record.uid = kp.record_uid
         WHERE record.deleted_at IS NULL
         ORDER BY record.slug, kp.record_uid",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_handle)
    .collect()
}

pub async fn get_revision(
    pool: &SqlitePool,
    revision_hash: &CanonicalHash,
) -> Result<Option<ProgramRevisionRow>, StoreError> {
    let row = sqlx::query("SELECT * FROM karma_program_revision WHERE revision_hash = ?")
        .bind(revision_hash.as_str())
        .fetch_optional(pool)
        .await?;
    row.map(map_revision).transpose()
}

pub async fn list_revisions(pool: &SqlitePool) -> Result<Vec<ProgramRevisionRow>, StoreError> {
    sqlx::query(
        "SELECT * FROM karma_program_revision
         ORDER BY program_uid, created_at, revision_hash",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_revision)
    .collect()
}

async fn mutate_activation<F>(
    pool: &SqlitePool,
    request_id: &str,
    program_uid: &str,
    expected_handle_revision: u64,
    selected_revision: Option<&CanonicalHash>,
    actor_person_uid: Option<String>,
    fingerprint: CanonicalHash,
    now: DateTime<Utc>,
    sign: &F,
) -> Result<ProgramMutationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let action = if selected_revision.is_some() {
        ProgramMutationAction::Activate
    } else {
        ProgramMutationAction::Pause
    };
    let now = canonical_time(now)?;
    let at = now.to_rfc3339();
    let mut tx = pool.begin().await?;
    if let Some(commit) = replay_request(&mut tx, request_id, &fingerprint).await? {
        tx.rollback().await?;
        return Ok(commit);
    }
    let current = get_handle_tx(&mut tx, program_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    if current.handle_revision != expected_handle_revision {
        tx.rollback().await?;
        return Ok(ProgramMutationCommit::Stale {
            current_handle_revision: current.handle_revision,
        });
    }
    if current.status == DefinitionStatus::Retired {
        return Err(protocol("retired Karma Programs cannot change activation"));
    }
    if let Some(revision_hash) = selected_revision {
        let accepted: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1 FROM karma_program_revision
                WHERE revision_hash = ? AND program_uid = ? AND proof_status = 'accepted'
             )",
        )
        .bind(revision_hash.as_str())
        .bind(program_uid)
        .fetch_one(&mut *tx)
        .await?;
        if !accepted {
            return Err(protocol(
                "Karma activation requires an accepted revision belonging to the Program",
            ));
        }
        if current.active_revision_hash.as_ref() == Some(revision_hash) {
            return Err(protocol("Karma Program revision is already active"));
        }
    } else if current.active_revision_hash.is_none() {
        return Err(protocol("Karma Program is not active"));
    }

    let next_revision = checked_next_revision(current.handle_revision)?;
    let status = if selected_revision.is_some() {
        DefinitionStatus::Active
    } else {
        DefinitionStatus::Paused
    };
    let updated = sqlx::query(
        "UPDATE karma_program
         SET handle_revision = ?, status = ?, active_revision_hash = ?, updated_at = ?
         WHERE record_uid = ? AND handle_revision = ? AND status != 'retired'",
    )
    .bind(sqlite_revision(next_revision)?)
    .bind(status.as_str())
    .bind(selected_revision.map(CanonicalHash::as_str))
    .bind(&at)
    .bind(program_uid)
    .bind(sqlite_revision(current.handle_revision)?)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        let actual = current_revision_tx(&mut tx, program_uid).await?;
        tx.rollback().await?;
        return Ok(ProgramMutationCommit::Stale {
            current_handle_revision: actual,
        });
    }
    let delta = crate::exact::integer(
        match (
            current.active_revision_hash.is_some(),
            selected_revision.is_some(),
        ) {
            (false, true) => 1,
            (true, false) => -1,
            _ => 0,
        },
    );
    let evidence = ProgramMutationEvidence {
        schema: ProgramMutationEvidenceSchema::V1,
        action,
        request_id: request_id.to_string(),
        program_uid: program_uid.to_string(),
        actor_person_uid: actor_person_uid.clone(),
        previous_handle_revision: Some(current.handle_revision),
        handle_revision: next_revision,
        previous_head_revision_hash: Some(current.head_revision_hash.clone()),
        head_revision_hash: current.head_revision_hash,
        previous_active_revision_hash: current.active_revision_hash,
        active_revision_hash: selected_revision.cloned(),
        status,
    };
    let fact = append_evidence_fact(
        &mut tx,
        program_uid,
        delta,
        request_id,
        actor_person_uid,
        &evidence,
        now,
        sign,
    )
    .await?;
    let handle = get_handle_tx(&mut tx, program_uid)
        .await?
        .expect("mutated Program exists");
    insert_request(
        &mut tx,
        request_id,
        action,
        &fingerprint,
        program_uid,
        Some(expected_handle_revision),
        next_revision,
        &handle,
        selected_revision,
        &fact.uid,
        &at,
    )
    .await?;
    tx.commit().await?;
    Ok(ProgramMutationCommit::Committed { handle, fact })
}

struct PreparedRevision {
    revision_hash: CanonicalHash,
    ast_json: String,
    canonical_dsl: String,
    proof_json: String,
    proof: Proof,
}

impl PreparedRevision {
    fn new(program: &ProgramAst) -> Result<Self, StoreError> {
        let proof = prove_program(program);
        let revision_hash = proof
            .revision_hash
            .clone()
            .ok_or_else(|| protocol("Karma Program cannot be canonically hashed"))?;
        let ast_json = canonical_string(program)?;
        let proof_json = canonical_string(&proof)?;
        Ok(Self {
            revision_hash,
            ast_json,
            canonical_dsl: format_program(program),
            proof_json,
            proof,
        })
    }
}

async fn insert_revision(
    tx: &mut Transaction<'_, Sqlite>,
    program_uid: &str,
    prepared: &PreparedRevision,
    at: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT OR IGNORE INTO karma_program_revision
            (revision_hash, program_uid, schema_name, ast_json, canonical_dsl,
             proof_json, proof_status, created_at)
         VALUES (?, ?, 'karma.program.v1', ?, ?, ?, ?, ?)",
    )
    .bind(prepared.revision_hash.as_str())
    .bind(program_uid)
    .bind(&prepared.ast_json)
    .bind(&prepared.canonical_dsl)
    .bind(&prepared.proof_json)
    .bind(proof_status_name(prepared.proof.status))
    .bind(at)
    .execute(&mut **tx)
    .await?;
    let owner: Option<String> = sqlx::query_scalar(
        "SELECT program_uid FROM karma_program_revision WHERE revision_hash = ?",
    )
    .bind(prepared.revision_hash.as_str())
    .fetch_optional(&mut **tx)
    .await?;
    if owner.as_deref() != Some(program_uid) {
        return Err(protocol(
            "canonical Program revision hash already belongs to another Program",
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_request(
    tx: &mut Transaction<'_, Sqlite>,
    request_id: &str,
    action: ProgramMutationAction,
    payload_hash: &CanonicalHash,
    program_uid: &str,
    expected_handle_revision: Option<u64>,
    result_handle_revision: u64,
    result_handle: &ProgramHandleRow,
    revision_hash: Option<&CanonicalHash>,
    fact_uid: &str,
    at: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO karma_request (request_id, family, payload_hash, created_at)
         VALUES (?, 'program', ?, ?)",
    )
    .bind(request_id)
    .bind(payload_hash.as_str())
    .bind(at)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO karma_program_request
            (request_id, action, payload_hash, program_uid, expected_handle_revision,
             result_handle_revision, result_json, revision_hash, fact_uid, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(request_id)
    .bind(action_name(action))
    .bind(payload_hash.as_str())
    .bind(program_uid)
    .bind(expected_handle_revision.map(sqlite_revision).transpose()?)
    .bind(sqlite_revision(result_handle_revision)?)
    .bind(canonical_string(result_handle)?)
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
) -> Result<Option<ProgramMutationCommit>, StoreError> {
    let Some(row) = sqlx::query(
        "SELECT request.payload_hash AS global_payload_hash,
                program_request.payload_hash, program_request.program_uid,
                program_request.result_handle_revision, program_request.result_json,
                program_request.fact_uid
         FROM karma_program_request program_request
         JOIN karma_request request ON request.request_id = program_request.request_id
         WHERE program_request.request_id = ? AND request.family = 'program'",
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
            "Karma Program request id was replayed with a different payload",
        ));
    }
    let program_uid: String = row.get("program_uid");
    let fact_uid: String = row.get("fact_uid");
    let result_json: String = row.get("result_json");
    let handle: ProgramHandleRow = serde_json::from_str(&result_json).map_err(json_protocol)?;
    if canonical_string(&handle)? != result_json
        || handle.record_uid != program_uid
        || sqlite_revision(handle.handle_revision)? != row.get::<i64, _>("result_handle_revision")
    {
        return Err(protocol("stored Karma Program request result is invalid"));
    }
    let fact = crate::facts::get_in_transaction(tx, &fact_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    Ok(Some(ProgramMutationCommit::Replayed { handle, fact }))
}

async fn append_evidence_fact<F>(
    tx: &mut Transaction<'_, Sqlite>,
    program_uid: &str,
    delta: nucleus::DecimalValue,
    request_id: &str,
    actor_person_uid: Option<String>,
    evidence: &ProgramMutationEvidence,
    now: DateTime<Utc>,
    sign: &F,
) -> Result<Fact, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let previous = crate::facts::last_hash(tx).await?;
    let payload = canonical_string(evidence)?;
    let mut fact = nucleus::fact::seal(
        NewFact {
            uid: None,
            record_uid: program_uid.to_string(),
            delta,
            at: Some(now),
            actor_uid: actor_person_uid,
            cause: Cause {
                kind: CauseKind::Action,
                uid: Some(request_id.to_string()),
            },
            payload: Some(payload),
        },
        &previous,
        now,
    );
    fact.signature = sign(&fact.hash);
    crate::facts::insert(tx, &fact).await?;
    crate::records::bump_quantity(tx, program_uid, delta, &now.to_rfc3339()).await?;
    Ok(fact)
}

async fn get_handle_tx(
    tx: &mut Transaction<'_, Sqlite>,
    program_uid: &str,
) -> Result<Option<ProgramHandleRow>, StoreError> {
    let row = sqlx::query(
        "SELECT kp.*, record.slug, record.kind
         FROM karma_program kp JOIN record ON record.uid = kp.record_uid
         WHERE kp.record_uid = ? AND record.deleted_at IS NULL",
    )
    .bind(program_uid)
    .fetch_optional(&mut **tx)
    .await?;
    row.map(map_handle).transpose()
}

fn map_handle(row: sqlx::sqlite::SqliteRow) -> Result<ProgramHandleRow, StoreError> {
    if row.get::<String, _>("kind") != RecordKind::Program.as_str() {
        return Err(protocol(
            "Karma Program sidecar is attached to a non-Program Record",
        ));
    }
    let slug = row
        .get::<Option<String>, _>("slug")
        .ok_or_else(|| protocol("Karma Program Record must retain a slug"))?;
    let status_name: String = row.get("status");
    let status = DefinitionStatus::parse(&status_name)
        .ok_or_else(|| protocol("stored Karma Program status is invalid"))?;
    Ok(ProgramHandleRow {
        record_uid: row.get("record_uid"),
        slug,
        handle_revision: rust_revision(row.get("handle_revision"))?,
        status,
        head_revision_hash: parse_hash(row.get("head_revision_hash"))?,
        active_revision_hash: row
            .get::<Option<String>, _>("active_revision_hash")
            .map(parse_hash)
            .transpose()?,
        owner_person_uid: row.get("owner_person_uid"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn map_revision(row: sqlx::sqlite::SqliteRow) -> Result<ProgramRevisionRow, StoreError> {
    let revision_hash = parse_hash(row.get("revision_hash"))?;
    let ast_json: String = row.get("ast_json");
    let program: ProgramAst = serde_json::from_str(&ast_json).map_err(json_protocol)?;
    if canonical_string(&program)? != ast_json {
        return Err(protocol("stored Karma Program AST is not canonical JSON"));
    }
    let computed_hash = canonical_hash(PROGRAM_REVISION_HASH_DOMAIN, &program).map_err(boundary)?;
    if computed_hash != revision_hash {
        return Err(protocol("stored Karma Program revision hash is invalid"));
    }
    let canonical_dsl: String = row.get("canonical_dsl");
    if format_program(&program) != canonical_dsl {
        return Err(protocol("stored Karma Program DSL disagrees with its AST"));
    }
    let proof_json: String = row.get("proof_json");
    let proof: Proof = serde_json::from_str(&proof_json).map_err(json_protocol)?;
    if canonical_string(&proof)? != proof_json || prove_program(&program) != proof {
        return Err(protocol(
            "stored Karma Program Proof disagrees with its AST",
        ));
    }
    let proof_status: String = row.get("proof_status");
    if proof_status != proof_status_name(proof.status) {
        return Err(protocol("stored Karma Program Proof status is invalid"));
    }
    let schema_name: String = row.get("schema_name");
    if schema_name != "karma.program.v1" {
        return Err(protocol("stored Karma Program schema is unsupported"));
    }
    Ok(ProgramRevisionRow {
        revision_hash,
        program_uid: row.get("program_uid"),
        program,
        canonical_dsl,
        proof,
        created_at: row.get("created_at"),
    })
}

#[derive(Serialize)]
struct RequestFingerprint<'a> {
    action: ProgramMutationAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    program_uid: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_handle_revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    revision_hash: Option<&'a CanonicalHash>,
    #[serde(skip_serializing_if = "Option::is_none")]
    owner_person_uid: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor_person_uid: Option<&'a str>,
}

fn request_hash(value: &RequestFingerprint<'_>) -> Result<CanonicalHash, StoreError> {
    canonical_hash(REQUEST_HASH_DOMAIN, value).map_err(boundary)
}

fn validate_request_id(request_id: &str) -> Result<(), StoreError> {
    if request_id.is_empty()
        || request_id.len() > MAX_REQUEST_ID_BYTES
        || request_id.trim() != request_id
        || request_id.chars().any(char::is_control)
    {
        Err(protocol(
            "Karma Program request id must contain 1 to 200 trimmed non-control bytes",
        ))
    } else {
        Ok(())
    }
}

fn validate_person(value: Option<&str>, label: &str) -> Result<(), StoreError> {
    if let Some(value) = value {
        TypedUid::new(ReferenceKind::Person, value.to_string())
            .map_err(|error| protocol(format!("invalid Program {label}: {error}")))?;
    }
    Ok(())
}

fn validate_program_uid(value: &str) -> Result<(), StoreError> {
    TypedUid::new(ReferenceKind::Program, value.to_string())
        .map(|_| ())
        .map_err(|error| protocol(format!("invalid Program uid: {error}")))
}

fn canonical_time(value: DateTime<Utc>) -> Result<DateTime<Utc>, StoreError> {
    Utc.timestamp_millis_opt(value.timestamp_millis())
        .single()
        .ok_or_else(|| protocol("Karma Program timestamp is outside the supported range"))
}

fn canonical_string<T: Serialize>(value: &T) -> Result<String, StoreError> {
    String::from_utf8(canonical_json_bytes(value).map_err(boundary)?)
        .map_err(|error| protocol(error.to_string()))
}

fn checked_next_revision(current: u64) -> Result<u64, StoreError> {
    current
        .checked_add(1)
        .ok_or_else(|| protocol("Karma Program handle revision overflowed"))
}

fn sqlite_revision(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| protocol("Karma Program revision exceeds SQLite range"))
}

fn rust_revision(value: i64) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|_| protocol("stored Karma Program revision is negative"))
}

async fn current_revision_tx(
    tx: &mut Transaction<'_, Sqlite>,
    program_uid: &str,
) -> Result<u64, StoreError> {
    let value: i64 =
        sqlx::query_scalar("SELECT handle_revision FROM karma_program WHERE record_uid = ?")
            .bind(program_uid)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
    rust_revision(value)
}

fn parse_hash(value: String) -> Result<CanonicalHash, StoreError> {
    CanonicalHash::parse(value).map_err(boundary)
}

fn proof_status_name(status: ProofStatus) -> &'static str {
    match status {
        ProofStatus::Accepted => "accepted",
        ProofStatus::Rejected => "rejected",
    }
}

fn action_name(action: ProgramMutationAction) -> &'static str {
    match action {
        ProgramMutationAction::Create => "create",
        ProgramMutationAction::Revise => "revise",
        ProgramMutationAction::Activate => "activate",
        ProgramMutationAction::Pause => "pause",
    }
}

fn protocol(message: impl Into<String>) -> StoreError {
    sqlx::Error::Protocol(message.into())
}

fn boundary(error: nucleus::karma::KarmaBoundaryError) -> StoreError {
    protocol(error.to_string())
}

fn json_protocol(error: serde_json::Error) -> StoreError {
    protocol(error.to_string())
}
