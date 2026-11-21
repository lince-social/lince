//! Selects and counts Records within a caller-supplied readable snapshot.

use crate::{Protein, authority::GraphSnapshot};

mod budget;
mod output;
mod plan;
mod predicate;
mod snapshot;
mod types;

use self::{
    budget::Budget,
    output::{counts, limit_results, order_records},
    plan::validate_query,
    predicate::{Compiled, compile_all},
    snapshot::Prepared,
};
pub use types::{
    CountGroup, CountRow, QueryError, QueryLimits, QueryResult, QueryRows, ReadableIdentities,
    RecordField, RecordProjection, RecordScalarRow,
};

/// Selects readable Records using a validated snapshot and explicit resource limits.
///
/// # Errors
///
/// Rejects invalid or oversized input, unsupported features, inaccessible operands,
/// and cycles in the relation graphs that the query traverses. Error messages omit
/// data from the snapshot.
pub fn select_records(
    query: &Protein,
    graph: &GraphSnapshot,
    readable: &ReadableIdentities,
    scalars: &[RecordScalarRow],
    limits: &QueryLimits,
) -> Result<QueryResult, QueryError> {
    limits.validate()?;
    let mut budget = Budget::new(limits);
    let validated = validate_query(query, &mut budget)?;
    let prepared = Prepared::new(graph, readable, scalars, &mut budget)?;
    let compiled = Compiled::All(compile_all(&query.filter, &prepared, &mut budget)?);
    let mut selected = Vec::new();
    for &row in prepared.rows.values() {
        budget.step()?;
        if compiled.matches(row, &mut budget)? {
            selected.push(row);
        }
    }
    let rows = match &query.aggregate {
        Some(aggregate) => QueryRows::Counts(counts(
            &selected,
            aggregate.by,
            &prepared,
            query.limit,
            &mut budget,
        )?),
        None => {
            order_records(&mut selected, &validated.order, &mut budget)?;
            limit_results(&mut selected, query.limit, limits.results)?;
            QueryRows::Records(selected.into_iter().map(|row| row.uid.clone()).collect())
        }
    };
    Ok(QueryResult {
        rows,
        projection: validated.projection,
    })
}
