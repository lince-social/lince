//! Transactional private effects for Transfer settlements.

use nucleus::{DecimalValue, transfer::application::EffectMode};
use serde::Serialize;

use crate::StoreError;

mod evidence;
mod quote;
mod writes;

pub(crate) use evidence::reviewed_hash_on;
pub use evidence::{correction_facts, facts, protected, recorded, records, reviewed_hash};
pub(crate) use quote::{assumptions_on, quote_effective_on, quote_prepared_on};
pub use quote::{quote, quote_on, require_review};
pub(crate) use writes::{apply_on, compensate_on};

fn invalid(value: impl ToString) -> StoreError {
    StoreError::Protocol(value.to_string())
}

pub(crate) fn mode(value: EffectMode) -> &'static str {
    match value {
        EffectMode::Quantity => "quantity",
        EffectMode::Fulfilment => "fulfilment",
    }
}

/// One reviewed private Record change and its cumulative quantities.
#[derive(Clone, Debug, Serialize)]
pub struct Effect {
    pub record: String,
    pub title: String,
    pub mode: EffectMode,
    pub unit: Option<String>,
    pub formula: String,
    pub before: DecimalValue,
    pub available_before: DecimalValue,
    pub delta: DecimalValue,
    pub after: DecimalValue,
    pub cumulative_before: DecimalValue,
    pub cumulative_after: DecimalValue,
}

/// An ordered set of proposed changes and the hash of its review.
#[derive(Clone, Debug, Serialize)]
pub struct Group {
    pub transfer: String,
    pub exchange: String,
    pub effects: Vec<Effect>,
    pub hash: String,
}
