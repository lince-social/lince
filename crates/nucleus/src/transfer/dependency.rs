//! Dependency terms and canonical topological ordering.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// Whether an authored dependency gates a Transfer or one Promise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferDependencyScope {
    Transfer,
    Promise,
}

impl TransferDependencyScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Transfer => "transfer",
            Self::Promise => "promise",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "transfer" => Self::Transfer,
            "promise" => Self::Promise,
            _ => return None,
        })
    }
}

/// Kind of node whose state can satisfy a dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferDependencyUpstreamKind {
    Transfer,
    Promise,
}

impl TransferDependencyUpstreamKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Transfer => "transfer",
            Self::Promise => "promise",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "transfer" => Self::Transfer,
            "promise" => Self::Promise,
            _ => return None,
        })
    }
}

/// Dependency and required state captured in a signed revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferRevisionDependency {
    pub uid: String,
    pub scope: TransferDependencyScope,
    pub promise_uid: Option<String>,
    pub upstream_kind: TransferDependencyUpstreamKind,
    pub upstream_uid: String,
    #[serde(default)]
    pub origin_organ_uid: Option<String>,
    #[serde(default = "default_dependency_required_state")]
    pub required_state: String,
}

/// A Transfer or Promise in the dependency graph.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "uid", rename_all = "snake_case")]
pub enum TransferDependencyNode {
    Transfer(String),
    Promise(String),
}

/// A directed edge from a prerequisite to its dependent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferDependencyEdge {
    pub upstream: TransferDependencyNode,
    pub downstream: TransferDependencyNode,
}

/// Order dependency nodes with a stable UID tie-break within each node kind.
///
/// Duplicate edges count once. Blocked nodes are returned in their natural order.
///
/// # Errors
///
/// Returns the nodes left unordered by a cycle, including nodes downstream of it.
pub fn transfer_dependency_order(
    edges: &[TransferDependencyEdge],
) -> Result<Vec<TransferDependencyNode>, Vec<TransferDependencyNode>> {
    #[derive(Default)]
    struct Links<'a> {
        incoming: usize,
        downstream: BTreeSet<&'a TransferDependencyNode>,
    }
    let mut graph = BTreeMap::<_, Links<'_>>::new();
    for edge in edges {
        if graph
            .entry(&edge.upstream)
            .or_default()
            .downstream
            .insert(&edge.downstream)
        {
            graph.entry(&edge.downstream).or_default().incoming += 1;
        }
    }
    let mut ready: BTreeSet<_> = graph
        .iter()
        .filter_map(|(&node, links)| (links.incoming == 0).then_some(node))
        .collect();
    let mut ordered = Vec::with_capacity(graph.len());
    while let Some(node) = ready.pop_first() {
        if let Some(links) = graph.remove(node) {
            for child in links.downstream {
                if let Some(links) = graph.get_mut(child) {
                    links.incoming -= 1;
                    if links.incoming == 0 {
                        ready.insert(child);
                    }
                }
            }
        }
        ordered.push(node);
    }
    if graph.is_empty() {
        Ok(ordered.into_iter().cloned().collect())
    } else {
        Err(graph.into_keys().cloned().collect())
    }
}

fn default_dependency_required_state() -> String {
    "kept".into()
}

#[cfg(test)]
mod tests {
    use super::{TransferDependencyEdge, TransferDependencyNode, transfer_dependency_order};

    #[test]
    fn duplicate_edges_count_once_and_cycles_keep_their_blocked_successors() {
        use TransferDependencyNode::{Promise, Transfer};

        let a = Transfer("a".into());
        let b = Promise("b".into());
        let c = Transfer("c".into());
        let d = Promise("d".into());
        let edge = |upstream: &TransferDependencyNode, downstream: &TransferDependencyNode| {
            TransferDependencyEdge {
                upstream: upstream.clone(),
                downstream: downstream.clone(),
            }
        };
        let mut edges = vec![edge(&b, &c), edge(&a, &b), edge(&a, &b), edge(&c, &d)];
        assert_eq!(
            transfer_dependency_order(&edges),
            Ok(vec![a, b.clone(), c.clone(), d.clone()])
        );
        edges.push(edge(&c, &b));
        assert_eq!(transfer_dependency_order(&edges), Err(vec![c, b, d]));
    }
}
