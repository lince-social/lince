//! Lince pure core.
//!
//! Blueprint: `docs/fable-improvement.md`. This crate cannot be called `core`
//! (Rust built-in), so it is `nucleus` — the part of the Cell that holds the
//! machinery of meaning.
//!
//! Invariants owned here:
//! - No IO, no SQL, no async: everything is a pure function over passed-in data,
//!   so the DST harness can replay a fact log with a virtual clock.
//! - Booleans are numbers (`true = 1.0`) in the expression engine.
//! - The rule pipeline is `condition -> gate -> carry -> consequences`.

pub mod error;
pub mod expr;
pub mod fact;
pub mod frequency;
pub mod graph;
pub mod id;
pub mod imagination;
pub mod place;
pub mod promise;
pub mod record;
pub mod rule;
pub mod transfer;

pub use error::NucleusError;
pub use expr::{Expr, MapResolver, Resolver, TokenKey, Value};
pub use fact::{Cause, CauseKind, Fact, NewFact};
pub use frequency::{parse_duration, FrequencySpec};
pub use id::{new_uid, ulid_from, valid_slug};
pub use promise::PromiseState;
pub use record::RecordKind;
pub use rule::{Carry, ConsequenceKind, ConsequenceSpec, Firing, Gate, RuleDef};
