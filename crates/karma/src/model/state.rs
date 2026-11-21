//! Lifecycle states and stable labels used by Karma records and actions.

use serde::{Deserialize, Serialize};

/// The lifecycle state of a definition revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DefinitionStatus {
    Draft,
    Proven,
    Shadow,
    Active,
    Paused,
    Superseded,
    Retired,
}

impl DefinitionStatus {
    /// Returns the stored status label.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Proven => "proven",
            Self::Shadow => "shadow",
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Superseded => "superseded",
            Self::Retired => "retired",
        }
    }

    /// Parses a stored status label, preserving unknown labels as absence.
    pub fn parse(value: &str) -> Option<Self> {
        use DefinitionStatus::*;
        Some(match value {
            "draft" => Draft,
            "proven" => Proven,
            "shadow" => Shadow,
            "active" => Active,
            "paused" => Paused,
            "superseded" => Superseded,
            "retired" => Retired,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunStatus {
    Queued,
    Evaluating,
    Staged,
    Waiting,
    Executing,
    Completed,
    Failed,
    Cancelled,
    DeadLetter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CandidateStatus {
    Proposed,
    Accepted,
    Edited,
    Dismissed,
    Snoozed,
    Muted,
    Stale,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkflowStatus {
    Queued,
    Running,
    Waiting,
    Compensating,
    Completed,
    Failed,
    Cancelled,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum DatumState {
    Value,
    Missing,
    Stale,
    Denied,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EngineMode {
    Normal,
    ObserveOnly,
    StageEffects,
    EmergencyStop,
    Maintenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KarmaObjectKind {
    Program,
    ProgramRevision,
    Frequency,
    FrequencyRevision,
    Occurrence,
    Run,
    EvidenceSet,
    Model,
    ModelCheckpoint,
    Candidate,
    Decision,
    TrustScope,
    Attempt,
    Receipt,
    Workflow,
    Simulation,
    EngineHealth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KarmaActionKind {
    ValidateKarmaDefinition,
    CreateKarmaProgram,
    ForkKarmaProgram,
    ReviseKarmaProgram,
    ActivateKarmaRevision,
    CreateKarmaFrequency,
    ReviseKarmaFrequency,
    ActivateKarmaFrequencyRevision,
    SetKarmaParameter,
    ResetKarmaParameter,
    Deactivate,
    Activate,
    RetireKarmaProgram,
    RunKarmaProgram,
    ReplayKarmaRun,
    RebuildKarmaModel,
    DisableKarmaModel,
    CreateAutomationTrustScope,
    ReviseAutomationTrustScope,
    ActivateAutomationTrustRevision,
    RespondKarmaCandidate,
    Decide,
    ControlKarmaWorkflow,
    SimulateKarmaProgram,
    ImportKarmaTemplate,
}
