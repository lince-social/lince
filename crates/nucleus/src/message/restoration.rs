use super::{
    MAX_CONTENT_BYTES, MAX_PARTS, MessagePart, StoredPart, decode, digest,
    validation::validate_with_attachments,
};
use crate::performance::{self, Span};

const MAX_RESTORED_CHUNKS: usize = 24;
const INCOMPLETE_ATTACHMENT: &str = "The attachment is incomplete or its contents changed.";

/// Reassembles verified stored content in its original order.
#[expect(
    missing_debug_implementations,
    reason = "stores private message content"
)]
#[derive(Default)]
pub struct Restoration {
    parts: Vec<MessagePart>,
    attachment_bytes: Vec<usize>,
    total_attachment_bytes: usize,
}

/// A stored content part awaiting its optional attachment data.
#[expect(
    missing_debug_implementations,
    reason = "stores private message content"
)]
pub struct RestoringPart<'a> {
    owner: &'a mut Restoration,
    part: StoredPart,
}

impl Restoration {
    /// Starts restoring one part, enforcing the part and declared attachment limits.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic if the part, chunk count, or byte limit is exceeded.
    pub fn part(&mut self, part: StoredPart) -> Result<RestoringPart<'_>, String> {
        if self.parts.len() >= MAX_PARTS {
            return Err("Use at most 16 content parts per message.".into());
        }
        if let StoredPart::Attachment { bytes, chunks, .. } = &part
            && (*bytes > MAX_CONTENT_BYTES - self.total_attachment_bytes
                || *chunks > MAX_RESTORED_CHUNKS)
        {
            return Err("Message attachments exceed their size limit.".into());
        }
        Ok(RestoringPart { owner: self, part })
    }

    /// Returns the restored content after checking its whole-message limits.
    ///
    /// # Errors
    ///
    /// Returns the first validation error for the restored content in part order.
    pub fn finish(self) -> Result<Vec<MessagePart>, String> {
        let _validation = Span::new("message-content-restoration");
        let mut attachment_bytes = self.attachment_bytes.into_iter();
        validate_with_attachments(
            &self.parts,
            |_| {
                let length = attachment_bytes.next().ok_or(INCOMPLETE_ATTACHMENT)?;
                performance::add("message-restoration-lengths-reused", 1);
                Ok((length, ()))
            },
            |_| {},
        )?;
        Ok(self.parts)
    }
}

impl RestoringPart<'_> {
    /// Returns the stored chunk count for an attachment.
    pub fn chunks(&self) -> Option<usize> {
        match &self.part {
            StoredPart::Attachment { chunks, .. } => Some(*chunks),
            _ => None,
        }
    }

    /// Finishes one part, checking attachment data against its stored descriptor.
    ///
    /// # Errors
    ///
    /// Returns an error if required attachment data is missing, invalid, or changed,
    /// or if attachment data is provided for a different content part.
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
                let _restoration = Span::new("message-attachment-restoration");
                let decoded = decode(&data)?;
                let length = decoded.len();
                performance::add("message-restoration-attachments-decoded", 1);
                performance::add(
                    "message-restoration-bytes-decoded",
                    u64::try_from(length).unwrap_or(u64::MAX),
                );
                if length != bytes || digest(&decoded) != sha256 {
                    return Err(INCOMPLETE_ATTACHMENT.into());
                }
                owner.attachment_bytes.push(length);
                owner.total_attachment_bytes += length;
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
