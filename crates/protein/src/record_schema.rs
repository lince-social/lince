use crate::Protein;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use store::{Store, records::RecordRow};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Field {
    pub key: &'static str,
    pub title: &'static str,
    pub kind: &'static str,
    pub editable: bool,
}

pub fn fields() -> Vec<Field> {
    [
        ("head", "Title", "text", true),
        ("body", "Description", "text", true),
        ("slug", "Slug", "text", true),
        ("quantity", "Quantity", "decimal", true),
        ("assertions", "Assertions", "assertions", true),
        ("kind", "Kind", "text", false),
        ("assignees", "Assignees", "records", true),
        ("start_date", "Start date", "date", true),
        ("due_date", "Due date", "date", true),
        ("estimate_min", "Estimate (minutes)", "number", true),
        ("spent_seconds", "Time spent (seconds)", "number", false),
        ("running_since", "Running since", "timestamp", false),
        ("work_logs", "Work logs", "logs", true),
        ("work_timer", "Work timer", "timer", true),
        ("threads", "Threads and messages", "threads", true),
        ("created_at", "Created", "timestamp", false),
        ("updated_at", "Updated", "timestamp", false),
    ]
    .into_iter()
    .map(|(key, title, kind, editable)| Field {
        key,
        title,
        kind,
        editable,
    })
    .collect()
}

pub fn selected(protein: &Protein) -> Vec<Field> {
    fields()
        .into_iter()
        .filter(|field| {
            protein.fields.as_ref().is_none_or(|fields| {
                fields.iter().any(|key| key == field.key) || field.key == "kind"
            })
        })
        .collect()
}

pub(crate) async fn attach(
    store: &Store,
    query: &Protein,
    records: &[RecordRow],
    visible: Option<&HashSet<String>>,
    work: &HashMap<String, Value>,
) -> Result<HashMap<String, Value>, crate::ProteinError> {
    let wants = |key: &str| {
        query
            .fields
            .as_ref()
            .is_none_or(|fields| fields.iter().any(|field| field == key))
    };
    let mut out = HashMap::new();
    for record in records {
        let mut value = json!({});
        let metadata = work.get(&record.uid);
        if wants("estimate_min") {
            value["estimate_min"] = metadata
                .and_then(|work| work.get("estimate_min"))
                .cloned()
                .unwrap_or(Value::Null);
        }
        let logs = metadata
            .and_then(|work| work["logs"].as_array())
            .cloned()
            .unwrap_or_default();
        if wants("work_logs") {
            let entries: Vec<(String, String)> = store::sqlx::query_as("SELECT property, value FROM record_property WHERE record_uid = ? AND property LIKE 'work.log:%' AND json_type(value) = 'object' ORDER BY json_extract(value, '$.start'), property")
                .bind(&record.uid).fetch_all(&store.pool).await?;
            let mut identified = logs.clone();
            for (index, log) in identified.iter_mut().enumerate() {
                log["id"] = json!(
                    entries
                        .get(index)
                        .map(|entry| entry.0.clone())
                        .unwrap_or_else(|| format!("work.log:seed-{index}"))
                );
            }
            value["work_logs"] = json!(identified);
        }
        let mut spent = 0i64;
        let mut running = Value::Null;
        for log in &logs {
            let Some(start) = log["start"]
                .as_str()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            else {
                continue;
            };
            let end = log["end"]
                .as_str()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok());
            if end.is_none() {
                running = log["start"].clone();
            }
            spent = spent.saturating_add(
                (end.map(|v| v.with_timezone(&chrono::Utc))
                    .unwrap_or_else(chrono::Utc::now)
                    - start.with_timezone(&chrono::Utc))
                .num_seconds()
                .max(0),
            );
        }
        if wants("spent_seconds") {
            value["spent_seconds"] = json!(spent);
        }
        if wants("running_since") {
            value["running_since"] = running;
        }
        if wants("assertions") {
            value["assertions"] = json!([]);
        }
        if wants("assignees") {
            value["assignees"] = json!([]);
        }
        out.insert(record.uid.clone(), value);
    }
    if wants("assertions") || wants("assignees") {
        let mut assignees = HashMap::<String, Option<String>>::new();
        let ids: Vec<_> = records.iter().map(|r| r.uid.clone()).collect();
        for chunk in ids.chunks(400) {
            for assertion in store::assertions::for_subjects(&store.pool, chunk).await? {
                if visible.is_some_and(|visible| {
                    assertion
                        .object_uid
                        .as_ref()
                        .is_some_and(|uid| !visible.contains(uid))
                }) {
                    continue;
                }
                let Some(row) = out.get_mut(&assertion.subject_uid) else {
                    continue;
                };
                if wants("assignees") && assertion.predicate == "assigned-to" {
                    if let Some(uid) = assertion.object_uid.as_deref() {
                        if !assignees.contains_key(uid) {
                            assignees.insert(
                                uid.into(),
                                store::records::get(&store.pool, uid)
                                    .await?
                                    .map(|person| person.head),
                            );
                        }
                        if let Some(head) = assignees.get(uid).and_then(Option::as_ref) {
                            row["assignees"]
                                .as_array_mut()
                                .unwrap()
                                .push(json!({"uid":uid,"head":head,"assertion":assertion.uid}));
                        }
                    }
                }
                if wants("assertions") {
                    row["assertions"].as_array_mut().unwrap().push(json!({"uid":assertion.uid,"role":assertion.role,"predicate":assertion.predicate,"predicate_uid":assertion.predicate_uid,"object":assertion.object_uid,"quantity":assertion.quantity.map(|q| q.to_string()),"unit":assertion.unit_uid}));
                }
            }
        }
    }
    Ok(out)
}
