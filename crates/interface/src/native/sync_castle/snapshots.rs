use crate::host::performance::{self, Count};
use crate::native::wake::WakeSignal;
use bevy::prelude::{Component, Entity, World};
use tokio::sync::watch;

pub(super) struct QueryRefresh {
    changes: watch::Receiver<engine::QueryChanges>,
    general: u64,
    timer: tokio::time::Interval,
}

impl QueryRefresh {
    pub(super) fn new(engine: &engine::Engine, interval: std::time::Duration) -> Self {
        let changes = engine.watch_query_changes();
        let general = changes.borrow().general;
        let mut timer = tokio::time::interval(interval);
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        Self {
            changes,
            general,
            timer,
        }
    }

    pub(super) async fn next(&mut self) -> bool {
        loop {
            tokio::select! {
                _ = self.timer.tick() => {
                    self.general = self.changes.borrow_and_update().general;
                }
                changed = self.changes.changed() => {
                    if changed.is_err() { return false; }
                    let general = self.changes.borrow_and_update().general;
                    if general == self.general {
                        performance::count(Count::SyncProjectionNoticesSkipped);
                        continue;
                    }
                    self.general = general;
                    self.timer.reset();
                }
            }
            performance::count(Count::SyncSnapshotReadsAdmitted);
            return true;
        }
    }
}

pub(super) fn needs_watch<T: Component>(world: &mut World, owner: Entity) -> bool {
    if !crate::native::output_views::presents_entity(world, owner) {
        if world.get::<T>(owner).is_some() {
            world.entity_mut(owner).remove::<T>();
            performance::count(Count::SyncPresentationWatchesRetired);
        }
        return false;
    }
    world.get::<T>(owner).is_none()
}

pub(super) fn publish<T: PartialEq>(
    sender: &watch::Sender<Option<T>>,
    next: T,
    wake: Option<&WakeSignal>,
) -> bool {
    if sender.is_closed() {
        return false;
    }
    let changed = sender.send_if_modified(|current| {
        if current.as_ref() == Some(&next) {
            return false;
        }
        *current = Some(next);
        true
    });
    if changed {
        performance::count(Count::SyncSnapshotPublished);
        if let Some(wake) = wake {
            wake.ring();
        }
    } else {
        performance::count(Count::SyncSnapshotUnchanged);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[test]
    fn snapshots_preserve_first_changes_errors_recovery_and_closed_views() {
        let wakes = Arc::new(AtomicUsize::new(0));
        let observed = wakes.clone();
        let wake = WakeSignal::new(move || {
            observed.fetch_add(1, Ordering::Relaxed);
        });
        let (sender, mut receiver) = watch::channel(None);
        let original: Result<Value, String> = Ok(json!({
            "rows": [{"uid":"private-fixture", "title":"café 日本語", "allowed":true}],
            "pending":0
        }));
        assert!(publish(&sender, original.clone(), Some(&wake)));
        assert!(receiver.has_changed().unwrap());
        assert_eq!(receiver.borrow_and_update().as_ref(), Some(&original));
        for _ in 0..128 {
            assert!(publish(&sender, original.clone(), Some(&wake)));
        }
        assert!(!receiver.has_changed().unwrap());
        assert_eq!(wakes.load(Ordering::Relaxed), 1);
        for changed in [
            Ok(json!({"rows":[],"pending":0})),
            Err("Permission changed".into()),
            original.clone(),
            Ok(json!({"rows":[],"pending":1})),
        ] {
            assert!(publish(&sender, changed.clone(), Some(&wake)));
            assert!(receiver.has_changed().unwrap());
            assert_eq!(receiver.borrow_and_update().as_ref(), Some(&changed));
            assert!(publish(&sender, changed, Some(&wake)));
            assert!(!receiver.has_changed().unwrap());
        }
        assert_eq!(wakes.load(Ordering::Relaxed), 5);
        drop(receiver);
        assert!(!publish(&sender, original, Some(&wake)));
        assert_eq!(wakes.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn a_slow_view_receives_the_latest_changed_state_without_a_wake_callback() {
        let (sender, mut receiver) = watch::channel(None);
        assert!(publish(&sender, 1, None));
        assert!(publish(&sender, 2, None));
        assert!(publish(&sender, 2, None));
        assert!(receiver.has_changed().unwrap());
        assert_eq!(*receiver.borrow_and_update(), Some(2));
        assert!(!receiver.has_changed().unwrap());
        assert!(publish(&sender, 1, None));
        assert_eq!(*receiver.borrow_and_update(), Some(1));
    }
}
