use crate::{Predicate, Protein, ProteinError, Source};
use std::borrow::Cow;

pub(crate) fn prepare(query: &Protein) -> Result<Cow<'_, Protein>, ProteinError> {
    if matches!(
        query.source,
        Source::Record | Source::Promise | Source::Transfer | Source::Karma
    ) {
        return Ok(Cow::Borrowed(query));
    }
    fn append(predicate: &Predicate, output: &mut Vec<Predicate>) -> Result<(), ProteinError> {
        match predicate {
            Predicate::All(children) => {
                for child in children {
                    append(child, output)?;
                }
            }
            Predicate::Any(_) | Predicate::Not(_) => {
                return Err(store::sqlx::Error::Protocol(
                    "protein_filter_operator_unsupported:this source supports All conditions only"
                        .into(),
                ));
            }
            predicate => output.push(predicate.clone()),
        }
        Ok(())
    }
    if !query
        .filter
        .iter()
        .any(|p| matches!(p, Predicate::All(_) | Predicate::Any(_) | Predicate::Not(_)))
    {
        return Ok(Cow::Borrowed(query));
    }
    let mut prepared = query.clone();
    prepared.filter.clear();
    for predicate in &query.filter {
        append(predicate, &mut prepared.filter)?;
    }
    Ok(Cow::Owned(prepared))
}
