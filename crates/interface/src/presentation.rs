use crate::settings::{Control, Definition, Value, Values};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Layout {
    Sand,
    Castle,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Presentation {
    pub name: String,
    pub layout: Layout,
    pub fields: Vec<String>,
    #[serde(default)]
    pub fills: BTreeMap<String, String>,
    #[serde(default)]
    pub settings: Values,
}

pub fn settings() -> Vec<Definition> {
    vec![
        Definition {
            id: "spacing".into(),
            label: "Space between fields".into(),
            default: Value::Number(8.0),
            control: Control::Number {
                min: 0.0,
                max: 40.0,
                step: 1.0,
            },
        },
        Definition {
            id: "labels".into(),
            label: "Show field labels".into(),
            default: Value::Toggle(true),
            control: Control::Toggle,
        },
    ]
}

impl Presentation {
    pub fn valid(&self) -> bool {
        !self.name.trim().is_empty()
            && self.name.chars().count() <= 80
            && !self.name.chars().any(char::is_control)
            && !self.fields.is_empty()
            && self.fields.len() <= 32
            && self.fields.iter().collect::<BTreeSet<_>>().len() == self.fields.len()
            && self.fields.iter().all(|field| {
                protein::record_schema::fields()
                    .iter()
                    .any(|definition| definition.key == field)
            })
            && self
                .fills
                .iter()
                .all(|(field, value)| self.fields.contains(field) && value.chars().count() <= 4096)
            && self.settings.valid(&settings())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comparison {
    pub matching: Vec<String>,
    pub missing: Vec<String>,
    pub extra: Vec<String>,
}

pub fn compare(original: &[String], target: &[String]) -> Comparison {
    Comparison {
        matching: target
            .iter()
            .filter(|field| original.contains(field))
            .cloned()
            .collect(),
        missing: target
            .iter()
            .filter(|field| !original.contains(field))
            .cloned()
            .collect(),
        extra: original
            .iter()
            .filter(|field| !target.contains(field))
            .cloned()
            .collect(),
    }
}
