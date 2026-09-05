use std::collections::BTreeMap;

use nucleus::karma::{
    Cadence, CadenceBound, CalendarAdvance, CalendarBoundary, CalendarBoundaryKind,
    CalendarDiscontinuity, CalendarSchedule, CanonicalHash, CivilDateTime, CivilTime, CivilWeekday,
    DayOfMonth, FoldPolicy, GapPolicy, InactiveGapPolicy, InvalidDay, KarmaBoundaryError,
    LocalTimeResolution, MissedPolicy, OverloadPolicy, RephasePolicy, TimeZoneId, TimeZoneProvider,
    TimerPolicy, TimestampMs, TzdbRevision, TzdbVersion, WeekdaySet, canonical_hash,
};

#[derive(Debug)]
struct FakeTimeZoneProvider {
    revision: TzdbRevision,
    values: BTreeMap<(TimeZoneId, CivilDateTime), LocalTimeResolution>,
}

impl FakeTimeZoneProvider {
    fn new(revision: TzdbRevision) -> Self {
        Self {
            revision,
            values: BTreeMap::new(),
        }
    }

    fn with(
        mut self,
        timezone: &TimeZoneId,
        local: CivilDateTime,
        resolution: LocalTimeResolution,
    ) -> Self {
        self.values.insert((timezone.clone(), local), resolution);
        self
    }
}

impl TimeZoneProvider for FakeTimeZoneProvider {
    fn revision(&self) -> &TzdbRevision {
        &self.revision
    }

    fn resolve_local(
        &self,
        timezone: &TimeZoneId,
        local: CivilDateTime,
    ) -> Result<LocalTimeResolution, KarmaBoundaryError> {
        self.values
            .get(&(timezone.clone(), local))
            .copied()
            .ok_or_else(|| KarmaBoundaryError::invalid_input("fake provider has no local value"))
    }
}

fn civil(value: &str) -> CivilDateTime {
    CivilDateTime::parse_canonical(value).unwrap()
}

fn time(value: &str) -> CivilTime {
    CivilTime::parse_canonical(value).unwrap()
}

fn instant(value: &str) -> TimestampMs {
    TimestampMs::parse_canonical(value).unwrap()
}

fn timezone() -> TimeZoneId {
    TimeZoneId::new("America/Sao_Paulo").unwrap()
}

fn revision(version: &str, digit: char) -> TzdbRevision {
    TzdbRevision {
        version: TzdbVersion::new(version).unwrap(),
        digest: CanonicalHash::parse(format!("sha256:{}", digit.to_string().repeat(64))).unwrap(),
    }
}

fn schedule(
    anchor: CivilDateTime,
    cadence: Cadence,
    tzdb: TzdbRevision,
    gap: GapPolicy,
    fold: FoldPolicy,
) -> CalendarSchedule {
    CalendarSchedule {
        anchor,
        cadence,
        timezone: timezone(),
        tzdb,
        gap,
        fold,
        timer: TimerPolicy::new(1, 20, 0).unwrap(),
        missed: MissedPolicy::Coalesce,
        inactive_gap: InactiveGapPolicy::SkipToNextAnchor,
        rephase: RephasePolicy::PreserveAnchor,
        overload: OverloadPolicy::PauseAndAsk,
    }
}

#[test]
fn civil_atoms_and_timezone_identifiers_are_canonical() {
    let value = civil("2026-03-29T02:30:00.125");
    assert_eq!(value.to_string(), "2026-03-29T02:30:00.125");
    assert!(CivilDateTime::parse_canonical("2026-03-29 02:30:00.125").is_err());
    assert!(CivilDateTime::parse_canonical("2026-03-29T02:30:00").is_err());
    assert_eq!(time("08:05:03.009").to_string(), "08:05:03.009");
    assert!(CivilTime::parse_canonical("8:05:03.009").is_err());
    assert!(TimeZoneId::new("America/Sao_Paulo").is_ok());
    assert!(TimeZoneId::new("../localtime").is_err());
    assert!(TimeZoneId::new("America//Sao_Paulo").is_err());
    assert!(TzdbVersion::new("2026b+lince.1").is_ok());
    assert!(DayOfMonth::new(0).is_err());
    assert!(DayOfMonth::new(32).is_err());
    assert!(WeekdaySet::new([]).is_err());
}

