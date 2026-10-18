use bevy::prelude::*;
use std::time::Duration;

#[derive(Resource, Clone, Copy)]
pub struct ElapsedClock(pub fn() -> u64);

impl Default for ElapsedClock {
    fn default() -> Self {
        Self(standard)
    }
}

fn standard() -> u64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_millis() as u64
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
        Duration::from_millis(
            (i128::from((self.read)()) - self.at).clamp(0, i128::from(u64::MAX)) as u64,
        )
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
