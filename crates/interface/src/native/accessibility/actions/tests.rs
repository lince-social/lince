use super::*;
use accesskit::{NodeId, Role, TreeId, TreeUpdate};
use bevy::ecs::system::RunSystemOnce;

fn fixture(value: &str) -> (World, Entity, Entity) {
    let mut world = World::new();
    world.init_resource::<InputFocus>();
    world.init_resource::<FontCx>();
    world.init_resource::<LayoutCx>();
    world.init_resource::<bevy::clipboard::Clipboard>();
    let font = Font::from_bytes(include_bytes!("../../../../../../institute/assets/fonts/Lato/Lato-Regular.ttf").to_vec());
    world.resource_mut::<FontCx>().collection.register_fonts(font.data, None);
    world.resource_mut::<FontCx>().set_sans_serif_family("Lato").unwrap();
    let root = world.spawn((Workspaces::default(), Node::default(), InheritedVisibility::VISIBLE)).id();
    let mut node = accesskit::Node::new(Role::TextInput); node.set_label("Workspace name");
    let field = world.spawn((EditableText::new(value), AccessibilityNode(node), Node::default(), InheritedVisibility::VISIBLE, ChildOf(root))).id();
    world.run_system_once(bevy::text::apply_text_edits).unwrap();
    (world, root, field)
}

fn request(action: Action, data: Option<ActionData>) -> ActionRequest {
    ActionRequest { action, target_tree: TreeId::ROOT, target_node: NodeId(5), data }
}

#[test]
fn accessibility_value_uses_normal_editor_limits_unicode_and_keyboard_ownership() {
    let (mut world, _, field) = fixture("Home");
    world.get_mut::<EditableText>(field).unwrap().max_characters = Some(8);
    let captured = context(&world, field, true).unwrap();
    perform(&mut world, field, &captured, &request(Action::SetValue, Some(ActionData::Value("Café 界".into())))).unwrap();
    assert!(owns_keyboard(&world));
    world.run_system_once(bevy::text::apply_text_edits).unwrap();
    assert_eq!(world.get::<EditableText>(field).unwrap().value(), "Café 界");
    let captured = context(&world, field, true).unwrap();
    assert!(perform(&mut world, field, &captured, &request(Action::SetValue, Some(ActionData::Value("value is too long".into())))).is_err());
    assert_eq!(world.get::<EditableText>(field).unwrap().value(), "Café 界");
    perform(&mut world, field, &captured, &request(Action::Blur, None)).unwrap();
    assert!(!owns_keyboard(&world));
}

#[test]
fn activity_loss_and_return_retire_captured_accessibility_edits_before_a_delayed_host_status_and_fresh_edits_recover() {
    let (mut world, _, field) = fixture("Retain this");
    world.insert_resource(crate::host::control::Status { available: true, active: true, locked: false, ..default() });
    let activity = crate::host::input::InputActivity::default();
    world.insert_resource(activity.clone());
    let captured = context(&world, field, true).unwrap();
    activity.set_active(false);
    assert!(context(&world, field, false).is_none());
    assert!(perform(&mut world, field, &captured, &request(Action::SetValue, Some(ActionData::Value("wrong".into())))).is_err());
    activity.set_active(true);
    assert!(perform(&mut world, field, &captured, &request(Action::SetValue, Some(ActionData::Value("still wrong".into())))).is_err());
    assert_eq!(world.get::<EditableText>(field).unwrap().value(), "Retain this");
    let current = context(&world, field, true).unwrap();
    assert!(!captured.same_owner(&current));
    perform(&mut world, field, &current, &request(Action::SetValue, Some(ActionData::Value("fresh".into())))).unwrap();
    world.run_system_once(bevy::text::apply_text_edits).unwrap();
    assert_eq!(world.get::<EditableText>(field).unwrap().value(), "fresh");
    world.get_resource_mut::<crate::host::input::InputActivity>().unwrap().begin_frame();
    activity.set_active(false); activity.set_active(true);
    assert!(context(&world, field, true).is_none());
    world.get_resource_mut::<crate::host::input::InputActivity>().unwrap().begin_frame();
    assert!(context(&world, field, true).is_some());
}

#[test]
fn changed_canvas_hidden_lock_and_replaced_modal_reject_queued_control_actions() {
    let (mut world, root, field) = fixture("Keep me");
    let captured = context(&world, field, true).unwrap();
    world.get_mut::<Node>(field).unwrap().display = Display::None;
    assert!(perform(&mut world, field, &captured, &request(Action::Focus, None)).is_err());
    world.get_mut::<Node>(field).unwrap().display = Display::Flex;
    world.insert_resource(crate::host::control::Status { available: true, active: true, locked: true, ..default() });
    assert!(perform(&mut world, field, &captured, &request(Action::SetValue, Some(ActionData::Value("wrong".into())))).is_err());
    world.resource_mut::<crate::host::control::Status>().locked = false;
    let replacement = world.spawn((Node::default(), ChildOf(root))).id();
    shortcuts::own_keyboard(&mut world, replacement);
    assert!(perform(&mut world, field, &captured, &request(Action::Focus, None)).is_err());
    shortcuts::release_keyboard(&mut world, replacement);
    world.get_mut::<Workspaces>(root).unwrap().canvas_id.push_str("-replacement");
    assert!(perform(&mut world, field, &captured, &request(Action::Focus, None)).is_err());
    assert_eq!(world.get::<EditableText>(field).unwrap().value(), "Keep me");
}

