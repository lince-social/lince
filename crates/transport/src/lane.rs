//! Ephemeral presence lanes (blueprint VII.3): cursors, typing, call signaling.
//! Scoped to a room, fanned out through the hub, **never written to the
//! Ledger**. When the last member leaves, the room evaporates.

use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub struct LaneEvent {
    pub room: String,
    pub from: String,
    pub payload: Value,
    /// The sender's Person/app_user subject, when they have one.
    ///
    /// Carried SEPARATELY from `from` (a connection id) because presence has
    /// two halves with different privacy: WHERE a cursor is, which everyone in
    /// the room may see, and WHO it belongs to, which only a viewer with read
    /// permission on that user may see. The receiving side resolves this to a
    /// name or drops it — see `ws::spawn_lane_forwarder`.
    pub from_subject: Option<String>,
}

/// Shared across all sessions on a host. Cheap to clone the handle via `Arc`.
#[derive(Default)]
pub struct LaneHub {
    rooms: Mutex<HashMap<String, broadcast::Sender<LaneEvent>>>,
}

impl LaneHub {
    pub fn new() -> LaneHub {
        LaneHub::default()
    }

    /// Join (or create) a room; returns a receiver for its events.
    pub fn join(&self, room: &str) -> broadcast::Receiver<LaneEvent> {
        let mut rooms = self.rooms.lock().unwrap();
        rooms
            .entry(room.to_string())
            .or_insert_with(|| broadcast::channel(256).0)
            .subscribe()
    }

    /// Publish to a room. Returns how many receivers saw it (0 = empty room).
    pub fn send(&self, event: LaneEvent) -> usize {
        let rooms = self.rooms.lock().unwrap();
        rooms
            .get(&event.room)
            .map(|tx| tx.send(event).unwrap_or(0))
            .unwrap_or(0)
    }

    /// Drop a room's sender if nobody is listening — keeps the map from growing.
    pub fn prune(&self, room: &str) {
        let mut rooms = self.rooms.lock().unwrap();
        if let Some(tx) = rooms.get(room) {
            if tx.receiver_count() == 0 {
                rooms.remove(room);
            }
        }
    }
}
