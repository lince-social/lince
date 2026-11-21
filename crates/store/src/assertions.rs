//! Stored Assertion reads and mutations.

mod binary;
mod graph;
mod legacy;
mod model;
mod mutation;
mod read;
mod related;
mod validation;

pub use binary::{
    BinaryAssertionRow, all_binary, binary_for_subjects, binary_of_predicate, binary_of_predicates,
    used_binary_predicates,
};
pub use graph::{edges_of_predicate, edges_of_predicates, record_descendants};
pub use legacy::{
    ImportedAssertion, assert, import_active, refine, refine_tx, retract,
    retract_predicate_within_set, retract_tuple, set_identity, set_identity_tx, transition_unary,
};
pub use model::{AssertionQuantity, AssertionRole, AssertionRow, NewAssertion};
pub use mutation::{insert_tx, insert_uri_tx, promote_identity_tx, retract_tx, set_quantity_tx};
pub use read::{
    ProjectedAssertion, active_of_predicates, concepts_for_record, for_subjects, get,
    identity_concept, incident_to, list_active,
};
pub use related::{
    active_object_kinds, active_object_uids, active_object_uids_from_subjects, messages_before,
    object_exists, object_uids_from_subject, object_uids_from_subjects, objects_from_subject,
    objects_from_subjects, recent_messages, subject_exists, subject_heads, subjects_pointing_to,
    subjects_pointing_to_many,
};

fn mutation_error(message: &str) -> crate::StoreError {
    crate::StoreError::Protocol(message.into())
}
