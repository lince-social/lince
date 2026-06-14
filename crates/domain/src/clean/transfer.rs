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
    Dependency,
}

storage_enum!(
    AgreementType,
    "agreement type",
    Individual => "individual",
    Full => "full",
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
