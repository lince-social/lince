use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use nucleus::DecimalValue;
use serde_json::Value;
use store::Store;
use store::records::{RecordBody, RecordRow};

use crate::{LinkEndpoint, Order, ProteinError, assertion_order};

pub(super) async fn records<Body: RecordBody>(
    store: &Store,
    mut rows: Vec<store::records::RecordRow<Body>>,
    order: &[Order],
) -> Result<Vec<store::records::RecordRow<Body>>, ProteinError> {
    let _ordering = nucleus::performance::Span::new("protein-record-order-await");
    if order.iter().any(|key| {
        matches!(key, Order::Asc(field) | Order::Desc(field) if field == "body" || field == "description")
    }) && rows.iter().any(|row| row.body.loaded().is_none())
    {
        return Err(store::StoreError::Protocol(
            "Record body was not loaded for this sort".into(),
        ));
    }
    if rows.len() > 1 {
        let indices = prepare(
            rows.iter()
                .map(|row| (row.head.to_lowercase(), row.uid.as_str())),
            Ord::cmp,
        );
        apply(&mut rows, indices);
    }
    let need_work = order.iter().any(|key| matches!(key, Order::Asc(field) | Order::Desc(field) if field == "start_date" || field == "due_date"));
    let work = if need_work {
        let selected: Vec<_> = rows.iter().map(|row| row.uid.clone()).collect();
        store::records::extensions_for_records(&store.pool, "work", &selected).await?
    } else {
        HashMap::new()
    };
    let concept_names: HashMap<String, String> = if order.iter().any(|key| {
        matches!(key, Order::Asc(field) | Order::Desc(field)
            if field == "concept_name" || field == "concept")
    }) {
        store::concepts::canonical_names(&store.pool)
            .await?
            .into_iter()
            .collect()
    } else {
        HashMap::new()
    };
    let mut assignees = HashMap::new();
    if order.iter().any(
        |key| matches!(key, Order::Asc(field) | Order::Desc(field) if field == "assignee_name"),
    ) {
        let binding = store::vocabulary::feature_binding(&store.pool, "assignment").await?;
        let subjects: Vec<_> = rows.iter().map(|row| row.uid.as_str()).collect();
        let links =
            store::assertions::binary_for_subjects(&store.pool, &binding.concept_uids, &subjects)
                .await?;
        let people: HashSet<_> = links.iter().map(|link| link.object_uid.as_str()).collect();
        let people: Vec<_> = people.into_iter().collect();
        let names = store::records::live_heads_for(&store.pool, &people).await?;
        for link in links {
            if let Some(name) = names.get(&link.object_uid) {
                assignees
                    .entry(link.subject_uid)
                    .or_insert_with(|| name.clone());
            }
        }
    }
    let assertion_fields: Vec<_> = order
        .iter()
        .filter_map(|key| match key {
            Order::Asc(field) | Order::Desc(field) if field.starts_with('#') => {
                Some(field.as_str())
            }
            _ => None,
        })
        .collect();
    let assertion_values = if assertion_fields.is_empty() {
        assertion_order::Values::default()
    } else {
        let uids: Vec<_> = rows.iter().map(|row| row.uid.as_str()).collect();
        assertion_order::values(store, &uids, &assertion_fields).await?
    };
    for key in order.iter().rev() {
        match key {
            Order::Asc(f) | Order::Desc(f) => {
                let desc = matches!(key, Order::Desc(_));
                let priorities = assertion_values.get(f);
                sort_field(
                    &mut rows,
                    f,
                    desc,
                    priorities,
                    &concept_names,
                    &assignees,
                    &work,
                );
            }
            Order::Link(rule) => {
                let family =
                    store::vocabulary::meaning_family(&store.pool, &rule.kind, None).await?;
                let edges = store::assertions::edges_of_predicates(&store.pool, &family).await?;
                let edges = if rule.higher == LinkEndpoint::From {
                    edges
                } else {
                    edges
                        .into_iter()
                        .map(|edge| nucleus::graph::Edge {
                            from: edge.to,
                            to: edge.from,
                            quantity: edge.quantity,
                        })
                        .collect()
                };
                sort_links(&mut rows, &edges);
            }
        }
    }
    Ok(rows)
}

