//! Bounded broadcasts for ephemeral room events.

use std::{
    collections::HashMap,
    fmt,
    sync::{Mutex, MutexGuard},
};

use serde_json::Value;
use tokio::sync::broadcast;

const ROOM_CAPACITY: usize = 256;

/// An event to broadcast inside one room.
#[derive(Debug, Clone)]
pub struct LaneEvent {
    /// Room that receives the event.
    pub room: String,
    /// Connection that sent the event.
    pub from: String,
    /// Application data carried by the event.
    pub payload: Value,
    /// Authenticated sender identity, when available.
    pub from_subject: Option<String>,
    /// Organ scope, when supplied by the session.
    pub organ: Option<String>,
}

/// A registry of independent room broadcasts.
#[derive(Default)]
pub struct LaneHub {
    rooms: Mutex<HashMap<String, broadcast::Sender<LaneEvent>>>,
}

impl fmt::Debug for LaneHub {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LaneHub").finish_non_exhaustive()
    }
}

impl LaneHub {
    /// Creates an empty room registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribes to new events, creating the room on first use.
    ///
    /// # Panics
    /// Panics if the room registry mutex was poisoned.
    pub fn join(&self, room: &str) -> broadcast::Receiver<LaneEvent> {
        let mut rooms = self.rooms();
        if let Some(sender) = rooms.get(room) {
            return sender.subscribe();
        }
        let (sender, receiver) = broadcast::channel(ROOM_CAPACITY);
        rooms.insert(room.into(), sender);
        receiver
    }

    /// Broadcasts an event and returns the number of subscribed receivers.
    ///
    /// # Panics
    /// Panics if the room registry mutex was poisoned.
    pub fn send(&self, event: LaneEvent) -> usize {
        self.rooms()
            .get(&event.room)
            .map_or(0, |sender| sender.send(event).unwrap_or(0))
    }

    /// Removes a room once its last subscriber has left.
    ///
    /// # Panics
    /// Panics if the room registry mutex was poisoned.
    pub fn prune(&self, room: &str) {
        let mut rooms = self.rooms();
        if rooms
            .get(room)
            .is_some_and(|sender| sender.receiver_count() == 0)
        {
            rooms.remove(room);
        }
    }

    fn rooms(&self) -> MutexGuard<'_, HashMap<String, broadcast::Sender<LaneEvent>>> {
        self.rooms
            .lock()
            .expect("the Lane room registry must be intact")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::broadcast::error::TryRecvError;

    fn event(room: &str, payload: usize) -> LaneEvent {
        LaneEvent {
            room: room.into(),
            from: "connection".into(),
            payload: payload.into(),
            from_subject: None,
            organ: None,
        }
    }

    #[test]
    fn subscriptions_start_at_join_and_prune_keeps_active_rooms() {
        let hub = LaneHub::new();
        assert_eq!(hub.send(event("absent", 0)), 0);
        let mut first = hub.join("one");
        let mut other = hub.join("two");
        assert_eq!(hub.send(event("one", 1)), 1);
        let mut second = hub.join("one");
        assert!(matches!(second.try_recv(), Err(TryRecvError::Empty)));
        hub.prune("one");
        assert_eq!(hub.send(event("one", 2)), 2);
        assert_eq!(first.try_recv().expect("first message").payload, 1);
        assert_eq!(first.try_recv().expect("second message").payload, 2);
        assert_eq!(second.try_recv().expect("new message").payload, 2);
        assert!(matches!(other.try_recv(), Err(TryRecvError::Empty)));
        drop(first);
        drop(second);
        hub.prune("one");
        assert!(!hub.rooms().contains_key("one"));
        assert!(matches!(
            hub.join("one").try_recv(),
            Err(TryRecvError::Empty)
        ));
    }

    #[test]
    fn lag_reports_the_exact_room_capacity() {
        let hub = LaneHub::new();
        let mut receiver = hub.join("one");
        for sequence in 0..=ROOM_CAPACITY {
            assert_eq!(hub.send(event("one", sequence)), 1);
        }
        assert!(matches!(receiver.try_recv(), Err(TryRecvError::Lagged(1))));
        assert_eq!(
            receiver.try_recv().expect("first retained event").payload,
            1
        );
    }
}
