//! Concept, Lingua, and Assertion catalogues.

use std::collections::HashSet;

use store::{concepts::descendants_including, sqlx::SqlitePool, vocabulary::resolve};

use crate::{Predicate, ProteinError};

mod assertions;
mod concepts;
mod linguas;

pub(crate) use assertions::execute_assertions;
pub(crate) use concepts::execute_concepts;
pub(crate) use linguas::execute_linguas;

async fn selected_family(
    pool: &SqlitePool,
    predicates: &[Predicate],
    subject: Option<&str>,
) -> Result<Option<HashSet<String>>, ProteinError> {
    let mut family = None;
    for predicate in predicates {
        if let Predicate::ConceptIn(name) = predicate {
            family = Some(match resolve(pool, name, subject).await? {
                Some(uid) => descendants_including(pool, &uid)
                    .await?
                    .into_iter()
                    .collect(),
                None => HashSet::new(),
            });
        }
    }
    Ok(family)
}

fn row_limit(limit: Option<usize>) -> usize {
    // Catalogue limits are applied after admitting the first matching row.
    limit.unwrap_or(usize::MAX).max(1)
}
