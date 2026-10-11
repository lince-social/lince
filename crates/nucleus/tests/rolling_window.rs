use nucleus::projection::RollingWindow;

fn at(value: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(value)
        .unwrap()
        .timestamp_millis()
}

#[test]
fn rolling_windows_follow_local_dates_and_daylight_saving() {
    let window = RollingWindow {
        past_days: 0,
        future_days: 1,
        timezone: "America/New_York".into(),
    };
    let spring = window.resolve(at("2026-03-08T12:00:00Z")).unwrap();
    assert_eq!(spring.from_ms, at("2026-03-08T05:00:00Z"));
    assert_eq!(spring.until_ms, at("2026-03-09T04:00:00Z"));
    assert_eq!(spring, window.resolve(at("2026-03-08T23:00:00Z")).unwrap());
    let autumn = window.resolve(at("2026-11-01T12:00:00Z")).unwrap();
    assert_eq!(autumn.until_ms - autumn.from_ms, 25 * 3_600_000);
    let next = window.resolve(at("2026-03-09T12:00:00Z")).unwrap();
    assert_eq!(next.from_ms, spring.until_ms);
}

#[test]
fn rolling_windows_bound_history_future_and_timezones() {
    let now = at("2026-10-11T12:00:00Z");
    for window in [
        RollingWindow {
            past_days: 0,
            future_days: 0,
            timezone: "UTC".into(),
        },
        RollingWindow {
            past_days: 100,
            future_days: 300,
            timezone: "UTC".into(),
        },
        RollingWindow {
            past_days: 0,
            future_days: 30,
            timezone: "invalid".into(),
        },
    ] {
        assert!(window.resolve(now).is_err());
    }
    let window = RollingWindow {
        past_days: 7,
        future_days: 30,
        timezone: "UTC".into(),
    }
    .resolve(now)
    .unwrap();
    assert_eq!(window.until_ms - window.from_ms, 37 * 86_400_000);
}
