use super::*;
use super::super::{capture_lifetime, magnification::clients, opening, privacy};
use crate::launch::Capability;
use lince_interface::host::display::DisplayOutcome;
use wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_manager_v1::Options;

const A: OutputId = OutputId(1);
const B: OutputId = OutputId(2);
const B_NAME: &str = "Rotation-B";
const B_SIZE: [u32; 2] = [800, 600];
const SCALE: f64 = 1.25;

mod consumers;

pub(super) fn create(runtime: &tokio::runtime::Runtime, directory: &std::path::Path) -> Fixture {
    let mut server = Server::new([1800, 1000]).unwrap();
    server.configure_scaled_outputs(&[
        Monitor { id: A, name: "Rotation-A".into(), origin: [40, 40], size: [600, 480] },
        Monitor { id: B, name: B_NAME.into(), origin: [700, 80], size: B_SIZE.map(|value| value as i32) },
    ], &std::collections::BTreeMap::from([(A, 1.25), (B, SCALE)])).unwrap();
    server.registered_output_catalog().unwrap();
    let harness = Harness::connect_authorized(server, Some(vec![Capability::Capture, Capability::WindowList]));
    let mut fixture = Fixture::with_size(harness, runtime, directory, [1800, 1000]);
    fixture.desktop.commands(directory.join("park.json"), directory.join("wayland-rotation"));
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry|
        geometry.outputs.len() == 2 && geometry.outputs.iter().all(|output| fixture.desktop.server.display_snapshot().heads.iter()
            .find(|head| head.id == output.id).is_some_and(|head| head.rotation == output.rotation)))));
    fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        let ids = world.get::<native::workspace::Workspaces>(root).unwrap().entries.iter().map(|workspace| workspace.id).collect::<Vec<_>>();
        for id in ids { native::workspace_config::set_physics(world, root, id, false); }
        world.get_mut::<native::topology::view::View>(root).unwrap().spatial = false;
    });
    fixture
}

fn tap(fixture: &mut Fixture, code: u32) {
    fixture.desktop.key(code, true, 10).unwrap();
    fixture.desktop.key(code, false, 11).unwrap();
    fixture.inspect(|_| ());
}

pub(super) fn output(fixture: &mut Fixture, id: OutputId) {
    let serial = fixture.desktop.worker.input(Input::Output(id)).unwrap();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.input_serial >= serial && frame.frame.output == Some(id)));
}

pub(super) fn open(fixture: &mut Fixture) {
    fixture.desktop.key(125, true, 1).unwrap();
    tap(fixture, 18);
    fixture.desktop.key(125, false, 2).unwrap();
    fixture.inspect(|_| ());
    opening::activate(fixture, "Park");
    opening::activate(fixture, "Arrange monitors");
    opening::activate(fixture, B_NAME);
}

pub(super) fn close(fixture: &mut Fixture) {
    opening::activate(fixture, "Close");
    opening::close(fixture);
}

pub(super) fn request(fixture: &mut Fixture, caption: &str, expected: DisplayOutcome) {
    let previous = fixture.desktop.server.display_snapshot().last_request;
    opening::activate(fixture, caption);
    fixture.until(|fixture| {
        let snapshot = fixture.desktop.server.display_snapshot();
        snapshot.last_request > previous && snapshot.response.as_ref().is_some_and(|response| response.outcome == expected)
    });
}

fn caption(rotation: QuarterTurn) -> &'static str {
    match rotation {
        QuarterTurn::Normal => "Rotation: 0° · change",
        QuarterTurn::Clockwise90 => "Rotation: 90° · change",
        QuarterTurn::UpsideDown => "Rotation: 180° · change",
        QuarterTurn::Clockwise270 => "Rotation: 270° · change",
    }
}

