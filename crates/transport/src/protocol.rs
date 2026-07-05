//! The wire protocol (blueprint VII.3): JSON messages both ways. Sands speak
//! only this — Protein for reads, Actions for writes, lanes for presence.

use engine::actions::Action;
use protein::Protein;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Client -> server. `id` correlates responses to requests/subscriptions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Open a live subscription: an immediate snapshot, then updates whenever a
    /// committed fact may have changed the result.
    Subscribe { id: String, protein: Protein },
    /// Subscribe to a saved Protein by slug/uid (blueprint VII.1).
    SubscribeSaved { id: String, name: String },
    Unsubscribe { id: String },
    /// A typed write. Protein never mutates; every mutation is an Action.
    Act { id: String, action: Action },
    /// Join an ephemeral room (presence/cursors/call signaling).
    LaneJoin { room: String },
    LaneLeave { room: String },
    /// Fan-out to everyone in the room. Never persisted (blueprint VII.3).
    LaneSend { room: String, payload: Value },
}

/// Server -> client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// The full current result of a subscription.
    Snapshot { id: String, rows: Vec<Value> },
    /// A recomputed result pushed after a relevant commit (v1: full re-send).
    Update { id: String, rows: Vec<Value> },
    /// Result of an Action: the created uid (if any) and committed fact count.
    ActionOk { id: String, created: Option<String>, facts: usize },
    /// An Action or subscription failed.
    Error { id: String, message: String },
    /// A message from another session in a joined room.
    LaneEvent { room: String, from: String, payload: Value },
}
