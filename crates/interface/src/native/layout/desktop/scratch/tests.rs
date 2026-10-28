use super::*;

pub(super) fn fixture() -> (World, Entity, Entity, Entity) {
    let (mut world, root, native, application) = super::super::tests::fixture();
    world.init_resource::<InputFocus>();
    world.init_resource::<Reveals>();
    world.insert_resource(crate::host::capture::PolicyGate::default());
    world.get_mut::<Workspaces>(root).unwrap().entries[0].desktop.set_mode(WorkspaceLayout::Tiled);
    super::super::super::engine::resolve(&mut world);
    focus_item(&mut world, native);
    (world, root, native, application)
}

fn other(world: &mut World, root: Entity) {
    let mut workspace = world.get::<Workspaces>(root).unwrap().entries[0].clone();
    workspace.id = 2;
    workspace.name = "Work".into();
    workspace.desktop = Model::new(WorkspaceLayout::Tiled);
    world.get_mut::<Workspaces>(root).unwrap().entries.push(workspace);
}

#[test]
fn scratch_commands_move_one_live_item_and_hide_its_family_without_client_minimization() {
    let (mut world, root, native, application) = fixture();
    let original = capture(&world, root, native).unwrap();
    checked(&mut world, root, DesktopAction::MarkScratch).unwrap();
    other(&mut world, root);
    let child = world.spawn((CanvasItem { position: bevy::math::DVec2::ZERO, size: Vec2::new(180.0, 120.0) },
        LayoutBox::new(Vec2::new(180.0, 120.0)), WorkspaceMember(1), TransientParent(native), ChildOf(root))).id();
    super::super::super::engine::resolve(&mut world);
    crate::native::workspace::switch_output(&mut world, root, 2);
    super::super::super::engine::resolve(&mut world);
    for _ in 0..20 {
        checked(&mut world, root, DesktopAction::ShowScratch).unwrap();
        super::super::super::engine::resolve(&mut world);
        assert_eq!(world.get::<WorkspaceMember>(native), Some(&WorkspaceMember(2)));
        assert_eq!(world.get::<WorkspaceMember>(child), Some(&WorkspaceMember(2)));
        assert_eq!(world.get::<LayoutBox>(native).unwrap().id, original.placement.0);
        assert!(focus::eligible(&world, root, 2, native));
        checked(&mut world, root, DesktopAction::HideScratch).unwrap();
        super::super::super::engine::resolve(&mut world);
        assert!(!focus::eligible(&world, root, 2, native));
        assert!(!focus::eligible(&world, root, 2, child));
        assert!(!crate::native::canvas_item::eligible(&world, root, native));
        assert!(crate::native::capture::excluded(&world, native));
        assert!(crate::native::capture::excluded(&world, child));
        let source = capture(&world, root, native).unwrap();
        assert!(!model(&world, &source).unwrap().minimized(source.placement));
        assert_eq!(world.get::<Workspaces>(root).unwrap().entries.iter().flat_map(|space| space.desktop.items()).filter(|item| item.id == source.placement).count(), 1);
    }
    let target = capture(&world, root, native).unwrap();
    let destination = destination(&world, root).unwrap();
    apply_target(&mut world, &target, DesktopAction::UnmarkScratch, &destination, None).unwrap();
    assert_eq!(world.get::<WorkspaceMember>(native), Some(&WorkspaceMember(1)));
    assert_eq!(world.get::<WorkspaceMember>(child), Some(&WorkspaceMember(1)));
    assert_eq!(world.get::<Workspaces>(root).unwrap().active, 2);
    assert_eq!(world.get::<ApplicationWindow>(application).unwrap().surface, SurfaceId(7));
}

