use super::*;
use accesskit::{Node, Role, Tree};
use std::collections::BTreeSet;

mod text;

#[cfg(test)]
pub(super) fn build(
    world: &mut World,
    view: Option<crate::native_input::projection::View>,
    geometry: Option<&lince_interface::host::GeometrySnapshot>,
    identities: &mut BTreeMap<Entity, NodeId>,
) -> (TreeUpdate, BTreeMap<NodeId, Target>, BTreeMap<NodeId, Span>) {
    build_views(world, &view.into_iter().collect::<Vec<_>>(), geometry, identities, &mut BTreeMap::new())
}

pub(super) fn build_views(
    world: &mut World,
    views: &[crate::native_input::projection::View],
    geometry: Option<&lince_interface::host::GeometrySnapshot>,
    identities: &mut BTreeMap<Entity, NodeId>,
    text_identities: &mut BTreeMap<(NodeId, usize), NodeId>,
) -> (TreeUpdate, BTreeMap<NodeId, Target>, BTreeMap<NodeId, Span>) {
    let mut window = Node::new(Role::Window);
    window.set_label("Lince Park");
    let mut update = TreeUpdate { nodes: Vec::new(), tree: Some(Tree::new(ROOT)), tree_id: TreeId::ROOT, focus: ROOT };
    if views.is_empty() {
        identities.clear(); text_identities.clear(); update.nodes.push((ROOT, window)); return (update, BTreeMap::new(), BTreeMap::new());
    }
    let rectangles = views.iter().filter_map(|view| view.viewport.as_ref()).map(|viewport| viewport.bounds).collect::<Vec<_>>();
    if !rectangles.is_empty() {
        let low: [f64; 2] = std::array::from_fn(|axis| rectangles.iter().map(|bounds| bounds.origin()[axis]).fold(f64::INFINITY, f64::min));
        let high: [f64; 2] = std::array::from_fn(|axis| rectangles.iter().map(|bounds| bounds.origin()[axis] + bounds.size()[axis]).fold(f64::NEG_INFINITY, f64::max));
        window.set_bounds(accesskit::Rect::new(low[0], low[1], high[0], high[1]));
    }
    let mut candidates = world.query::<(Entity, &bevy::a11y::AccessibilityNode)>().iter(world)
        .filter_map(|(entity, node)| {
            let context = native::accessibility::context(world, entity, false)?;
            let view = views.iter().find(|view| view.root == Some(context.root) && context.output().is_none_or(|output| output == view.output))?;
            Some((entity, node.0.clone(), context, view.clone()))
        })
        .take(MAX_NODES + 1).collect::<Vec<_>>();
    if candidates.len() > MAX_NODES {
        identities.clear();
        text_identities.clear();
        window.set_description("The shell accessibility tree exceeds its supported node limit.");
        update.nodes.push((ROOT, window));
        return (update, BTreeMap::new(), BTreeMap::new());
    }
    candidates.sort_by_key(|(entity, _, _, _)| *entity);
    let alive = candidates.iter().map(|(entity, _, _, _)| *entity).collect::<BTreeSet<_>>();
    identities.retain(|entity, _| alive.contains(entity));
    for (entity, _, _, _) in &candidates { identities.entry(*entity).or_insert_with(node_id); }
    let references = identities.iter().map(|(entity, id)| (entity.to_bits(), *id)).collect::<BTreeMap<_, _>>();
    let roots = candidates.iter().map(|(_, _, context, _)| context.root).collect::<BTreeSet<_>>();
    let Some(mut children) = hierarchy(world, &roots, identities) else {
        identities.clear();
        text_identities.clear();
        window.set_description("The shell accessibility hierarchy exceeds its supported bounds.");
        update.nodes.push((ROOT, window));
        return (update, BTreeMap::new(), BTreeMap::new());
    };
    let mut targets = BTreeMap::new();
    let mut spans = BTreeMap::new();
    let mut text_alive = BTreeSet::new();
    for (entity, mut node, context, view) in candidates {
        let id = identities[&entity];
        node.set_children(Vec::<NodeId>::new());
        node.clear_transform();
        relations(&mut node, &references);
        set_bounds(&mut node, display_bounds(world, &view, crate::native_input::projection::accessibility_bounds(world, &view, geometry, entity, None)));
        if let Some(input) = world.get::<bevy::text::EditableText>(entity) {
            if native::protected_text_input(world, entity) {
                node.set_role(Role::PasswordInput);
                node.set_value("•".repeat(input.value().chars().count()));
                node.clear_text_selection();
            }
            let start = update.nodes.len();
            native::accessibility::text_nodes(world, entity, &mut node, &mut update, node_id);
            let source = update.nodes.split_off(start);
            if text::project(world, &view, geometry, entity, &mut node, source, &mut update.nodes, &mut spans, text_identities, &mut text_alive).is_err() {
                identities.clear(); text_identities.clear();
                window.clear_children(); window.set_description("The shell accessibility text exceeds its supported node limit.");
                update.nodes = vec![(ROOT, window)];
                return (update, BTreeMap::new(), BTreeMap::new());
            }
        }
        let context = native::accessibility::context(world, entity, false).unwrap_or(context);
        let actions = native::accessibility::supported_actions(world, entity);
        node.clear_actions();
        for action in &actions { node.add_action(*action); }
        if native::accessibility::context(world, entity, true).is_none() { node.set_disabled(); }
        targets.insert(id, Target { entity, context, output: view.output, image: view.image, actions });
        update.nodes.push((id, node));
        if update.nodes.len() > MAX_NODES {
            identities.clear();
            text_identities.clear();
            window.clear_children(); window.set_description("The shell accessibility text exceeds its supported node limit.");
            update.nodes = vec![(ROOT, window)];
            return (update, BTreeMap::new(), BTreeMap::new());
        }
    }
    text_identities.retain(|key, _| text_alive.contains(key));
    for (id, node) in &mut update.nodes {
        if let Some(children) = children.remove(id) { for child in children { node.push_child(child); } }
    }
    window.set_children(children.remove(&ROOT).unwrap_or_default());
    update.nodes.insert(0, (ROOT, window));
    let mut focus = world.get_resource::<bevy::input_focus::InputFocus>().and_then(bevy::input_focus::InputFocus::get);
    for _ in 0..native::layout::MAX_DEPTH {
        let Some(entity) = focus else { break; };
        if let Some(id) = identities.get(&entity) { update.focus = *id; break; }
        focus = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    (update, targets, spans)
}

fn hierarchy(world: &World, roots: &BTreeSet<Entity>, ids: &BTreeMap<Entity, NodeId>) -> Option<BTreeMap<NodeId, Vec<NodeId>>> {
    let mut children = BTreeMap::<NodeId, Vec<NodeId>>::new();
    let mut stack = roots.iter().rev().map(|entity| (*entity, ROOT, 0)).collect::<Vec<_>>();
    let mut visited = BTreeSet::new();
    while let Some((entity, parent, depth)) = stack.pop() {
        if depth > native::layout::MAX_DEPTH || visited.len() >= MAX_NODES || !visited.insert(entity) { return None; }
        let parent = if let Some(id) = ids.get(&entity) { children.entry(parent).or_default().push(*id); *id } else { parent };
        if let Some(nodes) = world.get::<Children>(entity) {
            stack.extend(nodes.iter().rev().map(|entity| (entity, parent, depth + 1)));
        }
    }
    Some(children)
}

fn set_bounds(node: &mut Node, bounds: Option<lince_interface::host::Rectangle>) {
    if let Some(bounds) = bounds {
        let origin = bounds.origin(); let size = bounds.size();
        node.set_bounds(accesskit::Rect::new(origin[0], origin[1], origin[0] + size[0], origin[1] + size[1]));
    } else { node.clear_bounds(); }
}

fn display_bounds(world: &World, view: &crate::native_input::projection::View, bounds: Option<lince_interface::host::Rectangle>) -> Option<lince_interface::host::Rectangle> {
    let bounds = bounds?;
    let Some(snapshot) = world.get_resource::<lince_interface::host::mapping::Snapshot>() else { return Some(bounds); };
    if snapshot.outputs().is_empty() { return Some(bounds); }
    let mapping = snapshot.get(view.output)?;
    mapping.visible_displayed_rectangle(bounds)
}

#[cfg(test)]
mod tests;

pub(super) fn relations(node: &mut Node, ids: &BTreeMap<u64, NodeId>) {
    macro_rules! list {
        ($($get:ident, $set:ident);* $(;)?) => { $(
            let mapped = node.$get().iter().filter_map(|id| ids.get(&id.0).copied()).collect::<Vec<_>>();
            node.$set(mapped);
        )* };
    }
    macro_rules! single {
        ($($get:ident, $set:ident, $clear:ident);* $(;)?) => { $(
            if let Some(id) = node.$get().and_then(|id| ids.get(&id.0)).copied() { node.$set(id); } else { node.$clear(); }
        )* };
    }
    list!(controls, set_controls; details, set_details; described_by, set_described_by; flow_to, set_flow_to;
        labelled_by, set_labelled_by; owns, set_owns; radio_group, set_radio_group);
    single!(active_descendant, set_active_descendant, clear_active_descendant; error_message, set_error_message, clear_error_message;
        in_page_link_target, set_in_page_link_target, clear_in_page_link_target; member_of, set_member_of, clear_member_of;
        next_on_line, set_next_on_line, clear_next_on_line; previous_on_line, set_previous_on_line, clear_previous_on_line; popup_for, set_popup_for, clear_popup_for);
}
