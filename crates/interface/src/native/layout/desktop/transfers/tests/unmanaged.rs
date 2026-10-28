use super::*;

fn fresh() -> (World, Entity, Entity, Entity) {
    let (mut world, root, native, neighbour) = super::super::super::tests::fixture();
    world.entity_mut(neighbour).remove::<ApplicationWindow>();
    world.init_resource::<bevy::input_focus::InputFocus>();
    world.get_mut::<Workspaces>(root).unwrap().entries[0].desktop =
        Model::new(WorkspaceLayout::Free);
    world.get_mut::<CanvasItem>(native).unwrap().position = bevy::math::DVec2::new(617.0, -223.0);
    crate::native::layout::engine::resolve(&mut world);
    assert!(world.get::<Managed>(native).is_none());
    assert!(model(&world, root, 1).items().is_empty());
    (world, root, native, neighbour)
}

#[test]
fn a_first_free_canvas_transfer_stages_registration_without_focus_activation_or_draft_loss() {
    for active in [1, 2] {
        let (mut world, root, native, neighbour) = fresh();
        destination(&mut world, root, WorkspaceLayout::Free);
        world.get_mut::<Workspaces>(root).unwrap().active = active;
        let input = world.spawn_empty().id();
        world
            .resource_mut::<bevy::input_focus::InputFocus>()
            .set(input, bevy::input_focus::FocusCause::Pressed);
        let editor = world
            .spawn((
                crate::native::sand::editable("Unsaved fresh Sand"),
                ChildOf(native),
            ))
            .id();
        let layout = *world.get::<LayoutBox>(native).unwrap();
        let geometry = *world.get::<CanvasItem>(native).unwrap();
        let view = *world.get::<CanvasView>(root).unwrap();
        let source = model(&world, root, 1).clone();
        let target = model(&world, root, 2).clone();
        let prepared =
            prepare_captured(&mut world, root, 1, PlacementId(layout.id), 2, None).unwrap();
        assert_eq!(model(&world, root, 1), &source);
        assert_eq!(model(&world, root, 2), &target);
        assert!(world.get::<Managed>(native).is_none());
        prepared.commit(&mut world).unwrap().unwrap();
        assert_eq!(world.get::<Workspaces>(root).unwrap().active, active);
        assert_eq!(world.get::<WorkspaceMember>(native).unwrap().0, 2);
        assert_eq!(world.get::<WorkspaceMember>(neighbour).unwrap().0, 1);
        assert_eq!(
            world.resource::<bevy::input_focus::InputFocus>().get(),
            Some(input)
        );
        assert!(*world.get::<CanvasView>(root).unwrap() == view);
        assert_eq!(*world.get::<LayoutBox>(native).unwrap(), layout);
        assert_eq!(
            world.get::<CanvasItem>(native).unwrap().position,
            geometry.position
        );
        assert_eq!(world.get::<CanvasItem>(native).unwrap().size, geometry.size);
        assert!(model(&world, root, 1).items().is_empty());
        let item = &model(&world, root, 2).items()[0];
        assert_eq!(item.id, PlacementId(layout.id));
        assert_eq!(item.kind, ItemKind::Native);
        assert_eq!(item.rules, layout.rules);
        assert_eq!(item.free.origin, [467.0, -323.0]);
        assert_eq!(item.free.size, [300.0, 200.0]);
        assert!(!item.floating);
        assert_eq!(
            world
                .get::<bevy::text::EditableText>(editor)
                .unwrap()
                .value()
                .to_string(),
            "Unsaved fresh Sand"
        );
    }
}

