use super::*;
use smithay::backend::renderer::ExportMem;
use wayland_protocols::xdg::shell::client::xdg_positioner::{Anchor, Gravity};

const SHADOW: &str = "Shadow radius (0–32 logical units)";
const BLUR: &str = "Background blur radius (0–24 logical units)";

fn tap(fixture: &mut Fixture, code: u32) {
    fixture.desktop.key(code, true, 10).unwrap();
    fixture.desktop.key(code, false, 11).unwrap();
    fixture.inspect(|_| ());
}

fn activate(fixture: &mut Fixture, caption: &'static str) {
    magnification::focus(fixture, caption);
    tap(fixture, 28);
}

fn value(fixture: &mut Fixture, caption: &'static str, keys: &[u32]) {
    magnification::focus(fixture, caption);
    fixture.desktop.key(29, true, 12).unwrap();
    tap(fixture, 30);
    fixture.desktop.key(29, false, 13).unwrap();
    for &code in keys { tap(fixture, code); }
    tap(fixture, 28);
}

pub(super) fn effects(fixture: &mut Fixture, enabled: bool) {
    fixture.desktop.key(125, true, 1).unwrap();
    tap(fixture, 18);
    fixture.desktop.key(125, false, 2).unwrap();
    fixture.inspect(|_| ());
    activate(fixture, "General");
    value(fixture, SHADOW, if enabled { &[2, 3] } else { &[11] });
    value(fixture, BLUR, if enabled { &[9] } else { &[11] });
    activate(fixture, lince_interface::i18n::Token::CloseEditMode.text());
    fixture.desktop.send(Input::Action(DesktopAction::Unfocus)).unwrap();
    fixture.inspect(|_| ());
}

fn pattern_buffer(shm: &wl_shm::WlShm, qh: &QueueHandle<Client>, size: [u32; 2]) -> wl_buffer::WlBuffer {
    let mut pixels = vec![0; (size[0] * size[1] * 4) as usize];
    for y in 0..size[1] {
        for x in 0..size[0] {
            let pixel = if (size[0] * 7 / 16..size[0] * 9 / 16).contains(&x) && (size[1] / 12..size[1] * 11 / 12).contains(&y) {
                [0, 255, 0, 255]
            } else { [0, 0, 0, 96] };
            let index = ((y * size[0] + x) * 4) as usize;
            pixels[index..index + 4].copy_from_slice(&pixel);
        }
    }
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(&pixels).unwrap();
    let pool = shm.create_pool(file.as_fd(), pixels.len() as i32, qh, ());
    let buffer = pool.create_buffer(0, size[0] as i32, size[1] as i32, (size[0] * 4) as i32, wl_shm::Format::Argb8888, qh, ());
    pool.destroy();
    buffer
}

fn repaint(fixture: &mut Fixture, window: &ClientWindow, surface: SurfaceId, retained: &mut Vec<wl_buffer::WlBuffer>) {
    for _ in 0..8 {
        fixture.inspect(|_| ());
        let size = fixture.client.window_configurations[&window.role.id()].0;
        assert!(size[0] > 0 && size[1] > 0);
        let current = fixture.desktop.server.windows().into_iter().find(|window| window.id == surface).unwrap();
        if current.nodes[0].layout.size == size.map(f64::from) {
            let version = current.nodes[0].content.version;
            fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|current| current.windows.iter()
                .find(|window| window.id == surface).is_some_and(|window| window.nodes.first()
                    .is_some_and(|node| node.layout.size == size.map(f64::from) && node.content.version == version))));
            return;
        }
        let buffer = pattern_buffer(fixture.client.shm.as_ref().unwrap(), &fixture.queue.handle(), size.map(|value| value as u32));
        window.surface.attach(Some(&buffer), 0, 0);
        window.surface.damage_buffer(0, 0, size[0], size[1]);
        window.surface.commit();
        retained.push(buffer);
        fixture.until(|fixture| fixture.desktop.server.windows().iter().find(|window| window.id == surface)
            .is_some_and(|window| window.nodes[0].layout.size == size.map(f64::from)));
    }
    panic!("The real client did not settle its configure/ack/buffer dimensions");
}

