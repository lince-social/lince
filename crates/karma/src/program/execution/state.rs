//! Persisted node state and its revision chain.

use serde::{Deserialize, Serialize};

use super::validate_program_uid;
use crate::{
    CanonicalHash, ControlState, KarmaBoundaryError, LiteralValue, LocalId, canonical_hash,
};

pub const PROGRAM_STATE_EVENT_HASH_DOMAIN: &str = "karma.program-state-event.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProgramStateResetReason {
    ProgramActivation,
    RevisionChange,
    MigrationReset,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PersistedProgramNodeState {
    Delay { value: LiteralValue },
    Control { value: ControlState },
}

impl PersistedProgramNodeState {
    pub const fn kind_name(&self) -> &'static str {
        match self {
            Self::Delay { .. } => "delay",
            Self::Control { .. } => "control",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ProgramStateEventSchema {
    #[serde(rename = "karma.program-state-event.v1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgramStateEvent {
    pub schema: ProgramStateEventSchema,
    pub program_uid: String,
    pub node_id: LocalId,
    pub state_revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_event_hash: Option<CanonicalHash>,
    pub source_run_hash: CanonicalHash,
    pub definition_revision_hash: CanonicalHash,
    pub activation_handle_revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_reason: Option<ProgramStateResetReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<PersistedProgramNodeState>,
}

impl ProgramStateEvent {
    #[expect(
        clippy::too_many_arguments,
        reason = "The public constructor takes the fields of the execution record"
    )]
    pub fn new(
        program_uid: impl Into<String>,
        node_id: LocalId,
        state_revision: u64,
        previous_event_hash: Option<CanonicalHash>,
        source_run_hash: CanonicalHash,
        definition_revision_hash: CanonicalHash,
        activation_handle_revision: u64,
        reset_reason: Option<ProgramStateResetReason>,
        state: Option<PersistedProgramNodeState>,
    ) -> Result<Self, KarmaBoundaryError> {
        let event = Self {
            schema: ProgramStateEventSchema::V1,
            program_uid: program_uid.into(),
            node_id,
            state_revision,
            previous_event_hash,
            source_run_hash,
            definition_revision_hash,
            activation_handle_revision,
            reset_reason,
            state,
        };
        event.validate()?;
        Ok(event)
    }

    pub fn event_hash(&self) -> Result<CanonicalHash, KarmaBoundaryError> {
        self.validate()?;
        canonical_hash(PROGRAM_STATE_EVENT_HASH_DOMAIN, self)
    }

    pub fn validate(&self) -> Result<(), KarmaBoundaryError> {
        validate_program_uid(&self.program_uid)?;
        if self.schema != ProgramStateEventSchema::V1
            || self.state_revision == 0
            || self.activation_handle_revision == 0
            || (self.state_revision == 1) != self.previous_event_hash.is_none()
        {
            return Err(KarmaBoundaryError::invalid_input(
                "Program state event schema, revision chain, or activation revision is invalid",
            ));
        }
        if self.state.is_none() && self.reset_reason.is_none() {
            return Err(KarmaBoundaryError::invalid_input(
                "Program state tombstone requires an explicit reset reason",
            ));
        }
        Ok(())
    }
}
