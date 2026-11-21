//! Store-backed forecast inputs, cache reads, and publication.

mod inputs;
mod publication;
mod read;
mod triggers;

pub use inputs::{ManualInputs, manual_inputs, revision, set_runtime};
pub use publication::{publish, publish_schedule};
pub use read::{
    Cached, cached_until, covered_window, previous_schedule, read, read_calendar, read_schedule,
};
pub use triggers::install;

use crate::StoreError;

fn protocol(error: impl ToString) -> StoreError {
    StoreError::Protocol(error.to_string())
}
