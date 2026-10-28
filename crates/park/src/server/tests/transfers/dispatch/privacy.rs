use super::*;

mod settings;
use crate::launch::Capability;
use std::os::unix::fs::FileExt;
use wayland_protocols::ext::image_copy_capture::v1::client::{ext_image_copy_capture_manager_v1::Options, ext_image_copy_capture_session_v1::ExtImageCopyCaptureSessionV1};

pub(super) fn exclude_window(fixture: &mut Fixture, id: SurfaceId, excluded: bool) {
    fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        world.resource::<lince_interface::host::capture::PolicyGate>().invalidate();
        let entity = world.query::<(Entity, &native::layout::desktop::ApplicationWindow)>().iter(world)
            .find_map(|(entity, window)| (window.surface == id).then_some(entity)).unwrap();
        world.get_mut::<native::layout::LayoutBox>(entity).unwrap().capture_excluded = excluded;
    });
    fixture.until(|fixture| fixture.desktop.worker.test_activity().0
        && fixture.desktop.server.capture_policy_gate().read().is_some_and(|read|
            read.policy().is_some_and(|policy| !policy.unresolved_images && policy.windows.get(&id) == Some(&excluded))
                && fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.capture_source()
                    .is_some_and(|source| source.revision == read.revision()))));
}

pub(super) fn buffer(fixture: &Fixture, size: [u32; 2], color: [u8; 4]) -> wl_buffer::WlBuffer {
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(&color.repeat((size[0] * size[1]) as usize)).unwrap();
    let qh = fixture.queue.handle();
    let pool = fixture.client.shm.as_ref().unwrap().create_pool(file.as_fd(), (size[0] * size[1] * 4) as i32, &qh, ());
    let buffer = pool.create_buffer(0, size[0] as i32, size[1] as i32, (size[0] * 4) as i32, wl_shm::Format::Argb8888, &qh, ());
    pool.destroy();
    buffer
}

fn capture(fixture: &mut Fixture, session: &ExtImageCopyCaptureSessionV1) -> Vec<u8> {
    let [width, height] = *fixture.client.capture.sizes.last().expect("The real capture session did not announce its buffer size");
    let bytes = u64::from(width) * u64::from(height) * 4;
    let file = tempfile::tempfile().unwrap();
    file.set_len(bytes).unwrap();
    let qh = fixture.queue.handle();
    let pool = fixture.client.shm.as_ref().unwrap().create_pool(file.as_fd(), bytes as i32, &qh, ());
    let target = pool.create_buffer(0, width as i32, height as i32, width as i32 * 4, wl_shm::Format::Argb8888, &qh, ());
    pool.destroy();
    let frame = session.create_frame(&qh, ());
    frame.attach_buffer(&target);
    frame.damage_buffer(0, 0, width as i32, height as i32);
    frame.capture();
    let before = fixture.client.capture.ready;
    let failures = fixture.client.capture.failures.len();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        fixture.tick();
        fixture.composition.capture(&mut fixture.renderer, &mut fixture.desktop, true).unwrap();
        fixture.exchange();
        assert_eq!(fixture.client.capture.failures.len(), failures, "The real capture frame failed: {:?}", fixture.client.capture.failures);
        if fixture.client.capture.ready > before { break; }
        assert!(Instant::now() < deadline, "The real filtered compositor frame was never delivered");
        std::thread::sleep(Duration::from_millis(2));
    }
    let mut pixels = vec![0; bytes as usize];
    file.read_exact_at(&mut pixels, 0).unwrap();
    frame.destroy();
    target.destroy();
    fixture.exchange();
    pixels
}

