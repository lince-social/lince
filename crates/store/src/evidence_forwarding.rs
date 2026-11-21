//! The Store queue for reviewed public-evidence forwarding.

mod jobs;
mod queue;
mod transitions;

pub use jobs::{Job, get_on, page_on, pending_on};
pub use queue::{Queue, cleanup_on, queue_on};
pub use transitions::{cancel_on, claim_on, hold_on, interrupted_on, progress_on, result_on};

use crate::StoreError;

fn invalid(error: impl ToString) -> StoreError {
    StoreError::Protocol(error.to_string())
}
