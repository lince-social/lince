//! Requests for exact numeric Record fields.

use nucleus::shader_effect::bindings::NumericField;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub record: String,
    pub field: NumericField,
}
