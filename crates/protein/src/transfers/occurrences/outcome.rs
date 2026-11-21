use std::collections::HashSet;

use serde_json::{Value, json};
use store::{
    Store,
    transfer_evidence::{history, source_measures},
    transfers::TransferOccurrenceRow,
};

use crate::ProteinError;

pub(super) async fn read(
    store: &Store,
    occurrence: &TransferOccurrenceRow,
    visible: Option<&HashSet<String>>,
    create_person: Option<&str>,
) -> Result<(Value, Vec<Value>), ProteinError> {
    let outcome_history = history(&store.pool, &occurrence.uid, visible).await?;
    let mut outcome_measures = if let Some(record) = &occurrence.record_uid {
        source_measures(&store.pool, record, visible).await?
    } else {
        Vec::new()
    };
    let active = outcome_history["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| entry["superseded"] != true)
        .map(|entry| &entry["evidence"]);
    for evidence in active.clone() {
        let content = &evidence["content"];
        if content["kind"] == "need_binding" {
            let measure = &content["terms"]["measure"];
            outcome_measures.retain(|option| {
                !option["need_binding"].is_null() || &option["measure"] != measure
            });
            outcome_measures.push(json!({
                "label": format!("Need requested {} · {}", content["terms"]["requested"]["value"], evidence["record"]),
                "measure": measure,
                "need_binding": evidence["record"],
            }));
        }
    }
    for evidence in active {
        let content = &evidence["content"];
        if content["kind"] == "statement"
            && evidence["author"].as_str() == create_person
            && !outcome_measures
                .iter()
                .any(|option| option["measure"] == content["measure"])
        {
            outcome_measures.push(json!({
                "label": "Previously reported meaning",
                "measure": content["measure"],
                "need_binding": content["need_binding"],
            }));
        }
    }
    Ok((outcome_history, outcome_measures))
}
