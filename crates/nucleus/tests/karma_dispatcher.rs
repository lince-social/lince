use std::num::{NonZeroU32, NonZeroUsize};

use nucleus::karma::{
    ArmedDeadline, CanonicalHash, DeadlineAdmission, DeadlineEntry, DeadlineIndex,
    DeadlineRejectionReason, DemandResource, DemandedDeadline, DispatcherResourceGrant,
    ElapsedSchedule, ElapsedScheduleOccurrenceSchema, HostTimerCapabilities, InactiveGapPolicy,
    MAX_OCCURRENCE_BATCH_PAGE_TICKS, MissedPolicy, OccurrenceBatch, OccurrenceBatchEmission,
    OccurrenceRange, OverloadPolicy, RationalRate, RephasePolicy, ScheduleCursorLifecycle,
    ScheduleDemand, ScheduleDemandCapacity, ScheduleDemandRates, ScheduleEmission,
    ScheduleWorkloadUpperBounds, SchedulerCalibration, TimerPolicy, TimestampMs, canonical_hash,
    plan_deadlines, plan_demanded_deadlines,
};
use serde::Serialize;

#[test]
fn three_millisecond_and_five_hour_deadlines_never_share_a_polling_cadence() {
    let fast_at = timestamp("2026-07-22T12:00:00.003Z");
    let slow_at = timestamp("2026-07-22T17:00:00.000Z");
    let host = host(&[1, 10, 1_000]);
    let grant = grant(1, 8, 100, false);
    let mut plan = plan_deadlines(
        [
            entry('a', 1, fast_at, 1, 2, OverloadPolicy::PauseAndAsk),
            entry('b', 1, slow_at, 1_000, 5_000, OverloadPolicy::PauseAndAsk),
        ],
        &host,
        &grant,
    )
    .unwrap();

    assert_eq!(plan.index.len(), 2);
    assert_eq!(plan.index.lane_count(), 2);
    assert_eq!(plan.index.next_host_wake(), Some(fast_at));
    let fast = plan.index.pop_due(fast_at);
    assert_eq!(fast.due.len(), 1);
    assert_eq!(fast.due[0].entry.activation_hash(), &digest('a'));
    assert_eq!(fast.stale_discarded, 0);
    assert_eq!(plan.index.len(), 1);
    assert_eq!(plan.index.lane_count(), 1);
    assert_eq!(plan.index.next_host_wake(), Some(slow_at));

    let before_slow = plan.index.pop_due(timestamp("2026-07-22T12:00:00.006Z"));
    assert!(before_slow.due.is_empty());
    assert_eq!(before_slow.stale_discarded, 0);
    assert_eq!(plan.index.next_host_wake(), Some(slow_at));
}

#[test]
fn replacement_is_lazy_fenced_and_destroys_empty_resolution_lanes() {
    let first_at = timestamp("2026-07-22T12:00:00.003Z");
    let replacement_at = timestamp("2026-07-22T17:00:00.000Z");
    let mut index = DeadlineIndex::default();
    index.upsert(armed(entry(
        'c',
        1,
        first_at,
        1,
        2,
        OverloadPolicy::PauseAndAsk,
    )));
    assert_eq!(index.lane_count(), 1);
    index.upsert(ArmedDeadline {
        entry: entry(
            'c',
            2,
            replacement_at,
            1_000,
            2_000,
            OverloadPolicy::PauseAndAsk,
        ),
        lane_resolution_ms: nonzero(1_000),
        degraded: false,
    });
    assert_eq!(index.len(), 1);
    assert_eq!(index.lane_count(), 1);
    assert_eq!(index.next_host_wake(), Some(replacement_at));
    let due = index.pop_due(replacement_at);
    assert_eq!(due.due.len(), 1);
    assert_eq!(due.due[0].entry.cursor_revision(), 2);
    assert!(index.is_empty());
    assert_eq!(index.lane_count(), 0);
}

#[test]
fn host_and_grant_admission_apply_each_explicit_overload_policy() {
    let host = host(&[10]);
    let grant = grant(10, 1, 10, true);
    let now = timestamp("2026-07-22T12:00:00.003Z");

    let pause = plan_deadlines(
        [entry('d', 1, now, 1, 0, OverloadPolicy::PauseAndAsk)],
        &host,
        &grant,
    )
    .unwrap();
    assert!(matches!(
        &pause.admissions[0],
        DeadlineAdmission::Paused {
            reason: DeadlineRejectionReason::UnsupportedResolution,
            ..
        }
    ));
    assert!(pause.index.is_empty());

    let rejected = plan_deadlines(
        [entry('e', 1, now, 1, 0, OverloadPolicy::RejectActivation)],
        &host,
        &grant,
    )
    .unwrap_err();
    assert_eq!(
        rejected.reason,
        DeadlineRejectionReason::UnsupportedResolution
    );

    let degraded = plan_deadlines(
        [entry('f', 1, now, 1, 9, OverloadPolicy::DegradeWithinGrant)],
        &host,
        &grant,
    )
    .unwrap();
    assert!(matches!(
        &degraded.admissions[0],
        DeadlineAdmission::Admitted {
            lane_resolution_ms,
            degraded: true,
            ..
        } if lane_resolution_ms.get() == 10
    ));
}

