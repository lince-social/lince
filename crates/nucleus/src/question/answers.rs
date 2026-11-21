use std::collections::HashSet;

use serde_json::Value;

use super::{
    MAX_CHOICES, MAX_FORM_BYTES, MAX_PATTERN_BYTES, MAX_TEXT, choices::choice_refs, validate_schema,
};

/// Validates named answers against a supported form in field order.
///
/// # Errors
///
/// Returns the first form, size, required-field or answer validation diagnostic.
#[expect(
    clippy::map_err_ignore,
    reason = "preserves the public invalid-pattern diagnostic"
)]
pub fn validate_answers(schema: &Value, answers: &Value) -> Result<(), String> {
    validate_schema(schema)?;
    if answers.to_string().len() > MAX_FORM_BYTES {
        return Err("Answers exceed 64 KiB.".into());
    }
    let answers = answers
        .as_object()
        .ok_or("Answers must name the form fields.")?;
    let fields = schema["properties"]
        .as_object()
        .ok_or("The question has no fields.")?;
    if answers.keys().any(|key| !fields.contains_key(key)) {
        return Err("The answer contains an unknown field.".into());
    }
    for name in schema["required"].as_array().into_iter().flatten() {
        if name
            .as_str()
            .is_some_and(|name| !answers.contains_key(name))
        {
            return Err(format!("Answer the required field {name}."));
        }
    }
    for (name, answer) in answers {
        let field = fields
            .get(name)
            .ok_or("The answer contains an unknown field.")?;
        let invalid = || format!("Check the answer for {name}.");
        match field["type"].as_str() {
            Some("string") => {
                let text = answer.as_str().ok_or_else(invalid)?;
                let characters = text.chars().count();
                let count = u64::try_from(characters).unwrap_or(u64::MAX);
                if characters > MAX_TEXT
                    || field["minLength"].as_u64().is_some_and(|min| count < min)
                    || field["maxLength"].as_u64().is_some_and(|max| count > max)
                {
                    return Err(invalid());
                }
                let mut choices = choice_refs(field).peekable();
                if choices.peek().is_some() && !choices.any(|(_, value)| value == text) {
                    return Err(invalid());
                }
                if let Some(pattern) = field["pattern"].as_str()
                    && !regex::RegexBuilder::new(pattern)
                        .size_limit(MAX_PATTERN_BYTES)
                        .build()
                        .map_err(|_| invalid())?
                        .is_match(text)
                {
                    return Err(invalid());
                }
                let valid = match field["format"].as_str() {
                    Some("email") => text.split_once('@').is_some_and(|(local, host)| {
                        !local.is_empty()
                            && host.contains('.')
                            && !text.chars().any(char::is_whitespace)
                            && !host.contains('@')
                    }),
                    Some("uri") => url::Url::parse(text).is_ok(),
                    Some("date") => chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d").is_ok(),
                    Some("date-time") => chrono::DateTime::parse_from_rfc3339(text).is_ok(),
                    _ => true,
                };
                if !valid {
                    return Err(invalid());
                }
            }
            Some("number" | "integer") => {
                let number = answer
                    .as_f64()
                    .filter(|number| number.is_finite())
                    .ok_or_else(invalid)?;
                if field["type"] == "integer" && number.fract() != 0.0
                    || field["minimum"].as_f64().is_some_and(|min| number < min)
                    || field["maximum"].as_f64().is_some_and(|max| number > max)
                {
                    return Err(invalid());
                }
            }
            Some("boolean") if answer.is_boolean() => {}
            Some("array") => {
                let values = answer.as_array().ok_or_else(invalid)?;
                let count = u64::try_from(values.len()).unwrap_or(u64::MAX);
                let choices: HashSet<_> = choice_refs(&field["items"])
                    .map(|(_, value)| value)
                    .collect();
                let mut seen = HashSet::with_capacity(values.len().min(MAX_CHOICES));
                if values.len() > MAX_CHOICES
                    || field["minItems"].as_u64().is_some_and(|min| count < min)
                    || field["maxItems"].as_u64().is_some_and(|max| count > max)
                    || values.iter().any(|value| {
                        value
                            .as_str()
                            .is_none_or(|value| !choices.contains(value) || !seen.insert(value))
                    })
                {
                    return Err(invalid());
                }
            }
            _ => return Err(invalid()),
        }
    }
    Ok(())
}
