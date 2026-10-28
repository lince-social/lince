use super::*;
use accesskit::{Node, Role};
use native::{actions::{ActionButton, ActionSequence}, workspace::Workspaces};
use std::sync::atomic::AtomicU64;

fn fixture() -> (World, crate::native_input::projection::View, Entity, Entity) {
    let mut world = World::new();
    world.init_resource::<bevy::input_focus::InputFocus>();
    let root = world.spawn((Workspaces::default(), bevy::ui::Node::default(), InheritedVisibility::VISIBLE)).id();
    let mut accessible = accesskit::Node::new(Role::Button); accessible.set_label("Save changes");
    let button = world.spawn((bevy::ui::Node::default(), bevy::a11y::AccessibilityNode(accessible),
        InheritedVisibility::VISIBLE, ActionButton::new(root, ActionSequence::default()), ChildOf(root))).id();
    let view = crate::native_input::projection::View { root: Some(root), output: lince_interface::host::OutputId(1),
        image: Handle::<Image>::default().id(), raster: Default::default(), origin: [0.0; 2], viewport: None };
    (world, view, root, button)
}

fn shared() -> (Arc<Shared>, Receiver<Request>, Arc<AtomicU64>) {
    let (sender, receiver) = sync_channel(MAX_REQUESTS);
    let wakes = Arc::new(AtomicU64::new(0)); let observed = wakes.clone();
    let shared = Arc::new(Shared { catalog: Mutex::new(Catalog::default()), sender,
        requested: Default::default(), refresh: AtomicBool::new(false), wake: WakeSignal::new(move || { observed.fetch_add(1, Ordering::Relaxed); }),
        progress: None, pending_input: AtomicU64::new(0) });
    (shared, receiver, wakes)
}

fn action(target: NodeId) -> ActionRequest {
    ActionRequest { action: Action::Focus, target_tree: TreeId::ROOT, target_node: target, data: None }
}

#[test]
fn handler_requests_are_bounded_and_wake_the_existing_worker_without_mutating_its_world() {
    let (mut world, view, _, button) = fixture();
    let (_, targets, spans) = tree::build(&mut world, Some(view), None, &mut BTreeMap::new());
    let id = targets.iter().find(|(_, target)| target.entity == button).unwrap().0;
    let (mut shared, receiver, wakes) = shared();
    let progress = crate::runtime::InputProgress::default();
    progress.queued(7);
    Arc::get_mut(&mut shared).unwrap().progress = Some(progress.clone());
    shared.catalog.lock().unwrap().targets = targets.clone(); shared.catalog.lock().unwrap().spans = spans;
    let mut handler = Handler(shared.clone());
    assert!(handler.request_initial_tree().is_none());
    for _ in 0..MAX_REQUESTS + 20 { handler.do_action(action(*id)); }
    assert_eq!(shared.pending_input.load(Ordering::Acquire), 7);
    progress.queued(19);
    handler.do_action(action(*id));
    assert_eq!(shared.pending_input.load(Ordering::Acquire), 7);
    assert_eq!(wakes.load(Ordering::Relaxed), (MAX_REQUESTS + 1) as u64);
    assert_eq!(world.resource::<bevy::input_focus::InputFocus>().get(), None);
    let requests = receiver.try_iter().collect::<Vec<_>>();
    assert_eq!(requests.len(), MAX_REQUESTS);
    let first = &requests[0];
    native::accessibility::perform(&mut world, first.target.entity, &first.target.context, &first.action).unwrap();
    assert_eq!(world.resource::<bevy::input_focus::InputFocus>().get(), Some(button));
    assert!(native::accessibility::owns_keyboard(&world));
}

#[test]
fn deletion_and_lock_retire_tree_nodes_and_captured_actions_cannot_target_a_replacement() {
    let (mut world, view, root, button) = fixture();
    let mut ids = BTreeMap::new();
    let (before, targets, _) = tree::build(&mut world, Some(view.clone()), None, &mut ids);
    let (id, target) = targets.iter().find(|(_, target)| target.entity == button).map(|(id, target)| (*id, target.clone())).unwrap();
    assert!(before.nodes.iter().any(|(_, node)| node.label() == Some("Save changes")));
    world.entity_mut(button).despawn();
    let mut node = Node::new(Role::Button); node.set_label("Replacement");
    let replacement = world.spawn((bevy::ui::Node::default(), bevy::a11y::AccessibilityNode(node), InheritedVisibility::VISIBLE,
        ActionButton::new(root, ActionSequence::default()), ChildOf(root))).id();
    let (_, next, _) = tree::build(&mut world, Some(view.clone()), None, &mut ids);
    assert!(!next.contains_key(&id));
    assert_ne!(next.iter().find(|(_, target)| target.entity == replacement).unwrap().0, &id);
    assert!(native::accessibility::perform(&mut world, button, &target.context, &action(id)).is_err());
    world.insert_resource(lince_interface::host::control::Status { available: true, active: true, locked: true, ..default() });
    let (locked, targets, spans) = tree::build(&mut world, Some(view), None, &mut ids);
    assert_eq!(locked.nodes.len(), 1);
    assert!(targets.is_empty() && spans.is_empty());
}

