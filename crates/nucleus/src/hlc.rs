//! Hybrid logical clock, packed into one `i64` (Ontology §11 "Op log").
//!
//! 48 bits of wall-clock milliseconds + a 16-bit logical counter, so the whole
//! stamp is a single SQLite `INTEGER` and compare/sort/index are native int
//! operations. The clock follows wall time but is bumped past any stamp it has
//! seen (its own or an imported one), so "latest wins" stays consistent across
//! machines with skewed clocks and never goes backwards.
//!
//! One clock per Cell: a process-wide atomic. Two Cells sharing a process (as
//! integration tests do) share the counter, which preserves the only two
//! properties that matter — per-actor uniqueness and monotonicity.

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

/// Stamp the next local HLC: max(now, last seen + 1). A counter overflow rolls
/// into the millisecond bits, which is still a strictly greater stamp — the
/// packing makes "bump the counter" and "advance time" the same operation.
pub fn next() -> i64 {
    let floor = chrono::Utc::now().timestamp_millis() << COUNTER_BITS;
    let prev = LAST
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |last| {
            Some(if floor > last { floor } else { last + 1 })
        })
        .expect("closure always returns Some");
    std::cmp::max(floor, prev + 1)
}

/// Advance the clock past a stamp seen elsewhere (an imported op, or the log's
/// max at boot), so nothing stamped later can compare below it.
pub fn observe(seen: i64) {
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
