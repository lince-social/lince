//! Trust Analysis query policy and evidence evaluation.

mod aggregate;
mod analysis;
mod evaluate;
mod policy;

pub use analysis::{Analysis, EvidenceRow, Group, ProtocolRow, SourceEvidence, Withheld};
pub use evaluate::analyze;
pub use policy::{CounterpartyBasis, EvidenceLevel, NAMESPACE, Query, Weights};

#[cfg(test)]
mod tests;
