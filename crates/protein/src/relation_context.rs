//! Adds visible incident assertions and family names to Record results.

use crate::ProteinError;
use nucleus::performance::{Span, add};
pub use protein_authority::query::relation_context::Include;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use store::{
    Store, assertions::incident_to, concepts::descendants_including, records::RecordRow,
    vocabulary::resolve,
};

pub(crate) async fn attach<Body>(
    store: &Store,
    records: &[RecordRow<Body>],
    visible: Option<&HashSet<String>>,
    include: &Include,
    names: &HashMap<String, String>,
    subject: Option<&str>,
) -> Result<HashMap<String, Value>, ProteinError> {
    let _context = Span::new("protein-relation-context-await");
    let ids: Vec<_> = records.iter().map(|record| record.uid.as_str()).collect();
    let positions: HashMap<_, _> = ids
        .iter()
        .enumerate()
        .map(|(index, uid)| (*uid, index))
        .collect();
    let mut context = vec![Vec::new(); ids.len()];
    let mut families = BTreeMap::new();
    for family in &include.families {
        let family = family.trim();
        if families.contains_key(family) {
            continue;
        }
        let members: HashSet<_> = if let Some(uid) = resolve(&store.pool, family, subject).await? {
            descendants_including(&store.pool, &uid)
                .await?
                .into_iter()
                .collect()
        } else {
            HashSet::new()
        };
        families.insert(family, members);
    }
    let mut family_index: HashMap<String, Vec<&str>> = HashMap::new();
    let mut published = 0;
    let mut fanouts = 0;
    for assertion in incident_to(&store.pool, &ids).await? {
        if visible.is_some_and(|allowed| {
            !allowed.contains(&assertion.subject_uid)
                || assertion
                    .object_uid
                    .as_ref()
                    .is_some_and(|uid| !allowed.contains(uid))
        }) {
            continue;
        }
        let from = positions.get(assertion.subject_uid.as_str()).copied();
        let to = assertion.object_uid.or(assertion.resolved_object_uid);
        let target = to
            .as_deref()
            .filter(|to| *to != assertion.subject_uid.as_str())
            .and_then(|to| positions.get(to))
            .copied();
        let Some(primary) = from.or(target) else {
            continue;
        };
        let _publish = Span::new("protein-relation-context-publish");
        if !families.is_empty() && !family_index.contains_key(&assertion.predicate_uid) {
            let _index = Span::new("protein-relation-context-index");
            let matched = families
                .iter()
                .filter(|(_, members)| members.contains(&assertion.predicate_uid))
                .map(|(family, _)| *family)
                .collect();
            family_index.insert(assertion.predicate_uid.clone(), matched);
            add("protein-relation-context-family-lookups", 1);
        }
        let matched = family_index
            .get(&assertion.predicate_uid)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let predicate = names.get(&assertion.predicate_uid).map(String::as_str);
        let unit_name = assertion
            .unit_uid
            .as_ref()
            .and_then(|uid| names.get(uid))
            .map(String::as_str);
        let value = Value::from_iter([
            ("uid", assertion.uid.into()),
            ("from", assertion.subject_uid.into()),
            ("to", to.into()),
            ("uri", assertion.object_uri.into()),
            ("predicate_uid", assertion.predicate_uid.into()),
            ("predicate", predicate.into()),
            ("role", assertion.role.into()),
            (
                "quantity",
                assertion
                    .quantity
                    .map(|quantity| quantity.to_string())
                    .into(),
            ),
            ("unit", assertion.unit_uid.into()),
            ("families", matched.iter().copied().collect::<Value>()),
            ("unit_name", unit_name.into()),
        ]);
        // Positions come from the same IDs that sized context. Self-links have no target.
        if let Some(target) = target.filter(|_| from.is_some()) {
            context[primary].push(value.clone());
            context[target].push(value);
            fanouts += 1;
        } else {
            context[primary].push(value);
        }
        published += 1;
    }
    add("protein-relation-context-assertions", published);
    add("protein-relation-context-fanouts", fanouts);
    Ok(records
        .iter()
        .zip(context)
        .map(|(record, assertions)| {
            let unit_name = record
                .unit_uid
                .as_ref()
                .and_then(|uid| names.get(uid))
                .map(String::as_str);
            (
                record.uid.clone(),
                Value::from_iter([
                    ("assertions", Value::Array(assertions)),
                    ("unit_name", unit_name.into()),
                ]),
            )
        })
        .collect())
}
