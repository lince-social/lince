use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;

pub(super) const FILTERS: [(&str, &str); 7] = [
    ("all", "Any status"),
    ("awaiting_me", "Awaiting me"),
    ("awaiting_others", "Awaiting others"),
    ("active", "Active"),
    ("completed", "Completed"),
    ("cancelled_or_broken", "Cancelled / broken"),
    ("discoverable_open", "Discoverable OPEN"),
];
pub(super) const PRESETS: [&str; 9] = [
    "Donation",
    "Sale",
    "Assignment",
    "Group coordination",
    "Service",
    "Information",
    "Dependency plan",
    "Ride",
    "Delivery",
];
pub(super) const STEPS: [&str; 5] = ["Basics", "People", "Promises", "Terms", "Review"];

pub(super) fn text(value: &Value, key: &str) -> String {
    display(&value[key])
}

pub(super) fn display(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        _ => value.to_string(),
    }
}

pub(super) fn array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value[key].as_array().map(Vec::as_slice).unwrap_or_default()
}

pub(super) fn capability(value: &Value, key: &str) -> bool {
    value["capabilities"][key].as_bool() == Some(true)
}

pub(super) fn title(value: &Value) -> String {
    [
        "head",
        "record_head",
        "actor_head",
        "person_head",
        "slug",
        "uid",
    ]
    .into_iter()
    .map(|key| text(value, key))
    .find(|value| !value.is_empty())
    .unwrap_or_else(|| "Untitled".into())
}

pub(super) fn human(value: &str) -> String {
    value.replace('_', " ")
}

pub(super) fn status(value: &Value) -> String {
    ["primary_status", "operational_status", "status"]
        .into_iter()
        .map(|key| text(value, key))
        .find(|value| !value.is_empty())
        .unwrap_or_else(|| "draft".into())
}

pub(super) fn facet(value: &Value, key: &str) -> bool {
    key == "all" || value["inbox_facets"][key].as_bool() == Some(true)
}

pub(super) fn filtered(
    rows: &[Value],
    mine: bool,
    filter: &str,
    query: &str,
    sort: &str,
) -> Vec<Value> {
    let query = query.trim().to_lowercase();
    let mut result: Vec<_> = rows
        .iter()
        .filter(|row| {
            (!mine || facet(row, "mine"))
                && facet(row, filter)
                && (query.is_empty() || row.to_string().to_lowercase().contains(&query))
        })
        .cloned()
        .collect();
    result.sort_by_key(|row| {
        let priority = match sort {
            "name" => 0,
            _ if sort == "attention" && facet(row, "awaiting_me") => 0,
            _ if sort == "attention" && facet(row, "awaiting_others") => 1,
            _ => match status(row).as_str() {
                "system_disputed" | "participant_disputed" | "broken" => 2,
                "active" | "partially_settled" => 3,
                "draft" | "open" => 4,
                "completed" | "settled" | "kept" => 5,
                _ => 6,
            },
        };
        (priority, title(row).to_lowercase(), text(row, "uid"))
    });
    result
}

