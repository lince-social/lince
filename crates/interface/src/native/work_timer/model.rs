// non-lean-yet
// Scope: struct Entry, struct Logs, struct Timing, fn duration_caption.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 100, 178, 193,
//   198, 204.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   105, 106, 120, 129, 132, 133, 144.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (63, 195).

use bevy::prelude::*;
use chrono::{DateTime, FixedOffset, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{borrow::Cow, collections::HashMap};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Entry {
    pub id: String,
    pub start: String,
    pub end: Option<String>,
}

impl Entry {
    pub fn value(&self) -> Value {
        json!({"start":self.start, "end":self.end})
    }

    pub fn start_time(&self) -> Option<DateTime<FixedOffset>> {
        DateTime::parse_from_rfc3339(&self.start).ok()
    }
}

#[derive(Default)]
pub(super) struct Logs {
    entries: Vec<Entry>,
    timing: Vec<Timing>,
    by_id: HashMap<String, usize>,
    ordered: Vec<usize>,
    completed: i64,
    dynamic: Vec<usize>,
    active: bool,
    total_caption: Option<String>,
}

struct Timing {
    start: Option<DateTime<FixedOffset>>,
    completed: Option<i64>,
    caption: Option<String>,
}

impl Timing {
    fn new(entry: &Entry) -> Self {
        let start = entry.start_time();
        let completed = match start {
            None => Some(0),
            Some(start) => entry
                .end
                .as_deref()
                .and_then(|end| DateTime::parse_from_rfc3339(end).ok())
                .map(|end| (end - start).num_seconds().max(0)),
        };
        let caption = completed.map(|seconds| duration_caption(entry, seconds));
        Self {
            start,
            completed,
            caption,
        }
    }

    fn seconds(&self, now: DateTime<Utc>) -> i64 {
        self.completed.unwrap_or_else(|| {
            (now - self.start.unwrap().with_timezone(&Utc))
                .num_seconds()
                .max(0)
        })
    }
}

fn duration_caption(entry: &Entry, seconds: i64) -> String {
    format!(
        "{}{}",
        if entry.end.is_none() {
            "Current · "
        } else {
            ""
        },
        super::formatted(seconds)
    )
}

impl Logs {
    pub fn new(entries: Vec<Entry>) -> Self {
        let _prepare = nucleus::performance::Span::new("work-timer-log-preparation");
        nucleus::performance::add("work-timer-logs-prepared", entries.len() as u64);
        let mut logs = Self {
            timing: Vec::with_capacity(entries.len()),
            ordered: (0..entries.len()).collect(),
            entries,
            ..Default::default()
        };
        for (index, entry) in logs.entries.iter().enumerate() {
            let timing = Timing::new(entry);
            if let Some(seconds) = timing.completed {
                logs.completed = logs.completed.saturating_add(seconds);
            } else {
                logs.dynamic.push(index);
            }
            logs.timing.push(timing);
            logs.by_id.entry(entry.id.clone()).or_insert(index);
            logs.active |= entry.end.is_none();
        }
        logs.ordered.sort_by_key(|&index| {
            (
                logs.entries[index].end.is_some(),
                std::cmp::Reverse(logs.timing[index].start),
            )
        });
        if logs.dynamic.is_empty() {
            logs.total_caption = Some(format!("Total {}", super::formatted(logs.completed)));
        }
        logs
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn ordered(&self) -> impl Iterator<Item = &Entry> {
        self.ordered.iter().map(|&index| &self.entries[index])
    }

    pub fn contains(&self, id: &str) -> bool {
        self.by_id.contains_key(id)
    }

    pub fn caption(&self, id: &str, now: DateTime<Utc>) -> Option<Cow<'_, str>> {
        let &index = self.by_id.get(id)?;
        Some(match &self.timing[index].caption {
            Some(caption) => Cow::Borrowed(caption),
            None => Cow::Owned(duration_caption(
                &self.entries[index],
                self.timing[index].seconds(now),
            )),
        })
    }

    pub fn active(&self) -> bool {
        self.active
    }

    pub fn seconds(&self, now: DateTime<Utc>) -> i64 {
        self.dynamic.iter().fold(self.completed, |total, &index| {
            total.saturating_add(self.timing[index].seconds(now))
        })
    }

    pub fn total_caption(&self, now: DateTime<Utc>) -> Cow<'_, str> {
        if self.dynamic.is_empty() {
            Cow::Borrowed(self.total_caption.as_deref().unwrap_or("Total 00:00:00"))
        } else {
            Cow::Owned(format!("Total {}", super::formatted(self.seconds(now))))
        }
    }
}

#[derive(Component, Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LocalTimer {
    pub(super) logs: Vec<Entry>,
}

impl LocalTimer {
    pub(crate) fn valid(&self) -> bool {
        let ids: std::collections::HashSet<_> = self.logs.iter().map(|entry| &entry.id).collect();
        ids.len() == self.logs.len()
            && self
                .logs
                .iter()
                .all(|entry| entry.id.starts_with("work.log:") && entry.id.len() <= 160)
            && self.logs.iter().filter(|entry| entry.end.is_none()).count() <= 1
            && engine::private_work::WorkMetadata::validate(
                &json!({"logs":self.logs.iter().map(Entry::value).collect::<Vec<_>>()}),
            )
            .is_ok()
    }

    pub(super) fn change(&mut self, id: &str, value: Option<Entry>) -> Result<(), String> {
        let mut next = self.clone();
        next.logs.retain(|entry| entry.id != id);
        if let Some(value) = value {
            next.logs.push(value);
        }
        if !next.valid() {
            return Err("Use timestamps with a timezone, an end after the start, and at most one running entry.".into());
        }
        next.logs.sort_by_key(Entry::start_time);
        *self = next;
        Ok(())
    }

    pub(super) fn toggle(&mut self, now: DateTime<Utc>) -> Result<(), String> {
        if let Some(entry) = self.logs.iter().find(|entry| entry.end.is_none()) {
            let mut entry = entry.clone();
            entry.end = Some(
                now.max(entry.start_time().unwrap().with_timezone(&Utc))
                    .to_rfc3339(),
            );
            self.change(&entry.id.clone(), Some(entry))
        } else {
            let id = format!("work.log:{}", nucleus::new_uid("op"));
            self.change(
                &id,
                Some(Entry {
                    id: id.clone(),
                    start: now.to_rfc3339(),
                    end: None,
                }),
            )
        }
    }
}
