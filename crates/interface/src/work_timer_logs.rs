//! Prepared work log durations, ordering, and display captions.

use chrono::{DateTime, FixedOffset, Utc};
use nucleus::performance::{Span, add};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{borrow::Cow, cmp::Reverse, collections::HashMap};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Entry {
    pub(crate) id: String,
    pub(crate) start: String,
    pub(crate) end: Option<String>,
}

impl Entry {
    pub(crate) fn value(&self) -> Value {
        json!({"start": self.start, "end": self.end})
    }

    pub(crate) fn start_time(&self) -> Option<DateTime<FixedOffset>> {
        DateTime::parse_from_rfc3339(&self.start).ok()
    }
}

#[derive(Default)]
pub(crate) struct Logs {
    entries: Vec<Entry>,
    timing: Vec<Timing>,
    by_id: HashMap<String, usize>,
    ordered: Vec<usize>,
    completed: i64,
    dynamic: Vec<DateTime<FixedOffset>>,
    active: bool,
    total_caption: Option<String>,
}

enum Timing {
    Fixed {
        start: Option<DateTime<FixedOffset>>,
        seconds: i64,
        caption: String,
    },
    Running(DateTime<FixedOffset>),
}

impl Timing {
    fn new(entry: &Entry) -> Self {
        let start = entry.start_time();
        let seconds = if let Some(start) = start {
            let Some(end) = entry
                .end
                .as_deref()
                .and_then(|end| DateTime::parse_from_rfc3339(end).ok())
            else {
                return Self::Running(start);
            };
            (end - start).num_seconds().max(0)
        } else {
            0
        };
        Self::Fixed {
            start,
            seconds,
            caption: duration_caption(entry, seconds),
        }
    }

    fn start(&self) -> Option<DateTime<FixedOffset>> {
        match self {
            Self::Fixed { start, .. } => *start,
            Self::Running(start) => Some(*start),
        }
    }
}

fn elapsed(start: DateTime<FixedOffset>, now: DateTime<Utc>) -> i64 {
    (now - start.with_timezone(&Utc)).num_seconds().max(0)
}

fn duration_caption(entry: &Entry, seconds: i64) -> String {
    let prefix = if entry.end.is_none() {
        "Current · "
    } else {
        ""
    };
    format_duration(prefix, seconds)
}

/// Formats a duration as hours, minutes, and seconds, clamping negatives to zero.
pub fn formatted(seconds: i64) -> String {
    format_duration("", seconds)
}

