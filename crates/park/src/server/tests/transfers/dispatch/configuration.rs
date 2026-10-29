use super::*;
use super::opening::{activate, click, close, write};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

const DWELL: &str = "File drag hover delay (100–2000 ms)";

fn create(runtime: &tokio::runtime::Runtime, directory: &std::path::Path, wakes: Option<Arc<AtomicUsize>>) -> (Fixture, ClientWindow, wl_buffer::WlBuffer, SurfaceId) {
    let mut harness = Harness::new();
    harness.server.configure_outputs(&[
        Monitor { id: OutputId(1), name: "Configuration A".into(), origin: [0, 0], size: [500, 720] },
        Monitor { id: OutputId(2), name: "Configuration B".into(), origin: [500, 0], size: [500, 720] },
    ]).unwrap();
    harness.sync();
    let window = harness.window();
    let buffer = paint(&mut harness, &window, [320, 240], [40, 180, 60, 255]);
    let id = harness.server.windows()[0].id;
    let configuration = Configuration { showoff: false, size: [1000, 720], directory: directory.into(), runtime: runtime.handle().clone(), cell: None };
    let worker = if let Some(wakes) = wakes {
        CanvasWorker::spawn_with_setup(configuration, move |canvas| {
            let wake = canvas.world_mut().resource::<WakeSignal>().clone();
            canvas.world_mut().insert_resource(WakeSignal::new(move || { wakes.fetch_add(1, Ordering::AcqRel); wake.ring(); }));
        }).unwrap()
    } else { CanvasWorker::spawn(configuration).unwrap() };
    let mut fixture = Fixture::with_worker(harness, [1000, 720], worker);
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry| geometry.items.iter().any(|item| item.id == ItemId::Application(id)))));
    fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        let workspaces = world.get::<native::workspace::Workspaces>(root).unwrap().entries.iter().map(|space| space.id).collect::<Vec<_>>();
        for workspace in workspaces { native::workspace_config::set_physics(world, root, workspace, false); }
    });
    (fixture, window, buffer, id)
}

fn tap(fixture: &mut Fixture, code: u32) {
    fixture.desktop.key(code, true, 10).unwrap();
    fixture.desktop.key(code, false, 11).unwrap();
}

fn open(fixture: &mut Fixture) {
    fixture.desktop.key(125, true, 1).unwrap();
    tap(fixture, 18);
    fixture.desktop.key(125, false, 2).unwrap();
    fixture.inspect(|_| ());
    activate(fixture, "Park");
    activate(fixture, "Edit overview transfers");
}

fn value(fixture: &mut Fixture, field: Entity) -> (String, bool, bool) {
    fixture.inspect(move |canvas| {
        let input = canvas.world_mut().get::<bevy::text::EditableText>(field).unwrap();
        (input.value().to_string(), input.is_composing(), input.pending_paste.is_some())
    })
}

fn source(bytes: &[u8], dwell: u16) -> Vec<u8> {
    let mut text = String::from_utf8(bytes.to_vec()).unwrap();
    let name = text.find("hover_delay_ms").unwrap() + "hover_delay_ms".len();
    let start = name + text[name..].find(|character: char| character.is_ascii_digit()).unwrap();
    assert!(start - name <= 64);
    let end = start + text[start..].find(|character: char| !character.is_ascii_digit()).unwrap();
    text.replace_range(start..end, &dwell.to_string());
    text.into_bytes()
}

fn replace(path: &std::path::Path, bytes: &[u8]) {
    let mut file = tempfile::NamedTempFile::new_in(path.parent().unwrap()).unwrap();
    file.write_all(bytes).unwrap();
    file.as_file().sync_all().unwrap();
    file.persist(path).unwrap();
}

fn accepted(fixture: &mut Fixture, dwell: u16) {
    let prefix = format!("File hover: {dwell} ms ·");
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let prefix = prefix.clone();
        if fixture.inspect(move |canvas| { let world = canvas.world_mut(); world.query::<&Text>().iter(world).any(|text| text.0.starts_with(&prefix)) }) { return; }
        assert!(Instant::now() < deadline, "The real file watcher did not accept its atomic configuration revision");
        std::thread::sleep(Duration::from_millis(3));
    }
}

