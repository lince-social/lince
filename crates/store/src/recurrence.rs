//! Recurring rules, revision history, and their derived occurrences.

mod commands;
mod condition;
mod occurrences;
mod rules;

use crate::StoreError;

pub use commands::{
    NewRecurrence, RecurrenceCommit, ReviseRecurrence, create, create_identified, delete, revise,
    set_state,
};
pub use condition::{RuleCondition, RuleConditionInput};
pub use occurrences::{
    Occurrence, OccurrenceState, Occurrences, applied, occurrence_request_id, occurrences, skip,
    unskip,
};
pub use rules::{Recurrence, all, for_record, get, replayed};

/// Stored state of an active recurring rule.
pub const STATE_ACTIVE: &str = "active";
/// Stored state of a paused recurring rule.
pub const STATE_PAUSED: &str = "paused";

fn protocol(message: impl Into<String>) -> StoreError {
    StoreError::Protocol(message.into())
}
