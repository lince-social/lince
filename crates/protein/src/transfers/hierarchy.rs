//! Transfer hierarchy, progress, and dependency summaries.

use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub(super) struct TransferOutput {
    pub(super) uid: String,
    pub(super) slug: Option<String>,
    pub(super) head: String,
    pub(super) revision: u64,
    pub(super) status: String,
    pub(super) value: Value,
}

fn descendants<'a>(uid: &'a str, children: &HashMap<&'a str, Vec<&'a str>>) -> Vec<&'a str> {
    let mut stack = vec![uid];
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    while let Some(node) = stack.pop() {
        if seen.insert(node) {
            out.push(node);
            if let Some(children) = children.get(node) {
                stack.extend(children.iter().rev().copied());
            }
        }
    }
    out
}

fn hierarchy_path<'a>(
    uid: &'a str,
    parents: &HashMap<&'a str, Option<&'a str>>,
    visible: &HashSet<&str>,
) -> (&'a str, Vec<&'a str>, bool) {
    let mut path = Vec::new();
    let mut current = Some(uid);
    let mut seen = HashSet::new();
    while let Some(node) = current {
        if !seen.insert(node) {
            break;
        }
        path.push(node);
        current = parents
            .get(node)
            .copied()
            .flatten()
            .filter(|parent| visible.contains(parent));
    }
    let root = path.last().copied().unwrap_or(uid);
    path.reverse();
    (root, path, current.is_some())
}

#[derive(Default)]
struct ResourceRollup<'a> {
    concept_name: Option<&'a str>,
    unit_name: Option<&'a str>,
    canonical: f64,
    settled: f64,
    remaining: f64,
}

fn subtree_rollup<'a>(
    uid: &'a str,
    values: &HashMap<&'a str, &Value>,
    children: &HashMap<&'a str, Vec<&'a str>>,
) -> (Value, bool, Vec<&'a str>) {
    let descendants = descendants(uid, children);
    let mut status_counts = BTreeMap::<_, usize>::new();
    let mut resource_rollup = BTreeMap::<_, ResourceRollup>::new();
    let mut ready = true;
    let mut blocked_transfers = Vec::new();
    for &descendant in &descendants {
        let Some(value) = values.get(descendant) else {
            continue;
        };
        if let Some(status) = value.get("status").and_then(Value::as_str) {
            *status_counts.entry(status).or_default() += 1;
        }
        let terminal = value
            .get("status")
            .and_then(Value::as_str)
            .is_some_and(|status| matches!(status, "settled" | "satiated" | "withdrawn"));
        let transfer_ready = terminal
            || value
                .pointer("/readiness/ready")
                .and_then(Value::as_bool)
                .unwrap_or(false);
        if !transfer_ready {
            ready = false;
            blocked_transfers.push(descendant);
        }
        for (resource_index, resource) in value
            .pointer("/settlement_progress/by_resource")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let concept = resource.get("concept").and_then(Value::as_str);
            let unit = resource.get("unit").and_then(Value::as_str);
            let discriminator = (concept.is_none() || unit.is_none())
                .then(|| format!("{descendant}:{resource_index}"));
            let totals = resource_rollup
                .entry((concept, unit, discriminator))
                .or_insert_with(|| ResourceRollup {
                    concept_name: resource.get("concept_name").and_then(Value::as_str),
                    unit_name: resource.get("unit_name").and_then(Value::as_str),
                    ..ResourceRollup::default()
                });
            totals.canonical += resource
                .get("canonical_quantity")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            totals.settled += resource
                .get("settled_quantity")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            totals.remaining += resource
                .get("remaining_quantity")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
        }
    }
    blocked_transfers.sort();
    let remaining_by_resource = resource_rollup
        .into_iter()
        .map(
            |(
                (concept, unit, _),
                ResourceRollup {
                    concept_name,
                    unit_name,
                    canonical,
                    settled,
                    remaining,
                },
            )| {
                json!({
                    "concept": concept,
                    "concept_name": concept_name,
                    "unit": unit,
                    "unit_name": unit_name,
                    "canonical_quantity": canonical,
                    "settled_quantity": settled,
                    "remaining_quantity": remaining,
                })
            },
        )
        .collect::<Vec<_>>();
    (
        json!({
            "status_counts": status_counts,
            "ready": ready,
            "remaining_by_resource": remaining_by_resource,
            "transfers": descendants.len(),
        }),
        ready,
        blocked_transfers,
    )
}