fn physical(rotation: QuarterTurn, point: [f64; 2]) -> [f64; 2] {
    match rotation {
        QuarterTurn::Normal => point,
        QuarterTurn::Clockwise90 => [f64::from(B_SIZE[0]) - point[1], point[0]],
        QuarterTurn::UpsideDown => [f64::from(B_SIZE[0]) - point[0], f64::from(B_SIZE[1]) - point[1]],
        QuarterTurn::Clockwise270 => [point[1], f64::from(B_SIZE[1]) - point[0]],
    }
}

fn pixel(pixels: &[u8], size: [u32; 2], point: [f64; 2], bgra: bool, expected: [u8; 3]) {
    assert!((0..2).all(|axis| point[axis] >= 1.0 && point[axis] < f64::from(size[axis] - 1)));
    let offset = (point[1] as usize * size[0] as usize + point[0] as usize) * 4;
    let actual = if bgra { [pixels[offset + 2], pixels[offset + 1], pixels[offset]] } else { [pixels[offset], pixels[offset + 1], pixels[offset + 2]] };
    assert!((0..3).all(|axis| actual[axis].abs_diff(expected[axis]) <= 20), "Missing actual pixel at {point:?}: {actual:?} != {expected:?}");
}

struct Layer {
    surface: wl_surface::WlSurface,
    role: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    buffer: wl_buffer::WlBuffer,
}

fn layer(fixture: &mut Fixture, bar: bool) -> Layer {
    let qh = fixture.queue.handle();
    let surface = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
    let role = fixture.client.layer_shell.as_ref().unwrap().get_layer_surface(&surface, fixture.client.output.as_ref(),
        if bar { zwlr_layer_shell_v1::Layer::Top } else { zwlr_layer_shell_v1::Layer::Overlay },
        if bar { "rotation-reservation" } else { "rotation-markers" }.into(), &qh, ());
    role.set_size(if bar { 120 } else { 180 }, if bar { 28 } else { 100 });
    role.set_anchor(if bar { zwlr_layer_surface_v1::Anchor::Top } else { zwlr_layer_surface_v1::Anchor::Top | zwlr_layer_surface_v1::Anchor::Left });
    role.set_exclusive_zone(if bar { 28 } else { -1 });
    if !bar { role.set_margin(40, 0, 0, 60); }
    surface.commit();
    fixture.exchange();
    let buffer = if bar { privacy::buffer(fixture, [120, 28], [60, 60, 60, 255]) } else { clients::pattern(fixture, [180, 100]) };
    surface.attach(Some(&buffer), 0, 0);
    surface.damage_buffer(0, 0, if bar { 120 } else { 180 }, if bar { 28 } else { 100 });
    surface.commit();
    fixture.until(|fixture| fixture.desktop.layers.iter().any(|layer| layer.namespace == if bar { "rotation-reservation" } else { "rotation-markers" }));
    Layer { surface, role, buffer }
}

fn destroy(fixture: &mut Fixture, layer: Layer) {
    layer.role.destroy(); layer.surface.destroy(); layer.buffer.destroy();
    fixture.exchange();
}

