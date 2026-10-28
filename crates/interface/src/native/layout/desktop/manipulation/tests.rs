use super::*;

fn fixture(mode: WorkspaceLayout) -> (World, Entity, Entity, Entity) {
    let (mut world, root, native, application) = super::super::tests::fixture();
    world.get_mut::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .set_mode(mode);
    super::super::resolve(&mut world);
    focus::ensure(&mut world, root);
    world.init_resource::<bevy::input_focus::InputFocus>();
    (world, root, native, application)
}

fn layout(world: &World, root: Entity) -> Workspace {
    world.get::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .clone()
}

fn item(world: &World, target: Entity) -> CanvasItem {
    *world.get::<CanvasItem>(target).unwrap()
}

fn same(actual: CanvasItem, expected: CanvasItem) {
    assert_eq!(actual.position, expected.position);
    assert_eq!(actual.size, expected.size);
}

#[test]
fn live_preview_is_transient_holds_physics_and_cancel_restores_follow_and_input() {
    let (mut world, root, native, _) = fixture(WorkspaceLayout::Free);
    focus::select(&mut world, root, 1, native, true);
    let editor = world
        .spawn((
            crate::native::sand::editable("Keep my draft"),
            ChildOf(native),
        ))
        .id();
    world
        .resource_mut::<bevy::input_focus::InputFocus>()
        .set(editor, bevy::input_focus::FocusCause::Pressed);
    let original = item(&world, native);
    let saved = layout(&world, root);
    let view = *world.get::<CanvasView>(root).unwrap();
    assert!(begin(
        &mut world,
        root,
        native,
        Manipulation::Move,
        [0.5; 2]
    ));
    assert!(crate::native::physics::held(&world, native));
    assert!(!focus::following(&world, root));
    assert!(propose(&mut world, [120.0, -30.0], None));
    same(item(&world, native), original);
    assert_eq!(layout(&world, root), saved);
    let preview = crate::native::canvas::presented(&world, native).unwrap();
    assert_eq!(
        preview.position,
        original.position + bevy::math::DVec2::new(120.0, -30.0)
    );
    same(*world.get::<CanvasItem>(native).unwrap(), original);
    cancel(&mut world);
    assert!(!active(&world));
    assert!(world.get::<ItemPreview>(native).is_none());
    assert_eq!(layout(&world, root), saved);
    assert!(focus::following(&world, root));
    assert_eq!(
        world.resource::<bevy::input_focus::InputFocus>().get(),
        Some(editor)
    );
    assert_eq!(world.get::<CanvasView>(root).unwrap().center, view.center);
    assert_eq!(
        world
            .get::<bevy::text::EditableText>(editor)
            .unwrap()
            .value()
            .to_string(),
        "Keep my draft"
    );
}

#[test]
fn commit_publishes_once_and_preserves_concurrent_unrelated_geometry() {
    let (mut world, root, native, application) = fixture(WorkspaceLayout::Free);
    let original = item(&world, native);
    assert!(begin(
        &mut world,
        root,
        native,
        Manipulation::Move,
        [0.5; 2]
    ));
    assert!(propose(&mut world, [50.0, 40.0], None));
    world.get_mut::<CanvasItem>(application).unwrap().position.x += 70.0;
    let unrelated = item(&world, application);
    assert!(commit(&mut world));
    assert!(!commit(&mut world));
    assert!(!active(&world));
    assert!(world.get::<ItemPreview>(native).is_none());
    assert_eq!(
        item(&world, native).position,
        original.position + bevy::math::DVec2::new(50.0, 40.0)
    );
    same(item(&world, application), unrelated);
    assert_eq!(
        layout(&world, root).focused(),
        Some(world.get::<Managed>(native).unwrap().id)
    );
    super::super::resolve(&mut world);
    assert_eq!(
        item(&world, native).position,
        original.position + bevy::math::DVec2::new(50.0, 40.0)
    );
}

