//! Settlement evidence, corrections, and source group results.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[derive(schemars::JsonSchema)]
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

/// Signed canonical allocation for one settlement slice.
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

/// The winner chosen for a shared Transfer source.
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

/// Evidence that a sibling lost its shared source.
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

/// Evidence linking a local application to a canonical settlement.
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

/// Evidence that a local settlement application was reversed.
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
