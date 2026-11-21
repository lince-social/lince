//! Evaluates Lince Record access over caller-supplied snapshots.

mod budget;
mod graph;
mod mutation;
mod read;
mod selector;
mod types;

pub use mutation::{
    authorize_record_change, authorize_record_changes,
    authorize_record_changes_with_assertion_intents,
};
pub use read::{ReadGraph, may_read, readable_records, selector_membership_changes};
pub use types::{
    AssertionGrant, AssertionIntent, AssertionProperty, AssertionRole, AssertionState,
    AssertionTarget, AuthorityError, ConceptState, ExtensionProperty, GraphSnapshot, Limits,
    MutationGrant, MutationTarget, Operation, Property, RecordContent, RecordDecision, RecordState,
    RolePolicy, VisibilityCeiling,
};
