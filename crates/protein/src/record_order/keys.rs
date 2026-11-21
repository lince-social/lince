use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
};

use nucleus::DecimalValue;
use serde_json::Value;
use store::{
    Store,
    assertions::binary_for_subjects,
    concepts::canonical_names,
    records::{RecordBody, RecordRow, extensions_for_records, live_heads_for},
    vocabulary::feature_binding,
};

use super::{apply, fields, prepare};
use crate::{Order, ProteinError};

pub(super) struct Keys {
    concepts: HashMap<String, String>,
    assignees: HashMap<String, String>,
    work: HashMap<String, Value>,
}

impl Keys {
    pub(super) async fn load<Body: RecordBody>(
        store: &Store,
        rows: &[RecordRow<Body>],
        order: &[Order],
    ) -> Result<Self, ProteinError> {
        let pool = &store.pool;
        let work = if fields(order).any(|field| matches!(field, "start_date" | "due_date")) {
            let selected: Vec<_> = rows.iter().map(|row| row.uid.clone()).collect();
            extensions_for_records(pool, "work", &selected).await?
        } else {
            HashMap::new()
        };
        let concepts = if fields(order).any(|field| matches!(field, "concept_name" | "concept")) {
            canonical_names(pool).await?.into_iter().collect()
        } else {
            HashMap::new()
        };
        let mut assignees = HashMap::new();
        if fields(order).any(|field| field == "assignee_name") {
            let binding = feature_binding(pool, "assignment").await?;
            let subjects: Vec<_> = rows.iter().map(|row| row.uid.as_str()).collect();
            let links = binary_for_subjects(pool, &binding.concept_uids, &subjects).await?;
            let people: HashSet<_> = links.iter().map(|link| link.object_uid.as_str()).collect();
            let names = live_heads_for(pool, &people.into_iter().collect::<Vec<_>>()).await?;
            for link in links {
                if let Some(name) = names.get(&link.object_uid) {
                    assignees
                        .entry(link.subject_uid)
                        .or_insert_with(|| name.clone());
                }
            }
        }
        Ok(Self {
            concepts,
            assignees,
            work,
        })
    }

    pub(super) fn sort<Body: RecordBody>(
        &self,
        rows: &mut [RecordRow<Body>],
        field: &str,
        descending: bool,
        priorities: Option<&HashMap<String, DecimalValue>>,
    ) {
        if rows.len() < 2 {
            return;
        }
        let indices = match field {
            "quantity" => prepare(rows.iter().map(|row| row.quantity), |left, right| {
                direction(left.exact_numeric_cmp(*right), descending)
            }),
            "created_hlc" => prepare(
                rows.iter().map(|row| {
                    row.created_hlc
                        .map(|value| (value >= 0, value.unsigned_abs()))
                }),
                |left, right| direction(missing_last(left, right, false), descending),
            ),
            field if field.starts_with('#') => prepare(
                rows.iter()
                    .map(|row| priorities.and_then(|values| values.get(&row.uid))),
                |left, right| missing_last(left, right, descending),
            ),
            field => {
                let optional = match field {
                    "slug" | "concept_name" | "concept" | "assignee_name" | "assignee"
                    | "start_date" | "due_date" => true,
                    "title" | "head" | "description" | "body" | "uid" | "kind" | "created_at"
                    | "updated_at" => false,
                    _ => return,
                };
                prepare(
                    rows.iter().map(|row| self.text_key(row, field)),
                    |left, right| {
                        if optional {
                            missing_last(left, right, descending)
                        } else {
                            direction(left.cmp(right), descending)
                        }
                    },
                )
            }
        };
        apply(rows, indices);
    }

    fn text_key<'a, Body: RecordBody>(
        &'a self,
        row: &'a RecordRow<Body>,
        field: &str,
    ) -> Option<&'a str> {
        match field {
            "title" | "head" => Some(&row.head),
            "description" | "body" => row.body.loaded(),
            "uid" => Some(&row.uid),
            "slug" => row.slug.as_deref(),
            "kind" => Some(&row.kind),
            "concept_name" | "concept" => self
                .concepts
                .get(row.identity_predicate_uid.as_ref()?)
                .map(String::as_str),
            "assignee_name" | "assignee" => self.assignees.get(&row.uid).map(String::as_str),
            "start_date" | "due_date" => self.work.get(&row.uid)?[if field == "start_date" {
                "start"
            } else {
                "due"
            }]
            .as_str(),
            "created_at" => Some(&row.created_at),
            "updated_at" => Some(&row.updated_at),
            _ => None,
        }
    }
}

fn direction(order: Ordering, descending: bool) -> Ordering {
    if descending { order.reverse() } else { order }
}

fn missing_last<T: Ord>(left: &Option<T>, right: &Option<T>, descending: bool) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => direction(left.cmp(right), descending),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}
