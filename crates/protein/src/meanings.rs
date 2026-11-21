//! Resolves saved query meanings to stable Concept identities.

use nucleus::vocabulary::meaning_set;
use store::{
    Store,
    sqlx::{Error::Protocol, SqlitePool},
    vocabulary::{meaning_family, resolve},
};

use crate::{Order, Predicate, Protein, ProteinError, validate};

async fn concept(
    pool: &SqlitePool,
    token: &str,
    person: Option<&str>,
) -> Result<String, ProteinError> {
    resolve(pool, token, person)
        .await?
        .ok_or_else(|| Protocol(format!("Choose a saved Concept identity for `{token}`")))
}

fn is_family(token: &str) -> bool {
    token.starts_with("feature:") || token.starts_with("concepts:")
}

async fn relation_meaning(
    pool: &SqlitePool,
    token: &str,
    person: Option<&str>,
) -> Result<String, ProteinError> {
    if is_family(token) {
        Ok(meaning_set(&meaning_family(pool, token, person).await?))
    } else {
        concept(pool, token, person).await
    }
}

async fn bind_families(
    pool: &SqlitePool,
    tokens: &[String],
    person: Option<&str>,
) -> Result<Vec<String>, ProteinError> {
    let mut bound = Vec::new();
    for token in tokens {
        if is_family(token) {
            bound.extend(meaning_family(pool, token, person).await?);
        } else {
            bound.push(concept(pool, token, person).await?);
        }
    }
    bound.sort_unstable();
    bound.dedup();
    Ok(bound)
}

/// Binds Concept names and feature families in a query for the given reader.
///
/// # Errors
/// Returns an error if the query is invalid or a meaning cannot be resolved.
pub async fn bind_meanings(
    store: &Store,
    query: &Protein,
    person: Option<&str>,
) -> Result<Protein, ProteinError> {
    validate(query)?;
    let pool = &store.pool;
    let mut query = query.clone();
    let mut pending: Vec<_> = query.filter.iter_mut().rev().collect();
    while let Some(predicate) = pending.pop() {
        match predicate {
            Predicate::All(children) | Predicate::Any(children) => {
                pending.extend(children.iter_mut().rev())
            }
            Predicate::Not(child) => pending.push(child),
            Predicate::ConceptIn(token) | Predicate::UnitEq(token) => {
                *token = concept(pool, token, person).await?
            }
            Predicate::Relation { kind, .. } | Predicate::Under { kind, .. } => {
                *kind = relation_meaning(pool, kind, person).await?
            }
            _ => {}
        }
    }
    for order in &mut query.order {
        match order {
            Order::Link(link) => link.kind = relation_meaning(pool, &link.kind, person).await?,
            Order::Asc(field) | Order::Desc(field) => {
                if let Some(token) = field.strip_prefix('#') {
                    *field = format!("#{}", concept(pool, token, person).await?);
                }
            }
        }
    }
    if let Some(links) = &mut query.include.links {
        links.kinds = bind_families(pool, &links.kinds, person).await?;
    }
    if let Some(context) = &mut query.include.relation_context {
        context.families = bind_families(pool, &context.families, person).await?;
    }
    validate(&query)?;
    Ok(query)
}