#[test]
fn hierarchy_follows_existing_child_order_and_retains_disabled_controls_with_no_actions() {
    let (mut world, view, root, first) = fixture();
    let mut node = Node::new(Role::Button); node.set_label("Second");
    let second = world.spawn((bevy::ui::Node::default(), bevy::a11y::AccessibilityNode(node), InheritedVisibility::VISIBLE,
        ActionButton::new(root, ActionSequence::default()), ChildOf(root))).id();
    world.entity_mut(root).replace_children(&[second, first]);
    world.entity_mut(first).insert(bevy::ui::InteractionDisabled);
    let mut ids = BTreeMap::new();
    let (update, targets, _) = tree::build(&mut world, Some(view), None, &mut ids);
    let root = update.nodes.iter().find(|(id, _)| *id == ROOT).unwrap();
    assert_eq!(root.1.children(), &[ids[&second], ids[&first]]);
    let disabled = update.nodes.iter().find(|(id, _)| *id == ids[&first]).unwrap();
    assert!(disabled.1.is_disabled());
    assert!(targets[&ids[&first]].actions.is_empty());
}

#[test]
fn stale_child_lists_are_replaced_before_attaching_live_accessibility_descendants() {
    let (mut world, view, _, parent) = fixture();
    world.get_mut::<bevy::a11y::AccessibilityNode>(parent).unwrap().set_children([NodeId(u64::MAX)]);
    let child = world.spawn((bevy::ui::Node::default(), bevy::a11y::AccessibilityNode(Node::new(Role::Label)),
        InheritedVisibility::VISIBLE, ChildOf(parent))).id();
    let mut ids = BTreeMap::new();
    let (update, _, _) = tree::build(&mut world, Some(view), None, &mut ids);
    let parent = update.nodes.iter().find(|(id, _)| *id == ids[&parent]).unwrap();
    assert_eq!(parent.1.children(), &[ids[&child]]);
    assert!(update.nodes.iter().all(|(_, node)| !node.children().contains(&NodeId(u64::MAX))));
}

fn publication(world: &mut World, view: &crate::native_input::projection::View, serial: u64) -> Arc<Publication> {
    let (update, targets, spans) = tree::build(world, Some(view.clone()), None, &mut BTreeMap::new());
    Arc::new(Publication { epoch: 0, serial, update, targets, spans, bounds: BTreeMap::new() })
}

#[test]
fn each_output_publishes_independently_and_duplicate_receipts_do_not_wake() {
    let (mut world, view, _, _) = fixture();
    let publication = publication(&mut world, &view, 1);
    let (shared, _, wakes) = shared();
    shared.requested.set(true);
    let receipt = Receipt::new(publication.clone(), [view.output, lince_interface::host::OutputId(2)], shared.clone());
    receipt.presented(lince_interface::host::OutputId(3));
    receipt.presented(view.output);
    assert_eq!(wakes.load(Ordering::Relaxed), 1);
    assert_eq!(shared.catalog.lock().unwrap().presented.len(), 1);
    assert!(shared.catalog.lock().unwrap().targets.is_empty());
    receipt.clone().presented(lince_interface::host::OutputId(2));
    assert_eq!(wakes.load(Ordering::Relaxed), 2);
    assert_eq!(shared.catalog.lock().unwrap().presented.len(), 2);
    assert!(Arc::ptr_eq(shared.catalog.lock().unwrap().presented.get(&view.output).unwrap(), &publication));
    assert!(shared.catalog.lock().unwrap().targets.is_empty());
    receipt.presented(view.output);
    receipt.presented(lince_interface::host::OutputId(2));
    assert_eq!(wakes.load(Ordering::Relaxed), 2);
}

