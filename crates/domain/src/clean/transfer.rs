use {
    serde::{Deserialize, Serialize},
    std::{fmt, str::FromStr},
};

macro_rules! storage_enum {
    ($type_name:ident, $kind:literal, $($variant:ident => $storage:literal),+ $(,)?) => {
        impl $type_name {
            pub fn as_storage_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $storage,)+
                }
            }
        }

        impl FromStr for $type_name {
            type Err = TransferParseError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value {
                    $($storage => Ok(Self::$variant),)+
                    _ => Err(TransferParseError::new($kind, value)),
                }
            }
        }
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferParseError {
    value: String,
    kind: &'static str,
}

impl TransferParseError {
    fn new(kind: &'static str, value: &str) -> Self {
        Self {
            value: value.to_string(),
            kind,
        }
    }
}

impl fmt::Display for TransferParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid {} `{}`", self.kind, self.value)
    }
}

impl std::error::Error for TransferParseError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgreementType {
    Individual,
    Full,
    /// Percentage of parties that must reach level 2. Threshold stored in transfer_identity.agreement_percentage.
    Percentage,
    Dependency,
}

storage_enum!(
    AgreementType,
    "agreement type",
    Individual => "individual",
    Full => "full",
    Percentage => "percentage",
    Dependency => "dependency"
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettlementMode {
    Individual,
    Full,
}

storage_enum!(
    SettlementMode,
    "settlement mode",
    Individual => "individual",
    Full => "full"
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgreementLevel {
    None = 0,
    First = 1,
    Second = 2,
}

impl AgreementLevel {
    pub fn as_storage_i64(self) -> i64 {
        self as i64
    }

    pub fn from_storage_i64(value: i64) -> Result<Self, TransferParseError> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::First),
            2 => Ok(Self::Second),
            _ => Err(TransferParseError::new(
                "agreement level",
                &value.to_string(),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferRole {
    Need,
    Contribution,
    Support,
    Task,
    Information,
    Reservation,
}

storage_enum!(
    TransferRole,
    "transfer role",
    Need => "need",
    Contribution => "contribution",
    Support => "support",
    Task => "task",
    Information => "information",
    Reservation => "reservation"
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferDirection {
    Incoming,
    Outgoing,
    Mutual,
    Informational,
}

storage_enum!(
    TransferDirection,
    "transfer direction",
    Incoming => "incoming",
    Outgoing => "outgoing",
    Mutual => "mutual",
    Informational => "informational"
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferInteractionKind {
    ContributesTo,
    DependsOn,
    Unblocks,
    Replaces,
    Informs,
}

storage_enum!(
    TransferInteractionKind,
    "transfer interaction kind",
    ContributesTo => "contributes_to",
    DependsOn => "depends_on",
    Unblocks => "unblocks",
    Replaces => "replaces",
    Informs => "informs"
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipationKind {
    Participant,
    Coordinator,
    Observer,
    Placeholder,
}

storage_enum!(
    ParticipationKind,
    "participation kind",
    Participant => "participant",
    Coordinator => "coordinator",
    Observer => "observer",
    Placeholder => "placeholder"
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmationKind {
    Delivery,
    Receipt,
}

storage_enum!(
    ConfirmationKind,
    "confirmation kind",
    Delivery => "delivery",
    Receipt => "receipt"
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferState {
    PublicProposal,
    Negotiation,
    Inactive,
    Draft,
    Agreed,
    InTransfer,
    Settled,
    Disputed,
}

storage_enum!(
    TransferState,
    "transfer state",
    PublicProposal => "public_proposal",
    Negotiation => "negotiation",
    Inactive => "inactive",
    Draft => "draft",
    Agreed => "agreed",
    InTransfer => "in_transfer",
    Settled => "settled",
    Disputed => "disputed"
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferRelationKind {
    Parent,
    DependsOn,
}

storage_enum!(
    TransferRelationKind,
    "transfer relation kind",
    Parent => "parent",
    DependsOn => "depends_on"
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferDependencyKind {
    MustAgree,
    MustActivate,
    MustDeliver,
    MustReceive,
    MustSettle,
}

storage_enum!(
    TransferDependencyKind,
    "transfer dependency kind",
    MustAgree => "must_agree",
    MustActivate => "must_activate",
    MustDeliver => "must_deliver",
    MustReceive => "must_receive",
    MustSettle => "must_settle"
);

/// Typed event payload variants. Each event kind maps to exactly one payload struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum TransferEventPayload {
    ProposalCreated(ProposalCreatedPayload),
    ProposalDuplicated(ProposalDuplicatedPayload),
    ItemCreated(ItemCreatedPayload),
    ItemEdited(ItemEditedPayload),
    AgreementChanged(AgreementChangedPayload),
    DeliveryConfirmed(ConfirmationPayload),
    ReceiptConfirmed(ConfirmationPayload),
    SettlementApplied(SettlementAppliedPayload),
    TransferInactivated(TransferInactivatedPayload),
    VisibilityWave(VisibilityWavePayload),
    PackageReceived(PackageReceiptPayload),
    PackageSeen(PackageReceiptPayload),
    InteractionCreated(InteractionChangedPayload),
    InteractionEdited(InteractionChangedPayload),
    MessageSent(MessageSentPayload),
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalCreatedPayload {
    pub title: Option<String>,
    pub local_role: Option<String>,
    pub quantity: Option<f64>,
    pub counterparty_label: Option<String>,
    pub target_organ_id: Option<i64>,
    pub target_organ_name: Option<String>,
    pub topic_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalDuplicatedPayload {
    pub source_transfer_uid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemCreatedPayload {
    pub role: Option<String>,
    pub title: Option<String>,
    pub item_uid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemEditedPayload {
    pub role: Option<String>,
    pub title: Option<String>,
    pub item_uid: Option<String>,
    pub quantity: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgreementChangedPayload {
    pub role: Option<String>,
    pub agreement_type: Option<String>,
    pub agreement_level: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmationPayload {
    pub role: Option<String>,
    pub record_id: Option<i64>,
    pub quantity: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementAppliedPayload {
    pub role: Option<String>,
    pub record_id: Option<i64>,
    pub quantity_delta: Option<f64>,
    pub next_quantity: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferInactivatedPayload {
    pub role: Option<String>,
    pub progress_reset: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisibilityWavePayload {
    pub previous_max_visible_proximity: Option<i64>,
    pub max_visible_proximity: Option<i64>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageReceiptPayload {
    pub source_base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionChangedPayload {
    pub interaction_uid: Option<String>,
    pub from_item_uid: Option<String>,
    pub to_item_uid: Option<String>,
    pub interaction_kind: Option<String>,
    pub direction: Option<String>,
    pub dependency_kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSentPayload {
    pub message_uid: Option<String>,
    pub interaction_uid: Option<String>,
    pub parent_message_uid: Option<String>,
}
