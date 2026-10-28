use super::*;

pub(super) fn fixture() -> (World, Entity, Entity, Entity, Entity) {
    let (mut world, root, first, second) = super::super::tests::fixture();
    let third = world.spawn((
        CanvasItem { position: bevy::math::DVec2::new(700.0, 0.0), size: Vec2::new(240.0, 160.0) },
        LayoutBox::new(Vec2::new(240.0, 160.0)), WorkspaceMember(1), ChildOf(root), Name::new("Third item"),
    )).id();
    world.get_mut::<Workspaces>(root).unwrap().entries[0].desktop.set_mode(WorkspaceLayout::Tiled);
    world.init_resource::<bevy::input_focus::InputFocus>();
    world.insert_resource(crate::host::capture::PolicyGate::default());
    super::super::super::engine::resolve(&mut world);
    focus_item(&mut world, first);
    (world, root, first, second, third)
}

pub(super) fn visible(world: &mut World, entity: Entity) {
    let mut cursor = Some(entity);
    for _ in 0..crate::native::layout::MAX_DEPTH {
        let Some(entity) = cursor else { return; };
        cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
        world.entity_mut(entity).insert(InheritedVisibility::VISIBLE);
    }
}

fn group(world: &mut World, root: Entity, first: Entity, second: Entity) {
    let first = world.get::<Managed>(first).unwrap().id;
    let second = world.get::<Managed>(second).unwrap().id;
    world.get_mut::<Workspaces>(root).unwrap().entries[0].desktop.create_compact_group(first, &[second]).unwrap();
    super::super::super::engine::resolve(world);
}

#[test]
fn shared_commands_select_reorder_detach_and_ungroup_without_losing_native_or_application_identity() {
    let (mut world, root, first, second, third) = fixture();
    let first_target = capture(&world, root, first, false).unwrap();
    let second_target = capture(&world, root, second, false).unwrap();
    let original = world.get::<Workspaces>(root).unwrap().entries[0].desktop.items().to_vec();
    group(&mut world, root, first, second);
    let third_size = world.get::<CanvasItem>(third).unwrap().size;
    let revision = world.resource::<crate::host::capture::PolicyGate>().read().unwrap().revision();
    for _ in 0..20 {
        checked(&mut world, root, DesktopAction::NextCompactMember).unwrap();
        assert_eq!(world.get::<Workspaces>(root).unwrap().entries[0].desktop.focused(), Some(second_target.placement));
        assert!(!focus::eligible(&world, root, 1, first));
        checked(&mut world, root, DesktopAction::PreviousCompactMember).unwrap();
        assert_eq!(world.get::<Workspaces>(root).unwrap().entries[0].desktop.focused(), Some(first_target.placement));
        assert!(!focus::eligible(&world, root, 1, second));
    }
    assert!(world.resource::<crate::host::capture::PolicyGate>().read().unwrap().revision() > revision);
    checked(&mut world, root, DesktopAction::MoveCompactMember(Direction::Right)).unwrap();
    let source = model(&world, &first_target, false).unwrap();
    assert_eq!(source.compact_group(first_target.placement).unwrap().members, [second_target.placement, first_target.placement]);
    checked(&mut world, root, DesktopAction::DetachCompactMember).unwrap();
    assert!(world.get::<Workspaces>(root).unwrap().entries[0].desktop.compact_groups().is_empty());
    assert_eq!(world.get::<Workspaces>(root).unwrap().entries[0].desktop.items(), original);
    assert_eq!(world.get::<CanvasItem>(third).unwrap().size, third_size);
    assert_eq!(world.get::<ApplicationWindow>(second).unwrap().surface, second_target.surface.unwrap());
}

#[test]
fn dragging_blocks_group_changes_and_explicit_activation_reveals_a_live_hidden_member() {
    let (mut world, root, first, second, _) = fixture();
    group(&mut world, root, first, second);
    let before = world.get::<Workspaces>(root).unwrap().entries[0].desktop.clone();
    world.insert_resource(crate::host::input::DragActive(true));
    assert!(checked(&mut world, root, DesktopAction::NextCompactMember).unwrap_err().contains("drag"));
    assert!(activate(&mut world, root, second).unwrap_err().contains("drag"));
    assert_eq!(world.get::<Workspaces>(root).unwrap().entries[0].desktop, before);
    world.insert_resource(crate::host::input::DragActive(false));
    activate(&mut world, root, second).unwrap();
    assert!(focus::eligible(&world, root, 1, second));
    assert!(!focus::eligible(&world, root, 1, first));
    let unmanaged = world.spawn_empty().id();
    assert!(activate(&mut world, root, unmanaged).is_ok());
}

