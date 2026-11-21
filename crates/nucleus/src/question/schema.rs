use std::collections::HashSet;

use serde_json::Value;

use super::{MAX_CHOICES, MAX_FORM_BYTES, MAX_PATTERN_BYTES, choices::choice_refs};
use crate::json::encoded_len;

/// Validates a bounded flat question form and its supported constraints.
///
/// # Errors
///
/// Returns the first form, field or choice validation diagnostic.
#[expect(
    clippy::map_err_ignore,
    reason = "preserves the public invalid-pattern diagnostic"
)]
pub fn validate_schema(schema: &Value) -> Result<(), String> {
    if encoded_len(schema).map_err(|error| error.to_string())? > MAX_FORM_BYTES
        || schema["type"] != "object"
    {
        return Err("Use a flat question form smaller than 64 KiB.".into());
    }
    let fields = schema["properties"]
        .as_object()
        .ok_or("The question has no fields.")?;
    let root = schema
        .as_object()
        .ok_or("Use a flat question form smaller than 64 KiB.")?;
    if root.keys().any(|key| {
        ![
            "type",
            "properties",
            "required",
            "title",
            "description",
            "additionalProperties",
            "_meta",
        ]
        .contains(&key.as_str())
    }) {
        return Err("This question uses unsupported form constraints.".into());
    }
    if schema
        .get("required")
        .is_some_and(|value| !value.is_array())
        || schema
            .get("additionalProperties")
            .is_some_and(|value| !value.is_boolean())
    {
        return Err("Invalid form constraints.".into());
    }
    if fields.is_empty() || fields.len() > 32 {
        return Err("Use 1–32 question fields.".into());
    }
    for (name, field) in fields {
        let object = field
            .as_object()
            .filter(|_| {
                !name.is_empty() && name.len() <= 128 && !name.chars().any(char::is_control)
            })
            .ok_or("Invalid question field name.")?;
        let kind = field["type"].as_str();
        let specific: &[&str] = match kind {
            Some("string") => &[
                "minLength",
                "maxLength",
                "pattern",
                "format",
                "enum",
                "oneOf",
            ],
            Some("number" | "integer") => &["minimum", "maximum"],
            Some("boolean") => &[],
            Some("array") => &["items", "minItems", "maxItems"],
            _ => return Err(format!("Unsupported question field type: {name}")),
        };
        if object.keys().any(|key| {
            let key = key.as_str();
            !["type", "title", "description", "default", "_meta"].contains(&key)
                && !specific.contains(&key)
        }) {
            return Err(format!("Unsupported question constraint: {name}"));
        }
        for key in ["minLength", "maxLength", "minItems", "maxItems"] {
            if field.get(key).is_some_and(|value| value.as_u64().is_none()) {
                return Err(format!("Invalid {key} for {name}"));
            }
        }
        for key in ["minimum", "maximum"] {
            if field
                .get(key)
                .is_some_and(|value| value.as_f64().is_none_or(|value| !value.is_finite()))
            {
                return Err(format!("Invalid {key} for {name}"));
            }
        }
        for key in ["title", "description", "pattern", "format"] {
            if field.get(key).is_some_and(|value| !value.is_string()) {
                return Err(format!("Invalid {key} for {name}"));
            }
        }
        for (low, high) in [
            ("minimum", "maximum"),
            ("minLength", "maxLength"),
            ("minItems", "maxItems"),
        ] {
            if let (Some(low), Some(high)) = (field[low].as_f64(), field[high].as_f64())
                && low > high
            {
                return Err(format!("Conflicting limits for {name}"));
            }
        }
        let array = kind == Some("array");
        let choice_field = if array { &field["items"] } else { field };
        if array
            && (choice_field.as_object().is_none_or(|items| {
                items
                    .keys()
                    .any(|key| !["type", "enum", "anyOf", "oneOf", "_meta"].contains(&key.as_str()))
            }) || choice_field
                .get("type")
                .is_some_and(|value| value != "string"))
        {
            return Err("Multiple-choice fields need string choices.".into());
        }
        let mut count = 0;
        for key in ["enum", "oneOf", "anyOf"] {
            if let Some(value) = choice_field.get(key) {
                count += 1;
                let entries = value
                    .as_array()
                    .filter(|entries| !entries.is_empty() && entries.len() <= MAX_CHOICES)
                    .ok_or("Use 1–64 choices per field.")?;
                let mut seen = HashSet::with_capacity(entries.len());
                if choice_refs(choice_field).any(|(_, value)| !seen.insert(value))
                    || seen.len() != entries.len()
                {
                    return Err("Question choices must be distinct strings.".into());
                }
                if key != "enum"
                    && entries.iter().any(|entry| {
                        entry.as_object().is_none_or(|entry| {
                            entry.keys().any(|key| {
                                !["const", "title", "description", "_meta"].contains(&key.as_str())
                            })
                        })
                    })
                {
                    return Err("Unsupported constraint on a question choice.".into());
                }
            }
        }
        if count > 1 {
            return Err("Choose one form of choices for each field.".into());
        }
        if let Some(pattern) = field["pattern"].as_str() {
            regex::RegexBuilder::new(pattern)
                .size_limit(MAX_PATTERN_BYTES)
                .build()
                .map_err(|_| format!("Unsupported pattern in {name}"))?;
        }
        if field["format"]
            .as_str()
            .is_some_and(|format| !["email", "uri", "date", "date-time"].contains(&format))
        {
            return Err(format!("Unsupported field format: {name}"));
        }
        if array && count == 0 {
            return Err("Multiple-choice fields need 1–64 string choices.".into());
        }
    }
    if let Some(required) = schema["required"].as_array()
        && required
            .iter()
            .any(|name| name.as_str().is_none_or(|name| !fields.contains_key(name)))
    {
        return Err("Required fields must exist in the form.".into());
    }
    Ok(())
}
