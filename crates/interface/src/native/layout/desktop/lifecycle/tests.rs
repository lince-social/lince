use super::*;
use bevy::input_focus::InputFocus;

fn fixture(mode: WorkspaceLayout) -> (World, Entity, Entity, Entity) {
    let (mut world, root, parent, child) = super::super::tests::fixture();
    world.init_resource::<InputFocus>();
    world.insert_resource(DefaultLayout(mode));
    world.get_mut::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .set_mode(mode);
    world.entity_mut(child).insert(TransientParent(parent));
    super::super::super::engine::resolve(&mut world);
    (world, root, parent, child)
}

fn focused(world: &World, root: Entity) -> Option<PlacementId> {
    world.get::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .focused()
}

fn application(world: &mut World, root: Entity) -> Entity {
    world
        .spawn((
            CanvasItem {
                position: bevy::math::DVec2::new(800.0, 500.0),
                size: Vec2::new(300.0, 200.0),
            },
            LayoutBox::new(Vec2::new(300.0, 200.0)),
            ApplicationWindow {
                surface: SurfaceId(8),
                content_size: [300, 200],
            },
            WorkspaceMember(1),
            ChildOf(root),
        ))
        .id()
}

#[test]
fn background_mapping_preserves_focus_editor_camera_and_explicit_unfocus() {
    for mode in [WorkspaceLayout::Free, WorkspaceLayout::Tiled] {
        for unfocused in [false, true] {
            let (mut world, root, parent, _) = fixture(mode);
            focus::select(&mut world, root, 1, parent, true);
            if unfocused {
                focus::apply(&mut world, root, DesktopAction::Unfocus);
            }
            let before_focus = focused(&world, root);
            let editor = world.spawn(ChildOf(parent)).id();
            if !unfocused {
                world
                    .resource_mut::<InputFocus>()
                    .set(editor, bevy::input_focus::FocusCause::Navigated);
            }
            let before_input = world.resource::<InputFocus>().get();
            let before_view = *world.get::<CanvasView>(root).unwrap();
            let before_size = world.get::<CanvasItem>(parent).unwrap().size;
            let background = application(&mut world, root);
            world
                .entity_mut(background)
                .insert(crate::native::castle::Pending);
            for mapped in [false, true] {
                if mapped {
                    world
                        .entity_mut(background)
                        .remove::<crate::native::castle::Pending>();
                }
                super::super::super::engine::resolve(&mut world);
                assert_eq!(focused(&world, root), before_focus);
                assert_eq!(world.resource::<InputFocus>().get(), before_input);
                assert_eq!(
                    world.get::<CanvasView>(root).unwrap().center,
                    before_view.center
                );
                assert_eq!(
                    world.get::<CanvasView>(root).unwrap().zoom,
                    before_view.zoom
                );
                assert_eq!(world.get::<CanvasItem>(parent).unwrap().size, before_size);
            }
        }
    }
}

#[test]
fn first_application_does_not_create_focus_and_native_creation_still_does() {
    let (mut world, root, native, child) = fixture(WorkspaceLayout::Free);
    world.despawn(child);
    world.despawn(native);
    super::super::super::engine::resolve(&mut world);
    let first = application(&mut world, root);
    super::super::super::engine::resolve(&mut world);
    assert_eq!(focused(&world, root), None);
    assert!(!focus::following(&world, root));
    let native = world
        .spawn((
            CanvasItem {
                position: bevy::math::DVec2::ZERO,
                size: Vec2::new(100.0, 100.0),
            },
            LayoutBox::new(Vec2::new(100.0, 100.0)),
            WorkspaceMember(1),
            ChildOf(root),
        ))
        .id();
    super::super::super::engine::resolve(&mut world);
    assert_eq!(
        focused(&world, root),
        Some(world.get::<Managed>(native).unwrap().id)
    );
    assert!(world.get::<Managed>(first).is_some());
}

