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
pub enum IntentStatus {
    Staged,
    Authorized,
    Leased,
    Dispatching,
    Succeeded,
    Failed,
    Cancelled,
    Uncertain,
    Compensated,
    DeadLetter,
}

impl IntentStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Staged => "staged",
            Self::Authorized => "authorized",
            Self::Leased => "leased",
            Self::Dispatching => "dispatching",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Uncertain => "uncertain",
            Self::Compensated => "compensated",
            Self::DeadLetter => "dead-letter",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "staged" => Self::Staged,
            "authorized" => Self::Authorized,
            "leased" => Self::Leased,
            "dispatching" => Self::Dispatching,
            "succeeded" => Self::Succeeded,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            "uncertain" => Self::Uncertain,
            "compensated" => Self::Compensated,
            "dead-letter" => Self::DeadLetter,
            _ => return None,
        })
    }

    /// Whether an intent in this state still holds its share of a grant's
    /// budget. Only the states that could still cause work do; a cancelled
    /// intent releases what it reserved. States that arrive with execution
    /// (E0.3) are listed here so the accounting rule is stated once.
    pub const fn holds_reservation(self) -> bool {
        match self {
            Self::Authorized
            | Self::Leased
            | Self::Dispatching
            | Self::Succeeded
            | Self::Uncertain => true,
            Self::Staged
            | Self::Failed
            | Self::Cancelled
            | Self::Compensated
            | Self::DeadLetter => false,
        }
    }
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
    Grant,
    TrustScope,
    ActionIntent,
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
    CreateKarmaGrant,
    NarrowKarmaGrant,
    RevokeKarmaGrant,
    CreateAutomationTrustScope,
    ReviseAutomationTrustScope,
    ActivateAutomationTrustRevision,
    RespondKarmaCandidate,
    Decide,
    ControlKarmaWorkflow,
    ControlKarmaIntent,
    SimulateKarmaProgram,
    ImportKarmaTemplate,
}
