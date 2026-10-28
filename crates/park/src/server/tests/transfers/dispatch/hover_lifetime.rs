use super::*;

struct Drag {
    window: ClientWindow,
    _buffer: wl_buffer::WlBuffer,
    source: wl_data_source::WlDataSource,
    device: wl_data_device::WlDataDevice,
    id: SurfaceId,
    original: (Entity, u64, PlacementId),
    owner: u64,
    destination: u64,
}

fn prepare(runtime: &tokio::runtime::Runtime, directory: &std::path::Path, two_outputs: bool) -> (Fixture, Drag) {
    let mut harness = Harness::new();
    let mut monitors = vec![Monitor { id: OutputId(1), name: "Hover-A".into(), origin: [0, 0], size: [if two_outputs { 500 } else { 1000 }, 720] }];
    if two_outputs { monitors.push(Monitor { id: OutputId(2), name: "Hover-B".into(), origin: [500, 0], size: [500, 720] }); }
    harness.server.configure_outputs(&monitors).unwrap();
    harness.sync();
    let window = harness.window();
    let buffer = paint(&mut harness, &window, [320, 240], [0, 255, 0, 255]);
    let id = harness.server.windows()[0].id;
    let mut fixture = Fixture::new(harness, runtime, directory);
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry| geometry.items.iter().any(|item| item.id == ItemId::Application(id)))));
    let (owner, destination, original) = fixture.inspect(move |canvas| {
        let original = member(canvas, id);
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        canvas.create_workspace(root.to_bits()).unwrap();
        let spaces = canvas.world_mut().get::<native::workspace::Workspaces>(root).unwrap();
        let destination = spaces.active;
        let workspaces = spaces.entries.iter().map(|workspace| workspace.id).collect::<Vec<_>>();
        for workspace in workspaces { native::workspace_config::set_physics(canvas.world_mut(), root, workspace, false); }
        canvas.activate_workspace(root.to_bits(), original.1).unwrap();
        (root.to_bits(), destination, original)
    });
    fixture.desktop.send(Input::Action(DesktopAction::ToggleOverview)).unwrap();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.keyboard_modal));
    let point = fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        let preview = world.query::<(Entity, &Name)>().iter(world).find(|(_, name)| name.as_str() == format!("overview.workspace.{destination}")).unwrap().0;
        super::super::overview::center(canvas, preview).as_dvec2().to_array()
    });
    let surface = fixture.desktop.server.windows()[0].surface.clone();
    fixture.desktop.server.focus_surface(Some(surface.clone()), smithay::utils::SERIAL_COUNTER.next_serial());
    fixture.desktop.server.pointer_motion(Some((surface, [20.0, 20.0])), 1).unwrap();
    fixture.desktop.server.pointer_button(272, true, 2);
    fixture.exchange();
    let qh = fixture.queue.handle();
    let manager = fixture.client.data_manager.as_ref().unwrap();
    let device = manager.get_data_device(fixture.client.seat.as_ref().unwrap(), &qh, ());
    let source = manager.create_data_source(&qh, ());
    source.offer("text/uri-list".into());
    source.set_actions(wl_data_device_manager::DndAction::Copy);
    fixture.client.drop_payload = format!("file://{}\r\n", directory.join("not-dropped.txt").display()).into_bytes();
    device.start_drag(Some(&source), &window.surface, None, fixture.client.pointer_serial);
    for _ in 0..4 { fixture.exchange(); }
    fixture.desktop.move_pointer(point, 3).unwrap();
    fixture.until(|fixture| fixture.desktop.file_hover_observation().0.is_some());
    (fixture, Drag { window, _buffer: buffer, source, device, id, original, owner, destination })
}

fn active(fixture: &mut Fixture, owner: u64) -> u64 {
    fixture.inspect(move |canvas| canvas.world_mut().get::<native::workspace::Workspaces>(Entity::from_bits(owner)).unwrap().active)
}

