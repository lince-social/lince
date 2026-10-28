use super::*;
use crate::launch::Capability;
use lince_interface::host::control::{Arguments, Disposition, Error, Invocation, Response};

mod ownership;

fn invoke(fixture: &mut Fixture, id: u32, command: &str) -> Invocation {
    let invocation = Invocation { command: command.into(), context: fixture.desktop.current.as_ref().unwrap().frame.input.control.context.clone().unwrap(), arguments: Arguments::None };
    fixture.client.control.manager.as_ref().unwrap().invoke(id, serde_json::to_string(&invocation).unwrap());
    invocation
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn typed_commands_reach_the_real_worker_and_report_actual_launch_failure_without_stopping_park() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut harness = Harness::connect_authorized(Server::new([1000, 720]).unwrap(), Some(vec![Capability::WindowList, Capability::DesktopControl, Capability::WindowControl, Capability::Launch]));
    let window = harness.window();
    let _buffer = paint(&mut harness, &window, [320, 240], [40, 150, 230, 255]);
    let id = harness.server.windows()[0].id;
    let mut fixture = Fixture::new(harness, &runtime, directory.path());
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.control.context.is_some() && frame.frame.geometry.as_ref().is_some_and(|geometry| geometry.items.iter().any(|item| item.id == ItemId::Application(id)))));
    let status = fixture.inspect(|canvas| canvas.world_mut().resource::<lince_interface::host::control::Status>().clone());
    assert_eq!(status.controllers, 1);
    assert_eq!(status.readers, 1);
    assert!(status.active);
    let initial = fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        world.get::<native::workspace::Workspaces>(root).unwrap().entries[0].desktop.mode()
    });
    invoke(&mut fixture, 1, "layout.toggle-tiling");
    fixture.until(|fixture| fixture.client.control.replies.contains_key(&1));
    assert!(matches!(fixture.client.control.replies[&1], Response::Complete { outcome: Disposition::Applied }));
    let after = fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        world.get::<native::workspace::Workspaces>(root).unwrap().entries[0].desktop.mode()
    });
    assert_ne!(initial, after);
    let commands = directory.path().join("park.json");
    std::fs::write(&commands, serde_json::to_vec(&serde_json::json!({"launcher":{"program":directory.path().join("missing-automation-fixture-helper")}})).unwrap()).unwrap();
    fixture.desktop.commands(commands, directory.path().join("wayland-control-fixture"));
    fixture.tick();
    invoke(&mut fixture, 2, "launch.launcher");
    fixture.until(|fixture| fixture.client.control.replies.contains_key(&2));
    let Response::Failed { failure } = &fixture.client.control.replies[&2] else { panic!("A missing executable cannot be a dispatched success"); };
    assert_eq!(failure.code, Error::Unavailable);
    assert!(!failure.message.is_empty());
    assert!(fixture.desktop.running);
    assert_eq!(fixture.desktop.server.windows().iter().filter(|window| window.mapped).count(), 1);
    invoke(&mut fixture, 3, "missing.command");
    fixture.until(|fixture| fixture.client.control.replies.contains_key(&3));
    let Response::Failed { failure } = &fixture.client.control.replies[&3] else { panic!("An unknown command cannot execute"); };
    assert_eq!(failure.code, Error::UnknownCommand);
    fixture.desktop.send(Input::Activate(id)).unwrap();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| matches!(frame.frame.input.control.context.as_ref().and_then(|context| context.item), Some(lince_interface::host::control::Item::Application { surface, .. }) if surface == id.0)));
    invoke(&mut fixture, 4, "item.close");
    fixture.until(|fixture| fixture.client.closed && fixture.client.control.replies.contains_key(&4));
    assert!(matches!(fixture.client.control.replies[&4], Response::Complete { outcome: Disposition::Dispatched }));
    window.role.destroy();
    window.xdg.destroy();
    window.surface.destroy();
    fixture.until(|fixture| fixture.desktop.server.windows().iter().all(|window| !window.mapped));
    assert!(fixture.desktop.running);
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn changing_focus_ahead_of_a_queued_protocol_move_rejects_it_instead_of_moving_the_new_target() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut harness = Harness::connect_authorized(Server::new([1000, 720]).unwrap(), Some(vec![Capability::WindowList, Capability::DesktopControl, Capability::WindowControl]));
    let first = harness.window();
    let _first_buffer = paint(&mut harness, &first, [320, 240], [200, 70, 20, 255]);
    let first_id = harness.server.windows()[0].id;
    let second = harness.window();
    let _second_buffer = paint(&mut harness, &second, [320, 240], [20, 70, 200, 255]);
    let second_id = harness.server.windows().into_iter().find(|window| window.id != first_id).unwrap().id;
    let mut fixture = Fixture::new(harness, &runtime, directory.path());
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry| [first_id, second_id].iter().all(|id| geometry.items.iter().any(|item| item.id == ItemId::Application(*id))))));
    fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        let workspace = world.get::<native::workspace::Workspaces>(root).unwrap().active;
        native::workspace_config::set_physics(world, root, workspace, false);
    });
    fixture.desktop.send(Input::Activate(first_id)).unwrap();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| matches!(frame.frame.input.control.context.as_ref().and_then(|context| context.item), Some(lince_interface::host::control::Item::Application { surface, .. }) if surface == first_id.0)));
    let before = fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        world.get::<native::workspace::Workspaces>(root).unwrap().entries[0].desktop.items().iter().map(|item| (item.id, item.free)).collect::<Vec<_>>()
    });
    let (_, release) = fixture.pause();
    fixture.desktop.worker.input(Input::Activate(second_id)).unwrap();
    invoke(&mut fixture, 1, "item.move-right");
    fixture.exchange();
    fixture.desktop.dispatch().unwrap();
    release.send(()).unwrap();
    fixture.until(|fixture| fixture.client.control.replies.contains_key(&1));
    let Response::Failed { failure } = &fixture.client.control.replies[&1] else { panic!("The stale captured source cannot move a new focus target"); };
    assert_eq!(failure.code, Error::StaleTarget);
    let after = fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        world.get::<native::workspace::Workspaces>(root).unwrap().entries[0].desktop.items().iter().map(|item| (item.id, item.free)).collect::<Vec<_>>()
    });
    assert_eq!(before, after);
    assert!(fixture.desktop.running);
}
