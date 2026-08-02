//! Communication sand store layer (S2 of `notes/institute/Communication.md`).
//!
//! A conversation is an ordinary Record discovered by a tag link, never a
//! dedicated table. This module is the thin store surface the Communication
//! sand needs: read/write the `communication.v1` extension, list the
//! conversations carrying a tag (newest activity first, with a last-message
//! preview and resolved participants), and open/close `call_session` child
//! Records as the room's occupancy transitions. No call-intent / Karma
//! machinery lives here — that is deferred to the Far-Future part (F2).

use chrono::Utc;
use nucleus::RecordKind;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::{
    StoreError, assertions, concepts,
    records::{self, NewRecord, RecordRow},
};

/// Namespace of the room-control extension on a conversation Record.
pub const NAMESPACE: &str = "communication.v1";
/// Namespace of the per-occupancy sidecar on a `call_session` Record.
pub const SESSION_NAMESPACE: &str = "communication.session.v1";

// Link kinds (canonical concept names). Threads/messages keep their own
// kinds (`thread-of`, `message-in`) owned by the Record surface.
pub const KIND_PARTICIPANT: &str = "participant";
pub const KIND_GROUP_OF: &str = "group-of";
pub const KIND_CALL_SESSION_OF: &str = "call-session-of";
pub const KIND_TAGGED: &str = "tagged";
pub const KIND_CALL_RECORDING: &str = "call-recording";
pub const KIND_CALL_TRANSCRIPT: &str = "call-transcript";

fn provider_native() -> String {
    "native-webrtc".to_string()
}
fn media_audio() -> String {
    "audio".to_string()
}
fn room_idle() -> String {
    "idle".to_string()
}
fn recording_manual() -> String {
    "manual".to_string()
}
fn recording_idle() -> String {
    "idle".to_string()
}

/// Live room state mirrored into the conversation's `communication.v1`
/// extension. `occupants` is an advisory mirror; the durable truth is the
/// links + session Records.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoomState {
    #[serde(default = "room_idle")]
    pub state: String,
    #[serde(default = "media_audio")]
    pub media: String,
    #[serde(default)]
    pub occupants: Vec<String>,
    #[serde(default)]
    pub session_record_id: Option<String>,
}

impl Default for RoomState {
    fn default() -> Self {
        Self {
            state: room_idle(),
            media: media_audio(),
            occupants: Vec::new(),
            session_record_id: None,
        }
    }
}

/// The `communication.v1` extension: everything about a conversation's room
/// that is not a link or a message. Never holds credentials.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommunicationExt {
    #[serde(default = "provider_native")]
    pub provider: String,
    #[serde(default)]
    pub room_id: String,
    #[serde(default)]
    pub room: RoomState,
    #[serde(default)]
    pub controller_sand_uid: Option<String>,
    #[serde(default = "recording_manual")]
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

/// Recording state of one occupancy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecordingState {
    #[serde(default = "recording_idle")]
    pub state: String,
}

impl Default for RecordingState {
    fn default() -> Self {
        Self {
            state: recording_idle(),
        }
    }
}

/// The `communication.session.v1` sidecar on a `call_session` Record: one
/// occupancy of the room, from first join to last leave.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionSidecar {
    pub started_at: String,
    #[serde(default)]
    pub ended_at: Option<String>,
    #[serde(default = "media_audio")]
    pub media: String,
    #[serde(default)]
    pub peak_participants: i64,
    #[serde(default)]
    pub recording: RecordingState,
}

/// A conversation's row in the list view: the Record itself, resolved
/// participants, a last-message preview, and the timestamp used to sort the
/// list (newest activity first).
#[derive(Debug, Clone)]
pub struct ConversationSummary {
    pub record: RecordRow,
    pub participants: Vec<RecordRow>,
    pub last_message: Option<MessagePreview>,
    pub last_activity_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MessagePreview {
    pub uid: String,
    pub body: String,
    pub created_at: Option<String>,
}

// --- extension read/write ---------------------------------------------------

/// Read the `communication.v1` extension of a conversation, or `None` if it
/// has never carried a room.
pub async fn get_ext(
    pool: &SqlitePool,
    record_uid: &str,
) -> Result<Option<CommunicationExt>, StoreError> {
    Ok(records::get_extension(pool, record_uid, NAMESPACE)
        .await?
        .and_then(|fds| serde_json::from_value(fds).ok()))
}

/// Write the `communication.v1` extension of a conversation.
pub async fn set_ext(
    pool: &SqlitePool,
    record_uid: &str,
    ext: &CommunicationExt,
) -> Result<(), StoreError> {
    let fds = serde_json::to_value(ext)
        .map_err(|e| sqlx::Error::Protocol(format!("serialize communication.v1: {e}")))?;
    records::set_extension(pool, record_uid, NAMESPACE, &fds).await
}

/// Read the `communication.session.v1` sidecar of a `call_session` Record.
pub async fn get_session(
    pool: &SqlitePool,
    session_uid: &str,
) -> Result<Option<SessionSidecar>, StoreError> {
    Ok(records::get_extension(pool, session_uid, SESSION_NAMESPACE)
        .await?
        .and_then(|fds| serde_json::from_value(fds).ok()))
}

// --- tagging & participants -------------------------------------------------

/// Resolve (or create) the Record that stands for a tag `@slug` — the target
/// every conversation carrying that tag links to.
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
            quantity: crate::exact::one(),
        },
    )
    .await
}

