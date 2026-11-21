//! Action and query schemas exposed by the native tool catalogue.

use std::{collections::BTreeSet, sync::LazyLock};

use engine::actions::Action;
use nucleus::{canvas::Component, new_uid};
use protein::{Protein, record_schema::FIELDS};
use schemars::schema_for;
use serde::Deserialize;
use serde_json::{Map, Result as JsonResult, Value, from_value, json, to_value};

use super::{Context, error};
use guide::{CANVAS_CONTEXT, FIOTE_WORKFLOWS, INSTRUCTIONS};

mod guide;
mod tools;

// Schemars serializes keywords in a deliberate order when JSON preserves insertion order.
static ACTIONS: LazyLock<JsonResult<Value>> = LazyLock::new(|| to_value(schema_for!(Action)));
static PROTEIN: LazyLock<JsonResult<Value>> = LazyLock::new(|| to_value(schema_for!(Protein)));
static CANVAS: LazyLock<JsonResult<Value>> = LazyLock::new(|| to_value(schema_for!(Component)));

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    action: Option<String>,
}

pub(super) fn describe(arguments: Value, context: &Context) -> Result<Value, String> {
    let args: Arguments = from_value(arguments).map_err(error)?;
    let actions = ACTIONS.as_ref().map_err(error)?;
    let variants = actions
        .get("oneOf")
        .and_then(Value::as_array)
        .or_else(|| actions.get("anyOf").and_then(Value::as_array))
        .ok_or("Action schema has no variants.")?;
    if let Some(name) = args.action.as_deref() {
        let schema = action_schema(variants, actions.get("$defs"), name)?;
        return Ok(Value::Object(Map::from_iter([
            ("schema".into(), schema),
            ("new_record_uid".into(), new_uid("r").into()),
            ("new_change_id".into(), new_uid("op").into()),
            ("new_request_id".into(), new_uid("request").into()),
            ("new_placement_id".into(), new_uid("placement").into()),
        ])));
    }
    let names: Vec<_> = variants.iter().filter_map(action_name).collect();
    Ok(json!({
        "context":context,
        "record_fields":FIELDS,
        "protein_schema":PROTEIN.as_ref().map_err(error)?,
        "actions":names,
        "instructions": INSTRUCTIONS,
        "canvas_component_schema": CANVAS.as_ref().map_err(error)?,
        "canvas_context": CANVAS_CONTEXT,
        "fiote_workflows": FIOTE_WORKFLOWS,
        "examples":{
            "search":{"source":"record","where":[{"text_contains":"work"}],"fields":["uid","head","quantity","assertions","assignees"],"limit":20},
            "proteins":{"source":"record","where":[{"kind_eq":"protein"}],"include":{"extension":{"namespace":"lince.protein"}},"limit":20},
            "file_sync":{"source":"record","where":[{"kind_eq":"organ"}],"include":{"extension":{"namespace":"lince.file_sync"}},"limit":20},
            "karma_rules":{"source":"karma_rule","limit":20},
            "karma_frequencies":{"source":"frequency","limit":20}
        }
    }))
}

fn action_name(variant: &Value) -> Option<&str> {
    variant
        .get("properties")?
        .get("action")?
        .get("const")?
        .as_str()
}

fn action_schema(
    variants: &[Value],
    all_defs: Option<&Value>,
    name: &str,
) -> Result<Value, String> {
    let variant = variants
        .iter()
        .find(|variant| action_name(variant) == Some(name))
        .ok_or("Unknown Action name. Use lince_describe without an action to list names.")?;
    let mut references = BTreeSet::new();
    let mut definitions = Map::new();
    collect(variant, &mut references);
    while let Some(name) = references.pop_first() {
        if definitions.contains_key(name) {
            continue;
        }
        let value = all_defs
            .and_then(|defs| defs.get(name))
            .unwrap_or(&Value::Null);
        collect(value, &mut references);
        definitions.insert(name.to_owned(), value.clone());
    }
    let mut schema = variant.clone();
    if !definitions.is_empty()
        && let Some(schema) = schema.as_object_mut()
    {
        schema.insert("$defs".into(), definitions.into());
    }
    Ok(schema)
}

fn collect<'a>(value: &'a Value, references: &mut BTreeSet<&'a str>) {
    match value {
        Value::Object(object) => {
            if let Some(name) = object
                .get("$ref")
                .and_then(Value::as_str)
                .and_then(|reference| reference.strip_prefix("#/$defs/"))
            {
                references.insert(name);
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
