use std::{
    ops::{Add, Sub},
    time::Duration,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Instant(i128);

impl Instant {
    pub fn now() -> Self {
        Self(i128::from(elapsed_millis()))
    }

    pub fn elapsed(self) -> Duration {
        Duration::from_millis((Self::now().0 - self.0).clamp(0, i128::from(u64::MAX)) as u64)
    }
}

impl Add<Duration> for Instant {
    type Output = Self;
    fn add(self, duration: Duration) -> Self {
        Self(self.0.saturating_add(duration.as_millis() as i128))
    }
}

impl Sub<Duration> for Instant {
    type Output = Self;
    fn sub(self, duration: Duration) -> Self {
        Self(self.0.saturating_sub(duration.as_millis() as i128))
    }
}

impl Sub for Instant {
    type Output = Duration;
    fn sub(self, other: Self) -> Duration {
        Duration::from_millis((self.0 - other.0).clamp(0, i128::from(u64::MAX)) as u64)
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
pub fn elapsed_millis() -> u64 {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Boottime);
    (now.tv_sec as u64)
        .saturating_mul(1000)
        .saturating_add(now.tv_nsec as u64 / 1_000_000)
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
pub fn elapsed_millis() -> u64 {
    static STATE: std::sync::OnceLock<
        std::sync::Mutex<(std::time::Instant, std::time::SystemTime, Duration)>,
    > = std::sync::OnceLock::new();
    let now = std::time::Instant::now();
    let wall = std::time::SystemTime::now();
    let mut state = STATE
        .get_or_init(|| std::sync::Mutex::new((now, wall, Duration::ZERO)))
        .lock()
        .expect("Location elapsed clock");
    let elapsed = now.saturating_duration_since(state.0).max(
        wall.duration_since(state.1)
            .unwrap_or(Duration::from_secs(86_400)),
    );
    state.2 = state.2.saturating_add(elapsed);
    state.0 = now;
    state.1 = wall;
    state.2.as_millis().min(u128::from(u64::MAX)) as u64
}
