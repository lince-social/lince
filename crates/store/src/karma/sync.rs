//! Karma definitions crossing between an Organ's own Cells (Ontology C7, axis 1).
//!
//! Axis 1 is "is this rule synced", and until now the answer was no for
//! everyone: `op_in_scope` names the tables that travel — `record`, `fact`,
//! `record_assertion`, `record_extension`, `concept`, `crdt`/`snapshot` — and no
//! `karma_*` table is among them. Nor did the Program's Record row stand in for
//! it: `programs::create` inserts that row RAW, with no `log_local` and so no
//! op, which means not even the NAME of a rule reached another Cell. So axis 2
//! could say "the always-on Cell runs the common Karma" about an arrangement no
//! Organ could reach at all.
//!
//! **The definition rides as a Record extension**, following the executor
//! designation rather than inventing a second mechanism. Namespace `lince.karma`
//! with one key per kind, so the whole definition is ONE op: a hash and the
//! thing it hashes cannot arrive separately and disagree, which matters because
//! the receiver verifies one against the other.
//!
//! **Active state only, not history.** Extension ops have no snapshot to become
//! prunable under — that is `crdt`'s mechanism and a C10 box — so a key per
//! revision hash would carry every draft a rule ever had to every Cell forever.
//! Revision history stays local to the Cell that authored it, where it is
//! written and where it is read. What crosses is the rule that runs.
//!
//! **Programs AND Frequencies, together, because either alone does nothing.**
//! Every active Program is a member of every frozen occurrence epoch, so a Cell
//! holding a synced Program with no Frequency has nothing to schedule it and
//! the Program never runs. Shipping one without the other would be a feature
//! that syncs and does not work.
//!
//! **Nothing here writes an op or a Fact.** Publishing is the one exception and
//! it writes exactly one extension op through `records::set_extension`; the
//! import side materialises rows directly and never calls `programs::create` or
//! `frequencies::activate`, which would re-log ops, re-mint request ids and
//! append evidence Facts describing a decision this Cell did not make.

use chrono::Utc;
use nucleus::karma::{
    CanonicalHash, DefinitionStatus, FrequencyActivationEpoch, FrequencyAst, ProgramAst,
    format_frequency, format_program, prove_program,
};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::SqlitePool;

use crate::StoreError;

/// The extension namespace Karma definitions travel in.
///
/// One dot, and the key carries the kind: `lince.karma.program`,
/// `lince.karma.frequency`. The wire splits the op field with `rsplit_once`, so
/// the namespace may hold dots and the key may not — see
/// `engine/tests/sync_ops.rs::the_executor_designation_survives_the_wire`.
pub const NAMESPACE: &str = "lince.karma";
pub const KEY_PROGRAM: &str = "program";
pub const KEY_FREQUENCY: &str = "frequency";

/// Whether an op field belongs to this mechanism, for the import hook to test.
pub fn is_definition_field(field: &str) -> bool {
    field == format!("{NAMESPACE}.{KEY_PROGRAM}") || field == format!("{NAMESPACE}.{KEY_FREQUENCY}")
}

/// Publish this Cell's view of a Program to the Organ's other Cells.
///
/// Derived from the STORE rather than from the action that prompted it, so it
/// cannot drift from what this Cell actually holds: called after a create, a
/// revise, an activate or a pause, it always publishes the same thing — the
/// currently active definition, or `null` when there is none.
///
/// `null` is how a pause travels. Without it, pausing a rule on the laptop
/// would leave the always-on Cell running the last definition it heard about,
/// which is the failure mode that makes people distrust sync.
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

