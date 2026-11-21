//! Calendar queries combine live Record dates with cached observations.

use std::{collections::HashSet, fmt::Display};

use nucleus::{
    execution::now,
    projection::{Context, Dates, Window},
};
use serde_json::{Value, json};
use store::{Store, StoreError};

use crate::{Predicate, Protein, ProteinError, Source, execute_records, projection::status_row};

mod forecast;

/// Reports whether a predicate supplies the Calendar or Schedule window.
pub fn is_window(predicate: &Predicate) -> bool {
    matches!(
        predicate,
        Predicate::ProjectionWindow(_) | Predicate::RollingWindow(_)
    )
}

/// Resolves the query's window and copies the read scope into its context.
///
/// # Errors
/// Returns an error if the query lacks exactly one valid projection window.
pub fn context(query: &Protein, actor: Option<&str>) -> Result<Context, ProteinError> {
    context_with_dates(query, actor).map(|(context, _)| context)
}

pub(super) fn context_with_dates(
    query: &Protein,
    actor: Option<&str>,
) -> Result<(Context, Dates), ProteinError> {
    let mut windows = query.filter.iter().filter_map(|predicate| match predicate {
        Predicate::ProjectionWindow(window) => Some(Ok(window.clone())),
        Predicate::RollingWindow(window) => {
            Some(window.resolve(now().timestamp_millis()).map_err(invalid))
        }
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
    store: &Store,
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
    let manual = Protein {
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
    let mut rows = execute_records(store, &manual, visible, actor).await?;
    let from = dates
        .at(context.window.from_ms)
        .ok_or_else(|| invalid("invalid window start"))?;
    let until = dates
        .at(context.window.until_ms - 1)
        .ok_or_else(|| invalid("invalid window end"))?;
    rows.retain_mut(|row| {
        for property in ["start_date", "due_date"] {
            if let Some(value) = row[property].as_str() {
                row[property] = dates
                    .parse(value)
                    .map_or(Value::Null, |date| Value::String(date.to_string()));
            }
        }
        let start = row["start_date"].as_str();
        let end = row["due_date"].as_str();
        let keep = start
            .or(end)
            .zip(end.or(start))
            .is_some_and(|(start, end)| start <= until.as_str() && end >= from.as_str());
        if keep {
            row["origin"] = json!({ "kind": "manual" });
        }
        keep
    });
    let status = forecast::append(store, &context, &dates, manual, visible, &mut rows).await?;
    if let Some(limit) = query.limit {
        rows.truncate(limit);
    }
    rows.push(status_row(&status, &context.window.timezone));
    Ok(rows)
}

fn invalid(message: impl Display) -> ProteinError {
    StoreError::Protocol(format!("protein_calendar_invalid:{message}"))
}

/// Builds a Calendar query for the supplied window and Record predicates.
pub fn query(window: Window, mut filter: Vec<Predicate>) -> Protein {
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