#[test]
fn captured_properties_refuse_changed_surface_canvas_and_closed_record_associations() {
    let (mut world, root, first, second, _) = fixture();
    group(&mut world, root, second, first);
    focus_item(&mut world, second);
    let mut target = capture(&world, root, second, false).unwrap();
    let uid = nucleus::new_uid("r");
    let inspector = world.spawn((Metadata { placement: target.placement.0, uid: uid.clone(), title: "Captured".into(), app_id: "example.native".into(), live: true, state: "tiled".into() }, Node::default(), ChildOf(root))).id();
    world.entity_mut(second).insert(RecordBinding { area: second, uid: uid.clone(), source: crate::native::protein_area::Source::Local });
    target.inspector = Some((inspector, uid));
    let before = world.get::<Workspaces>(root).unwrap().entries[0].desktop.clone();
    world.get_mut::<Metadata>(inspector).unwrap().live = false;
    assert!(apply_target(&mut world, &target, DesktopAction::DissolveCompactGroup).is_err());
    world.get_mut::<Metadata>(inspector).unwrap().live = true;
    world.get_mut::<ApplicationWindow>(second).unwrap().surface = SurfaceId(999);
    assert!(apply_target(&mut world, &target, DesktopAction::DetachCompactMember).is_err());
    world.get_mut::<ApplicationWindow>(second).unwrap().surface = target.surface.unwrap();
    world.get_mut::<Workspaces>(root).unwrap().canvas_id.push_str("-replacement");
    assert!(apply_target(&mut world, &target, DesktopAction::NextCompactMember).is_err());
    assert_eq!(world.get::<Workspaces>(root).unwrap().entries[0].desktop, before);
}

#[test]
fn compact_properties_and_selection_refuse_inactive_seats_and_missing_or_replaced_local_record_bindings() {
    let (mut world, root, first, second, _) = fixture();
    group(&mut world, root, second, first);
    let mut target = capture(&world, root, second, false).unwrap();
    let uid = nucleus::new_uid("r");
    let inspector = world.spawn((Metadata { placement: target.placement.0, uid: uid.clone(), title: "Captured".into(), app_id: "example.native".into(), live: true, state: "tiled".into() }, Node::default(), ChildOf(root))).id();
    target.inspector = Some((inspector, uid.clone()));
    let before = world.get::<Workspaces>(root).unwrap().entries[0].desktop.clone();
    assert!(apply_target(&mut world, &target, DesktopAction::DissolveCompactGroup).unwrap_err().contains("Record association"));
    world.entity_mut(second).insert(RecordBinding { area: second, uid, source: crate::native::protein_area::Source::Local });
    assert!(model(&world, &target, false).is_ok());
    world.get_mut::<RecordBinding>(second).unwrap().area = inspector;
    assert!(model(&world, &target, false).is_err());
    world.get_mut::<RecordBinding>(second).unwrap().area = second;
    let activity = crate::host::input::InputActivity::default();
    world.insert_resource(activity.clone());
    activity.set_active(false);
    assert!(apply_target(&mut world, &target, DesktopAction::NextCompactMember).unwrap_err().contains("inactive"));
    assert!(activate(&mut world, root, first).is_err());
    assert_eq!(world.get::<Workspaces>(root).unwrap().entries[0].desktop, before);
    activity.set_active(true);
    assert!(model(&world, &target, false).is_ok());
    assert_eq!(world.get::<Workspaces>(root).unwrap().entries[0].desktop, before);
}

#[test]
fn strip_labels_keep_private_member_provenance_and_free_mode_retires_the_selector() {
    let (mut world, root, first, _, third) = fixture();
    group(&mut world, root, first, third);
    world.get_mut::<LayoutBox>(third).unwrap().capture_excluded = true;
    strip::refresh(&mut world);
    let buttons = world.query::<(Entity, &Name)>().iter(&world).filter(|(_, name)| name.as_str().starts_with("compact.member.")).map(|(entity, _)| entity).collect::<Vec<_>>();
    assert_eq!(buttons.len(), 2);
    let private = buttons.iter().copied().find(|entity| world.get::<crate::native::topology::presentation::VisualOwner>(*entity).is_some_and(|owner| owner.0 == third)).unwrap();
    assert!(crate::native::capture::excluded(&world, private));
    world.get_mut::<Workspaces>(root).unwrap().entries[0].desktop.set_mode(WorkspaceLayout::Free);
    strip::refresh(&mut world);
    assert!(buttons.iter().all(|entity| world.get_entity(*entity).is_err()));
    assert_eq!(world.get::<Workspaces>(root).unwrap().entries[0].desktop.compact_groups().len(), 1);
}

#[test]
fn compact_commands_are_registered_discoverable_and_unbound_without_changing_existing_defaults() {
    use crate::native::shortcuts::{Settings, Shortcut};
    let settings = Settings::default();
    for command in [Shortcut::CompactCreate, Shortcut::CompactAdd, Shortcut::CompactNext, Shortcut::CompactPrevious,
        Shortcut::CompactEarlier, Shortcut::CompactLater, Shortcut::CompactDetach, Shortcut::CompactDissolve]
    {
        assert_eq!(settings.text(command), "Unbound");
        assert!(!command.repeats());
        assert_eq!(Shortcut::from_id(command.id()), Some(command));
        assert_eq!(Shortcut::from_operation(command.operation_path().unwrap()), Some(command));
    }
    assert_eq!(settings.text(Shortcut::Tiling), "Mod+T");
    assert_eq!(settings.text(Shortcut::Float), "Mod+Shift+T");
}
