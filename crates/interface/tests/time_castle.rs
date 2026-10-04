use lince_interface::time_castle::{Category, Entry, MAX_STACKS, Settings, stacks, tick_interval};
use nucleus::schedule::TimeRange;

fn entry(id: &str, from: i64, until: Option<i64>) -> Entry {
    Entry {
        id: id.into(),
        record_uid: format!("r:{id}"),
        head: id.into(),
        quantity: "-1".into(),
        category: Category::Timed,
        time: Some(TimeRange {
            from_ms: from,
            until_ms: until,
        }),
        origin: serde_json::json!({"kind":"manual"}),
        preview: false,
        start_date: None,
        due_date: None,
    }
}

#[test]
fn distinct_lanes_preserve_equal_points_nested_ranges_and_window_boundaries() {
    use lince_interface::time_castle::occurrences;
    let mut all_day = entry("all day", 1100, None);
    all_day.category = Category::AllDay;
    let mut overdue = entry("overdue", 1100, None);
    overdue.category = Category::Overdue;
    let entries = vec![
        entry("outer", 500, Some(2400)),
        entry("inner", 1100, Some(1500)),
        entry("point a", 1100, None),
        entry("point b", 1100, None),
        entry("zero", 1000, Some(1000)),
        entry("crossing", 1400, Some(3000)),
        entry("past", 500, None),
        entry("end", 2000, None),
        all_day,
        overdue,
    ];
    let result = occurrences(&entries, 1000, 2000, "UTC");
    assert_eq!(result.len(), 6);
    let outer = result
        .iter()
        .find(|occurrence| entries[occurrence.index].id == "outer")
        .unwrap();
    assert_eq!(
        outer.time,
        TimeRange {
            from_ms: 1000,
            until_ms: Some(2000)
        }
    );
    let zero = result
        .iter()
        .find(|occurrence| entries[occurrence.index].id == "zero")
        .unwrap();
    assert_eq!(zero.time.until_ms, None);
    let same: Vec<_> = result
        .iter()
        .filter(|occurrence| occurrence.time.from_ms == 1100)
        .map(|occurrence| occurrence.lane)
        .collect();
    assert_eq!(same.len(), 3);
    assert_eq!(
        same.iter().collect::<std::collections::HashSet<_>>().len(),
        3
    );
    for (index, a) in result.iter().enumerate() {
        for b in result.iter().skip(index + 1) {
            if a.time.until_ms.unwrap_or(a.time.from_ms + 1) > b.time.from_ms {
                assert_ne!(a.lane, b.lane);
            }
        }
    }
}

#[test]
fn outward_labels_keep_full_text_and_timing_without_intersections_or_clock_resize() {
    use lince_interface::time_castle::{CursorMode, labels};
    let settings = Settings {
        cursor: CursorMode::Fixed,
        ..Settings::default()
    };
    let now = 1_791_021_600_000;
    let entries: Vec<_> = (0..96)
        .map(|index| {
            let mut event = entry(
                &format!("Task {index} with a complete title and readable words"),
                now + (index % 16) * 220_000,
                (index % 3 == 0).then_some(now + (index % 16) * 220_000 + 300_000),
            );
            event.preview = index % 7 == 0;
            event
        })
        .collect();
    let result = labels(&settings, &entries, now, [420.0, 420.0], 13.0, 8.0);
    assert_eq!(result.len(), entries.len());
    assert_eq!(
        result,
        labels(&settings, &entries, now, [420.0, 420.0], 13.0, 8.0)
    );
    for (index, a) in result.iter().enumerate() {
        let event = &entries[a.occurrence.index];
        assert_eq!(a.title.replace('\n', " "), event.head);
        assert_eq!(
            a.time.replace('\n', " "),
            format!(
                "{}{}",
                event.time_label(&settings, now),
                if event.preview { " · projected" } else { "" }
            )
        );
        let nearest_x = 0.0_f32.clamp(a.rect[0], a.rect[0] + a.rect[2]);
        let nearest_y = 0.0_f32.clamp(a.rect[1], a.rect[1] + a.rect[3]);
        assert!(nearest_x.hypot(nearest_y) >= 192.0);
        assert!(a.anchor[0].hypot(a.anchor[2]) <= 168.01);
        for b in result.iter().skip(index + 1) {
            assert!(
                a.rect[0] + a.rect[2] <= b.rect[0]
                    || b.rect[0] + b.rect[2] <= a.rect[0]
                    || a.rect[1] + a.rect[3] <= b.rect[1]
                    || b.rect[1] + b.rect[3] <= a.rect[1]
            );
        }
    }
}

