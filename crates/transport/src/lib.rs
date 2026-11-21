//! Message types and session drivers shared by Lince interfaces.
//!
//! The `mcp` feature exposes the local tool bridge. The `terminal` feature
//! enables interactive terminals, and `axum` exposes the WebSocket driver.

#![recursion_limit = "256"]

pub mod command;
mod connection_task;
pub mod fiote;
pub mod lane;
/// Authenticated live sessions on Iroh streams.
pub mod live;
pub mod live_client;
/// Local agent tools exposed through the Model Context Protocol.
#[cfg(feature = "mcp")]
pub mod mcp;
/// Native local tools backed by a Transport session.
pub mod native;
pub mod protocol;
/// Session identity, request dispatch, and active subscriptions.
pub mod session;
pub mod speech;
pub mod sync_events;
#[cfg(feature = "terminal")]
pub mod terminal;
#[cfg(feature = "axum")]
pub mod ws;

pub use lane::{LaneEvent, LaneHub};
pub use protocol::{ClientMessage, ServerMessage};
pub use session::Session;
pub use sync_events::{SyncEvent, SyncEvents};
