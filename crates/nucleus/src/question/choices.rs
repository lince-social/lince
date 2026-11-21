use serde_json::Value;

/// Collects the display label and exact value of each recognized choice.
pub fn choices(field: &Value) -> Vec<(String, String)> {
    choice_refs(field)
        .map(|(label, value)| (label.into(), value.into()))
        .collect()
}

pub(super) fn choice_refs(field: &Value) -> impl Iterator<Item = (&str, &str)> {
    let plain = field.get("enum").and_then(Value::as_array);
    let entries = plain.or_else(|| {
        field
            .get("oneOf")
            .or_else(|| field.get("anyOf"))
            .and_then(Value::as_array)
    });
    entries.into_iter().flatten().filter_map(move |entry| {
        if plain.is_some() {
            let value = entry.as_str()?;
            return Some((value, value));
        }
        let value = entry.get("const")?.as_str()?;
        let label = entry.get("title").and_then(Value::as_str).unwrap_or(value);
        Some((label, value))
    })
}
