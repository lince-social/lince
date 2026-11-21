//! Bounded recovery journal for unsent Record edits.

mod reads;
mod writes;

use std::{collections::HashSet, fmt};

use nucleus::valid_uid;

use crate::StoreError;

pub use reads::{history_if_current, metadata, replay};
pub use writes::{retire, save};

/// Maximum combined bytes retained by a recovery journal.
pub const MAX_BYTES: usize = 64 * 1024 * 1024;
/// Maximum bytes in a single recovery checkpoint.
pub const MAX_CHECKPOINT_BYTES: usize = 24 * 1024 * 1024;
/// Maximum number of pending requests or submitted request payloads.
pub const MAX_REQUESTS: usize = MAX_BYTES / 64;
const MAX_UPDATES: usize = 4096;

/// Optimistic concurrency stamp for a recovery journal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stamp {
    /// Current journal generation.
    pub generation: String,
    /// Sequence of the latest write in that generation.
    pub sequence: i64,
}

/// Bounded metadata describing a saved recovery journal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Metadata {
    /// Current generation and sequence.
    pub stamp: Stamp,
    /// Pending request identities in replay order.
    pub pending: Vec<String>,
    /// Number of updates retained after the checkpoint.
    pub update_count: usize,
    /// Combined bytes of the retained updates.
    pub update_bytes: usize,
    /// Combined bytes of the pending request payloads.
    pub request_bytes: usize,
    /// Bytes in the retained checkpoint.
    pub checkpoint_bytes: usize,
    /// Fingerprint used to recognize a retried write to the current generation.
    pub write_hash: String,
}

/// A request retained for ordered recovery.
#[derive(Clone)]
pub struct PendingRequest {
    /// Request identity.
    pub uid: String,
    /// Serialized request payload.
    pub payload: String,
}

impl fmt::Debug for PendingRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PendingRequest")
            .field("uid", &self.uid)
            .field("payload_bytes", &self.payload.len())
            .finish_non_exhaustive()
    }
}

/// One consistent recovery snapshot, including its pending requests.
pub struct Replay {
    /// Metadata read in the same transaction as the journal contents.
    pub metadata: Metadata,
    /// Retained checkpoint.
    pub checkpoint: String,
    /// Updates after the checkpoint, in sequence order.
    pub updates: Vec<Vec<u8>>,
    /// Requests in metadata order.
    pub requests: Vec<PendingRequest>,
}

impl fmt::Debug for Replay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Replay")
            .field("metadata", &self.metadata)
            .finish_non_exhaustive()
    }
}

/// Retained edits used to compact the recovery journal.
pub struct History {
    /// Retained checkpoint.
    pub checkpoint: String,
    /// Updates after the checkpoint, in sequence order.
    pub updates: Vec<Vec<u8>>,
}

impl fmt::Debug for History {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("History")
            .field("checkpoint_bytes", &self.checkpoint.len())
            .field("update_count", &self.updates.len())
            .finish_non_exhaustive()
    }
}

/// An optimistic edit to one recovery journal.
pub struct Write {
    /// Expected stamp, when updating an existing generation.
    pub expected: Option<Stamp>,
    /// Generation to write.
    pub generation: String,
    /// Replacement checkpoint, if any.
    pub checkpoint: Option<String>,
    /// Request identities still pending after the write, in replay order.
    pub pending: Vec<String>,
    /// New or unchanged payloads for those pending requests.
    pub requests: Vec<PendingRequest>,
    /// Updates to append in batch order.
    pub updates: Vec<Vec<u8>>,
}

impl fmt::Debug for Write {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Write")
            .field("expected", &self.expected)
            .field("generation", &self.generation)
            .field(
                "checkpoint_bytes",
                &self.checkpoint.as_ref().map(String::len),
            )
            .field("pending_count", &self.pending.len())
            .field("request_count", &self.requests.len())
            .field("update_count", &self.updates.len())
            .finish_non_exhaustive()
    }
}

fn invalid(message: impl Into<String>) -> StoreError {
    StoreError::Protocol(message.into())
}

fn identity(value: &str, maximum: usize) -> Result<(), StoreError> {
    if value.is_empty() || value.len() > maximum || value.contains('\0') {
        return Err(invalid("Invalid recovery journal identity"));
    }
    Ok(())
}

fn scope(source: &str, record: &str) -> Result<(), StoreError> {
    identity(source, 8192)?;
    if !valid_uid(record, "r") {
        return Err(invalid("Recovery journal requires a Record uid"));
    }
    Ok(())
}

fn pending(ids: &[String]) -> Result<HashSet<&str>, StoreError> {
    if ids.len() > MAX_REQUESTS {
        return Err(invalid("Too many pending recovery requests"));
    }
    let mut unique = HashSet::with_capacity(ids.len());
    for id in ids {
        identity(id, 128)?;
        if !unique.insert(id.as_str()) {
            return Err(invalid("Duplicate recovery request identity"));
        }
    }
    Ok(unique)
}

fn bounded_sum(values: impl IntoIterator<Item = usize>) -> Result<usize, StoreError> {
    values.into_iter().try_fold(0_usize, |sum, value| {
        sum.checked_add(value)
            .filter(|sum| *sum <= MAX_BYTES)
            .ok_or_else(|| invalid("Recovery journal exceeds its storage limit"))
    })
}
