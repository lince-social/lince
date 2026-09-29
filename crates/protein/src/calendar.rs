use std::collections::HashSet;

use nucleus::projection::{Context, Status, Window};
use serde_json::{Value, json};

use crate::{Predicate, Protein, ProteinError, Source};

pub fn context(query: &Protein, actor: Option<&str>) -> Result<Context, ProteinError> {
    let mut windows = query.filter.iter().filter_map(|predicate| match predicate {
        Predicate::ProjectionWindow(window) => Some(window),
        _ => None,
    });
    let window = windows
        .next()
        .ok_or_else(|| invalid("Calendar requires a projection window"))?
        .clone();
    if windows.next().is_some() {
        return Err(invalid("Calendar requires one projection window"));
    }
    window.validate().map_err(invalid)?;
    Ok(Context {
        window,
        actor: actor.map(String::from),
    })
}

pub(super) async fn execute(
    store: &store::Store,
    query: &Protein,
    visible: Option<&HashSet<String>>,
    actor: Option<&str>,
) -> Result<Vec<Value>, ProteinError> {
    let context = context(query, actor)?;
    if query.aggregate.is_some() {
        return Err(invalid("Calendar does not aggregate work"));
    }
    if query.include.projection.is_some() {
        return Err(invalid(
            "Calendar uses its saved simulation window; an additional projection include is not supported",
        ));
    }
    let mut manual_query = query.clone();
    manual_query.source = Source::Record;
    manual_query.fields = None;
    manual_query.limit = None;
    manual_query
        .filter
        .retain(|predicate| !matches!(predicate, Predicate::ProjectionWindow(_)));
    let mut rows = crate::execute_records(store, &manual_query, visible).await?;
    let from = context
        .window
        .date(context.window.from_ms)
        .ok_or_else(|| invalid("invalid window start"))?;
    let until = context
        .window
        .date(context.window.until_ms - 1)
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
        store::projection::read(&store.pool, &context, now).await?
    };
    let mut status = cached.as_ref().map_or_else(
        || {
            unavailable.map_or(Status::Updating {}, |reason| Status::Incomplete {
                base_ms: now,
                reason,
            })
        },
        |cached| cached.status.clone(),
    );
    if let Some(cached) = cached {
        let mut filters = Vec::new();
        let mut quantities = Vec::new();
        let supported = manual_query
            .filter
            .iter()
            .all(|predicate| split(predicate, &mut filters, &mut quantities));
        if supported {
            let mut candidates = manual_query.clone();
            candidates.filter = filters;
            let records = crate::execute_records(store, &candidates, visible).await?;
            let records: std::collections::HashMap<_, _> = records
                .into_iter()
                .filter_map(|row| Some((row["uid"].as_str()?.to_string(), row)))
                .collect();
            let links = store::records::all_extensions(&store.pool, "work").await?;
            let actual: HashSet<_> = rows
                .iter()
                .filter_map(|row| {
                    links
                        .get(row["uid"].as_str()?)?
                        .get("projection_occurrence")?
                        .as_object()
                        .and_then(|object| {
                            serde_json::from_value::<nucleus::projection::OccurrenceLink>(
                                Value::Object(object.clone()),
                            )
                            .ok()
                        })
                })
                .map(|link| serde_json::to_string(&link).expect("occurrence link"))
                .collect();
            for span in cached.spans {
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
                    && actual.contains(
                        &serde_json::to_string(&nucleus::projection::OccurrenceLink {
                            record: span.record.clone(),
                            occurrence: occurrence.clone(),
                            consequence: *consequence,
                        })
                        .expect("occurrence link"),
                    )
                {
                    continue;
                }
                let mut row = record.clone();
                row["uid"] = json!(format!("projection:{}", span.id));
                row["record_uid"] = json!(span.record.as_str());
                row["quantity"] = json!(span.quantity.value.to_string());
                row["start_date"] = json!(context.window.date(span.from_ms));
                row["due_date"] = json!(context.window.date(span.until_ms - 1));
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

fn split(
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

fn matches_quantity(predicate: &Predicate, value: nucleus::DecimalValue) -> bool {
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
