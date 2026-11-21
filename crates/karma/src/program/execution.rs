//! Durable records for Karma Program execution.

use crate::{KarmaBoundaryError, ReferenceKind, TypedUid};

mod candidate;
mod epoch;
mod run;
mod state;

pub use candidate::{
    CandidateReviewAction, CandidateReviewEvidence, CandidateReviewEvidenceSchema,
    KARMA_CANDIDATE_PROPOSAL_HASH_DOMAIN, KarmaCandidateProposal, KarmaCandidateProposalSchema,
};
pub use epoch::{
    OCCURRENCE_PROGRAM_EPOCH_HASH_DOMAIN, OccurrenceProgramEpoch, OccurrenceProgramEpochSchema,
    ProgramEpochMember,
};
pub use run::{
    KARMA_RUN_HASH_DOMAIN, KarmaRun, KarmaRunOutcome, KarmaRunSchema, ProgramNotApplicableReason,
    ProgramRunBlockCode,
};
pub use state::{
    PROGRAM_STATE_EVENT_HASH_DOMAIN, PersistedProgramNodeState, ProgramStateEvent,
    ProgramStateEventSchema, ProgramStateResetReason,
};

fn validate_program_uid(uid: &str) -> Result<(), KarmaBoundaryError> {
    TypedUid::validate(ReferenceKind::Program, uid)
}
