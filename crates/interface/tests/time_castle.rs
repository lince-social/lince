//! Integration coverage of Time Castle's clock, layout, and schedule models.

#![cfg(feature = "models")]

#[path = "time_castle/clock.rs"]
mod clock;
#[path = "time_castle/layout.rs"]
mod layout;
#[path = "time_castle/schedule.rs"]
mod schedule;

use lince_interface::time_castle::{Category, Entry};
use nucleus::schedule::TimeRange;
use serde_json::json;

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
        origin: json!({"kind":"manual"}),
        preview: false,
        start_date: None,
        due_date: None,
        estimate_min: None,
    }
}
