use super::recent::map;
use super::shortcuts::{capture, settle};
use super::*;
use lince_interface::host::{
    ContentMapping, DesktopAction, ItemId, Manipulation, SurfaceId, WorkspaceLayout,
};
use native::actions::Action;
use native::layout::desktop::manipulation;

#[path = "manipulation/capture.rs"]
mod capture_tests;

#[path = "manipulation/persistence.rs"]
mod persistence;

fn mapping(frame: &Frame, surface: SurfaceId) -> ContentMapping {
    item_mapping(frame, ItemId::Application(surface))
}

fn item_mapping(frame: &Frame, id: ItemId) -> ContentMapping {
    frame
        .geometry
        .as_ref()
        .unwrap()
        .items
        .iter()
        .find(|item| item.id == id)
        .unwrap()
        .mapping
}

#[derive(Resource, Default)]
struct Clicks(Vec<(bevy::picking::pointer::PointerId, Entity)>);

fn point(mapping: ContentMapping, uv: [f64; 2]) -> [f32; 2] {
    mapping
        .canvas_point(std::array::from_fn(|axis| {
            mapping.surface.origin()[axis] + mapping.surface.size()[axis] * uv[axis]
        }))
        .unwrap()
        .map(|value| value as f32)
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn modifier_input_moves_rotated_native_sands_resizes_cancels_and_keeps_normal_context_clicks() {
    use super::shortcuts::key;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for size in [[1000, 700], [360, 700]] {
        for spatial in [false, true] {
            let mut canvas = Presentation::new(
                size,
                directory
                    .path()
                    .join(format!("native-input-{}-{spatial}", size[0])),
                WakeSignal::new(|| {}),
            )
            .unwrap();
            canvas.app.init_resource::<Clicks>().add_observer(
                |event: On<Pointer<Press>>, mut clicks: ResMut<Clicks>| {
                    if event.entity == event.original_event_target() {
                        clicks.0.push((event.pointer_id, event.entity));
                    }
                },
            );
            settle(&mut canvas);
            let root = canvas.canvas_root().unwrap();
            canvas
                .desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free))
                .unwrap();
            assert!(native::workspace_config::set_physics(
                canvas.world_mut(),
                root,
                1,
                false
            ));
            let center = canvas
                .app
                .world()
                .get::<native::canvas::CanvasView>(root)
                .unwrap()
                .center;
            let target = native::sand_store::spawn_sand(
                canvas.world_mut(),
                root,
                1,
                native::sand_store::SandKind::Square,
                "Pointer input",
                center,
            );
            settle(&mut canvas);
            canvas
                .world_mut()
                .entity_mut(target)
                .insert(native::canvas::CanvasItem {
                    position: center,
                    size: Vec2::splat(96.0),
                });
            canvas
                .world_mut()
                .get_mut::<native::layout::LayoutBox>(target)
                .unwrap()
                .rules = lince_interface::layout::Rules::fixed([96.0; 2]);
            let pose = native::topology::Spatial {
                rotation: (bevy::math::DQuat::from_rotation_y(0.3)
                    * bevy::math::DQuat::from_rotation_x(0.25))
                .to_array(),
                depth: Some(8.0),
                ..default()
            };
            canvas.world_mut().entity_mut(target).insert(pose);
            let mut style = native::token_style::overrides(canvas.app.world(), target);
            style.0.insert(
                native::tokens::Token::SandBackground,
                native::tokens::TokenValue::Color([240, 20, 230, 255]),
            );
            native::token_style::set_overrides(canvas.world_mut(), target, style);
            if spatial {
                native::topology::ui::TopologyAction::ToggleView.apply(canvas.world_mut(), root);
            }
            let initial = settle(&mut canvas);
            let id = ItemId::Native(
                canvas
                    .app
                    .world()
                    .get::<native::layout::LayoutBox>(target)
                    .unwrap()
                    .id,
            );
            let initial_mapping = item_mapping(&initial, id);
            let start = point(initial_mapping, [0.5; 2]);
            colored(&initial, size, initial_mapping, [0.5; 2]);
            let batch_end = [start[0] + 20.0, start[1]];
            canvas.pointer(start).unwrap();
            key(&mut canvas, KeyCode::SuperLeft, true);
            canvas
                .button(bevy::input::mouse::MouseButton::Left, true)
                .unwrap();
            canvas.pointer(batch_end).unwrap();
            key(&mut canvas, KeyCode::SuperLeft, false);
            canvas
                .button(bevy::input::mouse::MouseButton::Left, false)
                .unwrap();
            let batch = settle(&mut canvas);
            assert!(!manipulation::active(canvas.app.world()));
            let batch_point = point(item_mapping(&batch, id), [0.5; 2]);
            assert!(
                (batch_point[0] - batch_end[0]).abs() < 1.0
                    && (batch_point[1] - batch_end[1]).abs() < 1.0
            );
            assert!(canvas.app.world().resource::<Clicks>().0.is_empty());
            let original = *canvas
                .app
                .world()
                .get::<native::canvas::CanvasItem>(target)
                .unwrap();
            let start = batch_point;
            canvas.pointer(start).unwrap();
            key(&mut canvas, KeyCode::SuperLeft, true);
            canvas
                .button(bevy::input::mouse::MouseButton::Left, true)
                .unwrap();
            settle(&mut canvas);
            assert!(
                manipulation::active(canvas.app.world()),
                "Modifier press must start native movement at {size:?}/{spatial}"
            );
            assert!(canvas.app.world().resource::<Clicks>().0.is_empty());
            let end = [start[0] + 80.0, start[1]];
            key(&mut canvas, KeyCode::SuperLeft, false);
            canvas.pointer(end).unwrap();
            let preview = settle(&mut canvas);
            capture(
                &format!("native-pointer-move-{}-{spatial}", size[0]),
                size,
                &preview,
            );
            assert!(manipulation::active(canvas.app.world()));
            assert_eq!(
                canvas
                    .app
                    .world()
                    .get::<native::canvas::CanvasItem>(target)
                    .unwrap()
                    .position,
                original.position
            );
            let moved = point(item_mapping(&preview, id), [0.5; 2]);
            assert!(
                (moved[0] - end[0]).abs() < 1.0 && (moved[1] - end[1]).abs() < 1.0,
                "Grab must follow the cursor: {moved:?}/{end:?}"
            );
            canvas
                .button(bevy::input::mouse::MouseButton::Left, false)
                .unwrap();
            let committed = settle(&mut canvas);
            assert!(!manipulation::active(canvas.app.world()));
            let committed_mapping = item_mapping(&committed, id);
            let corner = point(committed_mapping, [0.0; 2]);
            let resize_start = point(committed_mapping, [0.9; 2]);
            let resize_end = point(committed_mapping, [0.9 + 40.0 / 96.0, 0.9 + 25.0 / 96.0]);
            canvas.pointer(resize_start).unwrap();
            key(&mut canvas, KeyCode::SuperLeft, true);
            canvas
                .button(bevy::input::mouse::MouseButton::Right, true)
                .unwrap();
            settle(&mut canvas);
            assert!(manipulation::active(canvas.app.world()));
            key(&mut canvas, KeyCode::SuperLeft, false);
            canvas.pointer(resize_end).unwrap();
            let resized = settle(&mut canvas);
            capture(
                &format!("native-pointer-resize-{}-{spatial}", size[0]),
                size,
                &resized,
            );
            assert!(manipulation::active(canvas.app.world()));
            let opposite = point(item_mapping(&resized, id), [0.0; 2]);
            assert!((opposite[0] - corner[0]).abs() < 1.0 && (opposite[1] - corner[1]).abs() < 1.0);
            assert_eq!(
                canvas
                    .app
                    .world()
                    .get::<native::canvas::CanvasItem>(target)
                    .unwrap()
                    .size,
                Vec2::splat(96.0)
            );
            assert_eq!(
                *canvas
                    .app
                    .world()
                    .get::<native::topology::Spatial>(target)
                    .unwrap(),
                pose
            );
            key(&mut canvas, KeyCode::Escape, true);
            key(&mut canvas, KeyCode::Escape, false);
            settle(&mut canvas);
            assert!(!manipulation::active(canvas.app.world()));
            canvas
                .button(bevy::input::mouse::MouseButton::Right, false)
                .unwrap();
            let cancelled = settle(&mut canvas);
            assert!(canvas.app.world().resource::<Clicks>().0.is_empty());
            assert_eq!(item_mapping(&cancelled, id), committed_mapping);
            canvas.pointer(point(committed_mapping, [0.5; 2])).unwrap();
            canvas
                .button(bevy::input::mouse::MouseButton::Right, true)
                .unwrap();
            settle(&mut canvas);
            canvas
                .button(bevy::input::mouse::MouseButton::Right, false)
                .unwrap();
            settle(&mut canvas);
            let events = &canvas.app.world().resource::<Clicks>().0;
            let content: Vec<_> = events
                .iter()
                .filter(|(pointer, _)| *pointer == native::topology::input::CONTENT_POINTER)
                .collect();
            let world: Vec<_> = events
                .iter()
                .filter(|(pointer, _)| *pointer == bevy::picking::pointer::PointerId::Mouse)
                .collect();
            assert_eq!(
                content.len(),
                1,
                "One content press is required: {events:?}"
            );
            assert_eq!(
                world,
                vec![&(bevy::picking::pointer::PointerId::Mouse, root)],
                "The world press must stay on the canvas root: {events:?}"
            );
            let mut entity = content[0].1;
            while entity != target {
                entity = canvas
                    .app
                    .world()
                    .get::<ChildOf>(entity)
                    .expect("Content press must belong to the selected Sand")
                    .parent();
            }
        }
    }
}

