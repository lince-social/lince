//! Private Transfer policies, effective formulas, quantities, and corrections.

use std::fmt::Display;

use crate::StoreError;

mod binding;
mod correction;
mod policy;
mod quantity;

pub use binding::{Binding, Effective, effective, exchange_for_promise, handoff_exchange};
pub(crate) use binding::{effective_on, effective_with_policy_on, require_same_record};
pub use correction::compensate;
pub use policy::{Policy, PolicyInput, bound_record, policy, policy_on, set_policy};
pub use quantity::{Applied, allocated, amount, applied, calculate};
pub(crate) use quantity::{allocated_on, applied_on};

fn invalid(error: impl Display) -> StoreError {
    StoreError::Protocol(error.to_string())
}
