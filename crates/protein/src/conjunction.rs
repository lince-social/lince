//! Flattens conjunctions for sources that consume a list of leaf predicates.

use std::borrow::Cow;

use nucleus::performance::{Span, add};
use store::sqlx::Error::Protocol;

use crate::{Predicate, Protein, ProteinError, Source};

pub(super) fn prepare(query: &Protein) -> Result<Cow<'_, Protein>, ProteinError> {
    if matches!(
        query.source,
        Source::Record
            | Source::Calendar
            | Source::Schedule
            | Source::Promise
            | Source::Transfer
            | Source::Karma
    ) || !query
        .filter
        .iter()
        .any(|p| matches!(p, Predicate::All(_) | Predicate::Any(_) | Predicate::Not(_)))
    {
        return Ok(Cow::Borrowed(query));
    }
    let _preparation = Span::new("protein-conjunction-preparation");
    let mut filter = Vec::with_capacity(query.filter.len());
    for predicate in &query.filter {
        append_leaves(predicate, &mut filter)?;
    }
    add(
        "protein-conjunction-leaves",
        u64::try_from(filter.len()).unwrap_or(u64::MAX),
    );
    Ok(Cow::Owned(Protein {
        source: query.source,
        filter,
        fields: query.fields.clone(),
        include: query.include.clone(),
        aggregate: query.aggregate.clone(),
        order: query.order.clone(),
        limit: query.limit,
    }))
}

fn append_leaves(predicate: &Predicate, output: &mut Vec<Predicate>) -> Result<(), ProteinError> {
    match predicate {
        Predicate::All(children) => children
            .iter()
            .try_for_each(|child| append_leaves(child, output)),
        Predicate::Any(_) | Predicate::Not(_) => Err(Protocol(
            "protein_filter_operator_unsupported:this source supports All conditions only".into(),
        )),
        leaf => {
            output.push(leaf.clone());
            Ok(())
        }
    }
}
