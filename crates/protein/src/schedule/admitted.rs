use super::*;
use nucleus::{
    karma::{ReferenceKind, TypedUid},
    projection::{Context, OccurrenceLink},
    simulation::RuleOccurrence,
};
use store::sqlx::Row;

pub(super) async fn entries(
    store: &store::Store,
    context: &Context,
    records: &HashMap<String, Value>,
    work: &HashMap<String, Value>,
    visible: Option<&HashSet<String>>,
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
    let materialized: HashSet<_> = work
        .iter()
        .filter(|(uid, _)| visible.is_none_or(|visible| visible.contains(*uid)))
        .filter_map(|(_, work)| work.get("projection_occurrence").cloned())
        .filter_map(|value| serde_json::from_value::<OccurrenceLink>(value).ok())
        .map(|link| {
            (
                link.record.as_str().to_owned(),
                link.occurrence.rule_uid,
                link.occurrence.revision,
                link.occurrence.event_id,
            )
        })
        .collect();
    let mut entries = Vec::new();
    let mut ids = HashSet::new();
    let empty = json!({});
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
        if materialized.contains(&(
            record_uid.clone(),
            occurrence.rule_uid.clone(),
            occurrence.revision,
            occurrence.event_id.clone(),
        )) {
            continue;
        }
        let work = work.get(&record_uid).unwrap_or(&empty);
        let Some(time) = nucleus::schedule::range(
            work["start"].as_str(),
            work["due"].as_str(),
            work["estimate_min"].as_f64(),
            Some(at),
        )
        .map_err(invalid)?
        else {
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
        let mut entry = records[&record_uid].clone();
        entry["uid"] = json!(id);
        entry["record_uid"] = json!(record_uid);
        entry["record_kind"] = entry["kind"].clone();
        entry["kind"] = json!("schedule-entry");
        entry["category"] = json!("timed");
        entry["time"] = json!(time);
        entry["preview"] = json!(false);
        entry["origin"] = json!({"kind":"manual", "occurrence":link});
        entries.push(entry);
    }
    Ok((entries, budget))
}