pub(super) fn application_input(fixture: &mut Fixture, id: SurfaceId, window: &ClientWindow) {
    fixture.desktop.activate(id, false).unwrap();
    fixture.until(|fixture| fixture.desktop.pending_output.is_none() && fixture.client.keyboard_focus.as_ref() == Some(&window.surface));
    let item = fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().items.iter().find(|item| item.id == ItemId::Application(id)).unwrap();
    let origin = item.mapping.surface.origin();
    let size = item.mapping.surface.size();
    let point = item.mapping.canvas_point([origin[0] + size[0] * 0.5, origin[1] + size[1] * 0.5]).unwrap();
    let buttons = fixture.client.button_recipients.len();
    let keys = fixture.client.key_recipients.len();
    fixture.desktop.move_pointer(point, 20).unwrap();
    fixture.tick();
    fixture.desktop.button(272, true, 21).unwrap();
    fixture.tick();
    fixture.desktop.button(272, false, 22).unwrap();
    tap(fixture, 45);
    let started = Instant::now();
    let mut traced = false;
    fixture.until(|fixture| {
        let ready = fixture.client.key_recipients.len() >= keys + 2 && fixture.client.button_recipients.len() >= buttons + 2;
        if !ready && !traced && started.elapsed() > Duration::from_secs(5) {
            traced = true;
            eprintln!("Application setup input: keys={:?}, buttons={:?}, keyboard={:?}, pending_focus={:?}, pending_output={:?}, layer_focus={:?}, held_buttons={:?}, grabbed={}",
                fixture.client.key_recipients.get(keys..), fixture.client.button_recipients.get(buttons..), fixture.client.keyboard_focus,
                fixture.desktop.pending_focus, fixture.desktop.pending_output, fixture.desktop.layer_focus, fixture.desktop.buttons,
                fixture.desktop.grabbed.is_some());
        }
        ready
    });
    assert_eq!(&fixture.client.key_recipients[keys..], &[(Some(window.surface.id()), 45, true), (Some(window.surface.id()), 45, false)]);
    assert_eq!(&fixture.client.button_recipients[buttons..], &[(Some(window.surface.id()), 272, true), (Some(window.surface.id()), 272, false)]);
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn real_client_ime_and_controlled_pending_paste_preserve_a_stale_settings_draft_through_file_review_cancel_save_and_input_recovery() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let (mut fixture, window, _buffer, id) = create(&runtime, directory.path(), None);
    application_input(&mut fixture, id, &window);
    open(&mut fixture);
    write(&mut fixture, DWELL, "650");
    click(&mut fixture, "Save overview transfers");
    write(&mut fixture, DWELL, "800");
    let field = magnification::focus(&mut fixture, DWELL);
    let mut ime = ime::Ime::connect(&mut fixture);
    ime.until(&mut fixture, |_, ime| ime.active());
    ime.method.set_preedit_string("9".into(), 0, 1);
    ime.method.commit(ime.serial());
    let deadline = Instant::now() + Duration::from_secs(30);
    while !value(&mut fixture, field).1 {
        ime.tick(&mut fixture);
        assert!(Instant::now() < deadline);
    }
    let path = directory.path().join("interface.lingua");
    let original = std::fs::read(&path).unwrap();
    replace(&path, &source(&original, 900));
    accepted(&mut fixture, 900);
    assert_eq!(value(&mut fixture, field), ("800".into(), true, false));
    ime.method.set_preedit_string(String::new(), 0, 0);
    ime.method.commit(ime.serial());
    let deadline = Instant::now() + Duration::from_secs(30);
    while value(&mut fixture, field).1 {
        ime.tick(&mut fixture);
        assert!(Instant::now() < deadline);
    }
    fixture.desktop.key(29, true, 30).unwrap();
    tap(&mut fixture, 30);
    fixture.desktop.key(29, false, 31).unwrap();
    fixture.inspect(|_| ());
    let old_serial = ime.serial();
    let (paste, wake) = fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        let paste = Arc::new(std::sync::Mutex::new(None));
        world.get_mut::<bevy::text::EditableText>(field).unwrap().pending_paste = Some(bevy::clipboard::ClipboardRead::Pending(paste.clone()));
        (paste, world.resource::<WakeSignal>().clone())
    });
    let external = source(&original, 1000);
    replace(&path, &external);
    accepted(&mut fixture, 1000);
    let keys = fixture.client.key_recipients.len();
    click(&mut fixture, "Review overview draft against file");
    assert!(!fixture.inspect(|canvas| { let world = canvas.world_mut(); world.query::<&Name>().iter(world).any(|name| name.as_str() == "configuration.review") }));
    assert_eq!(value(&mut fixture, field), ("800".into(), false, true));
    assert!(fixture.inspect(|canvas| { let world = canvas.world_mut(); world.query::<&Text>().iter(world).any(|text| text.0.contains("Finish typing")) }));
    magnification::focus(&mut fixture, DWELL);
    *paste.lock().unwrap() = Some(Ok("700".into()));
    wake.ring();
    let deadline = Instant::now() + Duration::from_secs(30);
    while value(&mut fixture, field) != ("700".into(), false, false) {
        ime.tick(&mut fixture);
        assert!(Instant::now() < deadline, "The controlled clipboard completion did not settle in its original real editor");
    }
    click(&mut fixture, "Review overview draft against file");
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.keyboard_modal));
    ime.method.commit_string("999".into());
    ime.method.commit(old_serial);
    for _ in 0..8 { ime.tick(&mut fixture); }
    assert_eq!(value(&mut fixture, field), ("700".into(), false, false));
    click(&mut fixture, "Cancel review and keep draft");
    assert_eq!(value(&mut fixture, field), ("700".into(), false, false));
    assert_eq!(std::fs::read(&path).unwrap(), external);
    click(&mut fixture, "Review overview draft against file");
    click(&mut fixture, "Use my draft value");
    click(&mut fixture, "Continue with reviewed draft");
    assert_eq!(std::fs::read(&path).unwrap(), external);
    assert_eq!(fixture.client.key_recipients.len(), keys);
    click(&mut fixture, "Save overview transfers");
    accepted(&mut fixture, 700);
    close(&mut fixture);
    ime.destroy(&mut fixture);
    application_input(&mut fixture, id, &window);
    fixture.desktop.worker.stop().unwrap();
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn queued_review_cannot_apply_after_output_or_lock_loss_and_a_real_blocked_file_read_cannot_wake_its_disposed_worker() {
    use std::os::unix::fs::MetadataExt;
    for locked in [false, true] {
        let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
        let entered = runtime.enter();
        let directory = tempfile::tempdir().unwrap();
        let wakes = Arc::new(AtomicUsize::new(0));
        let (mut fixture, window, _buffer, id) = create(&runtime, directory.path(), Some(wakes.clone()));
        application_input(&mut fixture, id, &window);
        open(&mut fixture);
        write(&mut fixture, DWELL, "650");
        click(&mut fixture, "Save overview transfers");
        write(&mut fixture, DWELL, "800");
        let field = magnification::focus(&mut fixture, DWELL);
        let path = directory.path().join("interface.lingua");
        let accepted_source = source(&std::fs::read(&path).unwrap(), 900);
        replace(&path, &accepted_source);
        accepted(&mut fixture, 900);
        click(&mut fixture, "Review overview draft against file");
        click(&mut fixture, "Use my draft value");
        let confirm = magnification::focus(&mut fixture, "Continue with reviewed draft");
        let point = fixture.inspect(move |canvas| canvas.world_mut().get::<UiGlobalTransform>(confirm).unwrap().transform_point2(Vec2::ZERO).as_dvec2().to_array());
        let frame = &fixture.desktop.current.as_ref().unwrap().frame;
        let mapping = frame.input.mappings.get(frame.output.unwrap()).unwrap();
        let point = mapping.raster().logical_point(point);
        let target = frame.input.native_pointer.target(point).unwrap();
        let fifo = directory.path().join("held-config-read");
        rustix::fs::mkfifoat(rustix::fs::CWD, &fifo, rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR).unwrap();
        let metadata = std::fs::metadata(&fifo).unwrap();
        let identity = (metadata.dev(), metadata.ino());
        let mut writer = std::fs::OpenOptions::new().read(true).write(true).open(&fifo).unwrap();
        std::fs::rename(&fifo, &path).unwrap();
        fixture.until(|_| std::fs::read_dir("/proc/self/fd").unwrap().filter_map(|entry| entry.ok())
            .filter_map(|entry| std::fs::metadata(entry.path()).ok()).filter(|metadata| (metadata.dev(), metadata.ino()) == identity).count() >= 2);
        let (_, release) = fixture.pause();
        fixture.desktop.worker.input(Input::CapturedButton(bevy::input::mouse::MouseButton::Left, true, point.map(|value| value as f32), 0, target)).unwrap();
        let mut serial = fixture.desktop.worker.input(Input::Button(bevy::input::mouse::MouseButton::Left, false, point.map(|value| value as f32), 0)).unwrap();
        let mut queued = 2;
        while let Ok(next) = fixture.desktop.worker.input(Input::Pointer([0.0; 2])) { serial = next; queued += 1; assert!(queued <= 128); }
        assert_eq!(queued, 128);
        let locker = if locked {
            let locker = fixture.client.lock_manager.as_ref().unwrap().lock(&fixture.queue.handle(), ());
            fixture.exchange();
            assert!(fixture.desktop.server.locked());
            Some(locker)
        } else {
            fixture.desktop.server.configure_outputs(&[Monitor { id: OutputId(2), name: "Configuration B".into(), origin: [500, 0], size: [500, 720] }]).unwrap();
            fixture.desktop.server.registered_output_catalog().unwrap();
            fixture.desktop.invalidate_outputs().unwrap();
            None
        };
        fixture.desktop.dispatch().unwrap();
        release.send(()).unwrap();
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.input_serial >= serial));
        assert_eq!(value(&mut fixture, field).0, "800");
        let save_disabled = fixture.inspect(|canvas| {
            let world = canvas.world_mut();
            world.query::<(Entity, &bevy::a11y::AccessibilityNode)>().iter(world).find(|(_, node)| node.label() == Some("Save overview transfers"))
                .is_some_and(|(entity, _)| world.get::<bevy::ui::InteractionDisabled>(entity).is_some())
        });
        assert!(save_disabled, "A retired review changed its parent's base revision after output/lock loss");
        if locked {
            let keys = fixture.client.key_recipients.len();
            let buttons = fixture.client.button_recipients.len();
            tap(&mut fixture, 45);
            fixture.desktop.button(272, true, 50).unwrap();
            fixture.desktop.button(272, false, 51).unwrap();
            for _ in 0..8 { fixture.tick(); }
            assert_eq!(fixture.client.key_recipients.len(), keys);
            assert_eq!(fixture.client.button_recipients.len(), buttons);
        } else {
            close(&mut fixture);
            application_input(&mut fixture, id, &window);
        }
        fixture.desktop.worker.stop().unwrap();
        drop(fixture);
        drop(locker);
        let retired_wakes = wakes.load(Ordering::Acquire);
        writer.write_all(&source(&accepted_source, 1000)).unwrap();
        drop(writer);
        drop(entered);
        drop(runtime);
        assert_eq!(wakes.load(Ordering::Acquire), retired_wakes);
        let until = Instant::now() + Duration::from_millis(600);
        while Instant::now() < until {
            assert_eq!(wakes.load(Ordering::Acquire), retired_wakes, "The retired real file reader/watcher woke its disposed worker");
            std::thread::sleep(Duration::from_millis(3));
        }
    }
}
