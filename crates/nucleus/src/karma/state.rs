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
