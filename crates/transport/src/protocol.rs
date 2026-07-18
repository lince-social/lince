//! The wire protocol (blueprint VII.3): JSON messages both ways. Sands use
//! Protein for reads, Actions for writes, lanes for peer ephemera, and named
//! host-capability frames for streams that do not belong in the Ledger.

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
    Subscribe {
        id: String,
        protein: Protein,
    },
    /// Subscribe to a saved Protein by slug/uid (blueprint VII.1).
    SubscribeSaved {
        id: String,
        name: String,
    },
    Unsubscribe {
        id: String,
    },
    /// A typed write. Protein never mutates; every mutation is an Action.
    Act {
        id: String,
        action: Action,
    },
    /// Join an ephemeral room (presence/cursors/call signaling).
    LaneJoin {
        room: String,
    },
    LaneLeave {
        room: String,
    },
    /// Fan-out to everyone in the room. Never persisted (blueprint VII.3).
    LaneSend {
        room: String,
        payload: Value,
    },
    /// Open an ephemeral PTY owned by this transport connection. Terminal I/O
    /// is an explicit host capability, never Protein data or a Ledger write.
    TerminalOpen {
        id: String,
        cols: u16,
        rows: u16,
        #[serde(default)]
        pixel_width: u16,
        #[serde(default)]
        pixel_height: u16,
    },
    TerminalInput {
        id: String,
        data_base64: String,
    },
    TerminalResize {
        id: String,
        cols: u16,
        rows: u16,
        #[serde(default)]
        pixel_width: u16,
        #[serde(default)]
        pixel_height: u16,
    },
    TerminalClose {
        id: String,
    },
}

/// Server -> client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// The full current result of a subscription.
    Snapshot {
        id: String,
        rows: Vec<Value>,
    },
    /// A recomputed result pushed after a relevant commit (v1: full re-send).
    Update {
        id: String,
        rows: Vec<Value>,
    },
    /// Result of an Action: the created uid (if any), committed fact count,
    /// and non-fatal advisories (link cycles, rule Proof loops). Warnings are
    /// advice, never rejections — clients should show them, not error on them.
    ActionOk {
        id: String,
        created: Option<String>,
        facts: usize,
        #[serde(default)]
        warnings: Vec<String>,
    },
    /// An Action or subscription failed.
    Error {
        id: String,
        message: String,
    },
    /// A message from another session in a joined room.
    LaneEvent {
        room: String,
        from: String,
        payload: Value,
    },
    TerminalOpened {
        id: String,
        shell: String,
        cwd: String,
    },
    TerminalData {
        id: String,
        data_base64: String,
    },
    TerminalExit {
        id: String,
        exit_code: Option<u32>,
    },
}
