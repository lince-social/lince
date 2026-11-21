//! Core Lince types and operations shared by execution, storage, and interface crates.
//!
//! The crate supplies wire models, local validation, and pure computations.
//! Runtime adapters use those contracts to store data and perform effects.

/// Signed action intents and their session proofs.
pub mod action_intent;
/// Blob manifests and transfer destinations.
pub mod blob_sync;
/// Canvas models and scene authoring operations.
pub mod canvas;
/// Shell and process command descriptions and responses.
pub mod command;
/// Presented components and their compositions.
pub mod component;
/// Production and meal planning models.
pub mod demeter;
/// Description asset references and requests.
pub mod description_asset;
/// Drawing data and strokes.
pub mod drawing;
/// Errors shared by Nucleus operations.
pub mod error;
/// Execution context, control, and causal bookkeeping.
pub mod execution;
/// Expressions, values, and token resolution.
pub mod expr;
/// Facts, their causes, and exact quantities.
pub mod fact;
/// Dependency graphs and quantity propagation.
pub mod graph;
/// Hybrid logical clock encoding and advancement.
pub mod hlc;
pub mod id;
/// Person keys and identity commands.
pub mod identity;
/// Projected state and virtual timelines.
pub mod imagination;
/// JSON encoding checks and hashes.
pub mod json;
pub mod karma;
/// Sources, claims, and meanings.
pub mod knowledge;
/// Location settings, fixes, and invitations.
pub mod location;
/// Message parts and their stored representation.
pub mod message;
/// Nearby peer descriptors.
pub mod nearby;
/// Operation steps, usage, and cost.
pub mod operation;
pub mod optimization;
/// Physical workspace geometry, devices, and replay.
pub mod physical_workspace;
/// Geographic coordinates, distance, and map topology.
pub mod place;
pub mod plugins;
pub mod projection;
/// Promise states.
pub mod promise;
/// Rules for changes to Record properties.
pub mod property_rule;
/// Question states, schemas, and answer validation.
pub mod question;
/// Record kinds and message states.
pub mod record;
/// Typed extension fields and schemas.
pub mod record_extension;
/// Sand package metadata and contents.
pub mod sand_package;
pub mod schedule;
/// Script validation, evaluation, and host operations.
pub mod script;
pub mod shader_effect;
pub mod simulation;
pub mod social;
pub mod sync;
/// Terrain models, spatial bindings, and effects.
pub mod terrain;
pub mod transfer;
/// Transfer envelopes and delivery policy.
pub mod transfer_delivery;
pub mod trust_analysis;
pub mod visibility;
/// Vocabulary terms, references, and commands.
pub mod vocabulary;
/// Rules and models for opening workspace windows.
pub mod window_opening;

pub use error::NucleusError;
pub use expr::{Expr, MapResolver, Resolver, TokenKey, Value, parse_duration};
pub use fact::{Cause, CauseKind, Fact, NewFact};
pub use id::{new_uid, ulid_from, valid_slug, valid_uid};
pub use karma::DecimalValue;
/// Timing spans and counters for core operations.
pub use lince_instrumentation as performance;
pub use promise::PromiseState;
pub use record::{MessageDraftTiming, MessageState, RecordKind};
