use std::collections::{HashMap, HashSet};

use nucleus::{
    DecimalValue,
    schedule::{
        TimeRange,
        allocation::{Work, allocate},
        range,
    },
};
use serde::Deserialize;
use serde_json::{Value, json};
use store::Store;

use super::invalid;
use crate::{Order, Protein, ProteinError, assertion_order};

fn quantity(row: &Value) -> Option<DecimalValue> {
    DecimalValue::parse_inferred(row["quantity"].as_str()?).ok()
}

pub(super) async fn apply(
    store: &Store,
    query: &Protein,
    output: &mut Vec<Value>,
    base_ms: i64,
) -> Result<(), ProteinError> {
    let (fields, descending): (Vec<_>, Vec<_>) = query
        .order
        .iter()
        .filter_map(|order| match order {
            Order::Asc(field) if field.starts_with('#') => Some((field.as_str(), false)),
            Order::Desc(field) if field.starts_with('#') => Some((field.as_str(), true)),
            _ => None,
        })
        .unzip();
    if fields.is_empty() {
        return Ok(());
    }
    let records: Vec<_> = output
        .iter()
        .filter_map(|row| row["record_uid"].as_str())
        .collect();
    let values = assertion_order::values(store, &records, &fields).await?;
    let cursor_records: HashSet<_> = output
        .iter()
        .filter(|row| {
            row["category"] == "overdue" && quantity(row).is_some_and(|value| value.is_negative())
        })
        .filter_map(|row| row["record_uid"].as_str())
        .collect();
    let mut work = Vec::new();
    let mut origins = HashMap::<_, (i64, bool)>::new();
    let mut completed = HashSet::new();
    for row in output.iter() {
        let uid = row["uid"].as_str().unwrap_or_default();
        let record = row["record_uid"].as_str().unwrap_or_default();
        let quantity = quantity(row);
        let cursor =
            row["category"] == "overdue" && quantity.is_some_and(|value| value.is_negative());
        if !cursor
            && row["preview"] != true
            && row["origin"]["kind"] != "projection"
            && quantity.is_some_and(|value| value.is_zero())
        {
            completed.insert(uid.to_owned());
            continue;
        }
        let time = TimeRange::deserialize(&row["time"]).ok();
        if !cursor
            && cursor_records.contains(record)
            && time.as_ref().is_some_and(|time| time.from_ms <= base_ms)
        {
            completed.insert(uid.to_owned());
            continue;
        }
        let (release_ms, duration_ms) = if cursor {
            let duration = range(
                row["start_date"].as_str(),
                row["due_date"].as_str(),
                row["estimate_min"].as_f64(),
                Some(base_ms),
            )
            .map_err(invalid)?
            .map_or(0, |time| time.until_ms.unwrap_or(base_ms) - base_ms);
            (base_ms, duration)
        } else {
            let Some(time) = time.filter(|time| time.from_ms >= base_ms) else {
                continue;
            };
            (
                time.from_ms,
                time.until_ms.unwrap_or(time.from_ms) - time.from_ms,
            )
        };
        let origin = origins.entry(uid).or_default();
        *origin = (release_ms, origin.1 || cursor);
        work.push(Work {
            id: uid.to_owned(),
            release_ms,
            duration_ms,
            priority: fields
                .iter()
                .map(|field| {
                    values
                        .get(field)
                        .and_then(|values| values.get(record))
                        .copied()
                })
                .collect(),
        });
    }
    drop(cursor_records);
    let allocations = allocate(work, &descending).map_err(invalid)?;
    let mut allocated = HashMap::with_capacity(allocations.len());
    let mut previous_release = None;
    let mut position = 0usize;
    for (id, time) in allocations {
        let &(release, cursor) = origins
            .get(id.as_str())
            .ok_or_else(|| invalid("Allocation returned unknown Schedule work"))?;
        if previous_release == Some(release) {
            position += 1;
        } else {
            position = 0;
        }
        previous_release = Some(release);
        let order = (if cursor { 0 } else { release }, position);
        allocated.insert(id, (time, order, cursor));
    }
    drop(origins);
    output.retain(|row| !completed.contains(row["uid"].as_str().unwrap_or_default()));
    for row in output {
        let id = row["uid"].as_str().unwrap_or_default();
        if let Some((time, order, cursor)) = allocated.get(id) {
            row["origin"]["allocation_order"] = json!([order.0, order.1]);
            if *cursor {
                row["origin"]["allocation"] = json!({"cursor_offset_ms":time.from_ms - base_ms,"duration_ms":time.until_ms.unwrap_or(time.from_ms) - time.from_ms});
            }
            row["time"] = json!(time);
            row["category"] = json!("timed");
        }
    }
    Ok(())
}
