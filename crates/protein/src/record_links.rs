use crate::{LinkDirection, LinksInclude, ProteinError};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use store::{Store, assertions::BinaryAssertionRow};

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
        let _load = nucleus::performance::Span::new("protein-link-graph-load-await");
        nucleus::performance::add("protein-link-graph-loads", 1);
        let edges = if settings.kinds.iter().any(|kind| kind.trim() == "*") {
            store::assertions::all_binary(&store.pool).await?
        } else {
            let kinds = kind_uids(store, settings, subject).await?;
            if kinds.is_empty() {
                Vec::new()
            } else {
                store::assertions::binary_of_predicates(&store.pool, &kinds).await?
            }
        };
        Ok(Self::new(edges))
    }

    fn new(edges: Vec<BinaryAssertionRow>) -> Self {
        let mut adjacent: HashMap<String, Vec<usize>> = HashMap::new();
        for (index, edge) in edges.iter().enumerate() {
            adjacent
                .entry(edge.subject_uid.clone())
                .or_default()
                .push(index);
            if edge.subject_uid != edge.object_uid {
                adjacent
                    .entry(edge.object_uid.clone())
                    .or_default()
                    .push(index);
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
        let _projection = nucleus::performance::Span::new("protein-link-projection");
        let mut frontier = HashSet::from([record]);
        let mut visited = frontier.clone();
        let mut seen_links = HashSet::new();
        let mut out = Vec::new();
        let mut examined = 0;
        for hop in 1..=settings.depth.max(1) {
            let mut candidates: Vec<_> = frontier
                .iter()
                .filter_map(|record| self.adjacent.get(*record))
                .flatten()
                .copied()
                .collect();
            candidates.sort_unstable();
            candidates.dedup();
            examined += candidates.len() as u64;
            let mut next = HashSet::new();
            for index in candidates {
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
                if !visited.contains(other) {
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
            visited.extend(next.iter().copied());
            frontier = next;
        }
        nucleus::performance::add("protein-link-edges-visited", examined);
        out
    }
}

async fn kind_uids(
    store: &Store,
    links: &LinksInclude,
    subject: Option<&str>,
) -> Result<Vec<String>, ProteinError> {
    let mut requested = Vec::new();
    for kind in &links.kinds {
        let kind = kind.trim();
        if !kind.is_empty() && !requested.iter().any(|existing| existing == kind) {
            requested.push(kind.to_string());
        }
    }
    let mut out = Vec::new();
    for kind in requested {
        for uid in store::vocabulary::meaning_family(&store.pool, &kind, subject).await? {
            if !out.iter().any(|existing| existing == &uid) {
                out.push(uid);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
