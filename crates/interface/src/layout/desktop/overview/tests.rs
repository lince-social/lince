use super::*;

#[test]
fn dwell_fires_once_and_leaving_or_crossing_a_thumbnail_resets_its_clock() {
    let now = Instant::now();
    let delay = Duration::from_millis(400);
    let mut clock = HoverClock::default();
    assert_eq!(clock.update(Some(7), now, delay), None);
    assert_eq!(clock.deadline(), now.checked_add(delay));
    assert_eq!(
        clock.update(Some(8), now + Duration::from_millis(399), delay),
        None
    );
    assert_eq!(
        clock.update(Some(8), now + Duration::from_millis(798), delay),
        None
    );
    assert_eq!(
        clock.update(Some(8), now + Duration::from_millis(799), delay),
        Some(8)
    );
    assert_eq!(
        clock.update(Some(8), now + Duration::from_secs(2), delay),
        None
    );
    assert_eq!(clock.deadline(), None);
    assert_eq!(
        clock.update(None, now + Duration::from_secs(2), delay),
        None
    );
    assert_eq!(clock.target(), None);
    assert_eq!(
        clock.update(Some(8), now + Duration::from_secs(3), delay),
        None
    );
    assert_eq!(
        clock.update(Some(8), now + Duration::from_millis(3400), delay),
        Some(8)
    );
    assert_eq!(
        clock.update(Some(0), now + Duration::from_secs(4), delay),
        None
    );
    assert_eq!(clock.target(), None);
}

#[test]
fn a_hover_keeps_its_captured_deadline_and_rejects_unrepresentable_deadlines() {
    let now = Instant::now();
    let mut clock = HoverClock::default();
    clock.update(Some(9), now, Duration::from_millis(500));
    assert_eq!(
        clock.update(
            Some(9),
            now + Duration::from_millis(200),
            Duration::from_millis(100)
        ),
        None
    );
    assert_eq!(
        clock.update(
            Some(9),
            now + Duration::from_millis(500),
            Duration::from_secs(2)
        ),
        Some(9)
    );
    assert_eq!(clock.update(Some(10), now, Duration::MAX), None);
    assert_eq!(clock.target(), None);
}

#[test]
fn edge_speed_is_bounded_elapsed_time_based_and_can_be_disabled() {
    let bounds = Rectangle::new([100.0, 20.0], [800.0, 600.0]).unwrap();
    let settings = OverviewSettings::default();
    let top = edge_scroll([200.0, 20.0], bounds, Duration::from_millis(20), settings);
    let bottom = edge_scroll([200.0, 619.0], bounds, Duration::from_millis(20), settings);
    assert_eq!(top, -12.0);
    assert!(bottom > 0.0 && bottom <= 12.0);
    assert_eq!(
        edge_scroll([200.0, 300.0], bounds, Duration::from_secs(1), settings),
        0.0
    );
    assert_eq!(
        edge_scroll([f64::NAN, 20.0], bounds, Duration::from_secs(1), settings),
        0.0
    );
    assert_eq!(
        edge_scroll([0.0, 20.0], bounds, Duration::from_secs(1), settings),
        0.0
    );
    assert_eq!(
        edge_scroll([200.0, 20.0], bounds, Duration::from_secs(10), settings),
        -60.0
    );
    assert_eq!(
        edge_scroll(
            [200.0, 20.0],
            bounds,
            Duration::from_millis(20),
            OverviewSettings {
                edge_scroll_percent: 0,
                ..settings
            }
        ),
        0.0
    );
    let fine: f64 = (0..50)
        .map(|_| edge_scroll([200.0, 20.0], bounds, Duration::from_millis(20), settings))
        .sum();
    let coarse: f64 = (0..20)
        .map(|_| edge_scroll([200.0, 20.0], bounds, Duration::from_millis(50), settings))
        .sum();
    assert_eq!(fine, coarse);
    assert_eq!(fine, -600.0);
}

#[test]
fn portable_overview_preferences_are_validated_and_round_trip() {
    let settings = OverviewSettings {
        hover_delay_ms: 900,
        edge_scroll_percent: 25,
    };
    settings.validate().unwrap();
    assert_eq!(
        serde_json::from_slice::<OverviewSettings>(&serde_json::to_vec(&settings).unwrap())
            .unwrap(),
        settings
    );
    for settings in [
        OverviewSettings {
            hover_delay_ms: 99,
            ..settings
        },
        OverviewSettings {
            hover_delay_ms: 2001,
            ..settings
        },
        OverviewSettings {
            edge_scroll_percent: 101,
            ..settings
        },
    ] {
        assert!(settings.validate().is_err());
    }
}