#[test]
fn daily_gap_skip_records_the_gap_and_jumps_to_the_next_natural_boundary() {
    let tzdb = revision("2026a", '1');
    let zone = timezone();
    let first_local = civil("2026-03-28T02:30:00.000");
    let gap_local = civil("2026-03-29T02:30:00.000");
    let next_local = civil("2026-03-30T02:30:00.000");
    let provider = FakeTimeZoneProvider::new(tzdb.clone())
        .with(
            &zone,
            first_local,
            LocalTimeResolution::Unique {
                instant: instant("2026-03-28T01:30:00.000Z"),
            },
        )
        .with(
            &zone,
            gap_local,
            LocalTimeResolution::Gap {
                before: instant("2026-03-29T00:59:59.999Z"),
                first_valid_after: instant("2026-03-29T01:00:00.000Z"),
            },
        )
        .with(
            &zone,
            next_local,
            LocalTimeResolution::Unique {
                instant: instant("2026-03-30T00:30:00.000Z"),
            },
        );
    let schedule = schedule(
        first_local,
        Cadence::every_days(1),
        tzdb,
        GapPolicy::Skip,
        FoldPolicy::First,
    );

    let first = schedule.next_after(&provider, None).unwrap();
    let next = schedule
        .next_after(&provider, first.boundary)
        .expect("gap skip should continue deterministically");

    assert_eq!(next.boundary.unwrap().requested_local, next_local);
    assert_eq!(
        next.skipped,
        vec![CalendarDiscontinuity::Gap {
            local: gap_local,
            before: instant("2026-03-29T00:59:59.999Z"),
            first_valid_after: instant("2026-03-29T01:00:00.000Z"),
        }]
    );
}

#[test]
fn gap_shift_and_pause_are_distinct_and_preserve_requested_local_time() {
    let tzdb = revision("2026a", '2');
    let local = civil("2026-03-29T02:30:00.000");
    let resolution = LocalTimeResolution::Gap {
        before: instant("2026-03-29T00:59:59.999Z"),
        first_valid_after: instant("2026-03-29T01:00:00.000Z"),
    };
    let provider = FakeTimeZoneProvider::new(tzdb.clone()).with(&timezone(), local, resolution);
    let rule = Cadence::every_days(1);

    let shifted = schedule(
        local,
        rule.clone(),
        tzdb.clone(),
        GapPolicy::ShiftForward,
        FoldPolicy::First,
    )
    .next_after(&provider, None)
    .unwrap();
    assert_eq!(
        shifted.boundary.unwrap(),
        nucleus::karma::CalendarBoundary {
            requested_local: local,
            intended_at: instant("2026-03-29T01:00:00.000Z"),
            kind: CalendarBoundaryKind::GapShiftedForward,
        }
    );

    let paused = schedule(local, rule, tzdb, GapPolicy::Pause, FoldPolicy::First)
        .next_after(&provider, None)
        .unwrap();
    assert!(paused.boundary.is_none());
    assert_eq!(
        paused.pause,
        Some(CalendarDiscontinuity::Gap {
            local,
            before: instant("2026-03-29T00:59:59.999Z"),
            first_valid_after: instant("2026-03-29T01:00:00.000Z"),
        })
    );
}

#[test]
fn fold_both_returns_two_ordered_boundaries_for_one_local_time() {
    let tzdb = revision("2026b", '3');
    let local = civil("2026-10-25T02:30:00.000");
    let next_local = civil("2026-10-26T02:30:00.000");
    let provider = FakeTimeZoneProvider::new(tzdb.clone())
        .with(
            &timezone(),
            local,
            LocalTimeResolution::Fold {
                first: instant("2026-10-25T00:30:00.000Z"),
                second: instant("2026-10-25T01:30:00.000Z"),
            },
        )
        .with(
            &timezone(),
            next_local,
            LocalTimeResolution::Unique {
                instant: instant("2026-10-26T01:30:00.000Z"),
            },
        );
    let schedule = schedule(
        local,
        Cadence::every_days(1),
        tzdb,
        GapPolicy::Skip,
        FoldPolicy::Both,
    );

    let first = schedule
        .next_after(&provider, None)
        .unwrap()
        .boundary
        .unwrap();
    let second = schedule
        .next_after(&provider, Some(first))
        .unwrap()
        .boundary
        .unwrap();
    let third = schedule
        .next_after(&provider, Some(second))
        .unwrap()
        .boundary
        .unwrap();

    assert_eq!(first.kind, CalendarBoundaryKind::FoldFirst);
    assert_eq!(second.kind, CalendarBoundaryKind::FoldSecond);
    assert_eq!(first.requested_local, second.requested_local);
    assert!(first.intended_at < second.intended_at);
    assert_eq!(third.requested_local, next_local);
}

