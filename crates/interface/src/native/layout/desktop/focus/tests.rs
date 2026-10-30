use super::*;
use crate::native::actions::Action;

fn fixture() -> (World, Entity, Entity, Entity) {
    let (mut world, root, native, application) = super::super::tests::fixture();
    world.get_mut::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .set_mode(WorkspaceLayout::Free);
    world.get_mut::<CanvasItem>(native).unwrap().position = DVec2::new(-500.0, 120.0);
    world.get_mut::<CanvasItem>(application).unwrap().position = DVec2::new(600.0, -90.0);
    world
        .entity_mut(root)
        .insert(crate::native::canvas_selection::SandSelection(vec![native]));
    (world, root, native, application)
}

#[test]
fn returning_to_a_workspace_restores_its_editor_and_follow_without_restoring_stale_input() {
    let (mut world, root, native, application) = fixture();
    world.init_resource::<bevy::input_focus::InputFocus>();
    let mut second = world.get::<Workspaces>(root).unwrap().entries[0].clone();
    second.id = 2;
    second.desktop = crate::layout::desktop::Workspace::new(WorkspaceLayout::Free);
    world
        .get_mut::<Workspaces>(root)
        .unwrap()
        .entries
        .push(second);
    world.entity_mut(application).insert(WorkspaceMember(2));
    DesktopAction::ToggleFocus.apply(&mut world, root);
    frame(&mut world);
    let editor = world
        .spawn((
            crate::native::sand::editable("Retained draft"),
            ChildOf(native),
        ))
        .id();
    world
        .resource_mut::<bevy::input_focus::InputFocus>()
        .set(editor, bevy::input_focus::FocusCause::Pressed);
    let camera = *world.get::<CanvasView>(root).unwrap();
    assert!(crate::native::workspace::switch_output(&mut world, root, 2));
    DesktopAction::ToggleFocus.apply(&mut world, root);
    frame(&mut world);
    assert_eq!(
        world.resource::<bevy::input_focus::InputFocus>().get(),
        Some(application)
    );
    assert!(crate::native::workspace::switch_output(&mut world, root, 1));
    assert_eq!(
        world.resource::<bevy::input_focus::InputFocus>().get(),
        Some(editor)
    );
    frame(&mut world);
    assert!(following(&world, root));
    assert!(*world.get::<CanvasView>(root).unwrap() == camera);
    assert_eq!(
        world
            .get::<bevy::text::EditableText>(editor)
            .unwrap()
            .value()
            .to_string(),
        "Retained draft"
    );
    assert!(crate::native::workspace::switch_output(&mut world, root, 2));
    world.despawn(editor);
    assert!(crate::native::workspace::switch_output(&mut world, root, 1));
    assert_eq!(
        world.resource::<bevy::input_focus::InputFocus>().get(),
        Some(native)
    );
    DesktopAction::Unfocus.apply(&mut world, root);
    assert!(crate::native::workspace::switch_output(&mut world, root, 2));
    assert!(crate::native::workspace::switch_output(&mut world, root, 1));
    assert_eq!(
        world.resource::<bevy::input_focus::InputFocus>().get(),
        None
    );
    assert!(!following(&world, root));
}

#[test]
fn a_minimized_transient_parent_excludes_its_descendants_from_focus_and_input_restoration() {
    let (mut world, root, native, application) = fixture();
    world
        .entity_mut(application)
        .insert(TransientParent(native));
    DesktopAction::ToggleFocus.apply(&mut world, root);
    frame(&mut world);
    let parent = world.get::<Managed>(native).unwrap().id;
    world.get_mut::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .set_minimized(parent, true);
    frame(&mut world);
    assert!(candidates(&world, root, 1).is_empty());
    assert_eq!(status(&world, root), (false, false));
    DesktopAction::ToggleFocus.apply(&mut world, root);
    DesktopAction::Focus(Direction::Right).apply(&mut world, root);
    restore_input(&mut world, root);
    frame(&mut world);
    assert!(!following(&world, root));
    assert!(
        world.get::<Workspaces>(root).unwrap().entries[0]
            .desktop
            .minimized(parent)
    );
    assert_eq!(focused(&world, root, 1), None);
}

fn frame(world: &mut World) {
    super::super::super::engine::resolve(world);
    resolve(world);
    super::super::publish(world);
}

