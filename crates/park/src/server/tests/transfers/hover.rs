use super::overview::{capture, center, current};
use super::*;
use crate::presentation::Presentation;
use lince_interface::host::{FileDragContext, HostRequest, WakeSignal};

pub(super) fn control(canvas: &mut Presentation, caption: &str) -> Option<Entity> {
    let world = canvas.world_mut();
    world
        .query::<(Entity, &Children, &native::actions::ActionButton)>()
        .iter(world)
        .find(|(entity, children, _)| {
            world
                .get::<ComputedNode>(*entity)
                .is_some_and(|node| node.size().min_element() > 0.0)
                && children.iter().any(|child| {
                    world
                        .get::<Text>(child)
                        .is_some_and(|text| text.0 == caption)
                })
        })
        .map(|(entity, _, _)| entity)
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn a_real_uri_drag_opens_the_hovered_workspace_and_delivers_one_native_file_drop() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("hover.txt");
    const BODY: &str = "A real URI drop in the selected workspace.";
    std::fs::write(&file, BODY).unwrap();
    let uri = format!("file://{}", file.display());
    let mut canvas = Presentation::new(
        [1000, 720],
        directory.path().join("assets"),
        WakeSignal::new(|| {}),
    )
    .unwrap();
    settle(&mut canvas);
    let root = {
        let world = canvas.world_mut();
        world
            .query_filtered::<Entity, With<native::workspace::Workspaces>>()
            .single(world)
            .unwrap()
    };
    let (sender, receiver) = std::sync::mpsc::sync_channel(8);
    canvas
        .world_mut()
        .insert_resource(native::layout::desktop::HostRequests(sender));
    let mut harness = Harness::new();
    harness
        .server
        .configure_outputs(&[Monitor {
            id: OutputId(1),
            name: "URI hover output".into(),
            origin: [0, 0],
            size: [1000, 720],
        }])
        .unwrap();
    harness.sync();
    canvas
        .outputs(
            harness
                .server
                .output_viewports()
                .into_iter()
                .map(|(name, viewport)| crate::presentation::Output { name, viewport })
                .collect(),
        )
        .unwrap();
    let window = harness.window();
    window.role.set_title("File drag source".into());
    let _buffer = paint(&mut harness, &window, [320, 240], [0, 255, 0, 255]);
    current(&mut canvas, &mut harness);
    let id = harness.server.windows()[0].id;
    let original = member(&mut canvas, id);
    canvas.create_workspace(root.to_bits()).unwrap();
    let destination = canvas
        .world_mut()
        .get::<native::workspace::Workspaces>(root)
        .unwrap()
        .active;
    for workspace in [original.1, destination] {
        native::workspace_config::set_physics(canvas.world_mut(), root, workspace, false);
    }
    canvas
        .activate_workspace(root.to_bits(), original.1)
        .unwrap();
    current(&mut canvas, &mut harness);
    canvas
        .desktop_action(DesktopAction::ToggleOverview)
        .unwrap();
    let frame = current(&mut canvas, &mut harness);
    assert!(frame.input.drop_target.is_none());
    let preview = {
        let world = canvas.world_mut();
        world
            .query::<(Entity, &Name)>()
            .iter(world)
            .find(|(_, name)| name.as_str() == format!("overview.workspace.{destination}"))
            .unwrap()
            .0
    };
    let point = center(&mut canvas, preview);
    let surface = harness.server.windows()[0].surface.clone();
    harness.server.focus_surface(
        Some(surface.clone()),
        smithay::utils::SERIAL_COUNTER.next_serial(),
    );
    harness
        .server
        .pointer_motion(Some((surface, [20.0, 20.0])), 1)
        .unwrap();
    harness.server.pointer_button(272, true, 2);
    harness.sync();
    let qh = harness.queue.handle();
    let manager = harness.client.data_manager.as_ref().unwrap();
    let device = manager.get_data_device(harness.client.seat.as_ref().unwrap(), &qh, ());
    let source = manager.create_data_source(&qh, ());
    source.offer("text/uri-list".into());
    source.set_actions(wl_data_device_manager::DndAction::Copy);
    harness.client.drop_payload = format!("{uri}\r\n").into_bytes();
    device.start_drag(
        Some(&source),
        &window.surface,
        None,
        harness.client.pointer_serial,
    );
    for _ in 0..4 {
        harness.sync();
    }
    harness.server.native_drop_target(None);
    let native = harness.server.native_surface().unwrap();
    harness
        .server
        .pointer_motion(Some((native.clone(), point.as_dvec2().to_array())), 3)
        .unwrap();
    for _ in 0..8 {
        harness.sync();
    }
    let token = harness
        .server
        .native_file_drag()
        .expect("The real URI offer is missing");
    canvas
        .file_drag(Some(FileDragContext {
            owner: root.to_bits(),
            output: Some(OutputId(1)),
            token,
        }))
        .unwrap();
    canvas.pointer(point.to_array()).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let request = loop {
        current(&mut canvas, &mut harness);
        if let Ok(HostRequest::OverviewHover(request)) = receiver.try_recv() {
            break request;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "The real URI offer never activated hover"
        );
        std::thread::sleep(Duration::from_millis(2));
    };
    assert_eq!(request.workspace, destination);
    assert_eq!(harness.server.native_file_drag(), Some(request.drag.token));
    canvas.overview_hover(request, true).unwrap();
    canvas.pointer(point.to_array()).unwrap();
    let frame = current(&mut canvas, &mut harness);
    assert!(!frame.input.keyboard_modal);
    capture("uri-activated", [1000, 720], 1.0, &frame);
    assert_eq!(
        canvas
            .world_mut()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .active,
        destination
    );
    assert_eq!(member(&mut canvas, id), original);
    let target = frame
        .input
        .drop_target
        .clone()
        .expect("The selected workspace has no normal file target");
    assert!(matches!(
        target.destination,
        crate::native_input::transfer::Destination::Canvas(_)
    ));
    harness.server.native_drop_target(Some(target.clone()));
    for _ in 0..8 {
        harness.sync();
    }
    harness.server.pointer_button(272, false, 4);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let drop = loop {
        harness.sync();
        if let Some(drop) = harness
            .server
            .native_events()
            .into_iter()
            .find_map(|event| match event {
                crate::native_input::client::Event::Drop(drop) => Some(drop),
                _ => None,
            })
        {
            break drop;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "The URI payload was not delivered"
        );
        std::thread::sleep(Duration::from_millis(2));
    };
    assert_eq!(drop.target, target);
    assert_eq!(
        drop.payload,
        crate::native_input::transfer::Payload::Uri(uri)
    );
    let dropped = drop.token;
    crate::native_input::transfer::enqueue(canvas.world_mut(), drop);
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    let mut finished = 0;
    let choice = loop {
        current(&mut canvas, &mut harness);
        for request in receiver.try_iter() {
            if let HostRequest::DropFinished { token, accepted } = request {
                assert_eq!(token, dropped);
                assert!(accepted);
                finished += 1;
                harness.server.finish_native_drop(token, accepted);
            }
        }
        if control(&mut canvas, "Display as Plain text Sand").is_some()
            && let Some(choice) = control(&mut canvas, "Use editable text Sand")
        {
            break choice;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "The normal file chooser did not become ready"
        );
        std::thread::sleep(Duration::from_millis(2));
    };
    let point = center(&mut canvas, choice);
    capture("uri-choice", [1000, 720], 1.0, &current(&mut canvas, &mut harness));
    harness
        .server
        .pointer_motion(Some((native, point.as_dvec2().to_array())), 5)
        .unwrap();
    canvas.pointer(point.to_array()).unwrap();
    current(&mut canvas, &mut harness);
    for pressed in [true, false] {
        harness.server.pointer_button(272, pressed, 6);
        canvas
            .button(bevy::input::mouse::MouseButton::Left, pressed)
            .unwrap();
        current(&mut canvas, &mut harness);
    }
    let world = canvas.world_mut();
    let fields: Vec<_> = world
        .query::<(Entity, &bevy::text::EditableText)>()
        .iter(world)
        .filter(|(_, text)| text.value().to_string() == BODY)
        .map(|(entity, _)| entity)
        .collect();
    assert_eq!(fields.len(), 1);
    let mut owner = fields[0];
    while world
        .get::<native::workspace::WorkspaceMember>(owner)
        .is_none()
    {
        owner = world.get::<ChildOf>(owner).unwrap().parent();
    }
    assert_eq!(
        world
            .get::<native::workspace::WorkspaceMember>(owner)
            .unwrap()
            .0,
        destination
    );
    assert_eq!(member(&mut canvas, id), original);
    assert_eq!(finished, 1);
    capture("uri-imported", [1000, 720], 1.0, &current(&mut canvas, &mut harness));
    for _ in 0..4 {
        harness.sync();
    }
    assert_eq!(harness.client.drop_performed, 1);
    assert_eq!(harness.client.drop_finished, 1);
    assert!(harness.server.native_file_drag().is_none());
    source.destroy();
    device.release();
    harness.sync();
}
