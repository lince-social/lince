use super::{MAX_CONTENT_BYTES, MAX_PARTS, MessagePart, decode};
use crate::operation::{steps_text, validate_steps};

const MAX_TEXT_BYTES: usize = 64 * 1024;
const MAX_ATTACHMENT_NAME_BYTES: usize = 255;
const MAX_MEDIA_TYPE_BYTES: usize = 128;
const MAX_REFERENCE_NAME_BYTES: usize = 512;
const MAX_REFERENCE_URI_BYTES: usize = 4096;

/// Validates the ordered message content and its encoded attachments.
///
/// # Errors
///
/// Returns the first content, attachment encoding, or aggregate size diagnostic.
pub fn validate(parts: &[MessagePart]) -> Result<(), String> {
    validate_with_attachments(parts, decode_attachment, |_| {})
}

pub(super) fn decode_attachment(data: &str) -> Result<(usize, Vec<u8>), String> {
    let decoded = decode(data)?;
    Ok((decoded.len(), decoded))
}

pub(super) fn validate_with_attachments<T>(
    parts: &[MessagePart],
    mut prepare: impl FnMut(&str) -> Result<(usize, T), String>,
    mut attachment: impl FnMut(T),
) -> Result<(), String> {
    if parts.len() > MAX_PARTS {
        return Err("Use at most 16 content parts per message.".into());
    }
    let mut bytes = 0;
    for part in parts {
        let decoded = match part {
            MessagePart::Question { question } => {
                question.validate()?;
                bytes += question.text().len();
                None
            }
            MessagePart::Steps { steps } => {
                validate_steps(steps)?;
                bytes += steps_text(steps).len();
                None
            }
            MessagePart::Text { text } => {
                if text.len() > MAX_TEXT_BYTES {
                    return Err("A text part exceeds 64 KiB.".into());
                }
                bytes += text.len();
                None
            }
            MessagePart::Attachment {
                name,
                mime_type,
                data,
            } => {
                if name.is_empty()
                    || name.len() > MAX_ATTACHMENT_NAME_BYTES
                    || name
                        .chars()
                        .any(|c| c.is_control() || matches!(c, '/' | '\\'))
                {
                    return Err(
                        "Choose a filename without a directory or control characters.".into(),
                    );
                }
                if mime_type.len() > MAX_MEDIA_TYPE_BYTES
                    || !mime_type.contains('/')
                    || !mime_type
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"/.-+".contains(&c))
                {
                    return Err("The attachment media type is invalid.".into());
                }
                let (length, decoded) = prepare(data)?;
                bytes += length;
                Some(decoded)
            }
            MessagePart::Reference { name, uri } => {
                if name.is_empty()
                    || name.len() > MAX_REFERENCE_NAME_BYTES
                    || uri.len() > MAX_REFERENCE_URI_BYTES
                    || uri.chars().any(char::is_control)
                    || !["record:", "file:///", "https://", "http://"]
                        .iter()
                        .any(|prefix| uri.starts_with(prefix))
                {
                    return Err("Use a named Record, file, or HTTP resource reference.".into());
                }
                bytes += name.len() + uri.len();
                None
            }
        };
        if bytes > MAX_CONTENT_BYTES {
            return Err("Message contents exceed 4 MiB.".into());
        }
        if let Some(decoded) = decoded {
            attachment(decoded);
        }
    }
    Ok(())
}
