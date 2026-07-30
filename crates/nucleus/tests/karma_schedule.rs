use std::num::NonZeroU32;

use nucleus::karma::{
    ArmWindow, ElapsedSchedule, InactiveGapPolicy, MissedPolicy, OccurrenceRange, OverloadPolicy,
    RationalRate, RephasePolicy, ScheduleEmission, TimerPolicy, TimestampMs, canonical_hash,
};
use serde::Serialize;
use serde_json::json;

#[test]
fn three_milliseconds_and_five_hours_keep_independent_exact_cursors() {
    let anchor = at("2026-07-21T09:00:00.000Z");
    let fast = schedule(
        anchor,
        3,
        timer(1, 1, 0),
        MissedPolicy::Replay {
            max: NonZeroU32::new(64).unwrap(),
        },
        InactiveGapPolicy::SkipToNextAnchor,
    );
    let slow = schedule(
        anchor,
        18_000_000,
        timer(1, 20, 0),
        MissedPolicy::Coalesce,
        InactiveGapPolicy::SkipToNextAnchor,
    );
    assert_eq!(fast.semantic_rate(), RationalRate::new(1_000, 3).unwrap());
    assert_eq!(slow.semantic_rate(), RationalRate::new(1, 18_000).unwrap());

    let fast_cursor = fast.initial_cursor(anchor).unwrap();
    let slow_cursor = slow.initial_cursor(anchor).unwrap();
    assert_eq!(
        fast_cursor.next_intended_at(),
        at("2026-07-21T09:00:00.003Z")
    );
    assert_eq!(
        slow_cursor.next_intended_at(),
        at("2026-07-21T14:00:00.000Z")
    );

    let fast_advance = fast
        .advance(&fast_cursor, at("2026-07-21T09:00:00.010Z"))
        .unwrap()
        .unwrap();
    assert_eq!(fast_advance.due.count(), 3);
    assert_eq!(fast_advance.late_count, 2);
    assert_eq!(
        fast_advance.cursor.next_intended_at(),
        at("2026-07-21T09:00:00.012Z"),
        "late observed time must not move the anchor"
    );
    assert_eq!(
        slow_cursor.next_intended_at(),
        at("2026-07-21T14:00:00.000Z"),
        "advancing the dense schedule does not inspect or mutate the sparse one"
    );

    let slow_advance = slow
        .advance(&slow_cursor, at("2026-07-21T14:00:00.015Z"))
        .unwrap()
        .unwrap();
    assert_eq!(slow_advance.due.count(), 1);
    assert_eq!(slow_advance.late_count, 0);
    assert_eq!(
        slow_advance.cursor.next_intended_at(),
        at("2026-07-21T19:00:00.000Z")
    );
}

