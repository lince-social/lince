use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub mod ask;
pub mod gossip;
pub mod reports;
pub mod requests;
pub mod subscriptions;

pub const PRIVATE_NAMESPACE: &str = "lince.social.private";
pub const PUBLICATION_NAMESPACE: &str = "lince.social.publication";
pub const PROFILE_NAMESPACE: &str = "lince.social.profile";
pub const MAX_SNIPPET_BYTES: usize = 6 * 1024;
pub const MAX_PROFILE_BYTES: usize = 16 * 1024;
pub const MAX_FRAME_BYTES: usize = 256 * 1024;
pub const MAX_PRIVATE_FRAME_BYTES: usize = requests::MAX_ATTACHMENT_ENVELOPE_BYTES + 64 * 1024;
pub const MAX_LIFETIME: i64 = 7 * 86400;
pub const CONVERSATION_VECTOR_PREFIX: &str = "lince.own-conversation/";

pub fn private_sync_field(table: &str, field: &str) -> bool {
    table == "data_visibility" || (table == "record_extension" && field.starts_with("lince.social."))
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

impl PostDraft {
    pub fn validate(&self) -> Result<(), String> {
        if self
            .lifetime_days
            .is_some_and(|days| !(1..=7).contains(&days))
        {
            return Err("Choose an announcement lifetime from one to seven days".into());
        }
        text(&self.title, 160, false)?;
        text(&self.text, 1200, true)?;
        text(&self.alias, 80, true)?;
        text(&self.language, 32, true)?;
        text(&self.area, 160, true)?;
        text(&self.availability, 160, true)?;
        if self.destinations.len() > 8
            || self
                .destinations
                .iter()
                .any(|s| s.is_empty() || s.len() > 128)
        {
            return Err("Choose at most eight valid publication services".into());
        }
        for value in [&self.unit, &self.concept].into_iter().flatten() {
            text(value, 160, false)?;
            if ["r_", "c_", "p_", "pl_"]
                .iter()
                .any(|prefix| value.starts_with(prefix))
            {
                return Err(
                    "Use a public concept/unit label rather than a private Record UID".into(),
                );
            }
        }
        if let Some(amount) = &self.quantity {
            let number = crate::DecimalValue::parse_inferred(amount)
                .map_err(|_| "Enter an exact decimal quantity")?;
            if number.is_zero()
                || number.is_negative()
                || self.unit.as_ref().is_none_or(String::is_empty)
            {
                return Err("An optional quantity must be positive and have a unit; direction is selected separately".into());
            }
        }
        if self.mode == AuthorMode::Identified && !self.alias.is_empty() {
            return Err("An anonymous alias belongs to anonymous publication".into());
        }
        Ok(())
    }
}

pub fn text(value: &str, max: usize, empty: bool) -> Result<(), String> {
    if (!empty && value.trim().is_empty())
        || value.chars().count() > max
        || value
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        Err(format!(
            "Use at most {max} characters and remove unsupported control characters"
        ))
    } else {
        Ok(())
    }
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
    pub fn permission(&self) -> &'static str {
        match self {
            Self::Overview
            | Self::Requests { .. }
            | Self::PrivateDeliveryStatus { .. }
            | Self::ReplyKeyStatus { .. }
            | Self::PostPage { .. }
            | Self::Search { .. }
            | Self::FetchProfile { .. }
            | Self::FetchProfileImage { .. } => "view:stream",
            Self::InspectService { .. } => "view:stream",
            Self::ServiceHealth => "view:stream",
            Self::Mutes { .. } | Self::RemovedListings { .. } => "view:stream",
            Self::Reports | Self::ReceivedReports { .. } => "view:stream",
            Self::Subscriptions { .. } | Self::SubscriptionResults { .. } => "view:stream",
            Self::AskStatus | Self::AskResults { .. } => "view:stream",
            Self::SaveProfile { .. }
            | Self::ResetPrivateSessions
            | Self::WithdrawProfile { .. }
            | Self::RotateProfileAuthority => "organ:update",
            Self::ConfigureServices { .. }
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
            | Self::ImportProfileImage { .. }
            | Self::ImportProfileImageData { .. } => "organ:update",
            _ => "record:update",
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
pub enum PublicRequest {
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
    pub fn frame_limit(&self) -> usize {
        match self {
            Self::DeliverPrivate { .. }
            | Self::InspectPrivate { .. }
            | Self::CollectPrivate { .. } => MAX_PRIVATE_FRAME_BYTES,
            _ => MAX_FRAME_BYTES,
        }
    }
}