#[test]
fn a_later_presented_frame_wins_and_an_older_receipt_cannot_reopen_its_catalog() {
    let (mut world, view, _, _) = fixture();
    let (shared, _, wakes) = shared();
    shared.requested.set(true);
    let old = Receipt::new(publication(&mut world, &view, 1), [view.output], shared.clone());
    let new = Receipt::new(publication(&mut world, &view, 2), [view.output], shared.clone());
    new.presented(view.output);
    let presented = shared.catalog.lock().unwrap().presented.remove(&view.output).unwrap();
    assert_eq!(presented.serial, 2);
    old.presented(view.output);
    assert!(shared.catalog.lock().unwrap().presented.is_empty());
    assert_eq!(wakes.load(Ordering::Relaxed), 1);
}

#[test]
fn presented_controls_survive_pending_geometry_without_accepting_stale_actions_and_hidden_owners_retire_individually() {
    let (mut world, view, root, first) = fixture();
    let second = world.spawn((bevy::ui::Node::default(), bevy::a11y::AccessibilityNode(Node::new(Role::Button)),
        InheritedVisibility::VISIBLE, ActionButton::new(root, ActionSequence::default()), ChildOf(root))).id();
    let source = publication(&mut world, &view, 1);
    let (id, target) = source.targets.iter().find(|(_, target)| target.entity == first).unwrap();
    world.insert_resource(lince_interface::host::display::DisplaySnapshot { revision: 1, ..Default::default() });
    assert!(!target.context.describes(&world, first));
    assert!(publication::retained(&world, std::slice::from_ref(&view), view.output, &source).is_none());
    assert!(native::accessibility::perform(&mut world, first, &target.context, &action(*id)).is_err());
    world.entity_mut(first).insert(InheritedVisibility::HIDDEN);
    let filtered = publication::retained(&world, std::slice::from_ref(&view), view.output, &source).unwrap();
    assert!(!filtered.targets.values().any(|target| target.entity == first));
    assert!(filtered.targets.values().any(|target| target.entity == second));
    world.entity_mut(first).insert(InheritedVisibility::VISIBLE);
    world.insert_resource(lince_interface::host::control::Status { available: true, active: true, locked: true, ..Default::default() });
    let locked = publication::retained(&world, std::slice::from_ref(&view), view.output, &source).unwrap();
    assert!(locked.targets.is_empty());
}

#[test]
fn reader_deactivation_retires_queued_actions_and_receipts_even_if_reactivation_is_immediate() {
    let (mut world, view, _, _) = fixture();
    let publication = publication(&mut world, &view, 1);
    let node = *publication.targets.keys().next().unwrap();
    let (shared, receiver, _) = shared();
    shared.catalog.lock().unwrap().targets = publication.targets.clone();
    let receipt = Receipt::new(publication, [view.output], shared.clone());
    let mut handler = Handler(shared.clone());
    assert!(handler.request_initial_tree().is_none());
    handler.do_action(action(node));
    let queued = receiver.try_recv().unwrap();
    handler.deactivate_accessibility();
    assert!(handler.request_initial_tree().is_none());
    assert!(!current(&shared.catalog.lock().unwrap(), &queued));
    handler.do_action(action(node));
    assert!(receiver.try_recv().is_err());
    receipt.presented(view.output);
    assert!(shared.catalog.lock().unwrap().presented.is_empty());
}

#[test]
fn retiring_a_session_or_adapter_epoch_disables_all_in_flight_receipts() {
    let (mut world, view, _, _) = fixture();
    let (shared, _, wakes) = shared();
    shared.requested.set(true);
    let receipt = Receipt::new(publication(&mut world, &view, 1), [view.output], shared.clone());
    shared.catalog.lock().unwrap().epoch = 1;
    receipt.presented(view.output);
    assert!(shared.catalog.lock().unwrap().presented.is_empty());
    assert_eq!(wakes.load(Ordering::Relaxed), 0);
    let mut next = publication(&mut world, &view, 2);
    Arc::get_mut(&mut next).unwrap().epoch = 1;
    let receipt = Receipt::new(next, [view.output], shared.clone());
    shared.requested.set(false);
    receipt.presented(view.output);
    assert!(shared.catalog.lock().unwrap().presented.is_empty());
    assert_eq!(wakes.load(Ordering::Relaxed), 0);
}