#[test]
fn entry_and_lane_limits_are_deterministic_in_deadline_order() {
    let host = host(&[1, 10]);
    let lane_limited = grant(1, 1, 10, false);
    let first = timestamp("2026-07-22T12:00:00.003Z");
    let second = timestamp("2026-07-22T12:00:00.004Z");
    let mut plan = plan_deadlines(
        [
            entry('1', 1, second, 1, 10, OverloadPolicy::PauseAndAsk),
            entry('2', 1, first, 10, 10, OverloadPolicy::PauseAndAsk),
        ],
        &host,
        &lane_limited,
    )
    .unwrap();
    assert_eq!(plan.index.len(), 1);
    assert_eq!(plan.index.next_host_wake(), Some(first));
    assert!(matches!(
        &plan.admissions[1],
        DeadlineAdmission::Paused {
            activation_hash,
            reason: DeadlineRejectionReason::LaneLimit,
        } if activation_hash == &digest('1')
    ));
}

#[test]
fn deadline_wire_rejects_zero_revision_and_has_a_golden_vocabulary() {
    let deadline = entry(
        '9',
        7,
        timestamp("2026-07-22T12:00:00.003Z"),
        1,
        5,
        OverloadPolicy::DegradeWithinGrant,
    );
    let mut hostile = serde_json::to_value(&deadline).unwrap();
    hostile["cursor_revision"] = serde_json::json!(0);
    assert!(serde_json::from_value::<DeadlineEntry>(hostile).is_err());

    let fixture = DispatcherVocabulary {
        cursor_lifecycles: vec![
            ScheduleCursorLifecycle::Armed,
            ScheduleCursorLifecycle::Leased,
            ScheduleCursorLifecycle::Paused,
            ScheduleCursorLifecycle::Superseded,
            ScheduleCursorLifecycle::Failed,
        ],
        occurrence_schemas: vec![ElapsedScheduleOccurrenceSchema::V1],
        overloads: vec![
            OverloadPolicy::PauseAndAsk,
            OverloadPolicy::RejectActivation,
            OverloadPolicy::DegradeWithinGrant,
        ],
        reasons: vec![
            DeadlineRejectionReason::DuplicateActivation,
            DeadlineRejectionReason::UnsupportedResolution,
            DeadlineRejectionReason::EntryLimit,
            DeadlineRejectionReason::LaneLimit,
            DeadlineRejectionReason::DegradationExceedsLateness,
            DeadlineRejectionReason::SemanticRateLimit,
            DeadlineRejectionReason::WakeRateLimit,
            DeadlineRejectionReason::SchedulerCpuLimit,
            DeadlineRejectionReason::EvaluatorFuelLimit,
            DeadlineRejectionReason::WriteRateLimit,
            DeadlineRejectionReason::EffectRateLimit,
            DeadlineRejectionReason::TraceRateLimit,
            DeadlineRejectionReason::DemandArithmeticOverflow,
        ],
        deadline,
    };
    assert_eq!(
        canonical_hash("karma.dispatcher-vocabulary.v1", &fixture)
            .unwrap()
            .as_str(),
        "sha256:29d15cad3e6e6f380cd73dc0d52d13fed1f389350f3ebbf8b839b3983458d40e"
    );
}

#[test]
fn exact_schedule_demand_aggregates_three_milliseconds_and_five_hours_without_rounding() {
    let workload = ScheduleWorkloadUpperBounds::new(10, nonzero(1), 2, 100);
    let calibration = SchedulerCalibration::new(nonzero(100));
    let fast = ScheduleDemand::for_elapsed(&elapsed(3), workload, calibration).unwrap();
    let slow = ScheduleDemand::for_elapsed(&elapsed(18_000_000), workload, calibration).unwrap();
    assert_eq!(
        fast.semantic_ticks_per_second(),
        RationalRate::new(1_000, 3).unwrap()
    );
    assert_eq!(
        slow.semantic_ticks_per_second(),
        RationalRate::new(1, 18_000).unwrap()
    );

    let total = ScheduleDemandRates::zero()
        .checked_add(fast)
        .unwrap()
        .checked_add(slow)
        .unwrap();
    assert_eq!(
        total.semantic_ticks_per_second,
        RationalRate::new(6_000_001, 18_000).unwrap()
    );
    assert_eq!(
        total.scheduler_cpu_ns_per_second,
        RationalRate::new(600_000_100, 18_000).unwrap()
    );
    let exact_capacity = ScheduleDemandCapacity {
        semantic_ticks_per_second: total.semantic_ticks_per_second,
        timer_wakes_per_second: total.timer_wakes_per_second,
        scheduler_cpu_ns_per_second: total.scheduler_cpu_ns_per_second,
        evaluator_fuel_per_second: total.evaluator_fuel_per_second,
        writes_per_second: total.writes_per_second,
        effects_per_second: total.effects_per_second,
        trace_bytes_per_second: total.trace_bytes_per_second,
    };
    assert!(total.exceeded_resources(exact_capacity).is_empty());
    let wake_limited = ScheduleDemandCapacity {
        timer_wakes_per_second: fast.timer_wakes_per_second(),
        ..exact_capacity
    };
    assert_eq!(
        total.exceeded_resources(wake_limited),
        vec![DemandResource::TimerWakes]
    );
}

