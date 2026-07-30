use std::{collections::BTreeMap, num::NonZeroU32};

use nucleus::karma::{
    Cadence, CalendarCatchUpPause, CalendarCursorResolution, CalendarEmission,
    CalendarSchedule, CalendarScheduleOccurrence, CalendarScheduleOccurrenceSchema, CanonicalHash,
    CivilDateTime, FoldPolicy, GapPolicy, InactiveGapPolicy, LocalTimeResolution,
    MissedPolicy, OverloadPolicy, RephasePolicy, TimeZoneId, TimeZoneProvider, TimerPolicy,
    TimestampMs, TzdbRevision, TzdbVersion, advance_calendar_cursor, canonical_hash,
    resolve_calendar_cursor,
};
use serde::Serialize;

#[derive(Clone)]
struct FakeProvider {
    revision: TzdbRevision,
    resolutions: BTreeMap<CivilDateTime, LocalTimeResolution>,
}

impl TimeZoneProvider for FakeProvider {
    fn revision(&self) -> &TzdbRevision {
        &self.revision
    }

    fn resolve_local(
        &self,
        _timezone: &TimeZoneId,
        local: CivilDateTime,
    ) -> Result<LocalTimeResolution, nucleus::karma::KarmaBoundaryError> {
        self.resolutions.get(&local).copied().ok_or_else(|| {
            nucleus::karma::KarmaBoundaryError::invalid_input(format!(
                "fixture has no resolution for {local}"
            ))
        })
    }
}

#[test]
fn provider_verified_cursor_replays_calendar_boundaries_with_an_explicit_budget() {
    let provider = provider();
    let schedule = schedule(MissedPolicy::Replay { max: nonzero(2) });
    let CalendarCursorResolution::Armed(cursor) =
        resolve_calendar_cursor(&schedule, &provider, None).unwrap()
    else {
        panic!("expected armed calendar cursor");
    };
    assert_eq!(
        cursor.next().requested_local,
        local("2026-01-01T08:00:00.000")
    );
    assert_eq!(
        cursor.next().intended_at,
        instant("2026-01-01T11:00:00.000Z")
    );
    cursor.validate_for(&schedule, &provider).unwrap();

    let caught_up = advance_calendar_cursor(
        &schedule,
        &provider,
        &cursor,
        instant("2026-01-03T11:00:00.000Z"),
        nonzero(4),
    )
    .unwrap()
    .unwrap();
    assert_eq!(caught_up.due.len(), 3);
    assert_eq!(caught_up.replay_overflow, 1);
    assert_eq!(caught_up.skipped_due.len(), 1);
    assert!(matches!(
        caught_up.emission,
        Some(CalendarEmission::Individual(ref values)) if values.len() == 2
    ));
    assert_eq!(
        caught_up.next_cursor.unwrap().next().intended_at,
        instant("2026-01-04T11:00:00.000Z")
    );

    assert!(
        advance_calendar_cursor(
            &schedule,
            &provider,
            &cursor,
            instant("2026-01-03T11:00:00.000Z"),
            nonzero(2),
        )
        .is_err()
    );
}

#[test]
fn skip_pause_on_lag_and_discontinuity_pause_remain_distinct() {
    let provider = provider();
    let skip = schedule(MissedPolicy::Skip);
    let CalendarCursorResolution::Armed(cursor) =
        resolve_calendar_cursor(&skip, &provider, None).unwrap()
    else {
        panic!("expected cursor");
    };
    let skipped = advance_calendar_cursor(
        &skip,
        &provider,
        &cursor,
        instant("2026-01-03T11:00:00.000Z"),
        nonzero(4),
    )
    .unwrap()
    .unwrap();
    assert_eq!(skipped.skipped_due.len(), 2);
    assert!(matches!(
        skipped.emission,
        Some(CalendarEmission::Individual(ref values)) if values.len() == 1
    ));

    let pause_lag = schedule(MissedPolicy::PauseOnLag);
    let CalendarCursorResolution::Armed(cursor) =
        resolve_calendar_cursor(&pause_lag, &provider, None).unwrap()
    else {
        panic!("expected cursor");
    };
    let lagged = advance_calendar_cursor(
        &pause_lag,
        &provider,
        &cursor,
        instant("2026-01-03T11:00:00.000Z"),
        nonzero(4),
    )
    .unwrap()
    .unwrap();
    assert_eq!(lagged.pause, Some(CalendarCatchUpPause::Lag));
    assert_eq!(lagged.next_cursor, Some(cursor));
    assert!(lagged.emission.is_none());

    let mut gap_provider = provider.clone();
    gap_provider.resolutions.insert(
        local("2026-01-01T08:00:00.000"),
        LocalTimeResolution::Gap {
            before: instant("2026-01-01T10:59:59.999Z"),
            first_valid_after: instant("2026-01-01T12:00:00.000Z"),
        },
    );
    let mut gap_schedule = schedule(MissedPolicy::Coalesce);
    gap_schedule.gap = GapPolicy::Pause;
    assert!(matches!(
        resolve_calendar_cursor(&gap_schedule, &gap_provider, None).unwrap(),
        CalendarCursorResolution::Paused { .. }
    ));
}

