use super::*;

#[test]
fn an_inactive_captured_window_moves_without_retargeting_focus_or_recreating_its_editor() {
    let (mut world, root, native, application) = fixture();
    destination(&mut world, root, WorkspaceLayout::Tiled);
    let id = world.get::<Managed>(application).unwrap().id;
    let original = model(&world, root, 1)
        .items()
        .iter()
        .find(|item| item.id == id)
        .unwrap()
        .clone();
    let editor = world
        .spawn((
            crate::native::sand::editable("An unsaved note"),
            ChildOf(application),
        ))
        .id();
    let overlay = world.spawn_empty().id();
    world
        .resource_mut::<bevy::input_focus::InputFocus>()
        .set(overlay, bevy::input_focus::FocusCause::Pressed);
    world.get_mut::<Workspaces>(root).unwrap().active = 2;
    assert!(prepare_captured(&mut world, root, 2, id, 1, None).is_err());
    let prepared = prepare_captured(&mut world, root, 1, id, 2, None).unwrap();
    prepared.commit(&mut world).unwrap().unwrap();
    assert_eq!(world.get::<Workspaces>(root).unwrap().active, 2);
    assert_eq!(
        world.resource::<bevy::input_focus::InputFocus>().get(),
        Some(overlay)
    );
    assert_eq!(world.get::<WorkspaceMember>(application).unwrap().0, 2);
    assert_eq!(world.get::<WorkspaceMember>(native).unwrap().0, 1);
    assert_eq!(
        world.get::<ApplicationWindow>(application).unwrap().surface,
        SurfaceId(7)
    );
    assert_eq!(
        model(&world, root, 2)
            .items()
            .iter()
            .find(|item| item.id == id),
        Some(&original)
    );
    assert_eq!(
        world
            .get::<bevy::text::EditableText>(editor)
            .unwrap()
            .value()
            .to_string(),
        "An unsaved note"
    );
}

#[test]
fn same_workspace_capture_reorders_rows_without_moving_the_wrong_focused_window() {
    let (mut world, root, native, application) = fixture();
    let id = world.get::<Managed>(native).unwrap().id;
    let anchor = world.get::<Managed>(application).unwrap().id;
    let before = model(&world, root, 1).clone();
    let input = world.resource::<bevy::input_focus::InputFocus>().get();
    let prepared = prepare_captured(
        &mut world,
        root,
        1,
        id,
        1,
        Some(DropSlot {
            anchor,
            side: crate::host::Direction::Down,
        }),
    )
    .unwrap();
    prepared.commit(&mut world).unwrap().unwrap();
    assert_eq!(
        model(&world, root, 1).tiled_rows(),
        vec![vec![anchor], vec![id]]
    );
    assert_eq!(model(&world, root, 1).items(), before.items());
    assert_eq!(model(&world, root, 1).focused(), before.focused());
    assert_eq!(
        world.resource::<bevy::input_focus::InputFocus>().get(),
        input
    );
    assert_eq!(world.get::<WorkspaceMember>(native).unwrap().0, 1);
    assert_eq!(world.get::<WorkspaceMember>(application).unwrap().0, 1);
    assert!(model(&world, root, 1).valid());
}

#[test]
fn captured_transient_moves_its_original_live_family_once() {
    let (mut world, root, native, application) = fixture();
    destination(&mut world, root, WorkspaceLayout::Free);
    world
        .entity_mut(native)
        .insert(TransientParent(application));
    crate::native::layout::engine::resolve(&mut world);
    let id = world.get::<Managed>(native).unwrap().id;
    let prepared = prepare_captured(&mut world, root, 1, id, 2, None).unwrap();
    prepared.commit(&mut world).unwrap().unwrap();
    for entity in [native, application] {
        assert_eq!(world.get::<WorkspaceMember>(entity).unwrap().0, 2);
    }
    assert_eq!(world.get::<TransientParent>(native).unwrap().0, application);
    assert!(model(&world, root, 1).items().is_empty());
    assert_eq!(model(&world, root, 2).items().len(), 2);
    assert_eq!(world.get::<Workspaces>(root).unwrap().active, 1);
}

#[test]
fn a_changed_active_context_or_closed_source_rejects_before_any_partial_transfer() {
    for close in [false, true] {
        let (mut world, root, _, application) = fixture();
        destination(&mut world, root, WorkspaceLayout::Free);
        let id = world.get::<Managed>(application).unwrap().id;
        let prepared = prepare_captured(&mut world, root, 1, id, 2, None).unwrap();
        let source = model(&world, root, 1).clone();
        let destination = model(&world, root, 2).clone();
        if close {
            world.despawn(application);
        } else {
            world.get_mut::<Workspaces>(root).unwrap().active = 2;
        }
        assert!(prepared.commit(&mut world).is_err());
        assert_eq!(model(&world, root, 1), &source);
        assert_eq!(model(&world, root, 2), &destination);
    }
}
