//! Preparation and matching for live Record predicates.

use std::collections::{HashMap, HashSet};

use nucleus::{place::Place, vocabulary::SavedLocation};
use serde_json::Value;
use store::sqlx::Error::Protocol;

use crate::{LinkDirection, Predicate, ProteinError};

mod matching;
mod prepare;

type RelationKey<'a> = (&'a str, LinkDirection, Option<&'a str>);
type BranchKey<'a> = (&'a str, &'a str, bool);

#[derive(Default)]
pub(super) struct Context<'a> {
    predicates: &'a [Predicate],
    concept_families: HashMap<&'a str, HashSet<String>>,
    record_concepts: Option<HashMap<String, Vec<String>>>,
    anchors: HashMap<&'a str, Option<Place>>,
    locations: HashMap<String, SavedLocation>,
    relations: HashMap<RelationKey<'a>, HashSet<String>>,
    branches: HashMap<BranchKey<'a>, HashSet<String>>,
    work: Option<HashMap<String, Value>>,
    organs: HashMap<&'a str, Option<String>>,
    text_queries: HashMap<&'a str, String>,
}

impl Context<'_> {
    fn location(&self, uid: &str) -> Option<Place> {
        let selected = self.locations.get(uid)?.selected.as_ref()?;
        Some(Place {
            lat: selected.latitude,
            lon: selected.longitude,
        })
    }
}

pub(super) fn validate(predicates: &[Predicate]) -> Result<(), ProteinError> {
    if predicates.iter().all(supported) {
        Ok(())
    } else {
        Err(unsupported())
    }
}

fn unsupported() -> ProteinError {
    Protocol("protein_record_unsupported_predicate:predicate is not defined for Records".into())
}

fn supported(predicate: &Predicate) -> bool {
    match predicate {
        Predicate::All(children) | Predicate::Any(children) => children.iter().all(supported),
        Predicate::Not(child) => supported(child),
        Predicate::QuantityLt(_)
        | Predicate::QuantityLte(_)
        | Predicate::QuantityGt(_)
        | Predicate::QuantityGte(_)
        | Predicate::QuantityEq(_)
        | Predicate::UidEq(_)
        | Predicate::KindEq(_)
        | Predicate::SlugEq(_)
        | Predicate::ConceptIn(_)
        | Predicate::Relation { .. }
        | Predicate::Under { .. }
        | Predicate::TextContains(_)
        | Predicate::WorkDate { .. }
        | Predicate::Near { .. }
        | Predicate::OrganEq(_)
        | Predicate::FileOriginEq(_)
        | Predicate::OrganIn(_) => true,
        _ => false,
    }
}
