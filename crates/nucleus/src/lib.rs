// non-lean-yet
// Scope: mod action_intent, mod blob_sync, mod canvas, mod component.
// BeautifulRust (baseline lines):
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 1, 2,
//   3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27,
//   28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 56, 57, 59, 61.

pub mod action_intent;
pub mod blob_sync;
pub mod canvas;
pub mod component;
pub mod demeter;
pub mod description_asset;
pub mod drawing;
pub mod error;
pub mod execution;
pub mod expr;
pub mod fact;
pub mod graph;
pub mod hlc;
pub mod id;
pub mod identity;
pub mod imagination;
pub mod json;
pub mod karma;
pub mod knowledge;
pub mod location;
pub mod message;
pub mod nearby;
pub mod performance;
pub mod physical_workspace;
pub mod place;
pub mod plugins;
pub mod projection;
pub mod promise;
pub mod property_rule;
pub mod record;
pub mod record_extension;
pub mod sand_package;
pub mod schedule;
pub mod script;
pub mod shader_effect;
pub mod simulation;
pub mod social;
pub mod sync;
pub mod terrain;
pub mod transfer;
pub mod transfer_delivery;
pub mod trust_analysis;
pub mod visibility;
pub mod vocabulary;
pub mod window_opening;

pub use error::NucleusError;
pub use expr::{Expr, MapResolver, Resolver, TokenKey, Value, parse_duration};
pub use fact::{Cause, CauseKind, Fact, NewFact};
pub use id::{new_uid, ulid_from, valid_slug, valid_uid};
pub use karma::DecimalValue;
pub use promise::PromiseState;
pub use record::{MessageDraftTiming, MessageState, RecordKind};

pub mod operation;
pub mod optimization;

pub mod question;

pub mod command;
