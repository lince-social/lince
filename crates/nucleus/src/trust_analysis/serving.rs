//! Serving contracts for retained and observed evidence.

mod artifact;
mod chunk;
mod review;

pub use artifact::{Artifact, SourceKind};
pub use chunk::{Chunk, assemble};
pub use review::{Command, Request, Review, Scope, Source};

pub const NAMESPACE: &str = "lince.evidence-serving";
pub const CHUNK_BYTES: usize = 65_536;
pub const MAX_ARTIFACT_BYTES: usize = 800_000;
pub const MAX_STORAGE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_PERMISSIONS: usize = 256;

#[cfg(test)]
mod tests;