/// Link a conversation to a tag Record (`conversation --tagged--> tag`).
pub async fn tag(
    pool: &SqlitePool,
    conversation_uid: &str,
    tag_uid: &str,
) -> Result<(), StoreError> {
    let kind = concepts::ensure(pool, KIND_TAGGED).await?;
    assertions::assert(
        pool,
        assertions::NewAssertion {
            subject_uid: conversation_uid,
            predicate_uid: &kind,
            object_uid: Some(tag_uid),
            role: assertions::AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
            asserted_by: None,
        },
    )
    .await?;
    Ok(())
}

/// Link a participant (person/user Record) into a conversation
/// (`conversation --participant--> person`).
pub async fn add_participant(
    pool: &SqlitePool,
    conversation_uid: &str,
    person_uid: &str,
) -> Result<(), StoreError> {
    let kind = concepts::ensure(pool, KIND_PARTICIPANT).await?;
    assertions::assert(
        pool,
        assertions::NewAssertion {
            subject_uid: conversation_uid,
            predicate_uid: &kind,
            object_uid: Some(person_uid),
            role: assertions::AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
            asserted_by: None,
        },
    )
    .await?;
    Ok(())
}

/// Resolve the participant Records of a conversation.
pub async fn participants(
    pool: &SqlitePool,
    conversation_uid: &str,
) -> Result<Vec<RecordRow>, StoreError> {
    match concepts::resolve(pool, KIND_PARTICIPANT).await? {
        Some(kind) => assertions::objects_from_subject(pool, conversation_uid, &kind).await,
        None => Ok(Vec::new()),
    }
}

// --- list query -------------------------------------------------------------

/// Conversations carrying `tag_uid`, newest activity first, each with its
/// participants and a last-message preview resolved for the list view.
///
/// v0 composes existing link/message queries per conversation rather than one
/// hand-tuned SQL join; correct and testable, optimizable later once the list
/// grows.
pub async fn conversations_by_tag(
    pool: &SqlitePool,
    tag_uid: &str,
) -> Result<Vec<ConversationSummary>, StoreError> {
    let Some(tagged) = concepts::resolve(pool, KIND_TAGGED).await? else {
        return Ok(Vec::new());
    };
    let participant = concepts::resolve(pool, KIND_PARTICIPANT).await?;
    let thread_of = concepts::resolve(pool, "thread-of").await?;
    let message_in = concepts::resolve(pool, "message-in").await?;

    let mut out = Vec::new();
    for conv in assertions::subjects_pointing_to(pool, &tagged, tag_uid).await? {
        if !conv.quantity.is_positive() {
            continue; // deactivated conversation drops out of the list
        }
        let participants = match &participant {
            Some(kind) => assertions::objects_from_subject(pool, &conv.uid, kind).await?,
            None => Vec::new(),
        };
        let last_message =
            newest_message(pool, &conv.uid, thread_of.as_deref(), message_in.as_deref()).await?;
        let conv_created = records::created_at(pool, &conv.uid).await?;
        // Newest activity is the newer of the last message and the
        // conversation's own creation (an empty conversation still sorts).
        let last_activity_at = [
            last_message.as_ref().and_then(|m| m.created_at.clone()),
            conv_created,
        ]
        .into_iter()
        .flatten()
        .max();
        out.push(ConversationSummary {
            record: conv,
            participants,
            last_message,
            last_activity_at,
        });
    }

    // Newest first; conversations without a timestamp sink to the bottom.
    out.sort_by(|a, b| b.last_activity_at.cmp(&a.last_activity_at));
    Ok(out)
}

