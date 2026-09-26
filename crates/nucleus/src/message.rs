use base64::{Engine, prelude::BASE64_STANDARD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MAX_CONTENT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_PARTS: usize = 16;
pub const CHUNK_BYTES: usize = 256 * 1024;

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

pub fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

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

pub fn validate(parts: &[MessagePart]) -> Result<(), String> {
    if parts.len() > MAX_PARTS {
        return Err("Use at most 16 content parts per message.".into());
    }
    let mut bytes = 0;
    for part in parts {
        match part {
            MessagePart::Question { question } => {
                question.validate()?;
                bytes += question.text().len();
            }
            MessagePart::Steps { steps } => {
                crate::operation::validate_steps(steps)?;
                bytes += crate::operation::steps_text(steps).len();
            }
            MessagePart::Text { text } => {
                if text.len() > 65_536 {
                    return Err("A text part exceeds 64 KiB.".into());
                }
                bytes += text.len();
            }
            MessagePart::Attachment {
                name,
                mime_type,
                data,
            } => {
                if name.is_empty()
                    || name.len() > 255
                    || name
                        .chars()
                        .any(|c| c.is_control() || c == '/' || c == '\\')
                {
                    return Err(
                        "Choose a filename without a directory or control characters.".into(),
                    );
                }
                if mime_type.len() > 128
                    || !mime_type.contains('/')
                    || !mime_type
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"/.-+".contains(&c))
                {
                    return Err("The attachment media type is invalid.".into());
                }
                bytes += decode(data)?.len();
            }
            MessagePart::Reference { name, uri } => {
                if name.is_empty()
                    || name.len() > 512
                    || uri.len() > 4096
                    || uri.chars().any(char::is_control)
                    || !["record:", "file:///", "https://", "http://"]
                        .iter()
                        .any(|prefix| uri.starts_with(prefix))
                {
                    return Err("Use a named Record, file, or HTTP resource reference.".into());
                }
                bytes += name.len() + uri.len();
            }
        }
        if bytes > MAX_CONTENT_BYTES {
            return Err("Message contents exceed 4 MiB.".into());
        }
    }
    Ok(())
}

pub fn describe(parts: &[MessagePart]) -> Result<Vec<StoredPart>, String> {
    validate(parts)?;
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
                    let bytes = decode(data)?;
                    StoredPart::Attachment {
                        name: name.clone(),
                        mime_type: mime_type.clone(),
                        bytes: bytes.len(),
                        sha256: digest(&bytes),
                        chunks: data.len().div_ceil(CHUNK_BYTES).max(1),
                    }
                }
            })
        })
        .collect()
}

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
