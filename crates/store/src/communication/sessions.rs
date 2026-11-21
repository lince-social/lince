use nucleus::{RecordKind, execution::now};
use serde::{Deserialize, Serialize};
use serde_json::{from_value, to_value};
use sqlx::SqlitePool;

use super::{SESSION_NAMESPACE, assert_relation, get_ext, idle, media_audio, set_ext};
use crate::{
    StoreError,
    assertions::subjects_pointing_to,
    exact::one,
    records::{self, NewRecord, RecordRow},
    vocabulary::feature_read,
};

/// Persisted recording state of a Call Session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecordingState {
    #[serde(default = "idle")]
    /// Recording lifecycle state.
    pub state: String,
}

impl Default for RecordingState {
    fn default() -> Self {
        Self { state: idle() }
    }
}

/// Persisted Call Session timing and media state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionSidecar {
    /// Session start timestamp.
    pub started_at: String,
    #[serde(default)]
    /// Session end timestamp, when closed.
    pub ended_at: Option<String>,
    #[serde(default = "media_audio")]
    /// Media selected for this Session.
    pub media: String,
    #[serde(default)]
    /// Largest recorded participant count.
    pub peak_participants: i64,
    #[serde(default)]
    /// Recording state for this Session.
    pub recording: RecordingState,
}

/// Reads the Call Session sidecar when present and decodable.
/// # Errors
/// Returns an error if the Record extension cannot be read.
pub async fn get_session(
    pool: &SqlitePool,
    session_uid: &str,
) -> Result<Option<SessionSidecar>, StoreError> {
    Ok(records::get_extension(pool, session_uid, SESSION_NAMESPACE)
        .await?
        .and_then(|fds| from_value(fds).ok()))
}

/// Creates a Call Session, links it to the Conversation, and marks the room active.
/// # Errors
/// Returns an error if any of the Record, sidecar, relation, or room updates fail.
pub async fn open_session(
    pool: &SqlitePool,
    conversation_uid: &str,
    media: &str,
) -> Result<RecordRow, StoreError> {
    let now = now().to_rfc3339();
    let head = format!("Call · {now}");
    let session = records::create(
        pool,
        NewRecord {
            slug: None,
            kind: RecordKind::CallSession,
            head: &head,
            body: "",
            quantity: one(),
        },
    )
    .await?;

    let sidecar = SessionSidecar {
        started_at: now,
        ended_at: None,
        media: media.to_string(),
        peak_participants: 0,
        recording: RecordingState::default(),
    };
    save_session(pool, &session.uid, &sidecar).await?;
    assert_relation(pool, "call_session", &session.uid, conversation_uid).await?;

    let mut ext = get_ext(pool, conversation_uid).await?.unwrap_or_default();
    ext.room.state = "active".to_string();
    ext.room.media = media.to_string();
    ext.room.session_record_id = Some(session.uid.clone());
    set_ext(pool, conversation_uid, &ext).await?;

    Ok(session)
}

/// Updates the available Session sidecar and resets the Conversation room.
/// # Errors
/// Returns an error if reading or writing a sidecar or room extension fails.
pub async fn close_session(
    pool: &SqlitePool,
    conversation_uid: &str,
    session_uid: &str,
    peak_participants: i64,
) -> Result<(), StoreError> {
    if let Some(mut sidecar) = get_session(pool, session_uid).await? {
        sidecar.ended_at = Some(now().to_rfc3339());
        sidecar.peak_participants = peak_participants;
        save_session(pool, session_uid, &sidecar).await?;
    }

    if let Some(mut ext) = get_ext(pool, conversation_uid).await? {
        ext.room.state = idle();
        ext.room.occupants.clear();
        ext.room.session_record_id = None;
        set_ext(pool, conversation_uid, &ext).await?;
    }
    Ok(())
}

/// Lists Call Sessions linked to the Conversation by the active Vocabulary relation.
/// # Errors
/// Returns an error if the relation or Call Session Records cannot be read.
pub async fn sessions_of(
    pool: &SqlitePool,
    conversation_uid: &str,
) -> Result<Vec<RecordRow>, StoreError> {
    match feature_read(pool, "call_session").await? {
        Some(kind) => subjects_pointing_to(pool, &kind, conversation_uid).await,
        None => Ok(Vec::new()),
    }
}

async fn save_session(
    pool: &SqlitePool,
    session_uid: &str,
    sidecar: &SessionSidecar,
) -> Result<(), StoreError> {
    let fds = to_value(sidecar)
        .map_err(|error| StoreError::Protocol(format!("serialize session sidecar: {error}")))?;
    records::set_extension(pool, session_uid, SESSION_NAMESPACE, &fds).await
}
