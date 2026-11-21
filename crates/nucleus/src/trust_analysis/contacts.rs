//! Contact disclosures and observed contact relations.

mod document;
mod relations;
mod statement;

pub use document::Document;
pub use relations::{CheckedDisclosure, Edge, RelationKind, permitted_edges};
pub use statement::{Audience, Statement};

pub const NAMESPACE: &str = "lince.contact-disclosure";
pub const MAX_DOCUMENT_BYTES: usize = 32_768;
pub const MAX_TERM_SECONDS: i64 = 366 * 24 * 60 * 60;
