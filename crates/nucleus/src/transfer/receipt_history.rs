//! Public receipt history projection, provenance, and merged views.

mod projection;
mod view;

pub use projection::{Copy, Projection, Provenance, project, project_copy, source_key};
pub use view::{Outcome, Slot, View, merge};

pub(super) use projection::Projector;

#[cfg(test)]
mod tests;
