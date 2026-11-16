use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{borrow::Cow, fmt::Write};

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Source {
    #[default]
    Local,
    Organ(String),
}

pub fn baseline(data: &Value, property: &str) -> Value {
    let mut value = serde_json::json!({property: data[property]});
    if matches!(property, "start_date" | "due_date" | "estimate_min") {
        value["extension"] = data["extension"].clone();
    }
    value
}

pub fn baseline_matches(saved: &Value, data: &Value, property: &str) -> bool {
    saved[property] == data[property]
        && (!matches!(property, "start_date" | "due_date" | "estimate_min")
            || saved["extension"] == data["extension"])
}

pub fn display(value: &Value) -> String {
    display_ref(value).into_owned()
}

pub fn display_ref(value: &Value) -> Cow<'_, str> {
    match value {
        Value::Null => Cow::Borrowed(""),
        Value::String(value) => Cow::Borrowed(value),
        Value::Array(values) => Cow::Owned(display_list(values)),
        value => Cow::Owned(value.to_string()),
    }
}

pub fn display_list(values: &[Value]) -> String {
    let mut text = String::new();
    for (index, value) in values.iter().enumerate() {
        if index != 0 {
            text.push('\n');
        }
        if let Some(head) = value["head"].as_str() {
            text.push_str(head);
        } else if let Some(predicate) = value["predicate"].as_str() {
            text.push('#');
            text.push_str(predicate);
            if let Some(quantity) = value["quantity"].as_str() {
                text.push_str(": ");
                text.push_str(quantity);
            }
            if let Some(object) = value["object"].as_str() {
                text.push_str(" → ");
                text.push_str(object);
            }
        } else {
            write!(text, "{value}").expect("format Record field");
        }
    }
    text
}

pub const KANBAN_COLUMNS: [(&str, &str, i32); 7] = [
    ("Backlog", "backlog", 0),
    ("Todo", "todo", -1),
    ("Next", "next", -2),
    ("WIP", "wip", -3),
    ("Review", "review", -4),
    ("Done", "done", 1),
    ("Documented", "documented", 2),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverflowMode {
    Clip,
    ScrollDown,
    ScrollRight,
    GrowDown,
    GrowRight,
}

impl OverflowMode {
    pub fn title(self) -> &'static str {
        match self {
            Self::Clip => "Clip",
            Self::ScrollDown => "Scroll down",
            Self::ScrollRight => "Scroll right",
            Self::GrowDown => "Grow down",
            Self::GrowRight => "Grow right",
        }
    }
    pub const ALL: [Self; 5] = [
        Self::Clip,
        Self::ScrollDown,
        Self::ScrollRight,
        Self::GrowDown,
        Self::GrowRight,
    ];
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    pub property: String,
    pub square: bool,
    pub editable: bool,
    pub width: f32,
    pub height: f32,
    pub overflow: OverflowMode,
}

impl Binding {
    pub fn new(property: &str) -> Self {
        Self {
            property: property.into(),
            square: property == "head",
            editable: false,
            width: 280.0,
            height: if property == "body" { 100.0 } else { 40.0 },
            overflow: OverflowMode::ScrollDown,
        }
    }
    pub fn valid(&self) -> bool {
        (nucleus::record_extension::column_binding(&self.property).is_some()
            || protein::record_schema::FIELDS
                .iter()
                .any(|field| field.key == self.property && (!self.editable || field.editable)))
            && [self.width, self.height]
                .iter()
                .all(|v| v.is_finite() && (24.0..=4000.0).contains(v))
    }
}
