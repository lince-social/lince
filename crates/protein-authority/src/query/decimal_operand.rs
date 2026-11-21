//! Wire adapter for canonical decimal operands.

use std::fmt;

use nucleus::DecimalValue;
use serde::{
    Deserializer, Serializer,
    de::{Error, Visitor},
};

const MAX_DECIMAL_OPERAND_BYTES: usize = 64;

pub(super) fn serialize<S: Serializer>(
    value: &DecimalValue,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_str(value)
}

pub(super) fn deserialize<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<DecimalValue, D::Error> {
    deserializer.deserialize_str(DecimalOperandVisitor)
}

struct DecimalOperandVisitor;

impl Visitor<'_> for DecimalOperandVisitor {
    type Value = DecimalValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a canonical decimal string of at most 64 bytes")
    }

    fn visit_str<E: Error>(self, value: &str) -> Result<Self::Value, E> {
        if value.len() > MAX_DECIMAL_OPERAND_BYTES {
            return Err(E::custom("decimal operand exceeds its byte limit"));
        }
        let parsed = DecimalValue::parse_inferred(value).map_err(E::custom)?;
        if parsed.to_string() != value {
            return Err(E::custom("decimal operand is not canonical"));
        }
        Ok(parsed)
    }
}
