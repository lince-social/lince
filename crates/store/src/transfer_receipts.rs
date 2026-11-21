//! Store signed Transfer receipts, approved source mappings, and receipt versions.

use crate::StoreError;

mod read;
mod request;
mod save;
mod source;

pub use read::{
    Saved, VersionHeader, find, find_publication, get, get_on, get_version, list, replay, versions,
};
pub use save::{Commit, Input, save};
pub use source::{local_source, source_for_reviewed_scope, source_for_scope};

fn invalid(message: impl ToString) -> StoreError {
    StoreError::Protocol(message.to_string())
}
