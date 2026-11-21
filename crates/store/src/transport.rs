//! Journey evidence and per-segment Transport reservations.

use std::fmt::Display;

use crate::StoreError;

mod capacity;
mod journey;

pub use capacity::{capacity, validate_all_on};
pub use journey::{journey, journey_on};

fn invalid(message: impl Display) -> StoreError {
    StoreError::Protocol(message.to_string())
}
