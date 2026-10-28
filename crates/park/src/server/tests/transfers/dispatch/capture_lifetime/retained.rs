use super::*;

fn storage(client: &Client, queue: &EventQueue<Client>, size: [u32; 2], color: [u8; 4]) -> (std::fs::File, wl_buffer::WlBuffer) {
    let mut file = tempfile::tempfile().unwrap();
    let bytes = color.repeat(size[0] as usize * size[1] as usize);
    file.write_all(&bytes).unwrap();
    let qh = queue.handle();
    let pool = client.shm.as_ref().unwrap().create_pool(file.as_fd(), bytes.len() as i32, &qh, ());
    let buffer = pool.create_buffer(0, size[0] as i32, size[1] as i32, (size[0] * 4) as i32, wl_shm::Format::Argb8888, &qh, ());
    pool.destroy();
    (file, buffer)
}

fn snapshot(fixture: &Fixture, surface: SurfaceId) -> crate::server::WindowSnapshot {
    fixture.desktop.server.windows().into_iter().find(|window| window.id == surface && window.mapped).unwrap()
}

fn window_session(fixture: &mut Fixture) -> (Source, Session) {
    let surface = fixture.desktop.server.windows().into_iter().find(|window| window.mapped).unwrap().id;
    public(fixture, surface);
    fixture.exchange();
    let qh = fixture.queue.handle();
    let source = fixture.client.capture.toplevels.as_ref().unwrap().create_source(fixture.client.foreign_handles.last().unwrap(), &qh, ());
    let session = fixture.client.capture.manager.as_ref().unwrap().create_session(&source, Options::empty(), &qh, ());
    fixture.exchange();
    (source, session)
}

fn public(fixture: &mut Fixture, surface: SurfaceId) {
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry|
        geometry.items.iter().any(|item| item.id == ItemId::Application(surface))))
        && snapshot(fixture, surface).nodes.first().is_some_and(|node| node.content.is_released())
        && fixture.desktop.server.capture_policy_gate().read().is_some_and(|read|
            read.policy().is_some_and(|policy| policy.windows.get(&surface) == Some(&false))));
}

fn reference(pixels: &[u8], expected: [u8; 4]) {
    assert!(!pixels.is_empty());
    assert!(pixels.chunks_exact(4).all(|pixel| pixel.iter().zip(expected).all(|(actual, expected)| actual.abs_diff(expected) <= 1)),
        "Direct capture read reused client storage or changed retained source channels");
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn released_shm_reuse_without_commit_preserves_direct_pixels_and_twenty_new_versions_retire_owned_exports() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut harness = Harness::connect_authorized(Server::new([1000, 720]).unwrap(), Some(vec![Capability::Capture, Capability::WindowList]));
    let window = harness.window();
    let initial = paint(&mut harness, &window, [320, 240], [0, 255, 0, 255]);
    let surface = harness.server.windows()[0].id;
    let mut fixture = Fixture::new(harness, &runtime, directory.path());
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.windows.iter().any(|window| window.id == surface)));
    let mut previous = snapshot(&fixture, surface).nodes[0].content.version;
    let mut stable = None;
    for cycle in 0..20_u8 {
        let size = [320, 240];
        let expected = if cycle % 2 == 0 { [0, 255, 0, 255] } else { [255, 0, 255, 255] };
        let (file, buffer) = storage(&fixture.client, &fixture.queue, size, expected);
        let released = fixture.client.releases;
        window.surface.attach(Some(&buffer), 0, 0);
        window.surface.damage_buffer(0, 0, size[0] as i32, size[1] as i32);
        window.surface.commit();
        fixture.until(|fixture| {
            let source = snapshot(fixture, surface);
            source.nodes[0].content.version != previous && source.nodes[0].content.is_released()
                && fixture.client.releases > released
                && fixture.desktop.current.as_ref().is_some_and(|frame| frame.windows.iter().find(|window| window.id == surface)
                    .is_some_and(|window| window.nodes[0].content.version == source.nodes[0].content.version))
        });
        let captured = snapshot(&fixture, surface).nodes[0].content.clone();
        previous = captured.version;
        assert!(crate::graphics::retained_capture(&captured).unwrap().is_none(), "An uncaptured frame exported application pixels");
        file.write_all_at(&[19, 73, 211, 255].repeat(size[0] as usize * size[1] as usize), 0).unwrap();
        fixture.tick();
        assert_eq!(snapshot(&fixture, surface).nodes[0].content.version, previous);
        let (source, session) = window_session(&mut fixture);
        let pending = Pending::new(&mut fixture, &session, size);
        let demand = fixture.desktop.server.capture_demand();
        assert!(demand.outputs.is_empty());
        assert_eq!(demand.windows, std::collections::BTreeSet::from([surface]));
        submitted(&mut fixture, &pending);
        let exported = crate::graphics::retained_capture(&captured).unwrap().unwrap();
        assert_eq!(fixture.inspect(|canvas| canvas.capture_test_resources().retained_bytes), u64::from(size[0]) * u64::from(size[1]) * 4);
        fixture.inspect(|_| ());
        let reused = crate::graphics::retained_capture(&captured).unwrap().unwrap();
        assert!(Arc::ptr_eq(&exported, &reused), "A demanded pixel version was exported again without a commit");
        reference(&completed(&mut fixture, &pending), expected);
        pending.destroy(&mut fixture);
        session.destroy();
        source.destroy();
        drop(exported);
        drop(reused);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            fixture.tick();
            fixture.composition.capture(&mut fixture.renderer, &mut fixture.desktop, true).unwrap();
            if fixture.desktop.server.capture_demand().is_empty() && fixture.composition.capture_test_state() == [0; 4]
                && crate::graphics::retained_capture(&captured).unwrap().is_none()
            { break; }
            assert!(Instant::now() < deadline, "Retired capture demand retained an export or GPU readback");
            std::thread::sleep(Duration::from_millis(2));
        }
        let resources = fixture.composition.color_test_resources();
        assert_eq!(fixture.inspect(|canvas| canvas.capture_test_resources().retained_bytes), 0);
        if let Some(baseline) = stable { assert_eq!(resources, baseline, "Settled capture resources grew across source versions"); }
        else { stable = Some(resources); }
        super::super::configuration::application_input(&mut fixture, surface, &window);
        buffer.destroy();
        fixture.exchange();
    }
    initial.destroy();
    assert!(fixture.desktop.running);
}

