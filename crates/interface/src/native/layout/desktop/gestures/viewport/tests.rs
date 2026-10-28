use super::*;
use crate::host::{
    OutputId, OutputViewport,
    input::{GestureCompletion, GestureProgress},
};
use crate::native::actions::Action;

fn fixture(mode: WorkspaceLayout) -> (World, Entity, Entity, Entity) {
    let (mut world, root, native, application) = crate::native::layout::desktop::tests::fixture();
    world.get_mut::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .set_mode(mode);
    let bounds = Rectangle::new([0.0; 2], [1000.0, 700.0]).unwrap();
    world.entity_mut(root).insert(HostViewport(OutputViewport { rotation: Default::default(),
        id: OutputId(0),
        workspace: 1,
        bounds,
        usable: Some(bounds),
        physical_size: [1000, 700],
        raster: crate::host::raster::RasterMapping {
            origin: [0.0; 2],
            scale: [1.0; 2],
        },
    }));
    crate::native::layout::engine::resolve(&mut world);
    (world, root, native, application)
}

fn begin(world: &mut World, root: Entity, sequence: u64, fingers: u32) {
    assert_eq!(
        deliver(
            world,
            root,
            GestureDelivery::Begin(GestureContext {
                sequence,
                owner: root.to_bits(),
                workspace: 1,
                output: OutputId(0),
                fingers,
            })
        ),
        None
    );
}

fn progress(world: &mut World, root: Entity, sequence: u64, travel: f64) {
    assert_eq!(
        deliver(
            world,
            root,
            GestureDelivery::Pan(GestureProgress {
                sequence,
                output: OutputId(0),
                horizontal_travel: travel,
            })
        ),
        None
    );
}

fn finish(
    world: &mut World,
    root: Entity,
    sequence: u64,
    result: GestureResult,
) -> Option<DesktopAction> {
    deliver(
        world,
        root,
        GestureDelivery::Finish(GestureCompletion {
            sequence,
            output: OutputId(0),
            result,
        }),
    )
}

#[test]
fn free_pan_is_live_absolute_and_cancel_restores_camera_follow_and_editor() {
    let (mut world, root, native, _) = fixture(WorkspaceLayout::Free);
    world
        .entity_mut(root)
        .insert(crate::native::canvas_selection::SandSelection(vec![native]));
    DesktopAction::ToggleFocus.apply(&mut world, root);
    focus::resolve(&mut world);
    assert!(focus::following(&world, root));
    let editor = world
        .spawn((crate::native::sand::editable("kept draft"), ChildOf(native)))
        .id();
    world
        .resource_mut::<bevy::input_focus::InputFocus>()
        .set(editor, bevy::input_focus::FocusCause::Pressed);
    let before = *world.get::<CanvasView>(root).unwrap();
    let item = *world
        .get::<crate::native::canvas::CanvasItem>(native)
        .unwrap();
    begin(&mut world, root, 1, 3);
    assert!(focus::following(&world, root));
    progress(&mut world, root, 1, 40.0);
    assert_eq!(
        world.get::<CanvasView>(root).unwrap().center.x,
        before.center.x - 40.0 / before.zoom
    );
    assert!(!focus::following(&world, root));
    progress(&mut world, root, 1, 80.0);
    progress(&mut world, root, 1, 80.0);
    assert_eq!(
        world.get::<CanvasView>(root).unwrap().center.x,
        before.center.x - 80.0 / before.zoom
    );
    assert_eq!(
        world
            .get::<crate::native::canvas::CanvasItem>(native)
            .unwrap()
            .position,
        item.position
    );
    cancel(&mut world);
    assert!(*world.get::<CanvasView>(root).unwrap() == before);
    assert!(focus::following(&world, root));
    assert_eq!(
        world.resource::<bevy::input_focus::InputFocus>().get(),
        Some(editor)
    );
    assert_eq!(
        world
            .get::<bevy::text::EditableText>(editor)
            .unwrap()
            .value()
            .to_string(),
        "kept draft"
    );
    assert_eq!(
        finish(&mut world, root, 1, GestureResult::Workspace(1)),
        None
    );
}

