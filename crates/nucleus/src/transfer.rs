use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransferRevisionSnapshot {
    pub revision: u64,
    pub transfer: TransferRevisionTerms,
    pub parties: Vec<TransferRevisionParty>,
    pub invitations: Vec<TransferRevisionInvitation>,
    pub promises: Vec<TransferRevisionPromise>,
    #[serde(default)]
    pub dependencies: Vec<TransferRevisionDependency>,
}

impl TransferRevisionSnapshot {
    pub fn canonicalize(&mut self) {
        self.parties.sort_by(|a, b| a.uid.cmp(&b.uid));
        self.invitations.sort_by(|a, b| a.uid.cmp(&b.uid));
        self.promises.sort_by(|a, b| a.uid.cmp(&b.uid));
        self.dependencies.sort_by(|a, b| a.uid.cmp(&b.uid));
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransferRevisionTerms {
    pub uid: String,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferRevisionParty {
    pub uid: String,
    pub person_uid: String,
    pub kind: String,
}

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransferRevisionPromise {
    pub uid: String,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferDependencyScope {
    Transfer,
    Promise,
}

impl TransferDependencyScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Transfer => "transfer",
            Self::Promise => "promise",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "transfer" => Self::Transfer,
            "promise" => Self::Promise,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferDependencyUpstreamKind {
    Transfer,
    Promise,
}

impl TransferDependencyUpstreamKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Transfer => "transfer",
            Self::Promise => "promise",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "transfer" => Self::Transfer,
            "promise" => Self::Promise,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferRevisionDependency {
    pub uid: String,
    pub scope: TransferDependencyScope,
    pub promise_uid: Option<String>,
    pub upstream_kind: TransferDependencyUpstreamKind,
    pub upstream_uid: String,
    #[serde(default = "default_dependency_required_state")]
    pub required_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "uid", rename_all = "snake_case")]
pub enum TransferDependencyNode {
    Transfer(String),
    Promise(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferDependencyEdge {
    pub upstream: TransferDependencyNode,
    pub downstream: TransferDependencyNode,
}

pub fn transfer_dependency_order(
    edges: &[TransferDependencyEdge],
) -> Result<Vec<TransferDependencyNode>, Vec<TransferDependencyNode>> {
    let mut outgoing: BTreeMap<TransferDependencyNode, BTreeSet<TransferDependencyNode>> =
        BTreeMap::new();
    let mut incoming: BTreeMap<TransferDependencyNode, usize> = BTreeMap::new();
    for edge in edges {
        incoming.entry(edge.upstream.clone()).or_default();
        incoming.entry(edge.downstream.clone()).or_default();
        if outgoing
            .entry(edge.upstream.clone())
            .or_default()
            .insert(edge.downstream.clone())
        {
            *incoming.entry(edge.downstream.clone()).or_default() += 1;
        }
    }
    let mut ready: BTreeSet<_> = incoming
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(node, _)| node.clone())
        .collect();
    let mut ordered = Vec::with_capacity(incoming.len());
    while let Some(node) = ready.pop_first() {
        if let Some(children) = outgoing.get(&node) {
            for child in children {
                let count = incoming.get_mut(child).expect("graph node is indexed");
                *count -= 1;
                if *count == 0 {
                    ready.insert(child.clone());
                }
            }
        }
        ordered.push(node);
    }
    if ordered.len() == incoming.len() {
        Ok(ordered)
    } else {
        Err(incoming
            .into_iter()
            .filter(|(_, count)| *count != 0)
            .map(|(node, _)| node)
            .collect())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferDependencyReadiness {
    pub dependency_uid: String,
    pub upstream: TransferDependencyNode,
    pub required_state: String,
    pub current_state: Option<String>,
}

impl TransferDependencyReadiness {
    pub fn ready(&self) -> bool {
        self.current_state.as_deref() == Some(self.required_state.as_str())
    }
}

pub fn ordered_transfer_dependency_readiness(
    mut values: Vec<TransferDependencyReadiness>,
) -> Vec<TransferDependencyReadiness> {
    values.sort_by(|left, right| {
        left.ready()
            .cmp(&right.ready())
            .then_with(|| left.upstream.cmp(&right.upstream))
            .then_with(|| left.dependency_uid.cmp(&right.dependency_uid))
    });
    values
}

fn default_dependency_required_state() -> String {
    "kept".into()
}

fn default_revision_promise_state() -> String {
    "proposed".into()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransferLocationSnapshot {
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub address: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransferRevisionEvidence {
    pub action: String,
    pub idempotency_key: Option<String>,
    pub previous_revision: Option<u64>,
    pub terms: TransferRevisionSnapshot,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferAgreementTransitionEvidence {
    pub action: String,
    pub idempotency_key: String,
    pub transfer_uid: String,
    pub revision: u64,
    pub party_uid: String,
    pub person_uid: String,
    pub from_level: u8,
    pub to_level: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransferOccurrenceSnapshot {
    pub uid: String,
    pub promise_uid: String,
    pub exchange_path_uid: String,
    pub opposite_promise_uid: Option<String>,
    pub record_uid: Option<String>,
    pub concept_uid: Option<String>,
    pub unit_uid: Option<String>,
    pub quantity: f64,
    pub giver_person_uid: String,
    pub receiver_person_uid: String,
    pub window_start: Option<String>,
    pub window_end: Option<String>,
    pub location: Option<TransferLocationSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransferActivationEvidence {
    pub action: String,
    pub idempotency_key: String,
    pub transfer_uid: String,
    pub revision: u64,
    pub actor_person_uid: String,
    pub occurrences: Vec<TransferOccurrenceSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OccurrenceClaimRole {
    Delivery,
    Receipt,
}

impl OccurrenceClaimRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Delivery => "delivery",
            Self::Receipt => "receipt",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "delivery" => Self::Delivery,
            "receipt" => Self::Receipt,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferOccurrenceClaimEvidence {
    pub action: String,
    pub idempotency_key: String,
    pub transfer_uid: String,
    pub occurrence_uid: String,
    pub role: OccurrenceClaimRole,
    pub asserted: bool,
    pub actor_person_uid: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferBulkClaimReviewItem {
    pub occurrence_uid: String,
    pub transfer_uid: String,
    pub transfer_revision: u64,
    pub role: OccurrenceClaimRole,
    pub delivery_claimed: bool,
    pub receipt_claimed: bool,
}

pub fn transfer_bulk_claim_review_token(
    actor_person_uid: &str,
    items: &[TransferBulkClaimReviewItem],
) -> String {
    let mut items = items.to_vec();
    items.sort_by(|left, right| {
        left.occurrence_uid
            .cmp(&right.occurrence_uid)
            .then_with(|| left.role.as_str().cmp(right.role.as_str()))
    });
    let canonical = serde_json::to_vec(&(actor_person_uid, items))
        .expect("Transfer bulk review values are serializable");
    let digest = Sha256::digest(canonical);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferOccurrenceApplicationEvidence {
    pub action: String,
    pub idempotency_key: String,
    pub transfer_uid: String,
    pub occurrence_uid: String,
    pub receiver_person_uid: String,
    pub formula_hash: String,
    pub version: u64,
}

pub fn occurrence_application_formula_hash(formula: &str) -> String {
    let digest = Sha256::digest(formula.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TransferRemainderPolicy {
    #[default]
    Visible,
    LocalDraft,
}

impl TransferRemainderPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Visible => "visible",
            Self::LocalDraft => "local_draft",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "visible" => Self::Visible,
            "local_draft" => Self::LocalDraft,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransferOccurrenceSettlementEvidence {
    pub action: String,
    pub idempotency_key: String,
    pub settlement_uid: String,
    pub transfer_uid: String,
    pub occurrence_uid: String,
    pub promise_uid: String,
    pub owner_person_uid: String,
    pub canonical_quantity: f64,
    pub canonical_unit_uid: Option<String>,
    pub cumulative_before: f64,
    pub cumulative_after: f64,
    pub remaining_after: f64,
    pub application_formula_hash: String,
    pub application_formula_version: u64,
    pub remainder_policy: TransferRemainderPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferSourceGroupResultEvidence {
    pub action: String,
    pub source_uid: String,
    pub policy: String,
    pub winner_transfer_uid: String,
    pub winner_revision: u64,
    pub settlement_uid: String,
    pub observed_by_person_uid: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferSourceGroupLoserEvidence {
    pub action: String,
    pub source_uid: String,
    pub policy: String,
    pub winner_transfer_uid: String,
    pub losing_transfer_uid: String,
    pub losing_revision: u64,
    pub result_uid: String,
    pub observed_by_person_uid: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransferOccurrenceSettlementApplicationEvidence {
    pub action: String,
    pub settlement_uid: String,
    pub transfer_uid: String,
    pub occurrence_uid: String,
    pub promise_uid: String,
    pub owner_person_uid: String,
    pub evidence_fact_uid: String,
    pub application_formula_hash: String,
    pub application_formula_version: u64,
    pub local_cumulative_before: f64,
    pub local_cumulative_after: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferOccurrenceSettlementCompensationEvidence {
    pub action: String,
    pub idempotency_key: String,
    pub compensation_uid: String,
    pub settlement_uid: String,
    pub transfer_uid: String,
    pub occurrence_uid: String,
    pub owner_person_uid: String,
    pub original_application_fact_uid: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferOccurrenceDisputeEvidence {
    pub action: String,
    pub idempotency_key: String,
    pub transfer_uid: String,
    pub occurrence_uid: String,
    pub actor_person_uid: String,
    pub disputed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgreementType {
    Individual,
    Full,
    Percentage,
    Dependency,
}

impl AgreementType {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "individual" => Self::Individual,
            "full" => Self::Full,
            "percentage" => Self::Percentage,
            "dependency" => Self::Dependency,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Individual => "individual",
            Self::Full => "full",
            Self::Percentage => "percentage",
            Self::Dependency => "dependency",
        }
    }
}

pub fn policy_satisfied(agreement: AgreementType, pct: Option<u8>, party_levels: &[i64]) -> bool {
    let n = party_levels.len();
    let committed = party_levels.iter().filter(|&&l| l >= 2).count();
    match agreement {
        AgreementType::Individual => true,
        AgreementType::Full => n > 0 && committed == n,
        AgreementType::Percentage => {
            let pct = pct.unwrap_or(100) as usize;
            n > 0 && committed * 100 >= n * pct
        }
        AgreementType::Dependency => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policies() {
        assert!(policy_satisfied(AgreementType::Individual, None, &[0, 0]));
        assert!(!policy_satisfied(AgreementType::Full, None, &[2, 1]));
        assert!(policy_satisfied(AgreementType::Full, None, &[2, 2]));
        assert!(!policy_satisfied(AgreementType::Full, None, &[]));
        assert!(!policy_satisfied(
            AgreementType::Percentage,
            Some(50),
            &[2, 0, 0]
        ));
        assert!(policy_satisfied(
            AgreementType::Percentage,
            Some(50),
            &[2, 2, 0]
        ));
        assert!(!policy_satisfied(
            AgreementType::Percentage,
            Some(80),
            &[2, 2, 0]
        ));
    }
}
