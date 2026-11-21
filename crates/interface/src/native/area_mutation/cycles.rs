use bevy::{
    platform::collections::{Equivalent, HashMap},
    prelude::Entity,
};
use std::{
    collections::{VecDeque, hash_map::RandomState},
    time::{Duration, Instant},
};

const HISTORY_LIMIT: usize = 4096;
const LIFETIME: Duration = Duration::from_secs(10);
const SWEEP_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Hash, PartialEq, Eq)]
struct Key<S>(Entity, S);

impl Equivalent<Key<String>> for Key<&str> {
    fn equivalent(&self, key: &Key<String>) -> bool {
        self.0 == key.0 && self.1 == key.1
    }
}

impl From<&Key<&str>> for Key<String> {
    fn from(key: &Key<&str>) -> Self {
        Self(key.0, key.1.to_owned())
    }
}

struct Window {
    root: Entity,
    workspace: u64,
    started: Instant,
    crossings: [VecDeque<Instant>; 2],
}

impl Window {
    fn is_current(&self, root: Entity, workspace: u64, now: Instant) -> bool {
        self.root == root
            && self.workspace == workspace
            && now.duration_since(self.started) < LIFETIME
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_crossings_expire_and_checks_do_not_count_unsubmitted_work() {
        let mut cycles = Cycles::default();
        let area = Entity::PLACEHOLDER;
        let now = Instant::now();
        cycles.windows.insert(
            Key(area, "r_test".into()),
            Window {
                root: area,
                workspace: 1,
                started: now,
                crossings: [
                    VecDeque::new(),
                    VecDeque::from([now - Duration::from_secs(11), now]),
                ],
            },
        );
        for _ in 0..10 {
            assert!(cycles.check(area, 1, area, "r_test", true, 2).is_ok());
        }
        cycles.record(area, 1, area, "r_test", true);
        assert!(cycles.check(area, 1, area, "r_test", true, 2).is_err());
        assert!(cycles.check(area, 1, area, "r_test", false, 2).is_ok());
        assert!(cycles.check(area, 2, area, "r_test", true, 2).is_ok());
    }

    #[test]
    fn crossing_history_is_bounded_and_resuming_clears_it() {
        let mut cycles = Cycles::default();
        let area = Entity::PLACEHOLDER;
        for i in 0..HISTORY_LIMIT {
            let uid = format!("r_{i}");
            assert!(cycles.check(area, 1, area, &uid, true, 4).is_ok());
            cycles.record(area, 1, area, &uid, true);
        }
        assert!(cycles.check(area, 1, area, "r_extra", true, 4).is_err());
        assert!(cycles.check(area, 1, area, "r_0", false, 4).is_ok());
        cycles.forget(area);
        assert!(cycles.check(area, 1, area, "r_extra", true, 4).is_ok());
    }

    #[test]
    fn identical_record_ids_keep_distinct_area_histories() {
        let mut cycles = Cycles::default();
        let mut world = bevy::prelude::World::new();
        let [first, second] = std::array::from_fn(|_| world.spawn_empty().id());
        let uid = String::from("r_shared");
        cycles.record(first, 1, first, &uid, true);
        assert!(cycles.has(first, &uid));
        assert!(!cycles.has(second, &uid));
        assert!(cycles.check(second, 1, second, &uid, true, 1).is_ok());
        cycles.record(second, 1, second, &uid, false);
        assert_eq!(cycles.available(), HISTORY_LIMIT - 2);
        assert!(cycles.check(first, 1, first, &uid, true, 1).is_err());
        assert!(cycles.check(second, 1, second, &uid, false, 1).is_err());
        cycles.forget(first);
        assert!(!cycles.has(first, &uid));
        assert!(cycles.has(second, &uid));
    }
}

#[derive(Default)]
pub(super) struct Cycles {
    windows: HashMap<Key<String>, Window, RandomState>,
    swept: Option<Instant>,
}

impl Cycles {
    pub fn forget(&mut self, area: Entity) {
        self.windows.retain(|key, _| key.0 != area);
    }

    pub fn check(
        &mut self,
        root: Entity,
        workspace: u64,
        area: Entity,
        uid: &str,
        inside: bool,
        limit: u16,
    ) -> Result<(), &'static str> {
        let now = Instant::now();
        if self
            .swept
            .is_none_or(|last| now.duration_since(last) >= SWEEP_INTERVAL)
        {
            self.windows
                .retain(|_, window| now.duration_since(window.started) < LIFETIME);
            self.swept = Some(now);
        }
        let Some(window) = self.windows.get(&Key(area, uid)) else {
            return (self.windows.len() < HISTORY_LIMIT).then_some(()).ok_or(
                "Area paused: the bounded crossing history is full. Resume after reviewing the rules.",
            );
        };
        let limit = usize::from(limit);
        if window.is_current(root, workspace, now)
            && window.crossings[usize::from(inside)]
                .iter()
                .filter(|when| now.duration_since(**when) < LIFETIME)
                .take(limit)
                .count()
                == limit
        {
            return Err(
                "Area paused: repeated boundary cycle for this Record. Review the forces and entry/exit changes before resuming.",
            );
        }
        Ok(())
    }

    pub fn has(&self, area: Entity, uid: &str) -> bool {
        self.windows.contains_key(&Key(area, uid))
    }

    pub fn available(&self) -> usize {
        HISTORY_LIMIT - self.windows.len()
    }

    // Admission checks the batch capacity before recording submitted crossings.
    pub fn record(&mut self, root: Entity, workspace: u64, area: Entity, uid: &str, inside: bool) {
        let now = Instant::now();
        let fresh = || Window {
            root,
            workspace,
            started: now,
            crossings: Default::default(),
        };
        let window = self
            .windows
            .entry_ref(&Key(area, uid))
            .or_insert_with(fresh);
        if !window.is_current(root, workspace, now) {
            *window = fresh();
        }
        window.started = now;
        for crossings in &mut window.crossings {
            while crossings
                .front()
                .is_some_and(|when| now.duration_since(*when) >= LIFETIME)
            {
                crossings.pop_front();
            }
        }
        window.crossings[usize::from(inside)].push_back(now);
    }
}
