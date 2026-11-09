use std::time::{Duration, Instant};

pub(super) struct Pacing<T> {
    period: Duration,
    last: Option<Instant>,
    pending: Option<(Instant, T)>,
}

impl<T> Pacing<T> {
    pub fn new(refresh_millihz: i32) -> Self {
        let refresh = u64::try_from(refresh_millihz)
            .ok()
            .filter(|value| *value > 0)
            .unwrap_or(60_000);
        Self {
            period: Duration::from_nanos((1_000_000_000_000 / refresh).max(1)),
            last: None,
            pending: None,
        }
    }

    pub fn defer(&mut self, now: Instant, frame: T) {
        let deadline = self
            .pending
            .as_ref()
            .map(|(deadline, _)| *deadline)
            .unwrap_or_else(|| {
                let elapsed = self
                    .last
                    .map_or(0, |last| now.saturating_duration_since(last).as_nanos());
                let remaining = self.period.as_nanos() - elapsed % self.period.as_nanos();
                now + Duration::from_nanos(remaining as u64)
            });
        self.pending = Some((deadline, frame));
    }

    pub fn presented(&mut self, now: Instant) {
        self.pending = None;
        self.last = Some(now);
    }

    pub fn cancel(&mut self) {
        self.pending = None;
    }

    pub fn timeout(&self, now: Instant) -> Option<Duration> {
        self.pending
            .as_ref()
            .map(|(deadline, _)| deadline.saturating_duration_since(now))
    }

    pub fn take_due(&mut self, now: Instant) -> Option<T> {
        if self
            .pending
            .as_ref()
            .is_none_or(|(deadline, _)| *deadline > now)
        {
            return None;
        }
        self.last = Some(now);
        self.pending.take().map(|(_, frame)| frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_demand_has_no_timer_and_replacements_do_not_postpone_delivery() {
        let now = Instant::now();
        let mut clock = Pacing::new(60_000);
        assert_eq!(clock.timeout(now), None);
        clock.defer(now, 1);
        let deadline = now + clock.timeout(now).unwrap();
        assert!(clock.take_due(now).is_none());
        clock.defer(now + Duration::from_millis(5), 2);
        assert_eq!(clock.timeout(now), Some(deadline - now));
        assert_eq!(clock.take_due(deadline), Some(2));
        assert_eq!(clock.timeout(deadline), None);
        clock.defer(deadline, 3);
        assert!(clock.timeout(deadline).unwrap() >= Duration::from_millis(16));
    }

    #[test]
    fn outputs_are_independent_and_late_dispatch_never_catches_up_in_a_burst() {
        let now = Instant::now();
        let mut slow = Pacing::new(60_000);
        let mut fast = Pacing::new(144_000);
        slow.presented(now);
        fast.presented(now);
        slow.defer(now, 1);
        fast.defer(now, 2);
        let later = now + Duration::from_millis(8);
        assert_eq!(fast.take_due(later), Some(2));
        assert_eq!(slow.take_due(later), None);
        fast.defer(later, 3);
        assert!(fast.take_due(later).is_none());
        assert_eq!(slow.take_due(now + Duration::from_secs(2)), Some(1));
        slow.defer(now + Duration::from_secs(2), 4);
        assert!(slow.timeout(now + Duration::from_secs(2)).unwrap() >= Duration::from_millis(16));
    }

    #[test]
    fn real_submission_or_invalidation_retires_the_estimate() {
        let now = Instant::now();
        let mut clock = Pacing::new(59_940);
        clock.defer(now, 1);
        clock.presented(now + Duration::from_millis(1));
        assert!(clock.take_due(now + Duration::from_secs(1)).is_none());
        clock.defer(now, 2);
        clock.cancel();
        assert_eq!(clock.timeout(now), None);
    }
}
