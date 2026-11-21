// non-lean-yet

use nucleus::karma::rule_field::{RuleFieldInput, RuleFieldKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
pub mod transfers;
mod suggestions;
pub use suggestions::{Suggestions, suggestions};
pub use suggestions::tail as suggestion_tail;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedField {
    pub uid: String,
    pub kind: RuleFieldKind,
    pub source: String,
    pub revision: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Rule {
    pub uid: String,
    pub name: String,
    pub slug: String,
    pub fields: Vec<SharedField>,
    pub revision: i64,
    pub state: String,
    #[serde(default)]
    pub bindings: Vec<nucleus::karma::ConditionBinding>,
    #[serde(default)]
    pub record: String,
}

pub fn bound_reading(rule: &Rule, source: &str) -> String {
    let condition = nucleus::karma::Condition::parse(source).ok();
    let reads = condition
        .as_ref()
        .map_or(&[][..], |condition| condition.reads_ref());
    fragments(source)
        .into_iter()
        .map(|(text, slug)| {
            slug.and_then(|slug| {
                rule.bindings.iter().find(|binding| {
                    binding.authored == slug
                        && reads.iter().any(|read| {
                            read.slug.split('|').enumerate().any(|(position, name)| {
                                name == slug
                                    && if read.func == nucleus::expr::HAS_ASSERTION {
                                        nucleus::expr::assertion_binding(position)
                                            == binding.reading
                                    } else {
                                        nucleus::expr::binding_reading(&read.func)
                                            == binding.reading
                                    }
                            })
                        })
                })
            })
            .map_or(text, |binding| {
                format!(
                    "{}{}",
                    if binding.reading == "has_assertion.predicate" {
                        "#"
                    } else {
                        "@"
                    },
                    binding.target.as_str()
                )
            })
        })
        .collect()
}

#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldDraft {
    pub text: String,
    pub linked: Option<SharedField>,
}

impl FieldDraft {
    pub fn input(&self) -> RuleFieldInput {
        match &self.linked {
            Some(field) => RuleFieldInput::Reference {
                uid: field.uid.clone(),
                revision: field.revision,
            },
            None => RuleFieldInput::Text {
                source: self.text.clone(),
            },
        }
    }
}

#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draft {
    pub name: String,
    pub slug: String,
    pub rule: Option<String>,
    pub revision: Option<i64>,
    pub fields: [FieldDraft; 3],
}

impl Draft {
    pub fn from_rule(rule: &Rule) -> Self {
        Self {
            name: rule.name.clone(),
            slug: rule.slug.clone(),
            rule: Some(rule.uid.clone()),
            revision: Some(rule.revision),
            fields: RuleFieldKind::ALL.map(|kind| {
                let linked = rule.fields.iter().find(|field| field.kind == kind).cloned();
                FieldDraft {
                    text: linked
                        .as_ref()
                        .map_or_else(String::new, |field| field.source.clone()),
                    linked,
                }
            }),
        }
    }

    pub fn valid(&self) -> bool {
        self.name.len() <= 256
            && self.slug.len() <= 256
            && self.fields.iter().all(|field| {
                field.text.len() <= 16_384
                    && field
                        .linked
                        .as_ref()
                        .is_none_or(|field| field.source.len() <= 16_384 && field.uid.len() <= 256)
            })
            && self.rule.as_ref().is_none_or(|uid| uid.len() <= 256)
    }
}

#[derive(Clone, Debug)]
pub enum Suggestion {
    Shared(SharedField),
    Element(String),
}

pub fn element_label(element: &str, records: &[Value]) -> String {
    let Ok(condition) = nucleus::karma::Condition::parse(element) else {
        return element.into();
    };
    let [reading] = condition.reads_ref() else {
        return element.into();
    };
    let Some((namespace, property)) = nucleus::expr::extension_parts(&reading.func) else {
        return element.into();
    };
    let value = records
        .iter()
        .find(|record| record["slug"] == reading.slug || record["uid"] == reading.slug)
        .and_then(|record| record["numeric_extensions"].as_array())
        .and_then(|properties| {
            properties
                .iter()
                .find(|value| value["namespace"] == namespace && value["property"] == property)
        })
        .and_then(|value| value["value"].as_str());
    value.map_or_else(|| element.into(), |value| format!("{element} · {value}"))
}


