// non-lean-yet
// Scope: const PARTICIPANTS_NAMESPACE, const SESSION_AUTHORITY_NAMESPACE, const MESSAGE_NAMESPACE,
//   const DELIVERY_NAMESPACE.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 99, 148, 157, 207, 217.
// - [Clippy: clippy::wildcard_imports] Use explicit imports while preserving framework and test
//   conventions. Sites 1.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 3, 4,
//   5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 20, 21, 22, 23, 24, 25, 26, 31, 32, 33, 34, 35, 36,
//   37, 42, 43, 44, 45, 46, 47, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 66, 67, 68, 69, 70, 71, 72,
//   77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 99, 100, 101, 102, 107,
//   108, 109, 114, 115, 116, 121, 122, 123, 124, 125, 126, 127, 128, 129, 134, 135, 136, 137, 138,
//   139, 140, 141, 142, 143, 148, 149, 150, 151, 152, 157, 158, 159, 160, 161, 166, 167, 168, 169,
//   170, 171, 172, 173, 174, 179, 180, 181, 182, 183, 184, 185, 190, 191, 192, 193, 194, 195, 196,
//   197, 198, 199, 200, 201, 202, 207, 208, 209, 210, 211, 212, 217, 218, 219, 220, 221, 222, 224,
//   225, 227, 228, 229, 231, 232, 233, 234, 235, 236, 238, 239, 240, 241, 243, 244, 249, 257.

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