#[test]
fn saved_native_focus_restores_follow_but_explicit_unfocus_stays_unfocused() {
    let (mut world, root, native, _) = fixture();
    DesktopAction::ToggleFocus.apply(&mut world, root);
    frame(&mut world);
    let saved = snapshot(&world, root);
    let bytes = serde_json::to_vec(&saved).unwrap();
    world.get_mut::<Workspaces>(root).unwrap().saved_focus = serde_json::from_slice(&bytes).unwrap();
    world.remove_resource::<FocusState>();
    let primary = world.spawn((bevy::window::Window::default(), bevy::window::PrimaryWindow)).id();
    world.resource_mut::<bevy::input_focus::InputFocus>().set(primary, bevy::input_focus::FocusCause::Navigated);
    frame(&mut world);
    assert!(following(&world, root));
    assert_eq!(world.resource::<bevy::input_focus::InputFocus>().get(), Some(native));
    DesktopAction::Unfocus.apply(&mut world, root);
    let saved = snapshot(&world, root);
    assert_eq!(serde_json::to_value(&saved).unwrap()[0]["active"], serde_json::Value::Null);
    world.get_mut::<Workspaces>(root).unwrap().saved_focus = saved;
    world.remove_resource::<FocusState>();
    frame(&mut world);
    assert!(!status(&world, root).0);
    assert!(!following(&world, root));
    assert_eq!(world.resource::<bevy::input_focus::InputFocus>().get(), None);
}

#[test]
fn saved_focus_waits_for_a_native_target_and_cannot_replace_another_editor_input() {
    let (mut world, root, native, application) = fixture();
    DesktopAction::ToggleFocus.apply(&mut world, root);
    frame(&mut world);
    let saved = snapshot(&world, root);
    world.get_mut::<Workspaces>(root).unwrap().saved_focus = saved.clone();
    world.remove_resource::<FocusState>();
    let editor = world.spawn((crate::native::sand::editable("Another editor's draft"), ChildOf(application))).id();
    world.resource_mut::<bevy::input_focus::InputFocus>().set(editor, bevy::input_focus::FocusCause::Pressed);
    frame(&mut world);
    assert!(!following(&world, root));
    assert_eq!(world.resource::<bevy::input_focus::InputFocus>().get(), Some(editor));
    assert_eq!(world.get::<bevy::text::EditableText>(editor).unwrap().value().to_string(), "Another editor's draft");
    assert_eq!(snapshot(&world, root), saved);
    world.resource_mut::<bevy::input_focus::InputFocus>().clear();
    world.entity_mut(native).insert(crate::native::castle::Pending);
    frame(&mut world);
    assert!(!following(&world, root));
    assert_eq!(snapshot(&world, root), saved);
    world.entity_mut(native).remove::<crate::native::castle::Pending>();
    frame(&mut world);
    assert!(following(&world, root));
    assert_eq!(world.resource::<bevy::input_focus::InputFocus>().get(), Some(native));
}

#[test]
fn saved_application_focus_never_recreates_or_reactivates_a_protocol_target() {
    let (mut world, root, _, application) = fixture();
    world.entity_mut(root).insert(crate::native::canvas_selection::SandSelection(vec![application]));
    DesktopAction::ToggleFocus.apply(&mut world, root);
    frame(&mut world);
    let saved = snapshot(&world, root);
    assert!(serde_json::to_value(&saved).unwrap().as_array().unwrap().iter().all(|entry| entry["active"].is_null() && entry["last"].is_null()));
    world.get_mut::<Workspaces>(root).unwrap().saved_focus = saved;
    world.remove_resource::<FocusState>();
    world.resource_mut::<bevy::input_focus::InputFocus>().clear();
    frame(&mut world);
    assert!(!following(&world, root));
    assert_eq!(world.resource::<bevy::input_focus::InputFocus>().get(), None);
    assert!(world.get::<ApplicationWindow>(application).is_some());
}

#[test]
fn pointer_focus_is_optional_preserves_unfocus_and_has_no_idle_retargeting() {
    let (mut world, root, native, application) = fixture();
    assert!(!hover(&mut world, root, 1, native));
    world.insert_resource(crate::native::shortcuts::configuration::LayoutSettings {
        focus: crate::host::input::FocusPolicy { pointer_follow: true, ..Default::default() },
        ..Default::default()
    });
    assert!(hover(&mut world, root, 1, native));
    let id = world.get::<Managed>(native).unwrap().id;
    for index in 0..120 {
        world.get_mut::<CanvasItem>(application).unwrap().position = DVec2::splat(f64::from(index));
        frame(&mut world);
        assert_eq!(focused(&world, root, 1), Some(id));
    }
    DesktopAction::Unfocus.apply(&mut world, root);
    assert!(!hover(&mut world, root, 1, application));
    assert_eq!(focused(&world, root, 1), None);
    focus_item(&mut world, native);
    assert!(hover(&mut world, root, 1, application));
    assert!(!following(&world, root));
}

