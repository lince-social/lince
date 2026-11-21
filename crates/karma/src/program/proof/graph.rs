use std::collections::{BTreeMap, BTreeSet};

use crate::{LocalId, ProgramAst};

use super::{ProofIssueCode, Validator, nodes};

impl Validator {
    pub(super) fn validate_graph(&mut self, program: &ProgramAst) -> Vec<LocalId> {
        let ids: Vec<_> = program.nodes.keys().collect();
        let index: BTreeMap<_, _> = ids
            .iter()
            .enumerate()
            .map(|(slot, &id)| (id, slot))
            .collect();
        let mut successors = vec![Vec::new(); ids.len()];
        let mut indegrees = vec![0_usize; ids.len()];

        // Slots follow lexical node order. Edges for one target are appended together.
        for (target, node) in program.nodes.values().enumerate() {
            if node.operation.is_state_boundary() {
                continue;
            }
            for binding in node.inputs.values() {
                let Some(&source) = index.get(&binding.source.node) else {
                    continue;
                };
                let edges = &mut successors[source];
                if edges.last() != Some(&target) {
                    edges.push(target);
                    indegrees[target] += 1;
                }
            }
        }

        let mut ready: BTreeSet<_> = indegrees
            .iter()
            .enumerate()
            .filter_map(|(slot, &degree)| (degree == 0).then_some(slot))
            .collect();
        let mut order = Vec::with_capacity(ids.len());
        while let Some(slot) = ready.pop_first() {
            order.push(ids[slot].clone());
            for &successor in &successors[slot] {
                indegrees[successor] -= 1;
                if indegrees[successor] == 0 {
                    ready.insert(successor);
                }
            }
        }
        if order.len() != ids.len() {
            self.issue(
                ProofIssueCode::CombinationalCycle,
                "/nodes",
                "combinational cycle requires an explicit delay/state boundary",
                nodes(
                    ids.into_iter()
                        .zip(indegrees)
                        .filter_map(|(id, degree)| (degree > 0).then_some(id)),
                ),
            );
        }
        order
    }
}