#[test]
fn demanded_admission_denies_dense_work_by_exact_capacity_without_interval_classes() {
    let workload = ScheduleWorkloadUpperBounds::new(0, nonzero(1), 0, 0);
    let calibration = SchedulerCalibration::new(nonzero(1));
    let sparse_demand =
        ScheduleDemand::for_elapsed(&elapsed(18_000_000), workload, calibration).unwrap();
    let dense_demand = ScheduleDemand::for_elapsed(&elapsed(3), workload, calibration).unwrap();
    let sparse_entry = entry(
        '7',
        1,
        timestamp("2026-07-22T12:00:00.001Z"),
        1,
        5,
        OverloadPolicy::PauseAndAsk,
    );
    let dense_entry = entry(
        '8',
        1,
        timestamp("2026-07-22T12:00:00.002Z"),
        1,
        5,
        OverloadPolicy::PauseAndAsk,
    );
    let generous = RationalRate::new(u64::MAX, 1).unwrap();
    let capacity = ScheduleDemandCapacity {
        semantic_ticks_per_second: generous,
        timer_wakes_per_second: RationalRate::new(1, 1).unwrap(),
        scheduler_cpu_ns_per_second: generous,
        evaluator_fuel_per_second: generous,
        writes_per_second: generous,
        effects_per_second: generous,
        trace_bytes_per_second: generous,
    };
    let mut plan = plan_demanded_deadlines(
        [
            DemandedDeadline::new(dense_entry, dense_demand).unwrap(),
            DemandedDeadline::new(sparse_entry, sparse_demand).unwrap(),
        ],
        &host(&[1]),
        &grant(1, 2, 10, false),
        capacity,
    )
    .unwrap();
    assert_eq!(plan.index.len(), 1);
    assert_eq!(
        plan.index.next_host_wake(),
        Some(timestamp("2026-07-22T12:00:00.001Z"))
    );
    assert!(matches!(
        &plan.admissions[1],
        DeadlineAdmission::Paused {
            activation_hash,
            reason: DeadlineRejectionReason::WakeRateLimit,
        } if activation_hash == &digest('8')
    ));
    assert_eq!(
        plan.admitted_demand.timer_wakes_per_second,
        RationalRate::new(1, 18_000).unwrap()
    );
}

#[test]
fn schedule_demand_revalidates_wire_data_and_fails_closed_on_rate_overflow() {
    let demand = ScheduleDemand::for_elapsed(
        &elapsed(3),
        ScheduleWorkloadUpperBounds::new(10, nonzero(1), 2, 100),
        SchedulerCalibration::new(nonzero(100)),
    )
    .unwrap();
    let mut hostile = serde_json::to_value(demand).unwrap();
    hostile["timer_wakes_per_second"] = serde_json::json!({
        "numerator": 1001,
        "denominator": 3
    });
    assert!(serde_json::from_value::<ScheduleDemand>(hostile).is_err());
    assert!(
        ScheduleDemand::for_elapsed(
            &elapsed(1),
            ScheduleWorkloadUpperBounds::new(0, nonzero(1), 0, u64::MAX),
            SchedulerCalibration::new(nonzero(1)),
        )
        .is_err()
    );
    assert_eq!(
        canonical_hash("karma.schedule-demand.v1", &demand)
            .unwrap()
            .as_str(),
        "sha256:5c300c4d6033b6ad80facaec3229af038c9edaf71100f948d94e73bdcdaf5a4c"
    );
}

