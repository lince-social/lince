use crate::{Predicate, Protein, ProteinError, Source};
use nucleus::projection::{Incomplete, Scheduled, Status, Window};
use nucleus::schedule::{Endpoints, TimeValue};
use serde_json::{Value, json};
use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};

mod admitted;
mod allocation;
mod occurrences;

pub fn query(window: Window, filter: Vec<Predicate>) -> Protein {
    let mut query = crate::calendar::query(window, filter);
    query.source = Source::Schedule;
    query
}

pub(super) async fn execute(
    store: &store::Store,
    query: &Protein,
    visible: Option<&HashSet<String>>,
    actor: Option<&str>,
) -> Result<Vec<Value>, ProteinError> {
    let (context, dates) = crate::calendar::context_with_dates(query, actor)?;
    if query.aggregate.is_some() || query.include.projection.is_some() {
        return Err(invalid(
            "Schedule uses one cached projection window and does not aggregate work",
        ));
    }
    let mut manual = Protein {
        source: Source::Record,
        filter: query
            .filter
            .iter()
            .filter(|filter| !crate::calendar::is_window(filter))
            .cloned()
            .collect(),
        fields: Some(
            [
                "uid",
                "kind",
                "organ",
                "head",
                "slug",
                "quantity",
                "start_date",
                "due_date",
                "estimate_min",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        ),
        include: query.include.clone(),
        aggregate: None,
        order: Vec::new(),
        limit: Some(nucleus::projection::MAX_SPANS + 1),
    };
    let live = crate::execute_records(store, &manual, visible, actor).await?;
    let records = record_index(&live);
    let links = store::records::all_extensions(&store.pool, "work").await?;
    let work = occurrences::Work::new(&links);
    let materialized = occurrences::Materialized::new(&work, visible);
    let manual_budget = live.len() > nucleus::projection::MAX_SPANS;
    let now = nucleus::execution::now().timestamp_millis();
    let from_date = dates
        .at(context.window.from_ms)
        .ok_or_else(|| invalid("Invalid schedule window"))?;
    let until_date = dates
        .at(context.window.until_ms - 1)
        .ok_or_else(|| invalid("Invalid schedule window"))?;
    let today = dates
        .at(now)
        .ok_or_else(|| invalid("Invalid current time"))?;
    let mut output = Vec::new();
    let publish_manual = nucleus::performance::Span::new("protein-schedule-manual-publication");
    for row in &live {
        let endpoints = Endpoints::parse(row["start_date"].as_str(), row["due_date"].as_str())
            .map_err(invalid)?;
        let start = endpoints.start();
        let due = endpoints.due();
        let time = endpoints
            .range(row["estimate_min"].as_f64(), None)
            .map_err(invalid)?;
        let quantity = row["quantity"]
            .as_str()
            .and_then(|value| nucleus::DecimalValue::parse_inferred(value).ok());
        let active = quantity
            .as_ref()
            .is_some_and(|quantity| !quantity.is_zero());
        let needed = quantity
            .as_ref()
            .is_some_and(|quantity| quantity.is_negative());
        let undated_need = needed && start.is_none() && due.is_none();
        let expired_need = needed
            && time
                .as_ref()
                .is_some_and(|time| time.until_ms.unwrap_or(time.from_ms) < now);
        let overdue = active
            && due.is_some_and(|due| match due {
                TimeValue::Instant(time) => time.timestamp_millis() < now,
                TimeValue::Date(date) => date.to_string() < today,
            });
        let category = if overdue || undated_need || expired_need {
            "overdue"
        } else if let Some(time) = &time {
            if !time.overlaps(context.window.from_ms, context.window.until_ms) {
                continue;
            }
            "timed"
        } else {
            let start = start.or(due).map(|value| dates.value(value));
            let end = due.or(endpoints.start()).map(|value| dates.value(value));
            if !start.zip(end).is_some_and(|(start, end)| {
                start.to_string() <= until_date && end.to_string() >= from_date
            }) {
                continue;
            }
            "all-day"
        };
        let uid = row["uid"].as_str().unwrap_or_default();
        let mut row = Value::clone(row);
        nucleus::performance::add("protein-schedule-manual-row-copies", 1);
        row["record_uid"] = json!(uid);
        row["uid"] = json!(format!("manual:{uid}"));
        let kind = row["kind"].take();
        row["record_kind"] = kind;
        row["kind"] = json!("schedule-entry");
        row["category"] = json!(category);
        row["time"] = json!(time);
        row["origin"] = if undated_need {
            json!({"kind":"need"})
        } else if let Some(occurrence) = work.occurrence(uid) {
            json!({"kind":"manual", "occurrence":occurrence})
        } else {
            json!({"kind":"manual"})
        };
        row["preview"] = json!(false);
        output.push(row);
    }
    drop(publish_manual);
    let (confirmed, admission_budget) =
        admitted::entries(store, &context, &records, &work, &materialized).await?;
    drop(records);
    drop(live);
    output.extend(confirmed);
    let confirmed_ids: HashSet<_> = output
        .iter()
        .filter_map(|row| row["uid"].as_str())
        .collect();
    let mut projected = Vec::new();
    let unavailable = if actor.is_some() {
        Some(Incomplete::UnavailableRuntime {})
    } else if context.window.until_ms <= now {
        Some(Incomplete::PastWindow {})
    } else if context.window.until_ms.saturating_sub(now) > nucleus::schedule::MAX_DURATION_MS {
        Some(Incomplete::Budget {})
    } else {
        None
    };
    let cached = if unavailable.is_none() {
        store::projection::read_schedule(store, &context, now).await?
    } else {
        None
    };
    let (mut status, cached_schedule) = match cached {
        Some((status, schedule)) => (status, Some(schedule)),
        None => (
            unavailable.map_or(Status::Updating {}, |reason| Status::Incomplete {
                base_ms: now,
                reason,
            }),
            None,
        ),
    };
    if manual_budget || admission_budget {
        status = Status::Incomplete {
            base_ms: now,
            reason: Incomplete::Budget {},
        };
    }
    let refreshing = matches!(status, Status::Updating {});
    let has_cache = cached_schedule.is_some();
    let schedule = if let Some(schedule) = cached_schedule {
        schedule
    } else if refreshing {
        store::projection::previous_schedule(store, &context, now).await?
    } else {
        Vec::new()
    };
    if has_cache || !schedule.is_empty() {
        let mut filters = Vec::new();
        let mut quantities = Vec::new();
        if manual
            .filter
            .iter()
            .all(|filter| crate::calendar::split(filter, &mut filters, &mut quantities))
        {
            manual.filter = filters;
            let candidate_rows = crate::execute_records(store, &manual, visible, actor).await?;
            if candidate_rows.len() > nucleus::projection::MAX_SPANS {
                status = Status::Incomplete {
                    base_ms: now,
                    reason: Incomplete::Budget {},
                };
            }
            let candidates = record_index(&candidate_rows);
            let mut preview = None;
            let _publish =
                nucleus::performance::Span::new("protein-schedule-projected-publication");
            for entry in schedule {
                if !entry
                    .time
                    .overlaps(context.window.from_ms, context.window.until_ms)
                {
                    continue;
                }
                if refreshing
                    && (entry.preview || matches!(entry.cause, nucleus::simulation::Cause::Seed {}))
                {
                    continue;
                }
                if confirmed_ids.contains(entry.id.as_str()) {
                    continue;
                }
                if matches!(entry.cause, nucleus::simulation::Cause::Seed {})
                    && candidates.contains_key(entry.record.as_str())
                {
                    continue;
                }
                if !quantities
                    .iter()
                    .all(|filter| crate::calendar::matches_quantity(filter, entry.quantity.value))
                {
                    continue;
                }
                if let nucleus::simulation::Cause::Rule { occurrence, .. } = &entry.cause
                    && materialized.contains(entry.record.as_str(), occurrence)
                {
                    continue;
                }
                let row = if let Some(record) = candidates.get(entry.record.as_str()) {
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
                let mut row = row;
                row["uid"] = json!(entry.id);
                row["kind"] = json!("schedule-entry");
                row["record_uid"] = json!(entry.record.as_str());
                row["record_kind"] = json!(entry.record_kind);
                row["quantity"] = json!(entry.quantity.value.to_string());
                row["category"] = json!("timed");
                row["time"] = json!(entry.time);
                row["preview"] = json!(entry.preview);
                row["origin"] = json!({"kind":"projection", "cause":entry.cause});
                projected.push(row);
            }
        } else {
            status = Status::Incomplete {
                base_ms: now,
                reason: Incomplete::UnsupportedFilter {},
            };
        }
    }
    drop(confirmed_ids);
    output.extend(projected);
    let base_ms = match &status {
        Status::Ready { base_ms, .. } | Status::Incomplete { base_ms, .. } => *base_ms,
        Status::Updating {} => now,
    };
    allocation::apply(store, query, &mut output, base_ms).await?;
    output.sort_by(|a, b| {
        a["time"]["from_ms"]
            .as_i64()
            .cmp(&b["time"]["from_ms"].as_i64())
            .then_with(|| {
                let order = |row: &Value| {
                    Some((
                        row["origin"]["allocation_order"][0].as_i64()?,
                        row["origin"]["allocation_order"][1].as_u64()?,
                    ))
                };
                match (order(a), order(b)) {
                    (Some(a), Some(b)) => a.cmp(&b),
                    _ => std::cmp::Ordering::Equal,
                }
            })
            .then_with(|| a["uid"].as_str().cmp(&b["uid"].as_str()))
    });
    let limit = query
        .limit
        .unwrap_or(nucleus::projection::MAX_SPANS)
        .min(nucleus::projection::MAX_SPANS);
    if output.len() > limit {
        output.truncate(limit);
        status = Status::Incomplete {
            base_ms: now,
            reason: Incomplete::Budget {},
        };
    }
    output.push(json!({"uid":"projection-status","kind":"projection-status","status":status,"timezone":context.window.timezone}));
    Ok(output)
}

fn record_index(rows: &[Value]) -> HashMap<&str, &Value> {
    let _prepare = nucleus::performance::Span::new("protein-schedule-record-index");
    nucleus::performance::add("protein-schedule-record-index-rows", rows.len() as u64);
    rows.iter()
        .filter_map(|row| Some((row["uid"].as_str()?, row)))
        .collect()
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
        let needles = &mut self.needles;
        self.filters
            .iter()
            .all(|filter| Self::matches_filter(filter, entry, &head, needles))
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
                    let _prepare =
                        nucleus::performance::Span::new("protein-schedule-preview-prepare");
                    nucleus::performance::add("protein-schedule-preview-needles", 1);
                    text.to_lowercase()
                });
                head.get_or_init(|| {
                    nucleus::performance::add("protein-schedule-preview-heads", 1);
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

fn invalid(message: impl ToString) -> ProteinError {
    store::StoreError::Protocol(format!("protein_schedule_invalid:{}", message.to_string()))
}