#[test]
fn captured_commands_reject_replacement_and_lock_and_drag_without_mutation() {
    let (mut world, root, _, application) = fixture();
    focus_item(&mut world, application);
    checked(&mut world, root, DesktopAction::MarkScratch).unwrap();
    let target = capture(&world, root, application).unwrap();
    let destination = destination(&world, root).unwrap();
    let original = model(&world, &target).unwrap().clone();
    world.insert_resource(crate::host::control::Status { available: true, active: true, locked: true, ..default() });
    assert!(apply_target(&mut world, &target, DesktopAction::ShowScratch, &destination, None).unwrap_err().contains("locked"));
    world.resource_mut::<crate::host::control::Status>().locked = false;
    world.insert_resource(crate::host::input::DragActive(true));
    assert!(apply_target(&mut world, &target, DesktopAction::HideScratch, &destination, None).unwrap_err().contains("drag"));
    world.resource_mut::<crate::host::input::DragActive>().0 = false;
    world.get_mut::<ApplicationWindow>(application).unwrap().surface = SurfaceId(999);
    assert!(apply_target(&mut world, &target, DesktopAction::UnmarkScratch, &destination, None).is_err());
    assert_eq!(world.get::<Workspaces>(root).unwrap().entries[0].desktop, original);
}

#[test]
fn captured_scratch_properties_require_the_same_local_record_and_active_input_before_revealing_or_moving() {
    let (mut world, root, _, application) = fixture();
    focus_item(&mut world, application);
    checked(&mut world, root, DesktopAction::MarkScratch).unwrap();
    let mut target = capture(&world, root, application).unwrap();
    let uid = nucleus::new_uid("r");
    let inspector = world.spawn((Metadata { placement: target.placement.0, uid: uid.clone(), title: "Original scratch".into(), app_id: "example.native".into(), live: true, state: "tiled".into() }, Node::default(), ChildOf(root))).id();
    target.inspector = Some((inspector, uid.clone()));
    let destination = destination(&world, root).unwrap();
    let before = world.get::<Workspaces>(root).unwrap().entries[0].desktop.clone();
    assert!(apply_target(&mut world, &target, DesktopAction::HideScratch, &destination, None).unwrap_err().contains("Record association"));
    world.entity_mut(application).insert(RecordBinding { area: inspector, uid, source: crate::native::protein_area::Source::Local });
    assert!(apply_target(&mut world, &target, DesktopAction::HideScratch, &destination, None).is_err());
    world.get_mut::<RecordBinding>(application).unwrap().area = application;
    let activity = crate::host::input::InputActivity::default();
    world.insert_resource(activity.clone());
    activity.set_active(false);
    assert!(apply_target(&mut world, &target, DesktopAction::HideScratch, &destination, None).unwrap_err().contains("inactive"));
    assert!(checked(&mut world, root, DesktopAction::ShowScratch).unwrap_err().contains("inactive"));
    assert_eq!(world.get::<Workspaces>(root).unwrap().entries[0].desktop, before);
    activity.set_active(true);
    apply_target(&mut world, &target, DesktopAction::HideScratch, &destination, None).unwrap();
    assert!(model(&world, &target).unwrap().scratch_hidden(target.placement));
}

#[test]
fn lost_home_uses_the_current_workspace_without_recreating_the_home() {
    let (mut world, root, native, _) = fixture();
    checked(&mut world, root, DesktopAction::MarkScratch).unwrap();
    other(&mut world, root);
    crate::native::workspace::switch_output(&mut world, root, 2);
    checked(&mut world, root, DesktopAction::ShowScratch).unwrap();
    world.get_mut::<Workspaces>(root).unwrap().entries.retain(|space| space.id != 1);
    let target = capture(&world, root, native).unwrap();
    let destination = destination(&world, root).unwrap();
    apply_target(&mut world, &target, DesktopAction::UnmarkScratch, &destination, None).unwrap();
    assert!(world.get::<Workspaces>(root).unwrap().entries.iter().all(|space| space.id != 1));
    assert_eq!(world.get::<WorkspaceMember>(native), Some(&WorkspaceMember(2)));
    assert!(model(&world, &target).unwrap().scratch(target.placement).is_none());
}

#[test]
fn commands_are_discoverable_unbound_and_nonrepeating_for_shared_desktop_use() {
    use crate::native::shortcuts::{Shortcut, Scope};
    for action in [Shortcut::ScratchMark, Shortcut::ScratchShow, Shortcut::ScratchHide, Shortcut::ScratchUnmark] {
        assert_eq!(action.default_binding(), "Unbound");
        assert_eq!(action.scope(), Scope::Desktop);
        assert!(!action.repeats());
        assert!(action.desktop_action().is_some());
        assert_eq!(Shortcut::from_id(action.id()), Some(action));
    }
}