fn colored(frame: &Frame, size: [u32; 2], mapping: ContentMapping, uv: [f64; 2]) -> [f64; 2] {
    let point = mapping
        .canvas_point(std::array::from_fn(|axis| {
            mapping.surface.origin()[axis] + mapping.surface.size()[axis] * uv[axis]
        }))
        .unwrap();
    assert!(
        point
            .iter()
            .enumerate()
            .all(|(axis, point)| *point >= 0.0 && *point < f64::from(size[axis])),
        "Sample outside output: {point:?}"
    );
    let pixels = probe_pixels(frame.dmabuf().unwrap()).unwrap();
    let offset = (point[1] as usize * size[0] as usize + point[0] as usize) * 4;
    let pixel = &pixels[offset..offset + 4];
    assert!(
        pixel[0] > 170 && pixel[1] < 80 && pixel[2] > 170,
        "Application pixels missing at {point:?}: {pixel:?}"
    );
    point
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn transient_application_move_and_resize_render_current_pixels_without_saving_the_preview() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for size in [[1000, 700], [360, 700]] {
        for spatial in [false, true] {
            let mut canvas = Presentation::new(
                size,
                directory
                    .path()
                    .join(format!("assets-{}-{spatial}", size[0])),
                WakeSignal::new(|| {}),
            )
            .unwrap();
            settle(&mut canvas);
            let root = canvas.canvas_root().unwrap();
            canvas
                .desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free))
                .unwrap();
            assert!(native::workspace_config::set_physics(
                canvas.world_mut(),
                root,
                1,
                false
            ));
            let surface = SurfaceId(98);
            canvas
                .open_window(surface, "Generic application", "test.manipulation")
                .unwrap();
            map(&mut canvas, surface, [230, 20, 240, 255]);
            settle(&mut canvas);
            let target = canvas.windows[&surface].entity;
            let placement = canvas
                .app
                .world()
                .get::<native::layout::desktop::Managed>(target)
                .unwrap()
                .id;
            let original = native::canvas::CanvasItem {
                position: canvas
                    .app
                    .world()
                    .get::<native::canvas::CanvasView>(root)
                    .unwrap()
                    .center,
                size: Vec2::splat(96.0),
            };
            canvas.world_mut().entity_mut(target).insert(original);
            canvas
                .world_mut()
                .get_mut::<native::layout::LayoutBox>(target)
                .unwrap()
                .rules = lince_interface::layout::Rules::fixed([96.0; 2]);
            let mut spaces = canvas
                .world_mut()
                .get_mut::<native::workspace::Workspaces>(root)
                .unwrap();
            let space = spaces
                .entries
                .iter_mut()
                .find(|space| space.id == 1)
                .unwrap();
            space.desktop.request_state(
                placement,
                lince_interface::host::WindowRequest {
                    maximized: Some(false),
                    ..default()
                },
            );
            space
                .desktop
                .set_free_geometry(
                    placement,
                    lince_interface::layout::desktop::Geometry {
                        origin: (original.position - bevy::math::DVec2::splat(48.0)).to_array(),
                        size: [96.0; 2],
                    },
                )
                .unwrap();
            drop(spaces);
            if spatial {
                native::topology::ui::TopologyAction::ToggleView.apply(canvas.world_mut(), root);
            }
            let before = settle(&mut canvas);
            let initial = mapping(&before, surface);
            colored(&before, size, initial, [0.5; 2]);
            let saved = canvas
                .app
                .world()
                .get::<native::workspace::Workspaces>(root)
                .unwrap()
                .entries[0]
                .desktop
                .clone();
            assert!(manipulation::begin(
                canvas.world_mut(),
                root,
                target,
                Manipulation::Move,
                [0.5; 2]
            ));
            assert!(manipulation::propose(canvas.world_mut(), [80.0, 0.0], None));
            let moved = settle(&mut canvas);
            capture(
                &format!("manipulation-move-{}-{spatial}", size[0]),
                size,
                &moved,
            );
            assert!(manipulation::active(canvas.app.world()));
            let destination = mapping(&moved, surface);
            let point = colored(&moved, size, destination, [0.9, 0.5]);
            assert!(
                initial.surface_point(point).is_none(),
                "The checked pixels must lie outside the original window"
            );
            assert_eq!(
                canvas
                    .app
                    .world()
                    .get::<native::canvas::CanvasItem>(target)
                    .unwrap()
                    .position,
                original.position
            );
            assert_eq!(
                canvas
                    .app
                    .world()
                    .get::<native::workspace::Workspaces>(root)
                    .unwrap()
                    .entries[0]
                    .desktop,
                saved
            );
            manipulation::cancel(canvas.world_mut());
            let cancelled = settle(&mut canvas);
            assert_eq!(mapping(&cancelled, surface), initial);
            assert!(manipulation::begin(
                canvas.world_mut(),
                root,
                target,
                Manipulation::Resize,
                [1.0; 2]
            ));
            assert!(manipulation::propose(
                canvas.world_mut(),
                [60.0, 30.0],
                None
            ));
            let resized = settle(&mut canvas);
            capture(
                &format!("manipulation-resize-{}-{spatial}", size[0]),
                size,
                &resized,
            );
            let desired = resized
                .windows
                .iter()
                .find(|window| window.surface == surface)
                .unwrap();
            assert_eq!(desired.size, [156, 126]);
            let point = colored(&resized, size, mapping(&resized, surface), [0.9, 0.5]);
            assert!(initial.surface_point(point).is_none());
            assert_eq!(
                canvas
                    .app
                    .world()
                    .get::<native::workspace::Workspaces>(root)
                    .unwrap()
                    .entries[0]
                    .desktop,
                saved
            );
            assert!(manipulation::commit(canvas.world_mut()));
            let committed = settle(&mut canvas);
            colored(&committed, size, mapping(&committed, surface), [0.9, 0.5]);
            assert_eq!(
                canvas
                    .app
                    .world()
                    .get::<native::canvas::CanvasItem>(target)
                    .unwrap()
                    .size,
                Vec2::new(156.0, 126.0)
            );
            assert!(!manipulation::active(canvas.app.world()));
        }
    }
}
