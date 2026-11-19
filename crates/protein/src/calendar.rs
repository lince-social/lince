use std::collections::HashSet;

use nucleus::projection::{Context, Dates, Status, Window};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{Predicate, Protein, ProteinError, Source};

pub fn is_window(predicate: &Predicate) -> bool {
    matches!(
        predicate,
        Predicate::ProjectionWindow(_) | Predicate::RollingWindow(_)
    )
}

pub fn context(query: &Protein, actor: Option<&str>) -> Result<Context, ProteinError> {
    context_with_dates(query, actor).map(|(context, _)| context)
}

pub(super) fn context_with_dates(
    query: &Protein,
    actor: Option<&str>,
) -> Result<(Context, Dates), ProteinError> {
    let mut windows = query.filter.iter().filter_map(|predicate| match predicate {
        Predicate::ProjectionWindow(window) => Some(Ok(window.clone())),
        Predicate::RollingWindow(window) => Some(
            window
                .resolve(nucleus::execution::now().timestamp_millis())
                .map_err(invalid),
        ),
        _ => None,
    });
    let window = windows
        .next()
        .ok_or_else(|| invalid("Calendar requires a projection window"))??;
    if windows.next().is_some() {
        return Err(invalid("Calendar requires one projection window"));
    }
    let dates = window.dates().map_err(invalid)?;
    Ok((
        Context {
            window,
            actor: actor.map(String::from),
        },
        dates,
    ))
}

pub(super) async fn execute(
    store: &store::Store,
    query: &Protein,
    visible: Option<&HashSet<String>>,
    actor: Option<&str>,
) -> Result<Vec<Value>, ProteinError> {
    let (context, dates) = context_with_dates(query, actor)?;
    if query.aggregate.is_some() {
        return Err(invalid("Calendar does not aggregate work"));
    }
    if query.include.projection.is_some() {
        return Err(invalid(
            "Calendar uses its saved simulation window; an additional projection include is not supported",
        ));
    }
    let manual_query = Protein {
        source: Source::Record,
        filter: query
            .filter
            .iter()
            .filter(|predicate| !is_window(predicate))
            .cloned()
            .collect(),
        fields: None,
        include: query.include.clone(),
        aggregate: None,
        order: query.order.clone(),
        limit: None,
    };
    let mut rows = crate::execute_records(store, &manual_query, visible, actor).await?;
    for row in &mut rows {
        for property in ["start_date", "due_date"] {
            if let Some(value) = row[property].as_str() {
                row[property] = json!(dates.parse(value).map(|date| date.to_string()));
            }
        }
    }
    let from = dates
        .at(context.window.from_ms)
        .ok_or_else(|| invalid("invalid window start"))?;
    let until = dates
        .at(context.window.until_ms - 1)
        .ok_or_else(|| invalid("invalid window end"))?;
    rows.retain(|row| {
        let start = row["start_date"].as_str().or(row["due_date"].as_str());
        let end = row["due_date"].as_str().or(row["start_date"].as_str());
        start
            .zip(end)
            .is_some_and(|(start, end)| start <= until.as_str() && end >= from.as_str())
    });
    for row in &mut rows {
        row["origin"] = json!({ "kind": "manual" });
    }
    let now = nucleus::execution::now().timestamp_millis();
    let unavailable = if context.actor.is_some() {
        Some(nucleus::projection::Incomplete::UnavailableRuntime {})
    } else if context.window.until_ms <= now {
        Some(nucleus::projection::Incomplete::PastWindow {})
    } else if context.window.until_ms.saturating_sub(now) > 366 * 86_400_000 {
        Some(nucleus::projection::Incomplete::Budget {})
    } else {
        None
    };
    let cached = if unavailable.is_some() {
        None
    } else {
        store::projection::read_calendar(store, &context, now).await?
    };
    let (mut status, spans) = match cached {
        Some((status, spans)) => (status, Some(spans)),
        None => (
            unavailable.map_or(Status::Updating {}, |reason| Status::Incomplete {
                base_ms: now,
                reason,
            }),
            None,
        ),
    };
    if let Some(spans) = spans {
        let mut filters = Vec::new();
        let mut quantities = Vec::new();
        let supported = manual_query
            .filter
            .iter()
            .all(|predicate| split(predicate, &mut filters, &mut quantities));
        if supported {
            let mut candidates = manual_query;
            candidates.filter = filters;
            let records = crate::execute_records(store, &candidates, visible, actor).await?;
            let records: std::collections::HashMap<_, _> = records
                .into_iter()
                .filter_map(|row| Some((row["uid"].as_str()?.to_string(), row)))
                .collect();
            let manual_records = rows
                .iter()
                .filter_map(|row| row["uid"].as_str().map(String::from))
                .collect::<Vec<_>>();
            let links = {
                let _read =
                    nucleus::performance::Span::new("protein-calendar-work-links-read-await");
                let links =
                    store::records::extensions_for_records(&store.pool, "work", &manual_records)
                        .await?;
                nucleus::performance::add("protein-calendar-work-rows", links.len() as u64);
                links
            };
            let prepare_links =
                nucleus::performance::Span::new("protein-calendar-occurrence-index");
            let actual_links = rows
                .iter()
                .filter_map(|row| {
                    let value = links
                        .get(row["uid"].as_str()?)?
                        .get("projection_occurrence")?;
                    value.as_object()?;
                    nucleus::projection::OccurrenceLink::deserialize(value).ok()
                })
                .collect::<Vec<_>>();
            let actual = actual_links
                .iter()
                .map(|link| (&link.record, &link.occurrence, link.consequence))
                .collect::<HashSet<_>>();
            nucleus::performance::add(
                "protein-calendar-occurrence-links",
                actual_links.len() as u64,
            );
            drop(prepare_links);
            let _project = nucleus::performance::Span::new("protein-calendar-projection");
            for span in spans {
                if !quantities
                    .iter()
                    .all(|predicate| matches_quantity(predicate, span.quantity.value))
                {
                    continue;
                }
                let Some(record) = records.get(span.record.as_str()) else {
                    continue;
                };
                if let nucleus::simulation::Cause::Rule {
                    occurrence,
                    consequence,
                } = &span.cause
                {
                    nucleus::performance::add("protein-calendar-occurrence-lookups", 1);
                    if actual.contains(&(&span.record, occurrence, *consequence)) {
                        continue;
                    }
                }
                let mut row = record.clone();
                row["uid"] = json!(format!("projection:{}", span.id));
                row["record_uid"] = json!(span.record.as_str());
                row["quantity"] = json!(span.quantity.value.to_string());
                row["start_date"] = json!(dates.at(span.from_ms));
                row["due_date"] = json!(dates.at(span.until_ms - 1));
                row["origin"] = json!({ "kind": "projection", "observation": span.id, "from_ms": span.from_ms, "until_ms": span.until_ms, "cause": span.cause });
                rows.push(row);
            }
        } else {
            status = Status::Incomplete {
                base_ms: nucleus::execution::now().timestamp_millis(),
                reason: nucleus::projection::Incomplete::UnsupportedFilter {},
            };
        }
    }
    if let Some(limit) = query.limit {
        rows.truncate(limit);
    }
    rows.push(json!({ "uid": "projection-status", "kind": "projection-status", "status": status, "timezone": context.window.timezone }));
    Ok(rows)
}

