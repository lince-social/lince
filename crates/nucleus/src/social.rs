//! Social drafts, publication and private messaging wire contracts.
//!
//! [Command] classifies host permissions, [PublicRequest] defines relayed messages,
//! and [PostDraft] carries announcement editor input.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::visibility::plan::PRIVATE_NAMESPACE as AUDIENCE_PRIVATE_NAMESPACE;

pub mod ask;
pub mod gossip;
pub mod reports;
pub mod requests;
pub mod subscriptions;
pub mod transport;
mod validation;

pub use validation::text;

/// The owner-local namespace for private social data.
pub const PRIVATE_NAMESPACE: &str = "lince.social.private";
/// The namespace for announcement publication state.
pub const PUBLICATION_NAMESPACE: &str = "lince.social.publication";
/// The namespace for profile state.
pub const PROFILE_NAMESPACE: &str = "lince.social.profile";
/// The maximum serialized announcement size in bytes.
pub const MAX_SNIPPET_BYTES: usize = 6 * 1024;
/// The maximum serialized profile size in bytes.
pub const MAX_PROFILE_BYTES: usize = 16 * 1024;
/// The ordinary relay frame limit in bytes.
pub const MAX_FRAME_BYTES: usize = 256 * 1024;
/// The relay frame limit when carrying private attachment envelopes.
pub const MAX_PRIVATE_FRAME_BYTES: usize = requests::MAX_ATTACHMENT_ENVELOPE_BYTES + 64 * 1024;
/// The maximum announcement lifetime in seconds.
pub const MAX_LIFETIME: i64 = 7 * 86_400;
/// The synchronization vector prefix for the owner’s conversations.
pub const CONVERSATION_VECTOR_PREFIX: &str = "lince.own-conversation/";