#[test]
fn resize_commits_rules_and_size_without_an_authored_preview_write() {
    let (mut world, root, native, _) = fixture(WorkspaceLayout::Free);
    let original = item(&world, native);
    let rules = world.get::<LayoutBox>(native).unwrap().rules;
    assert!(begin(
        &mut world,
        root,
        native,
        Manipulation::Resize,
        [1.0; 2]
    ));
    assert!(propose(&mut world, [100.0, 40.0], None));
    same(item(&world, native), original);
    assert_eq!(world.get::<LayoutBox>(native).unwrap().rules, rules);
    assert_eq!(
        crate::native::canvas::presented(&world, native)
            .unwrap()
            .size,
        original.size + Vec2::new(100.0, 40.0)
    );
    assert!(commit(&mut world));
    super::super::super::engine::resolve(&mut world);
    assert_eq!(
        item(&world, native).size,
        original.size + Vec2::new(100.0, 40.0)
    );
    assert_eq!(
        world.get::<LayoutBox>(native).unwrap().rules.axes[0].size,
        original.size.x + 100.0
    );
}

#[test]
fn application_resize_edges_keep_height_and_survive_the_native_preview_transaction() {
    let (mut world, root, native, application) = fixture(WorkspaceLayout::Free);
    let original = item(&world, application);
    let saved = layout(&world, root);
    let unrelated = item(&world, native);
    assert!(!begin_with_edges(
        &mut world,
        root,
        application,
        Manipulation::Resize,
        [0.1; 2],
        Some([0; 2])
    ));
    assert!(!active(&world));
    assert!(begin_with_edges(
        &mut world,
        root,
        application,
        Manipulation::Resize,
        [0.1; 2],
        Some([1, 0])
    ));
    assert!(propose(&mut world, [40.0, 90.0], None));
    let preview = crate::native::canvas::presented(&world, application).unwrap();
    assert_eq!(preview.size, original.size + Vec2::new(40.0, 0.0));
    assert_eq!(
        preview.position,
        original.position + bevy::math::DVec2::new(20.0, 0.0)
    );
    same(item(&world, application), original);
    assert_eq!(layout(&world, root), saved);
    assert!(commit(&mut world));
    same(item(&world, application), preview);
    same(item(&world, native), unrelated);
}

#[test]
fn interactive_resize_state_covers_the_gesture_even_before_motion_and_clears_on_cancel() {
    let (mut world, root, native, application) = fixture(WorkspaceLayout::Free);
    assert!(!resizing(&world, application));
    assert!(begin_with_edges(
        &mut world,
        root,
        application,
        Manipulation::Resize,
        [0.5; 2],
        Some([1, 0])
    ));
    assert!(resizing(&world, application));
    assert!(!resizing(&world, native));
    assert!(propose(&mut world, [30.0, 50.0], None));
    assert!(window_state(&world, application).unwrap().resizing);
    cancel(&mut world);
    assert!(!resizing(&world, application));
    assert!(begin(
        &mut world,
        root,
        application,
        Manipulation::Move,
        [0.5; 2]
    ));
    assert!(!resizing(&world, application));
    cancel(&mut world);
}

#[test]
fn tiled_move_keeps_membership_and_one_stable_outline_until_commit() {
    let (mut world, root, native, application) = fixture(WorkspaceLayout::Tiled);
    let saved = layout(&world, root);
    let original = item(&world, native);
    let anchor = world.get::<Managed>(application).unwrap().id;
    let slot = Some(DropSlot {
        anchor,
        side: crate::host::Direction::Right,
    });
    assert!(begin(
        &mut world,
        root,
        native,
        Manipulation::Move,
        [0.5; 2]
    ));
    assert!(propose(&mut world, [40.0, 0.0], slot));
    let outline = world
        .resource::<State>()
        .0
        .as_ref()
        .unwrap()
        .outline
        .unwrap();
    assert!(
        world
            .get::<crate::native::inspection::InspectionExcluded>(outline)
            .is_some()
    );
    assert!(world.get::<LayoutBox>(outline).is_none());
    assert_eq!(layout(&world, root), saved);
    same(item(&world, native), original);
    assert!(propose(&mut world, [40.0, 0.0], slot));
    assert_eq!(
        world.resource::<State>().0.as_ref().unwrap().outline,
        Some(outline)
    );
    assert_eq!(
        world
            .query_filtered::<Entity, With<Outline>>()
            .iter(&world)
            .count(),
        1
    );
    assert!(commit(&mut world));
    assert!(world.get_entity(outline).is_err());
    assert_eq!(
        layout(&world, root).tiled_rows(),
        vec![vec![anchor, world.get::<Managed>(native).unwrap().id]]
    );
}

