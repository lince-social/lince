use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProteinDraft {
    pub name: String,
    pub slug: String,
    pub query: Value,
}

impl Default for ProteinDraft {
    fn default() -> Self {
        Self {
            name: "Protein".into(),
            slug: String::new(),
            query: json!({"source":"record", "where":[{"all":[]}], "order":[], "include":{}, "fields":null, "aggregate":null, "limit":100}),
        }
    }
}

impl ProteinDraft {
    pub fn from_protein(name: String, slug: String, query: protein::Protein) -> Self {
        let mut value = serde_json::to_value(query).unwrap();
        let filters = value["where"].as_array().unwrap();
        if filters.len() != 1
            || (filters[0].get("all").is_none() && filters[0].get("any").is_none())
        {
            value["where"] = json!([{"all":filters}]);
        }
        Self {
            name,
            slug,
            query: value,
        }
    }

    pub fn valid_storage(&self) -> bool {
        self.name.len() <= 1024
            && self.slug.len() <= 1024
            && serde_json::to_vec(&self.query).is_ok_and(|bytes| bytes.len() <= 131_072)
            && bounded(&self.query, 0, &mut 0)
            && self.query.is_object()
            && self.query["source"].is_string()
            && self.query["where"].is_array()
    }

    pub fn compile(&self) -> Result<protein::Protein, String> {
        if !self.valid_storage() {
            return Err("Protein is too large or malformed".into());
        }
        let mut query = self.query.clone();
        normalize(&mut query, "")?;
        options(&query)?;
        let protein: protein::Protein =
            serde_json::from_value(query).map_err(|error| error.to_string())?;
        protein::validate(&protein).map_err(|error| error.to_string())?;
        if protein.limit == Some(0) {
            return Err("Limit must be positive; leave it blank for unlimited rows".into());
        }
        for predicate in &protein.filter {
            complete(predicate, true)?;
        }
        if protein.order.len() > 16 {
            return Err("Use at most 16 sort rules".into());
        }
        Ok(protein)
    }
}

fn bounded(value: &Value, depth: usize, count: &mut usize) -> bool {
    *count += 1;
    if depth > 32 || *count > 4096 {
        return false;
    }
    match value {
        Value::Array(values) => values.iter().all(|v| bounded(v, depth + 1, count)),
        Value::Object(values) => values.values().all(|v| bounded(v, depth + 1, count)),
        _ => true,
    }
}