pub(super) fn attach_hierarchy_projection(rows: &mut [TransferOutput]) {
    // Build summaries from the original visible rows, then publish them together.
    let summaries = hierarchy_summaries(rows);
    for (row, [hierarchy, phase6, first_completes]) in rows.iter_mut().zip(summaries) {
        if let Some(object) = row.value.as_object_mut() {
            object.insert("hierarchy".into(), hierarchy);
            object.insert("phase6".into(), phase6);
            object.insert("first_completes".into(), first_completes);
        }
    }
}

fn hierarchy_summaries(rows: &[TransferOutput]) -> Vec<[Value; 3]> {
    let values: HashMap<_, _> = rows
        .iter()
        .map(|row| (row.uid.as_str(), &row.value))
        .collect();
    let visible = values.keys().copied().collect::<HashSet<_>>();
    let parents = values
        .iter()
        .map(|(&uid, value)| (uid, value.get("parent").and_then(Value::as_str)))
        .collect::<HashMap<_, _>>();
    let mut children = HashMap::<_, Vec<_>>::new();
    for (&uid, &parent) in &parents {
        if let Some(parent) = parent.filter(|parent| visible.contains(parent)) {
            children.entry(parent).or_default().push(uid);
        }
    }
    for children in children.values_mut() {
        children.sort();
    }

    let promise_owners = values
        .iter()
        .flat_map(|(&transfer, value)| {
            value
                .get("promises")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(move |promise| {
                    promise
                        .get("uid")
                        .and_then(Value::as_str)
                        .map(|promise| (promise, transfer))
                })
        })
        .collect::<HashMap<_, _>>();
    let mut outgoing = HashMap::<_, HashSet<_>>::new();
    let mut indegree = visible
        .iter()
        .map(|&uid| (uid, 0usize))
        .collect::<HashMap<_, _>>();
    let mut add_edge = |upstream, downstream| {
        if upstream == downstream || !visible.contains(&upstream) || !visible.contains(&downstream)
        {
            return;
        }
        if outgoing.entry(upstream).or_default().insert(downstream) {
            *indegree.entry(downstream).or_default() += 1;
        }
    };
    for (&uid, value) in &values {
        for dependency in value
            .get("dependencies")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let upstream = dependency.get("upstream").and_then(Value::as_str);
            let upstream_kind = dependency.get("upstream_kind").and_then(Value::as_str);
            let upstream_transfer = match (upstream_kind, upstream) {
                (Some("transfer"), Some(upstream)) => Some(upstream),
                (Some("promise"), Some(upstream)) => promise_owners.get(upstream).copied(),
                _ => None,
            };
            if let Some(upstream) = upstream_transfer {
                add_edge(upstream, uid);
            }
        }
    }
    let mut ready = indegree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(&uid, _)| uid)
        .collect::<BTreeSet<_>>();
    let mut dependency_order = HashMap::new();
    let mut dependency_ordered_uids = Vec::new();
    while let Some(uid) = ready.pop_first() {
        dependency_order.insert(uid, dependency_ordered_uids.len());
        dependency_ordered_uids.push(uid);
        // The ready set orders every newly available Transfer before the next selection.
        for &downstream in outgoing.get(uid).into_iter().flatten() {
            if let Some(degree) = indegree.get_mut(downstream) {
                *degree = degree.saturating_sub(1);
                if *degree == 0 {
                    ready.insert(downstream);
                }
            }
        }
    }

    let mut first_completes_groups = HashMap::<_, Vec<_>>::new();
    for (&uid, value) in &values {
        if value.get("satiation").and_then(Value::as_str) == Some("first_completes")
            && let Some(source) = value.get("source").and_then(Value::as_str)
        {
            first_completes_groups.entry(source).or_default().push(uid);
        }
    }
    for members in first_completes_groups.values_mut() {
        members.sort();
    }

    rows.iter()
        .map(|row| {
            let uid = row.uid.as_str();
            let (rollup, subtree_ready, blocked_transfers) =
                subtree_rollup(uid, &values, &children);
            let direct_children = children.get(uid).map(Vec::as_slice).unwrap_or_default();
            let branches = direct_children
                .iter()
                .filter_map(|&child| {
                    values.get(child).map(|value| {
                        let (branch_rollup, _, _) = subtree_rollup(child, &values, &children);
                        json!({
                            "uid": child,
                            "status": value.get("status"),
                            "revision": value.get("revision"),
                            "ready": value.pointer("/readiness/ready"),
                            "blockers": value.pointer("/readiness/blockers"),
                            "remaining_by_resource": branch_rollup.get("remaining_by_resource"),
                            "rollup": branch_rollup,
                        })
                    })
                })
                .collect::<Vec<_>>();
            let (root, path, parent_cycle) = hierarchy_path(uid, &parents, &visible);
            let dependency_cycle = !dependency_order.contains_key(uid);
            let self_ready = row
                .value
                .pointer("/readiness/ready")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let mut phase6_blockers = row
                .value
                .get("dependencies")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|dependency| {
                    dependency.get("satisfied").and_then(Value::as_bool) == Some(false)
                })
                .map(|dependency| {
                    json!({
                        "code": "dependency_unsatisfied",
                        "dependency": dependency.get("uid"),
                        "scope": dependency.get("scope"),
                        "promise": dependency.get("promise"),
                        "upstream_kind": dependency.get("upstream_kind"),
                        "upstream": dependency.get("upstream"),
                        "required_state": dependency.get("required_state"),
                        "actual_states": dependency.get("actual_states"),
                    })
                })
                .collect::<Vec<_>>();
            if dependency_cycle {
                phase6_blockers.push(json!({ "code": "dependency_cycle" }));
            }
            if parent_cycle {
                phase6_blockers.push(json!({ "code": "hierarchy_cycle" }));
            }
            if !subtree_ready {
                phase6_blockers.push(json!({
                    "code": "descendant_not_ready",
                    "transfers": blocked_transfers,
                }));
            }

            let visible_first_completes =
                row.value
                    .get("source")
                    .and_then(Value::as_str)
                    .and_then(|source| {
                        first_completes_groups.get(source).map(|members| {
                            let mut winners = members
                                .iter()
                                .filter(|member| {
                                    values
                                        .get(*member)
                                        .and_then(|value| value.get("status"))
                                        .and_then(Value::as_str)
                                        == Some("settled")
                                })
                                .copied();
                            let (state, winner) = match (winners.next(), winners.next()) {
                                (None, _) => ("pending", None),
                                (Some(winner), None) => ("completed", Some(winner)),
                                _ => ("conflict", None),
                            };
                            json!({
                                "policy": "first_completes",
                                "source": source,
                                "scope": "visible_projection",
                                "state": state,
                                "winner": winner,
                                "members": members,
                                "role": if winner == Some(row.uid.as_str()) {
                                    "winner"
                                } else if state == "completed" {
                                    "satiated"
                                } else {
                                    "candidate"
                                },
                            })
                        })
                    });
            let first_completes = row
                .value
                .get("first_completes_evidence")
                .filter(|evidence| !evidence.is_null())
                .map(|evidence| {
                    let source = evidence.pointer("/result/source").and_then(Value::as_str);
                    let winner = evidence.pointer("/result/winner").and_then(Value::as_str);
                    json!({
                        "policy": "first_completes",
                        "source": source,
                        "scope": "visible_projection_with_authoritative_result",
                        "state": evidence.get("state"),
                        "winner": winner,
                        "members": source
                            .and_then(|source| first_completes_groups.get(source))
                            .map(Vec::as_slice)
                            .unwrap_or_default(),
                        "role": if winner == Some(row.uid.as_str()) {
                            "winner"
                        } else {
                            "loser"
                        },
                        "evidence": evidence,
                    })
                })
                .or(visible_first_completes);
            let parent = parents.get(uid).copied().flatten();
            let empty_blockers = Value::Array(Vec::new());
            let authoritative_blockers = row
                .value
                .pointer("/readiness/blockers")
                .unwrap_or(&empty_blockers);
            [
                json!({
                    "scope": "visible_projection",
                    "root": root,
                    "parent": parent,
                    "parent_visible": parent.is_none_or(|parent| visible.contains(parent)),
                    "children": direct_children,
                    "depth": path.len().saturating_sub(1),
                    "path": path,
                    "leaf": direct_children.is_empty(),
                    "rollup": rollup,
                    "branches": branches,
                }),
                json!({
                    "order": dependency_order.get(uid),
                    "dependency_order": dependency_ordered_uids,
                    "dependency_cycle": dependency_cycle,
                    "hierarchy_cycle": parent_cycle,
                    "ready": self_ready,
                    "readiness": { "ready": self_ready, "blockers": authoritative_blockers },
                    "blockers": authoritative_blockers,
                    "visible_blockers": phase6_blockers,
                }),
                first_completes.unwrap_or(Value::Null),
            ]
        })
        .collect()
}