fn sort_links<Body: RecordBody>(rows: &mut Vec<RecordRow<Body>>, edges: &[nucleus::graph::Edge]) {
    let _ordering = nucleus::performance::Span::new("protein-record-link-order");
    nucleus::performance::add("protein-record-link-candidates", rows.len() as u64);
    let (mut indices, kept) = {
        let uids: Vec<_> = rows.iter().map(|row| row.uid.as_str()).collect();
        let mut indices = nucleus::graph::topo_order_indices(&uids, edges);
        let mut latest: HashMap<_, _> = uids
            .iter()
            .enumerate()
            .map(|(index, &uid)| (uid, index))
            .collect();
        indices.retain_mut(|index| {
            if let Some(source) = latest.remove(uids[*index]) {
                *index = source;
                true
            } else {
                false
            }
        });
        let kept = indices.len();
        (indices, kept)
    };
    if kept < rows.len() {
        let mut selected = vec![false; rows.len()];
        for &index in &indices {
            selected[index] = true;
        }
        indices.extend((0..rows.len()).filter(|&index| !selected[index]));
        nucleus::performance::add("protein-record-link-duplicates", (rows.len() - kept) as u64);
    }
    apply(rows, indices);
    rows.truncate(kept);
}

fn prepare<K>(keys: impl Iterator<Item = K>, compare: impl Fn(&K, &K) -> Ordering) -> Vec<usize> {
    let preparation = nucleus::performance::Span::new("protein-record-order-keys");
    let keys: Vec<_> = keys.collect();
    nucleus::performance::add("protein-record-order-keys", keys.len() as u64);
    drop(preparation);
    let _sorting = nucleus::performance::Span::new("protein-record-order-indices");
    if keys
        .windows(2)
        .all(|pair| compare(&pair[0], &pair[1]) != Ordering::Greater)
    {
        return Vec::new();
    }
    let mut indices: Vec<_> = (0..keys.len()).collect();
    indices.sort_unstable_by(|&a, &b| compare(&keys[a], &keys[b]).then_with(|| a.cmp(&b)));
    indices
}

fn apply<T>(rows: &mut [T], mut indices: Vec<usize>) {
    let _application = nucleus::performance::Span::new("protein-record-order-apply");
    let mut swaps = 0;
    for start in 0..indices.len() {
        let mut current = start;
        let mut next = indices[current];
        while next != start {
            rows.swap(current, next);
            swaps += 1;
            indices[current] = current;
            current = next;
            next = indices[current];
        }
        indices[current] = current;
    }
    nucleus::performance::add("protein-record-order-payload-swaps", swaps);
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

fn sort_field<Body: RecordBody>(
    rows: &mut [RecordRow<Body>],
    field: &str,
    descending: bool,
    priorities: Option<&HashMap<String, DecimalValue>>,
    concepts: &HashMap<String, String>,
    assignees: &HashMap<String, String>,
    work: &HashMap<String, Value>,
) {
    if rows.len() < 2 {
        return;
    }
    let indices = if field.starts_with('#') {
        prepare(
            rows.iter()
                .map(|row| priorities.and_then(|values| values.get(&row.uid))),
            |left, right| missing_last(left, right, descending),
        )
    } else {
        match field {
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
            "title" | "head" | "description" | "body" | "uid" | "slug" | "kind"
            | "concept_name" | "concept" | "assignee_name" | "assignee" | "start_date"
            | "due_date" | "created_at" | "updated_at" => {
                let optional = matches!(
                    field,
                    "slug"
                        | "concept_name"
                        | "concept"
                        | "assignee_name"
                        | "assignee"
                        | "start_date"
                        | "due_date"
                );
                prepare(
                    rows.iter()
                        .map(|row| text_key(row, field, concepts, assignees, work)),
                    |left, right| {
                        if optional {
                            missing_last(left, right, descending)
                        } else {
                            direction(left.cmp(right), descending)
                        }
                    },
                )
            }
            _ => return,
        }
    };
    apply(rows, indices);
}

fn text_key<'a, Body: RecordBody>(
    row: &'a RecordRow<Body>,
    field: &str,
    concepts: &'a HashMap<String, String>,
    assignees: &'a HashMap<String, String>,
    work: &'a HashMap<String, Value>,
) -> Option<&'a str> {
    match field {
        "title" | "head" => Some(&row.head),
        "description" | "body" => row.body.loaded(),
        "uid" => Some(&row.uid),
        "slug" => row.slug.as_deref(),
        "kind" => Some(&row.kind),
        "concept_name" | "concept" => row
            .identity_predicate_uid
            .as_ref()
            .and_then(|uid| concepts.get(uid))
            .map(String::as_str),
        "assignee_name" | "assignee" => assignees.get(&row.uid).map(String::as_str),
        "start_date" | "due_date" => work
            .get(&row.uid)
            .and_then(|value| {
                value.get(if field == "start_date" {
                    "start"
                } else {
                    "due"
                })
            })
            .and_then(Value::as_str),
        "created_at" => Some(&row.created_at),
        "updated_at" => Some(&row.updated_at),
        _ => None,
    }
}