fn format_duration(prefix: &str, seconds: i64) -> String {
    let seconds = seconds.max(0);
    format!(
        "{prefix}{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

impl Logs {
    pub(crate) fn new(entries: Vec<Entry>) -> Self {
        let _prepare = Span::new("work-timer-log-preparation");
        add("work-timer-logs-prepared", entries.len() as u64);
        let mut logs = Self {
            timing: Vec::with_capacity(entries.len()),
            ordered: (0..entries.len()).collect(),
            entries,
            ..Self::default()
        };
        for (index, entry) in logs.entries.iter().enumerate() {
            let timing = Timing::new(entry);
            match &timing {
                Timing::Fixed { seconds, .. } => {
                    logs.completed = logs.completed.saturating_add(*seconds);
                }
                Timing::Running(start) => logs.dynamic.push(*start),
            }
            logs.timing.push(timing);
            logs.by_id.entry(entry.id.clone()).or_insert(index);
            logs.active |= entry.end.is_none();
        }
        // The indices in `ordered` and `by_id` address the immutable parallel arrays.
        logs.ordered.sort_by_key(|&index| {
            (
                logs.entries[index].end.is_some(),
                Reverse(logs.timing[index].start()),
            )
        });
        if logs.dynamic.is_empty() {
            logs.total_caption = Some(format_duration("Total ", logs.completed));
        }
        logs
    }

    pub(crate) fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub(crate) fn ordered(&self) -> impl Iterator<Item = &Entry> {
        self.ordered.iter().map(|&index| &self.entries[index])
    }

    pub(crate) fn contains(&self, id: &str) -> bool {
        self.by_id.contains_key(id)
    }

    pub(crate) fn caption(&self, id: &str, now: DateTime<Utc>) -> Option<Cow<'_, str>> {
        let &index = self.by_id.get(id)?;
        Some(match &self.timing[index] {
            Timing::Fixed { caption, .. } => Cow::Borrowed(caption),
            Timing::Running(start) => {
                Cow::Owned(duration_caption(&self.entries[index], elapsed(*start, now)))
            }
        })
    }

    pub(crate) fn active(&self) -> bool {
        self.active
    }

    pub(crate) fn seconds(&self, now: DateTime<Utc>) -> i64 {
        self.dynamic.iter().fold(self.completed, |total, &start| {
            total.saturating_add(elapsed(start, now))
        })
    }

    pub(crate) fn total_caption(&self, now: DateTime<Utc>) -> Cow<'_, str> {
        if self.dynamic.is_empty() {
            Cow::Borrowed(self.total_caption.as_deref().unwrap_or("Total 00:00:00"))
        } else {
            Cow::Owned(format_duration("Total ", self.seconds(now)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, start: &str, end: Option<&str>) -> Entry {
        Entry {
            id: id.into(),
            start: start.into(),
            end: end.map(str::to_owned),
        }
    }

    fn instant(value: &str) -> DateTime<Utc> {
        value.parse().expect("valid fixture timestamp")
    }

    #[test]
    fn orders_logs_and_reuses_completed_captions() {
        let start = "2026-11-21T10:00:00Z";
        let first = entry("first", start, Some("2026-11-21T10:01:30Z"));
        assert_eq!(
            first.value(),
            json!({"start": start, "end": "2026-11-21T10:01:30Z"}),
        );
        assert_eq!(
            serde_json::to_value(&first).expect("serializable log"),
            json!({"id": "first", "start": start, "end": "2026-11-21T10:01:30Z"}),
        );
        let closed = Logs::new(vec![first.clone()]);
        let now = instant("2026-11-21T10:04:00Z");
        assert!(!closed.active());
        assert!(matches!(
            closed.total_caption(now),
            Cow::Borrowed("Total 00:01:30")
        ));

        let logs = Logs::new(vec![
            first,
            entry("tie", start, Some("2026-11-21T09:59:00Z")),
            entry("current", "2026-11-21T10:03:00Z", None),
            entry(
                "newer",
                "2026-11-21T10:02:00Z",
                Some("2026-11-21T10:03:00Z"),
            ),
        ]);
        assert_eq!(logs.entries().len(), 4);
        assert_eq!(
            logs.ordered()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            ["current", "newer", "first", "tie"],
        );
        assert!(logs.active() && logs.contains("current"));
        assert_eq!(logs.caption("missing", now), None);
        assert!(matches!(
            logs.caption("first", now),
            Some(Cow::Borrowed("00:01:30"))
        ));
        assert_eq!(
            logs.caption("current", now).as_deref(),
            Some("Current · 00:01:00")
        );
        assert_eq!(logs.seconds(now), 210);
        assert_eq!(logs.total_caption(now), "Total 00:03:30");
    }

    #[test]
    fn preserves_malformed_and_future_timestamp_behavior() {
        let now = instant("2026-11-21T10:02:00Z");
        let logs = Logs::new(vec![
            entry("invalid", "bad start", None),
            entry("bad-end", "2026-11-21T11:00:00+01:00", Some("bad end")),
            entry("future", "2026-11-21T10:03:00Z", None),
        ]);
        assert_eq!(
            logs.caption("invalid", now).as_deref(),
            Some("Current · 00:00:00")
        );
        assert_eq!(logs.caption("bad-end", now).as_deref(), Some("00:02:00"));
        assert_eq!(
            logs.caption("future", now).as_deref(),
            Some("Current · 00:00:00")
        );
        assert_eq!(logs.seconds(now), 120);
        assert_eq!(formatted(-1), "00:00:00");
    }
}
