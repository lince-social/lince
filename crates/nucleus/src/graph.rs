//! Link-graph algorithms (blueprint IV.2), shared by the focus queue (topo),
//! recipes/BOM (derive_needs), trails, and Proof (sccs over the rule graph).

use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub quantity: Option<f64>,
}

impl Edge {
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            quantity: None,
        }
    }

    pub fn qty(from: impl Into<String>, to: impl Into<String>, q: f64) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            quantity: Some(q),
        }
    }
}

/// Topological order restricted to `candidates` (blueprint: the focus queue is
/// topo(@precedes) over currently-active Needs). Edges whose endpoints are not
/// both candidates are ignored. Ties keep candidate order (deterministic);
/// cycles break by candidate order too (members surface in input order).
pub fn topo_order(candidates: &[String], edges: &[Edge]) -> Vec<String> {
    let index: HashMap<&str, usize> = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| (c.as_str(), i))
        .collect();
    let mut indegree: Vec<usize> = vec![0; candidates.len()];
    let mut succ: Vec<Vec<usize>> = vec![Vec::new(); candidates.len()];
    for e in edges {
        if let (Some(&f), Some(&t)) = (index.get(e.from.as_str()), index.get(e.to.as_str())) {
            succ[f].push(t);
            indegree[t] += 1;
        }
    }
    for s in &mut succ {
        s.sort_unstable();
    }
    let mut ready: VecDeque<usize> = (0..candidates.len())
        .filter(|&i| indegree[i] == 0)
        .collect();
    let mut out = Vec::with_capacity(candidates.len());
    let mut done = vec![false; candidates.len()];
    while let Some(i) = ready.pop_front() {
        if done[i] {
            continue;
        }
        done[i] = true;
        out.push(candidates[i].clone());
        for &t in &succ[i] {
            indegree[t] = indegree[t].saturating_sub(1);
            if indegree[t] == 0 && !done[t] {
                ready.push_back(t);
            }
        }
    }
    // cycle remnants, in candidate order
    for (i, c) in candidates.iter().enumerate() {
        if !done[i] {
            out.push(c.clone());
        }
    }
    out
}

/// Recipe explosion (blueprint: `derive_needs(@cake, 2)` -> "4 flour, 6 eggs").
/// Walks edges from `root`, multiplying quantities (missing = 1). Duplicate
/// descendants merge by sum. Cycles are cut by a path guard.
pub fn derive_needs(root: &str, qty: f64, edges: &[Edge]) -> Vec<(String, f64)> {
    let mut by_from: HashMap<&str, Vec<&Edge>> = HashMap::new();
    for e in edges {
        by_from.entry(e.from.as_str()).or_default().push(e);
    }
    let mut acc: HashMap<String, f64> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut path: HashSet<String> = HashSet::new();
    walk(root, qty, &by_from, &mut acc, &mut order, &mut path, 0);
    order.into_iter().map(|k| (k.clone(), acc[&k])).collect()
}

fn walk(
    node: &str,
    qty: f64,
    by_from: &HashMap<&str, Vec<&Edge>>,
    acc: &mut HashMap<String, f64>,
    order: &mut Vec<String>,
    path: &mut HashSet<String>,
    depth: usize,
) {
    if depth > 64 || path.contains(node) {
        return;
    }
    path.insert(node.to_string());
    if let Some(children) = by_from.get(node) {
        for e in children {
            let amount = qty * e.quantity.unwrap_or(1.0);
            if !acc.contains_key(&e.to) {
                order.push(e.to.clone());
            }
            *acc.entry(e.to.clone()).or_insert(0.0) += amount;
            walk(&e.to, amount, by_from, acc, order, path, depth + 1);
        }
    }
    path.remove(node);
}

