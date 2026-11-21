//! Discovery request deadlines and bounded pages of artifact hints.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    super::serving::{MAX_ARTIFACT_BYTES, SourceKind},
    MAX_CURSOR_BYTES, MAX_HEADERS, Question,
};
use crate::{karma::CanonicalHash, valid_uid};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub id: String,
    pub question: Question,
    pub issued_at: i64,
    pub deadline: i64,
    pub after: Option<String>,
}

impl Request {
    pub fn validate(&self, now: i64) -> Result<(), String> {
        self.question.validate()?;
        if !valid_uid(&self.id, "ask")
            || self.issued_at <= 0
            || self.issued_at > now.saturating_add(5)
            || !(1..=15).contains(&self.deadline.saturating_sub(self.issued_at))
            || self.deadline <= now
            || self.deadline > now.saturating_add(20)
            || !valid_cursor(self.after.as_deref())
        {
            return Err(
                "Choose a fresh discovery question with a deadline of at most 15 seconds".into(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Header {
    pub hash: CanonicalHash,
    pub kind: SourceKind,
    pub bytes: u32,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub id: String,
    pub question: CanonicalHash,
    pub headers: Vec<Header>,
    pub next: Option<String>,
    pub partial: bool,
}

impl Page {
    /// Verify that the page answers the original request within its freshness and size limits.
    ///
    /// # Errors
    ///
    /// The request is invalid, or the page is misbound or exceeds its limits.
    pub fn validate(&self, request: &Request, now: i64) -> Result<(), String> {
        request.validate(now)?;
        if self.id != request.id
            || self.question != request.question.validated_hash()?
            || !valid_headers(&self.headers, now)
            || !valid_cursor(self.next.as_deref())
        {
            return Err("Discovery answer differs from its bounded original question".into());
        }
        Ok(())
    }
}

fn valid_cursor(cursor: Option<&str>) -> bool {
    cursor.is_none_or(|cursor| !cursor.is_empty() && cursor.len() <= MAX_CURSOR_BYTES)
}

fn valid_headers(headers: &[Header], now: i64) -> bool {
    if headers.len() > MAX_HEADERS {
        return false;
    }
    let mut headers = headers.iter();
    while let Some(header) = headers.next() {
        if header.bytes == 0
            || usize::try_from(header.bytes).map_or(true, |bytes| bytes > MAX_ARTIFACT_BYTES)
            || header.expires_at <= now
            || headers
                .as_slice()
                .iter()
                .any(|other| other.hash == header.hash)
        {
            return false;
        }
    }
    true
}
