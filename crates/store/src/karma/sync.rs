use chrono::Utc;
use nucleus::karma::{
    CanonicalHash, DefinitionStatus, FrequencyActivationEpoch, FrequencyAst, ProgramAst,
    format_frequency, format_program, prove_program,
};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::SqlitePool;

use crate::StoreError;

pub const NAMESPACE: &str = "lince.karma";
pub const KEY_PROGRAM: &str = "program";
pub const KEY_FREQUENCY: &str = "frequency";

pub fn is_definition_field(field: &str) -> bool {
    field == format!("{NAMESPACE}.{KEY_PROGRAM}") || field == format!("{NAMESPACE}.{KEY_FREQUENCY}")
}

pub async fn publish_program(pool: &SqlitePool, program_uid: &str) -> Result<(), StoreError> {
    let handle = super::programs::get_handle(pool, program_uid).await?;
    let payload = match handle.and_then(|handle| handle.active_revision_hash) {
        Some(hash) => match super::programs::get_revision(pool, &hash).await? {
            Some(revision) => json!({
                "hash": revision.revision_hash.as_str(),
                "ast": revision.program,
            }),
            None => Value::Null,
        },
        None => Value::Null,
    };
    set_key(pool, program_uid, KEY_PROGRAM, payload).await
}

pub async fn publish_frequency(pool: &SqlitePool, frequency_uid: &str) -> Result<(), StoreError> {
    let handle = super::frequencies::get_handle(pool, frequency_uid).await?;
    let payload = match handle {
        Some(handle) => match (handle.active_revision_hash, handle.active_activation_hash) {
            (Some(revision_hash), Some(activation_hash)) => {
                let revision = super::frequencies::get_revision(pool, &revision_hash).await?;
                let activation = super::frequencies::get_activation(pool, &activation_hash).await?;
                match (revision, activation) {
                    (Some(revision), Some(activation)) => json!({
                        "hash": revision_hash.as_str(),
                        "ast": revision.frequency,
                        "activation": activation.epoch,
                    }),
                    _ => Value::Null,
                }
            }
            _ => Value::Null,
        },
        None => Value::Null,
    };
    set_key(pool, frequency_uid, KEY_FREQUENCY, payload).await
}

pub async fn import_definition(pool: &SqlitePool, record_uid: &str) -> Result<(), StoreError> {
    let Some(fds) = crate::records::get_extension(pool, record_uid, NAMESPACE).await? else {
        return Ok(());
    };
    if let Some(program) = fds.get(KEY_PROGRAM) {
        import_program(pool, record_uid, program).await?;
    }
    if let Some(frequency) = fds.get(KEY_FREQUENCY) {
        import_frequency(pool, record_uid, frequency).await?;
    }
    Ok(())
}