#[test]
fn disabled_controls_remain_describable_and_protected_fields_never_export_plain_text_runs() {
    let (mut world, _, field) = fixture("private API key");
    world.entity_mut(field).insert(bevy::ui::InteractionDisabled);
    assert!(context(&world, field, false).is_some());
    assert!(supported_actions(&world, field).is_empty());
    world.entity_mut(field).remove::<bevy::ui::InteractionDisabled>();
    super::super::secret(&mut world, field);
    let mut node = world.get::<AccessibilityNode>(field).unwrap().0.clone();
    let mut update = TreeUpdate { nodes: Vec::new(), tree: None, tree_id: TreeId::ROOT, focus: NodeId(1) };
    assert!(!super::super::text_nodes(&mut world, field, &mut node, &mut update, || NodeId(10)));
    assert!(update.nodes.is_empty());
    assert!(!supported_actions(&world, field).contains(&Action::SetTextSelection));
}

#[test]
fn text_runs_and_selection_come_from_the_existing_editor_layout() {
    let (mut world, _, field) = fixture("Café 世界");
    let mut node = world.get::<AccessibilityNode>(field).unwrap().0.clone();
    let mut update = TreeUpdate { nodes: Vec::new(), tree: None, tree_id: TreeId::ROOT, focus: NodeId(1) };
    let mut next = 10;
    assert!(super::super::text_nodes(&mut world, field, &mut node, &mut update, || { next += 1; NodeId(next) }));
    assert!(!update.nodes.is_empty());
    let text = update.nodes.iter().filter_map(|(_, node)| node.value()).collect::<String>();
    assert_eq!(text, "Café 世界");
    assert!(update.nodes.iter().all(|(_, node)| node.character_lengths().iter().map(|length| usize::from(*length)).sum::<usize>() == node.value().unwrap_or_default().len()));
}

#[test]
fn keyboard_reveal_uses_one_captured_intent_and_retires_it_on_focus_canvas_or_lock_loss() {
    let (mut world, root, field) = fixture("Keep typing");
    world.insert_resource(crate::host::magnification::Snapshot { available: true, ..default() });
    let mut accessible = accesskit::Node::new(Role::Button); accessible.set_label("Keep monitor arrangement");
    let button = world.spawn((Node::default(), AccessibilityNode(accessible), InheritedVisibility::VISIBLE, ChildOf(root))).id();
    world.resource_mut::<InputFocus>().set(button, FocusCause::Navigated);
    navigated(&mut world, button);
    assert_eq!(reveal_target(&mut world), Some(button));
    assert!(!owns_keyboard(&world));
    revealed(&mut world, field);
    assert_eq!(reveal_target(&mut world), Some(button));
    revealed(&mut world, button);
    assert_eq!(reveal_target(&mut world), None);
    navigated(&mut world, button);
    world.resource_mut::<InputFocus>().set(field, FocusCause::Navigated);
    assert_eq!(reveal_target(&mut world), None);
    request_reveal(&mut world, button, false);
    assert_eq!(reveal_target(&mut world), Some(button));
    world.get_mut::<Workspaces>(root).unwrap().canvas_id.push_str("-new-owner");
    assert_eq!(reveal_target(&mut world), None);
    navigated(&mut world, field);
    assert_eq!(reveal_target(&mut world), Some(field));
    world.insert_resource(crate::host::control::Status { available: true, active: true, locked: true, ..default() });
    assert_eq!(reveal_target(&mut world), None);
    assert_eq!(world.resource::<InputFocus>().get(), Some(field));
}

#[test]
fn accessibility_export_does_not_mark_text_changed_and_dirty_layout_retires_old_selection_actions() {
    let (mut world, _, field) = fixture("aé界 repeated words");
    let export = |world: &mut World| {
        let mut node = accesskit::Node::new(Role::TextInput);
        let mut update = TreeUpdate { nodes: Vec::new(), tree: None, tree_id: TreeId::ROOT, focus: NodeId(1) };
        let mut next = 100;
        assert!(super::super::text_nodes(world, field, &mut node, &mut update, || { next += 1; NodeId(next) }));
        node.text_selection().copied().unwrap()
    };
    let selection = export(&mut world);
    let captured = context(&world, field, true).unwrap();
    world.clear_trackers();
    export(&mut world);
    assert!(!world.get_entity(field).unwrap().get_ref::<EditableText>().unwrap().is_changed());
    assert!(context(&world, field, true).as_ref() == Some(&captured));
    world.get_mut::<EditableText>(field).unwrap().editor.set_width(Some(20.0));
    assert!(captured.describes(&world, field));
    let action = request(Action::SetTextSelection, Some(ActionData::SetTextSelection(selection)));
    assert!(perform(&mut world, field, &captured, &action).is_err());
    export(&mut world);
    assert!(context(&world, field, true).as_ref() != Some(&captured));
    assert_eq!(world.get::<EditableText>(field).unwrap().value(), "aé界 repeated words");
}

