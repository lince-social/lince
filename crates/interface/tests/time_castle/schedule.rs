use super::entry;
use lince_interface::{
    sound::Mode,
    time_castle::{
        Category, Entry, MAX_HORIZON_MS, Settings, clock_occurrences, clock_summaries, countdown,
        summaries,
    },
};
use nucleus::{
    karma::{ReferenceKind, TypedUid},
    projection::OccurrenceLink,
    schedule::TimeRange,
    simulation::{Cause, RuleOccurrence},
};
use serde_json::{from_str, from_value, json, to_string, to_value};

#[test]
fn outstanding_cards_retain_updated_estimates() {
    let mut need = entry("Banho", 0, None);
    need.origin = json!({"kind":"need"});
    need.category = Category::Overdue;
    need.time = None;
    need.estimate_min = Some(10.0);
    let restored: Entry = from_value(to_value(&need).unwrap()).unwrap();
    assert_eq!(
        restored.time_label(&Settings::default(), 1000),
        "Needed: 1 · 10 min"
    );
    need.estimate_min = Some(-1.0);
    assert_eq!(need.time_label(&Settings::default(), 1000), "Needed: 1");
}

#[test]
fn summary_includes_every_current_and_future_task_with_correct_countdowns() {
    let entries = [
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
    let occurrence = RuleOccurrence {
        rule_uid: "rule".into(),
        revision: 1,
        event_id: "event".into(),
        frequency: None,
        intended_at_ms: Some(6000),
    };
    let mut projected = entry("projection", 6000, None);
    projected.preview = true;
    projected.record_uid = "r_01ARZ3NDEKTSV4RRFFQ69G5FAV".into();
    projected.origin = json!({"kind":"projection", "cause":Cause::Rule { occurrence: occurrence.clone(), consequence: 0 }});
    let mut confirmed = entry("actual", 6000, None);
    confirmed.record_uid = "actual-record".into();
    confirmed.origin = json!({"kind":"manual", "occurrence":OccurrenceLink {
        record: TypedUid::new(ReferenceKind::Record, "r_01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap(), occurrence, consequence: 0,
    }});
    assert_eq!(projected.cue_key(""), confirmed.cue_key(""));
    confirmed.time.as_mut().unwrap().from_ms += 1234;
    assert_eq!(projected.cue_key(""), confirmed.cue_key(""));
    let mut settings = Settings::default();
    settings.sound.mode = Mode::Title;
    settings.sound.volume = 75;
    let restored: Settings = from_str(&to_string(&settings).unwrap()).unwrap();
    assert_eq!(restored.sound, settings.sound);
}

#[test]
fn confirmed_past_tasks_follow_the_cursor_and_share_the_bounded_schedule_window() {
    let now = 1_800_000_000_000;
    let mut settings = Settings::default();
    assert!(settings.past_tasks);
    let mut entries = [
        entry("past point", now - 60_000, None),
        entry("past range", now - 300_000, Some(now - 1000)),
        entry("ongoing", now - 1000, Some(now + 1000)),
        entry("future", now + 60_000, None),
        entry("old", now - 3_600_001, None),
        entry("unconfirmed", now - 2000, None),
        entry("overdue", now - 3000, None),
        entry("unapplied prediction", now - 4000, None),
    ];
    entries[5].preview = true;
    entries[6].category = Category::Overdue;
    for index in [0, 1, 4] {
        entries[index].quantity = "0".into();
    }
    entries[6].quantity = "1".into();
    entries[7].origin = json!({"kind":"projection"});
    let occurrences = clock_occurrences(&settings, &entries, now, now + settings.aperture_ms);
    let past: Vec<_> = occurrences
        .iter()
        .filter(|occurrence| occurrence.historical)
        .collect();
    assert_eq!(past.len(), 2);
    assert!(
        past.iter()
            .all(|occurrence| occurrence.time.from_ms == now && occurrence.time.until_ms.is_none())
    );
    assert_eq!(occurrences.len(), 4);
    let window = settings.window(now).unwrap();
    assert!(window.from_ms <= now - settings.aperture_ms);
    window.validate().unwrap();
    settings.past_tasks = false;
    assert!(
        clock_occurrences(&settings, &entries, now, now + settings.aperture_ms)
            .iter()
            .all(|occurrence| !occurrence.historical)
    );
    assert!(settings.window(now).unwrap().from_ms > now - settings.aperture_ms);
    settings.past_tasks = true;
    settings.set_aperture(MAX_HORIZON_MS);
    settings.window(now).unwrap().validate().unwrap();
}

#[test]
fn outstanding_needs_survive_the_history_window_and_clear_when_satisfied() {
    let now = 1_800_000_000_000;
    let mut settings = Settings::default();
    let mut need = entry("floss", now - 86_400_000, None);
    need.origin = json!({"kind":"need"});
    need.category = Category::Overdue;
    need.time = None;
    need.quantity = "-2".into();
    let mut old_occurrence = entry("yesterday", now - 86_400_000, None);
    old_occurrence.record_uid = need.record_uid.clone();
    let future = entry("tomorrow", now + 60_000, None);
    let mut entries = [need, old_occurrence, future];
    let occurrences = clock_occurrences(&settings, &entries, now, now + settings.aperture_ms);
    assert_eq!(occurrences.len(), 2);
    let cursor = occurrences
        .iter()
        .find(|occurrence| occurrence.historical)
        .unwrap();
    assert_eq!(cursor.index, 0);
    assert_eq!(cursor.anchor_ms(), now);
    assert_eq!(entries[0].time_label(&settings, now), "Needed: 2");
    settings.past_tasks = false;
    assert_eq!(
        clock_occurrences(&settings, &entries, now, now + settings.aperture_ms,).len(),
        1
    );
    settings.past_tasks = true;
    entries[0].quantity = "0".into();
    entries[1].quantity = "0".into();
    assert_eq!(
        clock_occurrences(&settings, &entries, now, now + settings.aperture_ms,).len(),
        1
    );
}

#[test]
fn cursor_queue_ranges_follow_time_without_recalculation_or_duplicate_history() {
    let now = 1_800_000;
    let mut queued = entry("queue", now + 600_000, Some(now + 1_200_000));
    queued.origin =
        json!({"kind":"need","allocation":{"cursor_offset_ms":600000,"duration_ms":600000}});
    let mut old = entry("old", now - 100_000, Some(now - 40_000));
    old.record_uid = queued.record_uid.clone();
    let settings = Settings {
        aperture_ms: 900_000,
        horizon_ms: 900_000,
        ..Default::default()
    };
    let entries = [queued, old];
    let hidden = Settings {
        past_tasks: false,
        ..settings.clone()
    };
    assert!(clock_summaries(&hidden, &entries, now, now + 900_000).is_empty());
    assert!(clock_occurrences(&hidden, &entries, now, now + 900_000).is_empty());
    for second in 0..120 {
        let time = now + second * 1000;
        let occurrences = clock_occurrences(&settings, &entries, time, time + 900_000);
        assert_eq!(occurrences.len(), 1);
        assert!(!occurrences[0].historical);
        assert_eq!(occurrences[0].time.from_ms, time + 600_000);
        assert_eq!(occurrences[0].time.until_ms, Some(time + 900_000));
        let summary = summaries(&entries, time, time + 900_000);
        assert_eq!(summary, vec![(0, 600_000)]);
        assert!(entries[0].sound_cue(0, &Default::default()).is_none());
    }
}

#[test]
fn cache_window_renewal_does_not_change_an_unchanged_cursor_schedule() {
    let mut before = entry("need", 1000, Some(61000));
    before.origin = json!({"kind":"need","allocation":{"cursor_offset_ms":0,"duration_ms":60000}});
    let mut renewed = before.clone();
    renewed.time = Some(TimeRange {
        from_ms: 301000,
        until_ms: Some(361000),
    });
    assert!(before.same_schedule(&renewed));
    renewed.origin["allocation"]["duration_ms"] = json!(120000);
    assert!(!before.same_schedule(&renewed));
    before.origin = json!({"kind":"manual"});
    renewed = before.clone();
    renewed.time.as_mut().unwrap().from_ms += 1000;
    assert!(!before.same_schedule(&renewed));
}

#[test]
fn allocation_order_survives_zero_estimates_in_cards_and_task_summaries() {
    let mut first = entry("z-first", 1000, None);
    first.origin = json!({"kind":"need","allocation":{"cursor_offset_ms":0,"duration_ms":0},"allocation_order":[0,0]});
    let mut second = entry("a-second", 1000, Some(61000));
    second.origin = json!({"kind":"need","allocation":{"cursor_offset_ms":0,"duration_ms":60000},"allocation_order":[0,1]});
    let entries = [first, second];
    let settings = Settings::default();
    assert_eq!(
        clock_summaries(&settings, &entries, 1000, 61000),
        [(0, 0), (1, 60000)]
    );
    let occurrences = clock_occurrences(&settings, &entries, 1000, 61000);
    assert_eq!(
        occurrences
            .iter()
            .map(|occurrence| occurrence.index)
            .collect::<Vec<_>>(),
        [0, 1]
    );
}
