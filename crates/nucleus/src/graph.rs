//! Deterministic dependency ordering, need propagation, and cycle detection.

use std::collections::{HashMap, HashSet, VecDeque};

/// A directed dependency, optionally carrying a quantity multiplier.
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    /// The prerequisite or source node.
    pub from: String,
    /// The dependent or required node.
    pub to: String,
    /// The multiplier used during need propagation, defaulting to one.
    pub quantity: Option<f64>,
}

impl Edge {
    /// Creates an unweighted edge.
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            quantity: None,
        }
    }

    /// Creates an edge with a quantity multiplier.
    pub fn qty(from: impl Into<String>, to: impl Into<String>, quantity: f64) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            quantity: Some(quantity),
        }
    }
}

/// Orders the supplied candidate names and appends blocked candidates in input order.
#[expect(
    clippy::indexing_slicing,
    reason = "topological emits indices into candidates"
)]
pub fn topo_order(candidates: &[String], edges: &[Edge]) -> Vec<String> {
    topological(candidates, edges, |index| candidates[index].clone())
}

/// Returns candidate indices in dependency order, followed by candidates blocked by cycles.
pub fn topo_order_indices(candidates: &[impl AsRef<str>], edges: &[Edge]) -> Vec<usize> {
    topological(candidates, edges, |index| index)
}

fn index_by_name(candidates: &[impl AsRef<str>]) -> HashMap<&str, usize> {
    candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| (candidate.as_ref(), index))
        .collect()
}

#[expect(
    clippy::indexing_slicing,
    reason = "the queues and edge endpoints use indices from candidates"
)]
fn topological<T>(
    candidates: &[impl AsRef<str>],
    edges: &[Edge],
    mut emit: impl FnMut(usize) -> T,
) -> Vec<T> {
    if candidates.len() <= 1 {
        return (0..candidates.len()).map(emit).collect();
    }
    let indices = index_by_name(candidates);
    let mut indegree = vec![0usize; candidates.len()];
    let mut successors = vec![Vec::new(); candidates.len()];
    for edge in edges {
        if let (Some(&source), Some(&target)) = (
            indices.get(edge.from.as_str()),
            indices.get(edge.to.as_str()),
        ) {
            successors[source].push(target);
            indegree[target] += 1;
        }
    }
    for successors in &mut successors {
        successors.sort_unstable();
    }
    let mut ready: VecDeque<_> = indegree
        .iter()
        .enumerate()
        .filter_map(|(index, &degree)| (degree == 0).then_some(index))
        .collect();
    let mut ordered = Vec::with_capacity(candidates.len());
    while let Some(source) = ready.pop_front() {
        ordered.push(emit(source));
        for &target in &successors[source] {
            // Each edge is consumed once, so a target enters the queue at zero exactly once.
            let degree = &mut indegree[target];
            *degree -= 1;
            if *degree == 0 {
                ready.push_back(target);
            }
        }
    }
    ordered.extend(
        indegree
            .into_iter()
            .enumerate()
            .filter(|&(_, degree)| degree != 0)
            .map(|(index, _)| emit(index)),
    );
    ordered
}

/// Propagates quantities in depth-first order and combines each required node's contributions.
#[expect(
    clippy::indexing_slicing,
    reason = "first visits add the same name to both collections"
)]
pub fn derive_needs(root: &str, qty: f64, edges: &[Edge]) -> Vec<(String, f64)> {
    let mut by_source: HashMap<&str, Vec<&Edge>> = HashMap::new();
    for edge in edges {
        by_source.entry(&edge.from).or_default().push(edge);
    }
    let mut totals = HashMap::new();
    let mut ordered = Vec::new();
    let mut path = HashSet::new();
    walk(
        root,
        qty,
        &by_source,
        &mut totals,
        &mut ordered,
        &mut path,
        0,
    );
    ordered
        .into_iter()
        .map(|node| (node.to_owned(), totals[node]))
        .collect()
}

const NEED_DEPTH_LIMIT: usize = 64;

fn walk<'graph>(
    node: &'graph str,
    quantity: f64,
    by_source: &HashMap<&'graph str, Vec<&'graph Edge>>,
    totals: &mut HashMap<&'graph str, f64>,
    ordered: &mut Vec<&'graph str>,
    path: &mut HashSet<&'graph str>,
    depth: usize,
) {
    if depth > NEED_DEPTH_LIMIT || !path.insert(node) {
        return;
    }
    if let Some(children) = by_source.get(node) {
        for edge in children {
            let target = edge.to.as_str();
            let amount = quantity * edge.quantity.unwrap_or(1.0);
            *totals.entry(target).or_insert_with(|| {
                ordered.push(target);
                0.0
            }) += amount;
            walk(target, amount, by_source, totals, ordered, path, depth + 1);
        }
    }
    path.remove(node);
}

