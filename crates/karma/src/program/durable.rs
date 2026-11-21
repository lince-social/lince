//! Durable evidence for Program and Frequency mutations.

use serde::{Deserialize, Serialize};

use crate::{CanonicalHash, DefinitionStatus, TimestampMs};

mod activation;

pub use activation::{
    FREQUENCY_ACTIVATION_HASH_DOMAIN, FrequencyActivationCause, FrequencyActivationEpoch,
    FrequencyActivationEpochSchema,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ProgramMutationEvidenceSchema {
    #[serde(rename = "karma.program-mutation-evidence.v1")]
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProgramMutationAction {
    Create,
    Revise,
    Activate,
    Pause,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgramMutationEvidence {
    pub schema: ProgramMutationEvidenceSchema,
    pub action: ProgramMutationAction,
    pub request_id: String,
    pub program_uid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_person_uid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_handle_revision: Option<u64>,
    pub handle_revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_head_revision_hash: Option<CanonicalHash>,
    pub head_revision_hash: CanonicalHash,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_active_revision_hash: Option<CanonicalHash>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_revision_hash: Option<CanonicalHash>,
    pub status: DefinitionStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum FrequencyMutationEvidenceSchema {
    #[serde(rename = "karma.frequency-mutation-evidence.v1")]
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FrequencyMutationAction {
    Create,
    Revise,
    Activate,
    SetParameters,
    ResetParameters,
    Pause,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrequencyMutationEvidence {
    pub schema: FrequencyMutationEvidenceSchema,
    pub action: FrequencyMutationAction,
    pub request_id: String,
    pub frequency_uid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_person_uid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_handle_revision: Option<u64>,
    pub handle_revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_head_revision_hash: Option<CanonicalHash>,
    pub head_revision_hash: CanonicalHash,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_active_revision_hash: Option<CanonicalHash>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_revision_hash: Option<CanonicalHash>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_activation_hash: Option<CanonicalHash>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activation_hash: Option<CanonicalHash>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_effective_parameter_hash: Option<CanonicalHash>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_parameter_hash: Option<CanonicalHash>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activated_at: Option<TimestampMs>,
    pub status: DefinitionStatus,
}
