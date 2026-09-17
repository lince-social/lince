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
    presence: Mutex<HashMap<(String, String), (std::time::Instant, crate::protocol::CollabCursor)>>,
    changed: tokio::sync::watch::Sender<u64>,
}

impl LaneHub {
    pub fn presence_changes(&self) -> tokio::sync::watch::Receiver<u64> {
        self.changed.subscribe()
    }

    pub fn cursor(&self, uid: &str, cursor: crate::protocol::CollabCursor) {
        let now = std::time::Instant::now();
        let mut presence = self.presence.lock().expect("presence");
        presence.retain(|_, (at, _)| at.elapsed().as_secs() < 20);
        let key = (uid.to_owned(), cursor.session.clone());
        if presence
            .get(&key)
            .is_some_and(|(at, _)| at.elapsed().as_millis() < 100)
            || (presence.len() >= 8192 && !presence.contains_key(&key))
        {
            return;
        }
        presence.insert(key, (now, cursor));
        drop(presence);
        self.changed
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }

    pub fn cursors(&self, uid: &str) -> Vec<crate::protocol::CollabCursor> {
        let mut presence = self.presence.lock().expect("presence");
        presence.retain(|_, (at, _)| at.elapsed().as_secs() < 20);
        let mut cursors: Vec<_> = presence
            .iter()
            .filter(|((record, _), _)| record == uid)
            .map(|(_, (_, cursor))| cursor.clone())
            .take(256)
            .collect();
        cursors.sort_by(|left, right| left.session.cmp(&right.session));
        cursors
    }

    pub fn leave_cursor(&self, uid: Option<&str>, session: &str) {
        self.presence
            .lock()
            .expect("presence")
            .retain(|(record, editor), _| {
                editor != session || uid.is_some_and(|uid| uid != record)
            });
        self.changed
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }

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
