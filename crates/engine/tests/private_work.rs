use chrono::{DateTime, NaiveDate};
use engine::private_work::{
    MAX_ESTIMATE_MINUTES, MAX_WORK_BYTES, MAX_WORK_LOGS, MAX_WORK_TIMESTAMP_BYTES, WorkError,
    WorkMetadata,
};
use serde_json::{Value, json};

fn parse(value: Value) -> Result<WorkMetadata, WorkError> {
    WorkMetadata::parse(&value)
}

fn log(start: &str, end: Option<&str>) -> Value {
    json!({"start": start, "end": end})
}

#[test]
fn missing_and_null_optional_values_remain_unset() {
    for value in [
        json!({}),
        json!({"start": null, "due": null, "estimate_min": null}),
    ] {
        let work = parse(value).unwrap();
        assert_eq!(work.start(), None);
        assert_eq!(work.due(), None);
        assert_eq!(work.estimate_minutes(), None);
        assert!(work.logs().is_empty());
    }
}

#[test]
fn exact_calendar_dates_and_estimate_are_typed() {
    let work = parse(json!({
        "start": "2024-02-29",
        "due": "2026-12-31",
        "estimate_min": 90.5,
    }))
    .unwrap();
    assert_eq!(work.start().copied(), NaiveDate::from_ymd_opt(2024, 2, 29));
    assert_eq!(work.due().copied(), NaiveDate::from_ymd_opt(2026, 12, 31));
    assert_eq!(work.estimate_minutes(), Some(90.5));
}

#[test]
fn malformed_or_noncanonical_calendar_dates_refuse() {
    for value in [
        json!(""),
        json!("2026-1-01"),
        json!("2026-01-1"),
        json!("2026/01/01"),
        json!("2025-02-29"),
        json!("2026-02-30"),
        json!(true),
        json!(7),
    ] {
        assert_eq!(
            parse(json!({"start": value})).unwrap_err(),
            WorkError::InvalidDate
        );
    }
    assert_eq!(
        parse(json!({"due": "2026-04-31"})).unwrap_err(),
        WorkError::InvalidDate
    );
}

#[test]
fn work_can_start_after_its_due_date() {
    let work = parse(json!({"start": "2026-09-07", "due": "2026-09-01"})).unwrap();
    assert!(work.start().unwrap() > work.due().unwrap());
}

#[test]
fn estimates_accept_zero_and_the_inclusive_maximum() {
    for estimate in [0.0, 0.25, MAX_ESTIMATE_MINUTES] {
        let work = parse(json!({"estimate_min": estimate})).unwrap();
        assert_eq!(work.estimate_minutes(), Some(estimate));
    }
    assert_eq!(parse(json!({})).unwrap().estimate_minutes(), None);
}

#[test]
fn invalid_estimates_refuse_without_coercion() {
    for estimate in [
        json!(-0.01),
        json!(MAX_ESTIMATE_MINUTES + 1.0),
        json!("30"),
        json!(true),
        json!([]),
        json!({}),
    ] {
        assert_eq!(
            parse(json!({"estimate_min": estimate})).unwrap_err(),
            WorkError::InvalidEstimate
        );
    }
    assert!(serde_json::Number::from_f64(f64::NAN).is_none());
    assert!(serde_json::Number::from_f64(f64::INFINITY).is_none());
}

#[test]
fn log_offsets_are_retained_and_instants_compare_across_offsets() {
    let start = "2026-09-07T10:15:00+02:00";
    let equal = "2026-09-07T08:15:00Z";
    let later = "2026-09-07T07:30:00-01:00";
    let work = parse(json!({"logs": [log(start, Some(equal)), log(start, Some(later))]})).unwrap();
    assert_eq!(work.logs().len(), 2);
    assert_eq!(work.logs()[0].start().offset().local_minus_utc(), 7200);
    assert_eq!(work.logs()[0].end().unwrap(), work.logs()[0].start());
    assert!(work.logs()[1].end().unwrap() > work.logs()[1].start());
}

#[test]
fn a_log_end_before_its_start_refuses_by_instant() {
    assert_eq!(
        parse(json!({
            "logs": [log(
                "2026-09-07T10:00:00+02:00",
                Some("2026-09-07T07:59:59Z"),
            )]
        }))
        .unwrap_err(),
        WorkError::EndBeforeStart
    );
}