fn normalize(value: &mut Value, key: &str) -> Result<(), String> {
    if let Value::String(text) = value {
        if matches!(
            key,
            "limit"
                | "depth"
                | "messages_limit"
                | "revision_eq"
                | "revision_lt"
                | "revision_lte"
                | "revision_gt"
                | "revision_gte"
        ) {
            *value = if text.trim().is_empty() && key == "limit" {
                Value::Null
            } else {
                json!(
                    text.trim()
                        .parse::<u64>()
                        .map_err(|_| format!("{} must be a whole number", title(key)))?
                )
            };
        } else if key == "meters" {
            let number = text
                .trim()
                .parse::<f64>()
                .map_err(|_| "Distance must be a number")?;
            if !number.is_finite() || number < 0.0 {
                return Err("Distance must be finite and nonnegative".into());
            }
            *value = json!(number);
        }
    }
    match value {
        Value::Array(values) => {
            for value in values {
                normalize(value, "")?;
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                normalize(value, key)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn complete(predicate: &protein::Predicate, root: bool) -> Result<(), String> {
    use protein::Predicate::*;
    match predicate {
        All(children) | Any(children) => {
            if children.is_empty() && !root {
                return Err("Add a condition or remove the empty group".into());
            }
            for child in children {
                complete(child, false)?;
            }
        }
        Not(child) => complete(child, false)?,
        Relation { kind, other, .. } => {
            required(kind)?;
            if let Some(other) = other {
                required(other)?;
            }
        }
        Under { record, kind, .. } => {
            required(record)?;
            required(kind)?;
        }
        WorkDate { op, value, .. } => {
            if *op != protein::DateComparison::Exists {
                required(value.as_deref().unwrap_or_default())?;
            }
        }
        Near { of, .. } => required(of)?,
        StateIn(values)
        | OrganIn(values)
        | OccurrenceIn(values)
        | StatusIn(values)
        | ViewerRoleIn(values)
        | InvitationStateIn(values) => {
            if values.is_empty() {
                return Err("Add at least one value".into());
            }
            for value in values {
                required(value)?;
            }
        }
        UidEq(value)
        | KindEq(value)
        | SlugEq(value)
        | ConceptIn(value)
        | TextContains(value)
        | PersonEq(value)
        | UnitEq(value)
        | WindowEndBefore(value)
        | WindowEndAfter(value)
        | AtSince(value)
        | AtBefore(value)
        | ClassifiedIn(value)
        | CauseKindEq(value)
        | RecordEq(value)
        | OrganEq(value) => required(value)?,
        _ => {}
    }
    Ok(())
}

fn required(value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err("Complete every condition before running or saving".into())
    } else {
        Ok(())
    }
}

pub(super) fn title(key: &str) -> String {
    match key {
        "head" => "Title".into(),
        "body" => "Description".into(),
        "uid" | "uid_eq" => "Identity".into(),
        "kind_eq" => "Kind".into(),
        "slug_eq" => "Slug".into(),
        "organ_eq" => "Organ".into(),
        "concept_in" => "Assertion".into(),
        "under" => "Within".into(),
        "all" => "All".into(),
        "any" => "Any".into(),
        "quantity_lt" => "Quantity <".into(),
        "quantity_lte" => "Quantity ≤".into(),
        "quantity_gt" => "Quantity >".into(),
        "quantity_gte" => "Quantity ≥".into(),
        "quantity_eq" => "Quantity =".into(),
        _ => {
            let value = key.replace('_', " ");
            let mut chars = value.chars();
            chars
                .next()
                .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        }
    }
}

pub(super) const SOURCES: &[&str] = &[
    "record",
    "promise",
    "decision",
    "fact",
    "concept",
    "lingua",
    "assertion",
    "transfer",
    "transfer_settlement_preview",
    "transfer_bulk_completion_preview",
    "auth",
    "karma",
    "karma_rule",
    "timeline",
    "entry",
    "frequency",
    "recurrence",
    "nearby",
];

pub(super) const SORT_FIELDS: &[&str] = &[
    "head",
    "body",
    "slug",
    "kind",
    "quantity",
    "concept_name",
    "assignee_name",
    "start_date",
    "due_date",
    "created_at",
    "updated_at",
];

pub(super) const FIELDS: &[&str] = &[
    "head",
    "body",
    "slug",
    "quantity",
    "concept",
    "assertions",
    "assignees",
    "estimate_min",
    "spent_seconds",
    "running_since",
    "work_logs",
    "unit",
    "organ",
    "concept_name",
    "created_at",
    "updated_at",
    "created_hlc",
    "start_date",
    "due_date",
    "facts",
    "promises",
    "links",
    "threads",
    "availability",
    "extension",
    "projected",
    "contact",
    "conversations",
    "reference_reads",
];

pub(super) fn filters(source: &str) -> &'static [&'static str] {
    match source {
        "record" => &[
            "text_contains",
            "kind_eq",
            "slug_eq",
            "uid_eq",
            "concept_in",
            "organ_eq",
            "organ_in",
            "quantity_eq",
            "quantity_lt",
            "quantity_lte",
            "quantity_gt",
            "quantity_gte",
            "work_date",
            "relation",
            "under",
            "near",
        ],
        "promise" => &["state_in", "uid_eq"],
        "decision" | "auth" | "frequency" | "nearby" => &[],
        "fact" => &[
            "record_eq",
            "at_since",
            "at_before",
            "classified_in",
            "cause_kind_eq",
            "concept_in",
        ],
        "timeline" => &["classified_in", "at_since", "at_before"],
        "entry" => &["record_eq", "classified_in"],
        "recurrence" => &["record_eq", "at_since", "at_before"],
        "karma_rule" => &["record_eq", "uid_eq"],
        "concept" => &["concept_in", "uid_eq", "slug_eq"],
        "lingua" => &["uid_eq", "slug_eq"],
        "assertion" => &["concept_in", "uid_eq"],
        "transfer_settlement_preview" => &["uid_eq", "quantity_eq"],
        "transfer_bulk_completion_preview" => &["uid_eq", "occurrence_in"],
        "karma" => &["kind_eq", "uid_eq", "slug_eq", "status_in"],
        _ => &[
            "uid_eq",
            "slug_eq",
            "concept_in",
            "record_eq",
            "state_in",
            "status_in",
            "viewer_role_in",
            "invitation_state_in",
            "person_eq",
            "unit_eq",
            "window_end_before",
            "window_end_after",
            "revision_eq",
            "revision_lt",
            "revision_lte",
            "revision_gt",
            "revision_gte",
        ],
    }
}

pub(super) fn nested(source: &str) -> bool {
    matches!(source, "record" | "promise" | "transfer" | "karma")
}

pub(super) fn sorts(source: &str) -> &'static [&'static str] {
    match source {
        "record" => SORT_FIELDS,
        "transfer" => &["uid", "slug", "head", "revision", "status"],
        "karma" => &[
            "object_kind",
            "uid",
            "slug",
            "status",
            "created_at",
            "updated_at",
        ],
        _ => &[],
    }
}

pub(super) fn groups(source: &str) -> &'static [&'static str] {
    match source {
        "record" => &["total", "kind", "concept"],
        "fact" => &[
            "total",
            "concept",
            "cause_kind",
            "classification",
            "day",
            "month",
        ],
        "timeline" => &["day", "month"],
        _ => &[],
    }
}

