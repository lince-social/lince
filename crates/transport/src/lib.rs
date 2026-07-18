//! Lince first-party channel (blueprint VII.3): the one bidirectional, typed,
//! streaming contract sands and every first-party surface speak.
//!
//! This crate is transport-agnostic on purpose. `Session` is a state machine:
//! feed it a `ClientMessage`, get `ServerMessage`s; feed it a committed `Fact`
//! from the engine's `fact_bus`, get live subscription updates. A real socket
//! (axum websocket, gRPC) is a thin driver over this — see `serve` for the
//! reference websocket-shaped driver contract in the docs. Testing needs no
//! socket at all.
//!
//! Multiplexed over one connection: N Protein subscriptions + Action
//! request/response + ephemeral presence lanes + capability-scoped host
//! streams such as terminal sessions. Ephemeral traffic never touches the
//! Ledger.

pub mod lane;
pub mod protocol;
pub mod session;
#[cfg(feature = "axum")]
mod terminal;
#[cfg(feature = "axum")]
pub mod ws;

pub use lane::{LaneEvent, LaneHub};
pub use protocol::{ClientMessage, ServerMessage};
pub use session::Session;