#[test]
fn every_missed_policy_records_an_exact_due_and_skipped_range() {
    let anchor = at("2026-07-21T09:00:00.000Z");
    let observed = at("2026-07-21T09:00:00.010Z");

    let skip = schedule(
        anchor,
        3,
        timer(1, 1, 0),
        MissedPolicy::Skip,
        InactiveGapPolicy::SkipToNextAnchor,
    );
    let cursor = skip.initial_cursor(anchor).unwrap();
    let advance = skip.advance(&cursor, observed).unwrap().unwrap();
    assert_eq!(advance.due.count(), 3);
    assert_eq!(advance.late_count, 2);
    assert_eq!(advance.skipped.as_ref().unwrap().count(), 2);
    assert_eq!(emitted_range(&advance.emission).count(), 1);
    assert_eq!(
        emitted_range(&advance.emission).first(),
        at("2026-07-21T09:00:00.009Z")
    );

    let all_late = skip
        .advance(&cursor, at("2026-07-21T09:00:00.011Z"))
        .unwrap()
        .unwrap();
    assert!(all_late.emission.is_none());
    assert_eq!(all_late.skipped.as_ref().unwrap().count(), 3);

    let coalesce = schedule(
        anchor,
        3,
        timer(1, 1, 0),
        MissedPolicy::Coalesce,
        InactiveGapPolicy::SkipToNextAnchor,
    );
    let advance = coalesce
        .advance(&coalesce.initial_cursor(anchor).unwrap(), observed)
        .unwrap()
        .unwrap();
    assert!(matches!(
        advance.emission,
        Some(ScheduleEmission::Coalesced(_))
    ));
    assert_eq!(emitted_range(&advance.emission).count(), 3);
    assert!(advance.skipped.is_none());

    let replay = schedule(
        anchor,
        3,
        timer(1, 1, 0),
        MissedPolicy::Replay {
            max: NonZeroU32::new(2).unwrap(),
        },
        InactiveGapPolicy::SkipToNextAnchor,
    );
    let advance = replay
        .advance(&replay.initial_cursor(anchor).unwrap(), observed)
        .unwrap()
        .unwrap();
    assert_eq!(emitted_range(&advance.emission).count(), 2);
    assert_eq!(advance.skipped.as_ref().unwrap().count(), 1);
    assert_eq!(advance.replay_overflow, 1);

    let pause = schedule(
        anchor,
        3,
        timer(1, 1, 0),
        MissedPolicy::PauseOnLag,
        InactiveGapPolicy::SkipToNextAnchor,
    );
    let cursor = pause.initial_cursor(anchor).unwrap();
    let advance = pause.advance(&cursor, observed).unwrap().unwrap();
    assert!(advance.paused);
    assert!(advance.emission.is_none());
    assert!(advance.skipped.is_none());
    assert_eq!(advance.cursor, cursor, "pause advances no semantic cursor");
}

#[test]
fn inactive_gap_is_either_explicitly_skipped_or_left_for_missed_policy() {
    let anchor = at("2026-07-21T09:00:00.000Z");
    let skip = schedule(
        anchor,
        3,
        timer(1, 1, 0),
        MissedPolicy::Coalesce,
        InactiveGapPolicy::SkipToNextAnchor,
    );
    let cursor = skip.initial_cursor(anchor).unwrap();
    let reactivated = skip
        .reactivate(&cursor, at("2026-07-21T09:00:00.015Z"))
        .unwrap();
    let skipped = reactivated.inactive_skipped.unwrap();
    assert_eq!(skipped.first(), at("2026-07-21T09:00:00.003Z"));
    assert_eq!(skipped.last().unwrap(), at("2026-07-21T09:00:00.015Z"));
    assert_eq!(skipped.count(), 5);
    assert_eq!(
        reactivated.cursor.next_intended_at(),
        at("2026-07-21T09:00:00.018Z")
    );

    let replay = schedule(
        anchor,
        3,
        timer(1, 1, 0),
        MissedPolicy::Coalesce,
        InactiveGapPolicy::ReplayByMissedPolicy,
    );
    let cursor = replay.initial_cursor(anchor).unwrap();
    let reactivated = replay
        .reactivate(&cursor, at("2026-07-21T09:00:00.015Z"))
        .unwrap();
    assert_eq!(reactivated.cursor, cursor);
    assert!(reactivated.inactive_skipped.is_none());
    let caught_up = replay
        .advance(&reactivated.cursor, at("2026-07-21T09:00:00.015Z"))
        .unwrap()
        .unwrap();
    assert_eq!(caught_up.due.count(), 5);
}

