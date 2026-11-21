use chrono::{DateTime, FixedOffset, NaiveDate, Utc};
use nucleus::schedule::{Endpoints, TimeValue, civil_instants, on_date, range};

#[test]
fn scheduling_duration_and_point_rules() {
    let at = "2026-10-03T11:02:00Z";
    let instant = TimeValue::parse(at).unwrap().instant_ms().unwrap();
    assert_eq!(
        range(Some(at), None, None, None).unwrap().unwrap().until_ms,
        None
    );
    let forward = range(Some(at), None, Some(10.0), None).unwrap().unwrap();
    assert_eq!(forward.from_ms, instant);
    assert_eq!(forward.until_ms, Some(instant + 600_000));
    let backward = range(None, Some(at), Some(10.0), None).unwrap().unwrap();
    assert_eq!(backward.from_ms, instant - 600_000);
    assert_eq!(backward.until_ms, Some(instant));
    let explicit = range(
        Some(at),
        Some("2026-10-03T11:07:00Z"),
        Some(10.0),
        Some(instant + 3_600_000),
    )
    .unwrap()
    .unwrap();
    assert_eq!(explicit.until_ms, Some(instant + 3_900_000));
    assert!(
        range(Some("2026-10-03"), None, Some(10.0), None)
            .unwrap()
            .is_none()
    );
}

#[test]
fn reversed_and_implicit_times_are_rejected() {
    assert!(TimeValue::parse("2026-10-03T11:02:00").is_err());
    assert!(TimeValue::parse("2026-1-03").is_err());
    assert!(
        range(
            Some("2026-10-03T11:02:00Z"),
            Some("2026-10-03T11:01:00Z"),
            None,
            None
        )
        .is_err()
    );
    assert!(range(Some("2026-10-04"), Some("2026-10-03"), None, None).is_err());
    assert!(
        range(
            Some("2026-10-03T11:02:00Z"),
            None,
            Some(f64::INFINITY),
            None
        )
        .is_err()
    );
}

#[test]
fn timezone_gaps_and_repeated_times_are_explicit() {
    assert_eq!(
        civil_instants("2026-10-03", "10:30:12.125", "America/Sao_Paulo").unwrap(),
        vec!["2026-10-03T10:30:12.125-03:00"]
    );
    assert!(civil_instants("2026-03-08", "02:30", "America/New_York").is_err());
    let repeated = civil_instants("2026-11-01", "01:30", "America/New_York").unwrap();
    let [first, second] = repeated.as_slice() else {
        panic!("expected two instants after a clock fold, got {repeated:?}");
    };
    let millis = |value: &str| TimeValue::parse(value).unwrap().instant_ms().unwrap();
    assert_eq!(millis(second) - millis(first), 3_600_000);
}

#[test]
fn window_clipping_never_turns_a_point_into_a_range() {
    let point = range(None, None, None, Some(1000)).unwrap().unwrap();
    assert!(point.clipped(1000, 2000).is_some());
    assert!(point.clipped(0, 1000).is_none());
    assert_eq!(point.clipped(1000, 2000).unwrap().until_ms, None);
    let interval = range(None, None, Some(1.0), Some(1000)).unwrap().unwrap();
    assert_eq!(interval.clipped(2000, 3000).unwrap().from_ms, 2000);
    assert_eq!(interval.clipped(2000, 3000).unwrap().until_ms, Some(3000));
}

#[test]
fn moving_a_date_preserves_precise_time_and_refuses_dst_ambiguity() {
    let changed = on_date(
        "2026-10-03T10:30:12.125-03:00",
        "2026-10-04",
        "America/Sao_Paulo",
    )
    .unwrap();
    assert_eq!(changed, "2026-10-04T10:30:12.125-03:00");
    assert!(
        on_date(
            "2026-10-31T01:30:00-04:00",
            "2026-11-01",
            "America/New_York"
        )
        .is_err()
    );
    assert!(
        on_date(
            "2026-03-07T02:30:00-05:00",
            "2026-03-08",
            "America/New_York"
        )
        .is_err()
    );
}

#[test]
fn mixed_endpoints_reject_an_unrepresentable_local_date() {
    let offset = FixedOffset::east_opt(9 * 3600).unwrap();
    let instant = TimeValue::Instant(DateTime::<Utc>::MAX_UTC.with_timezone(&offset));
    let due = TimeValue::Date(NaiveDate::MAX);
    let error = Endpoints::new(Some(&instant), Some(&due)).unwrap_err();
    assert_eq!(error, "Scheduling timestamp is outside the supported range");
}
