//! Stable JSON encoding and domain-separated SHA-256 hashes for durable Karma data.

use std::fmt::Write as _;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::failure::KarmaBoundaryError;

const CANONICAL_DOMAIN: &[u8] = b"lince.canonical-json.v1\0";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, schemars::JsonSchema)]
#[schemars(with = "String")]
pub struct CanonicalHash(String);

impl CanonicalHash {
    /// Validates an encoded canonical digest.
    ///
    /// # Errors
    ///
    /// Returns an invalid input error if the prefix or hexadecimal digest is malformed.
    pub fn parse(value: impl Into<String>) -> Result<Self, KarmaBoundaryError> {
        let value = value.into();
        let Some(hex) = value.strip_prefix("sha256:") else {
            return Err(KarmaBoundaryError::invalid_input(
                "canonical hash must start with sha256:",
            ));
        };
        if hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            Ok(Self(value))
        } else {
            Err(KarmaBoundaryError::invalid_input(
                "canonical hash must contain 64 lowercase hexadecimal digits",
            ))
        }
    }

    /// Borrows the complete encoded digest, including its algorithm prefix.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for CanonicalHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for CanonicalHash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(de::Error::custom)
    }
}

/// Encodes JSON without extra whitespace and with object keys in lexicographic order.
///
/// # Errors
///
/// Returns an invalid input error for serialization failures or floating-point numbers.
pub fn canonical_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, KarmaBoundaryError> {
    let value = serde_json::to_value(value).map_err(|error| {
        KarmaBoundaryError::invalid_input(format!("value is not JSON serializable: {error}"))
    })?;
    let mut output = Vec::new();
    write_value(&value, &mut output)?;
    Ok(output)
}

fn write_value(value: &Value, output: &mut Vec<u8>) -> Result<(), KarmaBoundaryError> {
    match value {
        Value::Null => output.extend_from_slice(b"null"),
        Value::Bool(value) => output.extend_from_slice(if *value { b"true" } else { b"false" }),
        Value::Number(value) if value.is_i64() || value.is_u64() => {
            write_atom(output, value, "integer")?;
        }
        Value::Number(_) => {
            return Err(KarmaBoundaryError::invalid_input(
                "canonical JSON forbids floating-point numbers; use an exact atom",
            ));
        }
        Value::String(value) => write_atom(output, value, "string")?,
        Value::Array(values) => {
            output.push(b'[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                write_value(value, output)?;
            }
            output.push(b']');
        }
        Value::Object(values) => {
            output.push(b'{');
            let mut sorted: Vec<_> = values.iter().collect();
            sorted.sort_unstable_by_key(|(key, _)| *key);
            for (index, (key, value)) in sorted.into_iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                write_atom(output, key, "object key")?;
                output.push(b':');
                write_value(value, output)?;
            }
            output.push(b'}');
        }
    }
    Ok(())
}

fn write_atom(
    output: &mut Vec<u8>,
    value: &impl Serialize,
    name: &str,
) -> Result<(), KarmaBoundaryError> {
    serde_json::to_writer(output, value).map_err(|error| {
        KarmaBoundaryError::invalid_input(format!("{name} is not JSON serializable: {error}"))
    })
}

/// Hashes canonical JSON under a versioned, dot-separated purpose domain.
///
/// # Errors
///
/// Returns an invalid input error for a malformed domain or a value that cannot
/// be encoded by [`canonical_json_bytes`].
pub fn canonical_hash<T: Serialize>(
    domain: &str,
    value: &T,
) -> Result<CanonicalHash, KarmaBoundaryError> {
    if !valid_purpose_domain(domain) {
        return Err(KarmaBoundaryError::invalid_input(
            "hash domain must be lowercase dot-separated ASCII and end in a version",
        ));
    }
    let bytes = canonical_json_bytes(value)?;
    Ok(hash_canonical_bytes(domain, &bytes))
}

/// Hashes bytes already verified to be canonical JSON under a validated domain.
pub(crate) fn hash_canonical_bytes(domain: &str, bytes: &[u8]) -> CanonicalHash {
    let mut hasher = Sha256::new();
    hasher.update(CANONICAL_DOMAIN);
    hasher.update(domain.as_bytes());
    hasher.update(b"\0");
    hasher.update(bytes);
    let mut encoded = String::with_capacity(71);
    encoded.push_str("sha256:");
    for byte in hasher.finalize() {
        write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    CanonicalHash(encoded)
}

fn valid_purpose_domain(domain: &str) -> bool {
    if domain.len() > 128 {
        return false;
    }
    let mut has_version = false;
    for (index, segment) in domain.split('.').enumerate() {
        if segment.is_empty()
            || !segment
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return false;
        }
        if index > 0
            && segment.strip_prefix('v').is_some_and(|number| {
                !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit())
            })
        {
            has_version = true;
        }
    }
    has_version
}