/// Publish this Cell's view of a Frequency, definition and activation together.
///
/// The activation epoch rides with the AST because it is what a schedule is
/// actually driven by: the compiled cadence plus the effective parameters. Two
/// Cells given the same epoch compute the same occurrence hashes independently,
/// which is why no occurrence has to travel — the schedule converges by
/// construction and each Cell decides for itself which Programs it runs.
pub async fn publish_frequency(pool: &SqlitePool, frequency_uid: &str) -> Result<(), StoreError> {
    let handle = super::frequencies::get_handle(pool, frequency_uid).await?;
    let payload = match handle {
        Some(handle) => {
            match (handle.active_revision_hash, handle.active_activation_hash) {
                (Some(revision_hash), Some(activation_hash)) => {
                    let revision =
                        super::frequencies::get_revision(pool, &revision_hash).await?;
                    let activation =
                        super::frequencies::get_activation(pool, &activation_hash).await?;
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
            }
        }
        None => Value::Null,
    };
    set_key(pool, frequency_uid, KEY_FREQUENCY, payload).await
}

/// Materialise whatever a peer Cell published for this Record.
///
/// **The caller decides whether to call this at all**, and that decision is the
/// security boundary: only a batch authenticated as one of THIS Organ's own
/// Cells may reach here. A definition arriving from a contact is stored as an
/// extension like any other data and never becomes a row `freeze_next_epoch`
/// can join, because code that arrives over a socket and runs on receipt is the
/// whole failure mode. See `engine::sync::import_ops`.
///
/// Everything is re-derived locally and checked against what arrived. The hash
/// is not taken on trust — it is recomputed from the AST, and a mismatch
/// refuses the definition rather than storing a row whose content-address is a
/// lie. The proof is re-run for the same reason: a Program this Cell cannot
/// prove does not become active here, whatever the sender believed.
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
        // Published as null: paused, or never activated. Pausing must travel,
        // so this is a real state rather than a no-op — but only for a Program
        // we already hold. A null for an unknown uid says nothing to record.
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
        // Not quarantine-worthy noise: a peer Cell of our own Organ sending a
        // definition whose hash does not match its content is either corruption
        // or something wearing that Cell's identity, and either way the honest
        // move is to refuse the row rather than store a content-address that
        // does not address its content.
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
    // The handle's `handle_revision` is this Cell's own optimistic-concurrency
    // counter and is deliberately NOT taken from the sender: it guards local
    // edits against each other, and adopting a peer's number would make two
    // Cells' counters collide in a way neither can detect.
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
    // Deserialising the epoch validates it — `FrequencyActivationEpoch` has a
    // hand-written `Deserialize` that runs the same boundary checks its
    // constructor does, so a malformed epoch cannot get in by the back door.
    let activation: FrequencyActivationEpoch = serde_json::from_value(
        object
            .get("activation")
            .cloned()
            .ok_or_else(|| protocol("published Karma Frequency has no activation"))?,
    )
    .map_err(|error| protocol(format!("published Karma Frequency activation is unreadable: {error}")))?;

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
    .bind(activation.previous_activation_hash().map(CanonicalHash::as_str))
    .bind(activation_cause_name(activation.cause()))
    .bind(activation.activated_at().to_string())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Give the definition a Record row of the right KIND, creating it if the only
/// thing that arrived was the extension.
///
/// Necessary because a Program's Record row does not sync either: `programs
/// ::create` inserts it RAW, with no `log_local`, so no op — the name never
/// travelled any more than the rule did. So the definition is the source of
/// truth for its own Record, which is the better arrangement anyway: slug and
/// purpose live in the AST, and deriving the row from them cannot leave the two
/// disagreeing.
///
/// The kind matters beyond tidiness. `karma_program`'s trigger refuses a handle
/// whose Record is not `kind = 'program'`, so a stub left as `plain` by the
/// arriving extension op is not a cosmetic wart — it is a rule that silently
/// never becomes runnable.
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
    // `record.slug` is UNIQUE, and a slug is a convenience where the uid is the
    // identity. Two Cells that each authored a rule called `run.daily` before
    // they ever synced is an ORDINARY situation, not an attack, and letting the
    // insert fail there would quarantine the definition and leave the rule
    // silently never running — the exact failure mode this whole cluster is
    // about. So a taken slug is dropped rather than fought over: the rule
    // arrives under a DISAMBIGUATED name and runs, which is recoverable and
    // visible, instead of being absent, which is visible nowhere.
    //
    // Not `NULL`: a trigger requires a Karma Record to keep a slug, and it is
    // right to — a nameless rule in a list is barely better than a missing one.
    // The suffix comes from the uid, so it is stable across re-imports rather
    // than growing a new tail every time the definition is republished.
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

/// Write one key of the namespace without disturbing the other.
///
/// `set_extension` takes the whole namespace and logs one op per CHANGED key,
/// so reading the current value first is what keeps a Program's key from
/// vanishing when a Frequency's is written. In practice a Record is one or the
/// other, and this is the cheap insurance against the day it is not.
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
