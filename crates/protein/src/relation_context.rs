use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use store::{Store, records::RecordRow};

#[derive(Clone, Debug, Default, Serialize, Deserialize, schemars::JsonSchema)]
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

pub(crate) async fn attach(
    store: &Store,
    records: &[RecordRow],
    visible: Option<&HashSet<String>>,
    include: &Include,
    names: &HashMap<String, String>,
) -> Result<HashMap<String, Value>, crate::ProteinError> {
    let ids: Vec<_> = records.iter().map(|record| record.uid.clone()).collect();
    let mut context: HashMap<_, Vec<Value>> =
        ids.iter().map(|uid| (uid.clone(), Vec::new())).collect();
    let mut families = BTreeMap::new();
    for family in &include.families {
        let family = family.trim();
        if families.contains_key(family) {
            continue;
        }
        let members: HashSet<_> =
            if let Some(uid) = store::concepts::resolve(&store.pool, family).await? {
                store::concepts::descendants_including(&store.pool, &uid)
                    .await?
                    .into_iter()
                    .collect()
            } else {
                HashSet::new()
            };
        families.insert(family.to_owned(), members);
    }
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
        let matched: Vec<_> = families
            .iter()
            .filter(|(_, members)| members.contains(&assertion.predicate_uid))
            .map(|(family, _)| family.clone())
            .collect();
        let value = json!({
            "uid": assertion.uid, "from": assertion.subject_uid, "to": assertion.object_uid,
            "predicate_uid": assertion.predicate_uid, "predicate": names.get(&assertion.predicate_uid),
            "role": assertion.role, "quantity": assertion.quantity.map(|quantity| quantity.to_string()),
            "unit": assertion.unit_uid, "families": matched,
            "unit_name": assertion.unit_uid.as_ref().and_then(|uid| names.get(uid)),
        });
        if let Some(rows) = context.get_mut(&assertion.subject_uid) {
            rows.push(value.clone());
        }
        if let Some(to) = &assertion.object_uid {
            if to != &assertion.subject_uid {
                if let Some(rows) = context.get_mut(to) {
                    rows.push(value);
                }
            }
        }
    }
    let units: HashMap<_, _> = records
        .iter()
        .map(|record| {
            (
                record.uid.as_str(),
                record.unit_uid.as_ref().and_then(|uid| names.get(uid)),
            )
        })
        .collect();
    Ok(context
        .into_iter()
        .map(|(uid, assertions)| {
            let unit_name = units.get(uid.as_str()).copied().flatten();
            (
                uid,
                json!({"assertions": assertions, "unit_name": unit_name}),
            )
        })
        .collect())
}
