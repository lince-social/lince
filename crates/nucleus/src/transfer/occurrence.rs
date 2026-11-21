//! Activation, occurrence claims, and application review evidence.

use serde::{Deserialize, Serialize};

use super::{TransferLocationSnapshot, receipt};
use crate::fact::sha256_hex;

/// Occurrence terms at activation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransferOccurrenceSnapshot {
    pub uid: String,
    pub promise_uid: String,
    pub exchange_path_uid: String,
    #[serde(default)]
    pub exchange_uid: Option<String>,
    pub opposite_promise_uid: Option<String>,
    pub record_uid: Option<String>,
    pub concept_uid: Option<String>,
    pub unit_uid: Option<String>,
    pub quantity: f64,
    pub giver_person_uid: String,
    pub receiver_person_uid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub giver_organ_uid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receiver_organ_uid: Option<String>,
    pub window_start: Option<String>,
    pub window_end: Option<String>,
    pub location: Option<TransferLocationSnapshot>,
}

/// Signed activation and the occurrences it created.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransferActivationEvidence {
    pub action: String,
    pub idempotency_key: String,
    pub transfer_uid: String,
    pub revision: u64,
    pub actor_person_uid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision_fact_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub original_authorities: Vec<receipt::Authority>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub original_authority_records: Vec<String>,
    pub occurrences: Vec<TransferOccurrenceSnapshot>,
}

/// Which side of the occurrence the acting Person attests to.
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

/// Signed addition or withdrawal of an occurrence claim.
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

/// Occurrence version and claim state bound by a bulk review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferBulkClaimReviewItem {
    pub occurrence_uid: String,
    pub transfer_uid: String,
    pub transfer_revision: u64,
    pub role: OccurrenceClaimRole,
    pub delivery_claimed: bool,
    pub receipt_claimed: bool,
}

/// Hash the actor and reviewed claims in canonical occurrence and role order.
///
/// Claims with the same occurrence and role retain their input order.
pub fn transfer_bulk_claim_review_token(
    actor_person_uid: &str,
    items: &[TransferBulkClaimReviewItem],
) -> String {
    let mut items: Vec<_> = items.iter().collect();
    items.sort_by(|left, right| {
        left.occurrence_uid
            .cmp(&right.occurrence_uid)
            .then_with(|| left.role.as_str().cmp(right.role.as_str()))
    });
    let canonical = serde_json::to_vec(&(actor_person_uid, items))
        .expect("review values contain only JSON primitives and an in-memory buffer");
    sha256_hex(&canonical)
}

/// Reviewed application formula and the Person applying it.
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

/// Hash the exact application formula bytes used by a review.
pub fn occurrence_application_formula_hash(formula: &str) -> String {
    sha256_hex(formula.as_bytes())
}

/// Signed change to an occurrence’s disputed state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferOccurrenceDisputeEvidence {
    pub action: String,
    pub idempotency_key: String,
    pub transfer_uid: String,
    pub occurrence_uid: String,
    pub actor_person_uid: String,
    pub disputed: bool,
}
