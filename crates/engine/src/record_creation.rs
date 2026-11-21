//! A [Draft] supplies the fields for an atomic Record creation. Its UID also
//! identifies retries. Each [Assertion] is saved with the Record.

mod authority;
mod persistence;
mod validation;

pub(crate) use validation::{AssertionFields, DraftFields};

use crate::EngineError;
use nucleus::{DecimalValue, new_uid};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Assertion {
    pub predicate: String,
    pub object: Option<String>,
    pub quantity: Option<String>,
    pub unit: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub uid: String,
    pub head: String,
    pub body: String,
    pub slug: Option<String>,
    pub quantity: String,
    pub assertions: Vec<Assertion>,
    pub work: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_origin: Option<String>,
}

impl Default for Draft {
    fn default() -> Self {
        Self {
            uid: new_uid("r"),
            head: String::new(),
            body: String::new(),
            slug: None,
            quantity: "0".into(),
            assertions: Vec::new(),
            work: json!({}),
            file_origin: None,
        }
    }
}

fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::Consequence(message.into())
}

impl Draft {
    /// Checks draft fields and returns the exact quantity before any Store writes.
    ///
    /// # Errors
    ///
    /// Returns a validation error for malformed fields, work metadata, or quantity.
    pub fn validate(&self) -> Result<DecimalValue, EngineError> {
        DraftFields {
            uid: &self.uid,
            head: &self.head,
            body: &self.body,
            slug: self.slug.as_deref(),
            quantity: &self.quantity,
            assertions: self.assertions.iter().map(|assertion| AssertionFields {
                predicate: &assertion.predicate,
                object: assertion.object.as_deref(),
                quantity: assertion.quantity.as_deref(),
                unit: assertion.unit.as_deref(),
            }),
            work: &self.work,
        }
        .validate()
    }
}
