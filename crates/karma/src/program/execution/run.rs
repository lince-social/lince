//! Run outcomes and validation of their recorded replay seal.

use serde::{Deserialize, Serialize};

use super::validate_program_uid;
use crate::{
    CanonicalHash, EvaluationError, EvaluationLimits, FrozenEvaluationContext, KarmaBoundaryError,
    SealedEvaluationReplayCapsule, TimestampMs, canonical_hash,
};

pub const KARMA_RUN_HASH_DOMAIN: &str = "karma.run.v1";

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
#[expect(
    clippy::large_enum_variant,
    reason = "The public outcome variants expose their replay and failure fields inline"
)]
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
    #[expect(
        clippy::too_many_arguments,
        reason = "The public constructor takes the fields of the execution record"
    )]
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
        validate_program_uid(&self.program_uid)?;
        if let KarmaRunOutcome::Succeeded { replay } = &self.outcome {
            let seal_hash = replay.capsule.seal_hash().map_err(|error| {
                KarmaBoundaryError::invalid_input(format!(
                    "successful Karma run replay capsule cannot be resealed: {error}"
                ))
            })?;
            if replay.capsule.program_revision_hash != self.program_revision_hash
                || replay.capsule.expected_result.revision_hash != self.program_revision_hash
                || seal_hash != replay.capsule_hash
            {
                return Err(KarmaBoundaryError::invalid_input(
                    "successful Karma run replay capsule disagrees with its Program revision or hash",
                ));
            }
        }
        Ok(())
    }
}
