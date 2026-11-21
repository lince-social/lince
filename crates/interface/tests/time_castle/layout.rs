use super::entry;
use lince_interface::time_castle::{
    Category, CursorMode, Entry, LabelMetrics, MAX_STACKS, Occurrence, Settings, labels,
    labels_with_metrics, occurrences, stacks,
};
use nucleus::schedule::TimeRange;
use serde_json::json;
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

fn for_entry(occurrences: &[Occurrence], index: usize) -> &Occurrence {
    occurrences
        .iter()
        .find(|occurrence| occurrence.index == index)
        .unwrap_or_else(|| panic!("missing occurrence for entry {index}"))
}

#[test]
fn ranges_rest_outside_the_rim_and_descend_after_the_supporting_range_ends() {
    let entries = [
        entry("short earlier", 1000, Some(6000)),
        entry("long later", 3000, Some(12_000)),
    ];
    let layout = occurrences(&entries, 0, 20_000, "UTC");
    let bottom = for_entry(&layout, 0);
    let top = for_entry(&layout, 1);
    assert_eq!(bottom.level_at(5000, 1000), 0.0);
    assert_eq!(top.level_at(5000, 1000), 1.0);
    let descending = top.level_at(6500, 1000);
    assert!(descending > 0.0 && descending < 1.0);
    assert_eq!(top.level_at(7500, 1000), 0.0);
    assert_eq!(top.anchor_ms(), 7500);
    assert!(top.offset_at(5000, 60_000, 168.0, 4.0) > bottom.offset_at(5000, 60_000, 168.0, 4.0));
}

#[test]
fn original_starts_and_longer_equal_start_ranges_keep_priority_after_clipping() {
    let entries = [
        entry("short", 1000, Some(6000)),
        entry("long", 1000, Some(12_000)),
        entry("earliest", 0, Some(5000)),
        entry("point", 4000, None),
    ];
    let layout = occurrences(&entries, 3000, 20_000, "UTC");
    let level = |index| for_entry(&layout, index).level_at(4000, 1);
    assert_eq!(
        (level(2), level(1), level(0), level(3)),
        (0.0, 1.0, 2.0, 3.0)
    );
}

#[test]
fn descending_bands_keep_separation_when_an_outer_range_ends_soon() {
    let entries = [
        entry("support", 1000, Some(6000)),
        entry("long middle", 2000, Some(20_000)),
        entry("short outer", 3000, Some(6200)),
    ];
    let layout = occurrences(&entries, 0, 30_000, "UTC");
    let middle = for_entry(&layout, 1);
    let outer = for_entry(&layout, 2);
    for at in 6000..6200 {
        assert!((outer.level_at(at, 1000) - middle.level_at(at, 1000) - 1.0).abs() < 0.0001);
    }
}

#[test]
fn distinct_lanes_preserve_equal_points_nested_ranges_and_window_boundaries() {
    let mut all_day = entry("all day", 1100, None);
    all_day.category = Category::AllDay;
    let mut overdue = entry("overdue", 1100, None);
    overdue.category = Category::Overdue;
    let entries = [
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
    let simultaneous_lanes = result
        .iter()
        .filter(|occurrence| occurrence.time.from_ms == 1100)
        .map(|occurrence| occurrence.lane);
    assert_eq!(simultaneous_lanes.clone().count(), 3);
    assert_eq!(simultaneous_lanes.collect::<HashSet<_>>().len(), 3);
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
        assert!(nearest_x.hypot(nearest_y) >= 186.0);
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
fn cards_fit_the_measured_text_and_wrap_long_titles() {
    let entries = [
        entry("Short", 1000, None),
        entry(
            "A lengthy task title that needs wrapping to stay readable",
            1000,
            None,
        ),
    ];
    let advance = |_: char| 7.0;
    let labels = labels_with_metrics(
        &Settings::default(),
        &entries,
        0,
        [420.0; 2],
        14.0,
        8.0,
        &LabelMetrics {
            band_width: 4.0,
            advance: &advance,
        },
    );
    for label in &labels {
        let width = label
            .title
            .lines()
            .chain(label.time.lines())
            .map(|line| line.chars().map(advance).sum::<f32>())
            .fold(0.0, f32::max);
        assert_eq!(label.rect[2], width + 16.0);
        assert!(label.rect[2] <= 200.0);
    }
    assert!(labels[0].rect[2] < 140.0);
    assert!(labels[1].title.contains('\n'));
}

#[test]
fn dense_annotations_preserve_every_occurrence_with_bounded_layout_work() {
    let settings = Settings::default();
    let entries: Vec<_> = (0..5000)
        .map(|index| entry(&format!("Task {index}"), 1000 + index % 60 * 60_000, None))
        .collect();
    let started = Instant::now();
    let result = labels(&settings, &entries, 1000, [420.0, 420.0], 13.0, 8.0);
    assert_eq!(result.len(), entries.len());
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn coincident_tasks_spread_in_two_dimensions_near_their_own_midpoints() {
    let settings = Settings {
        cursor: CursorMode::Fixed,
        ..Settings::default()
    };
    let entries: Vec<_> = (0..12)
        .map(|index| entry(&format!("task-{index}"), 1000, None))
        .collect();
    let labels = labels(&settings, &entries, 1000, [420.0; 2], 14.0, 8.0);
    let xs: HashSet<_> = labels
        .iter()
        .map(|label| label.rect[0].round() as i32)
        .collect();
    let ys: HashSet<_> = labels
        .iter()
        .map(|label| label.rect[1].round() as i32)
        .collect();
    assert!(xs.len() > 4 && ys.len() > 4);
    let first = &labels[0];
    assert!((first.anchor[2] - (first.rect[1] + first.rect[3]) - 30.0).abs() < 0.01);
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
            origin: json!({"kind":"manual"}),
            preview: false,
            start_date: None,
            due_date: None,
            estimate_min: None,
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