fn text(fixture: &mut Fixture, field: Entity) -> (String, bool) {
    fixture.inspect(move |canvas| {
        let text = canvas.world_mut().get::<bevy::text::EditableText>(field).unwrap();
        (text.value().to_string(), text.is_composing())
    })
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn escape_cancels_a_real_pointer_drag_without_a_drop_and_native_ime_and_application_input_recover() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let (mut fixture, drag) = prepare(&runtime, directory.path(), false);
    let mut ime = ime::Ime::connect(&mut fixture);
    let old_serial = ime.serial();
    let buttons = fixture.client.pointer_buttons.len();
    fixture.desktop.key(1, true, 10).unwrap();
    fixture.desktop.key(1, false, 11).unwrap();
    ime.until(&mut fixture, |fixture, _| !fixture.desktop.server.dragging()
        && fixture.desktop.file_hover_observation() == (None, None)
        && fixture.desktop.current.as_ref().is_some_and(|frame| !frame.frame.input.keyboard_modal)
        && fixture.client.drop_cancelled == 1);
    fixture.desktop.button(272, false, 12).unwrap();
    ime.tick(&mut fixture);
    assert_eq!(fixture.client.pointer_buttons.len(), buttons);
    assert_eq!(fixture.client.drop_performed, 0);
    assert_eq!(fixture.client.drop_finished, 0);
    assert_eq!(fixture.inspect(move |canvas| member(canvas, drag.id)), drag.original);
    assert!(!fixture.inspect(|canvas| canvas.world_mut().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::Escape)));
    let settled_workspace = active(&mut fixture, drag.owner);
    let deadline = Instant::now() + Duration::from_millis(650);
    while Instant::now() < deadline { ime.tick(&mut fixture); std::thread::sleep(Duration::from_millis(2)); }
    assert_eq!(active(&mut fixture, drag.owner), settled_workspace);
    assert_eq!(fixture.desktop.file_hover_observation(), (None, None));
    assert_eq!(fixture.client.drop_cancelled, 1);
    fixture.desktop.key(125, true, 20).unwrap();
    fixture.desktop.key(25, true, 21).unwrap();
    fixture.desktop.key(25, false, 22).unwrap();
    fixture.desktop.key(125, false, 23).unwrap();
    ime.until(&mut fixture, |fixture, _| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.modal && frame.frame.input.editor.is_some()));
    let field = magnification::focus(&mut fixture, "Operation: enter @slug or /command");
    ime.until(&mut fixture, |fixture, ime| ime.active() && ime.serial() != old_serial
        && fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.editor.is_some()));
    assert_eq!(text(&mut fixture, field), (String::new(), false));
    let client_keys = fixture.client.keys.len();
    ime.method.commit_string("stale".into());
    ime.method.commit(old_serial);
    for _ in 0..8 { ime.tick(&mut fixture); }
    assert_eq!(text(&mut fixture, field), (String::new(), false));
    ime.method.set_preedit_string("界".into(), 0, 3);
    ime.method.commit(ime.serial());
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        ime.tick(&mut fixture);
        let value = text(&mut fixture, field);
        if value.1 { assert!(value.0.is_empty()); break; }
        assert!(Instant::now() < deadline, "A fresh provider could not compose after drag cancellation");
        std::thread::sleep(Duration::from_millis(2));
    }
    ime.tick(&mut fixture);
    ime.method.commit_string("é".into());
    ime.method.set_preedit_string(String::new(), 0, 0);
    ime.method.commit(ime.serial());
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        ime.tick(&mut fixture);
        if text(&mut fixture, field) == ("é".into(), false) { break; }
        assert!(Instant::now() < deadline, "A fresh provider could not commit after drag cancellation");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(fixture.client.keys.len(), client_keys);
    fixture.desktop.key(1, true, 30).unwrap();
    fixture.desktop.key(1, false, 31).unwrap();
    ime.until(&mut fixture, |fixture, ime| !ime.active()
        && fixture.desktop.current.as_ref().is_some_and(|frame| !frame.frame.input.modal && !frame.frame.input.keyboard_modal && frame.frame.input.editor.is_none()));
    fixture.desktop.send(Input::Action(DesktopAction::SelectPlacement { workspace: drag.original.1, item: Some(drag.original.2.0) })).unwrap();
    fixture.inspect(|_| ());
    fixture.until(|fixture| fixture.desktop.pending_output.is_none() && fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref()
        .is_some_and(|geometry| geometry.items.iter().any(|item| item.id == ItemId::Application(drag.id)))));
    let point = {
        let frame = &fixture.desktop.current.as_ref().unwrap().frame;
        let item = frame.geometry.as_ref().unwrap().items.iter().find(|item| item.id == ItemId::Application(drag.id)).unwrap();
        let origin = item.mapping.surface.origin();
        let size = item.mapping.surface.size();
        item.mapping.canvas_point([origin[0] + size[0] * 0.5, origin[1] + size[1] * 0.5]).unwrap()
    };
    fixture.desktop.move_pointer(point, 40).unwrap();
    fixture.tick();
    assert_eq!(fixture.desktop.server.capture_pointer_owner(), crate::server::CaptureContentOwner::Window(drag.id));
    let keys = fixture.client.keys.len();
    let recipients = fixture.client.key_recipients.len();
    let buttons = fixture.client.pointer_buttons.len();
    fixture.desktop.button(272, true, 41).unwrap();
    fixture.desktop.button(272, false, 42).unwrap();
    fixture.desktop.key(45, true, 43).unwrap();
    fixture.desktop.key(45, false, 44).unwrap();
    fixture.until(|fixture| fixture.client.keys.len() >= keys + 2 && fixture.client.pointer_buttons.len() >= buttons + 2);
    assert_eq!(&fixture.client.keys[keys..], &[(45, true), (45, false)]);
    assert_eq!(&fixture.client.key_recipients[recipients..], &[(Some(drag.window.surface.id()), 45, true), (Some(drag.window.surface.id()), 45, false)]);
    assert_eq!(&fixture.client.pointer_buttons[buttons..], &[(272, true), (272, false)]);
    assert_eq!(fixture.client.drop_cancelled, 1);
    assert_eq!(fixture.client.drop_performed, 0);
    assert_eq!(fixture.client.drop_finished, 0);
    assert_eq!(fixture.inspect(move |canvas| member(canvas, drag.id)), drag.original);
    assert!(!fixture.inspect(|canvas| {
        let keys = canvas.world_mut().resource::<ButtonInput<KeyCode>>();
        keys.pressed(KeyCode::Escape) || keys.pressed(KeyCode::SuperLeft)
    }));
    ime.destroy(&mut fixture);
    drag.source.destroy();
    drag.device.release();
    fixture.exchange();
    assert_eq!(fixture.client.drop_cancelled, 1);
    assert!(fixture.desktop.running);
    fixture.desktop.worker.stop().unwrap();
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn a_real_drag_crosses_outputs_without_reusing_the_previous_hover_or_moving_its_source() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let (mut fixture, drag) = prepare(&runtime, directory.path(), true);
    let previous = fixture.desktop.file_hover_observation().0.unwrap();
    assert_eq!(previous.output, Some(OutputId(1)));
    fixture.desktop.move_pointer([750.0, 620.0], 4).unwrap();
    fixture.until(|fixture| fixture.desktop.pending_output.is_none() && fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.output == Some(OutputId(2)))
        && fixture.desktop.file_hover_observation().0.is_some_and(|context| context.output == Some(OutputId(2))));
    let replacement = fixture.desktop.file_hover_observation().0.unwrap();
    assert_eq!(replacement.owner, previous.owner);
    assert_eq!(replacement.token, previous.token);
    let deadline = Instant::now() + Duration::from_millis(650);
    while Instant::now() < deadline { fixture.tick(); std::thread::sleep(Duration::from_millis(2)); }
    assert_ne!(active(&mut fixture, drag.owner), drag.destination);
    assert_eq!(fixture.inspect(move |canvas| member(canvas, drag.id)), drag.original);
    assert_eq!(fixture.client.drop_performed, 0);
    assert_eq!(fixture.client.drop_finished, 0);
    drag.source.destroy();
    drag.device.release();
    fixture.exchange();
    fixture.desktop.button(272, false, 5).unwrap();
    fixture.until(|fixture| fixture.desktop.file_hover_observation() == (None, None));
    assert!(fixture.desktop.running);
}

