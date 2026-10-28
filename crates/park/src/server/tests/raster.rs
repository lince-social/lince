use super::*;
use bevy::prelude::*;
use lince_interface::{
    host::{DesktopAction, Rectangle as CanvasRectangle, WorkspaceLayout},
    native,
};
use smithay::backend::{
    allocator::Fourcc,
    egl::{EGLContext, EGLDevice, EGLDisplay},
    renderer::{
        Bind, ExportMem, Frame as _, Offscreen, Renderer,
        element::{Element, RenderElement},
        gles::{GlesRenderer, GlesTexture},
    },
};
use std::io::Write;

fn buffer(harness: &Harness, size: [i32; 2], pattern: bool) -> wl_buffer::WlBuffer {
    let mut bytes = Vec::with_capacity((size[0] * size[1] * 4) as usize);
    for _ in 0..size[1] {
        for x in 0..size[0] {
            let value = if pattern && x % 2 == 0 { 255 } else { 0 };
            bytes.extend_from_slice(&[value, value, value, 255]);
        }
    }
    pixel_buffer(harness, size, &bytes)
}

fn pixel_buffer(harness: &Harness, size: [i32; 2], bytes: &[u8]) -> wl_buffer::WlBuffer {
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(bytes).unwrap();
    let qh = harness.queue.handle();
    let pool =
        harness
            .client
            .shm
            .as_ref()
            .unwrap()
            .create_pool(file.as_fd(), bytes.len() as i32, &qh, ());
    let buffer = pool.create_buffer(
        0,
        size[0],
        size[1],
        size[0] * 4,
        wl_shm::Format::Argb8888,
        &qh,
        (),
    );
    pool.destroy();
    buffer
}

fn settle(canvas: &mut crate::presentation::Presentation) -> crate::presentation::Frame {
    for _ in 0..5 {
        canvas.probe_frame().unwrap();
    }
    canvas.probe_frame().unwrap()
}

fn publish(canvas: &mut crate::presentation::Presentation, harness: &Harness) {
    canvas
        .outputs(
            harness
                .server
                .output_viewports()
                .into_iter()
                .map(|(name, viewport)| crate::presentation::Output { name, viewport })
                .collect(),
        )
        .unwrap();
}

fn pixels(renderer: &mut GlesRenderer, texture: &mut GlesTexture, size: [u32; 2]) -> Vec<u8> {
    let mapping = {
        let target = renderer.bind(texture).unwrap();
        renderer
            .copy_framebuffer(
                &target,
                smithay::utils::Rectangle::from_size((size[0] as i32, size[1] as i32).into()),
                Fourcc::Abgr8888,
            )
            .unwrap()
    };
    renderer.map_texture(&mapping).unwrap().to_vec()
}

