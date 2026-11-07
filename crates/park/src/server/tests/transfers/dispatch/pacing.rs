use super::*;

fn accepted(fixture: &mut Fixture, ready: impl Fn(&crate::runtime::Completed) -> bool) -> Arc<crate::runtime::Completed> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        fixture.exchange();
        fixture.desktop.dispatch().unwrap();
        fixture.desktop.accept(&mut fixture.composition, &mut fixture.renderer).unwrap();
        assert!(fixture.desktop.worker.failure().is_none());
        if let Some(frame) = fixture.desktop.pending.as_ref().or(fixture.desktop.current.as_ref())
            && ready(frame) && frame.frame.ready().unwrap()
        { return frame.clone(); }
        assert!(Instant::now() < deadline, "The private output-independent frame did not become ready");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn render(fixture: &mut Fixture, frame: &crate::runtime::Completed) -> renderer::Presented {
    let mut target = fixture.renderer.bind(&mut fixture.texture).unwrap();
    fixture.composition.render(&mut fixture.renderer, &mut target, (fixture.size[0] as i32, fixture.size[1] as i32).into(),
        Transform::Normal, &mut fixture.desktop.server, Some(frame), fixture.desktop.pointer).unwrap()
}

#[test]
#[ignore = "requires hardware Vulkan/EGL; real native dispatcher, private client and controlled output submission"]
fn client_callbacks_follow_output_submission_instead_of_native_preparation() {
    let checks = native::laboratory::run_interface_checks().unwrap();
    assert!(checks.first().unwrap().name.ends_with("::protein_area_filter_visible_and_selectable"));
    assert!(checks.iter().all(|check| check.error.is_none()), "{checks:?}");
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut harness = Harness::new();
    let window = harness.window();
    let _red = crate::server::tests::pixel_tests::colored(&mut harness, &window, [255, 0, 0]);
    let mut fixture = Fixture::new(harness, &runtime, directory.path());
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.windows.len() == 1 && frame.frame.geometry.as_ref().is_some_and(|geometry| {
        geometry.items.iter().any(|item| item.id == lince_interface::host::ItemId::Application(frame.windows[0].id))
    })));
    fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        let ids = world.get::<native::workspace::Workspaces>(root).unwrap().entries.iter().map(|entry| entry.id).collect::<Vec<_>>();
        for id in ids { native::workspace_config::set_physics(world, root, id, false); }
    });
    let done = Arc::new(AtomicBool::new(false));
    window.surface.frame(&fixture.queue.handle(), done.clone());
    window.surface.commit();
    fixture.exchange();
    let releases = fixture.client.releases;
    let mut frame = fixture.desktop.current.as_ref().unwrap().clone();
    for _ in 0..4 {
        let serial = fixture.desktop.worker.input(Input::Test(Box::new(|_| {}))).unwrap();
        frame = accepted(&mut fixture, |frame| frame.input_serial >= serial);
        fixture.exchange();
        assert!(!done.load(Ordering::Acquire), "A native preparation invited the client without an output submission");
    }
    let presented = render(&mut fixture, &frame);
    fixture.exchange();
    assert!(!done.load(Ordering::Acquire), "Offscreen composition invited the client without an output submission");
    presented.submitted(&mut fixture.desktop.server, Some(OutputId(2)));
    fixture.exchange();
    assert!(!done.load(Ordering::Acquire), "Another output advanced this client's callback");
    presented.submitted(&mut fixture.desktop.server, Some(OutputId(1)));
    fixture.exchange();
    assert!(done.load(Ordering::Acquire));
    assert_eq!(fixture.client.releases, releases, "A draw invitation released the client's buffer");
    presented.sync.wait().unwrap();
    fixture.desktop.presented(presented, 0).unwrap();
    fixture.exchange();

    let next = Arc::new(AtomicBool::new(false));
    window.surface.frame(&fixture.queue.handle(), next.clone());
    window.surface.commit();
    fixture.exchange();
    fixture.desktop.dispatch().unwrap();
    let current = fixture.desktop.current.as_ref().unwrap().clone();
    let callback_only = render(&mut fixture, &current);
    callback_only.submitted(&mut fixture.desktop.server, Some(OutputId(1)));
    fixture.exchange();
    assert!(next.load(Ordering::Acquire), "A callback-only client needed another native frame");
    callback_only.sync.wait().unwrap();

    let next = Arc::new(AtomicBool::new(false));
    window.surface.frame(&fixture.queue.handle(), next.clone());
    let bytes = [255, 0, 0, 255].repeat(320 * 240);
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(&bytes).unwrap();
    let pool = fixture.client.shm.as_ref().unwrap().create_pool(file.as_fd(), bytes.len() as i32, &fixture.queue.handle(), ());
    let blue = pool.create_buffer(0, 320, 240, 320 * 4, wl_shm::Format::Argb8888, &fixture.queue.handle(), ());
    pool.destroy();
    window.surface.attach(Some(&blue), 0, 0);
    window.surface.damage_buffer(0, 0, 320, 240);
    window.surface.commit();
    fixture.exchange();
    let expected = fixture.desktop.server.windows()[0].nodes[0].content.version;
    let old = render(&mut fixture, &current);
    old.submitted(&mut fixture.desktop.server, Some(OutputId(1)));
    fixture.exchange();
    assert!(!next.load(Ordering::Acquire), "Old pixels invited the client for an unrepresented commit");
    old.sync.wait().unwrap();
    let frame = accepted(&mut fixture, |frame| frame.windows[0].nodes[0].content.version == expected);
    let pixels = crate::presentation::probe_pixels(frame.frame.dmabuf().unwrap()).unwrap();
    assert!(pixels.chunks_exact(4).filter(|pixel| pixel[2] > 225 && pixel[0] < 30 && pixel[1] < 30).count() > 20_000);
    let current = render(&mut fixture, &frame);
    current.submitted(&mut fixture.desktop.server, Some(OutputId(1)));
    fixture.exchange();
    assert!(next.load(Ordering::Acquire), "Ready pixels did not resume the client");
    current.sync.wait().unwrap();
    fixture.desktop.worker.stop().unwrap();
    eprintln!("Four native frames and offscreen rendering keep callbacks pending; submitted output, callback-only service, buffer lifetime and current GPU pixels pass");
}
