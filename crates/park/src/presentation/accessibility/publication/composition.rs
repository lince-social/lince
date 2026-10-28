use super::*;
use accesskit::{Node, Role, Tree};

pub(super) fn merge(exports: &BTreeMap<OutputId, Arc<Publication>>) -> (TreeUpdate, BTreeMap<NodeId, Target>, BTreeMap<NodeId, Span>) {
    let mut owners = BTreeMap::<NodeId, (u64, OutputId)>::new();
    for (output, publication) in exports {
        let fields = publication.targets.values().filter(|target| target.output == *output).map(|target| target.entity).collect::<BTreeSet<_>>();
        for id in publication.targets.iter().filter(|(_, target)| target.output == *output).map(|(id, _)| id)
            .chain(publication.spans.iter().filter(|(_, span)| fields.contains(&span.entity)).map(|(id, _)| id))
        {
            let candidate = (publication.serial, *output);
            if owners.get(id).is_none_or(|current| *current < candidate) { owners.insert(*id, candidate); }
            if owners.len() >= MAX_NODES {
                let mut root = Node::new(Role::Window);
                root.set_label("Lince Park");
                root.set_description("The presented shell accessibility tree exceeds its supported node limit.");
                return (TreeUpdate { nodes: vec![(ROOT, root)], tree: Some(Tree::new(ROOT)), tree_id: TreeId::ROOT, focus: ROOT }, BTreeMap::new(), BTreeMap::new());
            }
        }
    }
    let references = owners.keys().map(|id| (id.0, *id)).collect::<BTreeMap<_, _>>();
    let mut root = Node::new(Role::Window);
    root.set_label("Lince Park");
    if let Some(publication) = exports.values().max_by_key(|publication| publication.serial)
        && let Some((_, window)) = publication.update.nodes.iter().find(|(id, _)| *id == ROOT)
    { root = window.clone(); }
    root.clear_children();
    root.clear_bounds();
    let bounds = exports.iter().filter_map(|(output, publication)| publication.bounds.get(output)).collect::<Vec<_>>();
    if !bounds.is_empty() {
        let low: [f64; 2] = std::array::from_fn(|axis| bounds.iter().map(|bounds| bounds.origin()[axis]).fold(f64::INFINITY, f64::min));
        let high: [f64; 2] = std::array::from_fn(|axis| bounds.iter().map(|bounds| bounds.origin()[axis] + bounds.size()[axis]).fold(f64::NEG_INFINITY, f64::max));
        root.set_bounds(accesskit::Rect::new(low[0], low[1], high[0], high[1]));
    }
    let mut update = TreeUpdate { nodes: Vec::new(), tree: Some(Tree::new(ROOT)), tree_id: TreeId::ROOT, focus: ROOT };
    let mut targets = BTreeMap::new();
    let mut spans = BTreeMap::new();
    let mut focus_serial = 0;
    for (output, publication) in exports {
        let owns = |id: &NodeId| owners.get(id).is_some_and(|(_, owner)| owner == output);
        let mut children = BTreeSet::new();
        for (id, node) in &publication.update.nodes {
            if !owns(id) { continue; }
            let mut node = node.clone();
            tree::relations(&mut node, &references);
            let retained = node.children().iter().copied().filter(|id| owns(id)).collect::<Vec<_>>();
            children.extend(retained.iter().copied());
            node.set_children(retained);
            update.nodes.push((*id, node));
            if let Some(target) = publication.targets.get(id) { targets.insert(*id, target.clone()); }
            if let Some(entity) = publication.spans.get(id) { spans.insert(*id, *entity); }
        }
        for id in ordered(publication) {
            if owns(&id) && !children.contains(&id) { root.push_child(id); }
        }
        if publication.serial >= focus_serial && owns(&publication.update.focus) {
            update.focus = publication.update.focus;
            focus_serial = publication.serial;
        }
    }
    update.nodes.insert(0, (ROOT, root));
    (update, targets, spans)
}

fn ordered(publication: &Publication) -> Vec<NodeId> {
    let nodes = publication.update.nodes.iter().map(|(id, node)| (*id, node)).collect::<BTreeMap<_, _>>();
    let mut stack = vec![ROOT];
    let mut visited = BTreeSet::new();
    let mut result = Vec::new();
    while let Some(id) = stack.pop() {
        if !visited.insert(id) || visited.len() > MAX_NODES { continue; }
        if id != ROOT { result.push(id); }
        if let Some(node) = nodes.get(&id) { stack.extend(node.children().iter().rev().copied()); }
    }
    result
}

#[cfg(test)]
mod tests;
