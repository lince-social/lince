use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Pending,
    Answered,
    Declined,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Question {
    pub prompt: String,
    pub responder: String,
    pub schema: Value,
    pub state: State,
    pub answers: Option<Value>,
    pub expires_ms: Option<u64>,
}

impl Question {
    pub fn validate(&self) -> Result<(), String> {
        if self.prompt.trim().is_empty()
            || self.prompt.len() > 8192
            || self.responder.is_empty()
            || self.responder.len() > 256
        {
            return Err("A question needs a prompt and one intended responder.".into());
        }
        validate_schema(&self.schema)?;
        match (&self.state, &self.answers) {
            (State::Answered, Some(answers)) => validate_answers(&self.schema, answers),
            (State::Answered, None) => Err("An answered question needs its answers.".into()),
            (_, Some(_)) => Err("Only answered questions can contain answers.".into()),
            _ => Ok(()),
        }
    }

    pub fn text(&self) -> String {
        format!(
            "Question for {}: {}\nState: {:?}\nFields: {}\n{}",
            self.responder,
            self.prompt,
            self.state,
            self.schema,
            self.answers
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default()
        )
    }

    pub fn response(&self, next: &Self, actor: &str, author: &str, now: u64) -> Result<(), String> {
        let mut expected = self.clone();
        expected.state = next.state.clone();
        expected.answers = next.answers.clone();
        if &expected != next || self.state != State::Pending || next.state == State::Pending {
            return Err("This question is no longer waiting, or its fields changed.".into());
        }
        if actor != self.responder && !(actor == author && next.state == State::Cancelled) {
            return Err("Only the intended responder can answer this question.".into());
        }
        if next.state != State::Cancelled && self.expires_ms.is_some_and(|expires| now >= expires) {
            return Err("This question expired. Ask for a new one.".into());
        }
        next.validate()
    }
}