fn viewport(id: u64, workspace: u64, x: f64) -> crate::host::OutputViewport {
    let bounds = crate::host::Rectangle::new([x, 0.0], [800.0, 600.0]).unwrap();
    crate::host::OutputViewport { rotation: Default::default(), id: crate::host::OutputId(id), bounds, usable: Some(bounds), physical_size: [800, 600],
        raster: crate::host::raster::RasterMapping { origin: bounds.origin(), scale: [1.0; 2] }, workspace }
}

#[test]
fn visible_companion_controls_keep_their_output_owner_and_do_not_capture_the_other_output_keyboard() {
    let (mut world, root, field) = fixture("Companion editor");
    let mut second = world.get::<Workspaces>(root).unwrap().entries[0].clone();
    second.id = 2;
    world.get_mut::<Workspaces>(root).unwrap().entries.push(second);
    world.entity_mut(field).insert((
        crate::native::canvas::CanvasItem { position: bevy::math::DVec2::ZERO, size: Vec2::new(200.0, 80.0) },
        LayoutBox::new(Vec2::new(200.0, 80.0)), WorkspaceMember(2),
    ));
    let views = crate::native::output_views::OutputViews::new(vec![viewport(1, 1, 0.0), viewport(2, 2, 800.0)], crate::host::OutputId(1)).unwrap();
    world.insert_resource(views);
    let captured = context(&world, field, true).unwrap();
    assert_eq!(captured.output(), Some(crate::host::OutputId(2)));
    perform(&mut world, field, &captured, &request(Action::Focus, None)).unwrap();
    assert!(!owns_keyboard(&world));
    world.get_mut::<Workspaces>(root).unwrap().active = 2;
    world.resource_mut::<crate::native::output_views::OutputViews>().focused = crate::host::OutputId(2);
    assert!(context(&world, field, true) == Some(captured));
    assert!(owns_keyboard(&world));
    world.resource_mut::<crate::native::output_views::OutputViews>().outputs.retain(|output| output.id != crate::host::OutputId(2));
    assert!(context(&world, field, true).is_none());
    assert!(!owns_keyboard(&world));
}

#[test]
fn world_camera_and_item_motion_do_not_retarget_semantic_actions_but_final_mapping_changes_retire_them() {
    let (mut world, root, field) = fixture("Moving editor");
    world.entity_mut(root).insert(crate::native::canvas::CanvasView::default());
    world.entity_mut(field).insert((
        crate::native::canvas::CanvasItem { position: bevy::math::DVec2::ZERO, size: Vec2::new(200.0, 80.0) },
        LayoutBox::new(Vec2::new(200.0, 80.0)), WorkspaceMember(1),
    ));
    let captured = context(&world, field, true).unwrap();
    world.get_mut::<crate::native::canvas::CanvasView>(root).unwrap().center = bevy::math::DVec2::new(500.0, -400.0);
    world.get_mut::<crate::native::canvas::CanvasItem>(field).unwrap().position = bevy::math::DVec2::new(500.0, -400.0);
    perform(&mut world, field, &captured, &request(Action::Focus, None)).unwrap();
    assert!(owns_keyboard(&world));
    world.insert_resource(crate::host::mapping::Snapshot::new(1, []).unwrap());
    assert!(perform(&mut world, field, &captured, &request(Action::SetValue, Some(ActionData::Value("stale".into())))).is_err());
    assert_eq!(world.get::<EditableText>(field).unwrap().value(), "Moving editor");
}

#[test]
fn descriptions_of_presented_text_remain_valid_while_an_older_edit_request_is_rejected() {
    let (mut world, _, field) = fixture("Presented value");
    let captured = context(&world, field, true).unwrap();
    perform(&mut world, field, &captured, &request(Action::SetValue, Some(ActionData::Value("New value".into())))).unwrap();
    world.run_system_once(bevy::text::apply_text_edits).unwrap();
    assert!(captured.describes(&world, field));
    assert!(perform(&mut world, field, &captured, &request(Action::ReplaceSelectedText, Some(ActionData::Value("Late value".into())))).is_err());
    assert_eq!(world.get::<EditableText>(field).unwrap().value(), "New value");
    world.get_mut::<Node>(field).unwrap().display = Display::None;
    assert!(!captured.describes(&world, field));
}

#[test]
fn protecting_an_existing_field_retires_its_older_plain_text_description() {
    let (mut world, _, field) = fixture("Previously visible");
    let captured = context(&world, field, false).unwrap();
    super::super::secret(&mut world, field);
    assert!(!captured.describes(&world, field));
    assert!(context(&world, field, false).is_some());
    assert!(perform(&mut world, field, &captured, &request(Action::Focus, None)).is_err());
}
