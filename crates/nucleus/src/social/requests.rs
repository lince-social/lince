use super::*;

pub const PARTICIPANTS_NAMESPACE: &str = "lince.social.participants";
pub const SESSION_AUTHORITY_NAMESPACE: &str = "lince.social.session-authority";
pub const MESSAGE_NAMESPACE: &str = "lince.social.message";
pub const DELIVERY_NAMESPACE: &str = "lince.social.delivery";
pub const REQUEST_DRAFT_NAMESPACE: &str = "lince.social.request-draft";
pub const BLOCK_NAMESPACE: &str = "lince.social.blocks";
pub const REVEAL_NAMESPACE: &str = "lince.social.reveal";
pub const MAX_ENVELOPE_BYTES: usize = 32 * 1024;
pub const MAX_CONTENT_BYTES: usize = 20 * 1024;
pub const MAX_MESSAGE_CONTENT_BYTES: usize = 6 * 1024 * 1024;
pub const MAX_ATTACHMENT_ENVELOPE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_INTRO_BYTES: usize = 2 * 1024;
pub const MAX_TEXT_BYTES: usize = 16 * 1024;
pub const AUTHORITY_LIFETIME: i64 = 7 * 86400;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrivateOwnerBinding {
    pub organ: String,
    pub context: String,
    pub owner_cell: String,
    pub owner_key: String,
    pub root_key: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeviceAuthorizationRequest {
    pub context: String,
    pub cell: String,
    pub operational_key: String,
    pub route: ReplyRoute,
    pub requested_at: i64,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OwnerControl {
    pub owner_key: String,
    pub generation: String,
    pub issued_at: i64,
    pub expires_at: i64,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeviceCertificate {
    pub owner_key: String,
    pub signing_key: String,
    pub identity_key: String,
    pub pickup_key: String,
    pub mailbox: String,
    pub generation: String,
    pub issued_at: i64,
    pub expires_at: i64,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CertifiedRoute {
    pub route: ReplyRoute,
    pub accepting_introductions: bool,
    pub control: OwnerControl,
    pub certificate: DeviceCertificate,
    pub expires_at: i64,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrivateEnvelope {
    pub protocol: String,
    pub id: String,
    pub route: String,
    pub sender_owner: String,
    pub sender_key: String,
    pub identity_key: String,
    pub control: OwnerControl,
    pub certificate: DeviceCertificate,
    pub session_id: String,
    pub message: String,
    pub content_hash: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub message_type: u8,
    pub purpose: EnvelopePurpose,
    pub ciphertext: String,
    pub signature: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum EnvelopePurpose {
    Introduction,
    Content,
    Control,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreshAuthorization {
    pub control: OwnerControl,
    pub certificate: DeviceCertificate,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrivateDelivery {
    pub envelope: PrivateEnvelope,
    pub authorization: FreshAuthorization,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MailboxAccess {
    pub mailbox: String,
    pub sequence: String,
    pub at: i64,
    pub nonce: String,
    pub envelopes: Vec<String>,
    pub control: OwnerControl,
    pub certificate: DeviceCertificate,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SenderAdmission {
    pub mailbox: String,
    pub sender_owner: String,
    pub state: AdmissionState,
    pub window: i64,
    pub issued_at: i64,
    pub expires_at: i64,
    pub control: OwnerControl,
    pub certificate: DeviceCertificate,
    pub signature: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum AdmissionState {
    Provisional,
    Accepted,
    Blocked,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ReceiptStage {
    RecipientDurable,
    RecipientRefused,
    ConversationReady,
    Read,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecipientReceipt {
    pub envelope: String,
    pub envelope_hash: String,
    pub message: String,
    pub content_hash: String,
    pub stage: ReceiptStage,
    pub at: i64,
    pub certificate: DeviceCertificate,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrivateContent {
    pub protocol: String,
    pub conversation: String,
    pub message: String,
    pub author_owner: String,
    pub issued_at: i64,
    pub kind: ContentKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConversationParticipant {
    pub token: String,
    pub context: String,
    pub local_owner: String,
    pub peer_owner: String,
    pub alias: String,
    pub routes: Vec<CertifiedRoute>,
    pub state: ConversationState,
    pub incoming: bool,
    pub started_at: i64,
    pub local_accepted: bool,
    pub peer_accepted: bool,
    pub provisional_sent: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ConversationState {
    Pending,
    Accepted,
    Declined,
    Blocked,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ContentKind {
    Introduction {
        post: String,
        text: String,
        alias: String,
        reply: Box<CertifiedRoute>,
    },
    Text {
        text: String,
    },
    Message {
        text: String,
        content: Vec<crate::message::MessagePart>,
    },
    Accept,
    Decline,
    Close,
    Reveal {
        profile: Box<Profile>,
        binding_signature: String,
    },
    ContactRequest,
    ContactAccept,
    Receipt {
        receipt: Box<RecipientReceipt>,
    },
    FreshRoute {
        reply: Box<CertifiedRoute>,
    },
}

impl ContentKind {
    pub fn purpose(&self) -> EnvelopePurpose {
        match self {
            Self::Introduction { .. } => EnvelopePurpose::Introduction,
            Self::Text { .. } | Self::Message { .. } => EnvelopePurpose::Content,
            _ => EnvelopePurpose::Control,
        }
    }

    pub fn parts(&self) -> &[crate::message::MessagePart] {
        match self {
            Self::Message { content, .. } => content,
            _ => &[],
        }
    }
}