#[test]
fn a_fresh_free_item_round_trips_through_an_inactive_tiled_workspace_with_its_authored_home() {
    let (mut world, root, native, neighbour) = fresh();
    destination(&mut world, root, WorkspaceLayout::Tiled);
    let id = PlacementId(world.get::<LayoutBox>(native).unwrap().id);
    let home = *world.get::<CanvasItem>(native).unwrap();
    prepare_captured(&mut world, root, 1, id, 2, None)
        .unwrap()
        .commit(&mut world)
        .unwrap();
    assert_eq!(model(&world, root, 2).tiled_rows(), vec![vec![id]]);
    assert_eq!(
        model(&world, root, 2).items()[0].free.origin,
        [467.0, -323.0]
    );
    assert_eq!(world.get::<Workspaces>(root).unwrap().active, 1);
    prepare_captured(&mut world, root, 2, id, 1, None)
        .unwrap()
        .commit(&mut world)
        .unwrap();
    crate::native::layout::engine::resolve(&mut world);
    assert_eq!(
        world.get::<CanvasItem>(native).unwrap().position,
        home.position
    );
    assert_eq!(world.get::<CanvasItem>(native).unwrap().size, home.size);
    assert_eq!(world.get::<WorkspaceMember>(native).unwrap().0, 1);
    assert_eq!(world.get::<WorkspaceMember>(neighbour).unwrap().0, 1);
    assert!(model(&world, root, 2).items().is_empty());
    assert!(model(&world, root, 1).valid());
}

#[test]
fn an_unregistered_transient_moves_its_live_family_and_rejects_changed_captured_geometry() {
    for changed in [None, Some(false), Some(true)] {
        let (mut world, root, parent, child) = fresh();
        world.entity_mut(child).insert(TransientParent(parent));
        destination(&mut world, root, WorkspaceLayout::Free);
        let id = PlacementId(world.get::<LayoutBox>(child).unwrap().id);
        let prepared = prepare_captured(&mut world, root, 1, id, 2, None).unwrap();
        assert!(model(&world, root, 1).items().is_empty());
        assert!(model(&world, root, 2).items().is_empty());
        if let Some(change_child) = changed {
            world
                .get_mut::<CanvasItem>(if change_child { child } else { parent })
                .unwrap()
                .position
                .x += 1.0;
            assert!(prepared.commit(&mut world).is_err());
            assert!(model(&world, root, 1).items().is_empty());
            assert!(model(&world, root, 2).items().is_empty());
            for entity in [parent, child] {
                assert_eq!(world.get::<WorkspaceMember>(entity).unwrap().0, 1);
            }
        } else {
            prepared.commit(&mut world).unwrap().unwrap();
            for entity in [parent, child] {
                assert_eq!(world.get::<WorkspaceMember>(entity).unwrap().0, 2);
            }
            assert_eq!(world.get::<TransientParent>(child).unwrap().0, parent);
            assert!(
                model(&world, root, 2)
                    .items()
                    .iter()
                    .find(|item| item.id == id)
                    .unwrap()
                    .floating
            );
            assert_eq!(model(&world, root, 2).items().len(), 2);
        }
    }
}

#[test]
fn invalid_duplicate_or_unmanaged_application_sources_publish_no_registration() {
    for invalid in 0..3 {
        let (mut world, root, native, neighbour) = fresh();
        destination(&mut world, root, WorkspaceLayout::Free);
        let id = PlacementId(world.get::<LayoutBox>(native).unwrap().id);
        match invalid {
            0 => world.get_mut::<CanvasItem>(native).unwrap().size.x = f32::MAX,
            1 => world.get_mut::<LayoutBox>(neighbour).unwrap().id = id.0,
            _ => {
                world.entity_mut(native).insert(ApplicationWindow {
                    surface: SurfaceId(42),
                    content_size: [300, 200],
                });
            }
        }
        assert!(prepare_captured(&mut world, root, 1, id, 2, None).is_err());
        assert!(model(&world, root, 1).items().is_empty());
        assert!(model(&world, root, 2).items().is_empty());
        assert_eq!(world.get::<WorkspaceMember>(native).unwrap().0, 1);
        assert!(world.get::<Managed>(native).is_none());
    }
}