fn popup(fixture: &mut Fixture, window: &ClientWindow, id: SurfaceId, size: [u32; 2], rotation: QuarterTurn) {
    use wayland_protocols::xdg::shell::client::xdg_positioner::{Anchor, Gravity};
    let qh = fixture.queue.handle();
    let positioner = fixture.client.shell.as_ref().unwrap().create_positioner(&qh, ());
    positioner.set_size(72, 48); positioner.set_anchor_rect(size[0] as i32 - 1, size[1] as i32 / 2, 1, 1);
    positioner.set_anchor(Anchor::Right); positioner.set_gravity(Gravity::Right); positioner.set_offset(8, 0);
    let surface = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
    let xdg = fixture.client.shell.as_ref().unwrap().get_xdg_surface(&surface, &qh, ());
    let popup = xdg.get_popup(Some(&window.xdg), &positioner, &qh, ());
    surface.commit(); fixture.exchange();
    let buffer = privacy::buffer(fixture, [72, 48], [255, 0, 255, 255]);
    surface.attach(Some(&buffer), 0, 0); surface.damage_buffer(0, 0, 72, 48); surface.commit();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame|
        frame.windows.iter().find(|window| window.id == id).is_some_and(|window| window.popups.len() == 1)));
    let origin = fixture.desktop.current.as_ref().unwrap().windows.iter().find(|window| window.id == id).unwrap().popups[0].origin;
    let local = [12.0, 20.0];
    let item = clients::geometry(fixture, id);
    let world = item.mapping.canvas_point([f64::from(origin[0]) + local[0], f64::from(origin[1]) + local[1]]).unwrap();
    assert!(item.surface_point(world).is_none(), "The rotation case must expose a popup outside its parent's input region");
    let (viewport, pixels) = fixture.output_pixels(B);
    let point = viewport.raster.point(world);
    pixel(&pixels, viewport.source_size(), point, false, [255, 0, 255]);
    clients::pointer_click(fixture, B, point, &surface, local);
    let (source, session) = capture_lifetime::output_session(fixture);
    let pending = capture_lifetime::Pending::new(fixture, &session, B_SIZE);
    let pixels = capture_lifetime::completed(fixture, &pending);
    pixel(&pixels, B_SIZE, physical(rotation, point), true, [255, 0, 255]);
    pending.destroy(fixture); session.destroy(); source.destroy();
    popup.destroy(); xdg.destroy(); surface.destroy(); buffer.destroy(); positioner.destroy();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame|
        frame.windows.iter().find(|window| window.id == id).is_some_and(|window| window.popups.is_empty())));
}