#[test]
fn zero_motion_preserves_maximize_focus_and_serializable_layout() {
    for mode in [WorkspaceLayout::Free, WorkspaceLayout::Tiled] {
        let (mut world, root, native, _) = fixture(mode);
        let id = world.get::<Managed>(native).unwrap().id;
        world.get_mut::<Workspaces>(root).unwrap().entries[0]
            .desktop
            .request_state(
                id,
                crate::host::WindowRequest {
                    maximized: Some(true),
                    ..Default::default()
                },
            );
        super::super::resolve(&mut world);
        let saved = layout(&world, root);
        let bytes = serde_json::to_vec(&saved).unwrap();
        assert!(begin(
            &mut world,
            root,
            native,
            Manipulation::Move,
            [0.5; 2]
        ));
        assert!(commit(&mut world));
        assert_eq!(layout(&world, root), saved);
        assert_eq!(serde_json::to_vec(&layout(&world, root)).unwrap(), bytes);
    }
}

#[test]
fn unmap_or_identity_replacement_cancels_without_retargeting_the_new_window() {
    let (mut world, root, native, _) = fixture(WorkspaceLayout::Free);
    let managed = *world.get::<Managed>(native).unwrap();
    assert!(begin(
        &mut world,
        root,
        native,
        Manipulation::Move,
        [0.5; 2]
    ));
    assert!(propose(&mut world, [30.0; 2], None));
    world.despawn(native);
    let replacement = world
        .spawn((
            managed,
            CanvasItem {
                position: bevy::math::DVec2::splat(500.0),
                size: Vec2::splat(100.0),
            },
            WorkspaceMember(1),
            ChildOf(root),
        ))
        .id();
    let original = item(&world, replacement);
    assert!(!commit(&mut world));
    same(item(&world, replacement), original);
    assert!(!active(&world));
}

#[test]
fn a_new_modal_keeps_its_input_when_it_cancels_manipulation() {
    let (mut world, root, native, _) = fixture(WorkspaceLayout::Free);
    assert!(begin(
        &mut world,
        root,
        native,
        Manipulation::Move,
        [0.5; 2]
    ));
    assert!(propose(&mut world, [30.0; 2], None));
    let modal = world.spawn_empty().id();
    let input = world
        .spawn((crate::native::sand::editable("Modal draft"), ChildOf(modal)))
        .id();
    world.insert_resource(crate::native::shortcuts::KeyboardModalOwner(Some(modal)));
    world
        .resource_mut::<bevy::input_focus::InputFocus>()
        .set(input, bevy::input_focus::FocusCause::Pressed);
    update(&mut world);
    assert!(!active(&world));
    assert!(world.get::<ItemPreview>(native).is_none());
    assert_eq!(
        world.resource::<bevy::input_focus::InputFocus>().get(),
        Some(input)
    );
}

#[test]
fn moved_dialog_keeps_its_committed_parent_offset_as_the_parent_moves() {
    let (mut world, root, native, application) = fixture(WorkspaceLayout::Free);
    world.get_mut::<CanvasItem>(native).unwrap().position = bevy::math::DVec2::new(500.0, 350.0);
    world
        .entity_mut(application)
        .insert(TransientParent(native));
    super::super::resolve(&mut world);
    let original = item(&world, application);
    let saved = layout(&world, root);
    assert!(begin(
        &mut world,
        root,
        application,
        Manipulation::Move,
        [0.5; 2]
    ));
    assert!(propose(&mut world, [50.0, 20.0], None));
    assert_eq!(layout(&world, root), saved);
    same(item(&world, application), original);
    assert!(world.get::<transients::Placement>(application).is_none());
    assert!(commit(&mut world));
    super::super::resolve(&mut world);
    let parent = item(&world, native);
    let dialog = item(&world, application);
    assert_eq!(
        dialog.position - parent.position,
        bevy::math::DVec2::new(50.0, 20.0)
    );
    world.get_mut::<CanvasItem>(native).unwrap().position.x += 40.0;
    super::super::resolve(&mut world);
    assert_eq!(
        item(&world, application).position.x,
        dialog.position.x + 40.0
    );
    assert_eq!(
        item(&world, application).position - item(&world, native).position,
        bevy::math::DVec2::new(50.0, 20.0)
    );
}

