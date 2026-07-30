use std::collections::BTreeMap;

use chrono::{DateTime, TimeZone, Utc};
use nucleus::karma::DispatcherResourceGrant;
use nucleus::karma::{
    CanonicalHash, CompiledFrequency, DefinitionStatus, FrequencyActivationCause,
    FrequencyActivationEpoch, FrequencyAst, FrequencyMutationAction, FrequencyMutationEvidence,
    FrequencyMutationEvidenceSchema, FrequencyParameterValue, HostTimerCapabilities, LocalId,
    ReferenceKind, ScheduleDemandCapacity, TimeZoneProvider, TimestampMs, TypedUid, canonical_hash,
    canonical_json_bytes, format_frequency, parse_frequency,
};
use nucleus::{Cause, CauseKind, Fact, NewFact, RecordKind};
use serde::{Deserialize, Serialize};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use super::schedules;
use crate::StoreError;

const REQUEST_HASH_DOMAIN: &str = "karma.frequency-request.v1";
const MAX_REQUEST_ID_BYTES: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrequencyHandleRow {
    pub record_uid: String,
    pub slug: String,
    pub handle_revision: u64,
    pub status: DefinitionStatus,
    pub head_revision_hash: CanonicalHash,
    pub active_revision_hash: Option<CanonicalHash>,
    pub active_activation_hash: Option<CanonicalHash>,
    pub latest_activation_hash: Option<CanonicalHash>,
    pub owner_person_uid: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrequencyRevisionRow {
    pub revision_hash: CanonicalHash,
    pub frequency_uid: String,
    pub frequency: FrequencyAst,
    pub canonical_dsl: String,
    pub default_compiled: CompiledFrequency,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrequencyActivationRow {
    pub activation_hash: CanonicalHash,
    pub epoch: FrequencyActivationEpoch,
}

#[derive(Debug, Clone)]
pub struct CreateFrequencyInput {
    pub request_id: String,
    pub frequency: FrequencyAst,
    pub owner_person_uid: Option<String>,
    pub actor_person_uid: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ReviseFrequencyInput {
    pub request_id: String,
    pub frequency_uid: String,
    pub expected_handle_revision: u64,
    pub frequency: FrequencyAst,
    pub actor_person_uid: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ActivateFrequencyInput {
    pub request_id: String,
    pub frequency_uid: String,
    pub expected_handle_revision: u64,
    pub revision_hash: CanonicalHash,
    pub parameter_overrides: BTreeMap<LocalId, FrequencyParameterValue>,
    pub actor_person_uid: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SetFrequencyParametersInput {
    pub request_id: String,
    pub frequency_uid: String,
    pub expected_handle_revision: u64,
    pub expected_active_revision_hash: CanonicalHash,
    pub parameter_overrides: BTreeMap<LocalId, FrequencyParameterValue>,
    pub actor_person_uid: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResetFrequencyParametersInput {
    pub request_id: String,
    pub frequency_uid: String,
    pub expected_handle_revision: u64,
    pub expected_active_revision_hash: CanonicalHash,
    pub actor_person_uid: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PauseFrequencyInput {
    pub request_id: String,
    pub frequency_uid: String,
    pub expected_handle_revision: u64,
    pub actor_person_uid: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FrequencyMutationCommit {
    Committed {
        handle: FrequencyHandleRow,
        fact: Fact,
    },
    Replayed {
        handle: FrequencyHandleRow,
        fact: Fact,
    },
    Stale {
        current_handle_revision: u64,
    },
}

#[derive(Clone, Copy)]
pub struct FrequencyRuntimeAdmission<'a> {
    pub host: &'a HostTimerCapabilities,
    pub grant: &'a DispatcherResourceGrant,
    pub calendar_provider: Option<&'a dyn TimeZoneProvider>,
    pub demand_policy: schedules::ScheduleDemandPolicy,
    pub demand_capacity: ScheduleDemandCapacity,
}

pub async fn create<F>(
    pool: &SqlitePool,
    input: CreateFrequencyInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<FrequencyMutationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    validate_request_id(&input.request_id)?;
    validate_person(input.owner_person_uid.as_deref(), "owner")?;
    validate_person(input.actor_person_uid.as_deref(), "actor")?;
    let prepared = PreparedRevision::new(&input.frequency)?;
    let fingerprint = request_hash(&RequestFingerprint {
        action: FrequencyMutationAction::Create,
        frequency_uid: None,
        expected_handle_revision: None,
        revision_hash: Some(&prepared.revision_hash),
        parameter_overrides: None,
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

    let frequency_uid = nucleus::new_uid("r");
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
    .bind(&frequency_uid)
    .bind(input.frequency.slug.as_str())
    .bind(RecordKind::Frequency.as_str())
    .bind(&input.frequency.purpose)
    .bind(&at)
    .bind(&at)
    .bind(origin_organ_uid)
    .execute(&mut *tx)
    .await?;

    insert_revision(&mut tx, &frequency_uid, &prepared, &at).await?;
    sqlx::query(
        "INSERT INTO karma_frequency
            (record_uid, handle_revision, status, head_revision_hash,
             active_revision_hash, active_activation_hash, latest_activation_hash, owner_person_uid,
             created_at, updated_at)
         VALUES (?, 1, 'proven', ?, NULL, NULL, NULL, ?, ?, ?)",
    )
    .bind(&frequency_uid)
    .bind(prepared.revision_hash.as_str())
    .bind(&input.owner_person_uid)
    .bind(&at)
    .bind(&at)
    .execute(&mut *tx)
    .await?;

    let evidence = FrequencyMutationEvidence {
        schema: FrequencyMutationEvidenceSchema::V1,
        action: FrequencyMutationAction::Create,
        request_id: input.request_id.clone(),
        frequency_uid: frequency_uid.clone(),
        actor_person_uid: input.actor_person_uid.clone(),
        previous_handle_revision: None,
        handle_revision: 1,
        previous_head_revision_hash: None,
        head_revision_hash: prepared.revision_hash.clone(),
        previous_active_revision_hash: None,
        active_revision_hash: None,
        previous_activation_hash: None,
        activation_hash: None,
        previous_effective_parameter_hash: None,
        effective_parameter_hash: None,
        activated_at: None,
        status: DefinitionStatus::Proven,
    };
    let fact = append_evidence_fact(
        &mut tx,
        &frequency_uid,
        crate::exact::zero(),
        &input.request_id,
        input.actor_person_uid,
        &evidence,
        now,
        &sign,
    )
    .await?;
    let handle = get_handle_tx(&mut tx, &frequency_uid)
        .await?
        .expect("new Frequency handle exists");
    insert_request(
        &mut tx,
        &input.request_id,
        FrequencyMutationAction::Create,
        &fingerprint,
        &frequency_uid,
        None,
        1,
        &handle,
        Some(&prepared.revision_hash),
        None,
        &fact.uid,
        &at,
    )
    .await?;
    tx.commit().await?;
    Ok(FrequencyMutationCommit::Committed { handle, fact })
}

pub async fn revise<F>(
    pool: &SqlitePool,
    input: ReviseFrequencyInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<FrequencyMutationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    validate_request_id(&input.request_id)?;
    validate_frequency_uid(&input.frequency_uid)?;
    validate_person(input.actor_person_uid.as_deref(), "actor")?;
    let prepared = PreparedRevision::new(&input.frequency)?;
    let fingerprint = request_hash(&RequestFingerprint {
        action: FrequencyMutationAction::Revise,
        frequency_uid: Some(&input.frequency_uid),
        expected_handle_revision: Some(input.expected_handle_revision),
        revision_hash: Some(&prepared.revision_hash),
        parameter_overrides: None,
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
    let current = get_handle_tx(&mut tx, &input.frequency_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    if current.handle_revision != input.expected_handle_revision {
        tx.rollback().await?;
        return Ok(FrequencyMutationCommit::Stale {
            current_handle_revision: current.handle_revision,
        });
    }
    if current.status == DefinitionStatus::Retired {
        return Err(protocol("retired Karma Frequencies cannot be revised"));
    }
    if current.head_revision_hash == prepared.revision_hash {
        return Err(protocol(
            "Karma Frequency revision does not change canonical content",
        ));
    }
    let next_revision = checked_next_revision(current.handle_revision)?;
    insert_revision(&mut tx, &input.frequency_uid, &prepared, &at).await?;
    let status = if current.active_activation_hash.is_some() {
        DefinitionStatus::Active
    } else if current.status == DefinitionStatus::Paused {
        DefinitionStatus::Paused
    } else {
        DefinitionStatus::Proven
    };
    let updated = sqlx::query(
        "UPDATE karma_frequency
         SET handle_revision = ?, status = ?, head_revision_hash = ?, updated_at = ?
         WHERE record_uid = ? AND handle_revision = ? AND status != 'retired'",
    )
    .bind(sqlite_revision(next_revision)?)
    .bind(status.as_str())
    .bind(prepared.revision_hash.as_str())
    .bind(&at)
    .bind(&input.frequency_uid)
    .bind(sqlite_revision(current.handle_revision)?)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        let actual = current_revision_tx(&mut tx, &input.frequency_uid).await?;
        tx.rollback().await?;
        return Ok(FrequencyMutationCommit::Stale {
            current_handle_revision: actual,
        });
    }
    sqlx::query("UPDATE record SET slug = ?, head = ?, updated_at = ? WHERE uid = ?")
        .bind(input.frequency.slug.as_str())
        .bind(&input.frequency.purpose)
        .bind(&at)
        .bind(&input.frequency_uid)
        .execute(&mut *tx)
        .await?;
    let previous_parameter_hash =
        active_parameter_hash_tx(&mut tx, current.active_activation_hash.as_ref()).await?;
    let evidence = FrequencyMutationEvidence {
        schema: FrequencyMutationEvidenceSchema::V1,
        action: FrequencyMutationAction::Revise,
        request_id: input.request_id.clone(),
        frequency_uid: input.frequency_uid.clone(),
        actor_person_uid: input.actor_person_uid.clone(),
        previous_handle_revision: Some(current.handle_revision),
        handle_revision: next_revision,
        previous_head_revision_hash: Some(current.head_revision_hash),
        head_revision_hash: prepared.revision_hash.clone(),
        previous_active_revision_hash: current.active_revision_hash.clone(),
        active_revision_hash: current.active_revision_hash,
        previous_activation_hash: current.active_activation_hash.clone(),
        activation_hash: current.active_activation_hash,
        previous_effective_parameter_hash: previous_parameter_hash.clone(),
        effective_parameter_hash: previous_parameter_hash,
        activated_at: None,
        status,
    };
    let fact = append_evidence_fact(
        &mut tx,
        &input.frequency_uid,
        crate::exact::zero(),
        &input.request_id,
        input.actor_person_uid,
        &evidence,
        now,
        &sign,
    )
    .await?;
    let handle = get_handle_tx(&mut tx, &input.frequency_uid)
        .await?
        .expect("revised Frequency exists");
    insert_request(
        &mut tx,
        &input.request_id,
        FrequencyMutationAction::Revise,
        &fingerprint,
        &input.frequency_uid,
        Some(input.expected_handle_revision),
        next_revision,
        &handle,
        Some(&prepared.revision_hash),
        current_activation(&handle),
        &fact.uid,
        &at,
    )
    .await?;
    tx.commit().await?;
    Ok(FrequencyMutationCommit::Committed { handle, fact })
}

pub async fn activate<F>(
    pool: &SqlitePool,
    input: ActivateFrequencyInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<FrequencyMutationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    activate_with_admission(pool, input, None, now, sign).await
}

pub async fn activate_admitted<F>(
    pool: &SqlitePool,
    input: ActivateFrequencyInput,
    admission: FrequencyRuntimeAdmission<'_>,
    now: DateTime<Utc>,
    sign: F,
) -> Result<FrequencyMutationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    activate_with_admission(pool, input, Some(admission), now, sign).await
}

async fn activate_with_admission<F>(
    pool: &SqlitePool,
    input: ActivateFrequencyInput,
    admission: Option<FrequencyRuntimeAdmission<'_>>,
    now: DateTime<Utc>,
    sign: F,
) -> Result<FrequencyMutationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    validate_request_id(&input.request_id)?;
    validate_frequency_uid(&input.frequency_uid)?;
    validate_person(input.actor_person_uid.as_deref(), "actor")?;
    let fingerprint = request_hash(&RequestFingerprint {
        action: FrequencyMutationAction::Activate,
        frequency_uid: Some(&input.frequency_uid),
        expected_handle_revision: Some(input.expected_handle_revision),
        revision_hash: Some(&input.revision_hash),
        parameter_overrides: Some(&input.parameter_overrides),
        owner_person_uid: None,
        actor_person_uid: input.actor_person_uid.as_deref(),
    })?;
    mutate_epoch(
        pool,
        &input.request_id,
        &input.frequency_uid,
        input.expected_handle_revision,
        &input.revision_hash,
        input.parameter_overrides,
        FrequencyMutationAction::Activate,
        FrequencyActivationCause::ActivateRevision,
        input.actor_person_uid,
        fingerprint,
        admission,
        now,
        &sign,
    )
    .await
}

pub async fn set_parameters<F>(
    pool: &SqlitePool,
    input: SetFrequencyParametersInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<FrequencyMutationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    set_parameters_with_admission(pool, input, None, now, sign).await
}

pub async fn set_parameters_admitted<F>(
    pool: &SqlitePool,
    input: SetFrequencyParametersInput,
    admission: FrequencyRuntimeAdmission<'_>,
    now: DateTime<Utc>,
    sign: F,
) -> Result<FrequencyMutationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    set_parameters_with_admission(pool, input, Some(admission), now, sign).await
}

async fn set_parameters_with_admission<F>(
    pool: &SqlitePool,
    input: SetFrequencyParametersInput,
    admission: Option<FrequencyRuntimeAdmission<'_>>,
    now: DateTime<Utc>,
    sign: F,
) -> Result<FrequencyMutationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    validate_request_id(&input.request_id)?;
    validate_frequency_uid(&input.frequency_uid)?;
    validate_person(input.actor_person_uid.as_deref(), "actor")?;
    let fingerprint = request_hash(&RequestFingerprint {
        action: FrequencyMutationAction::SetParameters,
        frequency_uid: Some(&input.frequency_uid),
        expected_handle_revision: Some(input.expected_handle_revision),
        revision_hash: Some(&input.expected_active_revision_hash),
        parameter_overrides: Some(&input.parameter_overrides),
        owner_person_uid: None,
        actor_person_uid: input.actor_person_uid.as_deref(),
    })?;
    mutate_epoch(
        pool,
        &input.request_id,
        &input.frequency_uid,
        input.expected_handle_revision,
        &input.expected_active_revision_hash,
        input.parameter_overrides,
        FrequencyMutationAction::SetParameters,
        FrequencyActivationCause::SetParameters,
        input.actor_person_uid,
        fingerprint,
        admission,
        now,
        &sign,
    )
    .await
}

pub async fn reset_parameters<F>(
    pool: &SqlitePool,
    input: ResetFrequencyParametersInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<FrequencyMutationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    reset_parameters_with_admission(pool, input, None, now, sign).await
}

pub async fn reset_parameters_admitted<F>(
    pool: &SqlitePool,
    input: ResetFrequencyParametersInput,
    admission: FrequencyRuntimeAdmission<'_>,
    now: DateTime<Utc>,
    sign: F,
) -> Result<FrequencyMutationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    reset_parameters_with_admission(pool, input, Some(admission), now, sign).await
}

async fn reset_parameters_with_admission<F>(
    pool: &SqlitePool,
    input: ResetFrequencyParametersInput,
    admission: Option<FrequencyRuntimeAdmission<'_>>,
    now: DateTime<Utc>,
    sign: F,
) -> Result<FrequencyMutationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    validate_request_id(&input.request_id)?;
    validate_frequency_uid(&input.frequency_uid)?;
    validate_person(input.actor_person_uid.as_deref(), "actor")?;
    let empty = BTreeMap::new();
    let fingerprint = request_hash(&RequestFingerprint {
        action: FrequencyMutationAction::ResetParameters,
        frequency_uid: Some(&input.frequency_uid),
        expected_handle_revision: Some(input.expected_handle_revision),
        revision_hash: Some(&input.expected_active_revision_hash),
        parameter_overrides: Some(&empty),
        owner_person_uid: None,
        actor_person_uid: input.actor_person_uid.as_deref(),
    })?;
    mutate_epoch(
        pool,
        &input.request_id,
        &input.frequency_uid,
        input.expected_handle_revision,
        &input.expected_active_revision_hash,
        empty,
        FrequencyMutationAction::ResetParameters,
        FrequencyActivationCause::ResetParameters,
        input.actor_person_uid,
        fingerprint,
        admission,
        now,
        &sign,
    )
    .await
}

pub async fn pause<F>(
    pool: &SqlitePool,
    input: PauseFrequencyInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<FrequencyMutationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    validate_request_id(&input.request_id)?;
    validate_frequency_uid(&input.frequency_uid)?;
    validate_person(input.actor_person_uid.as_deref(), "actor")?;
    let fingerprint = request_hash(&RequestFingerprint {
        action: FrequencyMutationAction::Pause,
        frequency_uid: Some(&input.frequency_uid),
        expected_handle_revision: Some(input.expected_handle_revision),
        revision_hash: None,
        parameter_overrides: None,
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
    let current = get_handle_tx(&mut tx, &input.frequency_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    if current.handle_revision != input.expected_handle_revision {
        tx.rollback().await?;
        return Ok(FrequencyMutationCommit::Stale {
            current_handle_revision: current.handle_revision,
        });
    }
    if current.status == DefinitionStatus::Retired {
        return Err(protocol(
            "retired Karma Frequencies cannot change activation",
        ));
    }
    let Some(previous_activation_hash) = current.active_activation_hash.clone() else {
        return Err(protocol("Karma Frequency is not active"));
    };
    let previous = get_activation_tx(&mut tx, &previous_activation_hash)
        .await?
        .ok_or_else(|| protocol("active Karma Frequency activation is missing"))?;
    let next_revision = checked_next_revision(current.handle_revision)?;
    let updated = sqlx::query(
        "UPDATE karma_frequency
         SET handle_revision = ?, status = 'paused', active_revision_hash = NULL,
             active_activation_hash = NULL, updated_at = ?
         WHERE record_uid = ? AND handle_revision = ? AND status = 'active'",
    )
    .bind(sqlite_revision(next_revision)?)
    .bind(&at)
    .bind(&input.frequency_uid)
    .bind(sqlite_revision(current.handle_revision)?)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        let actual = current_revision_tx(&mut tx, &input.frequency_uid).await?;
        tx.rollback().await?;
        return Ok(FrequencyMutationCommit::Stale {
            current_handle_revision: actual,
        });
    }
    schedules::supersede_frequency_cursors_tx(&mut tx, &input.frequency_uid, &at).await?;
    let evidence = FrequencyMutationEvidence {
        schema: FrequencyMutationEvidenceSchema::V1,
        action: FrequencyMutationAction::Pause,
        request_id: input.request_id.clone(),
        frequency_uid: input.frequency_uid.clone(),
        actor_person_uid: input.actor_person_uid.clone(),
        previous_handle_revision: Some(current.handle_revision),
        handle_revision: next_revision,
        previous_head_revision_hash: Some(current.head_revision_hash.clone()),
        head_revision_hash: current.head_revision_hash,
        previous_active_revision_hash: current.active_revision_hash,
        active_revision_hash: None,
        previous_activation_hash: Some(previous_activation_hash),
        activation_hash: None,
        previous_effective_parameter_hash: Some(previous.epoch.effective_parameter_hash().clone()),
        effective_parameter_hash: None,
        activated_at: None,
        status: DefinitionStatus::Paused,
    };
    let fact = append_evidence_fact(
        &mut tx,
        &input.frequency_uid,
        crate::exact::integer(-1),
        &input.request_id,
        input.actor_person_uid,
        &evidence,
        now,
        &sign,
    )
    .await?;
    let handle = get_handle_tx(&mut tx, &input.frequency_uid)
        .await?
        .expect("paused Frequency exists");
    insert_request(
        &mut tx,
        &input.request_id,
        FrequencyMutationAction::Pause,
        &fingerprint,
        &input.frequency_uid,
        Some(input.expected_handle_revision),
        next_revision,
        &handle,
        None,
        None,
        &fact.uid,
        &at,
    )
    .await?;
    tx.commit().await?;
    Ok(FrequencyMutationCommit::Committed { handle, fact })
}

pub async fn get_handle(
    pool: &SqlitePool,
    frequency_uid: &str,
) -> Result<Option<FrequencyHandleRow>, StoreError> {
    let row = sqlx::query(
        "SELECT kf.*, record.slug, record.kind
         FROM karma_frequency kf JOIN record ON record.uid = kf.record_uid
         WHERE kf.record_uid = ? AND record.deleted_at IS NULL",
    )
    .bind(frequency_uid)
    .fetch_optional(pool)
    .await?;
    row.map(map_handle).transpose()
}

pub async fn list_handles(pool: &SqlitePool) -> Result<Vec<FrequencyHandleRow>, StoreError> {
    sqlx::query(
        "SELECT kf.*, record.slug, record.kind
         FROM karma_frequency kf JOIN record ON record.uid = kf.record_uid
         WHERE record.deleted_at IS NULL
         ORDER BY record.slug, kf.record_uid",
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
) -> Result<Option<FrequencyRevisionRow>, StoreError> {
    let row = sqlx::query("SELECT * FROM karma_frequency_revision WHERE revision_hash = ?")
        .bind(revision_hash.as_str())
        .fetch_optional(pool)
        .await?;
    row.map(map_revision).transpose()
}

pub async fn list_revisions(pool: &SqlitePool) -> Result<Vec<FrequencyRevisionRow>, StoreError> {
    sqlx::query(
        "SELECT * FROM karma_frequency_revision
         ORDER BY frequency_uid, created_at, revision_hash",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_revision)
    .collect()
}

pub async fn get_activation(
    pool: &SqlitePool,
    activation_hash: &CanonicalHash,
) -> Result<Option<FrequencyActivationRow>, StoreError> {
    let row = sqlx::query(
        "SELECT activation.*, revision.ast_json AS definition_ast_json
         FROM karma_frequency_activation activation
         JOIN karma_frequency_revision revision
           ON revision.revision_hash = activation.definition_revision_hash
         WHERE activation.activation_hash = ?",
    )
    .bind(activation_hash.as_str())
    .fetch_optional(pool)
    .await?;
    row.map(map_activation).transpose()
}

pub async fn list_activations(
    pool: &SqlitePool,
) -> Result<Vec<FrequencyActivationRow>, StoreError> {
    sqlx::query(
        "SELECT activation.*, revision.ast_json AS definition_ast_json
         FROM karma_frequency_activation activation
         JOIN karma_frequency_revision revision
           ON revision.revision_hash = activation.definition_revision_hash
         ORDER BY activation.frequency_uid, activation.activating_handle_revision,
                  activation.activation_hash",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_activation)
    .collect()
}

#[allow(clippy::too_many_arguments)]
async fn mutate_epoch<F>(
    pool: &SqlitePool,
    request_id: &str,
    frequency_uid: &str,
    expected_handle_revision: u64,
    selected_revision_hash: &CanonicalHash,
    parameter_overrides: BTreeMap<LocalId, FrequencyParameterValue>,
    action: FrequencyMutationAction,
    cause: FrequencyActivationCause,
    actor_person_uid: Option<String>,
    fingerprint: CanonicalHash,
    admission: Option<FrequencyRuntimeAdmission<'_>>,
    now: DateTime<Utc>,
    sign: &F,
) -> Result<FrequencyMutationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let now = canonical_time(now)?;
    let at = now.to_rfc3339();
    let activated_at = TimestampMs::from_millis(now.timestamp_millis()).map_err(boundary)?;
    let mut tx = pool.begin().await?;
    if let Some(commit) = replay_request(&mut tx, request_id, &fingerprint).await? {
        tx.rollback().await?;
        return Ok(commit);
    }
    let current = get_handle_tx(&mut tx, frequency_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    if current.handle_revision != expected_handle_revision {
        tx.rollback().await?;
        return Ok(FrequencyMutationCommit::Stale {
            current_handle_revision: current.handle_revision,
        });
    }
    if current.status == DefinitionStatus::Retired {
        return Err(protocol(
            "retired Karma Frequencies cannot change activation",
        ));
    }
    if action != FrequencyMutationAction::Activate {
        if current.status != DefinitionStatus::Active
            || current.active_revision_hash.as_ref() != Some(selected_revision_hash)
        {
            return Err(protocol(
                "Karma Frequency parameter mutation requires the expected active revision",
            ));
        }
    }
    let definition = get_revision_tx(&mut tx, selected_revision_hash)
        .await?
        .ok_or_else(|| protocol("Karma Frequency activation revision does not exist"))?;
    if definition.frequency_uid != frequency_uid {
        return Err(protocol(
            "Karma Frequency activation revision belongs to another Frequency",
        ));
    }
    let compiled = definition
        .frequency
        .compile(&parameter_overrides)
        .map_err(|error| protocol(error.to_string()))?;
    let was_active = current.active_activation_hash.is_some();
    let previous_activation = match current.latest_activation_hash.as_ref() {
        Some(hash) => Some(
            get_activation_tx(&mut tx, hash)
                .await?
                .ok_or_else(|| protocol("active Karma Frequency activation is missing"))?,
        ),
        None => None,
    };
    if was_active {
        if let Some(previous) = &previous_activation {
            if previous.epoch.definition_revision_hash() == selected_revision_hash
                && previous.epoch.compiled() == &compiled
            {
                return Err(protocol(
                    "Karma Frequency activation does not change effective configuration",
                ));
            }
        }
    }

    let next_revision = checked_next_revision(current.handle_revision)?;
    let epoch = FrequencyActivationEpoch::new(
        frequency_uid.to_string(),
        next_revision,
        selected_revision_hash.clone(),
        compiled,
        current.latest_activation_hash.clone(),
        cause,
        activated_at,
    )
    .map_err(boundary)?;
    let activation_hash = epoch.activation_hash().map_err(boundary)?;
    insert_activation(&mut tx, &activation_hash, &epoch).await?;
    let updated = sqlx::query(
        "UPDATE karma_frequency
         SET handle_revision = ?, status = 'active', active_revision_hash = ?,
             active_activation_hash = ?, latest_activation_hash = ?, updated_at = ?
         WHERE record_uid = ? AND handle_revision = ? AND status != 'retired'",
    )
    .bind(sqlite_revision(next_revision)?)
    .bind(selected_revision_hash.as_str())
    .bind(activation_hash.as_str())
    .bind(activation_hash.as_str())
    .bind(&at)
    .bind(frequency_uid)
    .bind(sqlite_revision(current.handle_revision)?)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        let actual = current_revision_tx(&mut tx, frequency_uid).await?;
        tx.rollback().await?;
        return Ok(FrequencyMutationCommit::Stale {
            current_handle_revision: actual,
        });
    }
    if let Some(admission) = admission {
        schedules::install_admitted_activation_cursor_tx(
            &mut tx,
            &activation_hash,
            &epoch,
            admission.calendar_provider,
            admission.demand_policy,
            admission.demand_capacity,
            admission.host,
            admission.grant,
            &at,
        )
        .await?;
    }
    let previous_effective_parameter_hash = previous_activation
        .as_ref()
        .map(|row| row.epoch.effective_parameter_hash().clone());
    let evidence = FrequencyMutationEvidence {
        schema: FrequencyMutationEvidenceSchema::V1,
        action,
        request_id: request_id.to_string(),
        frequency_uid: frequency_uid.to_string(),
        actor_person_uid: actor_person_uid.clone(),
        previous_handle_revision: Some(current.handle_revision),
        handle_revision: next_revision,
        previous_head_revision_hash: Some(current.head_revision_hash.clone()),
        head_revision_hash: current.head_revision_hash,
        previous_active_revision_hash: current.active_revision_hash,
        active_revision_hash: Some(selected_revision_hash.clone()),
        previous_activation_hash: current.latest_activation_hash,
        activation_hash: Some(activation_hash.clone()),
        previous_effective_parameter_hash,
        effective_parameter_hash: Some(epoch.effective_parameter_hash().clone()),
        activated_at: Some(activated_at),
        status: DefinitionStatus::Active,
    };
    let delta = crate::exact::integer(if was_active { 0 } else { 1 });
    let fact = append_evidence_fact(
        &mut tx,
        frequency_uid,
        delta,
        request_id,
        actor_person_uid,
        &evidence,
        now,
        sign,
    )
    .await?;
    let handle = get_handle_tx(&mut tx, frequency_uid)
        .await?
        .expect("activated Frequency exists");
    insert_request(
        &mut tx,
        request_id,
        action,
        &fingerprint,
        frequency_uid,
        Some(expected_handle_revision),
        next_revision,
        &handle,
        Some(selected_revision_hash),
        Some(&activation_hash),
        &fact.uid,
        &at,
    )
    .await?;
    tx.commit().await?;
    Ok(FrequencyMutationCommit::Committed { handle, fact })
}

struct PreparedRevision {
    revision_hash: CanonicalHash,
    ast_json: String,
    canonical_dsl: String,
    default_compiled_json: String,
}

impl PreparedRevision {
    fn new(frequency: &FrequencyAst) -> Result<Self, StoreError> {
        let default_compiled = frequency
            .compile(&BTreeMap::new())
            .map_err(|error| protocol(error.to_string()))?;
        Ok(Self {
            revision_hash: default_compiled.revision_hash.clone(),
            ast_json: canonical_string(frequency)?,
            canonical_dsl: format_frequency(frequency),
            default_compiled_json: canonical_string(&default_compiled)?,
        })
    }
}

async fn insert_revision(
    tx: &mut Transaction<'_, Sqlite>,
    frequency_uid: &str,
    prepared: &PreparedRevision,
    at: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT OR IGNORE INTO karma_frequency_revision
            (revision_hash, frequency_uid, schema_name, ast_json, canonical_dsl,
             default_compiled_json, created_at)
         VALUES (?, ?, 'karma.frequency.v1', ?, ?, ?, ?)",
    )
    .bind(prepared.revision_hash.as_str())
    .bind(frequency_uid)
    .bind(&prepared.ast_json)
    .bind(&prepared.canonical_dsl)
    .bind(&prepared.default_compiled_json)
    .bind(at)
    .execute(&mut **tx)
    .await?;
    let owner: Option<String> = sqlx::query_scalar(
        "SELECT frequency_uid FROM karma_frequency_revision WHERE revision_hash = ?",
    )
    .bind(prepared.revision_hash.as_str())
    .fetch_optional(&mut **tx)
    .await?;
    if owner.as_deref() != Some(frequency_uid) {
        return Err(protocol(
            "canonical Frequency revision hash already belongs to another Frequency",
        ));
    }
    Ok(())
}

async fn insert_activation(
    tx: &mut Transaction<'_, Sqlite>,
    activation_hash: &CanonicalHash,
    epoch: &FrequencyActivationEpoch,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO karma_frequency_activation
            (activation_hash, frequency_uid, activating_handle_revision,
             definition_revision_hash, effective_parameter_hash,
             effective_parameters_json, compiled_json, epoch_json,
             previous_activation_hash, cause_action, activated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(activation_hash.as_str())
    .bind(epoch.frequency_uid())
    .bind(sqlite_revision(epoch.activating_handle_revision())?)
    .bind(epoch.definition_revision_hash().as_str())
    .bind(epoch.effective_parameter_hash().as_str())
    .bind(canonical_string(epoch.effective_parameters())?)
    .bind(canonical_string(epoch.compiled())?)
    .bind(canonical_string(epoch)?)
    .bind(epoch.previous_activation_hash().map(CanonicalHash::as_str))
    .bind(activation_cause_name(epoch.cause()))
    .bind(epoch.activated_at().to_string())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_request(
    tx: &mut Transaction<'_, Sqlite>,
    request_id: &str,
    action: FrequencyMutationAction,
    payload_hash: &CanonicalHash,
    frequency_uid: &str,
    expected_handle_revision: Option<u64>,
    result_handle_revision: u64,
    result_handle: &FrequencyHandleRow,
    revision_hash: Option<&CanonicalHash>,
    activation_hash: Option<&CanonicalHash>,
    fact_uid: &str,
    at: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO karma_request (request_id, family, payload_hash, created_at)
         VALUES (?, 'frequency', ?, ?)",
    )
    .bind(request_id)
    .bind(payload_hash.as_str())
    .bind(at)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO karma_frequency_request
            (request_id, action, payload_hash, frequency_uid, expected_handle_revision,
             result_handle_revision, result_json, revision_hash, activation_hash,
             fact_uid, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(request_id)
    .bind(action_name(action))
    .bind(payload_hash.as_str())
    .bind(frequency_uid)
    .bind(expected_handle_revision.map(sqlite_revision).transpose()?)
    .bind(sqlite_revision(result_handle_revision)?)
    .bind(canonical_string(result_handle)?)
    .bind(revision_hash.map(CanonicalHash::as_str))
    .bind(activation_hash.map(CanonicalHash::as_str))
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
) -> Result<Option<FrequencyMutationCommit>, StoreError> {
    let Some(row) = sqlx::query(
        "SELECT request.payload_hash AS global_payload_hash,
                frequency_request.payload_hash, frequency_request.frequency_uid,
                frequency_request.result_handle_revision, frequency_request.result_json,
                frequency_request.fact_uid
         FROM karma_frequency_request frequency_request
         JOIN karma_request request ON request.request_id = frequency_request.request_id
         WHERE frequency_request.request_id = ? AND request.family = 'frequency'",
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
            "Karma Frequency request id was replayed with a different payload",
        ));
    }
    let frequency_uid: String = row.get("frequency_uid");
    let fact_uid: String = row.get("fact_uid");
    let result_json: String = row.get("result_json");
    let handle: FrequencyHandleRow = serde_json::from_str(&result_json).map_err(json_protocol)?;
    if canonical_string(&handle)? != result_json
        || handle.record_uid != frequency_uid
        || sqlite_revision(handle.handle_revision)? != row.get::<i64, _>("result_handle_revision")
    {
        return Err(protocol("stored Karma Frequency request result is invalid"));
    }
    let fact = crate::facts::get_in_transaction(tx, &fact_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    Ok(Some(FrequencyMutationCommit::Replayed { handle, fact }))
}

async fn append_evidence_fact<F>(
    tx: &mut Transaction<'_, Sqlite>,
    frequency_uid: &str,
    delta: nucleus::DecimalValue,
    request_id: &str,
    actor_person_uid: Option<String>,
    evidence: &FrequencyMutationEvidence,
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
            record_uid: frequency_uid.to_string(),
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
    crate::records::bump_quantity(tx, frequency_uid, delta, &now.to_rfc3339()).await?;
    Ok(fact)
}

async fn get_handle_tx(
    tx: &mut Transaction<'_, Sqlite>,
    frequency_uid: &str,
) -> Result<Option<FrequencyHandleRow>, StoreError> {
    let row = sqlx::query(
        "SELECT kf.*, record.slug, record.kind
         FROM karma_frequency kf JOIN record ON record.uid = kf.record_uid
         WHERE kf.record_uid = ? AND record.deleted_at IS NULL",
    )
    .bind(frequency_uid)
    .fetch_optional(&mut **tx)
    .await?;
    row.map(map_handle).transpose()
}

async fn get_revision_tx(
    tx: &mut Transaction<'_, Sqlite>,
    revision_hash: &CanonicalHash,
) -> Result<Option<FrequencyRevisionRow>, StoreError> {
    let row = sqlx::query("SELECT * FROM karma_frequency_revision WHERE revision_hash = ?")
        .bind(revision_hash.as_str())
        .fetch_optional(&mut **tx)
        .await?;
    row.map(map_revision).transpose()
}

async fn get_activation_tx(
    tx: &mut Transaction<'_, Sqlite>,
    activation_hash: &CanonicalHash,
) -> Result<Option<FrequencyActivationRow>, StoreError> {
    let row = sqlx::query(
        "SELECT activation.*, revision.ast_json AS definition_ast_json
         FROM karma_frequency_activation activation
         JOIN karma_frequency_revision revision
           ON revision.revision_hash = activation.definition_revision_hash
         WHERE activation.activation_hash = ?",
    )
    .bind(activation_hash.as_str())
    .fetch_optional(&mut **tx)
    .await?;
    row.map(map_activation).transpose()
}

async fn active_parameter_hash_tx(
    tx: &mut Transaction<'_, Sqlite>,
    activation_hash: Option<&CanonicalHash>,
) -> Result<Option<CanonicalHash>, StoreError> {
    match activation_hash {
        Some(hash) => Ok(Some(
            get_activation_tx(tx, hash)
                .await?
                .ok_or_else(|| protocol("active Karma Frequency activation is missing"))?
                .epoch
                .effective_parameter_hash()
                .clone(),
        )),
        None => Ok(None),
    }
}

fn map_handle(row: sqlx::sqlite::SqliteRow) -> Result<FrequencyHandleRow, StoreError> {
    if row.get::<String, _>("kind") != RecordKind::Frequency.as_str() {
        return Err(protocol(
            "Karma Frequency sidecar is attached to a non-Frequency Record",
        ));
    }
    let slug = row
        .get::<Option<String>, _>("slug")
        .ok_or_else(|| protocol("Karma Frequency Record must retain a slug"))?;
    let status_name: String = row.get("status");
    let status = DefinitionStatus::parse(&status_name)
        .ok_or_else(|| protocol("stored Karma Frequency status is invalid"))?;
    let active_revision_hash = row
        .get::<Option<String>, _>("active_revision_hash")
        .map(parse_hash)
        .transpose()?;
    let active_activation_hash = row
        .get::<Option<String>, _>("active_activation_hash")
        .map(parse_hash)
        .transpose()?;
    let latest_activation_hash = row
        .get::<Option<String>, _>("latest_activation_hash")
        .map(parse_hash)
        .transpose()?;
    if (status == DefinitionStatus::Active)
        != (active_revision_hash.is_some() && active_activation_hash.is_some())
        || active_revision_hash.is_some() != active_activation_hash.is_some()
        || (active_activation_hash.is_some() && active_activation_hash != latest_activation_hash)
    {
        return Err(protocol(
            "stored Karma Frequency activation status is inconsistent",
        ));
    }
    Ok(FrequencyHandleRow {
        record_uid: row.get("record_uid"),
        slug,
        handle_revision: rust_revision(row.get("handle_revision"))?,
        status,
        head_revision_hash: parse_hash(row.get("head_revision_hash"))?,
        active_revision_hash,
        active_activation_hash,
        latest_activation_hash,
        owner_person_uid: row.get("owner_person_uid"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn map_revision(row: sqlx::sqlite::SqliteRow) -> Result<FrequencyRevisionRow, StoreError> {
    let revision_hash = parse_hash(row.get("revision_hash"))?;
    let ast_json: String = row.get("ast_json");
    let frequency: FrequencyAst = serde_json::from_str(&ast_json).map_err(json_protocol)?;
    if canonical_string(&frequency)? != ast_json {
        return Err(protocol("stored Karma Frequency AST is not canonical JSON"));
    }
    let default_compiled = frequency
        .compile(&BTreeMap::new())
        .map_err(|error| protocol(error.to_string()))?;
    if default_compiled.revision_hash != revision_hash {
        return Err(protocol("stored Karma Frequency revision hash is invalid"));
    }
    let canonical_dsl: String = row.get("canonical_dsl");
    if format_frequency(&frequency) != canonical_dsl {
        return Err(protocol(
            "stored Karma Frequency DSL disagrees with its AST",
        ));
    }
    let parsed = parse_frequency(&canonical_dsl).map_err(|error| protocol(error.to_string()))?;
    if parsed != frequency {
        return Err(protocol(
            "stored Karma Frequency DSL does not reconstruct its AST",
        ));
    }
    let default_compiled_json: String = row.get("default_compiled_json");
    let stored_default: CompiledFrequency =
        serde_json::from_str(&default_compiled_json).map_err(json_protocol)?;
    if canonical_string(&stored_default)? != default_compiled_json
        || stored_default != default_compiled
    {
        return Err(protocol(
            "stored Karma Frequency default compilation disagrees with its AST",
        ));
    }
    let schema_name: String = row.get("schema_name");
    if schema_name != "karma.frequency.v1" {
        return Err(protocol("stored Karma Frequency schema is unsupported"));
    }
    Ok(FrequencyRevisionRow {
        revision_hash,
        frequency_uid: row.get("frequency_uid"),
        frequency,
        canonical_dsl,
        default_compiled,
        created_at: row.get("created_at"),
    })
}

fn map_activation(row: sqlx::sqlite::SqliteRow) -> Result<FrequencyActivationRow, StoreError> {
    let activation_hash = parse_hash(row.get("activation_hash"))?;
    let epoch_json: String = row.get("epoch_json");
    let epoch: FrequencyActivationEpoch =
        serde_json::from_str(&epoch_json).map_err(json_protocol)?;
    if canonical_string(&epoch)? != epoch_json
        || epoch.activation_hash().map_err(boundary)? != activation_hash
    {
        return Err(protocol(
            "stored Karma Frequency activation epoch is invalid",
        ));
    }
    let frequency_uid: String = row.get("frequency_uid");
    let definition_revision_hash = parse_hash(row.get("definition_revision_hash"))?;
    let effective_parameter_hash = parse_hash(row.get("effective_parameter_hash"))?;
    let previous_activation_hash = row
        .get::<Option<String>, _>("previous_activation_hash")
        .map(parse_hash)
        .transpose()?;
    let cause_action: String = row.get("cause_action");
    let activated_at: String = row.get("activated_at");
    if epoch.frequency_uid() != frequency_uid
        || sqlite_revision(epoch.activating_handle_revision())?
            != row.get::<i64, _>("activating_handle_revision")
        || epoch.definition_revision_hash() != &definition_revision_hash
        || epoch.effective_parameter_hash() != &effective_parameter_hash
        || epoch.previous_activation_hash() != previous_activation_hash.as_ref()
        || activation_cause_name(epoch.cause()) != cause_action
        || epoch.activated_at().to_string() != activated_at
    {
        return Err(protocol(
            "stored Karma Frequency activation columns disagree with its epoch",
        ));
    }
    let effective_parameters_json: String = row.get("effective_parameters_json");
    let compiled_json: String = row.get("compiled_json");
    if canonical_string(epoch.effective_parameters())? != effective_parameters_json
        || canonical_string(epoch.compiled())? != compiled_json
    {
        return Err(protocol(
            "stored Karma Frequency activation projections disagree with its epoch",
        ));
    }
    let definition_ast_json: String = row.get("definition_ast_json");
    let definition: FrequencyAst =
        serde_json::from_str(&definition_ast_json).map_err(json_protocol)?;
    if canonical_string(&definition)? != definition_ast_json {
        return Err(protocol(
            "stored Karma Frequency activation definition is not canonical",
        ));
    }
    let recompiled = definition
        .compile(epoch.effective_parameters())
        .map_err(|error| protocol(error.to_string()))?;
    if &recompiled != epoch.compiled() {
        return Err(protocol(
            "stored Karma Frequency activation compilation is not reproducible",
        ));
    }
    Ok(FrequencyActivationRow {
        activation_hash,
        epoch,
    })
}

#[derive(Serialize)]
struct RequestFingerprint<'a> {
    action: FrequencyMutationAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    frequency_uid: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_handle_revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    revision_hash: Option<&'a CanonicalHash>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameter_overrides: Option<&'a BTreeMap<LocalId, FrequencyParameterValue>>,
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
            "Karma Frequency request id must contain 1 to 200 trimmed non-control bytes",
        ))
    } else {
        Ok(())
    }
}

fn validate_person(value: Option<&str>, label: &str) -> Result<(), StoreError> {
    if let Some(value) = value {
        TypedUid::new(ReferenceKind::Person, value.to_string())
            .map_err(|error| protocol(format!("invalid Frequency {label}: {error}")))?;
    }
    Ok(())
}

fn validate_frequency_uid(value: &str) -> Result<(), StoreError> {
    TypedUid::new(ReferenceKind::Frequency, value.to_string())
        .map(|_| ())
        .map_err(|error| protocol(format!("invalid Frequency uid: {error}")))
}

fn canonical_time(value: DateTime<Utc>) -> Result<DateTime<Utc>, StoreError> {
    Utc.timestamp_millis_opt(value.timestamp_millis())
        .single()
        .ok_or_else(|| protocol("Karma Frequency timestamp is outside the supported range"))
}

fn canonical_string<T: Serialize>(value: &T) -> Result<String, StoreError> {
    String::from_utf8(canonical_json_bytes(value).map_err(boundary)?)
        .map_err(|error| protocol(error.to_string()))
}

fn checked_next_revision(current: u64) -> Result<u64, StoreError> {
    current
        .checked_add(1)
        .ok_or_else(|| protocol("Karma Frequency handle revision overflowed"))
}

fn sqlite_revision(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| protocol("Karma Frequency revision exceeds SQLite range"))
}

fn rust_revision(value: i64) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|_| protocol("stored Karma Frequency revision is negative"))
}

async fn current_revision_tx(
    tx: &mut Transaction<'_, Sqlite>,
    frequency_uid: &str,
) -> Result<u64, StoreError> {
    let value: i64 =
        sqlx::query_scalar("SELECT handle_revision FROM karma_frequency WHERE record_uid = ?")
            .bind(frequency_uid)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
    rust_revision(value)
}

fn parse_hash(value: String) -> Result<CanonicalHash, StoreError> {
    CanonicalHash::parse(value).map_err(boundary)
}

fn action_name(action: FrequencyMutationAction) -> &'static str {
    match action {
        FrequencyMutationAction::Create => "create",
        FrequencyMutationAction::Revise => "revise",
        FrequencyMutationAction::Activate => "activate",
        FrequencyMutationAction::SetParameters => "set-parameters",
        FrequencyMutationAction::ResetParameters => "reset-parameters",
        FrequencyMutationAction::Pause => "pause",
    }
}

fn activation_cause_name(cause: FrequencyActivationCause) -> &'static str {
    match cause {
        FrequencyActivationCause::ActivateRevision => "activate-revision",
        FrequencyActivationCause::SetParameters => "set-parameters",
        FrequencyActivationCause::ResetParameters => "reset-parameters",
    }
}

fn current_activation(handle: &FrequencyHandleRow) -> Option<&CanonicalHash> {
    handle.active_activation_hash.as_ref()
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
