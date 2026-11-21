//! Store-backed social documents, search, and publication queues.

mod authority;
mod budget;
mod device;
mod publication;
mod search;
mod snippets;

use crate::StoreError;
use nucleus::social::Direction;

pub use authority::{anchor_posting_authority_on, anchor_profile_authority_on, put_profile_on};
pub use budget::{BudgetClass, spend, spend_class, spend_class_on, spend_on};
pub use device::{device_state, put_device_state_on};
pub use publication::{
    PublicationJob, due_publications, enqueue_on, fail_publication, fail_publication_on, jobs,
    prune, publication_receipt_on, publication_result, publication_result_on,
};
pub use search::{search, search_public};
pub use snippets::put_snippet_on;

fn protocol(error: impl ToString) -> StoreError {
    StoreError::Protocol(error.to_string())
}

const fn direction_label(direction: Direction) -> &'static str {
    match direction {
        Direction::Need => "need",
        Direction::Contribution => "contribution",
    }
}