#[test]
fn cursor_warp_requires_host_support_and_is_retired_by_new_pointer_input() {
    let (mut world, root, native, application) = fixture();
    world.insert_resource(crate::native::shortcuts::configuration::LayoutSettings {
        focus: crate::host::input::FocusPolicy { warp: true, ..Default::default() },
        ..Default::default()
    });
    let bounds = Rectangle::new([200.0, -300.0], [1000.0, 700.0]).unwrap();
    world.entity_mut(root).insert(HostViewport(OutputViewport { rotation: Default::default(),
        id: OutputId(3), bounds, usable: Some(bounds), physical_size: [1250, 875],
        raster: crate::host::raster::RasterMapping { origin: bounds.origin(), scale: [1.25; 2] }, workspace: 1,
    }));
    DesktopAction::ToggleFocus.apply(&mut world, root);
    assert!(pending_warp(&world).is_none());
    world.insert_resource(CursorWarpSupport(true));
    pointer_epoch(&mut world, 7);
    select(&mut world, root, 1, native, true);
    let request = pending_warp(&world).unwrap();
    assert_eq!(request.pointer_epoch, 7);
    assert_eq!(request.target.owner, root.to_bits());
    assert_eq!(request.target.output, OutputId(3));
    pointer_epoch(&mut world, 7);
    assert_eq!(pending_warp(&world), Some(request));
    pointer_epoch(&mut world, 8);
    assert!(pending_warp(&world).is_none());
    world.resource_mut::<crate::native::shortcuts::configuration::LayoutSettings>().focus.pointer_follow = true;
    assert!(hover(&mut world, root, 1, application));
    assert!(pending_warp(&world).is_none());
}

#[test]
fn following_on_another_spatial_output_never_changes_the_current_output_camera() {
    let (mut world, root, native, _) = fixture();
    let mut second = world.get::<Workspaces>(root).unwrap().entries[0].clone();
    second.id = 2;
    second.desktop = crate::layout::desktop::Workspace::new(WorkspaceLayout::Free);
    second.topology.spatial = true;
    world.get_mut::<Workspaces>(root).unwrap().entries.push(second);
    world.entity_mut(native).insert(WorkspaceMember(2));
    world.entity_mut(root).insert(crate::native::topology::view::View::default());
    let output = |id, workspace, x| {
        let bounds = Rectangle::new([x, 0.0], [1000.0, 700.0]).unwrap();
        OutputViewport { rotation: Default::default(), id: OutputId(id), workspace, bounds, usable: Some(bounds), physical_size: [1000, 700],
            raster: crate::host::raster::RasterMapping { origin: bounds.origin(), scale: [1.0; 2] } }
    };
    let first = output(1, 1, 0.0);
    let second = output(2, 2, 1000.0);
    let mut views = crate::native::output_views::OutputViews::new(vec![first.clone(), second.clone()], OutputId(1)).unwrap();
    views.canvas = Some(root);
    world.insert_resource(views);
    world.entity_mut(root).insert(HostViewport(second));
    crate::native::workspace::switch_output(&mut world, root, 2);
    DesktopAction::ToggleFocus.apply(&mut world, root);
    frame(&mut world);
    world.entity_mut(root).insert(HostViewport(first));
    crate::native::workspace::switch_output(&mut world, root, 1);
    let camera = *world.get::<crate::native::topology::view::View>(root).unwrap();
    let before = world.get::<Workspaces>(root).unwrap().entries[1].topology;
    world.get_mut::<CanvasItem>(native).unwrap().position += DVec2::new(90.0, -75.0);
    frame(&mut world);
    assert_eq!(*world.get::<crate::native::topology::view::View>(root).unwrap(), camera);
    assert_ne!(world.get::<Workspaces>(root).unwrap().entries[1].topology.position, before.position);
    assert!(stamp(&world, root, 2).spatial.is_some());
    assert_eq!(world.get::<Workspaces>(root).unwrap().active, 1);
}

