//! Conversation extensions, relation lookups, and Call Session state.

mod listing;
mod sessions;

use nucleus::RecordKind;
use serde::{Deserialize, Serialize};
use serde_json::{from_value, to_value};
use sqlx::SqlitePool;

pub use listing::{ConversationSummary, MessagePreview, conversations_by_tag};
pub use sessions::{
    RecordingState, SessionSidecar, close_session, get_session, open_session, sessions_of,
};

use crate::{
    StoreError,
    assertions::{AssertionRole, NewAssertion, assert, object_exists, objects_from_subject},
    exact::one,
    records::{self, NewRecord, RecordRow},
    vocabulary::{feature_read, feature_write},
};

/// Extension namespace for Conversation room state.
pub const NAMESPACE: &str = "communication.v1";
/// Extension namespace for the Call Session sidecar.
pub const SESSION_NAMESPACE: &str = "communication.session.v1";

/// Stored Vocabulary key for linking a participant.
pub const KIND_PARTICIPANT: &str = "participant";
/// Stored Vocabulary key for the group relation.
pub const KIND_GROUP_OF: &str = "group-of";
/// Stored Vocabulary key for the Call Session relation.
pub const KIND_CALL_SESSION_OF: &str = "call-session-of";
/// Stored Vocabulary key for marking a Conversation with a tag.
pub const KIND_TAGGED: &str = "tagged";
/// Stored Vocabulary key for linking a call recording.
pub const KIND_CALL_RECORDING: &str = "call-recording";
/// Stored Vocabulary key for linking a call transcript.
pub const KIND_CALL_TRANSCRIPT: &str = "call-transcript";

fn provider_native() -> String {
    "native-webrtc".to_string()
}
fn media_audio() -> String {
    "audio".to_string()
}
fn idle() -> String {
    "idle".to_string()
}
fn recording_manual() -> String {
    "manual".to_string()
}

/// Persisted state of the Conversation room.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoomState {
    #[serde(default = "idle")]
    /// Room lifecycle state.
    pub state: String,
    #[serde(default = "media_audio")]
    /// Current media selection.
    pub media: String,
    #[serde(default)]
    /// People occupying the room.
    pub occupants: Vec<String>,
    #[serde(default)]
    /// Record identity of the current Call Session.
    pub session_record_id: Option<String>,
}

impl Default for RoomState {
    fn default() -> Self {
        Self {
            state: idle(),
            media: media_audio(),
            occupants: Vec::new(),
            session_record_id: None,
        }
    }
}

/// Persisted Communication extension for a Conversation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommunicationExt {
    #[serde(default = "provider_native")]
    /// Configured media provider.
    pub provider: String,
    #[serde(default)]
    /// Identifier recognized by the media provider.
    pub room_id: String,
    #[serde(default)]
    /// Current room state.
    pub room: RoomState,
    #[serde(default)]
    /// Sand controlling the Conversation, when set.
    pub controller_sand_uid: Option<String>,
    #[serde(default = "recording_manual")]
    /// Configured recording policy.
    pub recording_policy: String,
}

impl Default for CommunicationExt {
    fn default() -> Self {
        Self {
            provider: provider_native(),
            room_id: String::new(),
            room: RoomState::default(),
            controller_sand_uid: None,
            recording_policy: recording_manual(),
        }
    }
}

/// Reads the Communication extension when present and decodable.
/// # Errors
/// Returns an error if the Record extension cannot be read.
pub async fn get_ext(
    pool: &SqlitePool,
    record_uid: &str,
) -> Result<Option<CommunicationExt>, StoreError> {
    Ok(records::get_extension(pool, record_uid, NAMESPACE)
        .await?
        .and_then(|fds| from_value(fds).ok()))
}

/// Stores the Communication extension on a Record.
/// # Errors
/// Returns an error if the extension cannot be encoded or saved.
pub async fn set_ext(
    pool: &SqlitePool,
    record_uid: &str,
    ext: &CommunicationExt,
) -> Result<(), StoreError> {
    let fds = to_value(ext)
        .map_err(|error| StoreError::Protocol(format!("serialize communication.v1: {error}")))?;
    records::set_extension(pool, record_uid, NAMESPACE, &fds).await
}

/// Resolves or creates the active Record used as a tag.
/// # Errors
/// Returns an error if the name cannot be resolved or the Record cannot be created.
pub async fn ensure_tag_record(pool: &SqlitePool, slug: &str) -> Result<RecordRow, StoreError> {
    if let Some(existing) = records::resolve(pool, slug).await? {
        return Ok(existing);
    }
    records::create(
        pool,
        NewRecord {
            slug: Some(slug),
            kind: RecordKind::Plain,
            head: slug,
            body: "",
            quantity: one(),
        },
    )
    .await
}

/// Applies a tag to the Conversation through its active Vocabulary relation.
/// # Errors
/// Returns an error if the relation cannot be resolved or saved.
pub async fn tag(
    pool: &SqlitePool,
    conversation_uid: &str,
    tag_uid: &str,
) -> Result<(), StoreError> {
    assert_relation(pool, "tagging", conversation_uid, tag_uid).await
}

/// Adds the Person to the Conversation through its active Vocabulary relation.
/// # Errors
/// Returns an error if the relation cannot be resolved or saved.
pub async fn add_participant(
    pool: &SqlitePool,
    conversation_uid: &str,
    person_uid: &str,
) -> Result<(), StoreError> {
    assert_relation(pool, "participant", conversation_uid, person_uid).await
}

async fn assert_relation(
    pool: &SqlitePool,
    feature: &str,
    subject_uid: &str,
    object_uid: &str,
) -> Result<(), StoreError> {
    let predicate_uid = feature_write(pool, feature).await?;
    assert(
        pool,
        NewAssertion {
            subject_uid,
            predicate_uid: &predicate_uid,
            object_uid: Some(object_uid),
            role: AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
            asserted_by: None,
        },
    )
    .await?;
    Ok(())
}

/// Lists the participants using the active Vocabulary relation.
/// # Errors
/// Returns an error if the relation or participants cannot be read.
pub async fn participants(
    pool: &SqlitePool,
    conversation_uid: &str,
) -> Result<Vec<RecordRow>, StoreError> {
    match feature_read(pool, "participant").await? {
        Some(kind) => objects_from_subject(pool, conversation_uid, &kind).await,
        None => Ok(Vec::new()),
    }
}

/// Checks for an active Conversation participant relation to the Person.
/// # Errors
/// Returns an error if the relation or assertion cannot be read.
pub async fn is_participant(
    pool: &SqlitePool,
    conversation_uid: &str,
    person_uid: &str,
) -> Result<bool, StoreError> {
    match feature_read(pool, "participant").await? {
        Some(kind) => object_exists(pool, conversation_uid, &kind, Some(person_uid)).await,
        None => Ok(false),
    }
}
