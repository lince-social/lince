//! Capture and verify complete deterministic Karma evaluations.

#![expect(
    clippy::result_large_err,
    reason = "The public replay error retains its inline typed evaluation failure"
)]

use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

use crate::{
    CanonicalHash, EvaluationError, EvaluationLimits, EvaluationResult, FailurePath,
    FrozenEvaluationContext, ProgramAst, canonical_hash, evaluate_program,
};

pub use super::proof::PROGRAM_REVISION_HASH_DOMAIN;

/// Canonical hash domain for evaluated results.
pub const EVALUATION_RESULT_HASH_DOMAIN: &str = "karma.evaluation-result.v1";
/// Canonical hash domain for the full replay capsule.
pub const EVALUATION_REPLAY_CAPSULE_HASH_DOMAIN: &str = "karma.evaluation-replay-capsule.v1";

const CAPSULE_HASH_PATH: &str = "/capsule_hash";
const PROGRAM_HASH_PATH: &str = "/capsule/program_revision_hash";
const RESULT_HASH_PATH: &str = "/capsule/expected_result_hash";

/// Version of the replay capsule wire format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum EvaluationReplayCapsuleSchema {
    #[serde(rename = "karma.evaluation-replay-capsule.v1")]
    V1,
}

/// Evaluator revision recorded when the result was captured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum EvaluatorRevision {
    #[serde(rename = "karma.evaluator.v1")]
    V1,
}

/// The frozen inputs, expected output, and their identities for one evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationReplayCapsule {
    pub schema: EvaluationReplayCapsuleSchema,
    pub evaluator: EvaluatorRevision,
    pub program: ProgramAst,
    pub program_revision_hash: CanonicalHash,
    pub context: FrozenEvaluationContext,
    pub limits: EvaluationLimits,
    pub expected_result: EvaluationResult,
    pub expected_result_hash: CanonicalHash,
}

/// A replay capsule with a digest of its complete canonical content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedEvaluationReplayCapsule {
    pub capsule: EvaluationReplayCapsule,
    pub capsule_hash: CanonicalHash,
}

/// The stage at which capture or replay stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReplayErrorCode {
    EvaluationFailed,
    CanonicalizationFailed,
    CapsuleHashMismatch,
    ProgramRevisionMismatch,
    ExpectedResultHashMismatch,
    ResultMismatch,
}

/// A capture or replay failure with its location and any underlying evaluation error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayError {
    pub code: ReplayErrorCode,
    pub path: FailurePath,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evaluation: Option<EvaluationError>,
}

impl fmt::Display for ReplayError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:?} at {}: {}",
            self.code, self.path, self.message
        )
    }
}

impl Error for ReplayError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.evaluation.as_ref().map(|error| error as _)
    }
}

/// Evaluates a frozen occurrence and seals the inputs and result for replay.
///
/// # Errors
///
/// Returns the evaluation failure or a canonicalization error while sealing the record.
pub fn capture_evaluation_replay(
    program: &ProgramAst,
    context: &FrozenEvaluationContext,
    limits: EvaluationLimits,
) -> Result<SealedEvaluationReplayCapsule, ReplayError> {
    let expected_result = evaluate_program(program, context, limits)
        .map_err(|error| evaluation_error("capture evaluation failed", error))?;
    let program_revision_hash = expected_result.revision_hash.clone();
    let expected_result_hash = hash(
        EVALUATION_RESULT_HASH_DOMAIN,
        &expected_result,
        RESULT_HASH_PATH,
    )?;
    EvaluationReplayCapsule {
        schema: EvaluationReplayCapsuleSchema::V1,
        evaluator: EvaluatorRevision::V1,
        program: program.clone(),
        program_revision_hash,
        context: context.clone(),
        limits,
        expected_result,
        expected_result_hash,
    }
    .seal()
}

impl EvaluationReplayCapsule {
    /// Seals the capsule's current content with its canonical digest.
    ///
    /// # Errors
    ///
    /// Returns a canonicalization error for content that cannot be hashed.
    pub fn seal(self) -> Result<SealedEvaluationReplayCapsule, ReplayError> {
        let capsule_hash = self.seal_hash()?;
        Ok(SealedEvaluationReplayCapsule {
            capsule: self,
            capsule_hash,
        })
    }

    pub(crate) fn seal_hash(&self) -> Result<CanonicalHash, ReplayError> {
        hash(
            EVALUATION_REPLAY_CAPSULE_HASH_DOMAIN,
            self,
            CAPSULE_HASH_PATH,
        )
    }
}

impl SealedEvaluationReplayCapsule {
    /// Verifies the stored digests, then evaluates and compares the recorded occurrence.
    ///
    /// # Errors
    ///
    /// Returns the first failed integrity check or the typed replay evaluation failure.
    pub fn verify_and_replay(&self) -> Result<EvaluationResult, ReplayError> {
        let capsule = &self.capsule;
        if capsule.seal_hash()? != self.capsule_hash {
            return Err(error(
                ReplayErrorCode::CapsuleHashMismatch,
                CAPSULE_HASH_PATH,
                "sealed capsule hash does not match its canonical content",
            ));
        }

        let program_revision_hash = hash(
            PROGRAM_REVISION_HASH_DOMAIN,
            &capsule.program,
            PROGRAM_HASH_PATH,
        )?;
        if program_revision_hash != capsule.program_revision_hash
            || capsule.expected_result.revision_hash != program_revision_hash
        {
            return Err(error(
                ReplayErrorCode::ProgramRevisionMismatch,
                PROGRAM_HASH_PATH,
                "embedded Program, stored revision, and expected result revision must agree",
            ));
        }

        let expected_result_hash = hash(
            EVALUATION_RESULT_HASH_DOMAIN,
            &capsule.expected_result,
            RESULT_HASH_PATH,
        )?;
        if expected_result_hash != capsule.expected_result_hash {
            return Err(error(
                ReplayErrorCode::ExpectedResultHashMismatch,
                RESULT_HASH_PATH,
                "stored expected result hash does not match the expected result",
            ));
        }

        let actual = evaluate_program(&capsule.program, &capsule.context, capsule.limits)
            .map_err(|error| evaluation_error("replay evaluation failed", error))?;
        if actual != capsule.expected_result {
            return Err(error(
                ReplayErrorCode::ResultMismatch,
                "/capsule/expected_result",
                "replayed result differs from the captured expected result",
            ));
        }
        let actual_hash = hash(EVALUATION_RESULT_HASH_DOMAIN, &actual, RESULT_HASH_PATH)?;
        if actual_hash != capsule.expected_result_hash {
            return Err(error(
                ReplayErrorCode::ResultMismatch,
                RESULT_HASH_PATH,
                "replayed result hash differs from the captured result hash",
            ));
        }
        Ok(actual)
    }
}

fn hash<T: Serialize>(domain: &str, value: &T, path: &str) -> Result<CanonicalHash, ReplayError> {
    canonical_hash(domain, value).map_err(|boundary| {
        error(
            ReplayErrorCode::CanonicalizationFailed,
            path,
            boundary.to_string(),
        )
    })
}

fn evaluation_error(message: &str, evaluation: EvaluationError) -> ReplayError {
    ReplayError {
        code: ReplayErrorCode::EvaluationFailed,
        path: evaluation.path.clone(),
        message: message.to_string(),
        evaluation: Some(evaluation),
    }
}

fn error(code: ReplayErrorCode, path: &str, message: impl Into<String>) -> ReplayError {
    ReplayError {
        code,
        path: FailurePath::new(path).expect("fixed replay paths are valid JSON pointers"),
        message: message.into(),
        evaluation: None,
    }
}