#[test]
fn tiled_pan_preserves_members_and_client_geometry_and_cancel_restores_scroll() {
    let (mut world, root, _, _) = fixture(WorkspaceLayout::Tiled);
    let model = &mut world.get_mut::<Workspaces>(root).unwrap().entries[0].desktop;
    let item = model.items()[0].clone();
    for value in 10..14 {
        let mut item = item.clone();
        item.id = crate::layout::desktop::PlacementId([value; 16]);
        model.insert(item).unwrap();
    }
    let bounds = world.get::<HostViewport>(root).unwrap().0.usable.unwrap();
    let before = world.get::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .clone();
    begin(&mut world, root, 1, 3);
    progress(&mut world, root, 1, -100.0);
    let model = &world.get::<Workspaces>(root).unwrap().entries[0].desktop;
    assert_eq!(model.scroll(), [100.0, 0.0]);
    assert_eq!(model.items(), before.items());
    assert_eq!(model.tiled_rows(), before.tiled_rows());
    assert_eq!(model.focused(), before.focused());
    progress(&mut world, root, 1, -200.0);
    assert_eq!(
        world.get::<Workspaces>(root).unwrap().entries[0]
            .desktop
            .scroll(),
        [200.0, 0.0]
    );
    assert_eq!(finish(&mut world, root, 1, GestureResult::Cancelled), None);
    let model = &mut world.get_mut::<Workspaces>(root).unwrap().entries[0].desktop;
    assert_eq!(model.scroll(), before.scroll());
    assert_eq!(model.solve(bounds, bounds).unwrap().scroll, before.scroll());
}

#[test]
fn workspace_and_overview_commit_once_and_noop_does_not_release_focus() {
    let (mut world, root, native, _) = fixture(WorkspaceLayout::Free);
    world
        .entity_mut(root)
        .insert(crate::native::canvas_selection::SandSelection(vec![native]));
    DesktopAction::ToggleFocus.apply(&mut world, root);
    focus::resolve(&mut world);
    begin(&mut world, root, 1, 3);
    assert_eq!(finish(&mut world, root, 1, GestureResult::Noop), None);
    assert!(focus::following(&world, root));
    begin(&mut world, root, 2, 3);
    assert_eq!(
        finish(&mut world, root, 2, GestureResult::Workspace(1)),
        Some(DesktopAction::NextWorkspace)
    );
    assert_eq!(
        finish(&mut world, root, 2, GestureResult::Workspace(1)),
        None
    );
    begin(&mut world, root, 3, 4);
    assert_eq!(
        finish(&mut world, root, 3, GestureResult::Overview(true)),
        Some(DesktopAction::ToggleOverview)
    );
    assert_eq!(
        finish(&mut world, root, 3, GestureResult::Overview(true)),
        None
    );
    begin(&mut world, root, 4, 4);
    assert_eq!(
        finish(&mut world, root, 4, GestureResult::Workspace(1)),
        None
    );
    begin(&mut world, root, 5, 3);
    assert_eq!(
        finish(&mut world, root, 5, GestureResult::Overview(true)),
        None
    );
}

#[test]
fn stale_sequences_output_changes_and_new_workspaces_cannot_commit_or_restore_over_new_state() {
    let (mut world, root, _, _) = fixture(WorkspaceLayout::Free);
    begin(&mut world, root, 1, 3);
    progress(&mut world, root, 2, 80.0);
    progress(&mut world, root, 1, 20.0);
    let moved = *world.get::<CanvasView>(root).unwrap();
    world.get_mut::<Workspaces>(root).unwrap().active = 2;
    world.get_mut::<CanvasView>(root).unwrap().center.x = 1234.0;
    assert_eq!(
        finish(&mut world, root, 1, GestureResult::Workspace(1)),
        None
    );
    cancel(&mut world);
    assert_eq!(world.get::<CanvasView>(root).unwrap().center.x, 1234.0);
    world.get_mut::<Workspaces>(root).unwrap().active = 1;
    *world.get_mut::<CanvasView>(root).unwrap() = moved;
    begin(&mut world, root, 2, 3);
    world.get_mut::<HostViewport>(root).unwrap().0.id = OutputId(1);
    assert_eq!(
        finish(&mut world, root, 2, GestureResult::Workspace(1)),
        None
    );
    assert!(*world.get::<CanvasView>(root).unwrap() == moved);
}

