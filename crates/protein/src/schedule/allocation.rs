use super::*;
use crate::Order;
use nucleus::schedule::allocation::{Work, allocate};

pub(super) async fn apply(
    store: &store::Store,
    query: &Protein,
    output: &mut Vec<Value>,
    base_ms: i64,
) -> Result<(), ProteinError> {
    let hierarchy: Vec<_> = query
        .order
        .iter()
        .filter_map(|order| match order {
            Order::Asc(field) => field.strip_prefix('#').map(|name| (name, false)),
            Order::Desc(field) => field.strip_prefix('#').map(|name| (name, true)),
            Order::Link(_) => None,
        })
        .collect();
    if hierarchy.is_empty() {
        return Ok(());
    }
    let records: Vec<_> = output
        .iter()
        .filter_map(|row| row["record_uid"].as_str().map(str::to_owned))
        .collect();
    let fields: Vec<_> = hierarchy
        .iter()
        .map(|(name, _)| format!("#{name}"))
        .collect();
    let values = crate::assertion_order::values(store, &records, &fields).await?;
    let cursor_records: HashSet<_> = output
        .iter()
        .filter(|row| {
            row["category"] == "overdue"
                && row["quantity"]
                    .as_str()
                    .and_then(|value| nucleus::DecimalValue::parse_inferred(value).ok())
                    .is_some_and(|value| value.is_negative())
        })
        .filter_map(|row| row["record_uid"].as_str().map(str::to_owned))
        .collect();
    let mut work = Vec::new();
    let mut cursor_ids = HashSet::new();
    let mut completed = HashSet::new();
    for row in output.iter() {
        let uid = row["uid"].as_str().unwrap_or_default();
        let record = row["record_uid"].as_str().unwrap_or_default();
        let quantity = row["quantity"]
            .as_str()
            .and_then(|value| nucleus::DecimalValue::parse_inferred(value).ok());
        let cursor =
            row["category"] == "overdue" && quantity.is_some_and(|value| value.is_negative());
        let time = serde_json::from_value::<nucleus::schedule::TimeRange>(row["time"].clone()).ok();
        if !cursor
            && row["preview"] != true
            && row["origin"]["kind"] != "projection"
            && quantity.is_some_and(|value| value.is_zero())
        {
            completed.insert(uid.to_owned());
            continue;
        }
        if !cursor
            && cursor_records.contains(record)
            && time.as_ref().is_some_and(|time| time.from_ms <= base_ms)
        {
            completed.insert(uid.to_owned());
            continue;
        }
        if !cursor && !time.as_ref().is_some_and(|time| time.from_ms >= base_ms) {
            continue;
        }
        let duration = if cursor {
            nucleus::schedule::range(
                row["start_date"].as_str(),
                row["due_date"].as_str(),
                row["estimate_min"].as_f64(),
                Some(base_ms),
            )
            .map_err(invalid)?
            .map_or(0, |time| time.until_ms.unwrap_or(base_ms) - base_ms)
        } else {
            let time = time.as_ref().unwrap();
            time.until_ms.unwrap_or(time.from_ms) - time.from_ms
        };
        if cursor {
            cursor_ids.insert(uid.to_owned());
        }
        work.push(Work {
            id: uid.to_owned(),
            release_ms: if cursor {
                base_ms
            } else {
                time.unwrap().from_ms
            },
            duration_ms: duration,
            priority: fields
                .iter()
                .map(|field| values.get(&(record.to_owned(), field.clone())).copied())
                .collect(),
        });
    }
    let releases: HashMap<_, _> = work
        .iter()
        .map(|work| (work.id.clone(), work.release_ms))
        .collect();
    let mut previous_release = None;
    let mut position = 0usize;
    let allocated: HashMap<_, _> = allocate(
        work,
        &hierarchy
            .iter()
            .map(|(_, descending)| *descending)
            .collect::<Vec<_>>(),
    )
    .map_err(invalid)?
    .into_iter()
    .map(|(id, time)| {
        let release = releases[&id];
        if previous_release == Some(release) {
            position += 1;
        } else {
            position = 0;
        }
        previous_release = Some(release);
        let order = (if cursor_ids.contains(&id) { 0 } else { release }, position);
        (id, (time, order))
    })
    .collect();
    output.retain(|row| !completed.contains(row["uid"].as_str().unwrap_or_default()));
    for row in output {
        let id = row["uid"].as_str().unwrap_or_default();
        if let Some((time, order)) = allocated.get(id) {
            let cursor = cursor_ids.contains(id);
            row["origin"]["allocation_order"] = json!([order.0, order.1]);
            if cursor {
                row["origin"]["allocation"] = json!({"cursor_offset_ms":time.from_ms - base_ms,"duration_ms":time.until_ms.unwrap_or(time.from_ms) - time.from_ms});
            }
            row["time"] = json!(time);
            row["category"] = json!("timed");
        }
    }
    Ok(())
}