#[test]
fn occurrence_batches_reconstruct_stable_ticks_in_bounded_pages() {
    let schedule = elapsed(3);
    let activation = digest('6');
    let full = OccurrenceBatch::new(
        activation.clone(),
        9,
        &schedule,
        &ScheduleEmission::Individual(
            OccurrenceRange::new(timestamp("2026-07-22T12:00:00.003Z"), 3, 5).unwrap(),
        ),
    )
    .unwrap();
    assert_eq!(full.first_schedule_ordinal, 1);
    assert_eq!(full.covered_boundary_count(), 5);
    assert_eq!(full.semantic_occurrence_count(), 5);
    let page = full.individual_page(2, nonzero(2)).unwrap();
    assert_eq!(page.len(), 2);
    assert_eq!(page[0].schedule_ordinal, 3);
    assert_eq!(page[0].intended_at, timestamp("2026-07-22T12:00:00.009Z"));
    assert!(
        full.individual_page(
            0,
            NonZeroU32::new(MAX_OCCURRENCE_BATCH_PAGE_TICKS + 1).unwrap()
        )
        .is_err()
    );

    let later_segment = OccurrenceBatch::new(
        activation.clone(),
        10,
        &schedule,
        &ScheduleEmission::Individual(
            OccurrenceRange::new(timestamp("2026-07-22T12:00:00.009Z"), 3, 3).unwrap(),
        ),
    )
    .unwrap();
    assert_eq!(
        full.individual_tick(3).unwrap().tick_hash().unwrap(),
        later_segment
            .individual_tick(1)
            .unwrap()
            .tick_hash()
            .unwrap(),
        "host wake segmentation must not change semantic tick identity"
    );

    let coalesced = OccurrenceBatch::new(
        activation,
        11,
        &schedule,
        &ScheduleEmission::Coalesced(
            OccurrenceRange::new(timestamp("2026-07-22T12:00:00.003Z"), 3, 5).unwrap(),
        ),
    )
    .unwrap();
    assert_eq!(coalesced.emission, OccurrenceBatchEmission::Coalesced);
    assert_eq!(coalesced.covered_boundary_count(), 5);
    assert_eq!(coalesced.semantic_occurrence_count(), 1);
    assert!(coalesced.individual_tick(0).is_err());

    let mut hostile = serde_json::to_value(&full).unwrap();
    hostile["first_schedule_ordinal"] = serde_json::json!(u64::MAX);
    assert!(serde_json::from_value::<OccurrenceBatch>(hostile).is_err());
}

#[derive(Serialize)]
struct DispatcherVocabulary {
    cursor_lifecycles: Vec<ScheduleCursorLifecycle>,
    occurrence_schemas: Vec<ElapsedScheduleOccurrenceSchema>,
    overloads: Vec<OverloadPolicy>,
    reasons: Vec<DeadlineRejectionReason>,
    deadline: DeadlineEntry,
}

fn entry(
    digit: char,
    cursor_revision: u64,
    at: TimestampMs,
    required_resolution_ms: u32,
    max_lateness_ms: u32,
    overload: OverloadPolicy,
) -> DeadlineEntry {
    DeadlineEntry::new(
        digest(digit),
        cursor_revision,
        at,
        TimerPolicy::new(required_resolution_ms, max_lateness_ms, 0).unwrap(),
        overload,
    )
    .unwrap()
}

fn armed(entry: DeadlineEntry) -> ArmedDeadline {
    ArmedDeadline {
        lane_resolution_ms: entry.required_resolution_ms(),
        entry,
        degraded: false,
    }
}

fn host(resolutions: &[u32]) -> HostTimerCapabilities {
    HostTimerCapabilities::new(
        resolutions.iter().copied().map(nonzero),
        NonZeroUsize::new(100).unwrap(),
    )
    .unwrap()
}

fn grant(
    finest_resolution_ms: u32,
    maximum_lanes: usize,
    maximum_entries: usize,
    allow_bounded_degradation: bool,
) -> DispatcherResourceGrant {
    DispatcherResourceGrant::new(
        nonzero(finest_resolution_ms),
        NonZeroUsize::new(maximum_lanes).unwrap(),
        NonZeroUsize::new(maximum_entries).unwrap(),
        allow_bounded_degradation,
    )
}

fn timestamp(value: &str) -> TimestampMs {
    TimestampMs::parse_canonical(value).unwrap()
}

fn digest(digit: char) -> CanonicalHash {
    CanonicalHash::parse(format!("sha256:{}", digit.to_string().repeat(64))).unwrap()
}

fn nonzero(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).unwrap()
}

fn elapsed(interval_ms: u64) -> ElapsedSchedule {
    ElapsedSchedule::new(
        timestamp("2026-07-22T12:00:00.000Z"),
        interval_ms,
        TimerPolicy::new(1, 5, 0).unwrap(),
        MissedPolicy::Replay { max: nonzero(64) },
        InactiveGapPolicy::SkipToNextAnchor,
        RephasePolicy::PreserveAnchor,
        OverloadPolicy::PauseAndAsk,
    )
    .unwrap()
}