/// The single newest message across all of a conversation's threads (for the
/// list-row preview). RFC3339 timestamps compare lexicographically.
async fn newest_message(
    pool: &SqlitePool,
    conversation_uid: &str,
    thread_of: Option<&str>,
    message_in: Option<&str>,
) -> Result<Option<MessagePreview>, StoreError> {
    let (Some(thread_of), Some(message_in)) = (thread_of, message_in) else {
        return Ok(None);
    };
    let mut best: Option<(String, MessagePreview)> = None;
    for thread in assertions::subjects_pointing_to(pool, thread_of, conversation_uid).await? {
        if thread.kind != RecordKind::Thread.as_str() || !thread.quantity.is_positive() {
            continue;
        }
        for message in assertions::subjects_pointing_to(pool, message_in, &thread.uid).await? {
            if message.kind != RecordKind::Message.as_str() || !message.quantity.is_positive() {
                continue;
            }
            let Some(created) = records::created_at(pool, &message.uid).await? else {
                continue;
            };
            let newer = best.as_ref().map(|(at, _)| created > *at).unwrap_or(true);
            if newer {
                best = Some((
                    created.clone(),
                    MessagePreview {
                        uid: message.uid.clone(),
                        body: message.body.clone(),
                        created_at: Some(created),
                    },
                ));
            }
        }
    }
    Ok(best.map(|(_, preview)| preview))
}

// --- session lifecycle ------------------------------------------------------

/// Open a room occupancy: create a `call_session` child Record with its
/// sidecar, link it `call-session-of` → conversation, and flip the
/// conversation's room to `active`. Returns the session Record.
pub async fn open_session(
    pool: &SqlitePool,
    conversation_uid: &str,
    media: &str,
) -> Result<RecordRow, StoreError> {
    let now = Utc::now().to_rfc3339();
    let head = format!("Call · {now}");
    let session = records::create(
        pool,
        NewRecord {
            slug: None,
            kind: RecordKind::CallSession,
            head: &head,
            body: "",
            quantity: crate::exact::one(),
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
    let fds = serde_json::to_value(&sidecar)
        .map_err(|e| sqlx::Error::Protocol(format!("serialize session sidecar: {e}")))?;
    records::set_extension(pool, &session.uid, SESSION_NAMESPACE, &fds).await?;

    let kind = concepts::ensure(pool, KIND_CALL_SESSION_OF).await?;
    assertions::assert(
        pool,
        assertions::NewAssertion {
            subject_uid: &session.uid,
            predicate_uid: &kind,
            object_uid: Some(conversation_uid),
            role: assertions::AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
            asserted_by: None,
        },
    )
    .await?;

    let mut ext = get_ext(pool, conversation_uid).await?.unwrap_or_default();
    ext.room.state = "active".to_string();
    ext.room.media = media.to_string();
    ext.room.session_record_id = Some(session.uid.clone());
    set_ext(pool, conversation_uid, &ext).await?;

    Ok(session)
}

/// Close a room occupancy: stamp the session sidecar's `ended_at` and peak
/// participant count, and flip the conversation's room back to `idle`.
pub async fn close_session(
    pool: &SqlitePool,
    conversation_uid: &str,
    session_uid: &str,
    peak_participants: i64,
) -> Result<(), StoreError> {
    if let Some(mut sidecar) = get_session(pool, session_uid).await? {
        sidecar.ended_at = Some(Utc::now().to_rfc3339());
        sidecar.peak_participants = peak_participants;
        let fds = serde_json::to_value(&sidecar)
            .map_err(|e| sqlx::Error::Protocol(format!("serialize session sidecar: {e}")))?;
        records::set_extension(pool, session_uid, SESSION_NAMESPACE, &fds).await?;
    }

    if let Some(mut ext) = get_ext(pool, conversation_uid).await? {
        ext.room.state = "idle".to_string();
        ext.room.occupants.clear();
        ext.room.session_record_id = None;
        set_ext(pool, conversation_uid, &ext).await?;
    }
    Ok(())
}

/// The sessions of a conversation (child Records linked `call-session-of`),
/// oldest first.
pub async fn sessions_of(
    pool: &SqlitePool,
    conversation_uid: &str,
) -> Result<Vec<RecordRow>, StoreError> {
    match concepts::resolve(pool, KIND_CALL_SESSION_OF).await? {
        Some(kind) => assertions::subjects_pointing_to(pool, &kind, conversation_uid).await,
        None => Ok(Vec::new()),
    }
}
