use super::*;

struct Peer {
    connection: Connection,
    queue: EventQueue<Client>,
    client: Client,
}

impl Peer {
    fn connect(fixture: &mut Fixture) -> Self {
        let (server, client) = std::os::unix::net::UnixStream::pair().unwrap();
        fixture.desktop.server.insert_authorized_client(server, vec![Capability::Capture], false).unwrap();
        let connection = Connection::from_socket(client).unwrap();
        let queue = connection.new_event_queue();
        connection.display().get_registry(&queue.handle(), ());
        let mut peer = Self { connection, queue, client: Client::default() };
        peer.exchange(fixture);
        peer
    }

    fn exchange(&mut self, fixture: &mut Fixture) {
        synchronize(&mut fixture.desktop.server, &self.connection, &mut self.queue, &mut self.client).unwrap();
    }

    fn session(&mut self, fixture: &mut Fixture) -> (Source, Session) {
        self.session_options(fixture, Options::empty())
    }

    fn session_options(&mut self, fixture: &mut Fixture, options: Options) -> (Source, Session) {
        let qh = self.queue.handle();
        let source = self.client.capture.outputs.as_ref().unwrap().create_source(self.client.output.as_ref().unwrap(), &qh, ());
        let session = self.client.capture.manager.as_ref().unwrap().create_session(&source, options, &qh, ());
        self.exchange(fixture);
        (source, session)
    }

    fn request(&mut self, fixture: &mut Fixture, session: &Session, size: [u32; 2]) -> Pending {
        let pending = Pending::from_client(&self.client, &self.queue, session, size);
        self.exchange(fixture);
        pending
    }

    fn submitted(&mut self, fixture: &mut Fixture, pending: &Pending) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            fixture.tick();
            fixture.composition.capture(&mut fixture.renderer, &mut fixture.desktop, true).unwrap();
            self.exchange(fixture);
            assert_eq!(self.client.capture.ready, pending.ready);
            assert_eq!(self.client.capture.failures.len(), pending.failures);
            assert!(pending.pixels().chunks_exact(4).all(|pixel| pixel == SENTINEL));
            if fixture.composition.capture_test_state()[1] > 0 { return; }
            assert!(Instant::now() < deadline, "The independent capture peer never submitted its real GLES readback");
        }
    }

    fn completed(&mut self, fixture: &mut Fixture, pending: &Pending) -> Vec<u8> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            fixture.tick();
            fixture.composition.capture(&mut fixture.renderer, &mut fixture.desktop, true).unwrap();
            self.exchange(fixture);
            assert_eq!(self.client.capture.failures.len(), pending.failures);
            if self.client.capture.ready == pending.ready + 1 { return pending.pixels(); }
            assert!(Instant::now() < deadline, "The independent capture peer never received its real public pixels");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn destroy_pending(&mut self, fixture: &mut Fixture, pending: Pending) {
        pending.frame.destroy();
        pending.buffer.destroy();
        self.exchange(fixture);
    }

    fn rejected(&mut self, fixture: &mut Fixture, pending: &Pending) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            fixture.tick();
            fixture.composition.capture(&mut fixture.renderer, &mut fixture.desktop, true).unwrap();
            self.exchange(fixture);
            assert_eq!(self.client.capture.ready, pending.ready);
            assert!(pending.pixels().chunks_exact(4).all(|pixel| pixel == SENTINEL));
            if self.client.capture.failures.len() == pending.failures + 1 && fixture.composition.capture_test_state() == [0; 4] { return; }
            assert!(Instant::now() < deadline, "The retired independent readback did not fail and release its GPU resources");
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