/// Finds strongly connected dependency groups, then appends self-loops in input order.
#[expect(
    clippy::indexing_slicing,
    reason = "node indices originate from nodes, and child positions use checked lookup"
)]
pub fn cycles(nodes: &[String], edges: &[(String, String)]) -> Vec<Vec<String>> {
    let indices = index_by_name(nodes);
    let mut successors = vec![Vec::new(); nodes.len()];
    let mut self_loop = vec![false; nodes.len()];
    for (from, to) in edges {
        if let (Some(&source), Some(&target)) =
            (indices.get(from.as_str()), indices.get(to.as_str()))
        {
            if source == target {
                self_loop[source] = true;
            } else {
                successors[source].push(target);
            }
        }
    }

    struct Frame {
        node: usize,
        next_child: usize,
    }

    const UNVISITED: usize = usize::MAX;
    let mut discovery = vec![UNVISITED; nodes.len()];
    let mut low_link = vec![0; nodes.len()];
    let mut on_stack = vec![false; nodes.len()];
    let mut stack = Vec::new();
    let mut frames = Vec::new();
    let mut component = Vec::new();
    let mut next_index = 0;
    let mut cycles = Vec::new();

    for start in 0..nodes.len() {
        if discovery[start] != UNVISITED {
            continue;
        }
        frames.push(Frame {
            node: start,
            next_child: 0,
        });
        discovery[start] = next_index;
        low_link[start] = next_index;
        next_index += 1;
        stack.push(start);
        on_stack[start] = true;
        while let Some(frame) = frames.last_mut() {
            let node = frame.node;
            if let Some(&child) = successors[node].get(frame.next_child) {
                frame.next_child += 1;
                if discovery[child] == UNVISITED {
                    discovery[child] = next_index;
                    low_link[child] = next_index;
                    next_index += 1;
                    stack.push(child);
                    on_stack[child] = true;
                    frames.push(Frame {
                        node: child,
                        next_child: 0,
                    });
                } else if on_stack[child] {
                    low_link[node] = low_link[node].min(discovery[child]);
                }
            } else {
                frames.pop();
                if let Some(parent) = frames.last() {
                    low_link[parent.node] = low_link[parent.node].min(low_link[node]);
                }
                if low_link[node] == discovery[node] {
                    while let Some(member) = stack.pop() {
                        on_stack[member] = false;
                        component.push(member);
                        if member == node {
                            break;
                        }
                    }
                    if component.len() > 1 {
                        cycles.push(
                            component
                                .iter()
                                .rev()
                                .map(|&index| nodes[index].clone())
                                .collect(),
                        );
                    }
                    component.clear();
                }
            }
        }
    }
    for (is_loop, node) in self_loop.into_iter().zip(nodes) {
        if is_loop {
            cycles.push(vec![node.clone()]);
        }
    }
    cycles
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(values: &[&str]) -> Vec<String> {
        values.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn focus_queue_topo_restricted_to_active_needs() {
        let edges = vec![
            Edge::new("exercise", "shower"),
            Edge::new("shower", "breakfast"),
            Edge::new("standup", "code"),
            Edge::new("breakfast", "not-a-need-today"),
        ];
        let cands = names(&["standup", "shower", "exercise", "code", "breakfast"]);
        let order = topo_order(&cands, &edges);
        assert_eq!(
            order,
            names(&["standup", "exercise", "code", "shower", "breakfast"])
        );
    }

    #[test]
    fn recurring_and_oneshot_interleave() {
        let edges = vec![Edge::new("buy-gift", "gym")];
        let order = topo_order(&names(&["gym", "buy-gift"]), &edges);
        assert_eq!(order, names(&["buy-gift", "gym"]));
    }

    #[test]
    fn recipe_explosion() {
        let edges = vec![
            Edge::qty("cake", "flour", 2.0),
            Edge::qty("cake", "egg", 3.0),
            Edge::qty("flour", "wheat", 0.5),
        ];
        let needs = derive_needs("cake", 2.0, &edges);
        assert_eq!(
            needs,
            [
                ("flour".into(), 4.0),
                ("wheat".into(), 2.0),
                ("egg".into(), 6.0)
            ]
        );
    }

    #[test]
    fn proof_finds_rule_loops() {
        let nodes = names(&["a", "b", "c", "d"]);
        let edges = vec![
            ("a".to_string(), "b".to_string()),
            ("b".to_string(), "a".to_string()),
            ("c".to_string(), "d".to_string()),
        ];
        let loops = cycles(&nodes, &edges);
        assert_eq!(loops, [names(&["a", "b"])]);
    }

    #[test]
    fn topo_survives_cycles() {
        let edges = vec![Edge::new("a", "b"), Edge::new("b", "a")];
        let order = topo_order(&names(&["a", "b", "c"]), &edges);
        assert_eq!(order, names(&["c", "a", "b"]));
    }
}
