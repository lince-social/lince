use std::{collections::BTreeSet, sync::LazyLock};

use serde::Deserialize;
use serde_json::{Value, json};

use super::{Context, error};

static ACTIONS: LazyLock<Value> = LazyLock::new(|| {
    serde_json::to_value(schemars::schema_for!(engine::actions::Action)).expect("Action schema")
});
static PROTEIN: LazyLock<Value> = LazyLock::new(|| {
    serde_json::to_value(schemars::schema_for!(protein::Protein)).expect("Protein schema")
});

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    action: Option<String>,
}

pub(super) fn describe(arguments: Value, context: &Context) -> Result<Value, String> {
    let args: Arguments = serde_json::from_value(arguments).map_err(error)?;
    let variants = ACTIONS["oneOf"]
        .as_array()
        .or_else(|| ACTIONS["anyOf"].as_array())
        .ok_or("Action schema has no variants.")?;
    if let Some(name) = args.action {
        let mut schema = variants
            .iter()
            .find(|variant| variant["properties"]["action"]["const"] == name)
            .cloned()
            .ok_or("Unknown Action name. Use lince_describe without an action to list names.")?;
        let mut references = BTreeSet::new();
        collect(&schema, &mut references);
        let mut definitions = serde_json::Map::new();
        while let Some(name) = references.pop_first() {
            if definitions.contains_key(&name) {
                continue;
            }
            let value = ACTIONS["$defs"][&name].clone();
            collect(&value, &mut references);
            definitions.insert(name, value);
        }
        if !definitions.is_empty() {
            schema["$defs"] = definitions.into();
        }
        return Ok(
            json!({"schema":schema,"new_record_uid":nucleus::new_uid("r"),"new_change_id":nucleus::new_uid("op"),"new_request_id":nucleus::new_uid("request")}),
        );
    }
    let names: Vec<_> = variants
        .iter()
        .filter_map(|variant| variant["properties"]["action"]["const"].as_str())
        .collect();
    Ok(json!({
        "context":context,
        "record_fields":protein::record_schema::fields(),
        "protein_schema":*PROTEIN,
        "actions":names,
        "instructions":[
            "Use Protein queries for every read. The normal Session and engine decide what is visible and writable.",
            "Use stable Record UIDs. Read Records before changing them. Request individual Action schemas when needed.",
            "Action schema responses supply fresh IDs for actions that require a new Record UID or change ID. Never invent a UID for an existing Record. Protein and Action tool arguments are JSON-encoded strings.",
            "Text edits use lince_edit_text, never edit-record-text or raw CRDT bytes. Snapshot edits preserve concurrent insertions; read the merged result before claiming completion.",
            "Read IDs and request receipts last for this turn only. Query existing effects before retrying in a new turn.",
            "For external agents, a tool connection is the turn scope. Use lince_release_reads to free snapshots after edits. Reopen the connection when its operation budget is reached; inspect uncertain effects before retrying.",
            "Use lince_message for replies in the context thread. Start, update and finish the same message with your complete generated text; Lince applies only your CRDT differences and preserves human edits. Connection closure interrupts messages still writing.",
            "Change assertions and assignees with assert-record/retract-record. Assignees use the assigned-to relation; query its Concept and Person UIDs first.",
            "For board state, read the board's configured enter/leave effects. Use preview-area-transition and apply-area-transition to change assertions and exact quantity together. A stale preview must be read and reconsidered.",
            "The default Kanban columns are backlog=0, todo=-1, next=-2, wip=-3, review=-4, done=1, documented=2. Custom boards may differ.",
            "File sync is optional. Query Organ Records with include.extension.namespace=lince.file_sync, then file-sync-status. Use lince_sync_status for activity. Saved, exported and delivered are distinct outcomes.",
            "Tool availability does not grant extra permissions or signing keys. Backend refusal is final for that operation; report the reason."
        ],
        "examples":{
            "search":{"source":"record","where":[{"text_contains":"work"}],"fields":["uid","head","quantity_exact","assertions","assignees"],"limit":20},
            "proteins":{"source":"record","where":[{"kind_eq":"protein"}],"include":{"extension":{"namespace":"lince.protein"}},"limit":20},
            "file_sync":{"source":"record","where":[{"kind_eq":"organ"}],"include":{"extension":{"namespace":"lince.file_sync"}},"limit":20}
        }
    }))
}

fn collect(value: &Value, references: &mut BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            if let Some(name) = object
                .get("$ref")
                .and_then(Value::as_str)
                .and_then(|reference| reference.strip_prefix("#/$defs/"))
            {
                references.insert(name.into());
            }
            for value in object.values() {
                collect(value, references);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect(value, references);
            }
        }
        _ => {}
    }
}