fn scene(fixture: &mut Fixture, window: &ClientWindow, id: SurfaceId, markers: &Layer, buffers: &mut Vec<wl_buffer::WlBuffer>, rotation: QuarterTurn) {
    let size = clients::repaint(fixture, window, id, buffers);
    fixture.desktop.move_pointer([705.0, 85.0], 1).unwrap();
    fixture.tick();
    let item = clients::geometry(fixture, id);
    let (viewport, pixels) = fixture.output_pixels(B);
    assert_eq!(viewport.rotation, rotation);
    assert_eq!(viewport.physical_size, B_SIZE);
    let source_size = if matches!(rotation, QuarterTurn::Clockwise90 | QuarterTurn::Clockwise270) { [600, 800] } else { B_SIZE };
    assert_eq!(viewport.source_size(), source_size);
    let usable = viewport.usable.unwrap();
    assert_eq!(usable.origin(), [700.0, 108.0]);
    assert_eq!(usable.size(), [f64::from(source_size[0]) / SCALE, f64::from(source_size[1]) / SCALE - 28.0]);
    let (source, session) = capture_lifetime::output_session(fixture);
    let pending = capture_lifetime::Pending::new(fixture, &session, B_SIZE);
    let captured = capture_lifetime::completed(fixture, &pending);
    for (offset, color) in clients::MARKERS {
        let local = [90.0 + f64::from(offset[0]), 50.0 + f64::from(offset[1])];
        let expected = [(60.0 + local[0]) * SCALE, (40.0 + local[1]) * SCALE];
        let measured = clients::marker(&pixels, source_size, expected, 1.0, color, false);
        clients::compare(measured, expected, "rotation layer marker from declared margins");
        let final_point = physical(rotation, expected);
        clients::compare(clients::marker(&captured, B_SIZE, final_point, 1.0, color, true), final_point, "independent output-capture quarter turn");
        clients::pointer_click(fixture, B, measured, &markers.surface, local);
    }
    let mut target = None;
    for (offset, color) in clients::MARKERS {
        let local = std::array::from_fn(|axis| f64::from(size[axis] / 2) + f64::from(offset[axis]));
        let locator = viewport.raster.point(item.mapping.canvas_point(local).unwrap());
        let measured = clients::marker(&pixels, source_size, locator, 1.0, color, false);
        let expected = physical(rotation, measured);
        clients::compare(clients::marker(&captured, B_SIZE, expected, 1.0, color, true), expected, "mapped client's actual pixels and independent capture rotation");
        target = Some((measured, local));
    }
    pending.destroy(fixture); session.destroy(); source.destroy();
    let (point, local) = target.unwrap();
    clients::pointer_click(fixture, B, point, &window.surface, local);
    fixture.client.key_recipients.clear(); tap(fixture, 30);
    assert_eq!(fixture.client.key_recipients, [(Some(window.surface.id()), 30, true), (Some(window.surface.id()), 30, false)]);
    let qh = fixture.queue.handle();
    let handle = fixture.client.foreign_handles.iter().find(|handle| fixture.client.foreign_names.get(&handle.id()).is_some_and(|name| name == B_NAME)).unwrap();
    let source = fixture.client.capture.toplevels.as_ref().unwrap().create_source(handle, &qh, ());
    let session = fixture.client.capture.manager.as_ref().unwrap().create_session(&source, Options::empty(), &qh, ());
    fixture.exchange();
    assert_eq!(fixture.client.capture.sizes.last(), Some(&size));
    let pending = capture_lifetime::Pending::new(fixture, &session, size);
    let direct = capture_lifetime::completed(fixture, &pending);
    for (offset, color) in clients::MARKERS {
        let expected = std::array::from_fn(|axis| f64::from(size[axis] / 2) + f64::from(offset[axis]));
        clients::compare(clients::marker(&direct, size, expected, 1.0, color, true), expected, "direct-window source stays unrotated");
    }
    pending.destroy(fixture); session.destroy(); source.destroy(); fixture.exchange();
    popup(fixture, window, id, size, rotation);
    consumers::application_and_layer(fixture, window, id, size, markers, rotation);
    assert!(fixture.desktop.buttons.is_empty());
    assert_eq!(fixture.composition.capture_test_state(), [0; 4]);
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn real_monitor_controls_keep_four_rotations_pixel_input_popup_bar_and_capture_consistent_on_one_head_and_reload_the_saved_candidate() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("park.json");
    std::fs::write(&path, br#"{"keyboard":{"layout":"br"}}"#).unwrap();
    let mut fixture = create(&runtime, directory.path());
    output(&mut fixture, A);
    let (first, first_buffer, first_id) = opening::map(&mut fixture);
    first.role.set_title("Rotation-A".into()); first.role.set_max_size(180, 120); first.surface.commit(); fixture.exchange();
    let mut first_buffers = vec![first_buffer];
    clients::repaint(&mut fixture, &first, first_id, &mut first_buffers);
    output(&mut fixture, B);
    let (window, buffer, id) = opening::map(&mut fixture);
    window.role.set_title(B_NAME.into()); window.role.set_max_size(240, 180); window.surface.commit(); fixture.exchange();
    fixture.desktop.send(Input::Action(DesktopAction::SetLayout(WorkspaceLayout::Free))).unwrap();
    fixture.desktop.send(Input::Application(id)).unwrap(); fixture.inspect(|_| ());
    let mut buffers = vec![buffer];
    clients::repaint(&mut fixture, &window, id, &mut buffers);
    let bar = layer(&mut fixture, true);
    let markers = layer(&mut fixture, false);
    scene(&mut fixture, &window, id, &markers, &mut buffers, QuarterTurn::Normal);
    let a_item = clients::geometry(&fixture, first_id);
    let a_mapping = fixture.desktop.current.as_ref().unwrap().frame.input.mappings.get(A).unwrap().clone();
    let initial_a = fixture.desktop.server.display_snapshot().configuration()[&A].clone();
    let mut previous = QuarterTurn::Normal;
    for rotation in [QuarterTurn::Clockwise90, QuarterTurn::UpsideDown, QuarterTurn::Clockwise270] {
        open(&mut fixture);
        let before = fixture.desktop.server.display_snapshot().configuration();
        let saved = std::fs::read(&path).unwrap();
        opening::activate(&mut fixture, caption(previous));
        request(&mut fixture, "Test", DisplayOutcome::Tested);
        assert_eq!(fixture.desktop.server.display_snapshot().configuration(), before);
        assert_eq!(std::fs::read(&path).unwrap(), saved);
        let (source, session) = capture_lifetime::output_session(&mut fixture);
        let pending = capture_lifetime::Pending::new(&mut fixture, &session, B_SIZE);
        capture_lifetime::submitted(&mut fixture, &pending);
        request(&mut fixture, "Apply", DisplayOutcome::Previewed);
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry|
            geometry.outputs.iter().any(|output| output.id == B && output.rotation == rotation))));
        assert_eq!(std::fs::read(&path).unwrap(), saved, "Preview leaked into the saved startup configuration");
        capture_lifetime::rejected(&mut fixture, &pending); pending.destroy(&mut fixture); session.destroy(); source.destroy();
        request(&mut fixture, "Keep changes", DisplayOutcome::Kept);
        let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["outputs"][B_NAME]["rotation"], serde_json::to_value(rotation).unwrap());
        assert_eq!(saved["keyboard"]["layout"], "br");
        close(&mut fixture);
        scene(&mut fixture, &window, id, &markers, &mut buffers, rotation);
        assert_eq!(clients::geometry(&fixture, first_id), a_item);
        assert_eq!(fixture.desktop.current.as_ref().unwrap().frame.input.mappings.get(A), Some(&a_mapping));
        assert_eq!(fixture.desktop.server.display_snapshot().configuration()[&A], initial_a);
        previous = rotation;
    }
    destroy(&mut fixture, markers); destroy(&mut fixture, bar);
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry|
        geometry.outputs.iter().find(|output| output.id == B).is_some_and(|output| output.usable == Some(output.bounds)))));
    let saved = std::fs::read(&path).unwrap();
    let configuration = fixture.desktop.server.display_snapshot().configuration();
    fixture.desktop.worker.stop().unwrap();
    let mut restarted = create(&runtime, directory.path());
    assert_eq!(restarted.desktop.server.display_snapshot().configuration(), configuration);
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    assert_eq!(restarted.desktop.server.keyboard_snapshot().settings.layout, "br");
    assert_eq!(restarted.output_pixels(B).0.rotation, QuarterTurn::Clockwise270);
    restarted.desktop.worker.stop().unwrap();
}


