use std::collections::{HashMap, HashSet};

use nucleus::vocabulary::Term;
use serde_json::{Value, json};
use store::{Store, concepts::list_all, linguas::linguas_for_concepts, vocabulary};

use super::{row_limit, selected_family};
use crate::{Predicate, Protein, ProteinError};

#[derive(Default)]
struct ConceptTerms<'a> {
    rows: Vec<&'a Term>,
    folded: Vec<String>,
}

pub(crate) async fn execute_concepts(
    store: &Store,
    protein: &Protein,
    subject: Option<&str>,
) -> Result<Vec<Value>, ProteinError> {
    let pool = &store.pool;
    let names = vocabulary::display_names(pool, subject).await?;
    let terms = vocabulary::terms(pool, subject).await?;
    let sources = vocabulary::sources(pool).await?;
    let definitions = vocabulary::definition_versions(pool).await?;
    let terminology = vocabulary::terminology_state(pool, subject).await?;
    let features = vocabulary::feature_bindings(pool).await?;
    let (write_location, binding_revision, location_concepts) =
        vocabulary::location_bindings(pool).await?;

    let mut text_queries = HashMap::new();
    for predicate in &protein.filter {
        if let Predicate::TextContains(text) = predicate {
            text_queries
                .entry(text.as_str())
                .or_insert_with(|| text.to_lowercase());
        }
    }
    let mut terms_by_concept = HashMap::<_, ConceptTerms<'_>>::new();
    for term in &terms {
        let group = terms_by_concept
            .entry(term.concept_uid.as_str())
            .or_default();
        group.rows.push(term);
        if !text_queries.is_empty() {
            group.folded.push(term.name.to_lowercase());
        }
    }
    let empty = ConceptTerms::default();
    let terms_for = |uid: &str| terms_by_concept.get(uid).unwrap_or(&empty);
    let hidden_terms: HashSet<_> = terminology["hidden_terms"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let mut choices = HashMap::new();
    for choice in terminology["choices"].as_array().into_iter().flatten() {
        if let Some(uid) = choice["concept_uid"].as_str() {
            choices.entry(uid).or_insert(choice);
        }
    }
    let family = selected_family(pool, &protein.filter, subject).await?;
    let selected: Vec<_> = list_all(pool)
        .await?
        .into_iter()
        .filter(|concept| {
            if family
                .as_ref()
                .is_some_and(|family| !family.contains(&concept.uid))
            {
                return false;
            }
            let terms = terms_for(&concept.uid);
            protein.filter.iter().all(|predicate| match predicate {
                Predicate::UidEq(uid) => concept.uid == *uid,
                Predicate::SlugEq(name) => terms.rows.iter().any(|term| term.name == *name),
                Predicate::TextContains(text) => text_queries
                    .get(text.as_str())
                    .is_some_and(|needle| terms.folded.iter().any(|name| name.contains(needle))),
                _ => true,
            })
        })
        .take(row_limit(protein.limit))
        .collect();
    let ids: Vec<_> = selected
        .iter()
        .map(|concept| concept.uid.as_str())
        .collect();
    let mut memberships = linguas_for_concepts(pool, &ids).await?;
    let context = &terminology["context"];
    Ok(selected
        .into_iter()
        .map(|concept| {
            let uid = &concept.uid;
            let terms = &terms_for(uid).rows;
            let hidden: Vec<_> = terms
                .iter()
                .filter(|term| hidden_terms.contains(term.uid.as_str()))
                .map(|term| &term.uid)
                .collect();
            json!({
                "uid": uid,
                "name": names.get(uid).unwrap_or(&concept.canonical_name),
                "source": sources.get(uid),
                "definitions": definitions.get(uid),
                "terminology_context": context,
                "feature_bindings": features,
                "terminology_choice": choices.get(uid.as_str()),
                "hidden_terms": hidden,
                "terms": terms,
                "saved_location": location_concepts.contains(uid),
                "location_write": write_location == *uid,
                "binding_revision": binding_revision,
                "instinct": concept.instinct,
                "parents": concept.parents,
                "linguas": memberships.remove(uid).unwrap_or_default(),
            })
        })
        .collect())
}
