// non-lean-yet
// Scope: enum DefinitionStatus, enum RunStatus, enum CandidateStatus, enum WorkflowStatus.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 5, 44, 58, 71, 84, 94, 104, 126.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 5, 6,
//   7, 8, 9, 10, 11, 12, 16, 28, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 58, 59, 60, 61, 62, 63,
//   64, 65, 66, 71, 72, 73, 74, 75, 76, 77, 78, 84, 85, 86, 87, 88, 89, 94, 95, 96, 97, 98, 99,
//   104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 126,
//   127, 128, 129, 130, 131, 132, 133, 134, 135, 136, 137, 138, 139, 140, 141, 142, 143, 144, 145,
//   146, 147, 148, 149, 150, 151.

use serde::{Deserialize, Serialize};

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

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "draft" => Self::Draft,
            "proven" => Self::Proven,
            "shadow" => Self::Shadow,
            "active" => Self::Active,
            "paused" => Self::Paused,
            "superseded" => Self::Superseded,
            "retired" => Self::Retired,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
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
