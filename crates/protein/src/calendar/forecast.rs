use std::collections::{HashMap, HashSet};

use nucleus::{
    execution::now,
    performance::{Span, add},
    projection::{Context, Dates, Incomplete, OccurrenceLink, Status},
    simulation::Cause,
};
use serde::Deserialize;
use serde_json::{Value, json};
use store::{Store, projection::read_calendar, records::extensions_for_records};

use crate::{
    Protein, ProteinError, execute_records,
    projection::{matches_quantity, split},
};

const MAX_FORECAST_MS: i64 = 366 * 86_400_000;

pub(super) async fn append(
    store: &Store,
    context: &Context,
    dates: &Dates,
    mut manual: Protein,
    visible: Option<&HashSet<String>>,
    rows: &mut Vec<Value>,
) -> Result<Status, ProteinError> {
    let at = now().timestamp_millis();
    let unavailable = if context.actor.is_some() {
        Some(Incomplete::UnavailableRuntime {})
    } else if context.window.until_ms <= at {
        Some(Incomplete::PastWindow {})
    } else if context.window.until_ms.saturating_sub(at) > MAX_FORECAST_MS {
        Some(Incomplete::Budget {})
    } else {
        None
    };
    if let Some(reason) = unavailable {
        return Ok(Status::Incomplete {
            base_ms: at,
            reason,
        });
    }
    let Some((status, spans)) = read_calendar(store, context, at).await? else {
        return Ok(Status::Updating {});
    };
    let Some(quantities) = split(&mut manual.filter) else {
        return Ok(Status::Incomplete {
            base_ms: now().timestamp_millis(),
            reason: Incomplete::UnsupportedFilter {},
        });
    };
    let records = execute_records(store, &manual, visible, context.actor.as_deref()).await?;
    let records: HashMap<_, _> = records
        .iter()
        .filter_map(|row| Some((row["uid"].as_str()?, row)))
        .collect();
    let manual_records = rows
        .iter()
        .filter_map(|row| row["uid"].as_str().map(String::from))
        .collect::<Vec<_>>();
    let links = {
        let _read = Span::new("protein-calendar-work-links-read-await");
        let links = extensions_for_records(&store.pool, "work", &manual_records).await?;
        add(
            "protein-calendar-work-rows",
            u64::try_from(links.len()).unwrap_or(u64::MAX),
        );
        links
    };
    let prepare = Span::new("protein-calendar-occurrence-index");
    let actual_links: Vec<_> = manual_records
        .iter()
        .filter_map(|uid| {
            let value = links.get(uid)?.get("projection_occurrence")?;
            value.as_object()?;
            OccurrenceLink::deserialize(value).ok()
        })
        .collect();
    let actual: HashSet<_> = actual_links
        .iter()
        .map(|link| (&link.record, &link.occurrence, link.consequence))
        .collect();
    add(
        "protein-calendar-occurrence-links",
        u64::try_from(actual_links.len()).unwrap_or(u64::MAX),
    );
    drop(prepare);

    let _project = Span::new("protein-calendar-projection");
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
        if let Cause::Rule {
            occurrence,
            consequence,
        } = &span.cause
        {
            add("protein-calendar-occurrence-lookups", 1);
            if actual.contains(&(&span.record, occurrence, *consequence)) {
                continue;
            }
        }
        let mut row = Value::clone(record);
        row["uid"] = Value::String(format!("projection:{}", span.id));
        row["record_uid"] = json!(span.record.as_str());
        row["quantity"] = Value::String(span.quantity.value.to_string());
        row["start_date"] = dates.at(span.from_ms).map_or(Value::Null, Value::String);
        row["due_date"] = dates
            .at(span.until_ms - 1)
            .map_or(Value::Null, Value::String);
        row["origin"] = json!({ "kind": "projection", "observation": span.id, "from_ms": span.from_ms, "until_ms": span.until_ms, "cause": span.cause });
        rows.push(row);
    }
    Ok(status)
}
