//! Requests for exact numeric Record fields.

use super::ValidationError;
use nucleus::{
    performance::{Span, add},
    shader_effect::bindings::{NumericField, validate_record_field},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub record: String,
    pub field: NumericField,
}

/// Checks the request count and each Record UID and numeric field path.
///
/// # Errors
/// Returns a validation error for an oversized batch or malformed request.
pub fn validate(requests: &[Request]) -> Result<(), ValidationError> {
    let _validation = Span::new("numeric-projection-validation");
    if requests.len() > 128 {
        return Err(ValidationError(
            "Numeric projections support at most 128 requested fields.".into(),
        ));
    }
    for request in requests {
        add("numeric-projection-validation-attempts", 1);
        validate_record_field(&request.record, &request.field).map_err(ValidationError)?;
    }
    Ok(())
}