#[derive(Clone, Copy, Debug)]
enum Loss { Modal, Source, Output, Lock }

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn a_hover_reply_retained_by_a_full_input_queue_cannot_outlive_its_modal_source_output_or_lock() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for loss in [Loss::Modal, Loss::Source, Loss::Output, Loss::Lock] {
        let path = directory.path().join(format!("{loss:?}"));
        std::fs::create_dir_all(&path).unwrap();
        let (mut fixture, drag) = prepare(&runtime, &path, matches!(loss, Loss::Output));
        std::thread::sleep(Duration::from_millis(600));
        let (_, release) = fixture.pause();
        let mut queued = 0;
        let mut last_serial = 0;
        let point = fixture.desktop.pointer.map(|value| value as f32);
        if matches!(loss, Loss::Modal) {
            last_serial = fixture.desktop.worker.input(Input::Action(DesktopAction::ToggleOverview)).unwrap();
            queued += 1;
        }
        loop {
            match fixture.desktop.worker.input(Input::Pointer(point)) {
                Ok(serial) => { queued += 1; last_serial = serial; }
                Err(std::sync::mpsc::TrySendError::Full(_)) => break,
                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => panic!("The hover worker disconnected"),
            }
        }
        assert_eq!(queued, 128);
        fixture.desktop.dispatch().unwrap();
        let request = fixture.desktop.file_hover_observation().1.expect("The actual deadline request must retain its reply under pressure");
        assert_eq!(request.workspace, drag.destination);
        let lock = match loss {
            Loss::Source => { drag.window.role.destroy(); drag.window.xdg.destroy(); drag.window.surface.destroy(); None }
            Loss::Output => {
                fixture.desktop.server.configure_outputs(&[Monitor { id: OutputId(2), name: "Hover-B".into(), origin: [500, 0], size: [500, 720] }]).unwrap();
                None
            }
            Loss::Lock => Some(fixture.client.lock_manager.as_ref().unwrap().lock(&fixture.queue.handle(), ())),
            Loss::Modal => None,
        };
        fixture.exchange();
        release.send(()).unwrap();
        fixture.until(|fixture| fixture.desktop.file_hover_observation().1.is_none() && fixture.desktop.current.as_ref().is_some_and(|frame| frame.input_serial >= last_serial));
        let deadline = Instant::now() + Duration::from_millis(650);
        while Instant::now() < deadline { fixture.tick(); std::thread::sleep(Duration::from_millis(2)); }
        assert_ne!(active(&mut fixture, drag.owner), drag.destination, "A retired hover activated its destination after {loss:?}");
        assert_eq!(fixture.client.drop_performed, 0);
        assert_eq!(fixture.client.drop_finished, 0);
        if !matches!(loss, Loss::Source) { assert_eq!(fixture.inspect(move |canvas| member(canvas, drag.id)), drag.original); }
        if matches!(loss, Loss::Lock) { assert!(fixture.desktop.server.locked()); }
        drag.source.destroy();
        drag.device.release();
        fixture.exchange();
        assert!(fixture.desktop.running);
        drop(lock);
    }
}