pub fn choices(field: &Value) -> Vec<(String, String)> {
    if let Some(values) = field["enum"].as_array() {
        return values
            .iter()
            .filter_map(|value| value.as_str().map(|value| (value.into(), value.into())))
            .collect();
    }
    field
        .get("oneOf")
        .or_else(|| field.get("anyOf"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|choice| {
            let value = choice["const"].as_str()?;
            Some((
                choice["title"].as_str().unwrap_or(value).into(),
                value.into(),
            ))
        })
        .collect()
}

pub fn validate_schema(schema: &Value) -> Result<(), String> {
    if schema.to_string().len() > 65_536 || schema["type"] != "object" {
        return Err("Use a flat question form smaller than 64 KiB.".into());
    }
    let fields = schema["properties"]
        .as_object()
        .ok_or("The question has no fields.")?;
    if schema.as_object().unwrap().keys().any(|key| {
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
        if name.is_empty()
            || name.len() > 128
            || name.chars().any(char::is_control)
            || !field.is_object()
        {
            return Err("Invalid question field name.".into());
        }
        let allowed = match field["type"].as_str() {
            Some("string") => &[
                "type",
                "title",
                "description",
                "minLength",
                "maxLength",
                "pattern",
                "format",
                "default",
                "enum",
                "oneOf",
                "_meta",
            ][..],
            Some("number" | "integer") => &[
                "type",
                "title",
                "description",
                "minimum",
                "maximum",
                "default",
                "_meta",
            ][..],
            Some("boolean") => &["type", "title", "description", "default", "_meta"][..],
            Some("array") => &[
                "type",
                "title",
                "description",
                "items",
                "minItems",
                "maxItems",
                "default",
                "_meta",
            ][..],
            _ => return Err(format!("Unsupported question field type: {name}")),
        };
        if field
            .as_object()
            .unwrap()
            .keys()
            .any(|key| !allowed.contains(&key.as_str()))
        {
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
            if let (Some(low), Some(high)) = (field[low].as_f64(), field[high].as_f64()) {
                if low > high {
                    return Err(format!("Conflicting limits for {name}"));
                }
            }
        }
        let choice_field = if field["type"] == "array" {
            &field["items"]
        } else {
            field
        };
        if field["type"] == "array"
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
                    .filter(|entries| !entries.is_empty() && entries.len() <= 64)
                    .ok_or("Use 1–64 choices per field.")?;
                let choices = choices(choice_field);
                if choices.len() != entries.len()
                    || choices.iter().enumerate().any(|(index, (_, value))| {
                        choices[..index].iter().any(|(_, earlier)| earlier == value)
                    })
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
                .size_limit(65_536)
                .build()
                .map_err(|_| format!("Unsupported pattern in {name}"))?;
        }
        if field["format"]
            .as_str()
            .is_some_and(|format| !["email", "uri", "date", "date-time"].contains(&format))
        {
            return Err(format!("Unsupported field format: {name}"));
        }
        if field["type"] == "array"
            && (choices(&field["items"]).is_empty() || choices(&field["items"]).len() > 64)
        {
            return Err("Multiple-choice fields need 1–64 string choices.".into());
        }
        if choices(field).len() > 64 {
            return Err("Use at most 64 choices per field.".into());
        }
    }
    if let Some(required) = schema["required"].as_array() {
        if required
            .iter()
            .any(|name| name.as_str().is_none_or(|name| !fields.contains_key(name)))
        {
            return Err("Required fields must exist in the form.".into());
        }
    }
    Ok(())
}

pub fn validate_answers(schema: &Value, answers: &Value) -> Result<(), String> {
    validate_schema(schema)?;
    if answers.to_string().len() > 65_536 {
        return Err("Answers exceed 64 KiB.".into());
    }
    let answers = answers
        .as_object()
        .ok_or("Answers must name the form fields.")?;
    let fields = schema["properties"].as_object().unwrap();
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
        let field = &fields[name];
        let invalid = || format!("Check the answer for {name}.");
        match field["type"].as_str() {
            Some("string") => {
                let text = answer.as_str().ok_or_else(invalid)?;
                let count = text.chars().count() as u64;
                if count > 8192
                    || field["minLength"].as_u64().is_some_and(|min| count < min)
                    || field["maxLength"].as_u64().is_some_and(|max| count > max)
                {
                    return Err(invalid());
                }
                let choices = choices(field);
                if !choices.is_empty() && !choices.iter().any(|(_, value)| value == text) {
                    return Err(invalid());
                }
                if let Some(pattern) = field["pattern"].as_str() {
                    if !regex::RegexBuilder::new(pattern)
                        .size_limit(65_536)
                        .build()
                        .map_err(|_| invalid())?
                        .is_match(text)
                    {
                        return Err(invalid());
                    }
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
                let count = values.len() as u64;
                let choices = choices(&field["items"]);
                if count > 64
                    || field["minItems"].as_u64().is_some_and(|min| count < min)
                    || field["maxItems"].as_u64().is_some_and(|max| count > max)
                    || values.iter().enumerate().any(|(index, value)| {
                        !choices
                            .iter()
                            .any(|(_, choice)| value.as_str() == Some(choice.as_str()))
                            || values[..index].contains(value)
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn question() -> Question {
        Question {
            prompt: "Choose the next step".into(),
            responder: "person-one".into(),
            schema: json!({"type":"object","properties":{"choice":{"type":"string","enum":["a","b"]},"count":{"type":"integer","minimum":1,"maximum":4}},"required":["choice"]}),
            state: State::Pending,
            answers: None,
            expires_ms: Some(1000),
        }
    }

    #[test]
    fn answer_requires_exact_recipient_pending_state_and_unchanged_fields() {
        let original = question();
        let mut next = original.clone();
        next.state = State::Answered;
        next.answers = Some(json!({"choice":"a","count":2}));
        assert!(
            original
                .response(&next, "person-one", "author", 100)
                .is_ok()
        );
        assert!(
            original
                .response(&next, "person-two", "author", 100)
                .is_err()
        );
        assert!(
            original
                .response(&next, "person-one", "author", 1000)
                .is_err()
        );
        assert!(next.response(&next, "person-one", "author", 100).is_err());
        next.prompt = "Different question".into();
        assert!(
            original
                .response(&next, "person-one", "author", 100)
                .is_err()
        );
    }

    #[test]
    fn form_validation_checks_types_choices_limits_patterns_and_duplicates() {
        let schema = question().schema;
        for answers in [
            json!({}),
            json!({"choice":"c"}),
            json!({"choice":"a","count":2.5}),
            json!({"choice":"a","extra":true}),
            json!({"choice":"a","count":5}),
        ] {
            assert!(validate_answers(&schema, &answers).is_err());
        }
        let schema = json!({"type":"object","properties":{"name":{"type":"string","minLength":2,"maxLength":8,"pattern":"^[a-z]+$"},"flags":{"type":"array","items":{"type":"string","enum":["a","b"]},"minItems":1,"maxItems":2}}});
        assert!(validate_answers(&schema, &json!({"name":"hi","flags":["a","b"]})).is_ok());
        for answers in [
            json!({"name":"X"}),
            json!({"name":"toolongname"}),
            json!({"flags":[]}),
            json!({"flags":["a","a"]}),
            json!({"flags":["c"]}),
        ] {
            assert!(validate_answers(&schema, &answers).is_err());
        }
    }
}
