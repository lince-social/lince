use super::*;
use bevy::prelude::*;
use lince_interface::{
    host::{DesktopAction, ItemId, OutputViewport, Rectangle as CanvasRectangle},
    native,
};
use native::actions::Action;
use smithay::backend::allocator::Buffer;
use std::io::Write;

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn overview_renders_inactive_wayland_content_and_advances_only_its_presented_callbacks() {
    use smithay::backend::{
        allocator::Fourcc,
        egl::{EGLContext, EGLDevice, EGLDisplay},
        renderer::{
            Bind, Offscreen,
            gles::{GlesRenderer, GlesTexture},
        },
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut canvas = crate::presentation::Presentation::new(
        [640, 480],
        directory.path().join("assets"),
        lince_interface::host::WakeSignal::new(|| {}),
    )
    .unwrap();
    settle(&mut canvas);
    let root = canvas
        .world_mut()
        .query_filtered::<Entity, With<native::workspace::Workspaces>>()
        .iter(canvas.world_mut())
        .next()
        .unwrap();
    let bounds = CanvasRectangle::new([0.0; 2], [640.0, 480.0]).unwrap();
    canvas
        .viewport(OutputViewport { rotation: Default::default(),
            id: OutputId(1),
            bounds,
            usable: Some(bounds),
            physical_size: [640, 480],
            raster: lince_interface::host::raster::RasterMapping { origin: (bounds).origin(), scale: [1.0; 2] },
            workspace: 1,
        })
        .unwrap();
    canvas
        .desktop_action(DesktopAction::SetLayout(
            lince_interface::host::WorkspaceLayout::Free,
        ))
        .unwrap();
    native::workspace_config::set_physics(canvas.world_mut(), root, 1, false);
    let mut harness = Harness::new();
    let window = harness.window();
    let qh = harness.queue.handle();
    let mut file = tempfile::tempfile().unwrap();
    let content = [0_u8, 255, 0, 255].repeat(320 * 240);
    file.write_all(&content).unwrap();
    let pool = harness.client.shm.as_ref().unwrap().create_pool(
        file.as_fd(),
        content.len() as i32,
        &qh,
        (),
    );
    let buffer = pool.create_buffer(0, 320, 240, 320 * 4, wl_shm::Format::Argb8888, &qh, ());
    window.surface.attach(Some(&buffer), 0, 0);
    window.surface.damage_buffer(0, 0, 320, 240);
    window.surface.commit();
    harness.sync();
    let snapshot = harness.server.windows()[0].clone();
    canvas.sync_windows(&[snapshot.clone()]).unwrap();
    let initial = settle(&mut canvas);
    let requested = initial
        .windows
        .iter()
        .find(|window| window.surface == snapshot.id)
        .map(|window| window.size)
        .unwrap();
    let baseline = harness.client.configures.clone();
    native::workspace::create(canvas.world_mut(), root);
    settle(&mut canvas);
    canvas.desktop_action(DesktopAction::Workspace(2)).unwrap();
    settle(&mut canvas);
    let done = Arc::new(AtomicBool::new(false));
    window.surface.frame(&qh, done.clone());
    window.surface.commit();
    harness.sync();
    canvas.sync_windows(&harness.server.windows()).unwrap();
    canvas
        .desktop_action(DesktopAction::ToggleOverview)
        .unwrap();
    let mut frame = canvas.probe_frame().unwrap();
    for _ in 0..6 {
        assert_eq!(
            frame
                .windows
                .iter()
                .find(|window| window.surface == snapshot.id)
                .map(|window| window.size),
            Some(requested)
        );
        frame = canvas.probe_frame().unwrap();
    }
    assert!(frame.input.keyboard_modal);
    assert_eq!(frame.previewed, vec![snapshot.id]);
    let caption = {
        let world = canvas.world_mut();
        world
            .query::<(Entity, &Text)>()
            .iter(world)
            .find(|(_, text)| text.0 == "A real Wayland window")
            .map(|(entity, _)| entity)
            .expect("The real application title is missing from overview")
    };
    assert!(
        !frame
            .geometry
            .as_ref()
            .unwrap()
            .items
            .iter()
            .any(|item| item.id == ItemId::Application(snapshot.id))
    );
    let green = crate::presentation::probe_pixels(frame.dmabuf().unwrap())
        .unwrap()
        .chunks_exact(4)
        .filter(|pixel| pixel[1] > 200 && pixel[0] < 30 && pixel[2] < 30)
        .count();
    assert!(
        green > 1000,
        "The inactive client thumbnail is missing: {green}"
    );
    assert_eq!(
        frame
            .windows
            .iter()
            .find(|window| window.surface == snapshot.id)
            .map(|window| window.size),
        Some(requested)
    );
    capture("overview-real-client", &frame);
    assert!(!done.load(Ordering::Acquire));
    let completed = crate::runtime::Completed {
        sequence: 1,
        scene_revision: 1,
        frame,
        windows: Arc::new(canvas.painted_windows()),
        rejected: Vec::new(),
        input_serial: 0,
    };
    let device = EGLDevice::enumerate()
        .unwrap()
        .find(|device| !device.is_software())
        .unwrap();
    let display = unsafe { EGLDisplay::new(device) }.unwrap();
    let context = EGLContext::new(&display).unwrap();
    let mut renderer = unsafe { GlesRenderer::new(context) }.unwrap();
    let mut composition =
        crate::desktop::renderer::Composition::new(lince_interface::host::WakeSignal::new(|| {}))
            .unwrap();
    composition.canvas(&mut renderer, &completed.frame).unwrap();
    let mut texture: GlesTexture = renderer
        .create_buffer(Fourcc::Argb8888, (640, 480).into())
        .unwrap();
    let presented = {
        let mut target = renderer.bind(&mut texture).unwrap();
        composition
            .render(
                &mut renderer,
                &mut target,
                (640, 480).into(),
                smithay::utils::Transform::Flipped180,
                &mut harness.server,
                Some(&completed),
                [0.0; 2],
            )
            .unwrap()
    };
    presented.sync.wait().unwrap();
    let callbacks: Vec<_> = presented
        .callbacks
        .iter()
        .filter(|(_, nodes)| nodes.iter().any(|node| node.surface == snapshot.surface))
        .collect();
    assert_eq!(callbacks.len(), 1);
    for (_, nodes) in callbacks {
        harness.server.presented_nodes(nodes);
    }
    harness.sync();
    assert!(done.load(Ordering::Acquire));
    assert_eq!(harness.client.configures, baseline);
    window.role.set_title("Renamed Wayland window".into());
    harness.sync();
    canvas.sync_windows(&harness.server.windows()).unwrap();
    settle(&mut canvas);
    assert_eq!(
        canvas.world_mut().get::<Text>(caption).unwrap().0,
        "Renamed Wayland window"
    );
    canvas.activate_application(snapshot.id).unwrap();
    let frame = settle(&mut canvas);
    assert!(frame.previewed.is_empty());
    assert!(!frame.input.keyboard_modal);
    assert_eq!(
        frame.geometry.as_ref().unwrap().focused,
        Some(ItemId::Application(snapshot.id))
    );
}

fn settle(canvas: &mut crate::presentation::Presentation) -> crate::presentation::Frame {
    let mut frame = canvas.probe_frame().unwrap();
    for _ in 0..5 {
        frame = canvas.probe_frame().unwrap();
    }
    frame
}

fn green_center(frame: &crate::presentation::Frame) -> [f64; 2] {
    let pixels = crate::presentation::probe_pixels(frame.dmabuf().unwrap()).unwrap();
    let size = frame.dmabuf().unwrap().size();
    let width = size.w as usize;
    assert_eq!(pixels.len(), width * size.h as usize * 4);
    let mut total = [0_u64; 3];
    for (index, pixel) in pixels.chunks_exact(4).enumerate() {
        if pixel[1] > 200 && pixel[0] < 30 && pixel[2] < 30 {
            total[0] += (index % width) as u64;
            total[1] += (index / width) as u64;
            total[2] += 1;
        }
    }
    assert!(total[2] > 1000, "Client pixels disappeared: {total:?}");
    [
        total[0] as f64 / total[2] as f64,
        total[1] as f64 / total[2] as f64,
    ]
}

pub(super) fn capture(name: &str, frame: &crate::presentation::Frame) {
    let Some(directory) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    let pixels = crate::presentation::probe_pixels(frame.dmabuf().unwrap()).unwrap();
    let mut file = std::fs::File::create(directory.join(format!("{name}.ppm"))).unwrap();
    let size = frame.dmabuf().unwrap().size();
    file.write_all(format!("P6\n{} {}\n255\n", size.w, size.h).as_bytes())
        .unwrap();
    for pixel in pixels.chunks_exact(4) {
        file.write_all(&pixel[..3]).unwrap();
    }
}

fn toggle_with_keyboard(canvas: &mut crate::presentation::Presentation) {
    use bevy::input::keyboard::{Key, KeyCode};
    for (physical, logical, pressed) in [
        (KeyCode::SuperLeft, Key::Super, true),
        (KeyCode::Backspace, Key::Backspace, true),
        (KeyCode::Backspace, Key::Backspace, false),
        (KeyCode::SuperLeft, Key::Super, false),
    ] {
        canvas.key(physical, logical, None, pressed, false).unwrap();
        settle(canvas);
    }
}

fn focus_with_pointer(canvas: &mut crate::presentation::Presentation) {
    let world = canvas.world_mut();
    let button = world
        .query::<(Entity, &Children, &native::actions::ActionButton)>()
        .iter(world)
        .find(|(_, children, _)| {
            children.iter().any(|child| {
                world
                    .get::<Text>(child)
                    .is_some_and(|text| text.0 == lince_interface::i18n::Token::FocusTarget.text())
            })
        })
        .unwrap()
        .0;
    let point = world
        .get::<UiGlobalTransform>(button)
        .unwrap()
        .transform_point2(Vec2::ZERO);
    assert!(
        point.x >= 0.0 && point.x < 940.0 && point.y >= 0.0 && point.y < 630.0,
        "{point:?}"
    );
    canvas.pointer(point.to_array()).unwrap();
    settle(canvas);
    canvas
        .button(bevy::input::mouse::MouseButton::Left, true)
        .unwrap();
    settle(canvas);
    canvas
        .button(bevy::input::mouse::MouseButton::Left, false)
        .unwrap();
    settle(canvas);
}

fn controls_stay_in_the_toolbar(canvas: &mut crate::presentation::Presentation) {
    let world = canvas.world_mut();
    for caption in ["2D / 3D", "Floor"] {
        let button = world
            .query::<(&Text, &ChildOf)>()
            .iter(world)
            .find(|(text, _)| text.0 == caption)
            .unwrap()
            .1
            .parent();
        let point = world
            .get::<UiGlobalTransform>(button)
            .unwrap()
            .transform_point2(Vec2::ZERO);
        if point.y <= 570.0 {
            let mut ancestor = Some(button);
            for _ in 0..8 {
                let Some(entity) = ancestor else { break };
                let names: Vec<_> = world
                    .get_entity(entity)
                    .unwrap()
                    .archetype()
                    .components()
                    .iter()
                    .filter_map(|id| world.components().get_info(*id))
                    .map(|info| info.name().to_string())
                    .filter(|name| {
                        name.contains("Toolbar")
                            || name.contains("GhostNode")
                            || name.contains("UiTargetCamera")
                    })
                    .collect();
                eprintln!(
                    "Control ancestor {entity:?}: kinds {names:?}, camera {:?}, node {:?}, size {:?}, position {:?}",
                    world
                        .get::<ComputedUiTargetCamera>(entity)
                        .and_then(ComputedUiTargetCamera::get),
                    world.get::<Node>(entity).map(|node| (
                        node.display,
                        node.position_type,
                        node.width,
                        node.height,
                        node.right,
                        node.bottom
                    )),
                    world.get::<ComputedNode>(entity).map(ComputedNode::size),
                    world
                        .get::<UiGlobalTransform>(entity)
                        .map(|transform| transform.transform_point2(Vec2::ZERO))
                );
                ancestor = world.get::<ChildOf>(entity).map(ChildOf::parent);
            }
        }
        assert!(
            point.x >= 0.0 && point.x < 940.0 && point.y > 570.0 && point.y < 630.0,
            "{caption} left the toolbar: {point:?}"
        );
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn native_and_real_wayland_targets_follow_without_rendered_frame_lag_or_client_resizing() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut canvas = crate::presentation::Presentation::new(
        [1000, 700],
        directory.path().join("assets"),
        lince_interface::host::WakeSignal::new(|| {}),
    )
    .unwrap();
    settle(&mut canvas);
    let root = canvas
        .world_mut()
        .query_filtered::<Entity, With<native::workspace::Workspaces>>()
        .iter(canvas.world_mut())
        .next()
        .unwrap();
    let bounds = CanvasRectangle::new([0.0; 2], [1000.0, 700.0]).unwrap();
    let usable = CanvasRectangle::new([60.0, 40.0], [940.0, 630.0]).unwrap();
    canvas
        .viewport(OutputViewport { rotation: Default::default(),
            id: OutputId(1),
            bounds,
            usable: Some(usable),
            physical_size: [1000, 700],
            raster: lince_interface::host::raster::RasterMapping { origin: (bounds).origin(), scale: [1.0; 2] },
            workspace: 1,
        })
        .unwrap();
    native::workspace_config::set_physics(canvas.world_mut(), root, 1, false);
    canvas.desktop_action(DesktopAction::ToggleTiling).unwrap();
    native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
    native::edit_mode::EditAction::AddSand(native::sand_store::SandKind::Square)
        .apply(canvas.world_mut(), root);
    settle(&mut canvas);
    native::edit_mode::EditAction::ToggleAlwaysShowControls.apply(canvas.world_mut(), root);
    native::edit_mode::EditAction::Close.apply(canvas.world_mut(), root);
    let native = canvas
        .world_mut()
        .query::<(Entity, &native::sand_store::StoredSand)>()
        .iter(canvas.world_mut())
        .find(|(_, sand)| sand.kind == native::sand_store::SandKind::Square)
        .unwrap()
        .0;
    let mut harness = Harness::new();
    let window = harness.window();
    let qh = harness.queue.handle();
    let mut file = tempfile::tempfile().unwrap();
    let pixels = [0_u8, 255, 0, 255].repeat(320 * 240);
    file.write_all(&pixels).unwrap();
    let pool = harness.client.shm.as_ref().unwrap().create_pool(
        file.as_fd(),
        pixels.len() as i32,
        &qh,
        (),
    );
    let buffer = pool.create_buffer(0, 320, 240, 320 * 4, wl_shm::Format::Argb8888, &qh, ());
    window.surface.attach(Some(&buffer), 0, 0);
    window.surface.damage_buffer(0, 0, 320, 240);
    window.surface.commit();
    harness.sync();
    canvas.sync_windows(&harness.server.windows()).unwrap();
    settle(&mut canvas);
    let (application, surface) = canvas
        .world_mut()
        .query::<(Entity, &native::layout::desktop::ApplicationWindow)>()
        .iter(canvas.world_mut())
        .map(|(entity, window)| (entity, window.surface))
        .next()
        .unwrap();
    for spatial in [false, true] {
        for (target, application_target) in [(native, false), (application, true)] {
            canvas.desktop_action(DesktopAction::Unfocus).unwrap();
            let other = if application_target {
                native
            } else {
                application
            };
            canvas
                .world_mut()
                .get_mut::<native::canvas::CanvasItem>(other)
                .unwrap()
                .position = bevy::math::DVec2::new(100000.0, -100000.0);
            canvas
                .world_mut()
                .get_mut::<native::canvas::CanvasItem>(target)
                .unwrap()
                .position = bevy::math::DVec2::new(200.0, 200.0);
            canvas
                .world_mut()
                .get_mut::<native::topology::view::View>(root)
                .unwrap()
                .spatial = spatial;
            canvas
                .world_mut()
                .get_mut::<native::canvas::CanvasView>(root)
                .unwrap()
                .zoom = 0.6;
            canvas
                .world_mut()
                .entity_mut(root)
                .insert(native::canvas_selection::SandSelection(vec![target]));
            canvas
                .world_mut()
                .entity_mut(target)
                .insert(native::area_effects::AreaScale(1.25));
            canvas.desktop_action(DesktopAction::ToggleFocus).unwrap();
            let frame = settle(&mut canvas);
            controls_stay_in_the_toolbar(&mut canvas);
            if application_target && !spatial {
                capture("follow-flat-client-before", &frame);
            }
            assert!(native::layout::desktop::focus::following(
                canvas.world_mut(),
                root
            ));
            let native_id = canvas
                .world_mut()
                .get::<native::layout::LayoutBox>(target)
                .unwrap()
                .id;
            let id = if application_target {
                ItemId::Application(surface)
            } else {
                ItemId::Native(native_id)
            };
            let initial_size = canvas
                .world_mut()
                .get::<native::canvas::CanvasItem>(target)
                .unwrap()
                .size;
            let mut reference = None;
            let mut pixels = application_target.then(|| green_center(&frame));
            for index in 0..16 {
                let position = bevy::math::DVec2::new(
                    1000.0 + (index * index) as f64 * 11.125,
                    -600.0 + (index as f64 * 0.7).sin() * 90.0,
                );
                canvas
                    .world_mut()
                    .get_mut::<native::canvas::CanvasItem>(target)
                    .unwrap()
                    .position = position;
                let frame = canvas.probe_frame().unwrap();
                if application_target && !spatial && index == 3 {
                    capture("follow-flat-client-frame-3", &frame);
                }
                let geometry = frame.geometry.as_ref().unwrap();
                assert_eq!(geometry.focused, Some(id));
                let mapping = geometry
                    .items
                    .iter()
                    .find(|item| item.id == id)
                    .unwrap()
                    .mapping;
                let points = mapping.corners.unwrap_or_else(|| {
                    let origin = mapping.canvas.origin();
                    let size = mapping.canvas.size();
                    [
                        origin,
                        [origin[0] + size[0], origin[1]],
                        [origin[0] + size[0], origin[1] + size[1]],
                        [origin[0], origin[1] + size[1]],
                    ]
                });
                let expected = *reference.get_or_insert(points);
                for (actual, expected) in points.into_iter().zip(expected) {
                    for axis in 0..2 {
                        assert!(
                            (actual[axis] - expected[axis]).abs() < 0.05,
                            "Frame {index}, spatial {spatial}, application {application_target}: {actual:?} != {expected:?}"
                        );
                    }
                }
                assert_eq!(
                    canvas
                        .world_mut()
                        .get::<native::canvas::CanvasItem>(target)
                        .unwrap()
                        .size,
                    initial_size
                );
                if application_target {
                    let current = green_center(&frame);
                    let expected = pixels.get_or_insert(current);
                    for axis in 0..2 {
                        assert!(
                            (current[axis] - expected[axis]).abs() < 0.6,
                            "Client pixels lagged in frame {index}, spatial {spatial}: {current:?} != {expected:?}"
                        );
                    }
                    let configured = frame
                        .windows
                        .iter()
                        .find(|window| window.surface == surface)
                        .unwrap();
                    assert_eq!(
                        configured.size,
                        [initial_size.x.round() as u32, initial_size.y.round() as u32]
                    );
                }
                if index == 15 {
                    capture(
                        &format!(
                            "follow-{}-{}",
                            if spatial { "spatial" } else { "flat" },
                            if application_target {
                                "client"
                            } else {
                                "native"
                            }
                        ),
                        &frame,
                    );
                }
            }
            toggle_with_keyboard(&mut canvas);
            assert!(
                settle(&mut canvas)
                    .geometry
                    .as_ref()
                    .unwrap()
                    .focused
                    .is_none()
            );
            focus_with_pointer(&mut canvas);
            assert_eq!(
                settle(&mut canvas).geometry.as_ref().unwrap().focused,
                Some(id)
            );
            assert!(native::layout::desktop::focus::following(
                canvas.world_mut(),
                root
            ));
            canvas.desktop_action(DesktopAction::Unfocus).unwrap();
        }
    }
    canvas
        .world_mut()
        .entity_mut(root)
        .insert(native::canvas_selection::SandSelection(vec![application]));
    canvas.desktop_action(DesktopAction::ToggleFocus).unwrap();
    settle(&mut canvas);
    assert!(native::layout::desktop::focus::following(
        canvas.world_mut(),
        root
    ));
    window.surface.attach(None, 0, 0);
    window.surface.commit();
    harness.sync();
    canvas.sync_windows(&harness.server.windows()).unwrap();
    let frame = settle(&mut canvas);
    assert!(!native::layout::desktop::focus::following(
        canvas.world_mut(),
        root
    ));
    assert!(
        !frame
            .geometry
            .as_ref()
            .unwrap()
            .items
            .iter()
            .any(|item| item.id == ItemId::Application(surface))
    );
    window.surface.commit();
    harness.sync();
    harness.sync();
    window.surface.attach(Some(&buffer), 0, 0);
    window.surface.commit();
    harness.sync();
    canvas.sync_windows(&harness.server.windows()).unwrap();
    settle(&mut canvas);
    assert!(!native::layout::desktop::focus::following(
        canvas.world_mut(),
        root
    ));
    canvas.finish().unwrap();
}
