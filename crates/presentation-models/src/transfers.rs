// non-lean-yet

pub mod actions;
pub mod fields;
mod list;
pub mod matching;
pub use list::{Hierarchy, depth, filtered, filtered_ref};
pub mod transport;
pub mod workflows;
pub mod workspace;

mod draft;
mod form;

pub use draft::{
    bind_preset, composer, dependency, disclosure_preview, edit, loan_proposal, observer, place,
    promise, review_draft, validate_draft,
};
pub use form::{Field, FieldKind, Form};

use serde_json::Value;
use std::borrow::Cow;

pub const FILTERS: [(&str, &str); 7] = [
    ("all", "Any status"),
    ("awaiting_me", "Awaiting me"),
    ("awaiting_others", "Awaiting others"),
    ("active", "Active"),
    ("completed", "Completed"),
    ("cancelled_or_broken", "Cancelled / broken"),
    ("discoverable_open", "Discoverable OPEN"),
];
pub const PRESETS: [&str; 14] = [
    "Contribution",
    "Paid contribution",
    "Need to receive",
    "Need to contribute",
    "Donation",
    "Trade",
    "Sale",
    "Assignment",
    "Group coordination",
    "Service",
    "Information",
    "Dependency plan",
    "Ride",
    "Delivery",
];
pub const STEPS: [&str; 5] = ["Basics", "People", "Promises", "Terms", "Review"];

pub fn text(value: &Value, key: &str) -> String {
    display(&value[key])
}

pub fn display(value: &Value) -> String {
    display_ref(value).into_owned()
}

pub fn display_ref(value: &Value) -> Cow<'_, str> {
    match value {
        Value::Null => "".into(),
        Value::String(value) => value.as_str().into(),
        _ => value.to_string().into(),
    }
}

pub fn array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value[key].as_array().map(Vec::as_slice).unwrap_or_default()
}

pub fn capability(value: &Value, key: &str) -> bool {
    value["capabilities"][key].as_bool() == Some(true)
}

pub fn title(value: &Value) -> String {
    title_ref(value).into_owned()
}

pub fn title_ref(value: &Value) -> Cow<'_, str> {
    if value.pointer("/disclosed/title") == Some(&Value::Bool(false)) {
        return "Title unavailable".into();
    }
    [
        "title",
        "head",
        "record_head",
        "actor_head",
        "person_head",
        "slug",
        "uid",
    ]
    .into_iter()
    .map(|key| display_ref(&value[key]))
    .find(|value| !value.is_empty())
    .unwrap_or_else(|| "Untitled".into())
}

pub fn human(value: &str) -> String {
    value.replace('_', " ")
}

pub fn status(value: &Value) -> String {
    status_ref(value).into_owned()
}

pub fn status_ref(value: &Value) -> Cow<'_, str> {
    ["primary_status", "operational_status", "status"]
        .into_iter()
        .map(|key| display_ref(&value[key]))
        .find(|value| !value.is_empty())
        .unwrap_or_else(|| "draft".into())
}

pub fn facet(value: &Value, key: &str) -> bool {
    key == "all" || value["inbox_facets"][key].as_bool() == Some(true)
}

#[cfg(test)]
mod tests;
