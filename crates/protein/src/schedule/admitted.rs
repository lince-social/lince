use super::*;
use nucleus::{
    karma::{ReferenceKind, TypedUid},
    projection::{Context, OccurrenceLink},
    simulation::RuleOccurrence,
};
use std::collections::hash_map::Entry;
use store::sqlx::Row;

pub(super) async fn entries(
    store: &store::Store,
    context: &Context,
    records: &HashMap<&str, &Value>,
    work: &occurrences::Work<'_>,
    materialized: &occurrences::Materialized<'_, '_>,
) -> Result<(Vec<Value>, bool), ProteinError> {
    if records.is_empty() {
        return Ok((Vec::new(), false));
    }
    let allowed = serde_json::to_string(&records.keys().collect::<Vec<_>>()).map_err(invalid)?;
    let rows = store::sqlx::query("SELECT a.event_id, a.rule_uid, a.rule_revision, a.intended_at, a.frequency_uid, r.record_uid FROM karma_rule_application a JOIN recurrence r ON r.uid = a.rule_uid LEFT JOIN record_extension w ON w.record_uid = r.record_uid AND w.namespace = 'work' WHERE a.status = 'applied' AND r.record_uid IN (SELECT value FROM json_each(?)) AND ROUND(unixepoch(a.intended_at, 'subsec') * 1000) < ? AND (ROUND(unixepoch(a.intended_at, 'subsec') * 1000) >= ? OR ROUND(unixepoch(a.intended_at, 'subsec') * 1000) + COALESCE(ROUND((julianday(json_extract(w.fds, '$.due')) - julianday(json_extract(w.fds, '$.start'))) * 86400000), ROUND(json_extract(w.fds, '$.estimate_min') * 60000), 0) > ?) ORDER BY a.intended_at, a.event_id, a.rule_uid, a.rule_revision LIMIT ?")
        .bind(allowed)
        .bind(context.window.until_ms)
        .bind(context.window.from_ms)
        .bind(context.window.from_ms)
        .bind((nucleus::projection::MAX_SPANS + 1) as i64)
        .fetch_all(&store.pool)
        .await?;
    let budget = rows.len() > nucleus::projection::MAX_SPANS;
    let _publish = nucleus::performance::Span::new("protein-schedule-admitted-publication");
    let mut entries = Vec::new();
    let mut ids = HashSet::new();
    let mut endpoints = HashMap::new();
    for row in rows.into_iter().take(nucleus::projection::MAX_SPANS) {
        let record_uid: String = row.get("record_uid");
        let intended: String = row.get("intended_at");
        let at = chrono::DateTime::parse_from_rfc3339(&intended)
            .map_err(invalid)?
            .timestamp_millis();
        let frequency: Option<String> = row.get("frequency_uid");
        let occurrence = RuleOccurrence {
            rule_uid: row.get("rule_uid"),
            revision: u64::try_from(row.get::<i64, _>("rule_revision")).map_err(invalid)?,
            event_id: row.get("event_id"),
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
                    nucleus::performance::add("protein-schedule-endpoint-reuses", 1);
                    entry.into_mut()
                }
                Entry::Vacant(entry) => entry.insert(
                    Endpoints::parse(work["start"].as_str(), work["due"].as_str())
                        .map_err(invalid)?,
                ),
            };
            endpoints.range(work["estimate_min"].as_f64(), Some(at))
        } else {
            nucleus::schedule::range(None, None, None, Some(at))
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
        let mut entry = Value::clone(records[record_uid.as_str()]);
        entry["uid"] = json!(id);
        entry["record_uid"] = json!(record_uid);
        let kind = entry["kind"].take();
        entry["record_kind"] = kind;
        entry["kind"] = json!("schedule-entry");
        entry["category"] = json!("timed");
        entry["time"] = json!(time);
        entry["preview"] = json!(false);
        entry["origin"] = json!({"kind":"manual", "occurrence":link});
        entries.push(entry);
    }
    Ok((entries, budget))
}
