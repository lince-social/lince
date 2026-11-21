//! Per-query Record links and breadth-first projection.

use std::{
    collections::{HashMap, HashSet},
    mem::swap,
};

use nucleus::performance::{Span, add};
use serde_json::{Value, json};
use store::{
    Store,
    assertions::{BinaryAssertionRow, all_binary, binary_of_predicates},
    vocabulary::meaning_family,
};

use crate::{LinkDirection, LinksInclude, ProteinError};

pub(crate) struct Graph {
    edges: Vec<BinaryAssertionRow>,
    adjacent: HashMap<String, Vec<usize>>,
}

impl Graph {
    pub(crate) async fn load(
        store: &Store,
        settings: &LinksInclude,
        subject: Option<&str>,
    ) -> Result<Self, ProteinError> {
        let _load = Span::new("protein-link-graph-load-await");
        add("protein-link-graph-loads", 1);
        let edges = if settings.kinds.iter().any(|kind| kind.trim() == "*") {
            all_binary(&store.pool).await?
        } else {
            let kinds = kind_uids(store, settings, subject).await?;
            if kinds.is_empty() {
                Vec::new()
            } else {
                binary_of_predicates(&store.pool, &kinds).await?
            }
        };
        Ok(Self::new(edges))
    }

    fn new(edges: Vec<BinaryAssertionRow>) -> Self {
        let _index = Span::new("protein-link-graph-index");
        let mut adjacent: HashMap<String, Vec<usize>> = HashMap::new();
        let mut index_edge = |uid: &str, index| {
            if let Some(indices) = adjacent.get_mut(uid) {
                indices.push(index);
            } else {
                adjacent.insert(uid.to_owned(), vec![index]);
            }
        };
        for (index, edge) in edges.iter().enumerate() {
            index_edge(&edge.subject_uid, index);
            if edge.subject_uid != edge.object_uid {
                index_edge(&edge.object_uid, index);
            }
        }
        Self { edges, adjacent }
    }

    pub(crate) fn for_record(
        &self,
        record: &str,
        settings: &LinksInclude,
        concept_names: &HashMap<String, String>,
    ) -> Vec<Value> {
        let _projection = Span::new("protein-link-projection");
        let mut frontier = HashSet::from([record]);
        let mut visited = HashSet::from([record]);
        let mut seen_links = HashSet::new();
        let mut out = Vec::new();
        let mut examined: usize = 0;
        let mut candidates = Vec::new();
        let mut next = HashSet::new();
        for hop in 1..=settings.depth.max(1) {
            candidates.clear();
            candidates.extend(
                frontier
                    .iter()
                    .filter_map(|record| self.adjacent.get(*record))
                    .flatten()
                    .copied(),
            );
            candidates.sort_unstable();
            candidates.dedup();
            examined = examined.saturating_add(candidates.len());
            next.clear();
            for index in candidates.iter().copied() {
                // Each position came from the adjacency index for this edge list.
                let link = &self.edges[index];
                let outgoing = frontier.contains(link.subject_uid.as_str());
                let incoming = frontier.contains(link.object_uid.as_str());
                if matches!(settings.direction, LinkDirection::Out) && !outgoing {
                    continue;
                }
                if matches!(settings.direction, LinkDirection::In) && !incoming {
                    continue;
                }
                let other = if outgoing {
                    link.object_uid.as_str()
                } else {
                    link.subject_uid.as_str()
                };
                if visited.insert(other) {
                    next.insert(other);
                }
                if !seen_links.insert(link.uid.as_str()) {
                    continue;
                }
                out.push(json!({
                    "uid": link.uid,
                    "from": link.subject_uid,
                    "to": link.object_uid,
                    "kind_uid": link.predicate_uid,
                    "kind": concept_names.get(&link.predicate_uid).unwrap_or(&link.predicate),
                    "direction": if outgoing { "out" } else { "in" },
                    "other": other,
                    "hop": hop,
                    "quantity": link.quantity.map(|value| value.to_f64()),
                    "created_at": link.created_at,
                }));
            }
            if next.is_empty() {
                break;
            }
            swap(&mut frontier, &mut next);
        }
        add(
            "protein-link-edges-visited",
            u64::try_from(examined).unwrap_or(u64::MAX),
        );
        out
    }
}

async fn kind_uids(
    store: &Store,
    links: &LinksInclude,
    subject: Option<&str>,
) -> Result<Vec<String>, ProteinError> {
    let mut requested = HashSet::new();
    let mut out = Vec::new();
    for kind in links.kinds.iter().map(|kind| kind.trim()) {
        if kind.is_empty() || !requested.insert(kind) {
            continue;
        }
        for uid in meaning_family(&store.pool, kind, subject).await? {
            if !out.contains(&uid) {
                out.push(uid);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
