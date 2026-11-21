use nucleus::{
    DecimalValue,
    performance::{Span, add},
    projection::{Dates, Window},
    schedule::{Endpoints, TimeValue},
};
use serde_json::{Value, json};

use super::{ProteinError, invalid, occurrences::Work};

pub(super) fn entries(
    live: &[Value],
    window: &Window,
    dates: &Dates,
    now: i64,
    work: &Work<'_>,
) -> Result<Vec<Value>, ProteinError> {
    let from_date = dates
        .at(window.from_ms)
        .ok_or_else(|| invalid("Invalid schedule window"))?;
    let until_date = dates
        .at(window.until_ms - 1)
        .ok_or_else(|| invalid("Invalid schedule window"))?;
    let today = dates
        .at(now)
        .ok_or_else(|| invalid("Invalid current time"))?;
    let _publish = Span::new("protein-schedule-manual-publication");
    let mut output = Vec::new();
    for row in live {
        let endpoints = Endpoints::parse(row["start_date"].as_str(), row["due_date"].as_str())
            .map_err(invalid)?;
        let start = endpoints.start();
        let due = endpoints.due();
        let time = endpoints
            .range(row["estimate_min"].as_f64(), None)
            .map_err(invalid)?;
        let quantity = row["quantity"]
            .as_str()
            .and_then(|value| DecimalValue::parse_inferred(value).ok());
        let needed = quantity.is_some_and(|quantity| quantity.is_negative());
        let undated_need = needed && start.is_none() && due.is_none();
        let expired_need = needed
            && time
                .as_ref()
                .is_some_and(|time| time.until_ms.unwrap_or(time.from_ms) < now);
        let overdue = quantity.is_some_and(|quantity| !quantity.is_zero())
            && due.is_some_and(|due| match due {
                TimeValue::Instant(time) => time.timestamp_millis() < now,
                TimeValue::Date(date) => date.to_string() < today,
            });
        let category = if overdue || undated_need || expired_need {
            "overdue"
        } else if let Some(time) = &time {
            if !time.overlaps(window.from_ms, window.until_ms) {
                continue;
            }
            "timed"
        } else {
            let endpoints = start.or(due).zip(due.or(start));
            if !endpoints.is_some_and(|(start, end)| {
                dates.value(start).to_string() <= until_date
                    && dates.value(end).to_string() >= from_date
            }) {
                continue;
            }
            "all-day"
        };
        let uid = row["uid"].as_str().unwrap_or_default();
        // Admitted occurrences still need the original Record row after manual publication.
        let mut row = row.clone();
        add("protein-schedule-manual-row-copies", 1);
        row["record_uid"] = json!(uid);
        row["uid"] = Value::String(format!("manual:{uid}"));
        row["record_kind"] = row["kind"].take();
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
    Ok(output)
}
