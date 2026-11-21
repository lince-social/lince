use nucleus::RecordKind;
use sqlx::SqlitePool;

use crate::{
    StoreError,
    assertions::{objects_from_subject, subjects_pointing_to},
    records::{RecordRow, created_at},
    vocabulary::feature_read,
};

/// Display projection for an active Conversation.
#[derive(Debug, Clone)]
pub struct ConversationSummary {
    /// Conversation Record.
    pub record: RecordRow,
    /// Participant Records.
    pub participants: Vec<RecordRow>,
    /// Newest eligible Message, when present.
    pub last_message: Option<MessagePreview>,
    /// Latest Record or Message creation timestamp.
    pub last_activity_at: Option<String>,
}

/// The Message shown in a Conversation preview.
#[derive(Debug, Clone)]
pub struct MessagePreview {
    /// Message Record identity.
    pub uid: String,
    /// Message text.
    pub body: String,
    /// Observed Message creation timestamp.
    pub created_at: Option<String>,
}

/// Lists active tagged Conversations by latest activity, with participant and Message previews.
/// # Errors
/// Returns an error if the Vocabulary or any required Record, assertion, or timestamp cannot be read.
pub async fn conversations_by_tag(
    pool: &SqlitePool,
    tag_uid: &str,
) -> Result<Vec<ConversationSummary>, StoreError> {
    let Some(tagged) = feature_read(pool, "tagging").await? else {
        return Ok(Vec::new());
    };
    let participant = feature_read(pool, "participant").await?;
    let thread_of = feature_read(pool, "thread").await?;
    let message_in = feature_read(pool, "message").await?;

    let mut out = Vec::new();
    for conv in subjects_pointing_to(pool, &tagged, tag_uid).await? {
        if !conv.quantity.is_positive() {
            continue;
        }
        let participants = match &participant {
            Some(kind) => objects_from_subject(pool, &conv.uid, kind).await?,
            None => Vec::new(),
        };
        let last_message =
            newest_message(pool, &conv.uid, thread_of.as_deref(), message_in.as_deref()).await?;
        let conversation_at = created_at(pool, &conv.uid).await?;
        let message_at = last_message
            .as_ref()
            .and_then(|message| message.created_at.as_deref());
        let last_activity_at = if message_at > conversation_at.as_deref() {
            message_at.map(str::to_owned)
        } else {
            conversation_at
        };
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
    let mut best: Option<MessagePreview> = None;
    for thread in subjects_pointing_to(pool, thread_of, conversation_uid).await? {
        if thread.kind != RecordKind::Thread.as_str() || !thread.quantity.is_positive() {
            continue;
        }
        for message in subjects_pointing_to(pool, message_in, &thread.uid).await? {
            if message.kind != RecordKind::Message.as_str() || !message.quantity.is_positive() {
                continue;
            }
            let Some(created) = created_at(pool, &message.uid).await? else {
                continue;
            };
            if best
                .as_ref()
                .and_then(|preview| preview.created_at.as_deref())
                .is_none_or(|latest| created.as_str() > latest)
            {
                best = Some(MessagePreview {
                    uid: message.uid,
                    body: message.body,
                    created_at: Some(created),
                });
            }
        }
    }
    Ok(best)
}