fn capture(name: &str, size: [u32; 2], pixels: &[u8]) {
    let Some(directory) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    let mut bytes = format!("P6\n{} {}\n255\n", size[0], size[1]).into_bytes();
    for pixel in pixels.chunks_exact(4) {
        bytes.extend_from_slice(&pixel[..3]);
    }
    std::fs::write(directory.join(format!("raster-{name}.ppm")), bytes).unwrap();
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn scaled_outputs_preserve_native_pixels_client_geometry_and_logical_editor_input_through_final_composition()
 {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut canvas = crate::presentation::Presentation::new(
        [800, 600],
        directory.path().join("assets"),
        lince_interface::host::WakeSignal::new(|| {}),
    )
    .unwrap();
    settle(&mut canvas);
    let root = canvas
        .world_mut()
        .query_filtered::<Entity, With<native::workspace::Workspaces>>()
        .single(canvas.world_mut())
        .unwrap();
    let mut monitors = [
        Monitor {
            id: OutputId(1),
            name: "Raster-A".into(),
            origin: [50, 20],
            size: [800, 600],
        },
        Monitor {
            id: OutputId(2),
            name: "Raster-B".into(),
            origin: [1200, 50],
            size: [640, 480],
        },
    ];
    let mut harness = Harness::new();
    harness
        .server
        .configure_scaled_outputs(
            &monitors,
            &BTreeMap::from([(OutputId(1), 1.0), (OutputId(2), 1.5)]),
        )
        .unwrap();
    harness.sync();
    publish(&mut canvas, &harness);
    canvas
        .desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free))
        .unwrap();
    canvas.desktop_action(DesktopAction::Unfocus).unwrap();
    native::workspace_config::set_physics(canvas.world_mut(), root, 1, false);
    let qh = harness.queue.handle();
    let bar_surface = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let bar = harness
        .client
        .layer_shell
        .as_ref()
        .unwrap()
        .get_layer_surface(
            &bar_surface,
            None,
            zwlr_layer_shell_v1::Layer::Top,
            "Raster reservation".into(),
            &qh,
            (),
        );
    bar.set_anchor(
        zwlr_layer_surface_v1::Anchor::Top
            | zwlr_layer_surface_v1::Anchor::Left
            | zwlr_layer_surface_v1::Anchor::Right,
    );
    bar.set_size(0, 32);
    bar.set_exclusive_zone(32);
    bar_surface.commit();
    harness.sync();
    let bar_buffer = buffer(&harness, [800, 32], false);
    bar_surface.attach(Some(&bar_buffer), 0, 0);
    bar_surface.commit();
    harness.sync();
    let window = harness.window();
    window.role.set_min_size(160, 120);
    window.role.set_max_size(160, 120);
    let viewport =
        harness
            .client
            .viewporter
            .as_ref()
            .unwrap()
            .get_viewport(&window.surface, &qh, ());
    viewport.set_destination(160, 120);
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
    let mut retained = vec![bar_buffer];
    let field = {
        let world = canvas.world_mut();
        let bundle = native::sand::single_line_editor(
            "Draft",
            world.resource::<native::theme::Typography>(),
            0,
            2048,
        );
        world.spawn(bundle).id()
    };
    canvas.world_mut().entity_mut(field).insert((
        Node {
            position_type: PositionType::Absolute,
            left: px(16),
            top: px(16),
            width: px(180),
            height: px(36),
            padding: UiRect::all(px(6)),
            border: UiRect::all(px(1)),
            overflow: Overflow::clip(),
            ..default()
        },
        GlobalZIndex(99),
        native::inspection::InspectionExcluded,
        ChildOf(root),
    ));
    let mut placement = None;
    let (sender, receiver) = std::sync::mpsc::sync_channel(16);
    canvas.host_requests(sender);
    for (step, (scale, mode)) in [
        (1.0, [800, 600]),
        (1.25, [800, 600]),
        (1.5, [803, 601]),
        (2.0, [801, 601]),
        (4.0, [801, 601]),
    ]
    .into_iter()
    .enumerate()
    {
        monitors[0].size = mode;
        let raster_size = mode.map(|value| value as u32);
        eprintln!("Raster test: scale {scale}, mode {mode:?}");
        harness
            .server
            .configure_scaled_outputs(
                &monitors,
                &BTreeMap::from([(OutputId(1), scale), (OutputId(2), 1.5)]),
            )
            .unwrap();
        harness.sync();
        let reservation = if step == 1 { 33 } else { 32 };
        bar.set_size(0, reservation);
        bar.set_exclusive_zone(reservation as i32);
        let width = harness
            .server
            .output_viewports()
            .into_iter()
            .find(|(_, output)| output.id == OutputId(1))
            .unwrap()
            .1
            .bounds
            .size()[0] as i32;
        let resized_bar = buffer(&harness, [width, reservation as i32], false);
        bar_surface.attach(Some(&resized_bar), 0, 0);
        bar_surface.commit();
        retained.push(resized_bar);
        let client = buffer(
            &harness,
            [(160.0 * scale) as i32, (120.0 * scale) as i32],
            true,
        );
        window.surface.attach(Some(&client), 0, 0);
        window.surface.damage_buffer(0, 0, i32::MAX, i32::MAX);
        window.surface.commit();
        retained.push(client);
        harness.sync();
        publish(&mut canvas, &harness);
        canvas.focus_output(OutputId(1)).unwrap();
        assert!(!canvas.sync_windows(&harness.server.windows()).unwrap());
        let frame = settle(&mut canvas);
        let snapshot = &harness.server.windows()[0];
        let entity = canvas
            .world_mut()
            .query::<(Entity, &native::layout::desktop::ApplicationWindow)>()
            .iter(canvas.world_mut())
            .find(|(_, item)| item.surface == snapshot.id)
            .unwrap()
            .0;
        let id = canvas
            .world_mut()
            .get::<native::layout::LayoutBox>(entity)
            .unwrap()
            .id;
        assert_eq!(*placement.get_or_insert(id), id);
        let center = canvas
            .world_mut()
            .get::<native::canvas::CanvasItem>(entity)
            .unwrap()
            .position;
        *canvas
            .world_mut()
            .get_mut::<native::canvas::CanvasView>(root)
            .unwrap() = native::canvas::CanvasView { center, zoom: 1.0 };
        assert_eq!(
            frame
                .windows
                .iter()
                .find(|item| item.surface == snapshot.id)
                .unwrap()
                .size,
            [160, 120]
        );
        for focused in [OutputId(1), OutputId(2)] {
            canvas.focus_output(focused).unwrap();
            let frame = settle(&mut canvas);
            let native_buffer = if frame.output == Some(OutputId(1)) {
                frame.dmabuf().unwrap()
            } else {
                frame
                    .outputs
                    .iter()
                    .find(|output| output.id == OutputId(1))
                    .unwrap()
                    .dmabuf()
            };
            use smithay::backend::allocator::Buffer;
            assert_eq!(native_buffer.size().w, mode[0]);
            let output = harness
                .server
                .output_viewports()
                .into_iter()
                .find(|(_, output)| output.id == OutputId(1))
                .unwrap()
                .1;
            let (offset, extent) = output.physical_region(output.usable.unwrap()).unwrap();
            assert_eq!(native_buffer.size().h, extent[1] as i32);
            let native_pixels = crate::presentation::probe_pixels(native_buffer).unwrap();
            let mut completed = crate::runtime::Completed {
                sequence: 1,
                scene_revision: 1,
                frame,
                windows: Arc::new(canvas.painted_windows()),
                rejected: Vec::new(),
                input_serial: 0,
            };
            composition.canvas(&mut renderer, &completed.frame).unwrap();
            let mut texture: GlesTexture = renderer
                .create_buffer(Fourcc::Argb8888, (mode[0], mode[1]).into())
                .unwrap();
            let expected = harness.server.layers();
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            let scene = loop {
                let mut target = renderer.bind(&mut texture).unwrap();
                let scene = composition
                    .render_output(
                        &mut renderer,
                        &mut target,
                        &output,
                        &mut harness.server,
                        Some(&completed),
                        [-100.0; 2],
                        Default::default(),
                    )
                    .unwrap();
                scene.sync.wait().unwrap();
                if expected
                    .iter()
                    .filter(|layer| layer.output == output.id)
                    .all(|layer| {
                        scene.layers.iter().any(|actual| {
                            actual.surface == layer.surface
                                && layer.nodes.iter().all(|node| {
                                    actual.nodes.iter().any(|current| {
                                        current.surface == node.surface
                                            && current.content.version == node.content.version
                                    })
                                })
                        })
                    })
                {
                    break scene;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "The actual layer buffers must become ready before inspecting their pixels"
                );
                std::thread::yield_now();
            };
            scene.sync.wait().unwrap();
            let final_pixels = pixels(&mut renderer, &mut texture, raster_size);
            let element =
                crate::desktop::renderer::output_element(&renderer, texture.clone(), scale);
            let geometry = element.geometry(scale.into());
            assert_eq!(geometry.size, (mode[0], mode[1]).into());
            let mut scanout: GlesTexture = renderer
                .create_buffer(Fourcc::Argb8888, (mode[0], mode[1]).into())
                .unwrap();
            {
                let mut target = renderer.bind(&mut scanout).unwrap();
                let mut frame = renderer
                    .render(
                        &mut target,
                        (mode[0], mode[1]).into(),
                        smithay::utils::Transform::Normal,
                    )
                    .unwrap();
                RenderElement::<GlesRenderer>::draw(
                    &element,
                    &mut frame,
                    element.src(),
                    geometry,
                    &[smithay::utils::Rectangle::from_size(geometry.size)],
                    &[],
                )
                .unwrap();
                frame.finish().unwrap().wait().unwrap();
            }
            assert_eq!(
                pixels(&mut renderer, &mut scanout, raster_size),
                final_pixels,
                "The physical output element must preserve the mode-sized texture"
            );
            capture(&format!("{step}-{}", focused.0), raster_size, &final_pixels);
            let mut changed = 0;
            for y in 0..extent[1] as usize {
                for x in 0..extent[0] as usize {
                    let source = (y * extent[0] as usize + x) * 4;
                    let destination =
                        (((y + offset[1] as usize) * mode[0] as usize) + x + offset[0] as usize)
                            * 4;
                    if native_pixels[source..source + 3]
                        != final_pixels[destination..destination + 3]
                    {
                        changed += 1;
                    }
                }
            }
            assert_eq!(
                changed, 0,
                "Scale {scale}, focused output {} must preserve every native raster pixel",
                focused.0
            );
            let bar_width = output.raster.pixel_region(output.bounds).unwrap().1[0]
                .min(raster_size[0]) as usize;
            assert!(
                final_pixels
                    .chunks_exact(mode[0] as usize * 4)
                    .take(offset[1] as usize)
                    .all(|row| row[..bar_width * 4]
                        .chunks_exact(4)
                        .all(|pixel| pixel[..3] == [0; 3]))
            );
            if step == 1 {
                let item = completed
                    .frame
                    .geometry
                    .as_ref()
                    .unwrap()
                    .items
                    .iter()
                    .find(|item| item.id == lince_interface::host::ItemId::Application(snapshot.id))
                    .unwrap();
                let origin = output.raster.point(item.mapping.canvas.origin());
                assert!((origin[0] - origin[0].round()).abs() < 0.001, "{origin:?}");
                for x in 16..64 {
                    let index = (((origin[1].round() as usize + 30) * mode[0] as usize)
                        + origin[0].round() as usize
                        + x)
                        * 4;
                    let expected = if x % 2 == 0 { 255 } else { 0 };
                    assert_eq!(
                        &final_pixels[index..index + 3],
                        &[expected; 3],
                        "Fractional-scale client pixels must not pass through a differently sized capture bucket"
                    );
                }
            }
            harness.server.presented_outputs(
                completed.frame.geometry.as_ref().unwrap(),
                completed.frame.output,
            );
            let positioner = harness
                .client
                .shell
                .as_ref()
                .unwrap()
                .create_positioner(&qh, ());
            positioner.set_size(32, 16);
            positioner.set_anchor_rect(144, 8, 1, 1);
            positioner.set_anchor(xdg_positioner::Anchor::BottomRight);
            positioner.set_gravity(xdg_positioner::Gravity::BottomRight);
            let popup_surface = harness
                .client
                .compositor
                .as_ref()
                .unwrap()
                .create_surface(&qh, ());
            let xdg =
                harness
                    .client
                    .shell
                    .as_ref()
                    .unwrap()
                    .get_xdg_surface(&popup_surface, &qh, ());
            let popup = xdg.get_popup(Some(&window.xdg), &positioner, &qh, ());
            popup_surface.commit();
            harness.sync();
            let popup_buffer = pixel_buffer(&harness, [32, 16], &[0, 255, 0, 255].repeat(32 * 16));
            popup_surface.attach(Some(&popup_buffer), 0, 0);
            popup_surface.commit();
            harness.sync();
            completed.windows = Arc::new(harness.server.windows());
            let expected = completed.windows[0].popups[0].clone();
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            let popup_scene = loop {
                let mut target = renderer.bind(&mut texture).unwrap();
                let scene = composition
                    .render_output(
                        &mut renderer,
                        &mut target,
                        &output,
                        &mut harness.server,
                        Some(&completed),
                        [-100.0; 2],
                        Default::default(),
                    )
                    .unwrap();
                scene.sync.wait().unwrap();
                if let Some(popup) = scene.popups.into_iter().find(|popup| {
                    popup.surface == expected.surface
                        && expected.nodes.iter().all(|node| {
                            popup.nodes.iter().any(|actual| {
                                actual.surface == node.surface
                                    && actual.content.version == node.content.version
                            })
                        })
                }) {
                    break popup;
                }
                assert!(std::time::Instant::now() < deadline);
                std::thread::yield_now();
            };
            let parent = completed
                .frame
                .geometry
                .as_ref()
                .unwrap()
                .items
                .iter()
                .find(|item| item.id == lince_interface::host::ItemId::Application(snapshot.id))
                .unwrap()
                .mapping
                .canvas;
            let popup_point = std::array::from_fn(|axis| {
                popup_scene.mapping.canvas.origin()[axis]
                    + popup_scene.mapping.canvas.size()[axis] * if axis == 0 { 0.9 } else { 0.5 }
            });
            assert!(output.bounds.contains(popup_point));
            assert!(!parent.contains(popup_point));
            assert_eq!(popup_scene.hit(popup_point), Some(expected.surface));
            let point = output.raster.point(popup_point);
            let popup_pixels = pixels(&mut renderer, &mut texture, raster_size);
            let index = ((point[1] as usize * mode[0] as usize) + point[0] as usize) * 4;
            assert_eq!(&popup_pixels[index..index + 3], &[0, 255, 0]);
            capture(
                &format!("{step}-{}-popup", focused.0),
                raster_size,
                &popup_pixels,
            );
            let pointer = [
                output.bounds.origin()[0] + 12.0,
                output.bounds.origin()[1] + 8.0,
            ];
            {
                let mut target = renderer.bind(&mut texture).unwrap();
                composition
                    .render_output(
                        &mut renderer,
                        &mut target,
                        &output,
                        &mut harness.server,
                        Some(&completed),
                        pointer,
                        Default::default(),
                    )
                    .unwrap()
                    .sync
                    .wait()
                    .unwrap();
            }
            let with_cursor = pixels(&mut renderer, &mut texture, raster_size);
            for y in 0..offset[1] as usize {
                for x in 0..bar_width {
                    let white = [([12.0, 8.0], [4.0, 20.0]), ([12.0, 8.0], [14.0, 4.0])]
                        .into_iter()
                        .any(|(origin, size)| {
                            (0..2).all(|axis| {
                                let coordinate = if axis == 0 { x } else { y };
                                coordinate >= (origin[axis] * scale).round() as usize
                                    && coordinate
                                        < ((origin[axis] + size[axis]) * scale).round() as usize
                            })
                        });
                    let index = (y * mode[0] as usize + x) * 4;
                    assert_eq!(
                        &with_cursor[index..index + 3],
                        &[if white { 255 } else { 0 }; 3]
                    );
                }
            }
            popup.destroy();
            xdg.destroy();
            popup_surface.destroy();
            positioner.destroy();
            harness.sync();
        }
        canvas.focus_output(OutputId(1)).unwrap();
        settle(&mut canvas);
        let source = harness
            .server
            .output_viewports()
            .into_iter()
            .find(|(_, output)| output.id == OutputId(1))
            .unwrap()
            .1;
        let usable = source.usable.unwrap();
        let (offset, _) = source.physical_region(usable).unwrap();
        let point = canvas
            .world_mut()
            .get::<UiGlobalTransform>(field)
            .unwrap()
            .transform_point2(Vec2::ZERO);
        let logical = source.raster.logical_point(std::array::from_fn(|axis| {
            f64::from(point[axis]) + f64::from(offset[axis])
        }));
        canvas
            .pointer(std::array::from_fn(|axis| {
                (logical[axis] - usable.origin()[axis]) as f32
            }))
            .unwrap();
        settle(&mut canvas);
        canvas
            .button(bevy::input::mouse::MouseButton::Left, true)
            .unwrap();
        settle(&mut canvas);
        canvas
            .button(bevy::input::mouse::MouseButton::Left, false)
            .unwrap();
        settle(&mut canvas);
        canvas.text("界é".repeat(32)).unwrap();
        let edited = settle(&mut canvas);
        assert!(
            canvas
                .world_mut()
                .get::<bevy::text::EditableText>(field)
                .unwrap()
                .value()
                .to_string()
                .contains("界é")
        );
        let editor = edited
            .input
            .editor
            .as_ref()
            .expect("Scaled native editors must publish their IME context");
        assert_eq!(editor.owner, field.to_bits());
        let node = canvas
            .world_mut()
            .get::<ComputedNode>(field)
            .unwrap()
            .size();
        let physical = CanvasRectangle::new(
            [
                f64::from(point.x - node.x * 0.5) + f64::from(offset[0]),
                f64::from(point.y - node.y * 0.5) + f64::from(offset[1]),
            ],
            node.as_dvec2().to_array(),
        )
        .unwrap();
        let logical_rect = source.raster.logical_rectangle(physical).unwrap();
        assert!(
            edited
                .input
                .controls
                .iter()
                .any(|rectangle| (0..2).all(|axis| (rectangle.origin()[axis]
                    - logical_rect.origin()[axis])
                    .abs()
                    < 1e-6
                    && (rectangle.size()[axis] - logical_rect.size()[axis]).abs() < 1e-6))
        );
        let cursor = {
            let world = canvas.world_mut();
            let caret = world
                .get::<bevy::text::EditableText>(field)
                .unwrap()
                .editor
                .cursor_geometry(1.0)
                .unwrap();
            let content = world.get::<ComputedNode>(field).unwrap().content_box().min
                - world.get::<bevy::ui::widget::TextScroll>(field).unwrap().0;
            let point = world
                .get::<UiGlobalTransform>(field)
                .unwrap()
                .transform_point2(Vec2::new(caret.x0 as f32, caret.y0 as f32) + content);
            source.raster.logical_point(std::array::from_fn(|axis| {
                f64::from(point[axis]) + f64::from(offset[axis])
            }))
        };
        assert!(
            logical_rect.contains(cursor),
            "The rendered caret must remain inside the scrolled editor: {cursor:?} in {logical_rect:?}"
        );
        assert!(
            (0..2).all(|axis| cursor[axis] >= f64::from(editor.caret[axis]) - 1.0
                && cursor[axis] <= f64::from(editor.caret[axis] + editor.caret[axis + 2]) + 1.0)
        );
        let ime = canvas
            .world_mut()
            .query::<&bevy::window::Window>()
            .single(canvas.world_mut())
            .unwrap()
            .ime_position;
        let expected = source.raster.logical_point(std::array::from_fn(|axis| {
            f64::from(ime[axis]) * scale + f64::from(offset[axis])
        }));
        assert!((f64::from(editor.caret[0]) - expected[0]).abs() <= 1.0);
        assert!((f64::from(editor.caret[1] + editor.caret[3]) - expected[1]).abs() <= 1.0);
        assert!(
            edited
                .input
                .drop_target
                .as_ref()
                .is_some_and(|target| logical_rect.contains(target.pointer))
        );
        let target = edited.input.drop_target.clone().unwrap();
        let token = 100 + step as u64;
        crate::native_input::transfer::enqueue(
            canvas.world_mut(),
            crate::native_input::transfer::Request {
                token,
                target,
                payload: crate::native_input::transfer::Payload::Text(" dropped ".into()),
                cancelled: Arc::new(AtomicBool::new(false)),
            },
        );
        settle(&mut canvas);
        assert!(receiver.try_iter().any(|request| matches!(request, lince_interface::host::HostRequest::DropFinished { token: actual, accepted: true } if actual == token)));
        assert!(
            canvas
                .world_mut()
                .get::<bevy::text::EditableText>(field)
                .unwrap()
                .value()
                .to_string()
                .contains(" dropped ")
        );
    }
    assert_eq!(retained.len(), 11);
    let completed = crate::runtime::Completed {
        sequence: 1,
        scene_revision: 1,
        frame: settle(&mut canvas),
        windows: Arc::new(canvas.painted_windows()),
        rejected: Vec::new(),
        input_serial: 0,
    };
    let _owner = harness.client.lock_manager.as_ref().unwrap().lock(&qh, ());
    harness.sync();
    assert!(harness.server.locked());
    for (_, output) in harness.server.output_viewports() {
        let size = output.physical_size.map(|value| value as i32);
        let mut texture: GlesTexture = renderer
            .create_buffer(Fourcc::Argb8888, (size[0], size[1]).into())
            .unwrap();
        {
            let mut target = renderer.bind(&mut texture).unwrap();
            let mut previous = renderer
                .render(
                    &mut target,
                    (size[0], size[1]).into(),
                    smithay::utils::Transform::Normal,
                )
                .unwrap();
            previous
                .clear(
                    smithay::backend::renderer::Color32F::new(1.0, 0.0, 1.0, 1.0),
                    &[smithay::utils::Rectangle::from_size(
                        (size[0], size[1]).into(),
                    )],
                )
                .unwrap();
            previous.finish().unwrap().wait().unwrap();
            let scene = composition
                .render_output(
                    &mut renderer,
                    &mut target,
                    &output,
                    &mut harness.server,
                    Some(&completed),
                    output.bounds.origin(),
                    Default::default(),
                )
                .unwrap();
            scene.sync.wait().unwrap();
            assert!(scene.locked);
            assert!(scene.layers.is_empty());
            assert!(scene.popups.is_empty());
            assert!(scene.drag.is_empty());
            assert!(scene.callbacks.is_empty());
        }
        assert!(
            pixels(&mut renderer, &mut texture, output.physical_size)
                .chunks_exact(4)
                .all(|pixel| pixel[..3] == [0; 3])
        );
    }
}
