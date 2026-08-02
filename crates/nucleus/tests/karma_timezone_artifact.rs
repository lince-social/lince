use std::collections::BTreeMap;

use nucleus::karma::{
    ArtifactTimeZoneProvider, Cadence, CalendarSchedule, CanonicalHash, CivilDateTime, FoldPolicy,
    GapPolicy, InactiveGapPolicy, LocalTimeResolution, MissedPolicy, OverloadPolicy, RephasePolicy,
    TimeZoneArtifact, TimeZoneDefinition, TimeZoneId, TimeZoneProvider, TimerPolicy, TimestampMs,
    TzdbRevision, TzdbVersion, UtcOffsetSegment,
};

#[test]
fn canonical_artifact_resolves_unique_gap_and_fold_without_host_timezone_state() {
    let artifact = fixture_artifact();
    let bytes = artifact.canonical_bytes().unwrap();
    let provider = ArtifactTimeZoneProvider::from_canonical_bytes(&bytes).unwrap();
    assert_eq!(provider.revision(), &artifact.revision().unwrap());
    assert_eq!(
        provider
            .resolve_local(&timezone(), civil("2026-07-01T12:00:00.000"),)
            .unwrap(),
        LocalTimeResolution::Unique {
            instant: timestamp("2026-07-01T14:00:00.000Z")
        }
    );
    assert_eq!(
        provider
            .resolve_local(&timezone(), civil("2026-03-08T00:30:00.000"),)
            .unwrap(),
        LocalTimeResolution::Gap {
            before: timestamp("2026-03-08T02:59:59.999Z"),
            first_valid_after: timestamp("2026-03-08T03:00:00.000Z"),
        }
    );
    assert_eq!(
        provider
            .resolve_local(&timezone(), civil("2026-11-01T00:30:00.000"),)
            .unwrap(),
        LocalTimeResolution::Fold {
            first: timestamp("2026-11-01T02:30:00.000Z"),
            second: timestamp("2026-11-01T03:30:00.000Z"),
        }
    );

    let mut noncanonical = b" ".to_vec();
    noncanonical.extend(bytes);
    assert!(ArtifactTimeZoneProvider::from_canonical_bytes(&noncanonical).is_err());
}

#[test]
fn artifact_revision_and_schedule_demand_attestation_fail_closed() {
    let artifact = fixture_artifact();
    let bytes = artifact.canonical_bytes().unwrap();
    let provider = ArtifactTimeZoneProvider::from_canonical_bytes(&bytes).unwrap();
    let schedule = daily_schedule(provider.revision().clone(), FoldPolicy::First);
    assert_eq!(
        provider.minimum_interval_ms(&schedule).unwrap().get(),
        23 * 60 * 60 * 1_000
    );
    let fold_both = daily_schedule(provider.revision().clone(), FoldPolicy::Both);
    assert_eq!(
        provider.minimum_interval_ms(&fold_both).unwrap().get(),
        60 * 60 * 1_000
    );

    let wrong = TzdbRevision {
        version: TzdbVersion::new("2026a+lince.1").unwrap(),
        digest: CanonicalHash::parse(format!("sha256:{}", "9".repeat(64))).unwrap(),
    };
    assert!(ArtifactTimeZoneProvider::from_canonical_bytes_expected(&bytes, &wrong).is_err());
    let mut wrong_schedule = schedule;
    wrong_schedule.tzdb = wrong;
    assert!(provider.minimum_interval_ms(&wrong_schedule).is_err());
}

#[test]
fn artifact_rejects_noncontiguous_utc_and_triple_local_mappings() {
    let gap = TimeZoneDefinition::new(vec![
        segment(None, Some("2026-03-08T03:00:00.000Z"), -10_800),
        segment(Some("2026-03-08T04:00:00.000Z"), None, -7_200),
    ]);
    assert!(gap.is_err());

    let triple = TimeZoneDefinition::new(vec![
        segment(None, Some("2026-01-01T03:00:00.000Z"), 7_200),
        segment(
            Some("2026-01-01T03:00:00.000Z"),
            Some("2026-01-01T03:30:00.000Z"),
            0,
        ),
        segment(Some("2026-01-01T03:30:00.000Z"), None, -7_200),
    ]);
    assert!(triple.is_err());
}

fn fixture_artifact() -> TimeZoneArtifact {
    TimeZoneArtifact::new(
        TzdbVersion::new("2026a+lince.1").unwrap(),
        BTreeMap::from([(
            timezone(),
            TimeZoneDefinition::new(vec![
                segment(None, Some("2026-03-08T03:00:00.000Z"), -10_800),
                segment(
                    Some("2026-03-08T03:00:00.000Z"),
                    Some("2026-11-01T03:00:00.000Z"),
                    -7_200,
                ),
                segment(Some("2026-11-01T03:00:00.000Z"), None, -10_800),
            ])
            .unwrap(),
        )]),
    )
    .unwrap()
}

fn daily_schedule(tzdb: TzdbRevision, fold: FoldPolicy) -> CalendarSchedule {
    CalendarSchedule {
        anchor: civil("2026-01-01T08:00:00.000"),
        cadence: Cadence::every_days(1),
        timezone: timezone(),
        tzdb,
        gap: GapPolicy::ShiftForward,
        fold,
        timer: TimerPolicy::new(1, 100, 0).unwrap(),
        missed: MissedPolicy::Replay {
            max: std::num::NonZeroU32::new(16).unwrap(),
        },
        inactive_gap: InactiveGapPolicy::SkipToNextAnchor,
        rephase: RephasePolicy::PreserveAnchor,
        overload: OverloadPolicy::PauseAndAsk,
    }
}

fn segment(start: Option<&str>, end: Option<&str>, offset_seconds: i32) -> UtcOffsetSegment {
    UtcOffsetSegment::new(start.map(timestamp), end.map(timestamp), offset_seconds).unwrap()
}

fn timezone() -> TimeZoneId {
    TimeZoneId::new("America/Test_City").unwrap()
}

fn timestamp(value: &str) -> TimestampMs {
    TimestampMs::parse_canonical(value).unwrap()
}

fn civil(value: &str) -> CivilDateTime {
    CivilDateTime::parse_canonical(value).unwrap()
}
