use super::super::*;
use bevy::prelude::*;
use lince_interface::native::{self, actions::Action};
use std::{io::Write, time::{Duration, Instant}};

fn frame(worker: &CanvasWorker, serial: u64) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        assert!(worker.failure().is_none());
        if worker.frame().is_some_and(|frame| frame.input_serial >= serial) { return; }
        assert!(Instant::now() < deadline, "The configuration setup frame was not published");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn button(world: &mut World, caption: &str) -> Entity {
    world.query::<(Entity, &Children, &native::actions::ActionButton)>().iter(world)
        .find(|(_, children, _)| children.iter().any(|child| world.get::<Text>(child).is_some_and(|text| text.0 == caption))).unwrap().0
}

fn quiet(worker: &CanvasWorker) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut since = None;
    let mut count = worker.publications.load(Ordering::Acquire);
    loop {
        assert!(worker.failure().is_none());
        let _ = worker.frame();
        let now = worker.publications.load(Ordering::Acquire);
        if now != count || !worker.idle.load(Ordering::Acquire) { count = now; since = None; }
        else if since.get_or_insert_with(Instant::now).elapsed() >= Duration::from_millis(500) { return; }
        assert!(Instant::now() < deadline, "The configuration worker did not return to quiet idle");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn an_atomic_file_edit_wakes_a_quiet_worker_and_publishes_the_new_policy_without_injected_input() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut worker = CanvasWorker::spawn(Configuration { showoff: false, size: [1000, 720], directory: directory.path().into(), runtime: runtime.handle().clone(), cell: None }).unwrap();
    frame(&worker, 0);
    let serial = worker.input(Input::Test(Box::new(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        let workspaces: Vec<_> = world.get::<native::workspace::Workspaces>(root).unwrap().entries.iter().map(|workspace| workspace.id).collect();
        for workspace in workspaces { native::workspace_config::set_physics(world, root, workspace, false); }
        native::edit_mode::EditAction::Open.apply(world, root);
        native::edit_mode::EditAction::Park.apply(world, root);
    }))).unwrap();
    frame(&worker, serial);
    let serial = worker.input(Input::Test(Box::new(|canvas| {
        let world = canvas.world_mut();
        let toggle = button(world, "Edit overview transfers");
        world.trigger(bevy::ui_widgets::Activate { entity: toggle });
    }))).unwrap();
    frame(&worker, serial);
    let serial = worker.input(Input::Test(Box::new(|canvas| {
        let world = canvas.world_mut();
        let input = world.query::<(Entity, &bevy::a11y::AccessibilityNode)>().iter(world)
            .find(|(_, node)| node.label() == Some("File drag hover delay (100–2000 ms)")).unwrap().0;
        world.get_mut::<bevy::text::EditableText>(input).unwrap().editor.set_text("650");
    }))).unwrap();
    frame(&worker, serial);
    let serial = worker.input(Input::Test(Box::new(|canvas| {
        let world = canvas.world_mut();
        let save = button(world, "Save overview transfers");
        assert!(world.get::<bevy::ui::InteractionDisabled>(save).is_none());
        world.trigger(bevy::ui_widgets::Activate { entity: save });
    }))).unwrap();
    frame(&worker, serial);
    let path = directory.path().join("interface.lingua");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.is_file() {
        assert!(worker.failure().is_none());
        assert!(Instant::now() < deadline, "The valid setup draft did not create the private configuration");
        std::thread::sleep(Duration::from_millis(2));
    }
    let serial = worker.input(Input::Test(Box::new(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        native::edit_mode::EditAction::Close.apply(world, root);
        world.resource_mut::<bevy::input_focus::InputFocus>().clear();
    }))).unwrap();
    frame(&worker, serial);
    quiet(&worker);
    let before = worker.publications.load(Ordering::Acquire);
    let mut source = std::fs::read_to_string(&path).unwrap();
    let key = source.find("pointer_follow").unwrap() + "pointer_follow".len();
    let offset = source[key..].find("false").unwrap();
    assert!(offset < 32, "Expected the generated boolean field immediately after its name");
    source.replace_range(key + offset..key + offset + 5, "true");
    let mut replacement = tempfile::NamedTempFile::new_in(directory.path()).unwrap();
    replacement.write_all(source.as_bytes()).unwrap();
    replacement.as_file().sync_all().unwrap();
    let changed = Instant::now();
    replacement.persist(&path).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        assert!(worker.failure().is_none());
        if worker.frame().is_some_and(|frame| frame.frame.input.focus_policy.pointer_follow) { break; }
        assert!(Instant::now() < deadline, "A real filesystem notification did not publish the new policy without input");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(changed.elapsed() <= Duration::from_millis(1150), "The file edit exceeded the debounce plus publication target");
    quiet(&worker);
    let after = worker.publications.load(Ordering::Acquire);
    assert!(after > before);
    assert!(after - before <= 12, "The file edit caused recurring frame publication");
    let deadline = Instant::now() + Duration::from_millis(600);
    while Instant::now() < deadline {
        assert!(worker.failure().is_none());
        assert_eq!(worker.publications.load(Ordering::Acquire), after);
        std::thread::sleep(Duration::from_millis(2));
    }
    worker.stop().unwrap();
}