pub(super) fn has_limit(source: &str) -> bool {
    !matches!(
        source,
        "auth"
            | "frequency"
            | "timeline"
            | "recurrence"
            | "transfer_settlement_preview"
            | "transfer_bulk_completion_preview"
    )
}

fn options(query: &Value) -> Result<(), String> {
    let source = query["source"].as_str().unwrap_or_default();
    fn check(
        value: &Value,
        source: &str,
        seen: &mut std::collections::HashSet<String>,
    ) -> Result<(), String> {
        let Some((key, child)) = value.as_object().and_then(|o| o.iter().next()) else {
            return Err("Invalid condition".into());
        };
        match key.as_str() {
            "any" if !nested(source) => {
                return Err("This source supports All conditions only".into());
            }
            "all" | "any" => {
                for child in child.as_array().ok_or("Invalid group")? {
                    check(child, source, seen)?;
                }
            }
            "not" => {
                if !nested(source) {
                    return Err("This source does not support Not conditions".into());
                }
                check(child, source, seen)?;
            }
            _ => {
                if !filters(source).contains(&key.as_str()) {
                    return Err(format!(
                        "{} is not supported by {}",
                        title(key),
                        title(source)
                    ));
                }
                if !nested(source) && !seen.insert(key.clone()) {
                    return Err(format!("Use one {} condition for this source", title(key)));
                }
            }
        }
        Ok(())
    }
    let mut seen = std::collections::HashSet::new();
    for value in query["where"].as_array().ok_or("Missing filters")? {
        check(value, source, &mut seen)?;
    }
    for order in query["order"].as_array().into_iter().flatten() {
        if let Some(link) = order.get("link") {
            if source != "record" {
                return Err("Relation sorting requires Records".into());
            }
            required(link["kind"].as_str().unwrap_or_default())?;
        } else if let Some(field) = order.get("asc").or_else(|| order.get("desc")) {
            if !sorts(source).contains(&field.as_str().unwrap_or_default()) {
                return Err("This sort property is not supported by the source".into());
            }
        }
    }
    let aggregate = &query["aggregate"];
    if !aggregate.is_null() {
        if !groups(source).contains(&aggregate["by"].as_str().unwrap_or_default()) {
            return Err("This grouping is not supported by the source".into());
        }
        if source == "timeline" && aggregate["op"] != "sum" {
            return Err("Timeline uses sums".into());
        }
        if !query["order"].as_array().is_none_or(Vec::is_empty) {
            return Err("Remove sorting when using aggregation; the backend orders groups".into());
        }
    }
    if source != "record" {
        if !query["fields"].is_null() {
            return Err("Selecting output fields requires Records".into());
        }
        if query["include"].as_object().is_some_and(|o| {
            o.values()
                .any(|value| !value.is_null() && value != &json!(false))
        }) {
            return Err("Included data requires Records".into());
        }
    }
    if !has_limit(source) && !query["limit"].is_null() {
        return Err("This source does not support a row limit".into());
    }
    Ok(())
}

pub(super) fn condition(key: &str) -> Value {
    let value = match key {
        "relation" => json!({"kind":"assigned-to", "direction":"out", "other":null}),
        "under" => json!({"record":"", "kind":"part-of", "include_self":false}),
        "work_date" => json!({"field":"due", "op":"eq", "value":""}),
        "near" => json!({"of":"", "meters":100}),
        "organ_in"
        | "state_in"
        | "occurrence_in"
        | "status_in"
        | "viewer_role_in"
        | "invitation_state_in" => json!([]),
        key if key.starts_with("revision_") => json!(0),
        key if key.starts_with("quantity_") => json!("0"),
        _ => json!(""),
    };
    json!({key:value})
}