#[test]
fn dense_annotations_preserve_every_occurrence_with_bounded_layout_work() {
    let settings = Settings::default();
    let entries: Vec<_> = (0..5000)
        .map(|index| entry(&format!("Task {index}"), 1000 + index % 60 * 60_000, None))
        .collect();
    let started = std::time::Instant::now();
    let result =
        lince_interface::time_castle::labels(&settings, &entries, 1000, [420.0, 420.0], 13.0, 8.0);
    assert_eq!(result.len(), entries.len());
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}

#[test]
fn fixed_cursor_rotates_future_work_while_moving_cursor_preserves_clock_phase() {
    use lince_interface::time_castle::CursorMode;
    let now = chrono::DateTime::parse_from_rfc3339("2026-10-03T10:10:00Z")
        .unwrap()
        .timestamp_millis();
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
    let restored: Settings =
        serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
    assert_eq!(restored.cursor, CursorMode::Fixed);
}

#[test]
fn ring_wraps_into_the_future_and_untwists_left_to_right() {
    let settings = Settings::default();
    let now = chrono::DateTime::parse_from_rfc3339("2026-10-03T10:10:00Z")
        .unwrap()
        .timestamp_millis();
    let minute_eight = now + 58 * 60_000;
    let point = settings.position(minute_eight, now, [400.0, 400.0], 0.0);
    let angle = 8.0 / 60.0 * std::f64::consts::TAU;
    assert!((f64::from(point[0]) - 160.0 * angle.sin()).abs() < 0.001);
    assert!(point[1] < 0.0);
    assert!(
        settings.position(now, now, [400.0, 400.0], 1.0)[0]
            < settings.position(now + settings.horizon_ms, now, [400.0, 400.0], 1.0)[0]
    );
    assert_eq!(settings.position(now, now, [400.0, 400.0], 1.0)[1], 0.0);
}

#[test]
fn aperture_and_horizon_remain_independent_with_valid_bounds() {
    let mut settings = Settings::default();
    settings.set_aperture(7_200_000);
    assert_eq!(settings.horizon_ms, 14_400_000);
    settings.set_aperture(36_000_000);
    assert_eq!(settings.horizon_ms, 36_000_000);
    settings.set_horizon(3_600_000);
    assert_eq!(settings.aperture_ms, 3_600_000);
    assert!(settings.valid());
    settings.aperture_ms = 0;
    assert!(!settings.valid());
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
    let first = chrono::DateTime::parse_from_rfc3339("2026-11-01T01:30:00-04:00")
        .unwrap()
        .timestamp_millis();
    let second = first + 3_600_000;
    assert_eq!(settings.tick_label(first, 60_000), "01:30 -04:00");
    assert_eq!(settings.tick_label(second, 60_000), "01:30 -05:00");
    assert_ne!(settings.label(first), settings.label(second));
}

#[test]
fn dense_stacks_are_bounded_and_preserve_all_points() {
    let entries: Vec<_> = (0..50_000)
        .map(|index| Entry {
            id: index.to_string(),
            record_uid: format!("r:{index}"),
            head: "Task".into(),
            quantity: "1".into(),
            category: Category::Timed,
            time: Some(TimeRange {
                from_ms: 1000 + index * 1000,
                until_ms: None,
            }),
            origin: serde_json::json!({"kind":"manual"}),
            preview: false,
            start_date: None,
            due_date: None,
        })
        .collect();
    let result = stacks(&entries, 1000, 50_001_000, 1_000_000.0, "UTC");
    assert!(result.len() <= MAX_STACKS);
    assert_eq!(
        result.iter().map(|stack| stack.ids.len()).sum::<usize>(),
        50_000
    );
    assert!(result.iter().all(|stack| stack.time.until_ms.is_none()));
    assert!(stacks(&entries, 50_001_000, 50_002_000, 1000.0, "UTC").is_empty());
}
