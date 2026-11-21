use std::sync::atomic::{AtomicI64, Ordering::SeqCst};

use crate::execution::{current, now};

/// Number of low bits reserved for the logical counter.
pub const COUNTER_BITS: u32 = u16::BITS;

static LAST: AtomicI64 = AtomicI64::new(0);

/// Packs the low 48 bits of the wall time in milliseconds and the logical counter.
pub fn pack(wall_ms: i64, counter: u16) -> i64 {
    (wall_ms << COUNTER_BITS) | i64::from(counter)
}

/// Decodes the signed 48-bit wall time in milliseconds.
pub fn wall_ms(hlc: i64) -> i64 {
    hlc >> COUNTER_BITS
}

/// Decodes the logical counter.
pub fn counter(hlc: i64) -> u16 {
    let [low, high, ..] = hlc.to_le_bytes();
    u16::from_le_bytes([low, high])
}

/// Maximum accepted lead of a remote clock, in milliseconds.
pub const MAX_CLOCK_DRIFT_MS: i64 = 5 * 60 * 1000;

/// Returns whether a remote stamp is at most [`MAX_CLOCK_DRIFT_MS`] ahead.
pub fn within_drift(seen: i64) -> bool {
    wall_ms(seen) <= now().timestamp_millis() + MAX_CLOCK_DRIFT_MS
}

/// Advances the local clock, using the current wall time as the minimum stamp.
/// The clock saturates at [`i64::MAX`].
///
/// # Panics
///
/// Panics if the active execution's clock lock is poisoned.
pub fn next() -> i64 {
    if let Some(execution) = current() {
        return execution.next_hlc();
    }
    let floor = pack(now().timestamp_millis(), 0);
    let advance = |last: i64| floor.max(last.saturating_add(1));
    advance(LAST.update(SeqCst, SeqCst, advance))
}

/// Adopts a remote stamp if it is within the accepted clock drift.
///
/// # Panics
///
/// Panics if the stamp is accepted and the active execution's clock lock is poisoned.
pub fn observe(seen: i64) {
    if within_drift(seen) {
        adopt_own(seen);
    }
}

/// Advances the local clock to at least the given stamp from this device.
///
/// # Panics
///
/// Panics if the active execution's clock lock is poisoned.
pub fn adopt_own(seen: i64) {
    match current() {
        Some(execution) => execution.observe_hlc(seen),
        None => {
            LAST.fetch_max(seen, SeqCst);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_round_trips() {
        for time in [
            i64::MIN >> COUNTER_BITS,
            -1,
            0,
            1_754_000_000_000,
            i64::MAX >> COUNTER_BITS,
        ] {
            for logical in [0, 1, 7, u16::MAX] {
                let hlc = pack(time, logical);
                assert_eq!(wall_ms(hlc), time);
                assert_eq!(counter(hlc), logical);
            }
        }
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
