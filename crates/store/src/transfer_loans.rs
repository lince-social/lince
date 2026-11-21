//! Transfer loan agreements, linked terms, and projected Record availability.

use std::fmt::Display;

use crate::StoreError;

mod agreement;
mod availability;
mod effects;
mod extension;
mod terms;

pub(crate) use agreement::accept_ready_on;
pub use agreement::{Accepted, accepted, accepted_on};
pub use availability::{Adjustment, adjustments, adjustments_on};
pub use extension::{Extension, Outcome, extend};
pub use terms::{Terms, status, terms, validate_link};

fn invalid(value: impl Display) -> StoreError {
    StoreError::Protocol(value.to_string())
}

fn positive(value: nucleus::DecimalValue) -> nucleus::DecimalValue {
    if value.is_negative() {
        crate::exact::zero()
    } else {
        value
    }
}

fn minimum(a: nucleus::DecimalValue, b: nucleus::DecimalValue) -> nucleus::DecimalValue {
    if a.exact_numeric_cmp(b).is_gt() { b } else { a }
}
