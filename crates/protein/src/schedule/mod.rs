//! Schedule queries combine live work, admitted occurrences, and saved projections.

use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
};

use nucleus::{
    execution::now,
    performance::{Span, add},
    projection::{Incomplete, MAX_SPANS, Status, Window},
};
use serde_json::Value;
use store::{Store, StoreError};

use crate::{
    Predicate, Protein, ProteinError, Source, calendar, execute_records, projection::status_row,
};

mod admitted;
mod allocation;
mod forecast;
mod manual;
mod occurrences;

/// Queries live and projected work in a single Schedule window.
pub fn query(window: Window, filter: Vec<Predicate>) -> Protein {
    let mut query = calendar::query(window, filter);
    query.source = Source::Schedule;
    query
}

pub(super) async fn execute(
    store: &Store,
    query: &Protein,
    visible: Option<&HashSet<String>>,
    actor: Option<&str>,
) -> Result<Vec<Value>, ProteinError> {
    let (context, dates) = calendar::context_with_dates(query, actor)?;
    if query.aggregate.is_some() || query.include.projection.is_some() {
        return Err(invalid(
            "Schedule uses one cached projection window and does not aggregate work",
        ));
    }
    let manual = Protein {
        source: Source::Record,
        filter: query
            .filter
            .iter()
            .filter(|filter| !calendar::is_window(filter))
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
            .map(String::from)
            .into(),
        ),
        include: query.include.clone(),
        aggregate: None,
        order: Vec::new(),
        limit: Some(MAX_SPANS + 1),
    };
    let live = execute_records(store, &manual, visible, actor).await?;
    let records = record_index(&live);
    let links = store::records::all_extensions(&store.pool, "work").await?;
    let work = occurrences::Work::new(&links);
    let materialized = occurrences::Materialized::new(&work, visible);
    let now = now().timestamp_millis();
    let mut output = manual::entries(&live, &context.window, &dates, now, &work)?;
    let (confirmed, admission_budget) =
        admitted::entries(store, &context, &records, &work, &materialized).await?;
    let over_budget = live.len() > MAX_SPANS || admission_budget;
    drop(records);
    drop(live);
    output.extend(confirmed);

    let mut status = forecast::Cache::load(store, &context, now, over_budget)
        .await?
        .publish(store, manual, visible, &mut output, &materialized)
        .await?;
    let base_ms = match status {
        Status::Ready { base_ms, .. } | Status::Incomplete { base_ms, .. } => base_ms,
        Status::Updating {} => now,
    };
    allocation::apply(store, query, &mut output, base_ms).await?;
    output.sort_by(compare_entries);
    let limit = query.limit.unwrap_or(MAX_SPANS).min(MAX_SPANS);
    if output.len() > limit {
        output.truncate(limit);
        status = incomplete(now, Incomplete::Budget {});
    }
    output.push(status_row(&status, &context.window.timezone));
    Ok(output)
}

fn allocation_order(row: &Value) -> Option<(i64, u64)> {
    let order = &row["origin"]["allocation_order"];
    Some((order[0].as_i64()?, order[1].as_u64()?))
}

fn compare_entries(a: &Value, b: &Value) -> Ordering {
    a["time"]["from_ms"]
        .as_i64()
        .cmp(&b["time"]["from_ms"].as_i64())
        .then_with(|| {
            allocation_order(a)
                .zip(allocation_order(b))
                .map_or(Ordering::Equal, |(a, b)| a.cmp(&b))
        })
        .then_with(|| a["uid"].as_str().cmp(&b["uid"].as_str()))
}

fn record_index(rows: &[Value]) -> HashMap<&str, &Value> {
    let _prepare = Span::new("protein-schedule-record-index");
    add(
        "protein-schedule-record-index-rows",
        u64::try_from(rows.len()).unwrap_or(u64::MAX),
    );
    rows.iter()
        .filter_map(|row| Some((row["uid"].as_str()?, row)))
        .collect()
}

fn incomplete(base_ms: i64, reason: Incomplete) -> Status {
    Status::Incomplete { base_ms, reason }
}

fn invalid(message: impl std::fmt::Display) -> ProteinError {
    StoreError::Protocol(format!("protein_schedule_invalid:{message}"))
}
