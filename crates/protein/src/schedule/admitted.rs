use std::collections::{HashMap, HashSet, hash_map::Entry};

use chrono::DateTime;
use nucleus::{
    karma::{ReferenceKind, TypedUid},
    performance::{Span, add},
    projection::{Context, MAX_SPANS, OccurrenceLink},
    schedule::{Endpoints, range},
    simulation::RuleOccurrence,
};
use serde_json::{Value, json};
use store::{Store, sqlx::query_as};

use super::{
    ProteinError, invalid,
    occurrences::{Materialized, Work},
};

pub(super) async fn entries(
    store: &Store,
    context: &Context,
    records: &HashMap<&str, &Value>,
    work: &Work<'_>,
    materialized: &Materialized<'_, '_>,
) -> Result<(Vec<Value>, bool), ProteinError> {
    if records.is_empty() {
        return Ok((Vec::new(), false));
    }
    let allowed = serde_json::to_string(&records.keys().collect::<Vec<_>>()).map_err(invalid)?;
    type Application = (String, String, i64, String, Option<String>, String);
    let rows = query_as::<_, Application>("SELECT a.event_id, a.rule_uid, a.rule_revision, a.intended_at, a.frequency_uid, r.record_uid FROM karma_rule_application a JOIN recurrence r ON r.uid = a.rule_uid LEFT JOIN record_extension w ON w.record_uid = r.record_uid AND w.namespace = 'work' WHERE a.status = 'applied' AND r.record_uid IN (SELECT value FROM json_each(?)) AND ROUND(unixepoch(a.intended_at, 'subsec') * 1000) < ? AND (ROUND(unixepoch(a.intended_at, 'subsec') * 1000) >= ? OR ROUND(unixepoch(a.intended_at, 'subsec') * 1000) + COALESCE(ROUND((julianday(json_extract(w.fds, '$.due')) - julianday(json_extract(w.fds, '$.start'))) * 86400000), ROUND(json_extract(w.fds, '$.estimate_min') * 60000), 0) > ?) ORDER BY a.intended_at, a.event_id, a.rule_uid, a.rule_revision LIMIT ?")
        .bind(allowed)
        .bind(context.window.until_ms)
        .bind(context.window.from_ms)
        .bind(context.window.from_ms)
        .bind(i64::try_from(MAX_SPANS + 1).map_err(invalid)?)
        .fetch_all(&store.pool)
        .await?;
    let budget = rows.len() > MAX_SPANS;
    let _publish = Span::new("protein-schedule-admitted-publication");
    let mut entries = Vec::new();
    let mut ids = HashSet::new();
    let mut endpoints = HashMap::new();
    for (event_id, rule_uid, revision, intended, frequency, record_uid) in
        rows.into_iter().take(MAX_SPANS)
    {
        let at = DateTime::parse_from_rfc3339(&intended)
            .map_err(invalid)?
            .timestamp_millis();
        let occurrence = RuleOccurrence {
            rule_uid,
            revision: u64::try_from(revision).map_err(invalid)?,
            event_id,
            frequency: frequency
                .map(|uid| TypedUid::new(ReferenceKind::Frequency, uid))
                .transpose()
                .map_err(invalid)?,
            intended_at_ms: Some(at),
        };
        if materialized.contains(&record_uid, &occurrence) {
            continue;
        }
        let time = if let Some((uid, work)) = work.row(&record_uid) {
            let endpoints = match endpoints.entry(uid) {
                Entry::Occupied(entry) => {
                    add("protein-schedule-endpoint-reuses", 1);
                    entry.into_mut()
                }
                Entry::Vacant(entry) => entry.insert(
                    Endpoints::parse(work["start"].as_str(), work["due"].as_str())
                        .map_err(invalid)?,
                ),
            };
            endpoints.range(work["estimate_min"].as_f64(), Some(at))
        } else {
            range(None, None, None, Some(at))
        };
        let Some(time) = time.map_err(invalid)? else {
            continue;
        };
        if !time.overlaps(context.window.from_ms, context.window.until_ms) {
            continue;
        }
        let id = format!(
            "occurrence:{}:{}:{}:{}",
            occurrence.rule_uid, occurrence.revision, occurrence.event_id, record_uid
        );
        if !ids.insert(id.clone()) {
            continue;
        }
        let link = OccurrenceLink {
            record: TypedUid::new(ReferenceKind::Record, &record_uid).map_err(invalid)?,
            occurrence,
            consequence: 0,
        };
        let mut entry = Value::clone(
            records
                .get(record_uid.as_str())
                .ok_or_else(|| invalid("Admitted work references an unselected Record"))?,
        );
        entry["uid"] = Value::String(id);
        entry["record_uid"] = Value::String(record_uid);
        entry["record_kind"] = entry["kind"].take();
        entry["kind"] = json!("schedule-entry");
        entry["category"] = json!("timed");
        entry["time"] = json!(time);
        entry["preview"] = json!(false);
        entry["origin"] = json!({"kind":"manual", "occurrence":link});
        entries.push(entry);
    }
    Ok((entries, budget))
}
