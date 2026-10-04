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
fn ranges_rest_outside_the_rim_and_descend_after_the_supporting_range_ends() {
    use lince_interface::time_castle::occurrences;
    let entries = vec![
        entry("short earlier", 1000, Some(6000)),
        entry("long later", 3000, Some(12_000)),
    ];
    let layout = occurrences(&entries, 0, 20_000, "UTC");
    let bottom = layout.iter().find(|row| row.index == 0).unwrap();
    let top = layout.iter().find(|row| row.index == 1).unwrap();
    assert_eq!(bottom.level_at(5000, 1000), 0.0);
    assert_eq!(top.level_at(5000, 1000), 1.0);
    assert!(top.level_at(6500, 1000) > 0.0 && top.level_at(6500, 1000) < 1.0);
    assert_eq!(top.level_at(7500, 1000), 0.0);
    assert_eq!(top.anchor_ms(), 7500);
    assert!(top.offset_at(5000, 60_000, 168.0, 4.0) > bottom.offset_at(5000, 60_000, 168.0, 4.0));
}

#[test]
fn original_starts_and_longer_equal_start_ranges_keep_priority_after_clipping() {
    let entries = vec![
        entry("short", 1000, Some(6000)),
        entry("long", 1000, Some(12_000)),
        entry("earliest", 0, Some(5000)),
        entry("point", 4000, None),
    ];
    let layout = lince_interface::time_castle::occurrences(&entries, 3000, 20_000, "UTC");
    let level = |index| {
        layout
            .iter()
            .find(|row| row.index == index)
            .unwrap()
            .level_at(4000, 1)
    };
    assert_eq!(
        (level(2), level(1), level(0), level(3)),
        (0.0, 1.0, 2.0, 3.0)
    );
}

#[test]
fn descending_bands_keep_separation_when_an_outer_range_ends_soon() {
    let entries = vec![
        entry("support", 1000, Some(6000)),
        entry("long middle", 2000, Some(20_000)),
        entry("short outer", 3000, Some(6200)),
    ];
    let layout = lince_interface::time_castle::occurrences(&entries, 0, 30_000, "UTC");
    let middle = layout.iter().find(|row| row.index == 1).unwrap();
    let outer = layout.iter().find(|row| row.index == 2).unwrap();
    for at in 6000..6200 {
        assert!((outer.level_at(at, 1000) - middle.level_at(at, 1000) - 1.0).abs() < 0.0001);
    }
}

#[test]
fn ruler_labels_cross_midnight_and_disambiguate_daylight_saving_time() {
    let now = chrono::DateTime::parse_from_rfc3339("2026-10-03T23:58:12Z")
        .unwrap()
        .timestamp_millis();
    let settings = Settings::default();
    let ticks = settings.rim_ticks(now);
    assert!(
        ticks
            .iter()
            .any(|tick| tick.major && tick.label == "00" && tick.at_ms > now)
    );
    let now = chrono::DateTime::parse_from_rfc3339("2026-11-01T01:55:00-04:00")
        .unwrap()
        .timestamp_millis();
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
    let now = chrono::DateTime::parse_from_rfc3339("2026-10-03T20:06:12.345Z")
        .unwrap()
        .timestamp_millis();
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
fn summary_includes_every_current_and_future_task_with_correct_countdowns() {
    use lince_interface::time_castle::{countdown, summaries};
    let entries = vec![
        entry("later active", 2000, Some(9000)),
        entry("first active", 1000, Some(8000)),
        entry("next", 6000, None),
        entry("after", 7000, Some(10_000)),
    ];
    assert_eq!(
        summaries(&entries, 5000, 20_000),
        [(1, 3000), (0, 4000), (2, 1000), (3, 2000)]
    );
    assert_eq!(
        summaries(&entries, 0, 20_000),
        [(1, 1000), (0, 2000), (2, 6000), (3, 7000)]
    );
    assert_eq!(countdown(64_000), "1m 04s");
    assert_eq!(countdown(3_724_000), "1h 02m 04s");
    assert_eq!(countdown(1), "1s");
}

#[test]
fn projected_and_confirmed_records_share_the_occurrence_notification_identity() {
    let occurrence = nucleus::simulation::RuleOccurrence {
        rule_uid: "rule".into(),
        revision: 1,
        event_id: "event".into(),
        frequency: None,
        intended_at_ms: Some(6000),
    };
    let mut projected = entry("projection", 6000, None);
    projected.preview = true;
    projected.record_uid = "r_01ARZ3NDEKTSV4RRFFQ69G5FAV".into();
    projected.origin = serde_json::json!({"kind":"projection", "cause":nucleus::simulation::Cause::Rule { occurrence: occurrence.clone(), consequence: 0 }});
    let mut confirmed = entry("actual", 6000, None);
    confirmed.record_uid = "actual-record".into();
    confirmed.origin = serde_json::json!({"kind":"manual", "occurrence":nucleus::projection::OccurrenceLink {
        record: nucleus::karma::TypedUid::new(nucleus::karma::ReferenceKind::Record, "r_01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap(), occurrence, consequence: 0,
    }});
    assert_eq!(projected.cue_key(""), confirmed.cue_key(""));
    confirmed.time.as_mut().unwrap().from_ms += 1234;
    assert_eq!(projected.cue_key(""), confirmed.cue_key(""));
    let mut settings = Settings::default();
    settings.sound.mode = lince_interface::sound::Mode::Title;
    settings.sound.volume = 75;
    let restored: Settings =
        serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
    assert_eq!(restored.sound, settings.sound);
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
        assert!(a.anchor[0].hypot(a.anchor[2]) >= 173.9);
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