#[test]
fn calendar_runtime_fixture_has_a_stable_canonical_hash() {
    let provider = provider();
    let schedule = schedule(MissedPolicy::Coalesce);
    let resolution = resolve_calendar_cursor(&schedule, &provider, None).unwrap();
    let CalendarCursorResolution::Armed(cursor) = &resolution else {
        panic!("expected cursor");
    };
    let catch_up = advance_calendar_cursor(
        &schedule,
        &provider,
        cursor,
        instant("2026-01-02T11:00:00.000Z"),
        nonzero(4),
    )
    .unwrap()
    .unwrap();
    let fixture = CalendarRuntimeFixture {
        resolution,
        emissions: vec![
            CalendarEmission::Individual(Vec::new()),
            CalendarEmission::Coalesced(Vec::new()),
        ],
        pauses: vec![
            CalendarCatchUpPause::Lag,
            CalendarCatchUpPause::Discontinuity {
                reason: nucleus::karma::CalendarDiscontinuity::Gap {
                    local: local("2026-01-01T08:00:00.000"),
                    before: instant("2026-01-01T10:59:59.999Z"),
                    first_valid_after: instant("2026-01-01T12:00:00.000Z"),
                },
            },
        ],
        occurrence_schemas: vec![CalendarScheduleOccurrenceSchema::V1],
        occurrence: CalendarScheduleOccurrence::new(
            CanonicalHash::parse(format!("sha256:{}", "7".repeat(64))).unwrap(),
            1,
            1,
            1,
            catch_up.observed_at,
            catch_up.clone(),
        )
        .unwrap(),
        catch_up,
    };
    assert_eq!(
        canonical_hash("karma.calendar-runtime.v1", &fixture)
            .unwrap()
            .as_str(),
        "sha256:d8d9a82ae255c5ad4d32545691a61ad6389e6e85ffc3f7e73c5487db91a271b3"
    );
}

#[derive(Serialize)]
struct CalendarRuntimeFixture {
    resolution: CalendarCursorResolution,
    emissions: Vec<CalendarEmission>,
    pauses: Vec<CalendarCatchUpPause>,
    occurrence_schemas: Vec<CalendarScheduleOccurrenceSchema>,
    occurrence: CalendarScheduleOccurrence,
    catch_up: nucleus::karma::CalendarCatchUp,
}

fn schedule(missed: MissedPolicy) -> CalendarSchedule {
    CalendarSchedule {
        anchor: local("2026-01-01T08:00:00.000"),
        // The anchor already carries 08:00, which is where the time of day now
        // comes from: one statement, not two that could disagree.
        cadence: Cadence::every_days(1),
        timezone: TimeZoneId::new("America/Sao_Paulo").unwrap(),
        tzdb: revision(),
        gap: GapPolicy::ShiftForward,
        fold: FoldPolicy::Both,
        timer: TimerPolicy::new(1, 5, 0).unwrap(),
        missed,
        inactive_gap: InactiveGapPolicy::ReplayByMissedPolicy,
        rephase: RephasePolicy::PreserveAnchor,
        overload: OverloadPolicy::PauseAndAsk,
    }
}

fn provider() -> FakeProvider {
    let resolutions = (1..=5)
        .map(|day| {
            let local = local(&format!("2026-01-{day:02}T08:00:00.000"));
            let instant = instant(&format!("2026-01-{day:02}T11:00:00.000Z"));
            (local, LocalTimeResolution::Unique { instant })
        })
        .collect();
    FakeProvider {
        revision: revision(),
        resolutions,
    }
}

fn revision() -> TzdbRevision {
    TzdbRevision {
        version: TzdbVersion::new("2026a+lince.1").unwrap(),
        digest: CanonicalHash::parse(format!("sha256:{}", "8".repeat(64))).unwrap(),
    }
}

fn local(value: &str) -> CivilDateTime {
    CivilDateTime::parse_canonical(value).unwrap()
}

fn instant(value: &str) -> TimestampMs {
    TimestampMs::parse_canonical(value).unwrap()
}

fn nonzero(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).unwrap()
}