#[test]
fn an_older_completed_export_remains_available_while_a_newer_frame_is_waiting() {
    let (mut world, view, _, _) = fixture();
    let (shared, _, wakes) = shared();
    shared.requested.set(true);
    let first = Receipt::new(publication(&mut world, &view, 1), [view.output], shared.clone());
    let second = Receipt::new(publication(&mut world, &view, 2), [view.output], shared.clone());
    first.presented(view.output);
    let presented = shared.catalog.lock().unwrap().presented.remove(&view.output).unwrap();
    assert_eq!(presented.serial, 1);
    second.presented(view.output);
    assert_eq!(shared.catalog.lock().unwrap().presented.remove(&view.output).unwrap().serial, 2);
    assert_eq!(wakes.load(Ordering::Relaxed), 2);
}

#[test]
fn a_selection_request_requires_both_current_text_spans_of_the_same_field() {
    let (mut world, view, _, button) = fixture();
    let publication = publication(&mut world, &view, 1);
    let (node, target) = publication.targets.iter().next().unwrap();
    let mut catalog = Catalog { targets: publication.targets.clone(), ..Default::default() };
    let mut request = Request { epoch: 0, target: target.clone(), action: action(*node), selection: None };
    assert!(current(&catalog, &request));
    let anchor = NodeId(10_000);
    let focus = NodeId(10_001);
    request.action.action = Action::SetTextSelection;
    request.action.data = Some(ActionData::SetTextSelection(accesskit::TextSelection {
        anchor: accesskit::TextPosition { node: anchor, character_index: 0 },
        focus: accesskit::TextPosition { node: focus, character_index: 1 },
    }));
    assert!(!current(&catalog, &request));
    catalog.spans.insert(anchor, Span { entity: button, source: accesskit::TextPosition { node: anchor, character_index: 0 }, length: 1 });
    catalog.spans.insert(focus, Span { entity: button, source: accesskit::TextPosition { node: focus, character_index: 0 }, length: 1 });
    if let Some(ActionData::SetTextSelection(selection)) = &request.action.data { request.selection = Some(*selection); }
    assert!(current(&catalog, &request));
    catalog.spans.remove(&focus);
    assert!(!current(&catalog, &request));
}

#[test]
fn projected_selection_actions_reach_the_original_parley_clusters_and_reject_foreign_or_stale_spans() {
    use bevy::{ecs::system::RunSystemOnce, text::{EditableText, FontCx, LayoutCx}};
    let (mut world, view, _, field) = fixture();
    world.init_resource::<FontCx>();
    world.init_resource::<LayoutCx>();
    world.init_resource::<bevy::clipboard::Clipboard>();
    let font = Font::from_bytes(include_bytes!("../../../../../institute/assets/fonts/Lato/Lato-Regular.ttf").to_vec());
    world.resource_mut::<FontCx>().collection.register_fonts(font.data, None);
    world.resource_mut::<FontCx>().set_sans_serif_family("Lato").unwrap();
    world.entity_mut(field).remove::<ActionButton>();
    world.entity_mut(field).insert((EditableText::new("aé界z"), bevy::a11y::AccessibilityNode(Node::new(Role::TextInput))));
    world.run_system_once(bevy::text::apply_text_edits).unwrap();
    let (update, targets, spans) = tree::build(&mut world, Some(view.clone()), None, &mut BTreeMap::new());
    let target = targets.iter().find(|(_, target)| target.entity == field).map(|(id, _)| *id).unwrap();
    let anchor = update.nodes.iter().find(|(_, node)| node.value() == Some("é")).map(|(id, _)| *id).unwrap();
    let focus = update.nodes.iter().find(|(_, node)| node.value() == Some("界")).map(|(id, _)| *id).unwrap();
    let (shared, receiver, _) = shared();
    shared.requested.set(true);
    shared.catalog.lock().unwrap().targets = targets;
    shared.catalog.lock().unwrap().spans = spans;
    let mut handler = Handler(shared.clone());
    let selection = accesskit::TextSelection {
        anchor: accesskit::TextPosition { node: anchor, character_index: 0 },
        focus: accesskit::TextPosition { node: focus, character_index: 1 },
    };
    let action = ActionRequest { action: Action::SetTextSelection, target_tree: TreeId::ROOT, target_node: target,
        data: Some(ActionData::SetTextSelection(selection)) };
    handler.do_action(action.clone());
    let request = receiver.try_recv().unwrap();
    assert!(current(&shared.catalog.lock().unwrap(), &request));
    assert_ne!(request.native_action().data, request.action.data);
    native::accessibility::perform(&mut world, field, &request.target.context, &request.native_action()).unwrap();
    assert_eq!(world.get::<EditableText>(field).unwrap().editor.raw_selection().text_range(), 1..6);
    let (published, _, _) = tree::build(&mut world, Some(view), None, &mut BTreeMap::new());
    let field_node = published.nodes.iter().find(|(_, node)| node.role() == Role::TextInput).unwrap();
    let selection = field_node.1.text_selection().unwrap();
    let offset = |position: accesskit::TextPosition| {
        let mut offset = 0;
        for id in field_node.1.children() {
            let node = &published.nodes.iter().find(|(candidate, _)| candidate == id).unwrap().1;
            if *id == position.node { return offset + position.character_index; }
            offset += node.value().unwrap_or_default().chars().count();
        }
        panic!("The published selection must reference its field's text")
    };
    assert_eq!((offset(selection.anchor), offset(selection.focus)), (1, 3));
    let foreign = world.spawn_empty().id();
    shared.catalog.lock().unwrap().spans.get_mut(&focus).unwrap().entity = foreign;
    handler.do_action(action.clone());
    assert!(receiver.try_recv().is_err());
    assert!(!current(&shared.catalog.lock().unwrap(), &request));
    shared.catalog.lock().unwrap().spans.get_mut(&focus).unwrap().entity = field;
    handler.do_action(action);
    let request = receiver.try_recv().unwrap();
    native::accessibility::secret(&mut world, field);
    assert!(native::accessibility::perform(&mut world, field, &request.target.context, &request.native_action()).is_err());
    assert_eq!(world.get::<EditableText>(field).unwrap().value(), "aé界z");
}

