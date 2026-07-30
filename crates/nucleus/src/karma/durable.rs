use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, de};

use super::{
    CanonicalHash, CompiledFrequency, DefinitionStatus, FrequencyParameterValue,
    KarmaBoundaryError, LocalId, ReferenceKind, TimestampMs, TypedUid, canonical_hash,
    frequency::FREQUENCY_PARAMETER_HASH_DOMAIN,
};

pub const FREQUENCY_ACTIVATION_HASH_DOMAIN: &str = "karma.frequency-activation.v1";

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

/// Fact payload for one atomic mutation of a durable Program handle.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FrequencyActivationCause {
    ActivateRevision,
    SetParameters,
    ResetParameters,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum FrequencyActivationEpochSchema {
    #[serde(rename = "karma.frequency-activation-epoch.v1")]
    V1,
}

/// Immutable effective configuration consumed by scheduling cursors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrequencyActivationEpoch {
    schema: FrequencyActivationEpochSchema,
    frequency_uid: String,
    activating_handle_revision: u64,
    definition_revision_hash: CanonicalHash,
    effective_parameter_hash: CanonicalHash,
    effective_parameters: BTreeMap<LocalId, FrequencyParameterValue>,
    compiled: CompiledFrequency,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous_activation_hash: Option<CanonicalHash>,
    cause: FrequencyActivationCause,
    activated_at: TimestampMs,
}

#[derive(Deserialize)]
struct FrequencyActivationEpochWire {
    schema: FrequencyActivationEpochSchema,
    frequency_uid: String,
    activating_handle_revision: u64,
    definition_revision_hash: CanonicalHash,
    effective_parameter_hash: CanonicalHash,
    effective_parameters: BTreeMap<LocalId, FrequencyParameterValue>,
    compiled: CompiledFrequency,
    #[serde(default)]
    previous_activation_hash: Option<CanonicalHash>,
    cause: FrequencyActivationCause,
    activated_at: TimestampMs,
}

impl FrequencyActivationEpoch {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        frequency_uid: String,
        activating_handle_revision: u64,
        definition_revision_hash: CanonicalHash,
        compiled: CompiledFrequency,
        previous_activation_hash: Option<CanonicalHash>,
        cause: FrequencyActivationCause,
        activated_at: TimestampMs,
    ) -> Result<Self, KarmaBoundaryError> {
        TypedUid::new(ReferenceKind::Frequency, frequency_uid.clone())?;
        if activating_handle_revision < 2 {
            return Err(KarmaBoundaryError::invalid_input(
                "Frequency activation requires handle revision 2 or later",
            ));
        }
        if compiled.revision_hash != definition_revision_hash {
            return Err(KarmaBoundaryError::invalid_input(
                "Frequency activation definition hash disagrees with compilation",
            ));
        }
        let epoch = Self {
            schema: FrequencyActivationEpochSchema::V1,
            frequency_uid,
            activating_handle_revision,
            definition_revision_hash,
            effective_parameter_hash: compiled.effective_parameter_hash.clone(),
            effective_parameters: compiled.effective_parameters.clone(),
            compiled,
            previous_activation_hash,
            cause,
            activated_at,
        };
        epoch.validate()?;
        Ok(epoch)
    }

    pub fn activation_hash(&self) -> Result<CanonicalHash, KarmaBoundaryError> {
        canonical_hash(FREQUENCY_ACTIVATION_HASH_DOMAIN, self)
    }

    pub fn schema(&self) -> FrequencyActivationEpochSchema {
        self.schema
    }

    pub fn frequency_uid(&self) -> &str {
        &self.frequency_uid
    }

    pub fn activating_handle_revision(&self) -> u64 {
        self.activating_handle_revision
    }

    pub fn definition_revision_hash(&self) -> &CanonicalHash {
        &self.definition_revision_hash
    }

    pub fn effective_parameter_hash(&self) -> &CanonicalHash {
        &self.effective_parameter_hash
    }

    pub fn effective_parameters(&self) -> &BTreeMap<LocalId, FrequencyParameterValue> {
        &self.effective_parameters
    }

    pub fn compiled(&self) -> &CompiledFrequency {
        &self.compiled
    }

    pub fn previous_activation_hash(&self) -> Option<&CanonicalHash> {
        self.previous_activation_hash.as_ref()
    }

    pub fn cause(&self) -> FrequencyActivationCause {
        self.cause
    }

    pub fn activated_at(&self) -> TimestampMs {
        self.activated_at
    }

    fn validate(&self) -> Result<(), KarmaBoundaryError> {
        TypedUid::new(ReferenceKind::Frequency, self.frequency_uid.clone())?;
        if self.activating_handle_revision < 2 {
            return Err(KarmaBoundaryError::invalid_input(
                "Frequency activation requires handle revision 2 or later",
            ));
        }
        if self.compiled.revision_hash != self.definition_revision_hash
            || self.compiled.effective_parameter_hash != self.effective_parameter_hash
            || self.compiled.effective_parameters != self.effective_parameters
        {
            return Err(KarmaBoundaryError::invalid_input(
                "Frequency activation compilation disagrees with its frozen identities",
            ));
        }
        if canonical_hash(FREQUENCY_PARAMETER_HASH_DOMAIN, &self.effective_parameters)?
            != self.effective_parameter_hash
        {
            return Err(KarmaBoundaryError::invalid_input(
                "Frequency activation effective parameter hash is invalid",
            ));
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for FrequencyActivationEpoch {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = FrequencyActivationEpochWire::deserialize(deserializer)?;
        let epoch = Self {
            schema: wire.schema,
            frequency_uid: wire.frequency_uid,
            activating_handle_revision: wire.activating_handle_revision,
            definition_revision_hash: wire.definition_revision_hash,
            effective_parameter_hash: wire.effective_parameter_hash,
            effective_parameters: wire.effective_parameters,
            compiled: wire.compiled,
            previous_activation_hash: wire.previous_activation_hash,
            cause: wire.cause,
            activated_at: wire.activated_at,
        };
        epoch.validate().map_err(de::Error::custom)?;
        Ok(epoch)
    }
}

/// Fact payload for one atomic mutation of a durable Frequency handle.
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
