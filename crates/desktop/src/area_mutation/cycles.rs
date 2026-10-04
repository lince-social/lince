use bevy::prelude::*;
use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

struct Window {
    root: Entity,
    workspace: u64,
    started: Instant,
    crossings: [VecDeque<Instant>; 2],
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
            (area, "r_test".into()),
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
        for i in 0..4096 {
            let uid = format!("r_{i}");
            assert!(cycles.check(area, 1, area, &uid, true, 4).is_ok());
            cycles.record(area, 1, area, &uid, true);
        }
        assert!(cycles.check(area, 1, area, "r_extra", true, 4).is_err());
        assert!(cycles.check(area, 1, area, "r_0", false, 4).is_ok());
        cycles.forget(area);
        assert!(cycles.check(area, 1, area, "r_extra", true, 4).is_ok());
    }
}

#[derive(Default)]
pub(super) struct Cycles {
    windows: HashMap<(Entity, String), Window>,
    swept: Option<Instant>,
}

impl Cycles {
    pub fn forget(&mut self, area: Entity) {
        self.windows.retain(|(entity, _), _| *entity != area);
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
        let lifetime = Duration::from_secs(10);
        if self
            .swept
            .is_none_or(|last| now.duration_since(last) >= Duration::from_secs(1))
        {
            self.windows
                .retain(|_, window| now.duration_since(window.started) < lifetime);
            self.swept = Some(now);
        }
        let key = (area, uid.to_string());
        if !self.windows.contains_key(&key) && self.windows.len() >= 4096 {
            return Err(
                "Area paused: the bounded crossing history is full. Resume after reviewing the rules.",
            );
        }
        if self.windows.get(&key).is_some_and(|window| {
            window.root == root
                && window.workspace == workspace
                && now.duration_since(window.started) < lifetime
                && window.crossings[usize::from(inside)]
                    .iter()
                    .filter(|when| now.duration_since(**when) < lifetime)
                    .count()
                    >= usize::from(limit)
        }) {
            return Err(
                "Area paused: repeated boundary cycle for this Record. Review the forces and entry/exit changes before resuming.",
            );
        }
        Ok(())
    }

    pub fn has(&self, area: Entity, uid: &str) -> bool {
        self.windows.contains_key(&(area, uid.into()))
    }

    pub fn available(&self) -> usize {
        4096 - self.windows.len()
    }

    pub fn record(&mut self, root: Entity, workspace: u64, area: Entity, uid: &str, inside: bool) {
        let now = Instant::now();
        let window = self.windows.entry((area, uid.into())).or_insert(Window {
            root,
            workspace,
            started: now,
            crossings: Default::default(),
        });
        if window.root != root
            || window.workspace != workspace
            || now.duration_since(window.started) >= Duration::from_secs(10)
        {
            *window = Window {
                root,
                workspace,
                started: now,
                crossings: Default::default(),
            };
        }
        window.started = now;
        for crossings in &mut window.crossings {
            while crossings
                .front()
                .is_some_and(|when| now.duration_since(*when) >= Duration::from_secs(10))
            {
                crossings.pop_front();
            }
        }
        window.crossings[usize::from(inside)].push_back(now);
    }
}
