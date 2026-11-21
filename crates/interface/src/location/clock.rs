use std::{
    sync::OnceLock,
    time::{Duration, Instant},
};

use bevy::prelude::*;

/// Reads elapsed milliseconds from the UI's monotonic clock.
#[derive(Resource, Clone, Copy)]
pub struct ElapsedClock(pub fn() -> u64);

impl Default for ElapsedClock {
    fn default() -> Self {
        Self(standard)
    }
}

fn standard() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    u64::try_from(START.get_or_init(Instant::now).elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[derive(Clone, Copy)]
pub(super) struct Stamp {
    at: i128,
    read: fn() -> u64,
}

impl Stamp {
    pub(super) fn for_world(world: &World) -> Self {
        Self::with(
            world
                .get_resource::<ElapsedClock>()
                .copied()
                .unwrap_or_default(),
        )
    }

    #[cfg(test)]
    pub(super) fn now() -> Self {
        Self::with(ElapsedClock::default())
    }

    fn with(clock: ElapsedClock) -> Self {
        Self {
            at: i128::from((clock.0)()),
            read: clock.0,
        }
    }

    pub(super) fn elapsed(self) -> Duration {
        Duration::from_millis(self.elapsed_millis())
    }

    pub(super) fn elapsed_millis(self) -> u64 {
        let elapsed = (i128::from((self.read)()) - self.at).max(0);
        u64::try_from(elapsed).unwrap_or(u64::MAX)
    }
}

#[cfg(test)]
impl std::ops::Sub<Duration> for Stamp {
    type Output = Self;
    fn sub(self, duration: Duration) -> Self {
        Self {
            at: self.at - duration.as_millis() as i128,
            ..self
        }
    }
}