#[derive(Clone, Copy, Debug)]
enum Retirement { Privacy, Stop, Lock, Unmap, Deadline, InactiveSeat }

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn retained_direct_readbacks_reject_privacy_stop_lock_unmap_and_original_deadline_then_recover_fresh_pixels_and_input() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for retirement in [Retirement::Privacy, Retirement::Stop, Retirement::Lock, Retirement::Unmap, Retirement::Deadline, Retirement::InactiveSeat] {
        eprintln!("Retained capture retirement: {retirement:?}");
        let profile = directory.path().join(format!("{retirement:?}"));
        std::fs::create_dir_all(&profile).unwrap();
        let mut harness = Harness::connect_authorized(Server::new([1000, 720]).unwrap(), Some(vec![Capability::Capture, Capability::WindowList]));
        let window = harness.window();
        let initial = paint(&mut harness, &window, [320, 240], [255, 0, 255, 255]);
        let id = harness.server.windows()[0].id;
        let mut fixture = Fixture::new(harness, &runtime, &profile);
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.windows.iter().any(|window| window.id == id)));
        let (source, session) = window_session(&mut fixture);
        let pending = Pending::new(&mut fixture, &session, [320, 240]);
        submitted(&mut fixture, &pending);
        let mut locker = None;
        let mut replacement = None;
        match retirement {
            Retirement::Privacy => exclude(&mut fixture, id),
            Retirement::Stop => {
                let session = fixture.desktop.server.capture_status().sessions[0].id;
                fixture.desktop.server.stop_capture_session(session);
            }
            Retirement::Lock => {
                let qh = fixture.queue.handle();
                let lock = fixture.client.lock_manager.as_ref().unwrap().lock(&qh, ());
                fixture.exchange();
                let surface = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
                let role = lock.get_lock_surface(&surface, fixture.client.output.as_ref().unwrap(), &qh, ());
                fixture.exchange();
                let size = *fixture.client.lock_sizes.last().unwrap();
                let buffer = privacy::buffer(&fixture, size, [20, 20, 20, 255]);
                surface.attach(Some(&buffer), 0, 0); surface.damage_buffer(0, 0, size[0] as i32, size[1] as i32); surface.commit();
                fixture.until(|fixture| fixture.client.lock_replies.last() == Some(&true));
                locker = Some((lock, role, surface, buffer));
            }
            Retirement::Unmap => {
                let title = snapshot(&fixture, id).title;
                window.surface.attach(None, 0, 0); window.surface.commit(); fixture.exchange();
                let next = super::super::opening::map(&mut fixture);
                next.0.role.set_title(title); fixture.exchange();
                assert_ne!(next.2, id);
                replacement = Some(next);
            }
            Retirement::Deadline => std::thread::sleep(Duration::from_millis(2100)),
            Retirement::InactiveSeat => {
                fixture.composition.capture(&mut fixture.renderer, &mut fixture.desktop, false).unwrap();
                std::thread::sleep(Duration::from_millis(2100));
            }
        }
        rejected(&mut fixture, &pending);
        pending.destroy(&mut fixture); session.destroy(); source.destroy(); fixture.exchange();
        if let Some((lock, role, surface, buffer)) = locker {
            lock.unlock_and_destroy(); fixture.until(|fixture| !fixture.desktop.server.locked());
            role.destroy(); surface.destroy(); buffer.destroy(); fixture.exchange();
        }
        if matches!(retirement, Retirement::Privacy) {
            fixture.inspect(move |canvas| {
                let world = canvas.world_mut();
                world.resource::<lince_interface::host::capture::PolicyGate>().invalidate();
                let entity = world.query::<(Entity, &native::layout::desktop::ApplicationWindow)>().iter(world)
                    .find_map(|(entity, window)| (window.surface == id).then_some(entity)).unwrap();
                world.get_mut::<native::layout::LayoutBox>(entity).unwrap().capture_excluded = false;
            });
        }
        let (active, active_id) = replacement.as_ref().map_or((&window, id), |(window, _, id)| (window, *id));
        let fresh_buffer = privacy::buffer(&fixture, [320, 240], [0, 255, 0, 255]);
        active.surface.attach(Some(&fresh_buffer), 0, 0); active.surface.damage_buffer(0, 0, 320, 240); active.surface.commit(); fixture.exchange();
        let version = snapshot(&fixture, active_id).nodes[0].content.version;
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.windows.iter().find(|window| window.id == active_id)
            .is_some_and(|window| window.nodes[0].content.version == version)));
        let (fresh_source, fresh_session) = window_session(&mut fixture);
        let fresh = Pending::new(&mut fixture, &fresh_session, [320, 240]);
        assert!(fixture.desktop.server.capture_demand().windows.contains(&active_id));
        reference(&completed(&mut fixture, &fresh), [0, 255, 0, 255]);
        fresh.destroy(&mut fixture); fresh_session.destroy(); fresh_source.destroy(); fixture.exchange();
        super::super::configuration::application_input(&mut fixture, active_id, active);
        initial.destroy(); fresh_buffer.destroy(); fixture.exchange();
        assert!(fixture.desktop.running);
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn disconnecting_a_direct_capturer_retires_retained_pixels_without_writing_shm_or_disrupting_the_application() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut harness = Harness::connect_authorized(Server::new([1000, 720]).unwrap(), Some(vec![Capability::Capture, Capability::WindowList]));
    let window = harness.window();
    let buffer = paint(&mut harness, &window, [320, 240], [0, 255, 0, 255]);
    let id = harness.server.windows()[0].id;
    let mut fixture = Fixture::new(harness, &runtime, directory.path());
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.windows.iter().any(|window| window.id == id)));
    public(&mut fixture, id);
    let (server, socket) = std::os::unix::net::UnixStream::pair().unwrap();
    fixture.desktop.server.insert_authorized_client(server, vec![Capability::Capture, Capability::WindowList], false).unwrap();
    let connection = Connection::from_socket(socket).unwrap();
    let mut queue = connection.new_event_queue();
    connection.display().get_registry(&queue.handle(), ());
    let mut client = Client::default();
    synchronize(&mut fixture.desktop.server, &connection, &mut queue, &mut client).unwrap();
    synchronize(&mut fixture.desktop.server, &connection, &mut queue, &mut client).unwrap();
    let qh = queue.handle();
    let source = client.capture.toplevels.as_ref().unwrap().create_source(client.foreign_handles.last().unwrap(), &qh, ());
    let session = client.capture.manager.as_ref().unwrap().create_session(&source, Options::empty(), &qh, ());
    synchronize(&mut fixture.desktop.server, &connection, &mut queue, &mut client).unwrap();
    let pending = Pending::from_client(&client, &queue, &session, [320, 240]);
    synchronize(&mut fixture.desktop.server, &connection, &mut queue, &mut client).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        fixture.tick();
        fixture.composition.capture(&mut fixture.renderer, &mut fixture.desktop, true).unwrap();
        synchronize(&mut fixture.desktop.server, &connection, &mut queue, &mut client).unwrap();
        assert_eq!(client.capture.ready, pending.ready);
        assert_eq!(client.capture.failures.len(), pending.failures);
        if fixture.composition.capture_test_state()[1] > 0 { break; }
        assert!(Instant::now() < deadline, "No direct-window readback reached its real GPU fence");
    }
    rustix::net::shutdown(&connection, rustix::net::Shutdown::Both).unwrap();
    let captured = snapshot(&fixture, id).nodes[0].content.clone();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        fixture.tick();
        fixture.composition.capture(&mut fixture.renderer, &mut fixture.desktop, true).unwrap();
        assert!(pending.pixels().chunks_exact(4).all(|pixel| pixel == SENTINEL));
        if fixture.desktop.server.capture_status().sessions.is_empty() && fixture.desktop.server.capture_demand().is_empty()
            && fixture.composition.capture_test_state() == [0; 4] && crate::graphics::retained_capture(&captured).unwrap().is_none()
        { break; }
        assert!(Instant::now() < deadline, "A disconnected capturer retained its demand, export or GPU readback");
        std::thread::sleep(Duration::from_millis(2));
    }
    let (source, session) = window_session(&mut fixture);
    let fresh = Pending::new(&mut fixture, &session, [320, 240]);
    reference(&completed(&mut fixture, &fresh), [0, 255, 0, 255]);
    fresh.destroy(&mut fixture); session.destroy(); source.destroy(); fixture.exchange();
    super::super::configuration::application_input(&mut fixture, id, &window);
    buffer.destroy(); fixture.exchange();
    assert!(fixture.desktop.running);
}
