use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{
    CandidateRoute, CandidateStatus, CanonicalHash, ControlState, EvaluationError,
    EvaluationLimits, FrozenEvaluationContext, KarmaBoundaryError, LiteralValue, LocalId,
    ReferenceKind, SealedEvaluationReplayCapsule, Slug, TimestampMs, TypedUid, canonical_hash,
};

pub const OCCURRENCE_PROGRAM_EPOCH_HASH_DOMAIN: &str = "karma.occurrence-program-epoch.v1";
pub const KARMA_RUN_HASH_DOMAIN: &str = "karma.run.v1";
pub const PROGRAM_STATE_EVENT_HASH_DOMAIN: &str = "karma.program-state-event.v1";
pub const KARMA_CANDIDATE_PROPOSAL_HASH_DOMAIN: &str = "karma.candidate-proposal.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum OccurrenceProgramEpochSchema {
    #[serde(rename = "karma.occurrence-program-epoch.v1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ProgramEpochMember {
    pub program_uid: String,
    pub revision_hash: CanonicalHash,
    pub activation_handle_revision: u64,
}

impl ProgramEpochMember {
    pub fn new(
        program_uid: impl Into<String>,
        revision_hash: CanonicalHash,
        activation_handle_revision: u64,
    ) -> Result<Self, KarmaBoundaryError> {
        let program_uid = program_uid.into();
        TypedUid::new(ReferenceKind::Program, program_uid.clone())?;
        if activation_handle_revision == 0 {
            return Err(KarmaBoundaryError::invalid_input(
                "Program epoch activation handle revision must be positive",
            ));
        }
        Ok(Self {
            program_uid,
            revision_hash,
            activation_handle_revision,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OccurrenceProgramEpoch {
    pub schema: OccurrenceProgramEpochSchema,
    pub occurrence_hash: CanonicalHash,
    pub cell_sequence: u64,
    pub members: Vec<ProgramEpochMember>,
}

impl OccurrenceProgramEpoch {
    pub fn new(
        occurrence_hash: CanonicalHash,
        cell_sequence: u64,
        members: Vec<ProgramEpochMember>,
    ) -> Result<Self, KarmaBoundaryError> {
        let epoch = Self {
            schema: OccurrenceProgramEpochSchema::V1,
            occurrence_hash,
            cell_sequence,
            members,
        };
        epoch.validate()?;
        Ok(epoch)
    }

    pub fn epoch_hash(&self) -> Result<CanonicalHash, KarmaBoundaryError> {
        self.validate()?;
        canonical_hash(OCCURRENCE_PROGRAM_EPOCH_HASH_DOMAIN, self)
    }

    pub fn validate(&self) -> Result<(), KarmaBoundaryError> {
        if self.schema != OccurrenceProgramEpochSchema::V1 || self.cell_sequence == 0 {
            return Err(KarmaBoundaryError::invalid_input(
                "occurrence Program epoch needs a supported schema and positive Cell sequence",
            ));
        }
        let mut previous: Option<&ProgramEpochMember> = None;
        for member in &self.members {
            TypedUid::new(ReferenceKind::Program, member.program_uid.clone())?;
            if member.activation_handle_revision == 0 {
                return Err(KarmaBoundaryError::invalid_input(
                    "Program epoch activation handle revision must be positive",
                ));
            }
            if previous.is_some_and(|value| value >= member) {
                return Err(KarmaBoundaryError::invalid_input(
                    "occurrence Program epoch members must be unique and strictly sorted",
                ));
            }
            if previous.is_some_and(|value| value.program_uid == member.program_uid) {
                return Err(KarmaBoundaryError::invalid_input(
                    "occurrence Program epoch cannot contain two revisions of one Program",
                ));
            }
            previous = Some(member);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum KarmaRunSchema {
    #[serde(rename = "karma.run.v1")]
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProgramNotApplicableReason {
    NoMatchingTrigger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProgramRunBlockCode {
    StatefulRuntimeUnavailable,
    UnsupportedStatePersistence,
    StateMigrationRequired,
    RecordQuantityUnavailable,
    RecordUnitUntypable,
    InputSourceUnresolved,
}

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
    #[allow(clippy::too_many_arguments)]
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
        TypedUid::new(ReferenceKind::Program, self.program_uid.clone())?;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum KarmaCandidateProposalSchema {
    #[serde(rename = "karma.candidate-proposal.v1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum CandidateReviewAction {
    Accept,
    Dismiss,
    Snooze { until: TimestampMs },
}

impl CandidateReviewAction {
    pub const fn action_name(&self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Dismiss => "dismiss",
            Self::Snooze { .. } => "snooze",
        }
    }

    pub const fn resulting_status(&self) -> CandidateStatus {
        match self {
            Self::Accept => CandidateStatus::Accepted,
            Self::Dismiss => CandidateStatus::Dismissed,
            Self::Snooze { .. } => CandidateStatus::Snoozed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum CandidateReviewEvidenceSchema {
    #[serde(rename = "karma.candidate-review-evidence.v1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateReviewEvidence {
    pub schema: CandidateReviewEvidenceSchema,
    pub request_id: String,
    pub candidate_hash: CanonicalHash,
    pub actor_person_uid: Option<String>,
    pub previous_state_revision: u64,
    pub state_revision: u64,
    pub action: CandidateReviewAction,
    pub status: CandidateStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KarmaCandidateProposal {
    pub schema: KarmaCandidateProposalSchema,
    pub source_run_hash: CanonicalHash,
    pub occurrence_hash: CanonicalHash,
    pub program_uid: String,
    pub program_revision_hash: CanonicalHash,
    pub node_id: LocalId,
    pub output: LocalId,
    pub route: CandidateRoute,
    pub template: Slug,
    pub fields: BTreeMap<LocalId, LiteralValue>,
    pub status: CandidateStatus,
}

impl KarmaCandidateProposal {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source_run_hash: CanonicalHash,
        occurrence_hash: CanonicalHash,
        program_uid: impl Into<String>,
        program_revision_hash: CanonicalHash,
        node_id: LocalId,
        output: LocalId,
        route: CandidateRoute,
        template: Slug,
        fields: BTreeMap<LocalId, LiteralValue>,
    ) -> Result<Self, KarmaBoundaryError> {
        let proposal = Self {
            schema: KarmaCandidateProposalSchema::V1,
            source_run_hash,
            occurrence_hash,
            program_uid: program_uid.into(),
            program_revision_hash,
            node_id,
            output,
            route,
            template,
            fields,
            status: CandidateStatus::Proposed,
        };
        proposal.validate()?;
        Ok(proposal)
    }

    pub fn candidate_hash(&self) -> Result<CanonicalHash, KarmaBoundaryError> {
        self.validate()?;
        canonical_hash(KARMA_CANDIDATE_PROPOSAL_HASH_DOMAIN, self)
    }

    pub fn validate(&self) -> Result<(), KarmaBoundaryError> {
        TypedUid::new(ReferenceKind::Program, self.program_uid.clone())?;
        if self.schema != KarmaCandidateProposalSchema::V1
            || self.status != CandidateStatus::Proposed
        {
            return Err(KarmaBoundaryError::invalid_input(
                "candidate proposal requires the supported schema and proposed status",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum KarmaRunOutcome {
    Succeeded {
        replay: SealedEvaluationReplayCapsule,
    },
    NotApplicable {
        reason: ProgramNotApplicableReason,
    },
    Blocked {
        code: ProgramRunBlockCode,
    },
    EvaluationFailed {
        context: FrozenEvaluationContext,
        limits: EvaluationLimits,
        error: EvaluationError,
    },
}

impl KarmaRunOutcome {
    pub const fn status_name(&self) -> &'static str {
        match self {
            Self::Succeeded { .. } => "succeeded",
            Self::NotApplicable { .. } => "not-applicable",
            Self::Blocked { .. } => "blocked",
            Self::EvaluationFailed { .. } => "evaluation-failed",
        }
    }

    pub const fn fuel_used(&self) -> u64 {
        match self {
            Self::Succeeded { replay } => replay.capsule.expected_result.fuel_used,
            Self::EvaluationFailed { error, .. } => error.fuel_used,
            Self::NotApplicable { .. } | Self::Blocked { .. } => 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KarmaRun {
    pub schema: KarmaRunSchema,
    pub occurrence_hash: CanonicalHash,
    pub cell_sequence: u64,
    pub logical_at: TimestampMs,
    pub program_epoch_hash: CanonicalHash,
    pub member_ordinal: u64,
    pub program_uid: String,
    pub program_revision_hash: CanonicalHash,
    pub outcome: KarmaRunOutcome,
}

impl KarmaRun {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        occurrence_hash: CanonicalHash,
        cell_sequence: u64,
        logical_at: TimestampMs,
        program_epoch_hash: CanonicalHash,
        member_ordinal: u64,
        program_uid: impl Into<String>,
        program_revision_hash: CanonicalHash,
        outcome: KarmaRunOutcome,
    ) -> Result<Self, KarmaBoundaryError> {
        let run = Self {
            schema: KarmaRunSchema::V1,
            occurrence_hash,
            cell_sequence,
            logical_at,
            program_epoch_hash,
            member_ordinal,
            program_uid: program_uid.into(),
            program_revision_hash,
            outcome,
        };
        run.validate()?;
        Ok(run)
    }

    pub fn run_hash(&self) -> Result<CanonicalHash, KarmaBoundaryError> {
        self.validate()?;
        canonical_hash(KARMA_RUN_HASH_DOMAIN, self)
    }

    pub fn validate(&self) -> Result<(), KarmaBoundaryError> {
        if self.schema != KarmaRunSchema::V1 || self.cell_sequence == 0 {
            return Err(KarmaBoundaryError::invalid_input(
                "Karma run needs a supported schema and positive Cell sequence",
            ));
        }
        TypedUid::new(ReferenceKind::Program, self.program_uid.clone())?;
        if let KarmaRunOutcome::Succeeded { replay } = &self.outcome {
            let resealed = replay.capsule.clone().seal().map_err(|error| {
                KarmaBoundaryError::invalid_input(format!(
                    "successful Karma run replay capsule cannot be resealed: {error}"
                ))
            })?;
            if replay.capsule.program_revision_hash != self.program_revision_hash
                || replay.capsule.expected_result.revision_hash != self.program_revision_hash
                || resealed.capsule_hash != replay.capsule_hash
            {
                return Err(KarmaBoundaryError::invalid_input(
                    "successful Karma run replay capsule disagrees with its Program revision or hash",
                ));
            }
        }
        Ok(())
    }
}