fn private_pixels(pixels: &[u8]) -> usize {
    pixels.chunks_exact(4).filter(|pixel| pixel[2] > 220 && pixel[1] < 40 && pixel[0] > 140).count()
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn real_output_capture_keeps_public_updates_and_excludes_private_windows_and_owned_popups() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut harness = Harness::connect_authorized(Server::new([1000, 720]).unwrap(), Some(vec![Capability::Capture, Capability::WindowList]));
    let private = harness.window();
    let _private_buffer = paint(&mut harness, &private, [320, 240], [180, 0, 255, 255]);
    let private_id = harness.server.windows()[0].id;
    let public = harness.window();
    let _public_buffer = paint(&mut harness, &public, [320, 240], [0, 255, 0, 255]);
    let public_id = harness.server.windows().into_iter().find(|window| window.id != private_id).unwrap().id;
    let mut fixture = Fixture::new(harness, &runtime, directory.path());
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry| [private_id, public_id].iter().all(|id| geometry.items.iter().any(|item| item.id == ItemId::Application(*id))))));
    fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        native::workspace_config::set_physics(world, root, 1, false);
        world.resource::<lince_interface::host::capture::PolicyGate>().invalidate();
        let entity = world.query::<(Entity, &native::layout::desktop::ApplicationWindow)>().iter(world).find(|(_, window)| window.surface == private_id).unwrap().0;
        world.get_mut::<native::layout::LayoutBox>(entity).unwrap().capture_excluded = true;
    });
    fixture.until(|fixture| fixture.desktop.server.capture_policy_gate().read().is_some_and(|policy| policy.policy().is_some_and(|policy| policy.windows.get(&private_id) == Some(&true))));
    let qh = fixture.queue.handle();
    let source = fixture.client.capture.outputs.as_ref().unwrap().create_source(fixture.client.output.as_ref().unwrap(), &qh, ());
    let session = fixture.client.capture.manager.as_ref().unwrap().create_session(&source, Options::PaintCursors, &qh, ());
    fixture.until(|fixture| fixture.desktop.server.capture_status().sessions.len() == 1);
    let pixels = capture(&mut fixture, &session);
    assert_eq!(private_pixels(&pixels), 0);
    assert!(pixels.chunks_exact(4).filter(|pixel| pixel[2] < 40 && pixel[1] > 220 && pixel[0] < 40).count() > 200);
    let positioner = fixture.client.shell.as_ref().unwrap().create_positioner(&qh, ());
    positioner.set_size(110, 70);
    positioner.set_anchor_rect(16, 16, 1, 1);
    positioner.set_offset(550, -90);
    let popup_surface = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
    let popup_xdg = fixture.client.shell.as_ref().unwrap().get_xdg_surface(&popup_surface, &qh, ());
    let popup = popup_xdg.get_popup(Some(&private.xdg), &positioner, &qh, ());
    popup_surface.commit();
    fixture.tick();
    let popup_buffer = buffer(&fixture, [110, 70], [255, 0, 255, 255]);
    popup_surface.attach(Some(&popup_buffer), 0, 0);
    popup_surface.damage_buffer(0, 0, 110, 70);
    popup_surface.commit();
    fixture.until(|fixture| fixture.desktop.server.windows().iter().find(|window| window.id == private_id).is_some_and(|window| !window.popups.is_empty()));
    assert_eq!(private_pixels(&capture(&mut fixture, &session)), 0);
    let cyan = buffer(&fixture, [320, 240], [255, 255, 0, 255]);
    public.surface.attach(Some(&cyan), 0, 0);
    public.surface.damage_buffer(0, 0, 320, 240);
    public.surface.commit();
    fixture.tick();
    let version = fixture.desktop.server.windows().into_iter().find(|window| window.id == public_id).unwrap().nodes[0].content.version;
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.windows.iter().any(|window| window.id == public_id && window.nodes.iter().any(|node| node.content.version == version))));
    let pixels = capture(&mut fixture, &session);
    assert_eq!(private_pixels(&pixels), 0);
    assert!(pixels.chunks_exact(4).filter(|pixel| pixel[2] < 40 && pixel[1] > 220 && pixel[0] > 220).count() > 200);
    let pointer = fixture.client.seat.as_ref().unwrap().get_pointer(&qh, ());
    fixture.tick();
    for (surface, color, private_cursor) in [(private_id, [180, 0, 255, 255], true), (public_id, [255, 20, 20, 255], false)] {
        let item = fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().items.iter().find(|item| item.id == ItemId::Application(surface)).unwrap();
        let local = [item.mapping.surface.origin()[0] + item.mapping.surface.size()[0] * 0.25, item.mapping.surface.origin()[1] + item.mapping.surface.size()[1] * 0.75];
        let point = item.mapping.canvas_point(local).unwrap();
        fixture.desktop.move_pointer(point, 100).unwrap();
        fixture.until(|fixture| fixture.desktop.server.capture_pointer_owner() == CaptureContentOwner::Window(surface) && fixture.client.pointer_enter_serial != 0);
        let cursor_surface = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
        let cursor_buffer = buffer(&fixture, [32, 24], color);
        pointer.set_cursor(fixture.client.pointer_enter_serial, Some(&cursor_surface), 0, 0);
        cursor_surface.attach(Some(&cursor_buffer), 0, 0);
        cursor_surface.damage_buffer(0, 0, 32, 24);
        cursor_surface.commit();
        fixture.until(|fixture| matches!(fixture.desktop.server.cursor(), CursorImageStatus::Surface(surface) if surface.id().protocol_id() == cursor_surface.id().protocol_id()));
        let pixels = capture(&mut fixture, &session);
        assert_eq!(private_pixels(&pixels), 0);
        if !private_cursor { assert!(pixels.chunks_exact(4).filter(|pixel| pixel[0] > 220 && pixel[1] < 40 && pixel[2] < 40).count() > 100); }
        cursor_surface.destroy();
        cursor_buffer.destroy();
    }
    pointer.set_cursor(fixture.client.pointer_enter_serial, None, 0, 0);
    fixture.until(|fixture| matches!(fixture.desktop.server.cursor(), CursorImageStatus::Hidden));
    let _tablet_seat = fixture.client.tablet.manager.as_ref().unwrap().get_tablet_seat(fixture.client.seat.as_ref().unwrap(), &qh, ());
    assert!(fixture.desktop.server.tablet_device(901, smithay::wayland::tablet_manager::TabletDescriptor {
        name: "Capture privacy tablet".into(), usb_id: None, syspath: None,
    }));
    assert!(fixture.desktop.server.tablet_tool(901, 902, smithay::backend::input::TabletToolDescriptor {
        tool_type: smithay::backend::input::TabletToolType::Pen, hardware_serial: 902, hardware_id_wacom: 0,
        capabilities: smithay::backend::input::TabletToolCapabilities::PRESSURE,
    }));
    fixture.exchange();
    let tool = fixture.client.tablet.tools.last().unwrap().clone();
    for (owner, color, private) in [(private_id, [180, 0, 255, 255], true), (public_id, [255, 20, 20, 255], false)] {
        let surface = fixture.desktop.server.windows().into_iter().find(|window| window.id == owner).unwrap().surface;
        assert!(fixture.desktop.server.tablet_motion(902, OutputId(1), [850.0, 620.0], Some((surface, [20.0, 20.0])), false, Default::default(), 140));
        fixture.exchange();
        let cursor = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
        let artwork = buffer(&fixture, [32, 24], color);
        tool.set_cursor(fixture.client.tablet.serials[&tool.id().protocol_id()], Some(&cursor), 7, 5);
        cursor.attach(Some(&artwork), 0, 0);
        cursor.damage_buffer(0, 0, 32, 24);
        cursor.commit();
        fixture.until(|fixture| fixture.desktop.server.tablet_cursors().iter().any(|pen|
            pen.tool == 902 && pen.owner == CaptureContentOwner::Window(owner)
                && matches!(&pen.image, CursorImageStatus::Surface(surface) if surface.id().protocol_id() == cursor.id().protocol_id())));
        let pixels = capture(&mut fixture, &session);
        assert_eq!(private_pixels(&pixels), 0);
        if !private {
            let width = fixture.client.capture.sizes.last().unwrap()[0] as usize;
            let painted = pixels.chunks_exact(4).enumerate().filter_map(|(index, pixel)|
                (pixel[0] > 220 && pixel[1] < 40 && pixel[2] < 40).then_some([index % width, index / width])).collect::<Vec<_>>();
            assert!(painted.len() > 500, "A public pen cursor was hidden with its private sibling");
            for axis in 0..2 {
                let minimum = painted.iter().map(|point| point[axis]).min().unwrap() as i32;
                let maximum = painted.iter().map(|point| point[axis]).max().unwrap() as i32;
                let expected = [843, 615][axis];
                assert!((minimum - expected).abs() <= 1, "Pen hotspot was not applied to capture pixels");
                assert!((maximum - (expected + [31, 23][axis])).abs() <= 1);
            }
        }
        fixture.desktop.server.tablet_out(902, 141);
        cursor.destroy(); artwork.destroy(); fixture.exchange();
    }
    fixture.desktop.server.tablet_remove_device(901, 142);
    fixture.exchange();
    assert!(fixture.desktop.server.tablet_cursors().is_empty());
    for (surface, window, color, private_drag) in [
        (private_id, &private, [180, 0, 255, 255], true),
        (public_id, &public, [0, 140, 255, 255], false),
    ] {
        let item = fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().items.iter().find(|item| item.id == ItemId::Application(surface)).unwrap();
        let local = [item.mapping.surface.origin()[0] + item.mapping.surface.size()[0] * 0.25, item.mapping.surface.origin()[1] + item.mapping.surface.size()[1] * 0.75];
        let point = item.mapping.canvas_point(local).unwrap();
        fixture.desktop.move_pointer(point, 150).unwrap();
        fixture.until(|fixture| fixture.desktop.server.capture_pointer_owner() == CaptureContentOwner::Window(surface));
        fixture.desktop.server.pointer_button(272, true, 151);
        fixture.exchange();
        let manager = fixture.client.data_manager.as_ref().unwrap();
        let device = manager.get_data_device(fixture.client.seat.as_ref().unwrap(), &qh, ());
        let drag_source = manager.create_data_source(&qh, ());
        drag_source.offer("text/plain;charset=utf-8".into());
        drag_source.set_actions(wl_data_device_manager::DndAction::Copy);
        let icon = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
        let icon_buffer = buffer(&fixture, [64, 48], color);
        device.start_drag(Some(&drag_source), &window.surface, Some(&icon), fixture.client.pointer_serial);
        icon.attach(Some(&icon_buffer), 0, 0);
        icon.damage_buffer(0, 0, 64, 48);
        icon.commit();
        fixture.until(|fixture| fixture.desktop.server.capture_drag_owner() == CaptureContentOwner::Window(surface)
            && !fixture.desktop.server.drag_nodes().is_empty());
        fixture.desktop.move_pointer([850.0, 620.0], 152).unwrap();
        fixture.tick();
        assert_eq!(fixture.desktop.server.capture_drag_owner(), CaptureContentOwner::Window(surface));
        let pixels = capture(&mut fixture, &session);
        assert_eq!(private_pixels(&pixels), 0);
        if !private_drag {
            assert!(pixels.chunks_exact(4).filter(|pixel| pixel[2] > 240 && (120..=160).contains(&pixel[1]) && pixel[0] < 15).count() > 1000,
                "Public drag artwork was incorrectly hidden with a private sibling on the same connection");
        }
        fixture.desktop.server.pointer_motion(None, 153).unwrap();
        fixture.desktop.server.pointer_button(272, false, 154);
        fixture.until(|fixture| !fixture.desktop.server.dragging());
        assert_eq!(fixture.desktop.server.capture_drag_owner(), CaptureContentOwner::Unknown);
        icon.destroy();
        icon_buffer.destroy();
        drag_source.destroy();
        device.release();
        fixture.tick();
    }
    let owner = crate::presentation::probe_pixels(fixture.desktop.current.as_ref().unwrap().frame.dmabuf().unwrap()).unwrap();
    assert!(owner.chunks_exact(4).filter(|pixel| pixel[0] > 220 && pixel[1] < 40 && pixel[2] > 140).count() > 200);
    popup.destroy();
    popup_xdg.destroy();
    popup_surface.destroy();
    positioner.destroy();
    session.destroy();
    source.destroy();
    fixture.until(|fixture| fixture.desktop.server.capture_status().sessions.is_empty() && fixture.desktop.server.capture_demand().is_empty());
    assert!(fixture.desktop.running);
}