#[test]
fn all_rephase_policies_have_distinct_anchor_and_cursor_semantics() {
    let anchor = at("2026-07-21T09:00:00.000Z");
    let base = schedule(
        anchor,
        10,
        timer(1, 2, 0),
        MissedPolicy::Coalesce,
        InactiveGapPolicy::SkipToNextAnchor,
    );
    let initial = base.initial_cursor(anchor).unwrap();
    let cursor = base
        .advance(&initial, at("2026-07-21T09:00:00.010Z"))
        .unwrap()
        .unwrap()
        .cursor;
    let changed = at("2026-07-21T09:00:00.025Z");

    let preserve = base
        .rephase(&cursor, changed, 7, RephasePolicy::PreserveAnchor)
        .unwrap();
    assert_eq!(preserve.schedule.anchor(), anchor);
    assert_eq!(
        preserve.cursor.next_intended_at(),
        at("2026-07-21T09:00:00.028Z")
    );
    assert_eq!(preserve.cursor.last_intended_at(), None);

    let from_last = base
        .rephase(&cursor, changed, 7, RephasePolicy::FromLastIntended)
        .unwrap();
    assert_eq!(from_last.schedule.anchor(), at("2026-07-21T09:00:00.010Z"));
    assert_eq!(
        from_last.cursor.next_intended_at(),
        at("2026-07-21T09:00:00.031Z")
    );
    assert_eq!(
        from_last.cursor.last_intended_at(),
        Some(at("2026-07-21T09:00:00.010Z"))
    );

    let from_change = base
        .rephase(&cursor, changed, 7, RephasePolicy::FromChange)
        .unwrap();
    assert_eq!(from_change.schedule.anchor(), changed);
    assert_eq!(
        from_change.cursor.next_intended_at(),
        at("2026-07-21T09:00:00.032Z")
    );

    let immediate = base
        .rephase(&cursor, changed, 7, RephasePolicy::ImmediateIfOverdue)
        .unwrap();
    assert_eq!(immediate.schedule.anchor(), at("2026-07-21T09:00:00.020Z"));
    assert_eq!(
        immediate.cursor.next_intended_at(),
        at("2026-07-21T09:00:00.020Z")
    );

    let not_overdue = base
        .rephase(
            &cursor,
            at("2026-07-21T09:00:00.015Z"),
            7,
            RephasePolicy::ImmediateIfOverdue,
        )
        .unwrap();
    assert_eq!(not_overdue.schedule.anchor(), anchor);
    assert_eq!(
        not_overdue.cursor.next_intended_at(),
        at("2026-07-21T09:00:00.021Z")
    );
}

#[test]
fn timer_windows_bound_coalescing_without_changing_intended_time() {
    let anchor = at("2026-07-21T09:00:00.000Z");
    let schedule = schedule(
        anchor,
        3,
        timer(1, 20, 5),
        MissedPolicy::Coalesce,
        InactiveGapPolicy::SkipToNextAnchor,
    );
    let intended = schedule.initial_cursor(anchor).unwrap().next_intended_at();
    assert_eq!(
        schedule.arm_window(intended).unwrap(),
        ArmWindow {
            earliest: at("2026-07-21T09:00:00.003Z"),
            latest: at("2026-07-21T09:00:00.008Z"),
        }
    );
    assert_eq!(intended, at("2026-07-21T09:00:00.003Z"));
}

