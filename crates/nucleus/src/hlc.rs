use std::sync::atomic::{AtomicI64, Ordering};

pub const COUNTER_BITS: u32 = 16;
const COUNTER_MASK: i64 = (1 << COUNTER_BITS) - 1;

static LAST: AtomicI64 = AtomicI64::new(0);

pub fn pack(wall_ms: i64, counter: u16) -> i64 {
    (wall_ms << COUNTER_BITS) | i64::from(counter)
}

pub fn wall_ms(hlc: i64) -> i64 {
    hlc >> COUNTER_BITS
}

pub fn counter(hlc: i64) -> u16 {
    (hlc & COUNTER_MASK) as u16
}

pub const MAX_CLOCK_DRIFT_MS: i64 = 5 * 60 * 1000;

pub fn within_drift(seen: i64) -> bool {
    wall_ms(seen) <= chrono::Utc::now().timestamp_millis() + MAX_CLOCK_DRIFT_MS
}

pub fn next() -> i64 {
    let floor = chrono::Utc::now().timestamp_millis() << COUNTER_BITS;
    let prev = LAST
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |last| {
            Some(if floor > last {
                floor
            } else {
                last.saturating_add(1)
            })
        })
        .expect("closure always returns Some");
    std::cmp::max(floor, prev.saturating_add(1))
}

pub fn observe(seen: i64) {
    if within_drift(seen) {
        LAST.fetch_max(seen, Ordering::SeqCst);
    }
}

pub fn adopt_own(seen: i64) {
    LAST.fetch_max(seen, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_round_trips() {
        let hlc = pack(1_754_000_000_000, 7);
        assert_eq!(wall_ms(hlc), 1_754_000_000_000);
        assert_eq!(counter(hlc), 7);
    }

    #[test]
    fn next_is_strictly_monotonic() {
        let mut prev = next();
        for _ in 0..10_000 {
            let stamp = next();
            assert!(stamp > prev);
            prev = stamp;
        }
    }

    #[test]
    fn observe_pushes_the_clock_forward() {
        let future = pack(chrono::Utc::now().timestamp_millis() + 60_000, 0);
        observe(future);
        assert!(next() > future);
    }
}
