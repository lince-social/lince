//! Public profile claims, signed documents, and evidence support.

pub mod commands;
mod document;
pub mod recipes;
mod statement;
mod support;

pub use document::{Document, MAX_DOCUMENT_BYTES};
pub use statement::{Authorship, Category, Comparator, Descriptor, Metric, Statement};
pub use support::{Support, SupportState, compare, trusted_issuer_for_activity};

pub const NAMESPACE: &str = "lince.profile-claim";

#[cfg(test)]
mod tests;
