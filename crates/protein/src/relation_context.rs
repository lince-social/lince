use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use store::{Store, records::RecordRow};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Include {
    #[serde(default)]
    pub families: Vec<String>,
}

impl Include {
    pub fn valid(&self) -> bool {
        self.families.len() <= 128
            && self.families.iter().all(|family| {
                !family.trim().is_empty() && family.len() <= 128 && !family.contains('\0')
            })
    }
}

pub(crate) async fn attach<Body>(
    store: &Store,
    records: &[RecordRow<Body>],
    visible: Option<&HashSet<String>>,
    include: &Include,
    names: &HashMap<String, String>,
    subject: Option<&str>,
) -> Result<HashMap<String, Value>, crate::ProteinError> {
    let _context = nucleus::performance::Span::new("protein-relation-context-await");
    let ids: Vec<_> = records.iter().map(|record| record.uid.as_str()).collect();
    let positions: HashMap<_, _> = ids
        .iter()
        .enumerate()
        .map(|(index, uid)| (*uid, index))
        .collect();
    let mut context: Vec<Vec<Value>> = records.iter().map(|_| Vec::new()).collect();
    let mut families = BTreeMap::new();
    for family in &include.families {
        let family = family.trim();
        if families.contains_key(family) {
            continue;
        }
        let members: HashSet<_> =
            if let Some(uid) = store::vocabulary::resolve(&store.pool, family, subject).await? {
                store::concepts::descendants_including(&store.pool, &uid)
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
    for assertion in store::assertions::incident_to(&store.pool, &ids).await? {
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
        if from.is_none() && target.is_none() {
            continue;
        }
        let _publish = nucleus::performance::Span::new("protein-relation-context-publish");
        if !families.is_empty() && !family_index.contains_key(&assertion.predicate_uid) {
            let _index = nucleus::performance::Span::new("protein-relation-context-index");
            let matched = families
                .iter()
                .filter(|(_, members)| members.contains(&assertion.predicate_uid))
                .map(|(family, _)| *family)
                .collect();
            family_index.insert(assertion.predicate_uid.clone(), matched);
            nucleus::performance::add("protein-relation-context-family-lookups", 1);
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
        match (from, target) {
            (Some(from), Some(target)) => {
                context[from].push(value.clone());
                context[target].push(value);
                fanouts += 1;
            }
            (Some(index), None) | (None, Some(index)) => context[index].push(value),
            (None, None) => unreachable!(),
        }
        published += 1;
    }
    nucleus::performance::add("protein-relation-context-assertions", published);
    nucleus::performance::add("protein-relation-context-fanouts", fanouts);
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
