use std::collections::{HashMap, HashSet};

use nucleus::performance::Span;
use serde_json::{Map, Value, json};
use store::{
    Store,
    assertions::for_subjects,
    records::{RecordRow, live_heads_for},
    vocabulary::feature_binding,
};

use crate::{Protein, ProteinError};

use super::{requests, work::WorkFields};

pub(crate) async fn attach<Body>(
    store: &Store,
    query: &Protein,
    records: &[RecordRow<Body>],
    visible: Option<&HashSet<String>>,
    work: &mut HashMap<String, Value>,
    concept_names: &HashMap<String, String>,
) -> Result<HashMap<String, Value>, ProteinError> {
    let _schema = Span::new("protein-record-schema-await");
    let work_fields = WorkFields::selected(query);
    let want_assertions = requests(query, "assertions");
    let want_assignees = requests(query, "assignees");
    if !(work_fields.any() || want_assertions || want_assignees) {
        return Ok(HashMap::new());
    }

    let mut log_ids = work_fields.log_ids(store, records, work).await?;
    let mut out = HashMap::with_capacity(records.len());
    for record in records {
        let _publish = Span::new("protein-record-work-publish");
        let mut value = Map::new();
        work_fields.append(
            &mut value,
            work.get_mut(&record.uid),
            log_ids.remove(&record.uid),
        )?;
        for (key, requested) in [
            ("assertions", want_assertions),
            ("assignees", want_assignees),
        ] {
            if requested {
                value.insert(key.into(), Value::Array(Vec::new()));
            }
        }
        out.insert(record.uid.clone(), Value::Object(value));
    }

    if !want_assertions && !want_assignees {
        return Ok(out);
    }
    let assignment: HashSet<_> = if want_assignees {
        feature_binding(&store.pool, "assignment")
            .await?
            .concept_uids
            .into_iter()
            .collect()
    } else {
        HashSet::new()
    };
    let mut assertions = Vec::new();
    for chunk in records.chunks(400) {
        let ids = chunk.iter().map(|row| row.uid.clone()).collect::<Vec<_>>();
        assertions.extend(
            for_subjects(&store.pool, &ids)
                .await?
                .into_iter()
                .filter(|assertion| {
                    visible.is_none_or(|visible| {
                        assertion
                            .object_uid
                            .as_ref()
                            .is_none_or(|uid| visible.contains(uid))
                    })
                }),
        );
    }
    let assignees = if want_assignees {
        let people: HashSet<_> = assertions
            .iter()
            .filter(|assertion| assignment.contains(&assertion.predicate_uid))
            .filter_map(|assertion| assertion.object_uid.as_deref())
            .collect();
        live_heads_for(&store.pool, &people.into_iter().collect::<Vec<_>>()).await?
    } else {
        HashMap::new()
    };
    for assertion in assertions {
        let Some(row) = out.get_mut(&assertion.subject_uid) else {
            continue;
        };
        if assignment.contains(&assertion.predicate_uid)
            && let Some(uid) = assertion.object_uid.as_deref()
            && let Some(head) = assignees.get(uid)
            && let Some(Value::Array(assignees)) = row.get_mut("assignees")
        {
            assignees.push(json!({
                "uid": uid,
                "head": head,
                "assertion": assertion.uid,
                "predicate_uid": assertion.predicate_uid,
            }));
        }
        if let Some(Value::Array(entries)) = row.get_mut("assertions") {
            let predicate = concept_names
                .get(&assertion.predicate_uid)
                .map_or(assertion.predicate.as_str(), String::as_str);
            entries.push(Value::from_iter([
                ("uid", Value::from(assertion.uid)),
                ("role", assertion.role.into()),
                ("predicate", predicate.into()),
                ("predicate_uid", assertion.predicate_uid.into()),
                ("object", assertion.object_uid.into()),
                ("uri", assertion.object_uri.into()),
                ("resolved_object", assertion.resolved_object_uid.into()),
                (
                    "quantity",
                    assertion
                        .quantity
                        .map(|quantity| quantity.to_string())
                        .into(),
                ),
                ("unit", assertion.unit_uid.into()),
            ]));
        }
    }
    Ok(out)
}