#[test]
fn actual_dialog_unmap_returns_to_parent_without_starting_unrequested_follow() {
    for mode in [WorkspaceLayout::Free, WorkspaceLayout::Tiled] {
        for follow in [false, true] {
            let (mut world, root, parent, child) = fixture(mode);
            let neighbor = application(&mut world, root);
            super::super::super::engine::resolve(&mut world);
            focus::select(&mut world, root, 1, child, follow);
            let (sender, receiver) = std::sync::mpsc::sync_channel(1);
            world.insert_resource(HostRequests(sender));
            assert!(request_close(&world, child));
            assert_eq!(
                receiver.try_recv().unwrap(),
                crate::host::HostRequest::Close(SurfaceId(7))
            );
            assert_eq!(
                focused(&world, root),
                Some(world.get::<Managed>(child).unwrap().id)
            );
            assert!(before_unmap(&mut world, child));
            world.despawn(child);
            super::super::super::engine::resolve(&mut world);
            assert_eq!(
                focused(&world, root),
                Some(world.get::<Managed>(parent).unwrap().id)
            );
            assert_eq!(world.resource::<InputFocus>().get(), Some(parent));
            assert_eq!(
                focus::following(&world, root),
                follow && mode == WorkspaceLayout::Free
            );
            assert!(world.get::<Managed>(neighbor).is_some());
        }
    }
}

#[test]
fn closing_background_or_invalid_parent_dialog_never_retargets_focus() {
    for invalid in 0..5 {
        let (mut world, root, parent, child) = fixture(WorkspaceLayout::Free);
        focus::select(
            &mut world,
            root,
            1,
            if invalid == 0 { parent } else { child },
            false,
        );
        match invalid {
            1 => {
                world.despawn(parent);
            }
            2 => {
                let id = world.get::<Managed>(parent).unwrap().id;
                world.get_mut::<Workspaces>(root).unwrap().entries[0]
                    .desktop
                    .set_minimized(id, true);
            }
            3 => {
                world.entity_mut(parent).insert(WorkspaceMember(2));
            }
            4 => {
                world.entity_mut(parent).insert(TransientParent(child));
            }
            _ => {}
        }
        let before = focused(&world, root);
        assert!(!before_unmap(&mut world, child));
        assert_eq!(focused(&world, root), before);
    }
}

#[test]
fn closing_a_focused_dialog_preserves_its_parents_fullscreen() {
    let (mut world, root, parent, child) = fixture(WorkspaceLayout::Free);
    let id = world.get::<Managed>(parent).unwrap().id;
    world.get_mut::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .request_state(
            id,
            crate::host::WindowRequest {
                fullscreen: Some(true),
                ..default()
            },
        );
    focus::select(&mut world, root, 1, child, false);
    assert_eq!(
        world.get::<Workspaces>(root).unwrap().entries[0]
            .desktop
            .fullscreen(),
        Some(id)
    );
    assert!(before_unmap(&mut world, child));
    world.despawn(child);
    super::super::super::engine::resolve(&mut world);
    assert_eq!(focused(&world, root), Some(id));
    assert_eq!(
        world.get::<Workspaces>(root).unwrap().entries[0]
            .desktop
            .fullscreen(),
        Some(id)
    );
}

#[test]
fn asynchronous_dialog_close_preserves_an_active_modal_editor() {
    let (mut world, root, parent, child) = fixture(WorkspaceLayout::Free);
    focus::select(&mut world, root, 1, child, false);
    let overlay = world.spawn(ChildOf(root)).id();
    let editor = world.spawn(ChildOf(overlay)).id();
    world.insert_resource(crate::native::shortcuts::KeyboardModalOwner(Some(overlay)));
    world
        .resource_mut::<InputFocus>()
        .set(editor, bevy::input_focus::FocusCause::Navigated);
    assert!(before_unmap(&mut world, child));
    assert_eq!(
        focused(&world, root),
        Some(world.get::<Managed>(parent).unwrap().id)
    );
    assert_eq!(world.resource::<InputFocus>().get(), Some(editor));
}
