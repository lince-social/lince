//! Adapts storage-independent query validation to the executor's error contract.

use crate::{Protein, ProteinError};
use protein_authority::query::{self, ValidationError};
use store::sqlx::Error::Protocol;

pub(crate) fn protocol(error: ValidationError) -> ProteinError {
    Protocol(error.into_reason())
}

/// Checks which filters and includes the read executor can accept for a query.
///
/// # Errors
///
/// Returns a protocol error for unsupported combinations or invalid operands.
pub fn validate(protein: &Protein) -> Result<(), ProteinError> {
    query::validate(protein).map_err(protocol)
}
