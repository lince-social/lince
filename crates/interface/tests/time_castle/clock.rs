use super::entry;
use chrono::DateTime;
use lince_interface::time_castle::{CursorMode, Settings, tick_interval};
use serde_json::{from_str, to_string};
use std::f64::consts::TAU;

fn timestamp(value: &str) -> i64 {
    DateTime::parse_from_rfc3339(value)
        .expect("valid test timestamp")
        .timestamp_millis()
}

#[test]
fn ruler_labels_cross_midnight_and_disambiguate_daylight_saving_time() {
    let now = timestamp("2026-10-03T23:58:12Z");
    let settings = Settings::default();
    let ticks = settings.rim_ticks(now);
    assert!(
        ticks
            .iter()
            .any(|tick| tick.major && tick.label == "00" && tick.at_ms > now)
    );
    let now = timestamp("2026-11-01T01:55:00-04:00");
    let settings = Settings {
        timezone: "America/New_York".into(),
        aperture_ms: 7_200_000,
        ..Settings::default()
    };
    let ticks = settings.rim_ticks(now);
    assert_eq!(ticks.len(), 60);
    assert!(
        ticks
            .iter()
            .filter(|tick| tick.major)
            .any(|tick| tick.label.contains("-05:00"))
    );
    assert!(
        ticks
            .iter()
            .all(|tick| tick.at_ms >= now && tick.at_ms < now + settings.aperture_ms)
    );
}

#[test]
fn every_aperture_has_twelve_major_and_sixty_total_next_occurrence_ticks() {
    let now = timestamp("2026-10-03T20:06:12.345Z");
    for aperture_ms in [1000, 60_000, 3_600_000, 7_200_000, 36_000_000, 86_400_000] {
        let settings = Settings {
            aperture_ms,
            horizon_ms: aperture_ms,
            ..Settings::default()
        };
        let mut ticks = settings.rim_ticks(now);
        assert_eq!(ticks.len(), 60);
        assert_eq!(ticks.iter().filter(|tick| tick.major).count(), 12);
        assert!(
            ticks
                .iter()
                .all(|tick| tick.at_ms >= now && tick.at_ms < now + aperture_ms)
        );
        assert!(
            ticks
                .iter()
                .filter(|tick| tick.major)
                .all(|tick| !tick.label.is_empty())
        );
        ticks.sort_by_key(|tick| tick.at_ms);
        assert!(ticks.windows(2).all(|pair| {
            ((pair[1].at_ms - pair[0].at_ms) as f64 - aperture_ms as f64 / 60.0).abs() <= 1.0
        }));
    }
}

#[test]
fn fixed_cursor_rotates_future_work_while_moving_cursor_preserves_clock_phase() {
    let now = timestamp("2026-10-03T10:10:00Z");
    let mut settings = Settings::default();
    let at = now + 20 * 60_000;
    let first = settings.position(at, now, [400.0, 400.0], 0.0);
    let later = settings.position(at, now + 60_000, [400.0, 400.0], 0.0);
    assert!((first[0] - later[0]).abs() < 0.001);
    assert!((first[2] - later[2]).abs() < 0.001);
    settings.cursor = CursorMode::Fixed;
    assert_eq!(
        settings.position(now, now, [400.0, 400.0], 0.0),
        [0.0, 0.0, -160.0]
    );
    let first = settings.position(at, now, [400.0, 400.0], 0.0);
    let later = settings.position(at, now + 60_000, [400.0, 400.0], 0.0);
    assert!((first[0] - later[0]).abs() > 1.0);
    assert_eq!(settings.transverse(now, now, 0.0), [0.0, 0.0, -1.0]);
    let restored: Settings = from_str(&to_string(&settings).unwrap()).unwrap();
    assert_eq!(restored.cursor, CursorMode::Fixed);
}

#[test]
fn ring_wraps_into_the_future_and_untwists_left_to_right() {
    let settings = Settings::default();
    let now = timestamp("2026-10-03T10:10:00Z");
    let minute_eight = now + 58 * 60_000;
    let point = settings.position(minute_eight, now, [400.0, 400.0], 0.0);
    let angle = 8.0 / 60.0 * TAU;
    assert!((f64::from(point[0]) - 160.0 * angle.sin()).abs() < 0.001);
    assert!(point[1] < 0.0);
    assert!(
        settings.position(now, now, [400.0, 400.0], 1.0)[0]
            < settings.position(now + settings.horizon_ms, now, [400.0, 400.0], 1.0)[0]
    );
    assert_eq!(settings.position(now, now, [400.0, 400.0], 1.0)[1], 0.0);
}

#[test]
fn aperture_sets_the_schedule_window_with_valid_bounds() {
    let mut settings = Settings::default();
    settings.set_aperture(7_200_000);
    assert_eq!(settings.horizon_ms, 7_200_000);
    settings.set_aperture(36_000_000);
    assert_eq!(settings.horizon_ms, 36_000_000);
    settings.set_horizon(3_600_000);
    assert_eq!(settings.aperture_ms, 3_600_000);
    assert!(settings.valid());
    settings.aperture_ms = 0;
    assert!(!settings.valid());
}

#[test]
fn day_sized_apertures_show_each_event_date_once() {
    let now = timestamp("2026-10-05T19:16:00Z");
    let at = timestamp("2026-10-06T00:00:00Z");
    let mut settings = Settings::default();
    settings.set_aperture(2 * 86_400_000);
    assert_eq!(
        entry("task", at, None).time_label(&settings, now),
        "10-06 00:00"
    );
}

#[test]
fn fine_ticks_are_exposed_as_screen_density_increases() {
    assert!(tick_interval(3_600_000, 1000.0, 12.0) > tick_interval(3_600_000, 60_000.0, 12.0));
    assert_eq!(tick_interval(60_000, 1000.0, 12.0), 1000);
}

#[test]
fn repeated_clock_labels_identify_their_offsets() {
    let settings = Settings {
        timezone: "America/New_York".into(),
        ..Settings::default()
    };
    let first = timestamp("2026-11-01T01:30:00-04:00");
    let second = first + 3_600_000;
    assert_eq!(settings.tick_label(first, 60_000), "01:30 -04:00");
    assert_eq!(settings.tick_label(second, 60_000), "01:30 -05:00");
    assert_ne!(settings.label(first), settings.label(second));
}