#[test]
fn malformed_pan_rolls_back_and_modal_takeover_cannot_receive_a_late_workspace_commit() {
    let (mut world, root, _, _) = fixture(WorkspaceLayout::Free);
    let before = *world.get::<CanvasView>(root).unwrap();
    begin(&mut world, root, 1, 3);
    progress(&mut world, root, 1, 30.0);
    progress(&mut world, root, 1, f64::NAN);
    assert!(*world.get::<CanvasView>(root).unwrap() == before);
    begin(&mut world, root, 2, 3);
    let modal = world.spawn_empty().id();
    world.insert_resource(crate::native::shortcuts::KeyboardModalOwner(Some(modal)));
    assert_eq!(
        finish(&mut world, root, 2, GestureResult::Workspace(1)),
        None
    );
    begin(&mut world, root, 3, 4);
    assert_eq!(
        finish(&mut world, root, 3, GestureResult::Overview(true)),
        None
    );
    assert!(world.resource::<State>().session.is_none());
}

#[test]
fn spatial_pan_uses_the_existing_camera_projection_at_each_output_scale_and_cancel_restores_it() {
    use bevy::camera::CameraProjection;
    for pitch in [0.0, -0.6] {
        let mut expected = None;
        for scale in [1.0, 2.0] {
            let (mut world, root, _, _) = fixture(WorkspaceLayout::Free);
            let view = View {
                spatial: true,
                pitch,
                ..default()
            };
            world.entity_mut(root).insert(view);
            let physical_size = (Vec2::new(1000.0, 700.0) * scale).as_uvec2();
            world.get_mut::<HostViewport>(root).unwrap().0.physical_size = physical_size.to_array();
            world.get_mut::<HostViewport>(root).unwrap().0.raster.scale = [f64::from(scale); 2];
            let mut projection = PerspectiveProjection::default();
            projection.update(1000.0, 700.0);
            let mut camera = Camera::default();
            camera.computed.target_info = Some(bevy::camera::RenderTargetInfo {
                physical_size,
                scale_factor: scale,
            });
            camera.computed.clip_from_view = projection.get_clip_from_view();
            let camera = world
                .spawn((
                    camera,
                    GlobalTransform::from(
                        Transform::from_translation(DVec3::from_array(view.position).as_vec3())
                            .with_rotation(Quat::from_euler(
                                EulerRot::YXZ,
                                view.yaw,
                                view.pitch,
                                0.0,
                            )),
                    ),
                ))
                .id();
            world.insert_resource(crate::native::topology::presentation::SceneCamera(camera));
            let before = *world.get::<CanvasView>(root).unwrap();
            begin(&mut world, root, 1, 3);
            progress(&mut world, root, 1, 40.0);
            let moved = *world.get::<View>(root).unwrap();
            assert!(moved.valid());
            assert!(moved.position[0] < view.position[0]);
            let delta = DVec3::from_array(moved.position) - DVec3::from_array(view.position);
            assert!(delta.distance(*expected.get_or_insert(delta)) < 1e-3);
            assert_eq!((moved.yaw, moved.pitch), (view.yaw, view.pitch));
            assert_eq!(finish(&mut world, root, 1, GestureResult::Cancelled), None);
            assert_eq!(*world.get::<View>(root).unwrap(), view);
            assert!(*world.get::<CanvasView>(root).unwrap() == before);
        }
    }
}
