use serde_json::{Value, json};

#[derive(Clone)]
pub enum FieldKind {
    Text,
    Number,
    Filter,
    Scope,
    Choice(Vec<(String, Value)>),
}

impl FieldKind {
    pub fn parse(&self, value: &Value, text: &str) -> Result<Value, String> {
        Ok(match self {
            Self::Text => json!(text.trim()),
            Self::Number => json!(
                text.trim()
                    .parse::<u32>()
                    .map_err(|_| "Enter a non-negative whole number")?
            ),
            Self::Choice(_) => value.clone(),
            Self::Scope => scope(value, text)?,
            Self::Filter => {
                if !text.trim().is_empty() {
                    let predicate: protein::Predicate = serde_json::from_str(text.trim())
                        .map_err(|e| format!("Invalid filter: {e}"))?;
                    let query: protein::Protein =
                        serde_json::from_value(json!({"source":"record", "where":[predicate]}))
                            .map_err(|e| e.to_string())?;
                    protein::validate(&query).map_err(|e| e.to_string())?;
                }
                json!(text.trim())
            }
        })
    }
}

pub fn scope_text(value: &Value) -> String {
    value
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

pub fn scope(mode: &Value, text: &str) -> Result<Value, String> {
    match mode.as_str() {
        Some("all") => Ok(Value::Null),
        Some("none") => Ok(json!([])),
        Some("some") => {
            let mut fields: Vec<_> = text
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            fields.sort_unstable();
            fields.dedup();
            if fields.is_empty() {
                return Err("Name at least one field, or choose Minimum only".into());
            }
            Ok(json!(fields))
        }
        _ => Err("Choose a sharing limit".into()),
    }
}