pub fn insert_element(text: &str, element: &str, kind: RuleFieldKind) -> String {
    if kind == RuleFieldKind::Threshold {
        return element.into();
    }
    let start = text
        .rfind(|character: char| {
            character.is_whitespace()
                || matches!(character, '(' | ')' | '*' | '/' | '+' | '=' | ',')
        })
        .map_or(0, |at| at + 1);
    let tail = &text[start..];
    if !tail.is_empty() && element.to_lowercase().contains(&tail.to_lowercase()) {
        format!("{}{element}", &text[..start])
    } else if text.is_empty() || text.ends_with(['(', ' ']) {
        format!("{text}{element}")
    } else {
        format!("{text} {element}")
    }
}

pub fn insert_at(
    text: &str,
    selection: std::ops::Range<usize>,
    element: &str,
    kind: RuleFieldKind,
) -> String {
    if kind == RuleFieldKind::Threshold {
        return element.into();
    }
    let Some(before) = text.get(..selection.start) else {
        return text.into();
    };
    let Some(after) = text.get(selection.end..) else {
        return text.into();
    };
    if !selection.is_empty() {
        return format!("{before}{element}{after}");
    }
    if element.starts_with("extension(")
        && let Some((prefix, tail)) = before.rsplit_once("extension(")
        && !tail.contains(')')
    {
        let after = after.strip_prefix(')').unwrap_or(after);
        return format!("{prefix}{element}{after}");
    }
    let mut element = element.to_owned();
    if element.starts_with("freq(")
        && before
            .rsplit_once("freq(")
            .is_some_and(|(_, tail)| !tail.contains(['(', ')']))
    {
        element = element
            .strip_prefix("freq(")
            .unwrap()
            .trim_end_matches(')')
            .into();
        if !after.starts_with(')') {
            element.push(')');
        }
    }
    format!("{}{after}", insert_element(before, &element, kind))
}

pub fn fragments(text: &str) -> Vec<(String, Option<String>)> {
    let mut parts = Vec::new();
    let mut from = 0;
    let mut quoted = false;
    let mut escaped = false;
    for (at, character) in text.char_indices() {
        if quoted {
            if character == '"' && !escaped {
                quoted = false;
            }
            escaped = character == '\\' && !escaped;
            continue;
        }
        if character == '"' {
            quoted = true;
            continue;
        }
        if character != '@' || at < from {
            continue;
        }
        let end = text[at + 1..]
            .char_indices()
            .find(|(_, c)| !c.is_alphanumeric() && !matches!(c, '-' | '_' | '.'))
            .map_or(text.len(), |(offset, _)| at + 1 + offset);
        if end == at + 1 {
            continue;
        }
        if from < at {
            parts.push((text[from..at].into(), None));
        }
        parts.push((text[at..end].into(), Some(text[at + 1..end].into())));
        from = end;
    }
    if from < text.len() {
        parts.push((text[from..].into(), None));
    }
    parts
}

pub fn reading_at(source: &str, start: usize, end: usize) -> String {
    let prefix = &source[..start];
    if let Some(open) = prefix.rfind('(') {
        let name_start = prefix[..open]
            .rfind(|character: char| !character.is_ascii_alphanumeric() && character != '_')
            .map_or(0, |at| at + 1);
        let name = &prefix[name_start..open];
        let mut quoted = false;
        let mut escaped = false;
        let mut depth = 1;
        let close = source[end..]
            .char_indices()
            .find_map(|(offset, character)| {
                if quoted {
                    if character == '"' && !escaped {
                        quoted = false;
                    }
                    escaped = character == '\\' && !escaped;
                    return None;
                }
                match character {
                    '"' => quoted = true,
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(offset);
                        }
                    }
                    _ => {}
                }
                None
            });
        if !name.is_empty()
            && let Some(close) = close
        {
            let reading = &source[name_start..end + close + 1];
            if nucleus::karma::rule_field::check_condition_source(reading).is_ok()
                && nucleus::karma::Condition::parse(reading).is_ok()
            {
                return reading.into();
            }
        }
    }
    source[start..end].into()
}

pub fn frequency_hint(frequency: &Value, now_ms: i64) -> String {
    FrequencyHint::new(frequency, now_ms).to_string()
}

#[derive(Debug, PartialEq, Eq)]
pub struct FrequencyHint<'a>(FrequencyHintValue<'a>);

#[derive(Debug, PartialEq, Eq)]
enum FrequencyHintValue<'a> {
    Inactive(std::borrow::Cow<'a, str>),
    Scheduled {
        next: i64,
        offset: Option<i32>,
        seconds: u64,
    },
}