#[test]
fn multiple_open_logs_are_preserved() {
    let work = parse(json!({
        "logs": [
            log("2026-09-07T08:00:00Z", None),
            {"start": "2026-09-07T09:00:00+01:00"},
        ]
    }))
    .unwrap();
    assert_eq!(work.logs().len(), 2);
    assert!(work.logs().iter().all(|entry| entry.end().is_none()));
}

#[test]
fn timestamps_require_rfc3339_with_an_explicit_offset() {
    for timestamp in [
        "",
        "2026-09-07",
        "2026-09-07T08:00:00",
        "2026-09-07T08:00:00+0000",
        "2026-09-07T08:00:00+00:0",
        "2026-13-07T08:00:00Z",
        "not-a-time",
    ] {
        assert_eq!(
            parse(json!({"logs": [log(timestamp, None)]})).unwrap_err(),
            WorkError::InvalidTimestamp
        );
    }
    assert_eq!(
        parse(json!({
            "logs": [log(
                "2026-09-07T08:00:00Z",
                Some("2026-09-07T09:00:00"),
            )]
        }))
        .unwrap_err(),
        WorkError::InvalidTimestamp
    );
}

#[test]
fn timestamp_byte_limit_applies_before_parsing() {
    let timestamp = "x".repeat(MAX_WORK_TIMESTAMP_BYTES + 1);
    assert_eq!(
        parse(json!({"logs": [log(&timestamp, None)]})).unwrap_err(),
        WorkError::TimestampTooLong
    );
}

#[test]
fn log_shape_and_unknown_log_fields_refuse() {
    for value in [
        json!(null),
        json!("2026-09-07T08:00:00Z"),
        json!({}),
        json!({"start": null}),
        json!({"start": 7}),
        json!({"start": "2026-09-07T08:00:00Z", "end": false}),
    ] {
        assert!(matches!(
            parse(json!({"logs": [value]})),
            Err(WorkError::InvalidLog)
        ));
    }
    assert_eq!(
        parse(json!({
            "logs": [{"start": "2026-09-07T08:00:00Z", "actor": "someone"}]
        }))
        .unwrap_err(),
        WorkError::UnknownLogField
    );
}

#[test]
fn logs_must_be_an_array() {
    for logs in [json!(null), json!({}), json!("logs"), json!(3)] {
        assert_eq!(
            parse(json!({"logs": logs})).unwrap_err(),
            WorkError::InvalidLogs
        );
    }
}

#[test]
fn complete_log_count_is_bounded() {
    let entry = log("2026-09-07T08:00:00Z", None);
    let work = parse(json!({"logs": vec![entry.clone(); MAX_WORK_LOGS]})).unwrap();
    assert_eq!(work.logs().len(), MAX_WORK_LOGS);
    assert_eq!(
        parse(json!({"logs": vec![entry; MAX_WORK_LOGS + 1]})).unwrap_err(),
        WorkError::TooManyLogs
    );
}

#[test]
fn root_shape_and_unknown_fields_refuse() {
    for value in [json!(null), json!([]), json!("work"), json!(1)] {
        assert_eq!(parse(value).unwrap_err(), WorkError::NotObject);
    }
    assert_eq!(
        parse(json!({"owner": "someone"})).unwrap_err(),
        WorkError::UnknownField
    );
}

#[test]
fn complete_object_byte_limit_is_inclusive_and_refuses_overflow_before_parsing() {
    let boundary = "x".repeat(MAX_WORK_BYTES - 12);
    assert_eq!(
        parse(json!({"start": boundary})).unwrap_err(),
        WorkError::InvalidDate
    );
    let oversized = "x".repeat(MAX_WORK_BYTES - 11);
    assert_eq!(
        parse(json!({"start": oversized})).unwrap_err(),
        WorkError::TooLarge
    );
}

#[test]
fn try_from_uses_the_same_validator() {
    let value = json!({
        "start": "2026-09-07",
        "logs": [log("2026-09-07T08:00:00-03:00", None)],
    });
    let parsed = WorkMetadata::try_from(&value).unwrap();
    assert_eq!(parsed.start().unwrap().to_string(), "2026-09-07");
    assert_eq!(
        parsed.logs()[0].start(),
        &DateTime::parse_from_rfc3339("2026-09-07T08:00:00-03:00").unwrap()
    );
}