pub(crate) fn split(
    predicate: &Predicate,
    filters: &mut Vec<Predicate>,
    quantities: &mut Vec<Predicate>,
) -> bool {
    match predicate {
        Predicate::All(children) => children
            .iter()
            .all(|child| split(child, filters, quantities)),
        Predicate::Any(_) | Predicate::Not(_) => false,
        Predicate::QuantityLt(_)
        | Predicate::QuantityLte(_)
        | Predicate::QuantityGt(_)
        | Predicate::QuantityGte(_)
        | Predicate::QuantityEq(_) => {
            quantities.push(predicate.clone());
            true
        }
        other @ (Predicate::UidEq(_)
        | Predicate::RecordEq(_)
        | Predicate::KindEq(_)
        | Predicate::SlugEq(_)
        | Predicate::TextContains(_)
        | Predicate::OrganEq(_)
        | Predicate::OrganIn(_)) => {
            filters.push(other.clone());
            true
        }
        _ => false,
    }
}

pub(crate) fn matches_quantity(predicate: &Predicate, value: nucleus::DecimalValue) -> bool {
    let expected = match predicate {
        Predicate::QuantityLt(value)
        | Predicate::QuantityLte(value)
        | Predicate::QuantityGt(value)
        | Predicate::QuantityGte(value)
        | Predicate::QuantityEq(value) => *value,
        _ => return false,
    };
    let Some(delta) = value.aligned_sub(expected) else {
        return false;
    };
    match predicate {
        Predicate::QuantityLt(_) => delta.is_negative(),
        Predicate::QuantityLte(_) => delta.is_negative() || delta.is_zero(),
        Predicate::QuantityGt(_) => !delta.is_negative() && !delta.is_zero(),
        Predicate::QuantityGte(_) => !delta.is_negative(),
        Predicate::QuantityEq(_) => delta.is_zero(),
        _ => false,
    }
}

fn invalid(message: impl ToString) -> ProteinError {
    store::StoreError::Protocol(format!("protein_calendar_invalid:{}", message.to_string()))
}

pub fn query(window: Window, filter: Vec<Predicate>) -> Protein {
    let mut filter = filter;
    filter.push(Predicate::ProjectionWindow(window));
    Protein {
        source: Source::Calendar,
        filter,
        fields: None,
        include: Default::default(),
        aggregate: None,
        order: Vec::new(),
        limit: None,
    }
}
