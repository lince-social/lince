use base64::{Engine, prelude::BASE64_STANDARD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    json::lowercase_hex,
    performance::{self, Span},
};

mod restoration;
mod validation;

pub use restoration::{Restoration, RestoringPart};
pub use validation::validate;

use validation::{decode_attachment, validate_with_attachments};

/// Maximum total size of message content, counting decoded attachment bytes.
pub const MAX_CONTENT_BYTES: usize = 4 * 1024 * 1024;
/// Maximum number of ordered content parts per message.
pub const MAX_PARTS: usize = 16;
/// Maximum number of Base64 characters per persisted attachment chunk.
pub const CHUNK_BYTES: usize = 256 * 1024;

#[expect(
    missing_docs,
    clippy::exhaustive_enums,
    reason = "Rustdoc feeds the published schema and callers match every variant"
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MessagePart {
    Question {
        question: crate::question::Question,
    },
    Steps {
        steps: Vec<crate::operation::Step>,
    },
    Text {
        text: String,
    },
    Attachment {
        name: String,
        mime_type: String,
        data: String,
    },
    Reference {
        name: String,
        uri: String,
    },
}

#[expect(
    missing_docs,
    clippy::exhaustive_enums,
    reason = "Rustdoc feeds the published schema and callers match every variant"
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StoredPart {
    Question {
        question: crate::question::Question,
    },
    Steps {
        steps: Vec<crate::operation::Step>,
    },
    Text {
        text: String,
    },
    Attachment {
        name: String,
        mime_type: String,
        bytes: usize,
        sha256: String,
        chunks: usize,
    },
    Reference {
        name: String,
        uri: String,
    },
}

/// Returns the lowercase hexadecimal SHA-256 digest of attachment bytes.
pub fn digest(bytes: &[u8]) -> String {
    lowercase_hex(&Sha256::digest(bytes))
}

/// Decodes a standard Base64 attachment, limited to [`MAX_CONTENT_BYTES`].
///
/// # Errors
///
/// Returns a size or encoding diagnostic for an invalid attachment.
#[expect(
    clippy::map_err_ignore,
    reason = "invalid Base64 uses one stable client diagnostic"
)]
pub fn decode(data: &str) -> Result<Vec<u8>, String> {
    if data.len() > MAX_CONTENT_BYTES.div_ceil(3) * 4 {
        return Err("Attachments exceed the 4 MiB message limit.".into());
    }
    let bytes = BASE64_STANDARD
        .decode(data)
        .map_err(|_| "The attachment encoding is invalid.")?;
    if bytes.len() > MAX_CONTENT_BYTES {
        return Err("Attachments exceed the 4 MiB message limit.".into());
    }
    Ok(bytes)
}

/// Validates content and produces descriptors in the same order for storage.
///
/// # Errors
///
/// Returns the first validation error when the content cannot be stored.
pub fn describe(parts: &[MessagePart]) -> Result<Vec<StoredPart>, String> {
    let _preparation = Span::new("message-content-description");
    let mut attachments = Vec::new();
    validate_with_attachments(parts, decode_attachment, |bytes| {
        performance::add("message-description-attachments-decoded", 1);
        performance::add(
            "message-description-bytes-decoded",
            u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        );
        attachments.push((bytes.len(), digest(&bytes)));
    })?;
    let mut attachments = attachments.into_iter();
    parts
        .iter()
        .map(|part| {
            Ok(match part {
                MessagePart::Question { question } => StoredPart::Question {
                    question: question.clone(),
                },
                MessagePart::Steps { steps } => StoredPart::Steps {
                    steps: steps.clone(),
                },
                MessagePart::Text { text } => StoredPart::Text { text: text.clone() },
                MessagePart::Reference { name, uri } => StoredPart::Reference {
                    name: name.clone(),
                    uri: uri.clone(),
                },
                MessagePart::Attachment {
                    name,
                    mime_type,
                    data,
                } => {
                    let (bytes, sha256) = attachments
                        .next()
                        .ok_or("The validated attachment descriptor is missing.")?;
                    StoredPart::Attachment {
                        name: name.clone(),
                        mime_type: mime_type.clone(),
                        bytes,
                        sha256,
                        chunks: data.len().div_ceil(CHUNK_BYTES).max(1),
                    }
                }
            })
        })
        .collect()
}

/// Returns the persisted namespace for the indexed part and attachment chunk.
pub fn chunk_namespace(part: usize, chunk: usize) -> String {
    format!("lince.message-data.{part}.{chunk}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_validation_bounds_data_names_and_part_count() {
        let valid = MessagePart::Attachment {
            name: "note.txt".into(),
            mime_type: "text/plain".into(),
            data: BASE64_STANDARD.encode(b"note"),
        };
        assert!(validate(std::slice::from_ref(&valid)).is_ok());
        assert!(validate(&vec![valid; MAX_PARTS + 1]).is_err());
        for (name, mime_type, data) in [
            ("../secret", "text/plain", "YQ=="),
            ("note", "text/plain\r\nsecret", "YQ=="),
            ("note", "text/plain", "bad!"),
        ] {
            assert!(
                validate(&[MessagePart::Attachment {
                    name: name.into(),
                    mime_type: mime_type.into(),
                    data: data.into()
                }])
                .is_err()
            );
        }
        assert!(decode(&"A".repeat(MAX_CONTENT_BYTES * 2)).is_err());
    }
}
