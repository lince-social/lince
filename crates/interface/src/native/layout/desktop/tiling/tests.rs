use super::*;
use crate::native::actions::Action;

mod spatial;

fn fixture() -> (World, Entity, Entity, Entity) {
    let (mut world, root, native, application) = super::super::tests::fixture();
    world.get_mut::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .set_mode(WorkspaceLayout::Free);
    *world.get_mut::<CanvasView>(root).unwrap() = CanvasView {
        center: bevy::math::DVec2::new(1200.0, -450.0),
        zoom: 0.6,
    };
    world.get_mut::<CanvasItem>(native).unwrap().position = bevy::math::DVec2::new(1000.0, -400.0);
    world.get_mut::<CanvasItem>(application).unwrap().position =
        bevy::math::DVec2::new(1400.0, -500.0);
    world.insert_resource(bevy::input_focus::InputFocus::default());
    (world, root, native, application)
}

fn resolve(world: &mut World) {
    super::super::super::engine::resolve(world);
    super::super::resolve(world);
}

#[test]
fn tiling_round_trip_restores_authored_geometry_and_camera_and_keeps_window_identity() {
    let (mut world, root, native, application) = fixture();
    world
        .entity_mut(root)
        .insert(crate::native::canvas_selection::SandSelection(vec![native]));
    let originals = [native, application].map(|entity| *world.get::<CanvasItem>(entity).unwrap());
    DesktopAction::ToggleTiling.apply(&mut world, root);
    for _ in 0..3 {
        resolve(&mut world);
    }
    let managed = world.get::<Managed>(native).unwrap().id;
    let window = world.get::<Managed>(application).unwrap().id;
    let model = &world.get::<Workspaces>(root).unwrap().entries[0].desktop;
    assert_eq!(model.mode(), WorkspaceLayout::Tiled);
    assert_eq!(model.focused(), Some(managed));
    assert_eq!(model.tiled_rows(), vec![vec![managed, window]]);
    assert_eq!(world.get::<CanvasView>(root).unwrap().zoom, 1.0);
    let serialized =
        serde_json::to_value(&world.get::<Workspaces>(root).unwrap().entries[0]).unwrap();
    let restored: crate::native::workspace::Workspace = serde_json::from_value(serialized).unwrap();
    world.get_mut::<Workspaces>(root).unwrap().entries[0] = restored;
    DesktopAction::ToggleTiling.apply(&mut world, root);
    for _ in 0..3 {
        resolve(&mut world);
    }
    assert_eq!(
        world.get::<CanvasView>(root).unwrap().center,
        bevy::math::DVec2::new(1200.0, -450.0)
    );
    assert_eq!(world.get::<CanvasView>(root).unwrap().zoom, 0.6);
    for (entity, original) in [native, application].into_iter().zip(originals) {
        assert_eq!(
            world.get::<CanvasItem>(entity).unwrap().position,
            original.position
        );
        assert_eq!(world.get::<CanvasItem>(entity).unwrap().size, original.size);
    }
    assert_eq!(world.get::<Managed>(application).unwrap().id, window);
    DesktopAction::ToggleTiling.apply(&mut world, root);
    for _ in 0..3 {
        resolve(&mut world);
    }
    let model = &world.get::<Workspaces>(root).unwrap().entries[0].desktop;
    assert_eq!(model.tiled_rows(), vec![vec![managed, window]]);
    assert_eq!(model.focused(), Some(managed));
}

#[test]
fn entering_tiling_without_a_selection_uses_the_nearest_original_item() {
    let (mut world, root, native, application) = fixture();
    world.get_mut::<CanvasItem>(native).unwrap().position = bevy::math::DVec2::new(-5000.0, 0.0);
    DesktopAction::ToggleTiling.apply(&mut world, root);
    resolve(&mut world);
    let expected = world.get::<Managed>(application).unwrap().id;
    assert_eq!(
        world.get::<Workspaces>(root).unwrap().entries[0]
            .desktop
            .focused(),
        Some(expected)
    );
}

