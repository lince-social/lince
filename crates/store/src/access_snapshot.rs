//! Bounded SQLite snapshots consumed by the authorization evaluator.

mod content;
mod metadata;
mod model;
mod sql;
mod validate;

pub use content::content_on;
pub use metadata::metadata_on;
pub use model::{
    AccessMetadataSnapshot, AccessSnapshotLimits, AssertionMetadata, AssertionRole,
    ConceptMetadata, ConceptParentMetadata, ExtensionContent, LinguaConceptMetadata,
    LinguaMetadata, LinguaVisibility, MessageDraftMetadata, RecordMetadata, TargetRecordContent,
    VisibilityGrant, VisibilityRuleMetadata, VisibilitySubject, VisibilityTarget,
};

use crate::StoreError;

fn protocol(message: impl Into<String>) -> StoreError {
    StoreError::Protocol(message.into())
}
