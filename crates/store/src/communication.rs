use chrono::Utc;
use nucleus::RecordKind;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::{
    StoreError, assertions, concepts,
    records::{self, NewRecord, RecordRow},
};

pub const NAMESPACE: &str = "communication.v1";
pub const SESSION_NAMESPACE: &str = "communication.session.v1";

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

pub async fn get_ext(
    pool: &SqlitePool,
    record_uid: &str,
) -> Result<Option<CommunicationExt>, StoreError> {
    Ok(records::get_extension(pool, record_uid, NAMESPACE)
        .await?
        .and_then(|fds| serde_json::from_value(fds).ok()))
}

pub async fn set_ext(
    pool: &SqlitePool,
    record_uid: &str,
    ext: &CommunicationExt,
) -> Result<(), StoreError> {
    let fds = serde_json::to_value(ext)
        .map_err(|e| sqlx::Error::Protocol(format!("serialize communication.v1: {e}")))?;
    records::set_extension(pool, record_uid, NAMESPACE, &fds).await
}

pub async fn get_session(
    pool: &SqlitePool,
    session_uid: &str,
) -> Result<Option<SessionSidecar>, StoreError> {
    Ok(records::get_extension(pool, session_uid, SESSION_NAMESPACE)
        .await?
        .and_then(|fds| serde_json::from_value(fds).ok()))
}

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

pub async fn participants(
    pool: &SqlitePool,
    conversation_uid: &str,
) -> Result<Vec<RecordRow>, StoreError> {
    match concepts::resolve(pool, KIND_PARTICIPANT).await? {
        Some(kind) => assertions::objects_from_subject(pool, conversation_uid, &kind).await,
        None => Ok(Vec::new()),
    }
}

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
            continue;
        }
        let participants = match &participant {
            Some(kind) => assertions::objects_from_subject(pool, &conv.uid, kind).await?,
            None => Vec::new(),
        };
        let last_message =
            newest_message(pool, &conv.uid, thread_of.as_deref(), message_in.as_deref()).await?;
        let conv_created = records::created_at(pool, &conv.uid).await?;
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

    out.sort_by(|a, b| b.last_activity_at.cmp(&a.last_activity_at));
    Ok(out)
}

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

pub async fn sessions_of(
    pool: &SqlitePool,
    conversation_uid: &str,
) -> Result<Vec<RecordRow>, StoreError> {
    match concepts::resolve(pool, KIND_CALL_SESSION_OF).await? {
        Some(kind) => assertions::subjects_pointing_to(pool, &kind, conversation_uid).await,
        None => Ok(Vec::new()),
    }
}
