// non-lean-yet
// Scope: struct LaneEvent, struct LaneHub.
// BeautifulRust (baseline lines):
// - [Clippy probe: clippy::collapsible_if] this `if` statement can be collapsed. Sites 43.
// - [Clippy probe: clippy::expect_used] Check the stated invariant and handle reachable failures.
//   Sites 26, 34, 42.
// - [Clippy probe: clippy::map_unwrap_or] called `map(<f>).unwrap_or(<a>)` on an `Option` value.
//   Sites 35.
// - [Clippy probe: clippy::missing_panics_doc] State the panic conditions for exported functions.
//   Sites 25, 33, 41.
// - [rustc probe: missing_debug_implementations] type does not implement `std::fmt::Debug`;
//   consider adding `#[derive(Debug)]` or a manual implementation. Sites 16.
// - [rustc probe: missing_docs] Document the externally visible contracts where applicable. Sites
//   7, 8, 9, 10, 11, 12, 16, 21, 25, 33, 41.

use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub struct LaneEvent {
    pub room: String,
    pub from: String,
    pub payload: Value,
    pub from_subject: Option<String>,
    pub organ: Option<String>,
}

#[derive(Default)]
pub struct LaneHub {
    rooms: Mutex<HashMap<String, broadcast::Sender<LaneEvent>>>,
}

impl LaneHub {
    pub fn new() -> LaneHub {
        LaneHub::default()
    }

    pub fn join(&self, room: &str) -> broadcast::Receiver<LaneEvent> {
        let mut rooms = self.rooms.lock().expect("the Lane room registry must be intact");
        rooms
            .entry(room.to_string())
            .or_insert_with(|| broadcast::channel(256).0)
            .subscribe()
    }

    pub fn send(&self, event: LaneEvent) -> usize {
        let rooms = self.rooms.lock().expect("the Lane room registry must be intact");
        rooms
            .get(&event.room)
            .map(|tx| tx.send(event).unwrap_or(0))
            .unwrap_or(0)
    }

    pub fn prune(&self, room: &str) {
        let mut rooms = self.rooms.lock().expect("the Lane room registry must be intact");
        if let Some(tx) = rooms.get(room) {
            if tx.receiver_count() == 0 {
                rooms.remove(room);
            }
        }
    }
}
