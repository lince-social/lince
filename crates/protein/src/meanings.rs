use crate::{Order, Predicate, Protein, ProteinError};
use std::{future::Future, pin::Pin};
use store::Store;

async fn concept(store: &Store, token: &str, person: Option<&str>) -> Result<String, ProteinError> {
    store::vocabulary::resolve(&store.pool, token, person)
        .await?
        .ok_or_else(|| {
            store::StoreError::Protocol(format!("Choose a saved Concept identity for `{token}`"))
        })
}

async fn relation_meaning(
    store: &Store,
    token: &str,
    person: Option<&str>,
) -> Result<String, ProteinError> {
    if token.starts_with("feature:") || token.starts_with("concepts:") {
        let meanings = store::vocabulary::meaning_family(&store.pool, token, person).await?;
        Ok(nucleus::vocabulary::meaning_set(&meanings))
    } else {
        concept(store, token, person).await
    }
}

fn predicate<'a>(
    store: &'a Store,
    value: &'a mut Predicate,
    person: Option<&'a str>,
) -> Pin<Box<dyn Future<Output = Result<(), ProteinError>> + Send + 'a>> {
    Box::pin(async move {
        match value {
            Predicate::All(children) | Predicate::Any(children) => {
                for child in children {
                    predicate(store, child, person).await?;
                }
            }
            Predicate::Not(child) => predicate(store, child, person).await?,
            Predicate::ConceptIn(token) | Predicate::UnitEq(token) => {
                *token = concept(store, token, person).await?
            }
            Predicate::Relation { kind, .. } | Predicate::Under { kind, .. } => {
                *kind = relation_meaning(store, kind, person).await?;
            }
            _ => {}
        }
        Ok(())
    })
}

pub async fn bind_meanings(
    store: &Store,
    query: &Protein,
    person: Option<&str>,
) -> Result<Protein, ProteinError> {
    crate::validate(query)?;
    let mut query = query.clone();
    for value in &mut query.filter {
        predicate(store, value, person).await?;
    }
    for order in &mut query.order {
        match order {
            Order::Link(link) => link.kind = relation_meaning(store, &link.kind, person).await?,
            Order::Asc(field) | Order::Desc(field) if field.starts_with('#') => {
                *field = format!("#{}", concept(store, &field[1..], person).await?)
            }
            _ => {}
        }
    }
    if let Some(links) = &mut query.include.links {
        let mut bound = Vec::new();
        for token in &links.kinds {
            if token.starts_with("feature:") || token.starts_with("concepts:") {
                bound.extend(store::vocabulary::meaning_family(&store.pool, token, person).await?);
            } else {
                bound.push(concept(store, token, person).await?);
            }
        }
        bound.sort();
        bound.dedup();
        links.kinds = bound;
    }
    if let Some(context) = &mut query.include.relation_context {
        let mut bound = Vec::new();
        for token in &context.families {
            if token.starts_with("feature:") || token.starts_with("concepts:") {
                bound.extend(store::vocabulary::meaning_family(&store.pool, token, person).await?);
            } else {
                bound.push(concept(store, token, person).await?);
            }
        }
        bound.sort();
        bound.dedup();
        context.families = bound;
    }
    crate::validate(&query)?;
    Ok(query)
}
