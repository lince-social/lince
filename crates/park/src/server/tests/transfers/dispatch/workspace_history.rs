use super::*;

const OFF: &str = "Back-and-forth workspace switching: Off";
const ON: &str = "Back-and-forth workspace switching: On";

fn create(runtime: &tokio::runtime::Runtime, directory: &std::path::Path, size: [u32; 2], scale: f64, mut harness: Harness) -> Fixture {
    harness.server.configure_scaled_outputs(&[Monitor { id: OutputId(1), name: "History fixture".into(), origin: [0, 0], size: size.map(|value| value as i32) }],
        &std::collections::BTreeMap::from([(OutputId(1), scale)])).unwrap();
    harness.sync();
    let mut fixture = Fixture::with_size(harness, runtime, directory, size);
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.control.context.is_some()));
    fixture
}

fn tap(fixture: &mut Fixture, code: u32) {
    fixture.desktop.key(code, true, 20).unwrap();
    fixture.desktop.key(code, false, 21).unwrap();
}

fn chord(fixture: &mut Fixture, code: u32) {
    fixture.desktop.key(125, true, 19).unwrap();
    tap(fixture, code);
    fixture.desktop.key(125, false, 22).unwrap();
}

fn activate(fixture: &mut Fixture, caption: &'static str) {
    opening::activate(fixture, caption);
}

fn open(fixture: &mut Fixture) {
    chord(fixture, 18);
    fixture.inspect(|_| ());
    activate(fixture, "Park");
    activate(fixture, "Edit window layout");
}

fn close(fixture: &mut Fixture) {
    activate(fixture, lince_interface::i18n::Token::CloseEditMode.text());
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| !frame.frame.input.modal && !frame.frame.input.keyboard_modal));
}

fn active(fixture: &Fixture) -> u64 {
    fixture.desktop.current.as_ref().unwrap().frame.input.workspaces.entries.iter().find(|workspace| workspace.active).unwrap().id
}