pub(super) fn depth(row: &Value, rows: &[Value]) -> usize {
    let mut parent = text(row, "parent");
    let mut seen = HashSet::from([text(row, "uid")]);
    let mut depth = 0;
    while !parent.is_empty() && seen.insert(parent.clone()) {
        let Some(row) = rows.iter().find(|row| text(row, "uid") == parent) else {
            break;
        };
        parent = text(row, "parent");
        depth += 1;
    }
    depth
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Form {
    pub title: String,
    pub data: Value,
    pub fields: Vec<Field>,
    pub step: Option<usize>,
    pub transfer: Option<String>,
    pub revision: Option<u64>,
    pub mode: String,
    pub request_id: String,
    pub actor: Option<String>,
    pub preset: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Field {
    pub path: String,
    pub label: String,
    pub kind: FieldKind,
    pub value: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum FieldKind {
    Text,
    Optional,
    Number,
    OptionalNumber,
    People,
    Choice(Vec<String>),
    Reference(String),
}

impl Form {
    pub(super) fn action(title: &str, mut data: Value) -> Self {
        let request_id = nucleus::new_uid("transfer-ui");
        data["request_id"] = json!(request_id);
        Self {
            title: title.into(),
            data,
            fields: Vec::new(),
            step: None,
            transfer: None,
            revision: None,
            mode: String::new(),
            request_id,
            actor: None,
            preset: String::new(),
        }
    }

    pub(super) fn field(&mut self, path: &str, label: &str, kind: FieldKind) {
        let value = self.data.pointer(path).unwrap_or(&Value::Null);
        let value = if matches!(kind, FieldKind::People) {
            value
                .as_array()
                .map(|values| values.iter().map(display).collect::<Vec<_>>().join(", "))
                .unwrap_or_default()
        } else {
            display(value)
        };
        self.fields.push(Field {
            path: path.into(),
            label: label.into(),
            kind,
            value,
        });
    }

    pub(super) fn commit_fields(&mut self) -> Result<(), String> {
        let mut data = self.data.clone();
        for field in &self.fields {
            let raw = field.value.trim();
            let value = match &field.kind {
                FieldKind::Optional | FieldKind::Reference(_) if raw.is_empty() => Value::Null,
                FieldKind::OptionalNumber if raw.is_empty() => Value::Null,
                FieldKind::Number | FieldKind::OptionalNumber => {
                    let value: Value = serde_json::from_str(raw)
                        .map_err(|_| format!("{} must be a number", field.label))?;
                    if !value.is_number() {
                        return Err(format!("{} must be a finite number", field.label));
                    }
                    value
                }
                FieldKind::People => json!(
                    raw.split(',')
                        .map(str::trim)
                        .filter(|v| !v.is_empty())
                        .collect::<Vec<_>>()
                ),
                FieldKind::Choice(choices) => {
                    if !choices.iter().any(|choice| choice == raw) {
                        return Err(format!("Choose {}", field.label));
                    }
                    if raw == "true" || raw == "false" {
                        json!(raw == "true")
                    } else {
                        json!(raw)
                    }
                }
                _ => json!(raw),
            };
            let target = data
                .pointer_mut(&field.path)
                .ok_or("This field is no longer available")?;
            *target = value;
        }
        if self.data != data {
            self.request_id = nucleus::new_uid("transfer-ui");
            if self.step.is_none() {
                data["request_id"] = json!(self.request_id);
            }
        }
        self.data = data;
        Ok(())
    }

    pub(super) fn payload(&self) -> Result<Value, String> {
        if self.step.is_none() {
            return Ok(self.data.clone());
        }
        if self.transfer.is_none() && array(&self.data, "promises").is_empty() {
            return Err("Add at least one promise".into());
        }
        validate_draft(&self.data)?;
        let mut data = self.data.clone();
        if data["agreement"] != "percentage" {
            data["agreement_pct"] = Value::Null;
        }
        if data["visibility"] != "proximity" {
            data["max_proximity"] = Value::Null;
        }
        for promise in data["promises"].as_array_mut().into_iter().flatten() {
            if promise["open"] == true {
                promise["party"] = Value::Null;
            }
            clean_place(&mut promise["place"])?;
        }
        clean_place(&mut data["default_place"])?;
        let mut action = json!({"action": self.mode, "request_id": self.request_id});
        if let Some(transfer) = &self.transfer {
            action["transfer"] = json!(transfer);
            action["expected_revision"] = json!(self.revision);
            action["person"] = json!(self.actor);
            action["draft"] = data;
        } else {
            action
                .as_object_mut()
                .unwrap()
                .extend(data.as_object().unwrap().clone());
        }
        Ok(action)
    }

    pub(super) fn valid(&self) -> bool {
        self.data.is_object()
            && self.data.to_string().len() < 262_144
            && self.fields.len() <= 1024
            && self.fields.iter().all(|field| {
                field.value.len() <= 16_384 && self.data.pointer(&field.path).is_some()
            })
            && self.step.is_none_or(|step| step < STEPS.len())
            && self.step.is_none_or(|_| {
                ["promises", "dependencies", "invitees"]
                    .into_iter()
                    .all(|key| self.data[key].is_array())
                    && array(&self.data, "promises").len() <= 64
                    && array(&self.data, "dependencies").len() <= 64
                    && array(&self.data, "promises").iter().all(Value::is_object)
                    && array(&self.data, "dependencies")
                        .iter()
                        .all(Value::is_object)
            })
    }
}

fn clean_place(value: &mut Value) -> Result<(), String> {
    if value.is_null() {
        return Ok(());
    }
    if value["lat"].is_null() && value["lon"].is_null() {
        if text(value, "address").trim().is_empty() {
            *value = Value::Null;
        }
        return Ok(());
    }
    if !value["lat"]
        .as_f64()
        .is_some_and(|v| (-90.0..=90.0).contains(&v))
        || !value["lon"]
            .as_f64()
            .is_some_and(|v| (-180.0..=180.0).contains(&v))
    {
        return Err("Enter valid latitude and longitude together".into());
    }
    Ok(())
}

pub(super) fn promise(person: &str) -> Value {
    json!({"uid": nucleus::new_uid("p"), "record": null, "party": person,
        "open": person.is_empty(), "delta": -1, "unit": null, "window_start": null,
        "window_end": null, "place": place(), "condition": null,
        "reserve_from": "inherit", "reuse_policy": "duplicate"})
}

pub(super) fn place() -> Value {
    json!({"lat": null, "lon": null, "address": null})
}

pub(super) fn dependency() -> Value {
    json!({"uid": null, "scope": "transfer", "promise": null,
        "upstream_kind": "transfer", "upstream": "", "required_state": "kept"})
}

pub(super) fn composer(person: &str, preset: &str) -> Form {
    let open = matches!(preset, "Donation" | "Service" | "Information");
    let mut first = promise(if open { "" } else { person });
    first["open"] = json!(open);
    if matches!(preset, "Dependency plan" | "Ride" | "Delivery") {
        first["delta"] = json!(1);
    }
    if preset == "Assignment" {
        first["party"] = Value::Null;
    }
    let mut promises = vec![first];
    if matches!(preset, "Sale" | "Group coordination") {
        let mut second = promise(person);
        if preset == "Sale" {
            second["delta"] = json!(1);
        } else {
            second["party"] = Value::Null;
        }
        promises.push(second);
    }
    let mut form = Form::action(
        "Create transfer",
        json!({
            "creator": person, "head": preset, "slug": null, "agreement": if preset == "Dependency plan" { "dependency" } else { "full" },
            "agreement_pct": 100, "satiation": "none", "parent": null, "source": null,
            "visibility": "hidden", "max_proximity": 1, "reserve_default": "inherit",
            "require_confirmation": true, "default_place": place(), "invitees": [],
            "promises": promises, "dependencies": if preset == "Dependency plan" { vec![dependency()] } else { Vec::new() }
        }),
    );
    form.data.as_object_mut().unwrap().remove("request_id");
    form.mode = "create-transfer-draft".into();
    form.preset = preset.into();
    form.step = Some(0);
    form
}

pub(super) fn edit(row: &Value, counteroffer: bool, person: &str) -> Form {
    let mut form = composer(person, "");
    for key in [
        "head",
        "slug",
        "agreement_pct",
        "satiation",
        "parent",
        "source",
        "visibility",
        "max_proximity",
        "reserve_default",
        "require_confirmation",
        "default_place",
        "dependencies",
    ] {
        if !row[key].is_null() {
            form.data[key] = row[key].clone();
        }
    }
    form.data["agreement"] = row["agreement_type"].clone();
    let creator = array(row, "parties")
        .iter()
        .find(|p| p["role"] == "creator")
        .or_else(|| array(row, "parties").first())
        .map(|p| p["actor"].clone())
        .unwrap_or(json!(person));
    form.data["creator"] = creator;
    form.data["invitees"] = json!(
        array(row, "invitations")
            .iter()
            .filter(|i| i["status"] == "pending")
            .map(|i| i["addressed_person"].clone())
            .collect::<Vec<_>>()
    );
    form.data["promises"] = json!(
        array(row, "promises")
            .iter()
            .filter(|p| p["state"] != "withdrawn")
            .map(|p| {
                let mut terms = promise("");
                for key in [
                    "uid",
                    "record",
                    "party",
                    "open",
                    "delta",
                    "unit",
                    "window_start",
                    "window_end",
                    "place",
                    "condition",
                    "reserve_from",
                    "reuse_policy",
                ] {
                    if !p[key].is_null() {
                        terms[key] = p[key].clone();
                    }
                }
                terms
            })
            .collect::<Vec<_>>()
    );
    if let Some(snapshot) = row
        .pointer("/revision_evidence/current/terms")
        .filter(|value| value.is_object())
    {
        let terms = &snapshot["transfer"];
        for key in [
            "head",
            "slug",
            "agreement_pct",
            "visibility",
            "max_proximity",
            "require_confirmation",
            "default_place",
        ] {
            form.data[key] = terms[key].clone();
        }
        for (key, source) in [
            ("agreement", "agreement_type"),
            ("parent", "parent_uid"),
            ("source", "source_uid"),
        ] {
            form.data[key] = terms[source].clone();
        }
        for key in ["satiation", "reserve_default"] {
            if !terms[key].is_null() {
                form.data[key] = terms[key].clone();
            }
        }
        if let Some(creator) = array(snapshot, "parties")
            .iter()
            .find(|party| party["kind"] == "creator")
        {
            form.data["creator"] = creator["person_uid"].clone();
        }
        form.data["promises"] = json!(
            array(snapshot, "promises")
                .iter()
                .filter(|p| p["state"] != "withdrawn")
                .map(|p| {
                    let mut terms = promise("");
                    for key in [
                        "uid",
                        "delta",
                        "window_start",
                        "window_end",
                        "condition",
                        "reserve_from",
                    ] {
                        terms[key] = p[key].clone();
                    }
                    for (key, source) in [
                        ("record", "record_uid"),
                        ("party", "person_uid"),
                        ("unit", "unit_uid"),
                        ("place", "location"),
                        ("reuse_policy", "open_reuse_policy"),
                    ] {
                        terms[key] = p[source].clone();
                    }
                    terms["open"] = json!(p["state"] == "open");
                    terms
                })
                .collect::<Vec<_>>()
        );
        form.data["dependencies"] = json!(array(snapshot, "dependencies").iter().map(|d| json!({
            "uid":d["uid"],"scope":d["scope"],"promise":d["promise_uid"],"upstream_kind":d["upstream_kind"],"upstream":d["upstream_uid"],"required_state":d["required_state"]
        })).collect::<Vec<_>>());
    }
    if form.data["default_place"].is_null() {
        form.data["default_place"] = place();
    }
    form.actor = Some(person.into());
    form.mode = if counteroffer {
        "counteroffer-transfer"
    } else if row["revision"] == 0 {
        "adopt-transfer-draft"
    } else {
        "revise-transfer-draft"
    }
    .into();
    form.title = if counteroffer {
        "Counteroffer"
    } else {
        "Edit transfer"
    }
    .into();
    form.transfer = Some(text(row, "uid"));
    form.revision = row["revision"].as_u64();
    form
}

pub(super) fn bind_preset(form: &mut Form) {
    let creator = form.data["creator"].clone();
    let invitee = array(&form.data, "invitees")
        .first()
        .cloned()
        .unwrap_or(Value::Null);
    for (index, promise) in form.data["promises"]
        .as_array_mut()
        .into_iter()
        .flatten()
        .enumerate()
    {
        if promise["open"] == true {
            continue;
        }
        if text(promise, "party").is_empty() {
            promise["party"] = if form.preset == "Assignment"
                || (form.preset == "Group coordination" && index == 1)
            {
                invitee.clone()
            } else {
                creator.clone()
            };
        }
    }
}

pub(super) fn validate_draft(data: &Value) -> Result<(), String> {
    if text(data, "head").trim().is_empty() {
        return Err("Enter a transfer title".into());
    }
    let creator = text(data, "creator");
    if creator.is_empty() {
        return Err("Choose the Person creating this transfer".into());
    }
    let mut people = HashSet::from([creator]);
    for person in array(data, "invitees") {
        if !people.insert(display(person)) {
            return Err(
                "Each invitee must be different from the creator and other invitees".into(),
            );
        }
    }
    if data["agreement"] == "percentage"
        && !data["agreement_pct"]
            .as_u64()
            .is_some_and(|v| (1..=100).contains(&v))
    {
        return Err("Agreement percentage must be between 1 and 100".into());
    }
    if data["visibility"] == "proximity" && !data["max_proximity"].as_u64().is_some_and(|v| v > 0) {
        return Err("Maximum proximity must be a positive whole number".into());
    }
    if data["satiation"] == "first_completes" && text(data, "source").is_empty() {
        return Err("Choose a shared source for first-sibling completion".into());
    }
    for (index, promise) in array(data, "promises").iter().enumerate() {
        if text(promise, "record").is_empty()
            || !promise["delta"]
                .as_f64()
                .is_some_and(|v| v.is_finite() && v != 0.0)
        {
            return Err(format!(
                "Promise {} needs a record and a nonzero quantity",
                index + 1
            ));
        }
        if promise["open"] != true && text(promise, "party").is_empty() {
            return Err(format!("Choose a Person or OPEN for promise {}", index + 1));
        }
        let dates: Vec<_> = ["window_start", "window_end"].into_iter().map(|key| {
            if text(promise, key).is_empty() { Ok(None) } else {
                chrono::DateTime::parse_from_rfc3339(&text(promise, key)).map(Some)
                    .map_err(|_| format!("Promise {} needs a date with a timezone, such as 2026-10-01T10:00:00-03:00", index + 1))
            }
        }).collect::<Result<_, _>>()?;
        if let [Some(start), Some(end)] = dates.as_slice()
            && start >= end
        {
            return Err("The start must come before the deadline".into());
        }
    }
    if data["agreement"] == "dependency" && array(data, "dependencies").is_empty() {
        return Err("Add a dependency for this agreement".into());
    }
    for dependency in array(data, "dependencies") {
        if text(dependency, "upstream").is_empty() {
            return Err("Choose the upstream item for each dependency".into());
        }
    }
    Ok(())
}
