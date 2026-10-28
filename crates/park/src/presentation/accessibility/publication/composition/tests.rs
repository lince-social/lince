use super::*;

#[test]
fn moving_a_presented_field_to_another_output_retires_its_old_text_nodes_with_the_old_parent() {
    let (mut world, views, field, _) = crate::presentation::accessibility::tests::companion_fixture();
    let mut ids = BTreeMap::new();
    let mut older = Arc::try_unwrap(capture(&mut world, &views, &mut ids, 1)).ok().unwrap();
    let parent = ids[&field];
    let old_text = node_id();
    let new_text = node_id();
    let mut text = accesskit::Node::new(accesskit::Role::TextRun);
    text.set_value("a");
    older.update.nodes.iter_mut().find(|(id, _)| *id == parent).unwrap().1.set_children([old_text]);
    older.update.nodes.push((old_text, text.clone()));
    older.spans.insert(old_text, Span { entity: field, source: accesskit::TextPosition { node: old_text, character_index: 0 }, length: 1 });
    let mut newer = Publication { epoch: older.epoch, serial: 2, update: older.update.clone(), targets: older.targets.clone(),
        spans: BTreeMap::new(), bounds: older.bounds.clone() };
    newer.targets.get_mut(&parent).unwrap().output = views[1].output;
    newer.update.nodes.retain(|(id, _)| *id != old_text);
    newer.update.nodes.iter_mut().find(|(id, _)| *id == parent).unwrap().1.set_children([new_text]);
    newer.update.nodes.push((new_text, text));
    newer.spans.insert(new_text, Span { entity: field, source: accesskit::TextPosition { node: new_text, character_index: 0 }, length: 1 });
    let (update, targets, spans) = merge(&BTreeMap::from([(views[0].output, Arc::new(older)), (views[1].output, Arc::new(newer))]));
    assert_eq!(targets[&parent].output, views[1].output);
    assert!(!spans.contains_key(&old_text));
    assert!(spans.contains_key(&new_text));
    let nodes = update.nodes.iter().map(|(id, node)| (*id, node)).collect::<BTreeMap<_, _>>();
    let mut pending = vec![ROOT];
    let mut reachable = BTreeSet::new();
    while let Some(id) = pending.pop() {
        assert!(reachable.insert(id));
        pending.extend_from_slice(nodes[&id].children());
    }
    assert_eq!(reachable, nodes.keys().copied().collect::<BTreeSet<_>>());
}

fn capture(world: &mut World, views: &[crate::native_input::projection::View], ids: &mut BTreeMap<Entity, NodeId>, serial: u64) -> Arc<Publication> {
    let (update, targets, spans) = tree::build_views(world, views, None, ids, &mut BTreeMap::new());
    let bounds = views.iter().filter_map(|view| view.viewport.as_ref().map(|viewport| (view.output, viewport.bounds))).collect();
    Arc::new(Publication { epoch: 0, serial, update, targets, spans, bounds })
}

#[test]
fn each_output_keeps_its_own_presented_semantics_when_another_output_is_ahead() {
    let (mut world, views, first, second) = crate::presentation::accessibility::tests::companion_fixture();
    let mut ids = BTreeMap::new();
    let older = capture(&mut world, &views, &mut ids, 1);
    world.get_mut::<bevy::a11y::AccessibilityNode>(first).unwrap().set_label("New first action");
    world.get_mut::<bevy::a11y::AccessibilityNode>(second).unwrap().set_label("New companion action");
    let newer = capture(&mut world, &views, &mut ids, 2);
    let mut exports = BTreeMap::from([(views[0].output, older.clone())]);
    let (first_only, targets, _) = merge(&exports);
    assert!(first_only.nodes.iter().any(|(_, node)| node.label() == Some("Save changes")));
    assert!(!first_only.nodes.iter().any(|(_, node)| node.label() == Some("Companion action")));
    assert!(!targets.values().any(|target| target.entity == second));
    exports.insert(views[1].output, newer);
    let (combined, targets, _) = merge(&exports);
    assert!(combined.nodes.iter().any(|(_, node)| node.label() == Some("Save changes")));
    assert!(combined.nodes.iter().any(|(_, node)| node.label() == Some("New companion action")));
    assert!(!combined.nodes.iter().any(|(_, node)| node.label() == Some("New first action")));
    assert_eq!(targets[&ids[&first]].output, views[0].output);
    assert_eq!(targets[&ids[&second]].output, views[1].output);
    let live = combined.nodes.iter().map(|(id, _)| *id).collect::<BTreeSet<_>>();
    assert_eq!(live.len(), combined.nodes.len());
    assert!(combined.nodes.iter().all(|(_, node)| node.children().iter().all(|child| live.contains(child))));
    let root = combined.nodes.iter().find(|(id, _)| *id == ROOT).unwrap();
    assert_eq!(root.1.bounds().unwrap(), accesskit::Rect::new(0.0, 0.0, 1600.0, 600.0));
    exports.remove(&views[0].output);
    let (companion_only, targets, _) = merge(&exports);
    assert!(!targets.values().any(|target| target.entity == first));
    assert!(companion_only.nodes.iter().any(|(_, node)| node.label() == Some("New companion action")));
    assert_eq!(older.serial, 1);
}

#[test]
fn a_relation_to_an_output_that_has_not_presented_is_not_exported_as_a_dangling_node() {
    let (mut world, views, first, second) = crate::presentation::accessibility::tests::companion_fixture();
    world.get_mut::<bevy::a11y::AccessibilityNode>(first).unwrap().set_described_by([NodeId(second.to_bits())]);
    let mut ids = BTreeMap::new();
    let publication = capture(&mut world, &views, &mut ids, 1);
    let mut exports = BTreeMap::from([(views[0].output, publication.clone())]);
    let (alone, _, _) = merge(&exports);
    assert!(alone.nodes.iter().find(|(id, _)| *id == ids[&first]).unwrap().1.described_by().is_empty());
    exports.insert(views[1].output, publication);
    let (together, _, _) = merge(&exports);
    assert_eq!(together.nodes.iter().find(|(id, _)| *id == ids[&first]).unwrap().1.described_by(), &[ids[&second]]);
}
