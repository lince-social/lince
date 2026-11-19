use lince_plugin_files::{calendar, handle, manifest};
use nucleus::plugins::ExportSnapshot;
use serde_json::json;

fn snapshot() -> ExportSnapshot {
    ExportSnapshot {
        scope: "cell-one".into(),
        generated_at_ms: 1_760_000_000_000,
        window: None,
        rows: vec![
            json!({"uid":"shower:2026-10-11","head":"Shower","time":{"from_ms":1_760_001_000_000i64,"until_ms":1_760_001_600_000i64},"origin":{"kind":"projection"}}),
        ],
    }
}

#[test]
fn calendar_ids_survive_regeneration_and_distinguish_occurrences_and_cells() {
    let first = calendar("Personal", &snapshot()).unwrap();
    let uid = first.lines().find(|line| line.starts_with("UID:")).unwrap();
    let mut next = snapshot();
    next.generated_at_ms += 60_000;
    next.rows[0]["time"]["from_ms"] = json!(1_760_001_060_000i64);
    let regenerated = calendar("Personal", &next).unwrap();
    assert!(regenerated.lines().any(|line| line == uid));
    next.rows.push(json!({"uid":"shower:2026-10-12","head":"Shower","time":{"from_ms":1_760_087_400_000i64,"until_ms":null}}));
    let multiple = calendar("Personal", &next).unwrap();
    let ids: std::collections::HashSet<_> = multiple
        .lines()
        .filter(|line| line.starts_with("UID:"))
        .collect();
    assert_eq!(ids.len(), 2);
    next.scope = "cell-two".into();
    assert!(
        !calendar("Personal", &next)
            .unwrap()
            .lines()
            .any(|line| line == uid)
    );
    assert!(first.contains("STATUS:TENTATIVE\r\n"));
    assert!(first.contains("DTEND:"));
}

#[test]
fn calendar_escapes_injection_and_folds_unicode_by_octets() {
    let mut data = snapshot();
    data.rows[0]["head"] = json!(format!(
        "Banho, diário; água\\\r\nBEGIN:VEVENT {}",
        "🦊".repeat(100)
    ));
    let output = calendar("Name\nEND:VCALENDAR", &data).unwrap();
    assert_eq!(
        output
            .split("\r\n")
            .filter(|line| *line == "BEGIN:VEVENT")
            .count(),
        1
    );
    assert_eq!(
        output
            .split("\r\n")
            .filter(|line| *line == "END:VCALENDAR")
            .count(),
        1
    );
    assert!(output.split("\r\n").all(|line| line.len() <= 75));
    let unfolded = output.replace("\r\n ", "");
    assert!(unfolded.contains("Banho\\, diário\\; água\\\\\\nBEGIN:VEVENT"));
    assert!(unfolded.contains(&"🦊".repeat(100)));
}

#[test]
fn all_day_ranges_have_exclusive_end_dates_and_points_have_no_fake_duration() {
    let mut data = snapshot();
    data.rows = vec![
        json!({"uid":"all-day","head":"Trip","start_date":"2026-10-11","due_date":"2026-10-13","time":null}),
        json!({"uid":"point","head":"Reminder","time":{"from_ms":1_760_001_000_000i64,"until_ms":null}}),
        json!({"uid":"brief","head":"Brief task","time":{"from_ms":1_760_001_000_000i64,"until_ms":1_760_001_000_060i64}}),
    ];
    let output = calendar("Schedule", &data).unwrap();
    assert!(output.contains("DTSTART;VALUE=DATE:20261011\r\n"));
    assert!(output.contains("DTEND;VALUE=DATE:20261014\r\n"));
    assert_eq!(output.matches("DTEND").count(), 1);
    data.rows.push(data.rows[0].clone());
    assert!(calendar("Schedule", &data).is_err());
    data.rows = vec![
        json!({"uid":"overflow","head":"Trip","start_date":"2026-10-11","due_date":"9999-12-31","time":null}),
    ];
    assert!(calendar("Schedule", &data).is_err());
    data.rows.clear();
    let empty = calendar("Schedule", &data).unwrap();
    assert!(!empty.contains("BEGIN:VEVENT"));
    assert!(empty.contains("BEGIN:VTIMEZONE\r\nTZID:UTC\r\n"));
}

#[test]
fn package_declares_two_formats_and_rejects_oversized_or_invalid_snapshots() {
    manifest("runner".into()).validate().unwrap();
    let mut data = snapshot();
    data.rows = vec![data.rows[0].clone(); 1001];
    assert!(
        handle(
            "ics",
            "render",
            json!({"settings":{"title":"Schedule"},"snapshot":data})
        )
        .is_err()
    );
    assert!(
        handle(
            "ics",
            "render",
            json!({"settings":{"title":"Schedule"},"snapshot":{"rows":[]}})
        )
        .is_err()
    );
    assert!(handle("ics", "push", json!({"settings":{"title":"Schedule"}})).is_err());
}