fn drained(fixture: &mut Fixture) {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut settled = None;
    let mut publications = fixture.desktop.worker.test_activity().1;
    loop {
        fixture.tick();
        fixture.composition.capture(&mut fixture.renderer, &mut fixture.desktop, true).unwrap();
        let clear = fixture.desktop.server.capture_status().sessions.is_empty()
            && fixture.desktop.server.capture_demand().is_empty()
            && !fixture.desktop.server.capture_pending()
            && fixture.composition.capture_test_state() == [0; 4];
        let (idle, now) = fixture.desktop.worker.test_activity();
        if !clear || !idle || now != publications { settled = None; publications = now; }
        else if settled.get_or_insert_with(Instant::now).elapsed() >= Duration::from_millis(250) { return; }
        assert!(Instant::now() < deadline, "Capture teardown retained work or failed to return the worker to idle");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn quiet(fixture: &Fixture) -> u64 {
    let (idle, before) = fixture.desktop.worker.test_activity();
    assert!(idle);
    let deadline = Instant::now() + Duration::from_millis(600);
    while Instant::now() < deadline {
        assert!(fixture.desktop.worker.failure().is_none());
        assert_eq!(fixture.desktop.worker.test_activity(), (true, before), "Capture-only work continued after the final demand ended");
        std::thread::sleep(Duration::from_millis(3));
    }
    before
}

fn mode(fixture: &mut Fixture, size: [u32; 2]) {
    fixture.desktop.server.configure_outputs(&[Monitor { id: OutputId(1), name: "Capture-Lifecycle".into(), origin: [0, 0], size: size.map(|value| value as i32) }]).unwrap();
    fixture.desktop.server.registered_output_catalog().unwrap();
    fixture.desktop.invalidate_outputs().unwrap();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry|
        geometry.outputs.iter().any(|output| output.id == OutputId(1) && output.physical_size == size))));
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn twenty_actual_capture_session_resize_stop_and_disconnect_cycles_retire_gpu_resources_and_return_to_quiet_owner_input() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut harness = Harness::new();
    harness.server.configure_outputs(&[Monitor { id: OutputId(1), name: "Capture-Lifecycle".into(), origin: [0, 0], size: [1000, 720] }]).unwrap();
    let window = harness.window();
    let _buffer = paint(&mut harness, &window, [320, 240], [0, 255, 0, 255]);
    let surface = harness.server.windows()[0].id;
    let mut fixture = Fixture::new(harness, &runtime, directory.path());
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry|
        geometry.items.iter().any(|item| item.id == ItemId::Application(surface)))));
    appearance::effects(&mut fixture, true);
    fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        native::workspace_config::set_physics(world, root, 1, false);
        let size = Vec2::new(180.0, 160.0);
        let mut layout = native::layout::LayoutBox::new(size);
        layout.capture_excluded = true;
        world.spawn((native::canvas::CanvasItem { position: bevy::math::DVec2::new(-300.0, 0.0), size }, layout,
            Node::default(), BackgroundColor(Color::srgb(1.0, 0.0, 1.0)), native::workspace::WorkspaceMember(1), ChildOf(root)));
    });
    let mut samples = Vec::new();
    for cycle in 0..24 {
        let before = fixture.client.configures.len();
        let mut peer = Peer::connect(&mut fixture);
        let (source, session) = peer.session(&mut fixture);
        let first = peer.request(&mut fixture, &session, [1000, 720]);
        let pixels = peer.completed(&mut fixture, &first);
        assert!(pixels.chunks_exact(4).filter(|pixel| pixel[0] < 40 && pixel[1] > 220 && pixel[2] < 40).count() > 200);
        assert_eq!(pixels.chunks_exact(4).filter(|pixel| pixel[0] > 220 && pixel[1] < 40 && pixel[2] > 220).count(), 0);
        assert_eq!(fixture.client.configures.len(), before, "Starting capture alone reconfigured the ordinary application");
        peer.destroy_pending(&mut fixture, first);
        let old = peer.request(&mut fixture, &session, [1000, 720]);
        peer.submitted(&mut fixture, &old);
        mode(&mut fixture, [960, 600]);
        peer.rejected(&mut fixture, &old);
        peer.destroy_pending(&mut fixture, old);
        let resized = peer.request(&mut fixture, &session, [960, 600]);
        let pixels = peer.completed(&mut fixture, &resized);
        assert_eq!(pixels.len(), 960 * 600 * 4);
        assert!(pixels.chunks_exact(4).filter(|pixel| pixel[0] < 40 && pixel[1] > 220 && pixel[2] < 40).count() > 200);
        assert_eq!(pixels.chunks_exact(4).filter(|pixel| pixel[0] > 220 && pixel[1] < 40 && pixel[2] > 220).count(), 0);
        peer.destroy_pending(&mut fixture, resized);
        let pending = peer.request(&mut fixture, &session, [960, 600]);
        peer.submitted(&mut fixture, &pending);
        let before = fixture.client.configures.len();
        if cycle % 2 == 0 {
            let id = fixture.desktop.server.capture_status().sessions[0].id;
            fixture.desktop.server.stop_capture_session(id);
            peer.rejected(&mut fixture, &pending);
            peer.destroy_pending(&mut fixture, pending);
            session.destroy();
            source.destroy();
            peer.exchange(&mut fixture);
            rustix::net::shutdown(&peer.connection, rustix::net::Shutdown::Both).unwrap();
        } else {
            rustix::net::shutdown(&peer.connection, rustix::net::Shutdown::Both).unwrap();
            drained(&mut fixture);
            assert!(pending.pixels().chunks_exact(4).all(|pixel| pixel == SENTINEL));
            drop(pending);
        }
        drop(session);
        drop(source);
        drop(peer);
        drained(&mut fixture);
        assert_eq!(fixture.client.configures.len(), before, "Stopping capture alone reconfigured the ordinary application");
        mode(&mut fixture, [1000, 720]);
        drained(&mut fixture);
        let sample = fixture.inspect(|canvas| canvas.capture_test_resources());
        assert_eq!(sample.sessions, 0, "The worker retained a sharing session after host teardown");
        assert_eq!(sample.indicators, 0, "The ordinary UI retained its sharing indicator after host teardown");
        drained(&mut fixture);
        let publications = quiet(&fixture);
        assert!(fixture.desktop.current.as_ref().unwrap().frame.capture_source().is_some_and(|capture|
            fixture.desktop.current.as_ref().unwrap().frame.capture_dmabuf(OutputId(1), capture.revision, capture.mapping_revision).is_none()));
        assert!(appearance::pixels(&mut fixture, &format!("capture-teardown-{cycle}")).chunks_exact(4)
            .any(|pixel| pixel[0] > 220 && pixel[1] < 40 && pixel[2] > 220), "The normal owner lost its private content during capture teardown");
        samples.push((cycle, publications, sample));
        super::super::configuration::application_input(&mut fixture, surface, &window);
    }
    let warm = &samples[..4];
    for (_, _, sample) in &samples[4..] {
        for index in 0..3 { assert!(sample.assets[index] <= warm.iter().map(|sample| sample.2.assets[index]).max().unwrap(), "Capture assets grew after warmup: {samples:?}"); }
        assert!(sample.cameras <= warm.iter().map(|sample| sample.2.cameras).max().unwrap(), "Capture retained a temporary camera: {samples:?}");
        assert!(sample.shadow_views <= warm.iter().map(|sample| sample.2.shadow_views).max().unwrap(), "Capture retained temporary shadow views: {samples:?}");
        for resource in 0..5 {
            for counter in 0..2 { assert!(sample.gpu[resource][counter] <= warm.iter().map(|sample| sample.2.gpu[resource][counter]).max().unwrap(), "Capture GPU resources grew after warmup: {samples:?}"); }
        }
        if let Some(bytes) = sample.allocated_bytes {
            assert!(bytes <= warm.iter().filter_map(|sample| sample.2.allocated_bytes).max().unwrap(), "Capture GPU allocations grew after warmup: {samples:?}");
        }
    }
    let late = &samples[samples.len() - 10..];
    assert!(!(late.windows(2).all(|pair| pair[1].2.descriptors >= pair[0].2.descriptors)
        && late.last().unwrap().2.descriptors > late.first().unwrap().2.descriptors), "Capture descriptors grew monotonically: {samples:?}");
    if let Some(directory) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(std::path::Path::new(&directory).join("capture-session-resources.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "measured_cycles": 20, "warmup_cycles": 4, "samples": &samples,
            "gpu_order": ["textures", "texture_views", "buffers", "bind_groups", "samplers"],
            "gpu_fields": ["kept_from_user", "allocated_ids"], "quiet_interval_ms": 600,
        })).unwrap()).unwrap();
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn disconnecting_an_independent_capturer_cannot_write_its_retained_readback_or_retire_the_ordinary_application() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut harness = Harness::connect(Server::new([1000, 720]).unwrap());
    let window = harness.window();
    let _buffer = paint(&mut harness, &window, [320, 240], [0, 255, 0, 255]);
    let surface = harness.server.windows()[0].id;
    let mut fixture = Fixture::new(harness, &runtime, directory.path());
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry|
        geometry.items.iter().any(|item| item.id == ItemId::Application(surface)))));
    appearance::effects(&mut fixture, true);
    let mut peer = Peer::connect(&mut fixture);
    let (_source, session) = peer.session(&mut fixture);
    let warmup = peer.request(&mut fixture, &session, [1000, 720]);
    let pixels = peer.completed(&mut fixture, &warmup);
    assert!(pixels.chunks_exact(4).filter(|pixel| pixel[0] < 40 && pixel[1] > 220 && pixel[2] < 40).count() > 200);
    peer.destroy_pending(&mut fixture, warmup);
    let pending = peer.request(&mut fixture, &session, [1000, 720]);
    peer.submitted(&mut fixture, &pending);
    assert_eq!(fixture.desktop.server.capture_status().sessions.len(), 1);
    rustix::net::shutdown(&peer.connection, rustix::net::Shutdown::Both).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        fixture.tick();
        fixture.composition.capture(&mut fixture.renderer, &mut fixture.desktop, true).unwrap();
        assert!(pending.pixels().chunks_exact(4).all(|pixel| pixel == SENTINEL), "A disconnected peer's queued job wrote its retained SHM");
        if fixture.desktop.server.capture_status().sessions.is_empty()
            && fixture.desktop.server.capture_demand().is_empty()
            && !fixture.desktop.server.capture_pending()
            && fixture.composition.capture_test_state() == [0; 4]
        { break; }
        assert!(Instant::now() < deadline, "The disconnected capture peer retained a session, demand lease or GPU readback");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(fixture.desktop.server.windows().len(), 1);
    assert_eq!(fixture.desktop.server.windows()[0].id, surface);
    super::super::configuration::application_input(&mut fixture, surface, &window);
    assert!(fixture.desktop.running);
    assert_eq!(peer.client.capture.ready, pending.ready);
}