#[test]
fn fresh_free_canvas_focuses_selection_without_a_tiling_round_trip_and_unfocus_keeps_geometry() {
    let (mut world, root, native, application) = fixture();
    let original = [native, application].map(|entity| *world.get::<CanvasItem>(entity).unwrap());
    assert!(world.get::<Managed>(native).is_none());
    DesktopAction::ToggleFocus.apply(&mut world, root);
    frame(&mut world);
    let id = world.get::<Managed>(native).unwrap().id;
    assert_eq!(focused(&world, root, 1), Some(id));
    assert!(following(&world, root));
    assert_eq!(
        world.resource::<bevy::input_focus::InputFocus>().get(),
        Some(native)
    );
    let center = world.get::<CanvasView>(root).unwrap().center;
    DesktopAction::ToggleFocus.apply(&mut world, root);
    frame(&mut world);
    assert_eq!(focused(&world, root, 1), None);
    assert!(!following(&world, root));
    assert_eq!(world.get::<CanvasView>(root).unwrap().center, center);
    assert_eq!(
        world.resource::<bevy::input_focus::InputFocus>().get(),
        None
    );
    for (entity, original) in [native, application].into_iter().zip(original) {
        let item = world.get::<CanvasItem>(entity).unwrap();
        assert_eq!(item.position, original.position);
        assert_eq!(item.size, original.size);
    }
    DesktopAction::ToggleFocus.apply(&mut world, root);
    frame(&mut world);
    assert_eq!(focused(&world, root, 1), Some(id));
}

#[test]
fn ordinary_free_window_keyboard_focus_does_not_enter_explicit_camera_focus() {
    let (mut world, root, _, application) = fixture();
    world
        .entity_mut(root)
        .insert(crate::native::canvas_selection::SandSelection(vec![
            application,
        ]));
    focus_item(&mut world, application);
    assert!(focused(&world, root, 1).is_some());
    assert_eq!(status(&world, root), (false, true));
    DesktopAction::ToggleFocus.apply(&mut world, root);
    frame(&mut world);
    assert_eq!(status(&world, root), (true, true));
    assert!(following(&world, root));
}

#[test]
fn moving_scaled_native_and_application_targets_stay_at_the_reserved_viewport_center() {
    for application_target in [false, true] {
        let (mut world, root, native, application) = fixture();
        let target = if application_target {
            application
        } else {
            native
        };
        world
            .entity_mut(root)
            .insert(crate::native::canvas_selection::SandSelection(vec![target]));
        let bounds = Rectangle::new([0.0; 2], [1000.0, 700.0]).unwrap();
        let usable = Rectangle::new([90.0, 40.0], [910.0, 630.0]).unwrap();
        world.entity_mut(root).insert(HostViewport(OutputViewport { rotation: Default::default(),
            id: OutputId(1),
            bounds,
            usable: Some(usable),
            physical_size: [1000, 700],
            raster: crate::host::raster::RasterMapping {
                origin: (bounds).origin(),
                scale: [1.0; 2],
            },
            workspace: 1,
        }));
        world.get_mut::<CanvasView>(root).unwrap().zoom = 0.6;
        world
            .entity_mut(target)
            .insert(crate::native::area_effects::AreaScale(1.5));
        DesktopAction::ToggleFocus.apply(&mut world, root);
        for index in 0..40 {
            world.get_mut::<CanvasItem>(target).unwrap().position =
                DVec2::new(1250.0 + index as f64 * 17.25, -600.0 + index as f64 * 8.125);
            frame(&mut world);
            let mut displayed = *world.get::<CanvasItem>(target).unwrap();
            displayed.size *= 1.5;
            let view = world.get::<CanvasView>(root).unwrap();
            let top_left = view
                .screen_position(&displayed, Vec2::new(1000.0, 700.0))
                .unwrap();
            let center = top_left + displayed.size * view.zoom as f32 * 0.5;
            assert!(
                center.distance(Vec2::new(545.0, 355.0)) < 0.001,
                "{index}: {center:?}"
            );
            assert_eq!(
                world.get::<CanvasItem>(target).unwrap().size,
                Vec2::new(300.0, 200.0)
            );
            let id = if application_target {
                ItemId::Application(SurfaceId(7))
            } else {
                ItemId::Native(world.get::<Managed>(native).unwrap().id.0)
            };
            let rectangle = world
                .resource::<PublishedGeometry>()
                .0
                .as_ref()
                .unwrap()
                .items
                .iter()
                .find(|item| item.id == id)
                .unwrap()
                .mapping
                .canvas;
            for (axis, expected) in [545.0, 355.0].into_iter().enumerate() {
                assert!(
                    (rectangle.origin()[axis] + rectangle.size()[axis] * 0.5 - expected).abs()
                        < 0.001
                );
            }
        }
    }
}

