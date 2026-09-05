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
        let mut rooms = self.rooms.lock().unwrap();
        rooms
            .entry(room.to_string())
            .or_insert_with(|| broadcast::channel(256).0)
            .subscribe()
    }

    pub fn send(&self, event: LaneEvent) -> usize {
        let rooms = self.rooms.lock().unwrap();
        rooms
            .get(&event.room)
            .map(|tx| tx.send(event).unwrap_or(0))
            .unwrap_or(0)
    }

    pub fn prune(&self, room: &str) {
        let mut rooms = self.rooms.lock().unwrap();
        if let Some(tx) = rooms.get(room) {
            if tx.receiver_count() == 0 {
                rooms.remove(room);
            }
        }
    }
}