pub(super) fn pixels(fixture: &mut Fixture, label: &str) -> Vec<u8> {
    let size = (fixture.size[0] as i32, fixture.size[1] as i32);
    let mapping = {
        let target = fixture.renderer.bind(&mut fixture.texture).unwrap();
        fixture.renderer.copy_framebuffer(&target, smithay::utils::Rectangle::from_size(size.into()), Fourcc::Abgr8888).unwrap()
    };
    let source = fixture.renderer.map_texture(&mapping).unwrap();
    let stride = fixture.size[0] as usize * 4;
    let mut pixels = Vec::with_capacity(source.len());
    for row in 0..fixture.size[1] as usize {
        pixels.extend_from_slice(&source[row * stride..(row + 1) * stride]);
    }
    if let Some(directory) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
        std::fs::create_dir_all(&directory).unwrap();
        let mut ppm = format!("P6\n{} {}\n255\n", fixture.size[0], fixture.size[1]).into_bytes();
        for pixel in pixels.chunks_exact(4) { ppm.extend_from_slice(&pixel[..3]); }
        std::fs::write(std::path::Path::new(&directory).join(format!("appearance-client-{label}.ppm")), ppm).unwrap();
    }
    pixels
}

fn geometry(fixture: &Fixture, surface: SurfaceId) -> lince_interface::host::ItemGeometry {
    fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().items.iter()
        .find(|item| item.id == ItemId::Application(surface)).unwrap().clone()
}