#[test]
fn manual_camera_motion_releases_follow_without_releasing_item_focus_and_deletion_clears_it() {
    let (mut world, root, native, application) = fixture();
    DesktopAction::ToggleFocus.apply(&mut world, root);
    frame(&mut world);
    let id = world.get::<Managed>(native).unwrap().id;
    world.get_mut::<CanvasView>(root).unwrap().center.x += 25.0;
    let center = world.get::<CanvasView>(root).unwrap().center;
    frame(&mut world);
    assert!(!following(&world, root));
    assert_eq!(focused(&world, root, 1), Some(id));
    assert_eq!(world.get::<CanvasView>(root).unwrap().center, center);
    assert_eq!(
        world.resource::<bevy::input_focus::InputFocus>().get(),
        Some(native)
    );
    DesktopAction::Unfocus.apply(&mut world, root);
    DesktopAction::ToggleFocus.apply(&mut world, root);
    frame(&mut world);
    assert!(following(&world, root));
    world.despawn(native);
    frame(&mut world);
    assert!(!following(&world, root));
    assert_eq!(
        world.resource::<bevy::input_focus::InputFocus>().get(),
        None
    );
    assert_eq!(status(&world, root), (false, true));
    world.despawn(application);
    frame(&mut world);
    assert_eq!(status(&world, root), (false, false));
    DesktopAction::ToggleFocus.apply(&mut world, root);
}

#[test]
fn directional_focus_skips_other_workspaces_and_chrome_and_moves_the_actual_free_or_floating_item()
{
    let (mut world, root, native, application) = fixture();
    DesktopAction::ToggleFocus.apply(&mut world, root);
    let other = world
        .spawn((
            CanvasItem {
                position: DVec2::new(-450.0, 120.0),
                size: Vec2::splat(40.0),
            },
            LayoutBox::new(Vec2::splat(40.0)),
            WorkspaceMember(2),
            ChildOf(root),
        ))
        .id();
    let chrome = world
        .spawn((
            CanvasItem {
                position: DVec2::new(-400.0, 120.0),
                size: Vec2::splat(40.0),
            },
            LayoutBox::new(Vec2::splat(40.0)),
            WorkspaceMember(1),
            crate::native::inspection::InspectionExcluded,
            ChildOf(root),
        ))
        .id();
    DesktopAction::Focus(Direction::Right).apply(&mut world, root);
    frame(&mut world);
    let id = world.get::<Managed>(application).unwrap().id;
    assert_eq!(focused(&world, root, 1), Some(id));
    assert!(following(&world, root));
    world.insert_resource(crate::native::shortcuts::configuration::LayoutSettings {
        move_step: 37,
        ..default()
    });
    let position = world.get::<CanvasItem>(application).unwrap().position;
    DesktopAction::Move(Direction::Left).apply(&mut world, root);
    frame(&mut world);
    assert_eq!(
        world.get::<CanvasItem>(application).unwrap().position,
        position - DVec2::new(37.0, 0.0)
    );
    assert!(following(&world, root));
    DesktopAction::ToggleTiling.apply(&mut world, root);
    frame(&mut world);
    focus_item(&mut world, application);
    DesktopAction::ToggleFloating.apply(&mut world, root);
    frame(&mut world);
    let rows = world.get::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .tiled_rows();
    let position = world.get::<CanvasItem>(application).unwrap().position;
    DesktopAction::Move(Direction::Down).apply(&mut world, root);
    frame(&mut world);
    assert_eq!(
        world.get::<Workspaces>(root).unwrap().entries[0]
            .desktop
            .tiled_rows(),
        rows
    );
    assert_eq!(
        world.get::<CanvasItem>(application).unwrap().position,
        position + DVec2::new(0.0, 37.0)
    );
    assert!(world.get::<Managed>(native).is_some());
    assert_eq!(world.get::<WorkspaceMember>(other).unwrap().0, 2);
    assert!(
        world
            .get::<Managed>(chrome)
            .is_none_or(|managed| focused(&world, root, 1) != Some(managed.id))
    );
}

