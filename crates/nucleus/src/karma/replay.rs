use std::fmt;

use serde::{Deserialize, Serialize};

use super::{
    CanonicalHash, EvaluationError, EvaluationLimits, EvaluationResult, FailurePath,
    FrozenEvaluationContext, ProgramAst, canonical_hash, evaluate_program,
};

pub const PROGRAM_REVISION_HASH_DOMAIN: &str = "karma.program-revision.v1";
pub const EVALUATION_RESULT_HASH_DOMAIN: &str = "karma.evaluation-result.v1";
pub const EVALUATION_REPLAY_CAPSULE_HASH_DOMAIN: &str = "karma.evaluation-replay-capsule.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum EvaluationReplayCapsuleSchema {
    #[serde(rename = "karma.evaluation-replay-capsule.v1")]
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum EvaluatorRevision {
    #[serde(rename = "karma.evaluator.v1")]
    V1,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedEvaluationReplayCapsule {
    pub capsule: EvaluationReplayCapsule,
    pub capsule_hash: CanonicalHash,
}

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

impl std::error::Error for ReplayError {}

pub fn capture_evaluation_replay(
    program: &ProgramAst,
    context: &FrozenEvaluationContext,
    limits: EvaluationLimits,
) -> Result<SealedEvaluationReplayCapsule, ReplayError> {
    let expected_result = evaluate_program(program, context, limits)
        .map_err(|error| evaluation_error("capture evaluation failed", error))?;
    let program_revision_hash = hash(
        PROGRAM_REVISION_HASH_DOMAIN,
        program,
        "/capsule/program_revision_hash",
    )?;
    if expected_result.revision_hash != program_revision_hash {
        return Err(error(
            ReplayErrorCode::ProgramRevisionMismatch,
            "/capsule/program_revision_hash",
            "evaluator result and captured Program revision disagree",
        ));
    }
    let expected_result_hash = hash(
        EVALUATION_RESULT_HASH_DOMAIN,
        &expected_result,
        "/capsule/expected_result_hash",
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
    pub fn seal(self) -> Result<SealedEvaluationReplayCapsule, ReplayError> {
        let capsule_hash = hash(
            EVALUATION_REPLAY_CAPSULE_HASH_DOMAIN,
            &self,
            "/capsule_hash",
        )?;
        Ok(SealedEvaluationReplayCapsule {
            capsule: self,
            capsule_hash,
        })
    }
}

impl SealedEvaluationReplayCapsule {
    pub fn verify_and_replay(&self) -> Result<EvaluationResult, ReplayError> {
        let capsule_hash = hash(
            EVALUATION_REPLAY_CAPSULE_HASH_DOMAIN,
            &self.capsule,
            "/capsule_hash",
        )?;
        if capsule_hash != self.capsule_hash {
            return Err(error(
                ReplayErrorCode::CapsuleHashMismatch,
                "/capsule_hash",
                "sealed capsule hash does not match its canonical content",
            ));
        }

        let program_revision_hash = hash(
            PROGRAM_REVISION_HASH_DOMAIN,
            &self.capsule.program,
            "/capsule/program_revision_hash",
        )?;
        if program_revision_hash != self.capsule.program_revision_hash
            || self.capsule.expected_result.revision_hash != program_revision_hash
        {
            return Err(error(
                ReplayErrorCode::ProgramRevisionMismatch,
                "/capsule/program_revision_hash",
                "embedded Program, stored revision, and expected result revision must agree",
            ));
        }

        let expected_result_hash = hash(
            EVALUATION_RESULT_HASH_DOMAIN,
            &self.capsule.expected_result,
            "/capsule/expected_result_hash",
        )?;
        if expected_result_hash != self.capsule.expected_result_hash {
            return Err(error(
                ReplayErrorCode::ExpectedResultHashMismatch,
                "/capsule/expected_result_hash",
                "stored expected result hash does not match the expected result",
            ));
        }

        let actual = evaluate_program(
            &self.capsule.program,
            &self.capsule.context,
            self.capsule.limits,
        )
        .map_err(|error| evaluation_error("replay evaluation failed", error))?;
        if actual != self.capsule.expected_result {
            return Err(error(
                ReplayErrorCode::ResultMismatch,
                "/capsule/expected_result",
                "replayed result differs from the captured expected result",
            ));
        }
        let actual_hash = hash(
            EVALUATION_RESULT_HASH_DOMAIN,
            &actual,
            "/capsule/expected_result_hash",
        )?;
        if actual_hash != self.capsule.expected_result_hash {
            return Err(error(
                ReplayErrorCode::ResultMismatch,
                "/capsule/expected_result_hash",
                "replayed result hash differs from the captured result hash",
            ));
        }
        Ok(actual)
    }
}

fn hash<T: Serialize>(domain: &str, value: &T, path: &str) -> Result<CanonicalHash, ReplayError> {
    canonical_hash(domain, value).map_err(|boundary| ReplayError {
        code: ReplayErrorCode::CanonicalizationFailed,
        path: FailurePath::new(path).expect("replay paths are valid JSON pointers"),
        message: boundary.to_string(),
        evaluation: None,
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

fn error(code: ReplayErrorCode, path: &str, message: &str) -> ReplayError {
    ReplayError {
        code,
        path: FailurePath::new(path).expect("replay paths are valid JSON pointers"),
        message: message.to_string(),
        evaluation: None,
    }
}