impl<'a> FrequencyHint<'a> {
    pub fn new(frequency: &'a Value, now_ms: i64) -> Self {
        let Some(next) = frequency["next_at_ms"].as_i64() else {
            return Self(FrequencyHintValue::Inactive(
                frequency["status"].as_str().unwrap_or("inactive").into(),
            ));
        };
        Self(FrequencyHintValue::Scheduled {
            next,
            offset: chrono::DateTime::from_timestamp_millis(next).map(|date| {
                date.with_timezone(&chrono::Local)
                    .offset()
                    .local_minus_utc()
            }),
            seconds: (next.saturating_sub(now_ms).max(0) as u64).div_ceil(1000),
        })
    }

    pub fn scheduled(&self) -> bool {
        matches!(self.0, FrequencyHintValue::Scheduled { .. })
    }

    pub fn into_owned(self) -> FrequencyHint<'static> {
        FrequencyHint(match self.0 {
            FrequencyHintValue::Inactive(status) => {
                FrequencyHintValue::Inactive(status.into_owned().into())
            }
            FrequencyHintValue::Scheduled {
                next,
                offset,
                seconds,
            } => FrequencyHintValue::Scheduled {
                next,
                offset,
                seconds,
            },
        })
    }
}

impl std::fmt::Display for FrequencyHint<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            FrequencyHintValue::Inactive(status) => {
                write!(f, "Frequency: {status} · no scheduled beat")
            }
            FrequencyHintValue::Scheduled {
                next,
                offset,
                seconds,
            } => {
                if let Some(offset) = offset {
                    let date = chrono::DateTime::from_timestamp_millis(*next)
                        .expect("prepared Frequency date")
                        .with_timezone(
                            &chrono::FixedOffset::east_opt(*offset).expect("local offset"),
                        );
                    write!(f, "Next: {}\n", date.format("%Y-%m-%d %H:%M:%S %:z"))?;
                } else {
                    write!(f, "Next: {next}\n")?;
                }
                if *seconds == 0 {
                    f.write_str("Due now · waiting for the scheduler")
                } else {
                    write!(
                        f,
                        "In {}d {:02}h {:02}m {:02}s",
                        seconds / 86400,
                        seconds / 3600 % 24,
                        seconds / 60 % 60,
                        seconds % 60
                    )
                }
            }
        }
    }
}

#[cfg(test)]
mod extension_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn automatic_call_template_is_editable_and_uses_the_shared_consequence_parser() {
        let choices = suggestions(RuleFieldKind::Consequence, "start_call", &[], &[], &[]);
        let template = choices
            .iter()
            .find_map(|choice| match choice {
                Suggestion::Element(value) if value.contains("start_call") => Some(value),
                _ => None,
            })
            .unwrap();
        let source = insert_at("@family", 7..7, template, RuleFieldKind::Consequence);
        assert!(nucleus::karma::rule_field::RuleConsequence::parse(&source).is_ok());
    }

    #[test]
    fn native_choices_insert_namespace_property_and_show_the_current_value() {
        let records = vec![
            json!({"slug":"apples.stock", "numeric_extensions":[{"namespace":"shop.inventory","property":"price","value":"1.25"}]}),
        ];
        let reading = r#"extension(@apples.stock, "shop.inventory", "price")"#;
        let choices = suggestions(RuleFieldKind::Condition, "extension", &[], &records, &[]);
        assert!(
            choices
                .iter()
                .any(|choice| matches!(choice, Suggestion::Element(value) if value == reading))
        );
        assert_eq!(
            element_label(reading, &records),
            format!("{reading} · 1.25")
        );
        assert_eq!(
            insert_at("", 0..0, reading, RuleFieldKind::Condition),
            reading
        );
        let partial = "2 * extension(@apples.stock, \"shop";
        assert_eq!(
            insert_at(
                partial,
                partial.len()..partial.len(),
                reading,
                RuleFieldKind::Condition
            ),
            format!("2 * {reading}")
        );
        let start = reading.find('@').unwrap();
        let end = start + "@apples.stock".len();
        assert_eq!(reading_at(reading, start, end), reading);
        let unusual = r#"extension(@record, "shop(wholesale)", "weight (kg)")"#;
        let start = unusual.find('@').unwrap();
        assert_eq!(reading_at(unusual, start, start + "@record".len()), unusual);
        let literals = r##"extension(@record, "@namespace", "#property")"##;
        assert_eq!(
            fragments(literals)
                .iter()
                .filter(|(_, reference)| reference.is_some())
                .count(),
            1
        );
    }
}
pub mod schedules;
