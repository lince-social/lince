// non-lean-yet
// Scope: mod ask, mod gossip, mod reports, mod requests.
// BeautifulRust (baseline lines):
// - [review: §4 API] Check public bool parameters for a named mode or distinct operations. Sites
//   text:empty (139).
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 31, 39, 47, 310, 586, 602, 659.
// - [Clippy: clippy::large_enum_variant] large size difference between variants. Sites 310, 659.
// - [Clippy: clippy::map_err_ignore] Preserve relevant causes and context at the boundary. Sites
//   124.
// - [Clippy: clippy::match_same_arms] these match arms have identical bodies. Sites 521, 529, 530,
//   531, 532, 533, 534, 535, 539.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 87, 139.
// - [Clippy: clippy::struct_excessive_bools] more than 3 bools in a struct. Sites 611.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 4, 5,
//   6, 7, 8, 9, 11, 12, 13, 14, 15, 16, 17, 18, 19, 21, 31, 33, 34, 39, 41, 42, 47, 49, 50, 51, 52,
//   57, 59, 60, 61, 62, 63, 65, 67, 69, 71, 73, 75, 77, 79, 81, 83, 87, 139, 156, 157, 158, 159,
//   160, 161, 162, 163, 164, 169, 170, 171, 172, 173, 178, 179, 180, 185, 186, 187, 188, 189, 190,
//   191, 196, 197, 198, 203, 204, 205, 207, 209, 211, 213, 215, 220, 221, 222, 223, 224, 225, 226,
//   227, 228, 229, 230, 235, 236, 237, 238, 239, 240, 241, 246, 247, 248, 249, 254, 256, 257, 258,
//   259, 260, 261, 263, 264, 265, 266, 267, 268, 269, 270, 271, 272, 273, 274, 275, 276, 277, 278,
//   279, 280, 281, 282, 283, 284, 285, 290, 292, 293, 295, 297, 299, 301, 303, 305, 310, 311, 312,
//   313, 314, 315, 317, 318, 319, 321, 322, 323, 325, 327, 329, 330, 332, 333, 335, 336, 338, 339,
//   341, 342, 344, 345, 346, 348, 349, 350, 351, 353, 354, 356, 357, 359, 360, 361, 362, 364, 365,
//   367, 368, 370, 371, 373, 374, 376, 377, 378, 379, 381, 382, 384, 385, 387, 388, 389, 390, 392,
//   393, 395, 396, 398, 399, 401, 402, 404, 405, 407, 408, 410, 411, 412, 414, 415, 417, 418, 419,
//   420, 422, 423, 424, 426, 427, 429, 430, 432, 434, 436, 437, 439, 440, 442, 443, 444, 445, 447,
//   448, 449, 451, 452, 453, 455, 457, 458, 460, 461, 462, 464, 465, 467, 469, 471, 472, 473, 475,
//   476, 478, 480, 482, 483, 485, 486, 487, 488, 490, 491, 492, 494, 495, 496, 497, 499, 500, 501,
//   502, 504, 505, 506, 508, 509, 510, 512, 513, 515, 519, 571, 572, 573, 574, 575, 576, 577, 579,
//   581, 586, 587, 591, 602, 603, 604, 605, 606, 611, 612, 613, 614, 615, 616, 617, 618, 619, 620,
//   621, 622, 645, 646, 647, 648, 649, 650, 651, 652, 653, 654, 659, 660, 661, 663, 664, 665, 666,
//   668, 669, 671, 672, 674, 675, 677, 678, 679, 681, 682, 683, 685, 686, 688, 689, 691, 692, 694,
//   695, 697, 698, 699, 701, 702, 704, 705, 706, 708, 709, 711, 712, 714, 715, 717, 718, 720, 721,
//   722, 724, 725, 726, 728, 729, 731, 732, 734, 735, 737, 738, 739, 741, 742, 744, 745, 747, 748,
//   750, 752, 753, 758.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites text: String
//   (139), validate: String (87).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub mod ask;
pub mod gossip;
pub mod reports;
pub mod requests;
pub mod subscriptions;
pub mod transport;

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
    table == "data_visibility"
        || table == "record_extension" && field.starts_with("lince.stock-policy.private")
        || table == "record_extension"
            && field.starts_with(crate::visibility::plan::PRIVATE_NAMESPACE)
        || (table == "record_extension" && field.starts_with("lince.social."))
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

impl PostDraft {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(transport) = &self.transport {
            transport.validate()?;
        }
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
    pub fn frame_limit(&self) -> usize {
        match self {
            Self::DeliverPrivate { .. }
            | Self::InspectPrivate { .. }
            | Self::CollectPrivate { .. } => MAX_PRIVATE_FRAME_BYTES,
            _ => MAX_FRAME_BYTES,
        }
    }
}