fn replacement(fixture: &mut Fixture) -> (ClientWindow, wl_buffer::WlBuffer, SurfaceId) {
    let previous = fixture.desktop.server.windows().iter().map(|window| window.id).collect::<std::collections::BTreeSet<_>>();
    let qh = fixture.queue.handle();
    let surface = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
    let xdg = fixture.client.shell.as_ref().unwrap().get_xdg_surface(&surface, &qh, ());
    let role = xdg.get_toplevel(&qh, ());
    role.set_title("A real Wayland window".into());
    role.set_app_id("social.lince.park.test".into());
    surface.commit();
    fixture.exchange();
    let buffer = privacy::buffer(fixture, [320, 240], [255, 0, 0, 255]);
    surface.attach(Some(&buffer), 0, 0);
    surface.damage_buffer(0, 0, 320, 240);
    surface.commit();
    fixture.until(|fixture| fixture.desktop.server.windows().iter().any(|window| window.mapped && !previous.contains(&window.id)));
    let id = fixture.desktop.server.windows().iter().find(|window| window.mapped && !previous.contains(&window.id)).unwrap().id;
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry|
        geometry.items.iter().any(|item| item.id == ItemId::Application(id)))));
    (ClientWindow { surface, xdg, role }, buffer, id)
}

fn point(fixture: &Fixture, surface: SurfaceId) -> [f64; 2] {
    let item = fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().items.iter()
        .find(|item| item.id == ItemId::Application(surface)).unwrap();
    let origin = item.mapping.surface.origin();
    let size = item.mapping.surface.size();
    item.mapping.canvas_point([origin[0] + size[0] * 0.25, origin[1] + size[1] * 0.75]).unwrap()
}