#[test]
fn invalid_schedule_wire_values_and_overflow_fail_at_the_boundary() {
    assert!(TimerPolicy::new(0, 1, 0).is_err());
    assert!(TimerPolicy::new(1, 1, 2).is_err());
    assert!(
        ElapsedSchedule::new(
            at("2026-07-21T09:00:00.000Z"),
            0,
            timer(1, 1, 0),
            MissedPolicy::Skip,
            InactiveGapPolicy::SkipToNextAnchor,
            RephasePolicy::PreserveAnchor,
            OverloadPolicy::PauseAndAsk,
        )
        .is_err()
    );
    assert!(OccurrenceRange::new(at("9999-12-31T23:59:59.999Z"), 1, 2).is_err());
    assert!(RationalRate::new(1, 0).is_err());

    let valid = schedule(
        at("2026-07-21T09:00:00.000Z"),
        3,
        timer(1, 1, 0),
        MissedPolicy::Skip,
        InactiveGapPolicy::SkipToNextAnchor,
    );
    let mut wire = serde_json::to_value(&valid).unwrap();
    wire["interval_ms"] = json!(0);
    assert!(serde_json::from_value::<ElapsedSchedule>(wire).is_err());
    assert!(serde_json::from_str::<MissedPolicy>(r#"{"kind":"replay","max":0}"#).is_err());
    assert!(
        serde_json::from_str::<OccurrenceRange>(
            r#"{"first":"2026-07-21T09:00:00.003Z","interval_ms":3,"count":0}"#
        )
        .is_err()
    );
}

#[test]
fn schedule_wire_vocabulary_has_a_golden_hash() {
    let anchor = at("2026-07-21T09:00:00.000Z");
    let schedule = schedule(
        anchor,
        3,
        timer(1, 20, 5),
        MissedPolicy::Replay {
            max: NonZeroU32::new(64).unwrap(),
        },
        InactiveGapPolicy::SkipToNextAnchor,
    );
    let cursor = schedule.initial_cursor(anchor).unwrap();
    let advance = schedule
        .advance(&cursor, at("2026-07-21T09:00:00.010Z"))
        .unwrap()
        .unwrap();
    let fixture = ScheduleVocabularyFixture {
        rates: vec![
            RationalRate::new(1_000, 3).unwrap(),
            RationalRate::new(1, 18_000).unwrap(),
        ],
        timers: vec![timer(1, 1, 0), timer(1, 20, 5)],
        missed: vec![
            MissedPolicy::Skip,
            MissedPolicy::Coalesce,
            MissedPolicy::Replay {
                max: NonZeroU32::new(64).unwrap(),
            },
            MissedPolicy::PauseOnLag,
        ],
        inactive: vec![
            InactiveGapPolicy::SkipToNextAnchor,
            InactiveGapPolicy::ReplayByMissedPolicy,
        ],
        rephase: vec![
            RephasePolicy::PreserveAnchor,
            RephasePolicy::FromLastIntended,
            RephasePolicy::FromChange,
            RephasePolicy::ImmediateIfOverdue,
        ],
        overload: vec![
            OverloadPolicy::PauseAndAsk,
            OverloadPolicy::RejectActivation,
            OverloadPolicy::DegradeWithinGrant,
        ],
        schedule,
        cursor,
        advance,
    };
    assert_eq!(
        canonical_hash("karma.schedule-vocabulary.v1", &fixture)
            .unwrap()
            .as_str(),
        "sha256:9b80e08ae0e7e7ab2f235cf36ca6b0a7c586f2df727e032b03818b81bcb20213"
    );
}

#[derive(Serialize)]
struct ScheduleVocabularyFixture {
    rates: Vec<RationalRate>,
    timers: Vec<TimerPolicy>,
    missed: Vec<MissedPolicy>,
    inactive: Vec<InactiveGapPolicy>,
    rephase: Vec<RephasePolicy>,
    overload: Vec<OverloadPolicy>,
    schedule: ElapsedSchedule,
    cursor: nucleus::karma::ScheduleCursor,
    advance: nucleus::karma::ScheduleAdvance,
}

fn schedule(
    anchor: TimestampMs,
    interval_ms: u64,
    timer: TimerPolicy,
    missed: MissedPolicy,
    inactive_gap: InactiveGapPolicy,
) -> ElapsedSchedule {
    ElapsedSchedule::new(
        anchor,
        interval_ms,
        timer,
        missed,
        inactive_gap,
        RephasePolicy::PreserveAnchor,
        OverloadPolicy::PauseAndAsk,
    )
    .unwrap()
}

fn timer(resolution: u32, lateness: u32, coalesce: u32) -> TimerPolicy {
    TimerPolicy::new(resolution, lateness, coalesce).unwrap()
}

fn at(value: &str) -> TimestampMs {
    TimestampMs::parse_canonical(value).unwrap()
}

fn emitted_range(emission: &Option<ScheduleEmission>) -> &OccurrenceRange {
    match emission.as_ref().expect("emission") {
        ScheduleEmission::Individual(range) | ScheduleEmission::Coalesced(range) => range,
    }
}
