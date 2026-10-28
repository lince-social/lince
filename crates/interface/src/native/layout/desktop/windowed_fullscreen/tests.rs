use super::*;
use crate::native::actions::Action;

fn fixture() -> (World, Entity, Entity, Entity) {
    let (mut world, root, native, application) = super::super::tests::fixture();
    let (sender, _receiver) = std::sync::mpsc::sync_channel(4);
    world.insert_resource(HostRequests(sender));
    world.get_mut::<Workspaces>(root).unwrap().entries[0].desktop.set_mode(WorkspaceLayout::Tiled);
    super::super::super::engine::resolve(&mut world);
    focus_item(&mut world, application);
    (world, root, native, application)
}

#[test]
fn the_shared_command_toggles_protocol_intent_without_changing_camera_or_item_geometry() {
    let (mut world, root, native, application) = fixture();
    let view = *world.get::<CanvasView>(root).unwrap();
    let items = [native, application].map(|entity| { let item = world.get::<CanvasItem>(entity).unwrap(); (item.position, item.size) });
    let target = capture(&world, root, application).unwrap();
    for _ in 0..20 {
        DesktopAction::ToggleWindowedFullscreen.apply(&mut world, root);
        super::super::super::engine::resolve(&mut world);
        assert!(state(&world, &target).unwrap());
        assert_eq!(CommandContext::new(&world, root).windowed_fullscreen(), Some(true));
        assert_eq!([native, application].map(|entity| { let item = world.get::<CanvasItem>(entity).unwrap(); (item.position, item.size) }), items);
        let current = world.get::<CanvasView>(root).unwrap();
        assert_eq!((current.center, current.zoom), (view.center, view.zoom));
        DesktopAction::ToggleWindowedFullscreen.apply(&mut world, root);
        assert!(!state(&world, &target).unwrap());
    }
}

#[test]
fn a_properties_control_changes_its_captured_window_without_taking_focus_from_the_inspector() {
    let (mut world, root, native, application) = fixture();
    let target = capture(&world, root, application).unwrap();
    let uid = nucleus::new_uid("r");
    let inspector = world.spawn((Node::default(), WorkspaceMember(1), ChildOf(root), Metadata {
        placement: target.placement.0, uid: uid.clone(), title: "Original app".into(), app_id: "example.native".into(), live: true, state: "tiled".into(),
    })).id();
    let binding = RecordBinding { area: inspector, uid: uid.clone(), source: crate::native::protein_area::Source::Local };
    world.entity_mut(application).insert(RecordBinding { area: application, ..binding.clone() });
    panel(&mut world, inspector, &binding);
    let page = world.query_filtered::<Entity, With<Page>>().single(&world).unwrap();
    for entity in [root, inspector, page] {
        if let Some(mut visible) = world.get_mut::<InheritedVisibility>(entity) { *visible = InheritedVisibility::VISIBLE; }
    }
    focus_item(&mut world, native);
    let focused = world.get::<Workspaces>(root).unwrap().entries[0].desktop.focused();
    world.init_resource::<bevy::input_focus::InputFocus>();
    world.resource_mut::<bevy::input_focus::InputFocus>().set(inspector, bevy::input_focus::FocusCause::Pressed);
    Toggle.apply(&mut world, page);
    assert!(state(&world, &target).unwrap());
    assert_eq!(world.get::<Workspaces>(root).unwrap().entries[0].desktop.focused(), focused);
    assert_eq!(world.resource::<bevy::input_focus::InputFocus>().get(), Some(inspector));
    refresh(&mut world);
    let button = world.get::<Page>(page).unwrap().button.0;
    assert_eq!(world.get::<bevy::a11y::AccessibilityNode>(button).unwrap().toggled(), Some(accesskit::Toggled::True));
    world.get_mut::<Metadata>(inspector).unwrap().live = false;
    Toggle.apply(&mut world, page);
    assert!(world.get::<Page>(page).unwrap().message.contains("closed"));
    assert!(state(&world, &target).unwrap());
    refresh(&mut world);
    assert!(world.get::<bevy::ui::InteractionDisabled>(button).is_some());
}

#[test]
fn captured_controls_refuse_replaced_hidden_transient_canvas_and_sandbox_targets() {
    let (mut world, root, native, application) = fixture();
    let target = capture(&world, root, application).unwrap();
    assert!(capture(&world, root, native).is_err());
    world.entity_mut(application).insert(TransientParent(native));
    assert!(toggle(&mut world, &target).is_err());
    world.entity_mut(application).remove::<TransientParent>();
    world.get_mut::<Workspaces>(root).unwrap().canvas_id.push_str("-replacement");
    assert!(toggle(&mut world, &target).is_err());
    world.get_mut::<Workspaces>(root).unwrap().canvas_id = target.canvas.clone();
    world.get_mut::<ApplicationWindow>(application).unwrap().surface = SurfaceId(8);
    assert!(toggle(&mut world, &target).is_err());
    world.get_mut::<ApplicationWindow>(application).unwrap().surface = target.surface;
    world.get_mut::<Workspaces>(root).unwrap().entries[0].desktop.set_minimized(target.placement, true);
    assert!(toggle(&mut world, &target).is_err());
    world.get_mut::<Workspaces>(root).unwrap().entries[0].desktop.set_minimized(target.placement, false);
    world.remove_resource::<HostRequests>();
    assert!(toggle(&mut world, &target).is_err());
    assert!(CommandContext::new(&world, root).unavailable(DesktopAction::ToggleWindowedFullscreen).unwrap().contains("Park"));
    assert!(!world.get::<Workspaces>(root).unwrap().entries[0].desktop.windowed_fullscreen(target.placement));
}

#[test]
fn properties_availability_rejects_missing_record_binding_and_inactive_seat_then_recovers_the_same_target() {
    let (mut world, root, _, application) = fixture();
    let target = capture(&world, root, application).unwrap();
    let uid = nucleus::new_uid("r");
    let inspector = world.spawn((Node::default(), WorkspaceMember(1), ChildOf(root), Metadata {
        placement: target.placement.0, uid: uid.clone(), title: "Original app".into(), app_id: "example.native".into(), live: true, state: "tiled".into(),
    })).id();
    let binding = RecordBinding { area: inspector, uid, source: crate::native::protein_area::Source::Local };
    panel(&mut world, inspector, &binding);
    let page = world.query_filtered::<Entity, With<Page>>().single(&world).unwrap();
    assert!(source(&world, world.get::<Page>(page).unwrap()).unwrap_err().contains("Record association"));
    world.entity_mut(application).insert(RecordBinding { area: application, ..binding });
    assert!(source(&world, world.get::<Page>(page).unwrap()).is_ok());
    let activity = crate::host::input::InputActivity::default();
    world.insert_resource(activity.clone());
    activity.set_active(false);
    refresh(&mut world);
    let button = world.get::<Page>(page).unwrap().button.0;
    assert!(world.get::<bevy::ui::InteractionDisabled>(button).is_some());
    assert!(source(&world, world.get::<Page>(page).unwrap()).unwrap_err().contains("inactive"));
    activity.set_active(true);
    refresh(&mut world);
    assert!(world.get::<bevy::ui::InteractionDisabled>(button).is_none());
    world.get_mut::<RecordBinding>(application).unwrap().area = inspector;
    assert!(source(&world, world.get::<Page>(page).unwrap()).is_err());
    world.get_mut::<RecordBinding>(application).unwrap().area = application;
    assert!(source(&world, world.get::<Page>(page).unwrap()).is_ok());
    assert!(!state(&world, &target).unwrap());
}