async fn import_program(
    pool: &SqlitePool,
    program_uid: &str,
    payload: &Value,
) -> Result<(), StoreError> {
    let at = Utc::now().to_rfc3339();
    let Some(object) = payload.as_object() else {
        sqlx::query(
            "UPDATE karma_program SET status = 'paused', active_revision_hash = NULL,
                    handle_revision = handle_revision + 1, updated_at = ?
             WHERE record_uid = ? AND active_revision_hash IS NOT NULL",
        )
        .bind(&at)
        .bind(program_uid)
        .execute(pool)
        .await?;
        return Ok(());
    };
    let claimed = object
        .get("hash")
        .and_then(Value::as_str)
        .ok_or_else(|| protocol("published Karma Program has no revision hash"))?;
    let ast: ProgramAst = serde_json::from_value(
        object
            .get("ast")
            .cloned()
            .ok_or_else(|| protocol("published Karma Program has no definition"))?,
    )
    .map_err(|error| protocol(format!("published Karma Program is unreadable: {error}")))?;

    let proof = prove_program(&ast);
    let derived = proof
        .revision_hash
        .clone()
        .ok_or_else(|| protocol("published Karma Program cannot be canonically hashed"))?;
    if derived.as_str() != claimed {
        return Err(protocol(
            "published Karma Program hash disagrees with its definition",
        ));
    }
    if proof.status != nucleus::karma::ProofStatus::Accepted {
        return Err(protocol(
            "published Karma Program does not prove on this Cell",
        ));
    }

    let mut tx = crate::write_tx(pool).await?;
    ensure_definition_record(
        &mut tx,
        program_uid,
        nucleus::RecordKind::Program,
        ast.slug.as_str(),
        &ast.purpose,
        &at,
    )
    .await?;
    sqlx::query(
        "INSERT OR IGNORE INTO karma_program_revision
            (revision_hash, program_uid, schema_name, ast_json, canonical_dsl,
             proof_json, proof_status, created_at)
         VALUES (?, ?, 'karma.program.v1', ?, ?, ?, 'accepted', ?)",
    )
    .bind(derived.as_str())
    .bind(program_uid)
    .bind(canonical_string(&ast)?)
    .bind(format_program(&ast))
    .bind(canonical_string(&proof)?)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO karma_program
            (record_uid, handle_revision, status, head_revision_hash,
             active_revision_hash, owner_person_uid, created_at, updated_at)
         VALUES (?, 1, ?, ?, ?, NULL, ?, ?)
         ON CONFLICT(record_uid) DO UPDATE SET
            handle_revision = handle_revision + 1,
            status = excluded.status,
            head_revision_hash = excluded.head_revision_hash,
            active_revision_hash = excluded.active_revision_hash,
            updated_at = excluded.updated_at
         WHERE karma_program.status != 'retired'",
    )
    .bind(program_uid)
    .bind(DefinitionStatus::Active.as_str())
    .bind(derived.as_str())
    .bind(derived.as_str())
    .bind(&at)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn import_frequency(
    pool: &SqlitePool,
    frequency_uid: &str,
    payload: &Value,
) -> Result<(), StoreError> {
    let at = Utc::now().to_rfc3339();
    let Some(object) = payload.as_object() else {
        sqlx::query(
            "UPDATE karma_frequency SET status = 'paused', active_revision_hash = NULL,
                    active_activation_hash = NULL, handle_revision = handle_revision + 1,
                    updated_at = ?
             WHERE record_uid = ? AND active_revision_hash IS NOT NULL",
        )
        .bind(&at)
        .bind(frequency_uid)
        .execute(pool)
        .await?;
        return Ok(());
    };
    let claimed = object
        .get("hash")
        .and_then(Value::as_str)
        .ok_or_else(|| protocol("published Karma Frequency has no revision hash"))?;
    let ast: FrequencyAst = serde_json::from_value(
        object
            .get("ast")
            .cloned()
            .ok_or_else(|| protocol("published Karma Frequency has no definition"))?,
    )
    .map_err(|error| protocol(format!("published Karma Frequency is unreadable: {error}")))?;
    let activation: FrequencyActivationEpoch = serde_json::from_value(
        object
            .get("activation")
            .cloned()
            .ok_or_else(|| protocol("published Karma Frequency has no activation"))?,
    )
    .map_err(|error| {
        protocol(format!(
            "published Karma Frequency activation is unreadable: {error}"
        ))
    })?;

    let default_compiled = ast
        .compile(&std::collections::BTreeMap::new())
        .map_err(|error| protocol(error.to_string()))?;
    if default_compiled.revision_hash.as_str() != claimed {
        return Err(protocol(
            "published Karma Frequency hash disagrees with its definition",
        ));
    }
    if activation.frequency_uid() != frequency_uid {
        return Err(protocol(
            "published Karma Frequency activation names another Frequency",
        ));
    }
    if activation.definition_revision_hash().as_str() != claimed {
        return Err(protocol(
            "published Karma Frequency activation does not match the published definition",
        ));
    }
    let activation_hash = activation.activation_hash().map_err(boundary)?;

    let mut tx = crate::write_tx(pool).await?;
    ensure_definition_record(
        &mut tx,
        frequency_uid,
        nucleus::RecordKind::Frequency,
        ast.slug.as_str(),
        &ast.purpose,
        &at,
    )
    .await?;
    sqlx::query(
        "INSERT OR IGNORE INTO karma_frequency_revision
            (revision_hash, frequency_uid, schema_name, ast_json, canonical_dsl,
             default_compiled_json, created_at)
         VALUES (?, ?, 'karma.frequency.v1', ?, ?, ?, ?)",
    )
    .bind(claimed)
    .bind(frequency_uid)
    .bind(canonical_string(&ast)?)
    .bind(format_frequency(&ast))
    .bind(canonical_string(&default_compiled)?)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO karma_frequency
            (record_uid, handle_revision, status, head_revision_hash,
             active_revision_hash, active_activation_hash, latest_activation_hash,
             owner_person_uid, created_at, updated_at)
         VALUES (?, 1, ?, ?, ?, ?, ?, NULL, ?, ?)
         ON CONFLICT(record_uid) DO UPDATE SET
            handle_revision = handle_revision + 1,
            status = excluded.status,
            head_revision_hash = excluded.head_revision_hash,
            active_revision_hash = excluded.active_revision_hash,
            active_activation_hash = excluded.active_activation_hash,
            latest_activation_hash = excluded.latest_activation_hash,
            updated_at = excluded.updated_at
         WHERE karma_frequency.status != 'retired'",
    )
    .bind(frequency_uid)
    .bind(DefinitionStatus::Active.as_str())
    .bind(claimed)
    .bind(claimed)
    .bind(activation_hash.as_str())
    .bind(activation_hash.as_str())
    .bind(&at)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT OR IGNORE INTO karma_frequency_activation
            (activation_hash, frequency_uid, activating_handle_revision,
             definition_revision_hash, effective_parameter_hash,
             effective_parameters_json, compiled_json, epoch_json,
             previous_activation_hash, cause_action, activated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(activation_hash.as_str())
    .bind(frequency_uid)
    .bind(i64::try_from(activation.activating_handle_revision()).unwrap_or(i64::MAX))
    .bind(activation.definition_revision_hash().as_str())
    .bind(activation.effective_parameter_hash().as_str())
    .bind(canonical_string(activation.effective_parameters())?)
    .bind(canonical_string(activation.compiled())?)
    .bind(canonical_string(&activation)?)
    .bind(
        activation
            .previous_activation_hash()
            .map(CanonicalHash::as_str),
    )
    .bind(activation_cause_name(activation.cause()))
    .bind(activation.activated_at().to_string())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn ensure_definition_record(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    record_uid: &str,
    kind: nucleus::RecordKind,
    slug: &str,
    head: &str,
    at: &str,
) -> Result<(), StoreError> {
    let organ: Option<String> = sqlx::query_scalar(
        "SELECT uid FROM record WHERE slug = ? AND kind = 'organ' AND deleted_at IS NULL LIMIT 1",
    )
    .bind(crate::organs::LOCAL_ORGAN_SLUG)
    .fetch_optional(&mut **tx)
    .await?;
    let taken: Option<String> = sqlx::query_scalar("SELECT uid FROM record WHERE slug = ?")
        .bind(slug)
        .fetch_optional(&mut **tx)
        .await?;
    let disambiguated;
    let slug = match taken {
        Some(holder) if holder != record_uid => {
            let tail = record_uid.to_lowercase();
            let tail = &tail[tail.len().saturating_sub(6)..];
            disambiguated = format!("{slug}-{tail}");
            disambiguated.as_str()
        }
        _ => slug,
    };
    sqlx::query(
        "INSERT INTO record
            (uid, slug, kind, head, body, quantity_mantissa, quantity_scale,
             organ_uid, created_at, updated_at)
         VALUES (?, ?, ?, ?, '', '0', 0, ?, ?, ?)
         ON CONFLICT(uid) DO UPDATE SET
            slug = excluded.slug,
            kind = excluded.kind,
            head = excluded.head,
            updated_at = excluded.updated_at",
    )
    .bind(record_uid)
    .bind(slug)
    .bind(kind.as_str())
    .bind(head)
    .bind(organ)
    .bind(at)
    .bind(at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn set_key(
    pool: &SqlitePool,
    record_uid: &str,
    key: &str,
    payload: Value,
) -> Result<(), StoreError> {
    let mut fds = crate::records::get_extension(pool, record_uid, NAMESPACE)
        .await?
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    fds.insert(key.to_string(), payload);
    crate::records::set_extension(pool, record_uid, NAMESPACE, &Value::Object(fds)).await
}

fn canonical_string<T: Serialize>(value: &T) -> Result<String, StoreError> {
    String::from_utf8(nucleus::karma::canonical_json_bytes(value).map_err(boundary)?)
        .map_err(|_| protocol("canonical JSON was not valid UTF-8"))
}

fn activation_cause_name(cause: nucleus::karma::FrequencyActivationCause) -> &'static str {
    match cause {
        nucleus::karma::FrequencyActivationCause::ActivateRevision => "activate-revision",
        nucleus::karma::FrequencyActivationCause::SetParameters => "set-parameters",
        nucleus::karma::FrequencyActivationCause::ResetParameters => "reset-parameters",
    }
}

fn protocol(message: impl Into<String>) -> StoreError {
    sqlx::Error::Protocol(message.into())
}

fn boundary(error: nucleus::karma::KarmaBoundaryError) -> StoreError {
    protocol(error.to_string())
}