#[test]
fn dialog_manipulation_holds_its_ancestors_and_external_parent_motion_cancels_safely() {
    let (mut world, root, native, application) = fixture(WorkspaceLayout::Free);
    world.get_mut::<CanvasItem>(native).unwrap().position = bevy::math::DVec2::new(500.0, 350.0);
    world
        .entity_mut(application)
        .insert(TransientParent(native));
    super::super::resolve(&mut world);
    assert!(begin(
        &mut world,
        root,
        application,
        Manipulation::Move,
        [0.5; 2]
    ));
    assert!(crate::native::physics::held(&world, native));
    assert!(propose(&mut world, [50.0, 20.0], None));
    let original = item(&world, application);
    world.get_mut::<CanvasItem>(native).unwrap().position.x += 100.0;
    super::super::resolve(&mut world);
    let current = layout(&world, root);
    let moved = item(&world, application);
    assert_eq!(moved.position.x, original.position.x + 100.0);
    update(&mut world);
    assert!(!active(&world));
    assert_eq!(layout(&world, root), current);
    same(item(&world, application), moved);
    assert!(world.get::<ItemPreview>(application).is_none());
    assert!(world.get::<transients::Placement>(application).is_none());
    assert!(!held(&world, native));
}

#[test]
fn oversized_free_dialogs_keep_supported_sizes_and_authored_canvas_movement() {
    let (mut world, root, native, application) = fixture(WorkspaceLayout::Free);
    world.get_mut::<CanvasItem>(native).unwrap().position = bevy::math::DVec2::new(500.0, 350.0);
    world
        .entity_mut(application)
        .insert(TransientParent(native));
    let mut rules = world.get::<LayoutBox>(application).unwrap().rules;
    for (axis, minimum) in rules.axes.iter_mut().zip([1200.0, 900.0]) {
        axis.min = minimum;
        axis.size = minimum;
    }
    world.get_mut::<LayoutBox>(application).unwrap().rules = rules;
    super::super::resolve(&mut world);
    assert_eq!(item(&world, application).size, Vec2::new(1200.0, 900.0));
    let initial = item(&world, application);
    let saved = layout(&world, root);
    assert!(begin(
        &mut world,
        root,
        application,
        Manipulation::Move,
        [0.5; 2]
    ));
    assert!(propose(&mut world, [10_000.0; 2], None));
    let preview = crate::native::canvas::presented(&world, application).unwrap();
    assert_eq!(preview.size, Vec2::new(1200.0, 900.0));
    assert_eq!(
        preview.position,
        initial.position + bevy::math::DVec2::splat(10_000.0)
    );
    assert_eq!(layout(&world, root), saved);
    assert!(commit(&mut world));
    super::super::super::engine::resolve(&mut world);
    assert_eq!(item(&world, application).size, Vec2::new(1200.0, 900.0));
    let source = layout(&world, root);
    let id = world.get::<Managed>(application).unwrap().id;
    let current = source.items().iter().find(|item| item.id == id).unwrap();
    assert_eq!(current.free.size, [1200.0, 900.0]);
    assert_eq!(current.rules.axes[0].min, 1200.0);
    assert!(source.valid());
    world.get_mut::<CanvasItem>(native).unwrap().position.x -= 50.0;
    super::super::resolve(&mut world);
    let child = item(&world, application);
    assert_eq!(
        child.position,
        preview.position - bevy::math::DVec2::new(50.0, 0.0)
    );
}

#[test]
fn oversized_tiled_dialogs_keep_supported_sizes_and_the_normalized_output_extent() {
    let (mut world, root, native, application) = fixture(WorkspaceLayout::Tiled);
    world
        .entity_mut(application)
        .insert(TransientParent(native));
    let mut rules = world.get::<LayoutBox>(application).unwrap().rules;
    for (axis, minimum) in rules.axes.iter_mut().zip([1200.0, 900.0]) {
        axis.min = minimum;
        axis.size = minimum;
    }
    world.get_mut::<LayoutBox>(application).unwrap().rules = rules;
    super::super::resolve(&mut world);
    let saved = layout(&world, root);
    assert!(begin(
        &mut world,
        root,
        application,
        Manipulation::Move,
        [0.5; 2]
    ));
    assert!(propose(&mut world, [10_000.0; 2], None));
    let preview = crate::native::canvas::presented(&world, application).unwrap();
    assert_eq!(preview.size, Vec2::new(1200.0, 900.0));
    assert_eq!(
        preview.position - preview.size.as_dvec2() * 0.5,
        bevy::math::DVec2::ZERO
    );
    assert_eq!(layout(&world, root), saved);
    assert!(commit(&mut world));
    super::super::super::engine::resolve(&mut world);
    assert_eq!(item(&world, application).size, preview.size);
    assert_eq!(item(&world, application).position, preview.position);
    assert!(layout(&world, root).valid());
}

