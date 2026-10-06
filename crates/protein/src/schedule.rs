use crate::{Predicate, Protein, ProteinError, Source};
use nucleus::projection::{Incomplete, Scheduled, Status, Window};
use nucleus::schedule::{TimeValue, timezone_date};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

mod admitted;

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
    let context = crate::calendar::context(query, actor)?;
    if query.aggregate.is_some() || query.include.projection.is_some() {
        return Err(invalid(
            "Schedule uses one cached projection window and does not aggregate work",
        ));
    }
    let mut manual = query.clone();
    manual.source = Source::Record;
    manual.fields = Some(
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
        .map(str::to_string)
        .to_vec(),
    );
    manual.limit = Some(nucleus::projection::MAX_SPANS + 1);
    manual
        .filter
        .retain(|filter| !matches!(filter, Predicate::ProjectionWindow(_)));
    let live = crate::execute_records(store, &manual, visible, actor).await?;
    let records: HashMap<_, _> = live
        .iter()
        .filter_map(|row| Some((row["uid"].as_str()?.to_owned(), row.clone())))
        .collect();
    let links = store::records::all_extensions(&store.pool, "work").await?;
    let manual_budget = live.len() > nucleus::projection::MAX_SPANS;
    let now = nucleus::execution::now().timestamp_millis();
    let from_date = context
        .window
        .date(context.window.from_ms)
        .ok_or_else(|| invalid("Invalid schedule window"))?;
    let until_date = context
        .window
        .date(context.window.until_ms - 1)
        .ok_or_else(|| invalid("Invalid schedule window"))?;
    let today = context
        .window
        .date(now)
        .ok_or_else(|| invalid("Invalid current time"))?;
    let mut output = Vec::new();
    for mut row in live {
        let start = row["start_date"].as_str();
        let due = row["due_date"].as_str();
        let time = nucleus::schedule::range(start, due, row["estimate_min"].as_f64(), None)
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
        let due_time = due.and_then(|value| TimeValue::parse(value).ok());
        let overdue = active
            && due_time.as_ref().is_some_and(|due| match due {
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
            let start = start
                .or(due)
                .and_then(|value| timezone_date(value, &context.window.timezone));
            let end = due
                .or(row["start_date"].as_str())
                .and_then(|value| timezone_date(value, &context.window.timezone));
            if !start.zip(end).is_some_and(|(start, end)| {
                start.to_string() <= until_date && end.to_string() >= from_date
            }) {
                continue;
            }
            "all-day"
        };
        let uid = row["uid"].as_str().unwrap_or_default().to_owned();
        row["record_uid"] = json!(uid);
        row["uid"] = json!(format!("manual:{uid}"));
        row["record_kind"] = row["kind"].clone();
        row["kind"] = json!("schedule-entry");
        row["category"] = json!(category);
        row["time"] = json!(time);
        let occurrence = links
            .get(&uid)
            .and_then(|work| work.get("projection_occurrence"))
            .and_then(|value| {
                serde_json::from_value::<nucleus::projection::OccurrenceLink>(value.clone()).ok()
            });
        row["origin"] = if undated_need {
            json!({"kind":"need"})
        } else if let Some(occurrence) = occurrence {
            json!({"kind":"manual", "occurrence":occurrence})
        } else {
            json!({"kind":"manual"})
        };
        row["preview"] = json!(false);
        output.push(row);
    }
    let (confirmed, admission_budget) =
        admitted::entries(store, &context, &records, &links, visible).await?;
    output.extend(confirmed);
    let confirmed_ids: HashSet<_> = output
        .iter()
        .filter_map(|row| row["uid"].as_str().map(str::to_owned))
        .collect();
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
        store::projection::read(&store.pool, &context, now).await?
    } else {
        None
    };
    let mut status = cached
        .as_ref()
        .map(|cached| cached.status.clone())
        .unwrap_or_else(|| {
            unavailable.map_or(Status::Updating {}, |reason| Status::Incomplete {
                base_ms: now,
                reason,
            })
        });
    if manual_budget || admission_budget {
        status = Status::Incomplete {
            base_ms: now,
            reason: Incomplete::Budget {},
        };
    }
    if let Some(cached) = cached {
        let mut filters = Vec::new();
        let mut quantities = Vec::new();
        if manual
            .filter
            .iter()
            .all(|filter| crate::calendar::split(filter, &mut filters, &mut quantities))
        {
            manual.filter = filters;
            let candidates = crate::execute_records(store, &manual, visible, actor).await?;
            if candidates.len() > nucleus::projection::MAX_SPANS {
                status = Status::Incomplete {
                    base_ms: now,
                    reason: Incomplete::Budget {},
                };
            }
            let candidates: HashMap<_, _> = candidates
                .into_iter()
                .filter_map(|row| Some((row["uid"].as_str()?.to_owned(), row)))
                .collect();
            let actual: HashSet<_> = links
                .iter()
                .filter(|(uid, _)| visible.is_none_or(|visible| visible.contains(*uid)))
                .filter_map(|(_, work)| work.get("projection_occurrence").cloned())
                .filter_map(|value| {
                    serde_json::from_value::<nucleus::projection::OccurrenceLink>(value).ok()
                })
                .map(|link| {
                    (
                        link.record.as_str().to_owned(),
                        link.occurrence.rule_uid,
                        link.occurrence.revision,
                        link.occurrence.event_id,
                    )
                })
                .collect();
            for entry in cached.schedule {
                if confirmed_ids.contains(&entry.id) {
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
                    && actual.contains(&(
                        entry.record.as_str().to_owned(),
                        occurrence.rule_uid.clone(),
                        occurrence.revision,
                        occurrence.event_id.clone(),
                    ))
                {
                    continue;
                }
                let row = if let Some(record) = candidates.get(entry.record.as_str()) {
                    record.clone()
                } else if entry.preview
                    && manual
                        .filter
                        .iter()
                        .all(|filter| preview_matches(filter, &entry))
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
                output.push(row);
            }
        } else {
            status = Status::Incomplete {
                base_ms: now,
                reason: Incomplete::UnsupportedFilter {},
            };
        }
    }
    output.sort_by(|a, b| {
        a["time"]["from_ms"]
            .as_i64()
            .cmp(&b["time"]["from_ms"].as_i64())
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

fn preview_matches(filter: &Predicate, entry: &Scheduled) -> bool {
    match filter {
        Predicate::UidEq(uid) | Predicate::RecordEq(uid) => uid == entry.record.as_str(),
        Predicate::KindEq(kind) => kind == &entry.record_kind,
        Predicate::SlugEq(slug) => entry.slug.as_ref() == Some(slug),
        Predicate::TextContains(text) => entry.head.to_lowercase().contains(&text.to_lowercase()),
        Predicate::All(children) => children.iter().all(|filter| preview_matches(filter, entry)),
        _ => false,
    }
}

fn invalid(message: impl ToString) -> ProteinError {
    store::StoreError::Protocol(format!("protein_schedule_invalid:{}", message.to_string()))
}
