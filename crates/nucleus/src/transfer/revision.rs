//! Canonical signed Transfer revisions and authored snapshot terms.

use serde::{Deserialize, Serialize};

use super::{TransferRevisionDependency, disclosure, public_history};

/// Authored terms signed together as one Transfer revision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransferRevisionSnapshot {
    pub revision: u64,
    pub transfer: TransferRevisionTerms,
    pub parties: Vec<TransferRevisionParty>,
    pub invitations: Vec<TransferRevisionInvitation>,
    pub promises: Vec<TransferRevisionPromise>,
    #[serde(default)]
    pub dependencies: Vec<TransferRevisionDependency>,
    #[serde(default)]
    pub cancellations: Vec<TransferCancellationTerms>,
    #[serde(default)]
    pub children: Vec<TransferChildTerms>,
}

impl TransferRevisionSnapshot {
    /// Stably sort snapshot collections by UID before serialization and signing.
    pub fn canonicalize(&mut self) {
        self.parties.sort_by(|a, b| a.uid.cmp(&b.uid));
        self.invitations.sort_by(|a, b| a.uid.cmp(&b.uid));
        self.promises.sort_by(|a, b| a.uid.cmp(&b.uid));
        self.dependencies.sort_by(|a, b| a.uid.cmp(&b.uid));
        self.cancellations.sort_by(|a, b| a.uid.cmp(&b.uid));
        self.children.sort_by(|a, b| a.uid.cmp(&b.uid));
    }
}

/// A child Transfer and whether completion depends on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferChildTerms {
    pub uid: String,
    pub required: bool,
}

/// Canonical remainder and required participants for a cancellation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransferCancellationTerms {
    pub uid: String,
    pub exchange_path_uid: String,
    pub occurrences: Vec<String>,
    pub quantity: crate::DecimalValue,
    pub required_people: Vec<String>,
}

/// Transfer-level terms shared by the signed Promises.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransferRevisionTerms {
    pub uid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_organ_uid: Option<String>,
    pub slug: Option<String>,
    pub head: String,
    pub agreement_type: String,
    pub agreement_pct: Option<i64>,
    pub settlement: String,
    pub visibility: String,
    pub max_proximity: Option<i64>,
    pub satiation: Option<String>,
    pub parent_uid: Option<String>,
    pub source_uid: Option<String>,
    pub reserve_default: Option<String>,
    pub require_confirmation: bool,
    #[serde(default)]
    pub default_place: Option<TransferLocationSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_history: Option<public_history::Terms>,
}

/// Person and role bound to the signed revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferRevisionParty {
    pub uid: String,
    pub person_uid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organ_uid: Option<String>,
    pub kind: String,
}

/// Invitation and attempt state included in a revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferRevisionInvitation {
    pub uid: String,
    #[serde(default = "default_invitation_attempt")]
    pub attempt: u64,
    pub addressed_person_uid: String,
    pub invited_by_person_uid: String,
    pub status: String,
    pub expires_at: Option<String>,
}

fn default_invitation_attempt() -> u64 {
    1
}

/// Reviewed Promise terms with defaults for earlier signed revisions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransferRevisionPromise {
    pub uid: String,
    #[serde(default)]
    pub item: Option<disclosure::TransferItem>,
    #[serde(default)]
    pub source_promise_uid: Option<String>,
    pub revision: u64,
    pub record_uid: Option<String>,
    pub concept_uid: Option<String>,
    #[serde(default)]
    pub unit_uid: Option<String>,
    pub person_uid: Option<String>,
    pub delta: f64,
    #[serde(default)]
    pub window_start: Option<String>,
    pub window_end: Option<String>,
    #[serde(default)]
    pub location: Option<TransferLocationSnapshot>,
    pub condition: Option<String>,
    pub reserve_from: String,
    #[serde(default = "default_revision_promise_state")]
    pub state: String,
    #[serde(default)]
    pub open_reuse_policy: OpenPromiseReusePolicy,
}

fn default_revision_promise_state() -> String {
    "proposed".into()
}

/// Location values captured for review.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransferLocationSnapshot {
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub address: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[derive(schemars::JsonSchema)]
pub enum OpenPromiseReusePolicy {
    #[default]
    Duplicate,
    Consume,
}

impl OpenPromiseReusePolicy {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "duplicate" => Self::Duplicate,
            "consume" => Self::Consume,
            _ => return None,
        })
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Duplicate => "duplicate",
            Self::Consume => "consume",
        }
    }
}

/// Signed change and the resulting canonical snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransferRevisionEvidence {
    pub action: String,
    pub idempotency_key: Option<String>,
    pub previous_revision: Option<u64>,
    pub terms: TransferRevisionSnapshot,
}

/// Evidence of one invitation transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferInvitationLifecycleEvidence {
    pub action: String,
    pub idempotency_key: String,
    pub invitation_uid: String,
    pub transfer_uid: String,
    pub attempt: u64,
    pub from_status: Option<String>,
    pub to_status: String,
    pub actor_person_uid: Option<String>,
    pub revision: Option<u64>,
}
