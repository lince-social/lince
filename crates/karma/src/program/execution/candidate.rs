//! Inert candidate proposals and their review records.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::validate_program_uid;
use crate::{
    CandidateRoute, CandidateStatus, CanonicalHash, KarmaBoundaryError, LiteralValue, LocalId,
    Slug, TimestampMs, canonical_hash,
};

pub const KARMA_CANDIDATE_PROPOSAL_HASH_DOMAIN: &str = "karma.candidate-proposal.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum KarmaCandidateProposalSchema {
    #[serde(rename = "karma.candidate-proposal.v1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
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
    #[expect(
        clippy::too_many_arguments,
        reason = "The public constructor takes the fields of the execution record"
    )]
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
        validate_program_uid(&self.program_uid)?;
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
