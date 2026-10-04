use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Toggle(bool),
    Number(f32),
    Choice(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Control {
    Toggle,
    Number { min: f32, max: f32, step: f32 },
    Choice(Vec<String>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Definition {
    pub id: String,
    pub label: String,
    pub default: Value,
    pub control: Control,
}

impl Definition {
    pub fn accepts(&self, value: &Value) -> bool {
        match (&self.control, value) {
            (Control::Toggle, Value::Toggle(_)) => true,
            (Control::Number { min, max, step }, Value::Number(value)) => {
                min.is_finite()
                    && max.is_finite()
                    && step.is_finite()
                    && max > min
                    && (max - min).is_finite()
                    && *step >= 0.000001
                    && value.is_finite()
                    && value >= min
                    && value <= max
                    && (value == max
                        || (((value - min) / step).round() - (value - min) / step).abs() < 0.001)
            }
            (Control::Choice(choices), Value::Choice(value)) => choices.contains(value),
            _ => false,
        }
    }

    pub fn valid(&self) -> bool {
        !self.id.is_empty()
            && self.id.len() <= 80
            && self
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            && !self.label.trim().is_empty()
            && self.label.chars().count() <= 80
            && self.accepts(&self.default)
            && match &self.control {
                Control::Choice(choices) => {
                    !choices.is_empty()
                        && choices.len() <= 32
                        && choices
                            .iter()
                            .all(|choice| !choice.is_empty() && choice.len() <= 80)
                }
                _ => true,
            }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declarations_enforce_type_range_step_choices_and_defaults() {
        let number = Definition {
            id: "amount".into(),
            label: "Amount".into(),
            default: Value::Number(5.0),
            control: Control::Number {
                min: 0.0,
                max: 10.0,
                step: 0.5,
            },
        };
        assert!(number.valid());
        for value in [
            Value::Number(f32::NAN),
            Value::Number(f32::INFINITY),
            Value::Number(-1.0),
            Value::Number(11.0),
            Value::Number(5.1),
            Value::Toggle(true),
        ] {
            assert!(!number.accepts(&value));
        }
        let choice = Definition {
            id: "mode".into(),
            label: "Mode".into(),
            default: Value::Choice("Scroll".into()),
            control: Control::Choice(vec!["Scroll".into(), "Grow".into()]),
        };
        assert!(choice.valid());
        assert!(!choice.accepts(&Value::Choice("Other".into())));
        assert_eq!(Values::default().resolve(&number), Value::Number(5.0));
        let values = Values(BTreeMap::from([("unknown".into(), Value::Number(5.0))]));
        assert!(!values.valid(&[number]));
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Values(pub BTreeMap<String, Value>);

impl Values {
    pub fn valid(&self, definitions: &[Definition]) -> bool {
        definitions.len() <= 32
            && definitions.iter().all(Definition::valid)
            && self.0.iter().all(|(id, value)| {
                definitions
                    .iter()
                    .find(|definition| definition.id == *id)
                    .is_some_and(|definition| definition.accepts(value))
            })
    }

    pub fn resolve(&self, definition: &Definition) -> Value {
        self.0
            .get(&definition.id)
            .filter(|value| definition.accepts(value))
            .unwrap_or(&definition.default)
            .clone()
    }
}