#[test]
fn maximize_preview_only_changes_the_transient_client_state_until_commit() {
    let (mut world, root, native, _) = fixture(WorkspaceLayout::Free);
    let id = world.get::<Managed>(native).unwrap().id;
    world.get_mut::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .request_state(
            id,
            crate::host::WindowRequest {
                maximized: Some(true),
                ..Default::default()
            },
        );
    super::super::resolve(&mut world);
    assert!(begin(
        &mut world,
        root,
        native,
        Manipulation::Move,
        [0.5; 2]
    ));
    assert!(propose(&mut world, [30.0; 2], None));
    assert!(!window_state(&world, native).unwrap().maximized);
    assert!(layout(&world, root).maximized(id));
    cancel(&mut world);
    assert!(window_state(&world, native).is_none());
    assert!(layout(&world, root).maximized(id));
    assert!(begin(
        &mut world,
        root,
        native,
        Manipulation::Move,
        [0.5; 2]
    ));
    assert!(propose(&mut world, [30.0; 2], None));
    assert!(commit(&mut world));
    assert!(!layout(&world, root).maximized(id));
}

#[test]
fn rotated_scaled_resize_keeps_the_opposite_corner_and_pose_preview_out_of_saved_state() {
    let (mut world, root, native, _) = fixture(WorkspaceLayout::Free);
    let spatial = crate::native::topology::Spatial {
        elevation: 20.0,
        rotation: (bevy::math::DQuat::from_rotation_y(0.6)
            * bevy::math::DQuat::from_rotation_x(0.35))
        .to_array(),
        ..Default::default()
    };
    world
        .entity_mut(native)
        .insert((spatial, crate::native::area_effects::AreaScale(2.0)));
    let corner = |item: CanvasItem, spatial: crate::native::topology::Spatial| {
        spatial.position(item.position)
            + spatial.rotation()
                * bevy::math::DVec3::new(-f64::from(item.size.x), 0.0, -f64::from(item.size.y))
    };
    let original = item(&world, native);
    let saved = layout(&world, root);
    let opposite = corner(original, spatial);
    assert!(begin(
        &mut world,
        root,
        native,
        Manipulation::Resize,
        [1.0; 2]
    ));
    assert!(propose(&mut world, [80.0, 50.0], None));
    let preview = crate::native::canvas::presented(&world, native).unwrap();
    let pose = crate::native::canvas::presented_spatial(&world, native);
    assert!((corner(preview, pose) - opposite).length() < 0.000_001);
    same(item(&world, native), original);
    assert_eq!(crate::native::topology::spatial(&world, native), spatial);
    assert_eq!(layout(&world, root), saved);
    assert!(commit(&mut world));
    assert!(
        (corner(
            item(&world, native),
            crate::native::topology::spatial(&world, native)
        ) - opposite)
            .length()
            < 0.000_001
    );
    assert!(
        world
            .get::<crate::native::canvas::SpatialPreview>(native)
            .is_none()
    );
}

#[test]
fn moving_a_maximized_window_restores_normal_size_under_the_grab_without_a_camera_jump() {
    let (mut world, root, native, _) = fixture(WorkspaceLayout::Free);
    let id = world.get::<Managed>(native).unwrap().id;
    let normal = item(&world, native);
    world.get_mut::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .request_state(
            id,
            crate::host::WindowRequest {
                maximized: Some(true),
                ..Default::default()
            },
        );
    super::super::resolve(&mut world);
    let maximized = item(&world, native);
    let camera = *world.get::<CanvasView>(root).unwrap();
    let uv = [0.8, 0.2];
    let point = |item: CanvasItem| {
        item.position
            + bevy::math::DVec2::new(
                (uv[0] - 0.5) * f64::from(item.size.x),
                (uv[1] - 0.5) * f64::from(item.size.y),
            )
    };
    let grabbed = point(maximized);
    assert!(begin(&mut world, root, native, Manipulation::Move, uv));
    assert!(propose(&mut world, [40.0, 30.0], None));
    let preview = crate::native::canvas::presented(&world, native).unwrap();
    assert_eq!(preview.size, normal.size);
    assert!((point(preview) - grabbed - bevy::math::DVec2::new(40.0, 30.0)).length() < 0.000_001);
    assert!(commit(&mut world));
    super::super::resolve(&mut world);
    assert_eq!(world.get::<CanvasView>(root).unwrap().center, camera.center);
    assert_eq!(world.get::<CanvasView>(root).unwrap().zoom, camera.zoom);
    assert!((point(item(&world, native)) - point(preview)).length() < 0.000_001);
}

