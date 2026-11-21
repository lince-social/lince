//! Karma queries combine typed definition, schedule, and execution history rows.

use std::fmt::Display;

use serde_json::Value;
use store::{Store, StoreError};

use crate::{Protein, ProteinError};

mod candidates;
mod definitions;
mod history;
mod query;
mod schedule;

pub(crate) async fn execute_karma(
    store: &Store,
    protein: &Protein,
) -> Result<Vec<Value>, ProteinError> {
    if protein.aggregate.is_some() {
        return Err(karma_query_error(
            "protein_karma_aggregate_unsupported",
            "Karma object unions cannot be aggregated",
        ));
    }
    let include = &protein.include;
    if include.facts.is_some()
        || include.promises.is_some()
        || include.links.is_some()
        || include.threads.is_some()
        || include.availability
        || include.extension.is_some()
        || include.numeric_extensions
        || include.projection.is_some()
        || include.contact
        || include.conversations
    {
        return Err(karma_query_error(
            "protein_karma_include_unsupported",
            "Karma rows are already complete typed projections",
        ));
    }
    let mut rows = Vec::new();
    let pool = &store.pool;
    definitions::append(pool, &mut rows).await?;
    schedule::append(pool, &mut rows).await?;
    history::append(pool, &mut rows).await?;
    candidates::append(pool, &mut rows).await?;
    query::finish(rows, protein)
}

pub(super) fn karma_query_error(code: &str, message: impl Display) -> ProteinError {
    StoreError::Protocol(format!("{code}:{message}"))
}