#[test]
fn width_requires_focus_on_a_fresh_free_sandbox_and_large_focus_keeps_client_size() {
    let (mut world, root, native, _) = fixture();
    let before = *world.get::<CanvasItem>(native).unwrap();
    assert!(
        super::super::CommandContext::new(&world, root)
            .unavailable(DesktopAction::CycleItemWidth)
            .is_some()
    );
    DesktopAction::CycleItemWidth.apply(&mut world, root);
    frame(&mut world);
    assert_eq!(world.get::<CanvasItem>(native).unwrap().size, before.size);
    assert_eq!(
        world.get::<CanvasItem>(native).unwrap().position,
        before.position
    );
    DesktopAction::ToggleFocus.apply(&mut world, root);
    DesktopAction::CycleItemWidth.apply(&mut world, root);
    frame(&mut world);
    assert_eq!(world.get::<CanvasItem>(native).unwrap().size.x, 984.0);
    DesktopAction::Unfocus.apply(&mut world, root);
    world.get_mut::<CanvasItem>(native).unwrap().size = Vec2::new(100000.0, 80000.0);
    world.get_mut::<LayoutBox>(native).unwrap().rules =
        crate::layout::Rules::fixed([100000.0, 80000.0]);
    frame(&mut world);
    DesktopAction::ToggleFocus.apply(&mut world, root);
    frame(&mut world);
    let item = world.get::<CanvasItem>(native).unwrap();
    let view = world.get::<CanvasView>(root).unwrap();
    assert_eq!(item.size, Vec2::new(100000.0, 80000.0));
    assert!(item.size.x * view.zoom as f32 <= 984.001);
    assert!(item.size.y * view.zoom as f32 <= 684.001);
}

#[test]
fn tiled_directional_focus_moves_keyboard_ownership_out_of_the_previous_native_scope() {
    let (mut world, root, native, application) = fixture();
    DesktopAction::ToggleTiling.apply(&mut world, root);
    frame(&mut world);
    focus_item(&mut world, native);
    keyboard_focus(&mut world, native);
    DesktopAction::Focus(Direction::Right).apply(&mut world, root);
    assert_eq!(
        world.resource::<bevy::input_focus::InputFocus>().get(),
        Some(application)
    );
    assert_eq!(
        focused(&world, root, 1),
        Some(world.get::<Managed>(application).unwrap().id)
    );
    let child = world.spawn(ChildOf(application)).id();
    keyboard_focus(&mut world, child);
    DesktopAction::Focus(Direction::Right).apply(&mut world, root);
    assert_eq!(
        world.resource::<bevy::input_focus::InputFocus>().get(),
        Some(child)
    );
}

#[test]
fn spatial_follow_updates_the_view_and_world_camera_together_and_manual_flight_releases_it() {
    let (mut world, root, native, _) = fixture();
    world
        .entity_mut(root)
        .insert(crate::native::topology::view::View {
            spatial: true,
            ..default()
        });
    let camera = world
        .spawn((
            Camera3d::default(),
            crate::native::topology::presentation::WorldCamera,
            Transform::default(),
        ))
        .id();
    world.init_resource::<ButtonInput<KeyCode>>();
    world.get_mut::<CanvasItem>(native).unwrap().size = Vec2::new(100000.0, 80000.0);
    world.get_mut::<LayoutBox>(native).unwrap().rules =
        crate::layout::Rules::fixed([100000.0, 80000.0]);
    DesktopAction::ToggleFocus.apply(&mut world, root);
    let mut reference = None;
    for index in 0..40 {
        crate::native::topology::view::synchronize(&mut world);
        world.get_mut::<CanvasItem>(native).unwrap().position =
            DVec2::new(1000.0 + index as f64 * 15.0, -300.0 + index as f64 * 6.0);
        frame(&mut world);
        crate::native::topology::view::present(&mut world);
        let anchor = crate::native::topology::position(&world, native).unwrap()
            - crate::native::topology::presentation::origin(&world, root);
        let transform = world.get::<Transform>(camera).unwrap();
        let point = transform
            .compute_affine()
            .inverse()
            .transform_point3(anchor.as_vec3());
        let expected = *reference.get_or_insert(point);
        assert!(
            point.distance(expected) < 0.001,
            "{index}: {point:?} != {expected:?}"
        );
    }
    world
        .get_mut::<crate::native::topology::view::View>(root)
        .unwrap()
        .yaw += 0.1;
    frame(&mut world);
    assert!(!following(&world, root));
    assert!(focused(&world, root, 1).is_some());
}