/// Marks fields that belong in the owner’s private synchronization stream.
pub fn private_sync_field(table: &str, field: &str) -> bool {
    match table {
        "data_visibility" => true,
        "record_extension" => [
            "lince.stock-policy.private",
            AUDIENCE_PRIVATE_NAMESPACE,
            "lince.social.",
        ]
        .iter()
        .any(|prefix| field.starts_with(prefix)),
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum AuthorMode {
    #[default]
    Anonymous,
    Identified,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Direction {
    #[default]
    Need,
    Contribution,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum PostState {
    #[default]
    Active,
    Paused,
    Fulfilled,
    Withdrawn,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PostDraft {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<transport::Announcement>,
    pub title: String,
    pub text: String,
    pub direction: Direction,
    pub mode: AuthorMode,
    #[serde(default)]
    pub alias: String,
    #[serde(default)]
    pub quantity: Option<String>,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub concept: Option<String>,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub area: String,
    #[serde(default)]
    pub availability: String,
    #[serde(default)]
    pub redistribute: bool,
    #[serde(default)]
    pub destinations: Vec<String>,
    #[serde(default)]
    pub lifetime_days: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Delegation {
    pub organ: String,
    pub root_key: String,
    pub editor_key: String,
    pub generation: String,
    pub issued_at: i64,
    pub successions: Vec<RootSuccession>,
    pub expires_at: i64,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RootSuccession {
    pub old_key: String,
    pub new_key: String,
    pub created_at: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuthorityPublication {
    pub authority: Delegation,
    pub destinations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PostingAuthority {
    pub owner_key: String,
    pub editor_key: String,
    pub generation: String,
    pub issued_at: i64,
    pub expires_at: i64,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PostingAuthorityPublication {
    pub authority: PostingAuthority,
    pub destinations: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProfileFields {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub area: String,
    #[serde(default)]
    pub contact: String,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub banner: Option<String>,
    #[serde(default)]
    pub claims: Vec<crate::trust_analysis::claims::Document>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub protocol: String,
    pub authority: Delegation,
    pub revision: String,
    pub parents: Vec<String>,
    pub issued_at: i64,
    pub expires_at: i64,
    pub fields: ProfileFields,
    pub state: PostState,
    pub destinations: Vec<String>,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReplyRoute {
    pub mailbox: String,
    pub pickup_key: String,
    pub signing_key: String,
    pub identity_key: String,
    pub prekey: String,
    pub services: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicImage {
    pub profile: Profile,
    pub hash: String,
    pub encoded: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Snippet {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<transport::Announcement>,
    pub protocol: String,
    pub id: String,
    pub nonce: String,
    pub revision: String,
    pub parent: Option<String>,
    #[serde(default)]
    pub resolves: Vec<String>,
    pub created_at: i64,
    pub issued_at: i64,
    pub expires_at: i64,
    pub mode: AuthorMode,
    pub signing_key: String,
    pub profile: Option<Delegation>,
    pub anonymous: Option<PostingAuthority>,
    pub alias: String,
    pub title: String,
    pub text: String,
    pub direction: Direction,
    pub quantity: Option<String>,
    pub unit: Option<String>,
    pub concept: Option<String>,
    pub language: String,
    pub area: String,
    pub availability: String,
    pub state: PostState,
    pub redistribute: bool,
    pub destinations: Vec<String>,
    pub reply: Option<requests::CertifiedRoute>,
    pub signature: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Search {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<transport::Filter>,
    pub text: String,
    #[serde(default)]
    pub direction: Option<Direction>,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub area: String,
    #[serde(default)]
    pub concept: String,
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub after: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "command", rename_all = "kebab-case", deny_unknown_fields)]
#[expect(
    clippy::large_enum_variant,
    reason = "Publish retains its public inline Snippet payload"
)]
pub enum Command {
    OpenRequest {
        post: Box<Snippet>,
        text: String,
        alias: String,
        services: Vec<String>,
    },
    SendPrivate {
        conversation: String,
        text: String,
    },
    DecideRequest {
        conversation: String,
        decision: RequestDecision,
    },
    Requests {
        #[serde(default)]
        after: Option<String>,
    },
    ResumePrivate {
        message: String,
    },
    PrivateDeliveryStatus {
        message: String,
    },
    ResendExpiredPrivate {
        message: String,
    },
    ResumeRequest {
        record: String,
    },
    ArchiveRequest {
        record: String,
    },
    UnblockParticipant {
        context: String,
        peer: String,
    },
    DiscardPrivate {
        context: String,
        service: String,
        envelope: String,
    },
    RevealProfile {
        conversation: String,
    },
    ConnectParticipant {
        conversation: String,
    },
    ResetPrivateSessions,
    PrepareReplyKeys {
        record: String,
        services: Vec<String>,
    },
    ReplyKeyStatus {
        record: String,
    },
    ArchivePost {
        record: String,
    },
    ImportProfileImage {
        path: String,
    },
    ImportProfileImageData {
        encoded: String,
    },
    FetchProfileImage {
        organ: String,
        hash: String,
        services: Vec<String>,
    },
    PrepareFromRecord {
        source: String,
    },
    ConfigureServices {
        settings: ServiceSettings,
    },
    ServiceHealth,
    RebuildPublicIndex,
    InspectService {
        endpoint: String,
    },
    SaveServer {
        choice: ServerChoice,
    },
    RemoveServer {
        endpoint: String,
    },
    ConfigureGossip {
        enabled: bool,
    },
    SetGossipContact {
        choice: gossip::ContactConsent,
    },
    ConfigureAsk {
        enabled: bool,
    },
    SetAskContact {
        choice: ask::ContactConsent,
    },
    StartAsk {
        query: Search,
        contacts: Vec<String>,
    },
    CancelAsk {
        id: String,
    },
    ClearAsks,
    AskStatus,
    AskResults {
        id: String,
    },
    Overview,
    SaveSubscription {
        filter: Box<subscriptions::Subscription>,
    },
    RemoveSubscription {
        id: String,
    },
    ConfigureSubscriptions {
        enabled: bool,
    },
    Subscriptions {
        #[serde(default)]
        after: Option<String>,
    },
    SubscriptionResults {
        id: String,
    },
    ClearSubscriptionMatches {
        id: String,
    },
    PreviewReport {
        post: String,
        service: String,
        explanation: String,
    },
    SendReport {
        document: Box<reports::Report>,
        preview_hash: String,
    },
    Reports,
    ClearReports,
    ReceivedReports {
        #[serde(default)]
        after: Option<String>,
    },
    DismissReport {
        id: String,
    },
    MutePost {
        post: String,
        whole_author: bool,
    },
    Unmute {
        key: String,
    },
    Mutes {
        #[serde(default)]
        after: Option<String>,
    },
    RemoveListing {
        post: String,
        reason: String,
    },
    RestoreListing {
        post: String,
    },
    RemovedListings {
        #[serde(default)]
        after: Option<String>,
    },
    PostPage {
        after: String,
    },
    SaveDraft {
        record: Option<String>,
        source: Option<String>,
        draft: PostDraft,
    },
    Preview {
        record: String,
        state: PostState,
    },
    Publish {
        record: String,
        preview_hash: String,
        document: Snippet,
    },
    SaveProfile {
        fields: ProfileFields,
        parents: Vec<String>,
        destinations: Vec<String>,
    },
    Search {
        query: Search,
        services: Vec<String>,
    },
    FetchProfile {
        organ: String,
        services: Vec<String>,
    },
    WithdrawProfile {
        parents: Vec<String>,
    },
    RotateProfileAuthority,
}

impl Command {
    /// Returns the host permission required for this social command.
    pub fn permission(&self) -> &'static str {
        match self {
            Self::Requests { .. }
            | Self::PrivateDeliveryStatus { .. }
            | Self::ReplyKeyStatus { .. }
            | Self::FetchProfileImage { .. }
            | Self::ServiceHealth
            | Self::InspectService { .. }
            | Self::AskStatus
            | Self::AskResults { .. }
            | Self::Overview
            | Self::Subscriptions { .. }
            | Self::SubscriptionResults { .. }
            | Self::Reports
            | Self::ReceivedReports { .. }
            | Self::Mutes { .. }
            | Self::RemovedListings { .. }
            | Self::PostPage { .. }
            | Self::Search { .. }
            | Self::FetchProfile { .. } => "view:stream",
            Self::ResetPrivateSessions
            | Self::ImportProfileImage { .. }
            | Self::ImportProfileImageData { .. }
            | Self::ConfigureServices { .. }
            | Self::RebuildPublicIndex
            | Self::SaveServer { .. }
            | Self::RemoveServer { .. }
            | Self::ConfigureGossip { .. }
            | Self::SetGossipContact { .. }
            | Self::ConfigureAsk { .. }
            | Self::SetAskContact { .. }
            | Self::StartAsk { .. }
            | Self::CancelAsk { .. }
            | Self::ClearAsks
            | Self::SaveSubscription { .. }
            | Self::RemoveSubscription { .. }
            | Self::ConfigureSubscriptions { .. }
            | Self::ClearSubscriptionMatches { .. }
            | Self::PreviewReport { .. }
            | Self::SendReport { .. }
            | Self::ClearReports
            | Self::DismissReport { .. }
            | Self::MutePost { .. }
            | Self::Unmute { .. }
            | Self::RemoveListing { .. }
            | Self::RestoreListing { .. }
            | Self::SaveProfile { .. }
            | Self::WithdrawProfile { .. }
            | Self::RotateProfileAuthority => "organ:update",
            Self::OpenRequest { .. }
            | Self::SendPrivate { .. }
            | Self::DecideRequest { .. }
            | Self::ResumePrivate { .. }
            | Self::ResendExpiredPrivate { .. }
            | Self::ResumeRequest { .. }
            | Self::ArchiveRequest { .. }
            | Self::UnblockParticipant { .. }
            | Self::DiscardPrivate { .. }
            | Self::RevealProfile { .. }
            | Self::ConnectParticipant { .. }
            | Self::PrepareReplyKeys { .. }
            | Self::ArchivePost { .. }
            | Self::PrepareFromRecord { .. }
            | Self::SaveDraft { .. }
            | Self::Preview { .. }
            | Self::Publish { .. } => "record:update",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServerChoice {
    pub endpoint: String,
    pub label: String,
    pub operator: String,
    pub publication: bool,
    pub query: bool,
    pub mailbox: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organ: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<ServerPreset>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ServerPreset {
    InstituteDiscovery,
}

impl ServerChoice {
    /// Builds the optional Institute discovery preset.
    pub fn institute_helper() -> Self {
        Self {
            label: "Optional Institute discovery helper".into(),
            preset: Some(ServerPreset::InstituteDiscovery),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RequestDecision {
    Accept,
    Decline,
    Block,
    Close,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServiceSettings {
    pub directory: bool,
    pub townsquare: bool,
    pub mailbox: bool,
    pub relay: bool,
    pub gossip: bool,
    pub cache_entries: u32,
    pub storage_bytes: u64,
    pub incoming_bytes_per_minute: u64,
    pub outgoing_bytes_per_minute: u64,
    pub contact: String,
    pub policy: String,
}

impl Default for ServiceSettings {
    fn default() -> Self {
        Self {
            directory: false,
            townsquare: false,
            mailbox: false,
            relay: false,
            gossip: false,
            cache_entries: 10_000,
            storage_bytes: 1024 * 1024 * 1024,
            incoming_bytes_per_minute: 4 * 1024 * 1024,
            outgoing_bytes_per_minute: 4 * 1024 * 1024,
            contact: String::new(),
            policy: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServiceDescriptor {
    pub protocol: String,
    pub endpoint: String,
    pub roles: Vec<String>,
    pub settings: ServiceSettings,
    pub frame_bytes: u32,
    pub envelope_bytes: u32,
    pub mailbox_retention_days: u32,
    pub issued_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "request", rename_all = "kebab-case", deny_unknown_fields)]
#[expect(
    clippy::large_enum_variant,
    reason = "PublishSnippet retains its public inline Snippet payload"
)]
pub enum PublicRequest {
    CustodyBegin {
        offer: Box<crate::trust_analysis::custody::Offer>,
    },
    CustodyChunk {
        upload: String,
        offset: u32,
        bytes: String,
    },
    CustodyFinish {
        upload: String,
    },
    CustodyCancel {
        upload: String,
    },
    DiscoverEvidence {
        document: Box<crate::trust_analysis::discovery::Request>,
    },
    FetchEvidence {
        hash: crate::karma::CanonicalHash,
        offset: u32,
    },
    DescribeService,
    SubmitReport {
        document: Box<reports::Report>,
    },
    GossipOffer {
        offer: gossip::Offer,
    },
    GossipDeliver {
        payload: Box<gossip::Payload>,
    },
    AskContacts {
        document: Box<ask::Request>,
    },
    UpdateReplyAuthority {
        document: requests::OwnerControl,
    },
    LookupReplyRoutes {
        owner: String,
        after: Option<String>,
    },
    InspectPrivate {
        document: requests::PrivateDelivery,
    },
    RegisterReplyRoute {
        document: requests::CertifiedRoute,
        post: Option<Box<Snippet>>,
    },
    EndReplyPost {
        document: Box<Snippet>,
    },
    AdmitPrivateSender {
        document: requests::SenderAdmission,
    },
    DeliverPrivate {
        document: requests::PrivateDelivery,
    },
    CollectPrivate {
        access: requests::MailboxAccess,
    },
    AcknowledgePrivate {
        access: requests::MailboxAccess,
        receipts: Vec<requests::RecipientReceipt>,
    },
    DiscardPrivate {
        access: requests::MailboxAccess,
        receipts: Vec<requests::RecipientReceipt>,
    },
    PublishAuthority {
        document: AuthorityPublication,
    },
    PublishPostingAuthority {
        document: PostingAuthorityPublication,
    },
    PublishProfileImage {
        document: PublicImage,
    },
    FetchProfileImage {
        organ: String,
        hash: String,
    },
    PublishSnippet {
        document: Snippet,
    },
    PublishProfile {
        document: Profile,
    },
    Search {
        query: Search,
        #[serde(default)]
        known: Vec<String>,
    },
    FetchProfile {
        organ: String,
    },
}

impl PublicRequest {
    /// Returns the maximum serialized request size in bytes.
    pub fn frame_limit(&self) -> usize {
        match self {
            Self::DeliverPrivate { .. }
            | Self::InspectPrivate { .. }
            | Self::CollectPrivate { .. } => MAX_PRIVATE_FRAME_BYTES,
            _ => MAX_FRAME_BYTES,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AUDIENCE_PRIVATE_NAMESPACE, private_sync_field, text};

    #[test]
    fn private_sync_matches_only_the_owner_tables_and_prefixes() {
        assert!(private_sync_field("data_visibility", "any-field"));
        for field in [
            "lince.stock-policy.private.policy",
            AUDIENCE_PRIVATE_NAMESPACE,
            "lince.social.profile",
        ] {
            assert!(private_sync_field("record_extension", field));
            assert!(!private_sync_field("record", field));
        }
        assert!(!private_sync_field("record_extension", "lince.social"));
    }

    #[test]
    fn draft_text_limits_apply_to_unicode_characters_and_allowed_controls() {
        assert!(text("é🙂\t\n", 4, false).is_ok());
        assert!(text("é🙂\t\n", 3, false).is_err());
        assert!(text("é🙂\r", 4, true).is_err());
        assert!(text(" \t", 2, true).is_ok());
        assert!(text(" \t", 2, false).is_err());
    }
}