fn click(fixture: &mut Fixture, point: [f64; 2]) {
    fixture.desktop.move_pointer(point, 20).unwrap();
    fixture.tick();
    fixture.desktop.button(272, true, 21).unwrap();
    fixture.desktop.button(272, false, 22).unwrap();
    fixture.inspect(|_| ());
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn real_wayland_client_keeps_sharp_content_popup_pixels_and_input_across_appearance_in_flat_spatial_free_and_tiled_scenes() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut harness = Harness::connect(Server::new([1000, 720]).unwrap());
    harness.server.configure_scaled_outputs(&[Monitor { id: OutputId(1), name: "Appearance client".into(), origin: [0, 0], size: [1000, 720] }],
        &std::collections::BTreeMap::from([(OutputId(1), 1.25)])).unwrap();
    harness.sync();
    let window = harness.window();
    let buffer = pattern_buffer(harness.client.shm.as_ref().unwrap(), &harness.queue.handle(), [320, 240]);
    window.surface.attach(Some(&buffer), 0, 0);
    window.surface.damage_buffer(0, 0, 320, 240);
    window.surface.commit();
    harness.sync();
    let mut retained = vec![buffer];
    let surface = harness.server.windows()[0].id;
    let mut fixture = Fixture::new(harness, &runtime, directory.path());
    fixture.desktop.commands(directory.path().join("park.json"), directory.path().join("wayland-appearance-fixture"));
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|current| current.frame.geometry.as_ref()
        .is_some_and(|geometry| geometry.items.iter().any(|item| item.id == ItemId::Application(surface)))));
    fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        native::workspace_config::set_physics(world, root, 1, false);
    });
    for spatial in [false, true] {
        for layout in [WorkspaceLayout::Free, WorkspaceLayout::Tiled] {
            fixture.inspect(move |canvas| {
                let world = canvas.world_mut();
                let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
                world.get_mut::<native::topology::view::View>(root).unwrap().spatial = spatial;
            });
            fixture.desktop.send(Input::Action(DesktopAction::SetLayout(layout))).unwrap();
            fixture.inspect(|_| ());
            if layout == WorkspaceLayout::Tiled {
                fixture.desktop.send(Input::Application(surface)).unwrap();
                fixture.inspect(|_| ());
                for _ in 0..4 {
                    if geometry(&fixture, surface).mapping.canvas.size()[0] < 600.0 { break; }
                    fixture.desktop.send(Input::Action(DesktopAction::CycleItemWidth)).unwrap();
                    fixture.inspect(|_| ());
                }
            }
            repaint(&mut fixture, &window, surface, &mut retained);
            effects(&mut fixture, false);
            let before = geometry(&fixture, surface);
            let off = pixels(&mut fixture, &format!("off-{layout:?}-{spatial}"));
            let green = |pixels: &[u8]| pixels.chunks_exact(4).filter(|pixel| pixel[0] < 20 && pixel[1] > 230 && pixel[2] < 20).count();
            assert!(green(&off) > 200, "The real client's sharp reference was not visible in {layout:?}/{spatial}");
            let configurations = fixture.client.configures.len();
            let size = *fixture.client.configures.last().unwrap();
            let original = fixture.inspect(move |canvas| {
                let world = canvas.world_mut();
                let entity = world.query::<(Entity, &native::layout::desktop::ApplicationWindow)>().iter(world)
                    .find(|(_, window)| window.surface == surface).unwrap().0;
                world.get::<native::topology::presentation::Surface>(entity).unwrap().material.clone()
            });
            effects(&mut fixture, true);
            let on = pixels(&mut fixture, &format!("on-{layout:?}-{spatial}"));
            assert_eq!(geometry(&fixture, surface), before, "Appearance changed published client/input geometry");
            assert!(fixture.client.configures[configurations..].iter().all(|candidate| *candidate == size), "Appearance issued a size-only configure");
            assert!(green(&on).abs_diff(green(&off)) <= 4, "Appearance blurred or rescaled actual client content");
            fixture.inspect(move |canvas| {
                let world = canvas.world_mut();
                let entity = world.query::<(Entity, &native::layout::desktop::ApplicationWindow)>().iter(world)
                    .find(|(_, window)| window.surface == surface).unwrap().0;
                let face = world.get::<native::topology::presentation::Surface>(entity).unwrap();
                assert_eq!(face.material, original);
                assert_eq!(world.get::<Children>(face.face).into_iter().flatten().filter(|child| world.get::<Mesh3d>(**child).is_some()).count(), 2);
            });
            let origin = before.mapping.surface.origin();
            let extent = before.mapping.surface.size();
            let raster = fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().outputs.iter()
                .find(|output| output.id == before.output).unwrap().raster;
            let render_size = fixture.size;
            let (shadow_point, shadow_pixel) = [
                [origin[0] + extent[0] + 4.0, origin[1] + extent[1] * 0.5],
                [origin[0] - 4.0, origin[1] + extent[1] * 0.5],
                [origin[0] + extent[0] * 0.5, origin[1] - 4.0],
                [origin[0] + extent[0] * 0.5, origin[1] + extent[1] + 4.0],
            ].into_iter().find_map(|source| {
                let point = before.mapping.canvas_point(source)?;
                let pixel = raster.point(point);
                (pixel[0] >= 1.0 && pixel[1] >= 1.0 && pixel[0] < f64::from(render_size[0] - 1) && pixel[1] < f64::from(render_size[1] - 1))
                    .then_some((point, (pixel[1].floor() as usize * render_size[0] as usize + pixel[0].floor() as usize) * 4))
            }).expect("The actual client needs a visible exterior shadow reference");
            assert!(before.surface_point(shadow_point).is_none());
            assert!((0..3).any(|channel| on[shadow_pixel + channel].saturating_add(1) < off[shadow_pixel + channel]), "The independently located exterior was not actually shadowed in {layout:?}/{spatial}");
            let buttons = fixture.client.pointer_buttons.len();
            click(&mut fixture, shadow_point);
            assert_ne!(fixture.desktop.server.capture_pointer_owner(), CaptureContentOwner::Window(surface));
            assert_eq!(fixture.client.pointer_buttons.len(), buttons, "A visible shadow enlarged the actual client input region");
            let source = std::array::from_fn(|axis| before.mapping.surface.origin()[axis] + before.mapping.surface.size()[axis] * 0.5);
            click(&mut fixture, before.mapping.canvas_point(source).unwrap());
            fixture.client.keys.clear();
            tap(&mut fixture, 45);
            fixture.until(|fixture| fixture.client.keys.ends_with(&[(45, true), (45, false)]));
            let qh = fixture.queue.handle();
            let positioner = fixture.client.shell.as_ref().unwrap().create_positioner(&qh, ());
            positioner.set_size(110, 70);
            let client_size = fixture.desktop.server.windows().into_iter().find(|window| window.id == surface).unwrap().nodes[0].layout.size.map(|value| value.round() as i32);
            positioner.set_anchor_rect(client_size[0] - 20, 40, 20, 20);
            positioner.set_anchor(Anchor::Right);
            positioner.set_gravity(Gravity::Right);
            positioner.set_offset(20, 0);
            let popup_surface = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
            let popup_xdg = fixture.client.shell.as_ref().unwrap().get_xdg_surface(&popup_surface, &qh, ());
            let popup = popup_xdg.get_popup(Some(&window.xdg), &positioner, &qh, ());
            popup_surface.commit();
            fixture.tick();
            let popup_buffer = privacy::buffer(&fixture, [110, 70], [255, 255, 0, 255]);
            popup_surface.attach(Some(&popup_buffer), 0, 0);
            popup_surface.damage_buffer(0, 0, 110, 70);
            popup_surface.commit();
            fixture.until(|fixture| fixture.desktop.server.windows().iter().find(|window| window.id == surface).is_some_and(|window| !window.popups.is_empty()));
            let snapshot = fixture.desktop.server.windows().into_iter().find(|window| window.id == surface).unwrap();
            let origin = snapshot.popups[0].origin;
            let parent = geometry(&fixture, surface);
            let point = parent.mapping.canvas_point([f64::from(origin[0]) + 55.0, f64::from(origin[1]) + 35.0]).unwrap();
            assert!(parent.surface_point(point).is_none(), "The popup reference must exercise pixels outside its parent");
            fixture.client.pointer_buttons.clear();
            click(&mut fixture, point);
            fixture.until(|fixture| fixture.client.pointer_buttons.ends_with(&[(272, true), (272, false)]));
            let output = fixture.desktop.current.as_ref().unwrap().frame.output.unwrap();
            let physical = fixture.desktop.current.as_ref().unwrap().frame.input.mappings.get(output).unwrap().logical_to_physical(point).unwrap();
            assert!(physical[0] >= 0.0 && physical[1] >= 0.0 && physical[0] < f64::from(fixture.size[0]) && physical[1] < f64::from(fixture.size[1]));
            let image = pixels(&mut fixture, &format!("popup-{layout:?}-{spatial}"));
            let index = (physical[1] as usize * fixture.size[0] as usize + physical[0] as usize) * 4;
            assert!(image[index] < 20 && image[index + 1] > 230 && image[index + 2] > 230, "Actual popup pixels were clipped, blurred or misplaced");
            popup.destroy();
            popup_xdg.destroy();
            popup_surface.destroy();
            popup_buffer.destroy();
            positioner.destroy();
            fixture.until(|fixture| fixture.desktop.server.windows().iter().find(|window| window.id == surface).is_some_and(|window| window.popups.is_empty()));
            assert!(fixture.desktop.buttons.is_empty() && fixture.desktop.keys.is_empty());
        }
    }
    window.role.destroy();
    window.xdg.destroy();
    window.surface.destroy();
    fixture.until(|fixture| fixture.desktop.server.windows().iter().all(|window| window.id != surface)
        && fixture.desktop.current.as_ref().is_some_and(|current| current.frame.geometry.as_ref()
            .is_some_and(|geometry| geometry.items.iter().all(|item| item.id != ItemId::Application(surface)))));
    assert!(fixture.desktop.running && fixture.desktop.buttons.is_empty() && fixture.desktop.keys.is_empty());
    fixture.desktop.worker.stop().unwrap();
}
