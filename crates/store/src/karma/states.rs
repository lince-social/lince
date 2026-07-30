use nucleus::karma::{
    CanonicalHash, LocalId, PersistedProgramNodeState, ProgramStateEvent, canonical_json_bytes,
};
use serde::Serialize;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::StoreError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramNodeStateRow {
    pub program_uid: String,
    pub node_id: LocalId,
    pub state_revision: u64,
    pub current_event_hash: CanonicalHash,
    pub definition_revision_hash: CanonicalHash,
    pub activation_handle_revision: u64,
    pub state: Option<PersistedProgramNodeState>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramStateEventRow {
    pub event_hash: CanonicalHash,
    pub event: ProgramStateEvent,
    pub created_at: String,
}

pub async fn get_node_state(
    pool: &SqlitePool,
    program_uid: &str,
    node_id: &LocalId,
) -> Result<Option<ProgramNodeStateRow>, StoreError> {
    let row =
        sqlx::query("SELECT * FROM karma_program_node_state WHERE program_uid = ? AND node_id = ?")
            .bind(program_uid)
            .bind(node_id.as_str())
            .fetch_optional(pool)
            .await?;
    row.map(map_node_state).transpose()
}

pub async fn list_node_states(pool: &SqlitePool) -> Result<Vec<ProgramNodeStateRow>, StoreError> {
    sqlx::query(
        "SELECT * FROM karma_program_node_state
         ORDER BY program_uid, node_id",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_node_state)
    .collect()
}

pub async fn list_events(pool: &SqlitePool) -> Result<Vec<ProgramStateEventRow>, StoreError> {
    sqlx::query(
        "SELECT * FROM karma_program_state_event
         ORDER BY program_uid, node_id, state_revision",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_event)
    .collect()
}

pub(crate) async fn apply_event_tx(
    tx: &mut Transaction<'_, Sqlite>,
    event: &ProgramStateEvent,
    expected_current_event_hash: Option<&CanonicalHash>,
    at: &str,
) -> Result<ProgramStateEventRow, StoreError> {
    event.validate().map_err(boundary)?;
    if event.previous_event_hash.as_ref() != expected_current_event_hash {
        return Err(protocol(
            "Program state event previous hash disagrees with its CAS expectation",
        ));
    }
    let event_hash = event.event_hash().map_err(boundary)?;
    let event_json = canonical_string(event)?;
    let state_json = event.state.as_ref().map(canonical_string).transpose()?;
    let state_kind = event
        .state
        .as_ref()
        .map_or("reset", PersistedProgramNodeState::kind_name);
    sqlx::query(
        "INSERT INTO karma_program_state_event
            (event_hash, program_uid, node_id, state_revision, previous_event_hash,
             source_run_hash, definition_revision_hash, activation_handle_revision,
             reset_reason, state_kind, state_json, event_json, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(event_hash.as_str())
    .bind(&event.program_uid)
    .bind(event.node_id.as_str())
    .bind(sql_i64(event.state_revision, "Program state revision")?)
    .bind(
        event
            .previous_event_hash
            .as_ref()
            .map(CanonicalHash::as_str),
    )
    .bind(event.source_run_hash.as_str())
    .bind(event.definition_revision_hash.as_str())
    .bind(sql_i64(
        event.activation_handle_revision,
        "Program activation handle revision",
    )?)
    .bind(event.reset_reason.map(reset_reason_name))
    .bind(state_kind)
    .bind(&state_json)
    .bind(&event_json)
    .bind(at)
    .execute(&mut **tx)
    .await?;

    let changed = if let Some(expected_hash) = expected_current_event_hash {
        sqlx::query(
            "UPDATE karma_program_node_state
             SET state_revision = ?, current_event_hash = ?, definition_revision_hash = ?,
                 activation_handle_revision = ?, state_kind = ?, state_json = ?, updated_at = ?
             WHERE program_uid = ? AND node_id = ? AND current_event_hash = ?",
        )
        .bind(sql_i64(event.state_revision, "Program state revision")?)
        .bind(event_hash.as_str())
        .bind(event.definition_revision_hash.as_str())
        .bind(sql_i64(
            event.activation_handle_revision,
            "Program activation handle revision",
        )?)
        .bind(state_kind)
        .bind(&state_json)
        .bind(at)
        .bind(&event.program_uid)
        .bind(event.node_id.as_str())
        .bind(expected_hash.as_str())
        .execute(&mut **tx)
        .await?
        .rows_affected()
    } else {
        sqlx::query(
            "INSERT OR IGNORE INTO karma_program_node_state
                (program_uid, node_id, state_revision, current_event_hash,
                 definition_revision_hash, activation_handle_revision,
                 state_kind, state_json, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&event.program_uid)
        .bind(event.node_id.as_str())
        .bind(sql_i64(event.state_revision, "Program state revision")?)
        .bind(event_hash.as_str())
        .bind(event.definition_revision_hash.as_str())
        .bind(sql_i64(
            event.activation_handle_revision,
            "Program activation handle revision",
        )?)
        .bind(state_kind)
        .bind(&state_json)
        .bind(at)
        .execute(&mut **tx)
        .await?
        .rows_affected()
    };
    if changed != 1 {
        return Err(protocol("Program node state lost CAS serialization"));
    }
    Ok(ProgramStateEventRow {
        event_hash,
        event: event.clone(),
        created_at: at.to_string(),
    })
}

fn map_node_state(row: sqlx::sqlite::SqliteRow) -> Result<ProgramNodeStateRow, StoreError> {
    let state_kind: String = row.get("state_kind");
    let state = parse_state(state_kind.as_str(), row.get("state_json"))?;
    Ok(ProgramNodeStateRow {
        program_uid: row.get("program_uid"),
        node_id: LocalId::new(row.get::<String, _>("node_id")).map_err(boundary)?,
        state_revision: rust_u64(row.get("state_revision"), "Program state revision")?,
        current_event_hash: parse_hash(row.get("current_event_hash"))?,
        definition_revision_hash: parse_hash(row.get("definition_revision_hash"))?,
        activation_handle_revision: rust_u64(
            row.get("activation_handle_revision"),
            "Program activation handle revision",
        )?,
        state,
        updated_at: row.get("updated_at"),
    })
}

fn map_event(row: sqlx::sqlite::SqliteRow) -> Result<ProgramStateEventRow, StoreError> {
    let event_hash = parse_hash(row.get("event_hash"))?;
    let event_json: String = row.get("event_json");
    let event: ProgramStateEvent = serde_json::from_str(&event_json).map_err(json_protocol)?;
    event.validate().map_err(boundary)?;
    let state_kind: String = row.get("state_kind");
    let state = parse_state(state_kind.as_str(), row.get("state_json"))?;
    if canonical_string(&event)? != event_json
        || event.event_hash().map_err(boundary)? != event_hash
        || event.program_uid != row.get::<String, _>("program_uid")
        || event.node_id.as_str() != row.get::<String, _>("node_id")
        || sql_i64(event.state_revision, "Program state revision")?
            != row.get::<i64, _>("state_revision")
        || event
            .previous_event_hash
            .as_ref()
            .map(CanonicalHash::as_str)
            != row
                .get::<Option<String>, _>("previous_event_hash")
                .as_deref()
        || event.source_run_hash.as_str() != row.get::<String, _>("source_run_hash")
        || event.definition_revision_hash.as_str()
            != row.get::<String, _>("definition_revision_hash")
        || sql_i64(
            event.activation_handle_revision,
            "Program activation handle revision",
        )? != row.get::<i64, _>("activation_handle_revision")
        || event.reset_reason.map(reset_reason_name)
            != row.get::<Option<String>, _>("reset_reason").as_deref()
        || event.state != state
    {
        return Err(protocol("stored Program state event is invalid"));
    }
    Ok(ProgramStateEventRow {
        event_hash,
        event,
        created_at: row.get("created_at"),
    })
}

fn parse_state(
    state_kind: &str,
    state_json: Option<String>,
) -> Result<Option<PersistedProgramNodeState>, StoreError> {
    match (state_kind, state_json) {
        ("reset", None) => Ok(None),
        ("delay" | "control", Some(json)) => {
            let state: PersistedProgramNodeState =
                serde_json::from_str(&json).map_err(json_protocol)?;
            if state.kind_name() != state_kind || canonical_string(&state)? != json {
                return Err(protocol("stored Program node state projection is invalid"));
            }
            Ok(Some(state))
        }
        _ => Err(protocol(
            "stored Program node state kind/content is invalid",
        )),
    }
}

fn reset_reason_name(reason: nucleus::karma::ProgramStateResetReason) -> &'static str {
    match reason {
        nucleus::karma::ProgramStateResetReason::ProgramActivation => "program-activation",
        nucleus::karma::ProgramStateResetReason::RevisionChange => "revision-change",
        nucleus::karma::ProgramStateResetReason::MigrationReset => "migration-reset",
    }
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