fn route(fixture: &mut Fixture, expected: &[u64]) {
    let mut observed = vec![active(fixture)];
    let mut stable = None;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        fixture.tick();
        let workspace = active(fixture);
        if observed.last() != Some(&workspace) { observed.push(workspace); }
        assert!(expected.starts_with(&observed), "Workspace input published an extra or incorrect transition: {observed:?}, expected {expected:?}");
        if observed.as_slice() == expected && fixture.desktop.pending_output.is_none() {
            if stable.get_or_insert_with(Instant::now).elapsed() >= Duration::from_millis(250) { break; }
        } else { stable = None; }
        assert!(Instant::now() < deadline, "Workspace input lost a distinct action: {observed:?}, expected {expected:?}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn rendered_workspace_history_save_cancel_cold_start_and_real_held_batched_provider_input_preserve_distinct_actions() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for (size, scale) in [([360, 720], 1.5)] {
        let profile = directory.path().join(size[0].to_string());
        std::fs::create_dir_all(&profile).unwrap();
        let mut harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(vec![crate::launch::Capability::WindowControl]));
        let window = harness.window();
        let _buffer = paint(&mut harness, &window, [320, 240], [80, 170, 60, 255]);
        let id = harness.server.windows()[0].id;
        let mut fixture = create(&runtime, &profile, size, scale, harness);
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref()
            .is_some_and(|geometry| geometry.items.iter().any(|item| item.id == ItemId::Application(id)))));
        let spaces = fixture.inspect(|canvas| {
            let world = canvas.world_mut();
            let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
            let first = world.get::<native::workspace::Workspaces>(root).unwrap().active;
            assert!(native::workspace::rename(world, root, "History A"));
            canvas.create_workspace(root.to_bits()).unwrap();
            let second = canvas.world_mut().get::<native::workspace::Workspaces>(root).unwrap().active;
            assert!(native::workspace::rename(canvas.world_mut(), root, "History B"));
            let ids = canvas.world_mut().get::<native::workspace::Workspaces>(root).unwrap().entries.iter().map(|workspace| workspace.id).collect::<Vec<_>>();
            for workspace in ids { native::workspace_config::set_physics(canvas.world_mut(), root, workspace, false); }
            [first, second]
        });
        chord(&mut fixture, 2);
        route(&mut fixture, &[spaces[1], spaces[0]]);
        open(&mut fixture);
        activate(&mut fixture, OFF);
        magnification::focus(&mut fixture, ON);
        activate(&mut fixture, "Cancel layout draft");
        close(&mut fixture);
        chord(&mut fixture, 3);
        route(&mut fixture, &[spaces[0], spaces[1]]);
        chord(&mut fixture, 3);
        route(&mut fixture, &[spaces[1]]);
        open(&mut fixture);
        activate(&mut fixture, OFF);
        activate(&mut fixture, "Save window layout");
        close(&mut fixture);
        let configuration = profile.join("interface.lingua");
        let saved = std::fs::read(&configuration).unwrap();
        fixture.desktop.key(125, true, 30).unwrap();
        fixture.desktop.key(2, true, 31).unwrap();
        for time in 32..52 { fixture.desktop.key(2, true, time).unwrap(); }
        fixture.desktop.key(2, false, 52).unwrap();
        fixture.desktop.key(125, false, 53).unwrap();
        route(&mut fixture, &[spaces[1], spaces[0]]);
        chord(&mut fixture, 2);
        route(&mut fixture, &[spaces[0], spaces[1]]);
        fixture.desktop.send(Input::Action(DesktopAction::Unfocus)).unwrap();
        fixture.inspect(|_| ());
        chord(&mut fixture, 3);
        route(&mut fixture, &[spaces[1], spaces[0]]);
        chord(&mut fixture, 2);
        route(&mut fixture, &[spaces[0], spaces[1]]);
        chord(&mut fixture, 3);
        chord(&mut fixture, 3);
        route(&mut fixture, &[spaces[1], spaces[0], spaces[1]]);
        fixture.until(|fixture| fixture.client.workspace_name_events.iter().any(|(_, name)| name == "History B"));
        let workspace = fixture.client.workspace_name_events.iter().rev()
            .find(|(_, name)| name == "History B").unwrap().0.clone();
        let manager = fixture.client.workspace_manager.as_ref().unwrap().clone();
        for _ in 0..3 {
            workspace.activate();
            manager.commit();
            fixture.exchange();
            route(&mut fixture, &[spaces[1]]);
        }
        assert!(fixture.client.keys.iter().all(|(key, _)| ![2, 3].contains(key)), "A consumed workspace number leaked into the application");
        assert!(!fixture.inspect(|canvas| {
            let keys = canvas.world_mut().resource::<ButtonInput<KeyCode>>();
            keys.pressed(KeyCode::SuperLeft) || keys.pressed(KeyCode::Digit1) || keys.pressed(KeyCode::Digit2)
        }));
        assert_eq!(std::fs::read(&configuration).unwrap(), saved);
        fixture.desktop.worker.stop().unwrap();
        drop(fixture);
        let harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(vec![crate::launch::Capability::WindowControl]));
        let mut reopened = create(&runtime, &profile, size, scale, harness);
        reopened.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.workspaces.entries.iter().any(|workspace| workspace.id == spaces[1])));
        let current = active(&reopened);
        let position = reopened.desktop.current.as_ref().unwrap().frame.input.workspaces.entries.iter().position(|workspace| workspace.id == current).unwrap();
        chord(&mut reopened, 2 + position as u32);
        route(&mut reopened, &[current]);
        open(&mut reopened);
        activate(&mut reopened, ON);
        magnification::focus(&mut reopened, OFF);
        activate(&mut reopened, "Cancel layout draft");
        close(&mut reopened);
        assert_eq!(std::fs::read(&configuration).unwrap(), saved);
        chord(&mut reopened, 2 + position as u32);
        route(&mut reopened, &[current]);
        open(&mut reopened);
        opening::available(&mut reopened, ON);
        activate(&mut reopened, "Cancel layout draft");
        close(&mut reopened);
        assert!(reopened.desktop.server.windows().is_empty());
        reopened.desktop.worker.stop().unwrap();
    }
}