#[test]
fn fitting_content_does_not_rewrite_authored_size_while_a_preview_owns_the_item() {
    let (mut world, root, native, _) = fixture(WorkspaceLayout::Free);
    world.get_mut::<LayoutBox>(native).unwrap().rules.axes[0].sizing = crate::layout::Sizing::Fit;
    world.entity_mut(native).insert(ComputedNode {
        size: Vec2::new(300.0, 200.0),
        content_size: Vec2::new(300.0, 200.0),
        inverse_scale_factor: 1.0,
        ..Default::default()
    });
    super::super::super::engine::resolve(&mut world);
    let original = item(&world, native);
    let saved = layout(&world, root);
    assert!(begin(
        &mut world,
        root,
        native,
        Manipulation::Move,
        [0.5; 2]
    ));
    assert!(propose(&mut world, [80.0; 2], None));
    world
        .get_mut::<ComputedNode>(native)
        .unwrap()
        .content_size
        .x = 600.0;
    for _ in 0..2 {
        super::super::super::engine::resolve(&mut world);
        update(&mut world);
    }
    assert!(active(&world));
    same(item(&world, native), original);
    assert_eq!(layout(&world, root), saved);
    cancel(&mut world);
    super::super::super::engine::resolve(&mut world);
    assert_eq!(item(&world, native).size.x, 600.0);
}

#[test]
fn another_desktop_command_cancels_before_applying_its_new_focus() {
    use crate::native::actions::Action;
    let (mut world, root, native, application) = fixture(WorkspaceLayout::Free);
    world.get_mut::<CanvasItem>(native).unwrap().position.x = -200.0;
    world.get_mut::<CanvasItem>(application).unwrap().position.x = 200.0;
    super::super::resolve(&mut world);
    focus::select(&mut world, root, 1, native, true);
    let original = item(&world, native);
    assert!(begin(
        &mut world,
        root,
        native,
        Manipulation::Move,
        [0.5; 2]
    ));
    assert!(propose(&mut world, [80.0; 2], None));
    DesktopAction::Focus(crate::host::Direction::Right).apply(&mut world, root);
    assert!(!active(&world));
    same(item(&world, native), original);
    assert_eq!(
        layout(&world, root).focused(),
        Some(world.get::<Managed>(application).unwrap().id)
    );
}

#[test]
fn resize_state_distinguishes_position_only_neighbors_from_shared_row_size_changes() {
    let (mut world, root, native, application) = fixture(WorkspaceLayout::Tiled);
    let original = item(&world, application);
    assert!(begin(
        &mut world,
        root,
        native,
        Manipulation::Resize,
        [0.9, 0.5]
    ));
    assert!(resizing(&world, native));
    assert!(!resizing(&world, application));
    assert!(propose(&mut world, [50.0, 0.0], None));
    let preview = crate::native::canvas::presented(&world, application).unwrap();
    assert_eq!(preview.size, original.size);
    assert!((preview.position.x - original.position.x - 50.0).abs() < 0.01);
    assert!(held(&world, application));
    assert!(!resizing(&world, application));
    assert!(!window_state(&world, application).unwrap().resizing);
    cancel(&mut world);
    assert!(!resizing(&world, native));
    assert!(!resizing(&world, application));
    assert!(begin(
        &mut world,
        root,
        application,
        Manipulation::Resize,
        [0.5, 0.99]
    ));
    assert!(propose(&mut world, [0.0, -40.0], None));
    for entity in [native, application] {
        assert!(resizing(&world, entity));
        assert!(window_state(&world, entity).unwrap().resizing);
        assert!(held(&world, entity));
        assert!(
            crate::native::canvas::presented(&world, entity)
                .unwrap()
                .size
                .y
                < item(&world, entity).size.y
        );
    }
    cancel(&mut world);
    assert!(!resizing(&world, native));
    assert!(!resizing(&world, application));
}