#[test]
fn a_weekday_set_is_a_landing_rule_not_a_shape_of_its_own() {
    let tzdb = revision("2026c", '4');
    let monday = civil("2026-07-06T08:00:00.000");
    let wednesday = civil("2026-07-08T08:00:00.000");
    let next_monday = civil("2026-07-13T08:00:00.000");
    let provider = FakeTimeZoneProvider::new(tzdb.clone())
        .with(
            &timezone(),
            monday,
            LocalTimeResolution::Unique {
                instant: instant("2026-07-06T11:00:00.000Z"),
            },
        )
        .with(
            &timezone(),
            wednesday,
            LocalTimeResolution::Unique {
                instant: instant("2026-07-08T11:00:00.000Z"),
            },
        )
        .with(
            &timezone(),
            next_monday,
            LocalTimeResolution::Unique {
                instant: instant("2026-07-13T11:00:00.000Z"),
            },
        );
    let schedule = schedule(
        monday,
        Cadence::every_days(1)
            .landing_on(WeekdaySet::new([CivilWeekday::Wednesday, CivilWeekday::Monday]).unwrap()),
        tzdb,
        GapPolicy::Skip,
        FoldPolicy::First,
    );

    let first = schedule
        .next_after(&provider, None)
        .unwrap()
        .boundary
        .unwrap();
    let second = schedule
        .next_after(&provider, Some(first))
        .unwrap()
        .boundary
        .unwrap();
    let third = schedule
        .next_after(&provider, Some(second))
        .unwrap()
        .boundary
        .unwrap();
    assert_eq!(first.requested_local, monday);
    assert_eq!(second.requested_local, wednesday);
    assert_eq!(third.requested_local, next_monday);
}

#[test]
fn monthly_invalid_dates_are_skipped_clamped_or_paused_explicitly() {
    let tzdb = revision("2026d", '5');
    let january = civil("2026-01-31T08:00:00.000");
    let march = civil("2026-03-31T08:00:00.000");
    let february_last = civil("2026-02-28T08:00:00.000");
    let provider = FakeTimeZoneProvider::new(tzdb.clone())
        .with(
            &timezone(),
            january,
            LocalTimeResolution::Unique {
                instant: instant("2026-01-31T11:00:00.000Z"),
            },
        )
        .with(
            &timezone(),
            march,
            LocalTimeResolution::Unique {
                instant: instant("2026-03-31T11:00:00.000Z"),
            },
        )
        .with(
            &timezone(),
            february_last,
            LocalTimeResolution::Unique {
                instant: instant("2026-02-28T11:00:00.000Z"),
            },
        );
    let monthly = |policy| Cadence::every_months(1).with_invalid_day(policy);

    let skipped_schedule = schedule(
        january,
        monthly(InvalidDay::Skip),
        tzdb.clone(),
        GapPolicy::Skip,
        FoldPolicy::First,
    );
    let january_boundary = skipped_schedule
        .next_after(&provider, None)
        .unwrap()
        .boundary
        .unwrap();
    let skipped = skipped_schedule
        .next_after(&provider, Some(january_boundary))
        .unwrap();
    assert_eq!(skipped.boundary.unwrap().requested_local, march);
    assert_eq!(
        skipped.skipped,
        vec![CalendarDiscontinuity::InvalidDayOfMonth {
            year: 2026,
            month: 2,
            day: DayOfMonth::new(31).unwrap(),
        }]
    );

    let clamped_schedule = schedule(
        january,
        monthly(InvalidDay::Clamp),
        tzdb.clone(),
        GapPolicy::Skip,
        FoldPolicy::First,
    );
    let january_boundary = clamped_schedule
        .next_after(&provider, None)
        .unwrap()
        .boundary
        .unwrap();
    assert_eq!(
        clamped_schedule
            .next_after(&provider, Some(january_boundary))
            .unwrap()
            .boundary
            .unwrap()
            .requested_local,
        february_last
    );

    let paused_schedule = schedule(
        january,
        monthly(InvalidDay::Pause),
        tzdb,
        GapPolicy::Skip,
        FoldPolicy::First,
    );
    let january_boundary = paused_schedule
        .next_after(&provider, None)
        .unwrap()
        .boundary
        .unwrap();
    let paused = paused_schedule
        .next_after(&provider, Some(january_boundary))
        .unwrap();
    assert!(paused.boundary.is_none());
    assert!(matches!(
        paused.pause,
        Some(CalendarDiscontinuity::InvalidDayOfMonth {
            year: 2026,
            month: 2,
            ..
        })
    ));
}

