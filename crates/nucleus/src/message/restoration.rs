// non-lean-yet
// Scope: struct Restoration, struct RestoringPart.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::missing_panics_doc] State the panic conditions for exported functions. Sites
//   28.
// - [Clippy: clippy::as_conversions] Use From or TryFrom when it preserves the conversion contract.
//   Sites 77.
// - [Clippy: clippy::expect_used] Check the stated invariant and handle reachable failures. Sites
//   36.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 16, 28, 54.
// - [rustc: missing_debug_implementations] type does not implement `std::fmt::Debug`; consider
//   adding `#[derive(Debug)]` or a manual implementation. Sites 4, 10.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 4, 10,
//   16, 28, 47, 54.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites finish: String
//   (28, 54), part: String (16).

use super::{MAX_CONTENT_BYTES, MAX_PARTS, MessagePart, StoredPart};

#[derive(Default)]
pub struct Restoration {
    parts: Vec<MessagePart>,
    attachment_bytes: Vec<usize>,
    total_attachment_bytes: usize,
}

pub struct RestoringPart<'a> {
    owner: &'a mut Restoration,
    part: StoredPart,
}

impl Restoration {
    pub fn part(&mut self, part: StoredPart) -> Result<RestoringPart<'_>, String> {
        if self.parts.len() >= MAX_PARTS {
            return Err("Use at most 16 content parts per message.".into());
        }
        if let StoredPart::Attachment { bytes, chunks, .. } = &part
            && (*bytes > MAX_CONTENT_BYTES - self.total_attachment_bytes || *chunks > 24)
        {
            return Err("Message attachments exceed their size limit.".into());
        }
        Ok(RestoringPart { owner: self, part })
    }

    pub fn finish(self) -> Result<Vec<MessagePart>, String> {
        let _validation = crate::performance::Span::new("message-content-restoration");
        let mut attachment_bytes = self.attachment_bytes.into_iter();
        super::validate_with_attachments(
            &self.parts,
            |_| {
                let length = attachment_bytes
                    .next()
                    .expect("each owned attachment retains its verified length");
                crate::performance::add("message-restoration-lengths-reused", 1);
                Ok((length, ()))
            },
            |_| {},
        )?;
        Ok(self.parts)
    }
}

impl RestoringPart<'_> {
    pub fn chunks(&self) -> Option<usize> {
        match &self.part {
            StoredPart::Attachment { chunks, .. } => Some(*chunks),
            _ => None,
        }
    }

    pub fn finish(self, data: Option<String>) -> Result<(), String> {
        let Self { owner, part } = self;
        let part = match (part, data) {
            (StoredPart::Question { question }, None) => MessagePart::Question { question },
            (StoredPart::Steps { steps }, None) => MessagePart::Steps { steps },
            (StoredPart::Text { text }, None) => MessagePart::Text { text },
            (StoredPart::Reference { name, uri }, None) => MessagePart::Reference { name, uri },
            (
                StoredPart::Attachment {
                    name,
                    mime_type,
                    bytes,
                    sha256,
                    ..
                },
                data,
            ) => {
                let data = data.ok_or_else(|| {
                    "This attachment has not arrived or is no longer available.".to_owned()
                })?;
                let _restoration = crate::performance::Span::new("message-attachment-restoration");
                let decoded = super::decode(&data)?;
                crate::performance::add("message-restoration-attachments-decoded", 1);
                crate::performance::add("message-restoration-bytes-decoded", decoded.len() as u64);
                if decoded.len() != bytes || super::digest(&decoded) != sha256 {
                    return Err("The attachment is incomplete or its contents changed.".into());
                }
                owner.attachment_bytes.push(decoded.len());
                owner.total_attachment_bytes += decoded.len();
                MessagePart::Attachment {
                    name,
                    mime_type,
                    data,
                }
            }
            (_, Some(_)) => {
                return Err("Only an attachment part can contain attachment data.".into());
            }
        };
        owner.parts.push(part);
        Ok(())
    }
}
