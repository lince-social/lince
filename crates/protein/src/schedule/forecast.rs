use std::{
    cell::OnceCell,
    collections::{HashMap, HashSet},
};

use nucleus::{
    performance::{Span, add},
    projection::{Context, Incomplete, MAX_SPANS, Scheduled, Status},
    schedule::MAX_DURATION_MS,
    simulation::Cause,
};
use serde_json::{Value, json};
use store::{Store, projection};

use super::{incomplete, occurrences::Materialized, record_index};
use crate::{
    Predicate, Protein, ProteinError, execute_records,
    projection::{matches_quantity, split},
};

pub(super) struct Cache<'a> {
    context: &'a Context,
    now: i64,
    status: Status,
    schedule: Option<Vec<Scheduled>>,
}

impl<'a> Cache<'a> {
    pub(super) async fn load(
        store: &Store,
        context: &'a Context,
        now: i64,
        over_budget: bool,
    ) -> Result<Self, ProteinError> {
        let unavailable = if context.actor.is_some() {
            Some(Incomplete::UnavailableRuntime {})
        } else if context.window.until_ms <= now {
            Some(Incomplete::PastWindow {})
        } else if context.window.until_ms.saturating_sub(now) > MAX_DURATION_MS {
            Some(Incomplete::Budget {})
        } else {
            None
        };
        let (mut status, cached) = if let Some(reason) = unavailable {
            (incomplete(now, reason), None)
        } else if let Some((status, schedule)) =
            projection::read_schedule(store, context, now).await?
        {
            (status, Some(schedule))
        } else {
            (Status::Updating {}, None)
        };
        if over_budget {
            status = incomplete(now, Incomplete::Budget {});
        }
        let schedule = if cached.is_none() && matches!(status, Status::Updating {}) {
            let previous = projection::previous_schedule(store, context, now).await?;
            (!previous.is_empty()).then_some(previous)
        } else {
            cached
        };
        Ok(Self {
            context,
            now,
            status,
            schedule,
        })
    }

    pub(super) async fn publish(
        mut self,
        store: &Store,
        mut manual: Protein,
        visible: Option<&HashSet<String>>,
        output: &mut Vec<Value>,
        materialized: &Materialized<'_, '_>,
    ) -> Result<Status, ProteinError> {
        let Some(schedule) = self.schedule else {
            return Ok(self.status);
        };
        let Some(quantities) = split(&mut manual.filter) else {
            return Ok(incomplete(self.now, Incomplete::UnsupportedFilter {}));
        };
        let refreshing = matches!(self.status, Status::Updating {});
        let candidate_rows =
            execute_records(store, &manual, visible, self.context.actor.as_deref()).await?;
        if candidate_rows.len() > MAX_SPANS {
            self.status = incomplete(self.now, Incomplete::Budget {});
        }
        let candidates = record_index(&candidate_rows);
        let confirmed_ids: HashSet<_> = output
            .iter()
            .filter_map(|row| row["uid"].as_str())
            .collect();
        let mut projected = Vec::new();
        let mut preview = None;
        let window = &self.context.window;
        let _publish = Span::new("protein-schedule-projected-publication");
        for entry in schedule {
            let seed = matches!(entry.cause, Cause::Seed {});
            if !entry.time.overlaps(window.from_ms, window.until_ms)
                || (refreshing && (entry.preview || seed))
                || confirmed_ids.contains(entry.id.as_str())
                || (seed && candidates.contains_key(entry.record.as_str()))
                || !quantities
                    .iter()
                    .all(|filter| matches_quantity(filter, entry.quantity.value))
            {
                continue;
            }
            if let Cause::Rule { occurrence, .. } = &entry.cause
                && materialized.contains(entry.record.as_str(), occurrence)
            {
                continue;
            }
            let mut row = if let Some(record) = candidates.get(entry.record.as_str()) {
                Value::clone(record)
            } else if entry.preview
                && preview
                    .get_or_insert_with(|| Preview::new(&manual.filter))
                    .matches(&entry)
            {
                json!({"head":entry.head,"slug":entry.slug,"record_kind":entry.record_kind})
            } else {
                continue;
            };
            row["uid"] = Value::String(entry.id);
            row["kind"] = json!("schedule-entry");
            row["record_uid"] = json!(entry.record.as_str());
            row["record_kind"] = Value::String(entry.record_kind);
            row["quantity"] = Value::String(entry.quantity.value.to_string());
            row["category"] = json!("timed");
            row["time"] = json!(entry.time);
            row["preview"] = json!(entry.preview);
            row["origin"] = json!({"kind":"projection", "cause":entry.cause});
            projected.push(row);
        }
        drop(confirmed_ids);
        drop(_publish);
        output.extend(projected);
        Ok(self.status)
    }
}

struct Preview<'a> {
    filters: &'a [Predicate],
    needles: HashMap<&'a str, String>,
}

impl<'a> Preview<'a> {
    fn new(filters: &'a [Predicate]) -> Self {
        Self {
            filters,
            needles: HashMap::new(),
        }
    }

    fn matches(&mut self, entry: &Scheduled) -> bool {
        let head = OnceCell::new();
        self.filters
            .iter()
            .all(|filter| Self::matches_filter(filter, entry, &head, &mut self.needles))
    }

    fn matches_filter(
        filter: &'a Predicate,
        entry: &Scheduled,
        head: &OnceCell<String>,
        needles: &mut HashMap<&'a str, String>,
    ) -> bool {
        match filter {
            Predicate::UidEq(uid) | Predicate::RecordEq(uid) => uid == entry.record.as_str(),
            Predicate::KindEq(kind) => kind == &entry.record_kind,
            Predicate::SlugEq(slug) => entry.slug.as_ref() == Some(slug),
            Predicate::TextContains(text) => {
                let needle = needles.entry(text.as_str()).or_insert_with(|| {
                    let _prepare = Span::new("protein-schedule-preview-prepare");
                    add("protein-schedule-preview-needles", 1);
                    text.to_lowercase()
                });
                head.get_or_init(|| {
                    add("protein-schedule-preview-heads", 1);
                    entry.head.to_lowercase()
                })
                .contains(needle.as_str())
            }
            Predicate::All(children) => children
                .iter()
                .all(|filter| Self::matches_filter(filter, entry, head, needles)),
            _ => false,
        }
    }
}