#[derive(Clone, Copy, Debug)]
enum Retirement { Deadline, Lock, ParentResize }

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn queued_monitor_keep_cannot_save_after_real_deadline_composed_lock_or_last_head_replacement_and_fresh_input_and_capture_recover() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for retirement in [Retirement::Deadline, Retirement::Lock, Retirement::ParentResize] {
        let profile = directory.path().join(format!("{retirement:?}")); std::fs::create_dir(&profile).unwrap();
        let path = profile.join("park.json");
        let original = br#"{"keyboard":{"layout":"br"}}"#;
        std::fs::write(&path, original).unwrap();
        let mut fixture = single(&runtime, &profile, [1000, 720], 1.25);
        open(&mut fixture);
        let before = fixture.desktop.server.display_snapshot().configuration();
        opening::activate(&mut fixture, caption(QuarterTurn::Normal));
        request(&mut fixture, "Apply", DisplayOutcome::Previewed);
        let preview = fixture.desktop.server.display_snapshot().preview.unwrap();
        super::super::magnification::focus(&mut fixture, "Keep changes");
        if matches!(retirement, Retirement::Deadline) {
            while preview.deadline.saturating_duration_since(Instant::now()) > Duration::from_millis(750) {
                fixture.tick();
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        let (source, session) = capture_lifetime::output_session(&mut fixture);
        let captured = Instant::now();
        let pending = capture_lifetime::Pending::new(&mut fixture, &session, [1000, 720]);
        capture_lifetime::submitted(&mut fixture, &pending);
        let (_, release) = fixture.pause();
        fixture.desktop.key(28, true, 60).unwrap(); fixture.desktop.key(28, false, 61).unwrap();
        assert_eq!(fixture.desktop.server.display_snapshot().preview.as_ref().map(|preview| preview.token), Some(preview.token));
        let mut locker = None;
        match retirement {
            Retirement::Deadline => {
                std::thread::sleep(preview.deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(30));
                fixture.desktop.server.advance_display(Instant::now());
            }
            Retirement::Lock => {
                let qh = fixture.queue.handle();
                let lock = fixture.client.lock_manager.as_ref().unwrap().lock(&qh, ());
                fixture.exchange();
                assert!(fixture.desktop.server.locked());
                let surface = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
                let role = lock.get_lock_surface(&surface, fixture.client.output.as_ref().unwrap(), &qh, ());
                fixture.exchange();
                let size = *fixture.client.lock_sizes.last().unwrap();
                let buffer = privacy::buffer(&fixture, size, [20, 20, 20, 255]);
                surface.attach(Some(&buffer), 0, 0); surface.damage_buffer(0, 0, size[0] as i32, size[1] as i32); surface.commit();
                fixture.exchange();
                fixture.desktop.server.advance_display(Instant::now());
                locker = Some((lock, role, surface, buffer, size));
            }
            Retirement::ParentResize => {
                fixture.desktop.resize_virtual_outputs([320, 240]).unwrap();
                fixture.desktop.server.advance_display(Instant::now());
            }
        }
        release.send(()).unwrap();
        fixture.until(|fixture| fixture.desktop.server.display_snapshot().preview.is_none()
            && fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry|
                if matches!(retirement, Retirement::ParentResize) { geometry.outputs.len() == 1 && geometry.outputs[0].id == A
                    && geometry.outputs[0].physical_size == [320, 240] }
                else { geometry.outputs.len() == 1 && geometry.outputs[0].rotation == QuarterTurn::Normal })));
        assert_eq!(std::fs::read(&path).unwrap(), original, "A queued Keep saved a retired {retirement:?} candidate");
        assert!(!matches!(fixture.desktop.server.display_snapshot().response.as_ref().map(|response| &response.outcome), Some(DisplayOutcome::Kept)));
        if !matches!(retirement, Retirement::ParentResize) { assert_eq!(fixture.desktop.server.display_snapshot().configuration(), before); }
        capture_lifetime::rejected(&mut fixture, &pending);
        assert!(captured.elapsed() < Duration::from_secs(2), "The {retirement:?} case reached capture timeout instead of proving target retirement");
        pending.destroy(&mut fixture); session.destroy(); source.destroy(); fixture.exchange();
        if let Some((lock, role, surface, buffer, initial_size)) = locker {
            let size = *fixture.client.lock_sizes.last().unwrap();
            let replacement = if size != initial_size {
                let next = privacy::buffer(&fixture, size, [20, 20, 20, 255]);
                surface.attach(Some(&next), 0, 0); surface.damage_buffer(0, 0, size[0] as i32, size[1] as i32); surface.commit();
                Some(next)
            } else { None };
            fixture.until(|fixture| fixture.client.lock_replies.last() == Some(&true));
            let count = fixture.client.key_recipients.len();
            fixture.desktop.key(30, true, 70).unwrap(); fixture.desktop.key(30, false, 71).unwrap();
            fixture.tick();
            assert!(fixture.client.key_recipients[count..].iter().all(|(recipient, _, _)| *recipient == Some(surface.id())), "Ordinary keys escaped the composed lock's own surface");
            lock.unlock_and_destroy(); fixture.exchange();
            fixture.until(|fixture| !fixture.desktop.server.locked());
            role.destroy(); surface.destroy(); buffer.destroy(); fixture.exchange();
            if let Some(buffer) = replacement { buffer.destroy(); fixture.exchange(); }
        }
        close(&mut fixture);
        let (window, buffer, id) = opening::map(&mut fixture);
        window.role.set_max_size(180, 120); window.surface.commit();
        fixture.desktop.send(Input::Application(id)).unwrap();
        let mut buffers = vec![buffer];
        let size = clients::repaint(&mut fixture, &window, id, &mut buffers);
        let item = clients::geometry(&fixture, id);
        let (viewport, pixels) = fixture.output_pixels(item.output);
        let local = std::array::from_fn(|axis| f64::from(size[axis] / 2) + f64::from(clients::MARKERS[0].0[axis]));
        let near = viewport.raster.point(item.mapping.canvas_point(local).unwrap());
        let point = clients::marker(&pixels, viewport.source_size(), near, 1.0, clients::MARKERS[0].1, false);
        clients::pointer_click(&mut fixture, item.output, point, &window.surface, local);
        fixture.client.key_recipients.clear(); tap(&mut fixture, 30);
        assert_eq!(fixture.client.key_recipients, [(Some(window.surface.id()), 30, true), (Some(window.surface.id()), 30, false)]);
        let (source, session) = capture_lifetime::output_session(&mut fixture);
        let pending = capture_lifetime::Pending::new(&mut fixture, &session, viewport.physical_size);
        let pixels = capture_lifetime::completed(&mut fixture, &pending);
        clients::compare(clients::marker(&pixels, viewport.physical_size, point, 1.0, clients::MARKERS[0].1, true), point, "fresh public capture after retired monitor confirmation");
        pending.destroy(&mut fixture); session.destroy(); source.destroy(); fixture.exchange();
        assert_eq!(fixture.composition.capture_test_state(), [0; 4]);
        assert!(fixture.desktop.keys.is_empty());
        assert!(fixture.desktop.buttons.is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        fixture.desktop.worker.stop().unwrap();
    }
}

