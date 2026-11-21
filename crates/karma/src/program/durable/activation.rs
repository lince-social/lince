//! Validated Frequency activations with their persisted compilation evidence.

use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de, ser::SerializeStruct};

use crate::{
    CanonicalHash, CompiledFrequency, FrequencyParameterValue, KarmaBoundaryError, LocalId,
    ReferenceKind, TimestampMs, TypedUid, canonical_hash,
    frequency::FREQUENCY_PARAMETER_HASH_DOMAIN,
};

pub const FREQUENCY_ACTIVATION_HASH_DOMAIN: &str = "karma.frequency-activation.v1";

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

#[derive(Clone, PartialEq, Eq)]
pub struct FrequencyActivationEpoch {
    schema: FrequencyActivationEpochSchema,
    frequency_uid: String,
    activating_handle_revision: u64,
    compiled: CompiledFrequency,
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
    /// Freezes the compiled Frequency at the activating handle revision.
    ///
    /// # Errors
    ///
    /// Rejects invalid identities, an initial handle revision, or a mismatched compilation hash.
    pub fn new(
        frequency_uid: String,
        activating_handle_revision: u64,
        definition_revision_hash: CanonicalHash,
        compiled: CompiledFrequency,
        previous_activation_hash: Option<CanonicalHash>,
        cause: FrequencyActivationCause,
        activated_at: TimestampMs,
    ) -> Result<Self, KarmaBoundaryError> {
        validate_activation(&frequency_uid, activating_handle_revision)?;
        if compiled.revision_hash != definition_revision_hash {
            return Err(KarmaBoundaryError::invalid_input(
                "Frequency activation definition hash disagrees with compilation",
            ));
        }
        validate_parameters(&compiled)?;
        Ok(Self {
            schema: FrequencyActivationEpochSchema::V1,
            frequency_uid,
            activating_handle_revision,
            compiled,
            previous_activation_hash,
            cause,
            activated_at,
        })
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
        &self.compiled.revision_hash
    }

    pub fn effective_parameter_hash(&self) -> &CanonicalHash {
        &self.compiled.effective_parameter_hash
    }

    pub fn effective_parameters(&self) -> &BTreeMap<LocalId, FrequencyParameterValue> {
        &self.compiled.effective_parameters
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
}

fn validate_activation(uid: &str, revision: u64) -> Result<(), KarmaBoundaryError> {
    TypedUid::validate(ReferenceKind::Frequency, uid)?;
    if revision < 2 {
        return Err(KarmaBoundaryError::invalid_input(
            "Frequency activation requires handle revision 2 or later",
        ));
    }
    Ok(())
}

fn validate_parameters(compiled: &CompiledFrequency) -> Result<(), KarmaBoundaryError> {
    if canonical_hash(
        FREQUENCY_PARAMETER_HASH_DOMAIN,
        &compiled.effective_parameters,
    )? != compiled.effective_parameter_hash
    {
        return Err(KarmaBoundaryError::invalid_input(
            "Frequency activation effective parameter hash is invalid",
        ));
    }
    Ok(())
}

impl Serialize for FrequencyActivationEpoch {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut record = serializer.serialize_struct(
            "FrequencyActivationEpoch",
            9 + usize::from(self.previous_activation_hash.is_some()),
        )?;
        record.serialize_field("schema", &self.schema)?;
        record.serialize_field("frequency_uid", &self.frequency_uid)?;
        record.serialize_field(
            "activating_handle_revision",
            &self.activating_handle_revision,
        )?;
        record.serialize_field("definition_revision_hash", self.definition_revision_hash())?;
        record.serialize_field("effective_parameter_hash", self.effective_parameter_hash())?;
        record.serialize_field("effective_parameters", self.effective_parameters())?;
        record.serialize_field("compiled", &self.compiled)?;
        if self.previous_activation_hash.is_some() {
            record.serialize_field("previous_activation_hash", &self.previous_activation_hash)?;
        } else {
            record.skip_field("previous_activation_hash")?;
        }
        record.serialize_field("cause", &self.cause)?;
        record.serialize_field("activated_at", &self.activated_at)?;
        record.end()
    }
}

impl fmt::Debug for FrequencyActivationEpoch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FrequencyActivationEpoch")
            .field("schema", &self.schema)
            .field("frequency_uid", &self.frequency_uid)
            .field(
                "activating_handle_revision",
                &self.activating_handle_revision,
            )
            .field("definition_revision_hash", self.definition_revision_hash())
            .field("effective_parameter_hash", self.effective_parameter_hash())
            .field("effective_parameters", self.effective_parameters())
            .field("compiled", &self.compiled)
            .field("previous_activation_hash", &self.previous_activation_hash)
            .field("cause", &self.cause)
            .field("activated_at", &self.activated_at)
            .finish()
    }
}

impl<'de> Deserialize<'de> for FrequencyActivationEpoch {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = FrequencyActivationEpochWire::deserialize(deserializer)?;
        validate_activation(&wire.frequency_uid, wire.activating_handle_revision)
            .map_err(de::Error::custom)?;
        if wire.compiled.revision_hash != wire.definition_revision_hash
            || wire.compiled.effective_parameter_hash != wire.effective_parameter_hash
            || wire.compiled.effective_parameters != wire.effective_parameters
        {
            return Err(de::Error::custom(KarmaBoundaryError::invalid_input(
                "Frequency activation compilation disagrees with its frozen identities",
            )));
        }
        validate_parameters(&wire.compiled).map_err(de::Error::custom)?;
        Ok(Self {
            schema: wire.schema,
            frequency_uid: wire.frequency_uid,
            activating_handle_revision: wire.activating_handle_revision,
            compiled: wire.compiled,
            previous_activation_hash: wire.previous_activation_hash,
            cause: wire.cause,
            activated_at: wire.activated_at,
        })
    }
}