#[test]
fn provider_revision_and_resolution_order_fail_closed() {
    let local = civil("2026-10-25T02:30:00.000");
    let schedule_revision = revision("2026e", '6');
    let mismatch = FakeTimeZoneProvider::new(revision("2026f", '7'));
    let schedule = schedule(
        local,
        Cadence::every_days(1),
        schedule_revision.clone(),
        GapPolicy::Skip,
        FoldPolicy::Both,
    );
    assert!(schedule.next_after(&mismatch, None).is_err());

    let broken = FakeTimeZoneProvider::new(schedule_revision).with(
        &timezone(),
        local,
        LocalTimeResolution::Fold {
            first: instant("2026-10-25T01:30:00.000Z"),
            second: instant("2026-10-25T00:30:00.000Z"),
        },
    );
    assert!(schedule.next_after(&broken, None).is_err());

    let good = FakeTimeZoneProvider::new(schedule.tzdb.clone()).with(
        &timezone(),
        local,
        LocalTimeResolution::Fold {
            first: instant("2026-10-25T00:30:00.000Z"),
            second: instant("2026-10-25T01:30:00.000Z"),
        },
    );
    let forged = CalendarBoundary {
        requested_local: local,
        intended_at: instant("2026-10-25T01:30:00.000Z"),
        kind: CalendarBoundaryKind::FoldFirst,
    };
    assert!(schedule.next_after(&good, Some(forged)).is_err());
}

#[test]
fn calendar_wire_vocabulary_has_a_golden_hash() {
    let local = civil("2026-01-31T08:00:00.000");
    let first = instant("2026-01-31T10:00:00.000Z");
    let second = instant("2026-01-31T11:00:00.000Z");
    let day = DayOfMonth::new(31).unwrap();
    let weekdays = WeekdaySet::new([
        CivilWeekday::Monday,
        CivilWeekday::Tuesday,
        CivilWeekday::Wednesday,
        CivilWeekday::Thursday,
        CivilWeekday::Friday,
        CivilWeekday::Saturday,
        CivilWeekday::Sunday,
    ])
    .unwrap();
    let rules = vec![
        Cadence::every_days(1),
        Cadence::every_weeks(2).landing_on(weekdays.clone()),
        Cadence::every_months(1),
        Cadence {
            every: nucleus::karma::CadenceStep {
                months: 1,
                days: 1,
                seconds: 1,
                milliseconds: 10,
                ..Default::default()
            },
            land_on: Some(weekdays.clone()),
            invalid_day: InvalidDay::Skip,
            bound: CadenceBound::Count { occurrences: 12 },
        },
    ];
    let resolutions = [
        LocalTimeResolution::Unique { instant: first },
        LocalTimeResolution::Gap {
            before: first,
            first_valid_after: second,
        },
        LocalTimeResolution::Fold { first, second },
    ];
    let discontinuities = vec![
        CalendarDiscontinuity::Gap {
            local,
            before: first,
            first_valid_after: second,
        },
        CalendarDiscontinuity::Fold {
            local,
            first,
            second,
        },
        CalendarDiscontinuity::FoldFirst {
            local,
            instant: first,
        },
        CalendarDiscontinuity::FoldSecond {
            local,
            instant: second,
        },
        CalendarDiscontinuity::InvalidDayOfMonth {
            year: 2026,
            month: 2,
            day,
        },
    ];
    let boundaries = [
        CalendarBoundary {
            requested_local: local,
            intended_at: first,
            kind: CalendarBoundaryKind::Unique,
        },
        CalendarBoundary {
            requested_local: local,
            intended_at: second,
            kind: CalendarBoundaryKind::GapShiftedForward,
        },
        CalendarBoundary {
            requested_local: local,
            intended_at: first,
            kind: CalendarBoundaryKind::FoldFirst,
        },
        CalendarBoundary {
            requested_local: local,
            intended_at: second,
            kind: CalendarBoundaryKind::FoldSecond,
        },
    ];
    let fixture = (
        schedule(
            local,
            rules[2].clone(),
            revision("2026a+lince.1", '8'),
            GapPolicy::ShiftForward,
            FoldPolicy::Both,
        ),
        [GapPolicy::Skip, GapPolicy::ShiftForward, GapPolicy::Pause],
        [
            FoldPolicy::First,
            FoldPolicy::Second,
            FoldPolicy::Both,
            FoldPolicy::Pause,
        ],
        [InvalidDay::Skip, InvalidDay::Clamp, InvalidDay::Pause],
        weekdays,
        rules,
        resolutions,
        boundaries,
        discontinuities.clone(),
        [
            CalendarAdvance {
                boundary: Some(boundaries[0]),
                skipped: discontinuities,
                pause: None,
            },
            CalendarAdvance {
                boundary: None,
                skipped: Vec::new(),
                pause: Some(CalendarDiscontinuity::Fold {
                    local,
                    first,
                    second,
                }),
            },
        ],
    );
    assert_eq!(
        canonical_hash("karma.calendar-wire.v1", &fixture)
            .unwrap()
            .as_str(),
        "sha256:badeaff50429925dd3bdf32bd360853aa57ebab0b0aa8f2e59f63110974e5108"
    );
}
