//! Lince pure core.
//!
//! Blueprint: `docs/new-version-capabilities-and-maneirisms.md`. This crate cannot be called `core`
//! (Rust built-in), so it is `nucleus` — the part of the Cell that holds the
//! machinery of meaning.
//!
//! Invariants owned here:
//! - No IO, no SQL, no async: everything is a pure function over passed-in data,
//!   so the DST harness can replay a fact log with a virtual clock.
//! - Booleans are numbers (`true = 1.0`) in the expression engine.
//! - The rule pipeline is `condition -> gate -> carry -> consequences`.

pub mod action_intent;
pub mod error;
pub mod expr;
pub mod fact;
pub mod graph;
pub mod id;
pub mod imagination;
pub mod karma;
pub mod place;
pub mod promise;
pub mod record;
pub mod transfer;
pub mod transfer_delivery;

pub use error::NucleusError;
pub use expr::{Expr, MapResolver, Resolver, TokenKey, Value};
pub use fact::{Cause, CauseKind, Fact, NewFact};
pub use id::{new_uid, ulid_from, valid_slug};
// The Ledger's exact quantity type is the kernel's exact quantity type — one
// representation from Karma evaluation through to the Fact chain (E0.0).
pub use expr::parse_duration;
pub use karma::DecimalValue;
pub use promise::PromiseState;
pub use record::RecordKind;