fn single(runtime: &tokio::runtime::Runtime, directory: &std::path::Path, size: [u32; 2], scale: f64) -> Fixture {
    let parent = [size[0].max(size[1]); 2];
    let mut server = Server::new(parent.map(|value| value as i32)).unwrap();
    server.configure_scaled_outputs(&[Monitor { id: B, name: B_NAME.into(), origin: [0, 0], size: size.map(|value| value as i32) }],
        &std::collections::BTreeMap::from([(B, scale)])).unwrap();
    server.registered_output_catalog().unwrap();
    let harness = Harness::connect_authorized(server, Some(vec![Capability::Capture, Capability::WindowList]));
    let mut fixture = Fixture::with_size(harness, runtime, directory, parent);
    fixture.desktop.commands(directory.join("park.json"), directory.join("wayland-rotation-controls"));
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry|
        geometry.outputs.len() == 1 && geometry.outputs[0].id == B
            && geometry.outputs[0].rotation == fixture.desktop.server.display_snapshot().heads[0].rotation)));
    fixture
}

fn modal(fixture: &mut Fixture) -> Entity {
    fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        world.query::<(Entity, &Name)>().iter(world).find(|(_, name)| name.as_str() == "desktop.monitors").unwrap().0
    })
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn scaled_and_narrow_monitor_keyboard_controls_preserve_a_failed_save_and_revert_then_keep_and_recreate_the_accepted_rotation() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for (size, scale) in [([1000, 720], 1.25), ([360, 720], 1.5)] {
        let profile = directory.path().join(format!("{}-{scale}", size[0]));
        std::fs::create_dir(&profile).unwrap();
        let path = profile.join("park.json");
        let backup = profile.join("park.saved");
        let original = br#"{"keyboard":{"layout":"br"}}"#;
        std::fs::write(&path, original).unwrap();
        let mut fixture = single(&runtime, &profile, size, scale);
        open(&mut fixture);
        let owner = modal(&mut fixture);
        let before = fixture.desktop.server.display_snapshot().configuration();
        opening::activate(&mut fixture, caption(QuarterTurn::Normal));
        request(&mut fixture, "Test", DisplayOutcome::Tested);
        assert_eq!(fixture.desktop.server.display_snapshot().configuration(), before);
        request(&mut fixture, "Apply", DisplayOutcome::Previewed);
        let token = fixture.desktop.server.display_snapshot().preview.unwrap().token;
        assert_eq!(std::fs::read(&path).unwrap(), original);
        std::fs::rename(&path, &backup).unwrap();
        std::fs::create_dir(&path).unwrap();
        let previous = fixture.desktop.server.display_snapshot().last_request;
        opening::activate(&mut fixture, "Keep changes");
        fixture.until(|fixture| {
            let snapshot = fixture.desktop.server.display_snapshot();
            snapshot.last_request > previous && snapshot.response.as_ref().is_some_and(|response|
                matches!(&response.outcome, DisplayOutcome::Failed(reason) if reason.contains("Could not save display settings")))
        });
        assert_eq!(fixture.desktop.server.display_snapshot().preview.unwrap().token, token);
        assert_eq!(fixture.desktop.server.display_snapshot().configuration()[&B].as_ref().unwrap().rotation, QuarterTurn::Clockwise90);
        assert_eq!(modal(&mut fixture), owner, "A failed save replaced the monitor draft/modal");
        assert!(fixture.inspect(|canvas| {
            let world = canvas.world_mut();
            world.query::<(&Text, &InheritedVisibility)>().iter(world).any(|(text, visible)|
                visible.get() && text.0.contains("Could not save display settings"))
        }), "The ordinary monitor UI did not expose the actual filesystem failure");
        assert_eq!(std::fs::read(&backup).unwrap(), original);
        std::fs::remove_dir(&path).unwrap(); std::fs::rename(&backup, &path).unwrap();
        request(&mut fixture, "Revert", DisplayOutcome::Reverted);
        assert_eq!(fixture.desktop.server.display_snapshot().configuration(), before);
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert_eq!(modal(&mut fixture), owner);
        opening::activate(&mut fixture, caption(QuarterTurn::Normal));
        request(&mut fixture, "Apply", DisplayOutcome::Previewed);
        request(&mut fixture, "Keep changes", DisplayOutcome::Kept);
        let saved = std::fs::read(&path).unwrap();
        let settings: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        assert_eq!(settings["outputs"][B_NAME]["rotation"], "clockwise90");
        assert_eq!(settings["keyboard"]["layout"], "br");
        close(&mut fixture);
        let (window, buffer, id) = opening::map(&mut fixture);
        window.role.set_max_size(180, 120); window.surface.commit();
        fixture.desktop.send(Input::Application(id)).unwrap();
        let mut buffers = vec![buffer]; clients::repaint(&mut fixture, &window, id, &mut buffers);
        fixture.client.key_recipients.clear(); tap(&mut fixture, 30);
        assert_eq!(fixture.client.key_recipients, [(Some(window.surface.id()), 30, true), (Some(window.surface.id()), 30, false)]);
        assert!(!fixture.desktop.current.as_ref().unwrap().frame.input.keyboard_modal);
        let expected = fixture.desktop.server.display_snapshot().configuration();
        fixture.desktop.worker.stop().unwrap();
        let mut restarted = single(&runtime, &profile, size, scale);
        assert_eq!(restarted.desktop.server.display_snapshot().configuration(), expected);
        assert_eq!(std::fs::read(&path).unwrap(), saved);
        assert_eq!(restarted.desktop.server.keyboard_snapshot().settings.layout, "br");
        assert_eq!(restarted.output_pixels(B).0.rotation, QuarterTurn::Clockwise90);
        restarted.desktop.worker.stop().unwrap();
    }
}