/// Strongly connected components with >1 member, plus self-loops.
/// Powers Proof (blueprint VI.4): "these N rules form a loop".
pub fn cycles(nodes: &[String], edges: &[(String, String)]) -> Vec<Vec<String>> {
    let index: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.as_str(), i))
        .collect();
    let mut succ: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    let mut self_loop = vec![false; nodes.len()];
    for (f, t) in edges {
        if let (Some(&fi), Some(&ti)) = (index.get(f.as_str()), index.get(t.as_str())) {
            if fi == ti {
                self_loop[fi] = true;
            } else {
                succ[fi].push(ti);
            }
        }
    }
    // iterative Tarjan
    #[derive(Clone)]
    struct Frame {
        v: usize,
        child: usize,
    }
    let n = nodes.len();
    let mut idx = vec![usize::MAX; n];
    let mut low = vec![0usize; n];
    let mut on_stack = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut counter = 0usize;
    let mut out: Vec<Vec<String>> = Vec::new();

    for start in 0..n {
        if idx[start] != usize::MAX {
            continue;
        }
        let mut frames = vec![Frame { v: start, child: 0 }];
        idx[start] = counter;
        low[start] = counter;
        counter += 1;
        stack.push(start);
        on_stack[start] = true;
        while let Some(frame) = frames.last().cloned() {
            let v = frame.v;
            if frame.child < succ[v].len() {
                let w = succ[v][frame.child];
                frames.last_mut().unwrap().child += 1;
                if idx[w] == usize::MAX {
                    idx[w] = counter;
                    low[w] = counter;
                    counter += 1;
                    stack.push(w);
                    on_stack[w] = true;
                    frames.push(Frame { v: w, child: 0 });
                } else if on_stack[w] {
                    low[v] = low[v].min(idx[w]);
                }
            } else {
                frames.pop();
                if let Some(parent) = frames.last() {
                    low[parent.v] = low[parent.v].min(low[v]);
                }
                if low[v] == idx[v] {
                    let mut comp = Vec::new();
                    while let Some(w) = stack.pop() {
                        on_stack[w] = false;
                        comp.push(nodes[w].clone());
                        if w == v {
                            break;
                        }
                    }
                    if comp.len() > 1 {
                        comp.reverse();
                        out.push(comp);
                    }
                }
            }
        }
    }
    for (i, is_loop) in self_loop.iter().enumerate() {
        if *is_loop {
            out.push(vec![nodes[i].clone()]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn focus_queue_topo_restricted_to_active_needs() {
        // morning chain: exercise -> shower -> breakfast ; work chain: standup -> code
        let edges = vec![
            Edge::new("exercise", "shower"),
            Edge::new("shower", "breakfast"),
            Edge::new("standup", "code"),
            Edge::new("breakfast", "not-a-need-today"), // endpoint outside candidates: ignored
        ];
        // candidate order encodes the tie-breaker (window/oldest, decided by caller)
        let cands = s(&["standup", "shower", "exercise", "code", "breakfast"]);
        let order = topo_order(&cands, &edges);
        let pos = |x: &str| order.iter().position(|o| o == x).unwrap();
        assert!(pos("exercise") < pos("shower"));
        assert!(pos("shower") < pos("breakfast"));
        assert!(pos("standup") < pos("code"));
        // disjoint chains merge; head of the sort is the focus
        assert_eq!(order[0], "standup"); // first indegree-0 in candidate order
    }

    #[test]
    fn recurring_and_oneshot_interleave() {
        // one-shot 'buy-gift' linked before recurring 'gym'
        let edges = vec![Edge::new("buy-gift", "gym")];
        let order = topo_order(&s(&["gym", "buy-gift"]), &edges);
        assert_eq!(order, s(&["buy-gift", "gym"]));
    }

    #[test]
    fn recipe_explosion() {
        // cake needs 2 flour and 3 eggs; flour needs 0.5 wheat
        let edges = vec![
            Edge::qty("cake", "flour", 2.0),
            Edge::qty("cake", "egg", 3.0),
            Edge::qty("flour", "wheat", 0.5),
        ];
        let needs = derive_needs("cake", 2.0, &edges);
        let get = |k: &str| needs.iter().find(|(n, _)| n == k).unwrap().1;
        assert_eq!(get("flour"), 4.0);
        assert_eq!(get("egg"), 6.0);
        assert_eq!(get("wheat"), 2.0);
    }

    #[test]
    fn proof_finds_rule_loops() {
        let nodes = s(&["a", "b", "c", "d"]);
        let edges = vec![
            ("a".to_string(), "b".to_string()),
            ("b".to_string(), "a".to_string()),
            ("c".to_string(), "d".to_string()),
        ];
        let loops = cycles(&nodes, &edges);
        assert_eq!(loops.len(), 1);
        assert_eq!(loops[0].len(), 2);
    }

    #[test]
    fn topo_survives_cycles() {
        let edges = vec![Edge::new("a", "b"), Edge::new("b", "a")];
        let order = topo_order(&s(&["a", "b", "c"]), &edges);
        assert_eq!(order.len(), 3, "cycle members still appear");
    }
}