fn start_drag(fixture: &mut Fixture, window: &ClientWindow, icon: &wl_surface::WlSurface)
    -> (wl_data_device::WlDataDevice, wl_data_source::WlDataSource)
{
    fixture.desktop.server.pointer_button(272, true, 151);
    fixture.exchange();
    let qh = fixture.queue.handle();
    let manager = fixture.client.data_manager.as_ref().unwrap();
    let device = manager.get_data_device(fixture.client.seat.as_ref().unwrap(), &qh, ());
    let source = manager.create_data_source(&qh, ());
    source.offer("text/plain;charset=utf-8".into());
    source.set_actions(wl_data_device_manager::DndAction::Copy);
    device.start_drag(Some(&source), &window.surface, Some(icon), fixture.client.pointer_serial);
    fixture.exchange();
    (device, source)
}

fn orange(pixels: &[u8]) -> usize {
    pixels.chunks_exact(4).filter(|pixel| pixel[2] > 240 && (120..=160).contains(&pixel[1]) && pixel[0] < 15).count()
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn retained_cursor_and_drag_artwork_cannot_inherit_a_replacement_origin_without_a_fresh_authorized_request() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for drag in [false, true] {
        for private in [false, true] {
            let profile = directory.path().join(format!("artwork-{drag}-{private}"));
            std::fs::create_dir_all(&profile).unwrap();
            let mut harness = Harness::connect(Server::new([1000, 720]).unwrap());
            let origin = harness.window();
            let _origin_buffer = paint(&mut harness, &origin, [320, 240], [255, 0, 255, 255]);
            let origin_id = harness.server.windows()[0].id;
            let survivor = harness.window();
            let _survivor_buffer = paint(&mut harness, &survivor, [320, 240], [0, 255, 0, 255]);
            let survivor_id = harness.server.windows().iter().find(|window| window.id != origin_id).unwrap().id;
            let mut fixture = Fixture::new(harness, &runtime, &profile);
            fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry|
                [origin_id, survivor_id].iter().all(|id| geometry.items.iter().any(|item| item.id == ItemId::Application(*id))))));
            appearance::effects(&mut fixture, true);
            if private { exclude(&mut fixture, origin_id); }
            let qh = fixture.queue.handle();
            let pointer = fixture.client.seat.as_ref().unwrap().get_pointer(&qh, ());
            let location = point(&fixture, origin_id);
            fixture.desktop.move_pointer(location, 100).unwrap();
            fixture.until(|fixture| fixture.desktop.server.capture_pointer_owner() == CaptureContentOwner::Window(origin_id)
                && fixture.client.pointer_enter_serial != 0);
            let icon = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
            let size = if drag { [64, 48] } else { [32, 24] };
            let icon_buffer = privacy::buffer(&fixture, size, [0, 140, 255, 255]);
            let old_drag = if drag { Some(start_drag(&mut fixture, &origin, &icon)) }
                else { pointer.set_cursor(fixture.client.pointer_enter_serial, Some(&icon), 0, 0); None };
            icon.attach(Some(&icon_buffer), 0, 0);
            icon.damage_buffer(0, 0, size[0] as i32, size[1] as i32);
            icon.commit();
            fixture.until(|fixture| if drag {
                fixture.desktop.server.capture_drag_owner() == CaptureContentOwner::Window(origin_id) && !fixture.desktop.server.drag_nodes().is_empty()
            } else { fixture.desktop.server.capture_cursor_owner() == CaptureContentOwner::Window(origin_id)
                && matches!(fixture.desktop.server.cursor(), CursorImageStatus::Surface(surface) if surface.id().protocol_id() == icon.id().protocol_id()) });
            let mut peer = Peer::connect(&mut fixture);
            let (source, session) = peer.session_options(&mut fixture, Options::PaintCursors);
            let first = peer.request(&mut fixture, &session, [1000, 720]);
            let pixels = peer.completed(&mut fixture, &first);
            if private { assert_eq!(orange(&pixels), 0); }
            else { assert!(orange(&pixels) > 100, "Public same-client artwork never became visible"); }
            peer.destroy_pending(&mut fixture, first);
            let pending = peer.request(&mut fixture, &session, [1000, 720]);
            peer.submitted(&mut fixture, &pending);
            origin.role.destroy();
            origin.xdg.destroy();
            origin.surface.destroy();
            fixture.until(|fixture| !fixture.desktop.server.windows().iter().any(|window| window.id == origin_id)
                && fixture.desktop.server.capture_policy_gate().read().is_some_and(|read| read.policy().is_some_and(|policy| !policy.windows.contains_key(&origin_id))));
            peer.rejected(&mut fixture, &pending);
            peer.destroy_pending(&mut fixture, pending);
            let (new, _new_buffer, new_id) = replacement(&mut fixture);
            assert_ne!(new_id, origin_id);
            if drag {
                assert_eq!(fixture.desktop.server.capture_drag_owner(), CaptureContentOwner::Unknown);
                fixture.desktop.server.pointer_motion(None, 152).unwrap();
                fixture.desktop.server.pointer_button(272, false, 153);
                fixture.until(|fixture| !fixture.desktop.server.dragging());
            }
            let location = point(&fixture, new_id);
            fixture.desktop.move_pointer(location, 160).unwrap();
            fixture.until(|fixture| fixture.desktop.server.capture_pointer_owner() == CaptureContentOwner::Window(new_id));
            if !drag { assert_ne!(fixture.desktop.server.capture_cursor_owner(), CaptureContentOwner::Window(new_id)); }
            let fresh = peer.request(&mut fixture, &session, [1000, 720]);
            let pixels = peer.completed(&mut fixture, &fresh);
            assert_eq!(orange(&pixels), 0, "Retained artwork inherited the new same-client window's public ownership");
            assert!(pixels.chunks_exact(4).filter(|pixel| pixel[0] > 220 && pixel[1] < 40 && pixel[2] < 40).count() > 200);
            peer.destroy_pending(&mut fixture, fresh);
            let new_drag = if drag { Some(start_drag(&mut fixture, &new, &icon)) }
                else { pointer.set_cursor(fixture.client.pointer_enter_serial, Some(&icon), 0, 0); None };
            fixture.until(|fixture| if drag { fixture.desktop.server.capture_drag_owner() == CaptureContentOwner::Window(new_id) }
                else { fixture.desktop.server.capture_cursor_owner() == CaptureContentOwner::Window(new_id) });
            let accepted = peer.request(&mut fixture, &session, [1000, 720]);
            assert!(orange(&peer.completed(&mut fixture, &accepted)) > 100, "A fresh authorized public artwork request was incorrectly blacklisted");
            peer.destroy_pending(&mut fixture, accepted);
            if drag {
                fixture.desktop.server.pointer_motion(None, 170).unwrap();
                fixture.desktop.server.pointer_button(272, false, 171);
                fixture.until(|fixture| !fixture.desktop.server.dragging());
            } else { pointer.set_cursor(fixture.client.pointer_enter_serial, None, 0, 0); }
            for (device, source) in old_drag.into_iter().chain(new_drag) { source.destroy(); device.release(); }
            icon.destroy();
            icon_buffer.destroy();
            session.destroy();
            source.destroy();
            peer.exchange(&mut fixture);
            rustix::net::shutdown(&peer.connection, rustix::net::Shutdown::Both).unwrap();
            super::super::configuration::application_input(&mut fixture, survivor_id, &survivor);
            drained(&mut fixture);
        }
    }
}