pub(super) fn companion_fixture() -> (World, Vec<crate::native_input::projection::View>, Entity, Entity) {
    let (mut world, mut first_view, root, first) = fixture();
    let mut second_space = world.get::<Workspaces>(root).unwrap().entries[0].clone();
    second_space.id = 2;
    world.get_mut::<Workspaces>(root).unwrap().entries.push(second_space);
    let output = |id, workspace, x| {
        let bounds = lince_interface::host::Rectangle::new([x, 0.0], [800.0, 600.0]).unwrap();
        lince_interface::host::OutputViewport { rotation: Default::default(), id: lince_interface::host::OutputId(id), bounds, usable: Some(bounds),
            physical_size: [800, 600], raster: lince_interface::host::raster::RasterMapping { origin: bounds.origin(), scale: [1.0; 2] }, workspace }
    };
    let first_output = output(1, 1, 0.0);
    let second_output = output(2, 2, 800.0);
    world.insert_resource(native::output_views::OutputViews::new(vec![first_output.clone(), second_output.clone()], first_output.id).unwrap());
    first_view.viewport = Some(first_output);
    let mut second_view = first_view.clone();
    second_view.output = second_output.id;
    second_view.raster = second_output.raster;
    second_view.viewport = Some(second_output);
    let mut node = Node::new(Role::Button); node.set_label("Companion action");
    let second = world.spawn((bevy::ui::Node::default(), bevy::a11y::AccessibilityNode(node), InheritedVisibility::VISIBLE,
        ActionButton::new(root, ActionSequence::default()), native::canvas::CanvasItem { position: bevy::math::DVec2::ZERO, size: Vec2::new(200.0, 80.0) },
        native::layout::LayoutBox::new(Vec2::new(200.0, 80.0)), native::workspace::WorkspaceMember(2), ChildOf(root))).id();
    (world, vec![first_view, second_view], first, second)
}

#[test]
fn companion_controls_are_exported_on_their_own_view_and_disappear_when_that_view_is_missing() {
    let (mut world, views, first, second) = companion_fixture();
    let first_view = views[0].clone();
    let second_view = views[1].clone();
    let mut ids = BTreeMap::new();
    let (_, targets, _) = tree::build_views(&mut world, &[first_view.clone(), second_view.clone()], None, &mut ids, &mut BTreeMap::new());
    let old_id = ids[&second];
    assert_eq!(targets[&ids[&first]].output, first_view.output);
    assert_eq!(targets[&old_id].output, second_view.output);
    let (_, targets, _) = tree::build_views(&mut world, std::slice::from_ref(&first_view), None, &mut ids, &mut BTreeMap::new());
    assert!(!targets.values().any(|target| target.entity == second));
    let (_, targets, _) = tree::build_views(&mut world, &[first_view, second_view], None, &mut ids, &mut BTreeMap::new());
    assert_ne!(ids[&second], old_id);
    assert!(!targets.contains_key(&old_id));
}
