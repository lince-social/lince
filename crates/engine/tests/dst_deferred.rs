//! Properties this document depends on that no single-process test can reach.
//!
//! Every test here is `#[ignore]`d and every one is REAL — it compiles, it
//! names its scenario, and it fails honestly rather than passing vacuously,
//! because a stub that passes is worse than no test at all. They are ignored
//! because they need what Resenha's DST mode supplies: several Cells with
//! separate stores, a network that can be partitioned asymmetrically, and a
//! clock the harness drives.
//!
//! **This file is the backlog.** When DST mode lands, its first job is removing
//! these attributes one at a time. Until then the rule that produced the file
//! stands: a property that cannot be tested now gets WRITTEN now, with its seed
//! conditions named, instead of being recorded as a sentence in a plan where it
//! quietly becomes a hope.
//!
//! Each test states, in order: the property, the setup the simulator must
//! provide, and what would have to be observed for it to pass. That ordering is
//! deliberate — the setup is the part a future reader cannot reconstruct, and
//! it is the part that decides whether the assertion means anything.

/// C7 — a rule marked single-executor runs on exactly one Cell.
///
/// **Setup:** three Cells of one Organ, all holding the same synced Program,
/// one designated executor. Sixty simulated days, the schedule firing daily.
///
/// **Passes when:** the count of effects equals the count of days. Not "no
/// duplicates within a window" — exactly once over the whole run, because the
/// failure this guards is a slow leak that a windowed assertion averages away.
#[test]
#[ignore = "Resenha DST: needs three Cells with separate stores"]
fn a_designated_rule_acts_exactly_once_across_three_cells() {
    unimplemented!("scenario: three Cells, one designated executor, 60 days");
}

/// C7 — the laptop must NOT take over merely because it cannot see the VPS.
///
/// **Setup:** two Cells, designated executor on the always-on one, then a
/// ONE-WAY partition: the laptop stops receiving from the VPS while the VPS
/// keeps running normally. This is the ordinary state of a laptop, not an
/// exotic fault, and it is the exact case where heartbeat-and-expiry failover
/// produces duplicates in the steady state.
///
/// **Passes when:** the laptop performs no outward act for the whole partition,
/// and the VPS's acts continue uninterrupted.
#[test]
#[ignore = "Resenha DST: needs an asymmetric partition"]
fn an_unreachable_holder_does_not_hand_the_lease_to_whoever_cannot_see_it() {
    unimplemented!("scenario: one-way partition, designated executor unreachable");
}

/// C7 — the non-Rule schedulers duplicate the same way and need the same answer.
///
/// **Setup:** two Cells, both with transfer delivery retries pending for the
/// same Transfer, both online.
///
/// **Passes when:** the counterparty receives one delivery, not two. Filed here
/// rather than under Karma because it is the same defect through a different
/// scheduler, and fixing it for Rules alone would leave it live.
///
/// **The local half is wired — 2026-08-15.** `drain_envelopes` now consults
/// `store::executor::runs_here` on the Transfer Record and SKIPS rather than
/// fails, so a Cell that is not the designated one burns none of the sender's
/// attempt budget. `store/tests/executor.rs` pins the predicate. What is still
/// untestable here is the only part that matters to the recipient: that the
/// second Cell, running the same loop against the same Transfer at the same
/// time, actually stays quiet — which needs two Cells and a counterparty
/// counting arrivals.
#[test]
#[ignore = "Resenha DST: needs two Cells sharing one pending delivery"]
fn two_cells_do_not_each_retry_the_same_transfer_delivery() {
    unimplemented!("scenario: two Cells, one pending delivery, both online");
}

/// C1/C2 — two Cells writing as one Organ never collide on op identity.
///
/// **Setup:** two Cells writing concurrently for a simulated hour, with the
/// clock skewed between them and at least one backwards jump.
///
/// **Passes when:** no write is lost and no identity is reused. The
/// multi-process harness covers two processes sharing ONE store; this is the
/// other case — separate stores reconciling through sync — and the backwards
/// jump is the part that makes it worth running.
#[test]
#[ignore = "Resenha DST: needs a driven clock with skew"]
fn concurrent_cells_under_clock_skew_lose_no_write() {
    unimplemented!("scenario: two Cells, skewed clocks, one backwards jump");
}

/// C4 — relay-only mode actually relays, and the always-on Cell is always on.
///
/// **Setup:** three nodes — two Cells that cannot reach each other directly
/// and one relay — with the relay restarted mid-run.
///
/// **Passes when:** both Cells converge, and the relay observes no plaintext it
/// is not entitled to. Recorded as VPS-shaped and deferred in C4; it is
/// scenario-shaped, not deployment-shaped, so it belongs here.
#[test]
#[ignore = "Resenha DST: needs three nodes and a restartable relay"]
fn two_unreachable_cells_converge_through_a_restarted_relay() {
    unimplemented!("scenario: two Cells behind one relay, relay restarts mid-run");
}

/// C5 — narrowing a contact's scope never retroactively leaks.
///
/// **Setup:** two Organs mid-sync, scope narrowed WHILE a batch is in flight,
/// with the network reordering and duplicating delivery.
///
/// **Passes when:** the receiving Organ holds nothing outside the narrowed
/// scope once the dust settles. The single-process tests pin the predicate;
/// this pins the RACE, which is where a filter applied at serve time can still
/// lose to a batch already queued.
#[test]
#[ignore = "Resenha DST: needs reorder and duplicate injection"]
fn narrowing_mid_flight_leaves_nothing_outside_the_new_scope() {
    unimplemented!("scenario: narrow scope during an in-flight batch, reordered delivery");
}
