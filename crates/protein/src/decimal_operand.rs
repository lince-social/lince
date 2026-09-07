use std::fmt;

use nucleus::DecimalValue;
use serde::de::Visitor;
use serde::{Deserializer, Serializer};

pub const MAX_DECIMAL_OPERAND_BYTES: usize = 64;

pub fn serialize<S>(value: &DecimalValue, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&value.to_string())
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<DecimalValue, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_str(DecimalOperandVisitor)
}

struct DecimalOperandVisitor;

impl<'de> Visitor<'de> for DecimalOperandVisitor {
    type Value = DecimalValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a canonical decimal string of at most 64 bytes")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
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
