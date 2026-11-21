//! Bounded Record document storage, generation metadata, and CRDT journal queries.

mod journal;
mod metadata;
mod snapshots;

#[cfg(test)]
mod tests;

use crate::StoreError;

pub use journal::{
    compaction_due_on, crdt_ops_since, doc_tail, has_crdt_history, records_needing_compaction,
};
pub use metadata::{Qualification, RecordDocMetadata, metadata_on};
pub use snapshots::{
    RecordDocRow, RecordDocSnapshot, delete, get, put, put_on, put_qualified_on, reset_on,
    snapshot_on,
};

/// Maximum byte size of an accepted Record document snapshot.
pub const MAX_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;

fn protocol(message: impl Into<String>) -> StoreError {
    StoreError::Protocol(message.into())
}
