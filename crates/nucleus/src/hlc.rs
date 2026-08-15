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

/// How far ahead of local wall time a stamp from elsewhere may be and still be
/// believed. Cells disagree about the time and there is no authority to ask,
/// so every cross-Cell time comparison is tolerant by construction rather than
/// exact (Ontology §11, decision 5) — and this is the ONE tolerance, shared
/// with lease takeover and card TTL grace. Three subsystems reasoning about
/// skew with three different tolerances is how they come to disagree.
pub const MAX_CLOCK_DRIFT_MS: i64 = 5 * 60 * 1000;

/// Whether a stamp from elsewhere is close enough to now to be believed.
///
/// Without this, one op stamped with a far-future HLC drags this Cell's clock
/// there permanently: `observe` takes the max, `next()` never goes backwards,
/// and every later local write is stamped in that same future. It wins every
/// LWW race it will ever be in, and nothing on the machine can undo it. The
/// bound turns "poison the clock forever" into "one op is refused".
///
/// Only the FUTURE is bounded. A stamp from the past is ordinary — an op
/// written while a peer was offline, or a Cell whose clock is behind — and
/// refusing those would drop honest history.
pub fn within_drift(seen: i64) -> bool {
    wall_ms(seen) <= chrono::Utc::now().timestamp_millis() + MAX_CLOCK_DRIFT_MS
}

/// Stamp the next local HLC: max(now, last seen + 1). A counter overflow rolls
/// into the millisecond bits, which is still a strictly greater stamp — the
/// packing makes "bump the counter" and "advance time" the same operation.
///
/// Saturating, so the packed stamp can never wrap into a negative and start
/// comparing BELOW every real stamp. With `observe` bounded this is not
/// reachable, which is exactly why it is cheap to make certain.
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

/// Advance the clock past a stamp seen elsewhere (an imported op, or the log's
/// max at boot), so nothing stamped later can compare below it.
///
/// A stamp beyond `MAX_CLOCK_DRIFT_MS` is IGNORED here rather than trusted.
/// Callers that can refuse the whole op should check `within_drift` first and
/// quarantine it; this is the backstop for the ones that cannot.
pub fn observe(seen: i64) {
    if within_drift(seen) {
        LAST.fetch_max(seen, Ordering::SeqCst);
    }
}

/// Advance past a stamp THIS CELL itself wrote, read back from its own log —
/// unconditionally, with no drift bound.
///
/// The bound on `observe` exists to stop a hostile or broken PEER from
/// dragging this clock into the future, and your own database is not a peer.
/// Applying it here would make the two guards fight: `store::sync_ops` re-mints
/// a local stamp when another process writing as this same Cell has already
/// taken the identity, and it jumps past what that process published. If that
/// stamp were beyond the drift window, `observe` would ignore it, `next()`
/// would return roughly local-now, and every retry would collide against the
/// same occupied range until the write failed outright — turning a clock skew
/// on your own machine into a refusal to save.
///
/// So: a peer's claim is bounded, this Cell's own history is believed.
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