#[test]
fn floating_is_a_free_mode_noop_and_restores_its_row_slot_after_retiling() {
    let (mut world, root, native, _) = fixture();
    DesktopAction::ToggleFloating.apply(&mut world, root);
    assert!(
        world.get::<Workspaces>(root).unwrap().entries[0]
            .desktop
            .items()
            .is_empty()
    );
    world
        .entity_mut(root)
        .insert(crate::native::canvas_selection::SandSelection(vec![native]));
    DesktopAction::ToggleTiling.apply(&mut world, root);
    resolve(&mut world);
    let before = world.get::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .tiled_rows();
    DesktopAction::ToggleFloating.apply(&mut world, root);
    resolve(&mut world);
    assert!(world.get::<Managed>(native).unwrap().floating);
    DesktopAction::ToggleFloating.apply(&mut world, root);
    resolve(&mut world);
    assert!(!world.get::<Managed>(native).unwrap().floating);
    assert_eq!(
        world.get::<Workspaces>(root).unwrap().entries[0]
            .desktop
            .tiled_rows(),
        before
    );
}

#[test]
fn leaving_fullscreen_after_untile_restores_the_original_free_camera() {
    let (mut world, root, native, _) = fixture();
    world
        .entity_mut(root)
        .insert(crate::native::canvas_selection::SandSelection(vec![native]));
    DesktopAction::ToggleTiling.apply(&mut world, root);
    resolve(&mut world);
    DesktopAction::ToggleFullscreen.apply(&mut world, root);
    resolve(&mut world);
    DesktopAction::ToggleTiling.apply(&mut world, root);
    resolve(&mut world);
    DesktopAction::ToggleFullscreen.apply(&mut world, root);
    for _ in 0..3 {
        resolve(&mut world);
    }
    assert_eq!(
        world.get::<CanvasView>(root).unwrap().center,
        bevy::math::DVec2::new(1200.0, -450.0)
    );
    assert_eq!(world.get::<CanvasView>(root).unwrap().zoom, 0.6);
}

#[test]
fn entering_tiling_captures_a_last_frame_free_move_before_layout_projection() {
    let (mut world, root, native, _) = fixture();
    world.insert_resource(DefaultLayout(WorkspaceLayout::Free));
    resolve(&mut world);
    let moved = bevy::math::DVec2::new(1760.0, -540.0);
    world.get_mut::<CanvasItem>(native).unwrap().position = moved;
    DesktopAction::ToggleTiling.apply(&mut world, root);
    resolve(&mut world);
    DesktopAction::ToggleTiling.apply(&mut world, root);
    resolve(&mut world);
    assert_eq!(world.get::<CanvasItem>(native).unwrap().position, moved);
}

#[test]
fn entering_tiling_from_free_fullscreen_keeps_the_authored_camera_for_later_restoration() {
    let (mut world, root, _, application) = fixture();
    world.insert_resource(DefaultLayout(WorkspaceLayout::Free));
    resolve(&mut world);
    let id = world.get::<Managed>(application).unwrap().id;
    let bounds = usable(&world, root).unwrap();
    world.get_mut::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .focus(id, bounds)
        .unwrap();
    DesktopAction::ToggleFullscreen.apply(&mut world, root);
    resolve(&mut world);
    DesktopAction::ToggleTiling.apply(&mut world, root);
    resolve(&mut world);
    DesktopAction::ToggleFullscreen.apply(&mut world, root);
    resolve(&mut world);
    assert_eq!(world.get::<CanvasView>(root).unwrap().zoom, 1.0);
    DesktopAction::ToggleTiling.apply(&mut world, root);
    resolve(&mut world);
    assert_eq!(
        world.get::<CanvasView>(root).unwrap().center,
        bevy::math::DVec2::new(1200.0, -450.0)
    );
    assert_eq!(world.get::<CanvasView>(root).unwrap().zoom, 0.6);
}
