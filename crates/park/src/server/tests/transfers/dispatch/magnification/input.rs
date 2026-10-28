use super::*;
use super::super::{focus_policy, opening};
use lince_interface::host::magnification::Adjustment;

fn action(fixture: &mut Fixture, action: DesktopAction) {
    fixture.desktop.send(Input::Action(action)).unwrap(); settle(fixture);
}

fn factor(fixture: &mut Fixture, change: Adjustment, expected: f64) {
    let previous = fixture.desktop.magnification_snapshot().last_request;
    action(fixture, DesktopAction::Magnify(change));
    fixture.until(|fixture| fixture.desktop.magnification_snapshot().last_request > previous && saved(fixture, expected));
}

fn markers(fixture: &mut Fixture, surface: SurfaceId) -> [[f64; 2]; 3] {
    let geometry = clients::geometry(fixture, surface);
    let frame = &fixture.desktop.current.as_ref().unwrap().frame;
    let mapping = frame.input.mappings.get(geometry.output).unwrap().clone();
    let size = geometry.mapping.surface.size();
    let image = super::super::appearance::pixels(fixture, "magnified-virtual-warp-constraints");
    std::array::from_fn(|index| {
        let local = std::array::from_fn(|axis| (size[axis] * 0.5).floor() + f64::from(clients::MARKERS[index].0[axis]));
        let locator = mapping.logical_to_physical(geometry.mapping.canvas_point(local).unwrap()).unwrap();
        clients::marker(&image, fixture.size, locator, mapping.magnification(), clients::MARKERS[index].1, false)
    })
}

fn rendered_center(points: [[f64; 2]; 3]) -> [f64; 2] {
    std::array::from_fn(|axis| (154.0 * points[0][axis] + 156.0 * points[1][axis] + 123.0 * points[2][axis]) / 433.0)
}

fn assert_warp(fixture: &mut Fixture, surface: SurfaceId, window: &ClientWindow) -> [[f64; 2]; 3] {
    fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&window.surface)
        && fixture.client.pointer_focus.as_ref() == Some(&window.surface));
    let points = markers(fixture, surface);
    let viewport = &fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().outputs[0];
    clients::compare(viewport.raster.point(fixture.desktop.displayed_pointer()), rendered_center(points), "virtual warp against actual rendered client markers");
    let local = *fixture.client.pointer_positions.last().unwrap();
    let size = clients::geometry(fixture, surface).mapping.surface.size();
    for axis in 0..2 { assert!((local[axis] - size[axis] * 0.5).abs() <= 1.0); }
    points
}

fn assert_relative(points: [[f64; 2]; 3], actual: [f64; 4]) {
    let first: [f64; 2] = std::array::from_fn(|axis| points[1][axis] - points[0][axis]);
    let second: [f64; 2] = std::array::from_fn(|axis| points[2][axis] - points[0][axis]);
    let matrix: [[f64; 2]; 2] = std::array::from_fn(|axis| [
        (8.0 * first[axis] - 27.0 * second[axis]) / -866.0,
        (20.0 * second[axis] - 38.0 * first[axis]) / -866.0,
    ]);
    for (delta, expected) in [([actual[0], actual[1]], [8.0, -6.0]), ([actual[2], actual[3]], [4.0, -2.0])] {
        let rendered = std::array::from_fn(|axis| matrix[axis][0] * delta[0] + matrix[axis][1] * delta[1]);
        clients::compare(rendered, expected, "relative client delta against the independently observed pixel basis");
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn actual_client_constraints_relative_motion_and_virtual_host_warp_use_the_presented_lens_and_retire_stale_focus_work() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut fixture = create(&runtime, directory.path(), [1000, 720], 1.25);
    fixture.desktop.send(Input::CursorWarpSupport(true)).unwrap(); settle(&mut fixture);
    focus_policy::open(&mut fixture);
    focus_policy::activate(&mut fixture, "Warp cursor after keyboard focus: Off");
    focus_policy::activate(&mut fixture, "Save focus settings");
    focus_policy::close(&mut fixture);
    assert!(fixture.desktop.current.as_ref().unwrap().frame.input.focus_policy.warp);
    let (first, first_buffer, first_id) = opening::map(&mut fixture);
    let (second, second_buffer, second_id) = opening::map(&mut fixture);
    for window in [&first, &second] { window.role.set_max_size(240, 180); window.surface.commit(); }
    fixture.exchange(); action(&mut fixture, DesktopAction::SetLayout(WorkspaceLayout::Free));
    fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        let workspace = world.get::<native::workspace::Workspaces>(root).unwrap().active;
        native::workspace_config::set_physics(world, root, workspace, false);
        let windows = world.query::<(Entity, &native::layout::desktop::ApplicationWindow)>().iter(world).map(|(entity, window)| (entity, window.surface)).collect::<Vec<_>>();
        for (entity, surface) in windows {
            world.get_mut::<native::canvas::CanvasItem>(entity).unwrap().position = bevy::math::DVec2::new(if surface == first_id { -180.0 } else { 180.0 }, 0.0);
        }
    });
    let mut buffers = vec![first_buffer, second_buffer];
    clients::repaint(&mut fixture, &first, first_id, &mut buffers);
    clients::repaint(&mut fixture, &second, second_id, &mut buffers);
    fixture.desktop.send(Input::Application(second_id)).unwrap(); settle(&mut fixture);
    for step in 1..=4 { factor(&mut fixture, Adjustment::Increase, 1.0 + f64::from(step) * 0.25); }
    action(&mut fixture, DesktopAction::Focus(Direction::Left));
    assert_warp(&mut fixture, first_id, &first);
    let serial = fixture.desktop.worker.input(Input::Action(DesktopAction::Focus(Direction::Right))).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        fixture.exchange(); fixture.desktop.dispatch().unwrap(); fixture.desktop.accept(&mut fixture.composition, &mut fixture.renderer).unwrap();
        if fixture.desktop.pending.as_ref().is_some_and(|frame| frame.input_serial >= serial && frame.frame.ready().unwrap()
            && frame.frame.input.cursor_warp.is_some()) { break; }
        assert!(Instant::now() < deadline, "The real focus request did not reach a withheld presentation");
        std::thread::sleep(Duration::from_millis(2));
    }
    let moved = fixture.desktop.displayed_pointer().map(|value| value + 2.0);
    fixture.desktop.move_pointer(moved, 40).unwrap();
    let retained = fixture.desktop.displayed_pointer();
    fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&second.surface));
    let raster = fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().outputs[0].raster;
    clients::compare(raster.point(fixture.desktop.displayed_pointer()), raster.point(retained), "a pre-motion pending warp must retain newer real pointer input");
    action(&mut fixture, DesktopAction::Focus(Direction::Left));
    let points = assert_warp(&mut fixture, first_id, &first);
    let qh = fixture.queue.handle();
    let seat = fixture.client.seat.as_ref().unwrap();
    let (pointer, locked, relative) = fixture.client.controls.pointer_lock(&first.surface, seat, &qh);
    fixture.until(|fixture| fixture.desktop.server.pointer_locked() && fixture.client.controls.lock_events().last() == Some(&true));
    let position = fixture.desktop.displayed_pointer();
    fixture.desktop.move_pointer(position.map(|value| value + 200.0), 50).unwrap(); fixture.tick();
    clients::compare(raster.point(fixture.desktop.displayed_pointer()), raster.point(position), "a locked client cannot admit absolute lens motion");
    let before = fixture.client.controls.relative_deltas().len();
    fixture.desktop.relative_motion([8.0 / 1.25, -6.0 / 1.25], [4.0 / 1.25, -2.0 / 1.25], 1000);
    fixture.until(|fixture| fixture.client.controls.relative_deltas().len() == before + 1);
    assert_relative(points, fixture.client.controls.relative_deltas()[before]);
    factor(&mut fixture, Adjustment::Increase, 2.25);
    assert!(fixture.desktop.server.pointer_locked());
    assert_eq!(fixture.client.controls.lock_events(), [true]);
    action(&mut fixture, DesktopAction::Focus(Direction::Right));
    assert_warp(&mut fixture, second_id, &second);
    assert_eq!(fixture.client.controls.lock_events(), [true, false]);
    locked.destroy(); relative.destroy(); pointer.release(); fixture.exchange();
    let local = clients::geometry(&fixture, second_id).mapping.surface.size().map(|value| value * 0.5);
    let region = fixture.client.compositor.as_ref().unwrap().create_region(&qh, ());
    region.add(local[0] as i32 - 20, local[1] as i32 - 20, 40, 40);
    let (pointer, confined) = fixture.client.controls.pointer_confinement(&second.surface, fixture.client.seat.as_ref().unwrap(), &region, &qh);
    fixture.until(|fixture| fixture.desktop.server.pointer_constrained());
    let point = fixture.desktop.displayed_pointer();
    fixture.desktop.move_pointer([point[0] + 500.0, point[1] + 400.0], 60).unwrap(); fixture.tick();
    assert_eq!(fixture.client.pointer_focus.as_ref(), Some(&second.surface));
    let constrained = *fixture.client.pointer_positions.last().unwrap();
    for axis in 0..2 { assert!(constrained[axis] >= local[axis] - 20.0 && constrained[axis] < local[axis] + 20.0); }
    let buttons = fixture.client.button_recipients.len();
    fixture.desktop.button(272, true, 61).unwrap(); fixture.desktop.button(272, false, 62).unwrap();
    fixture.until(|fixture| fixture.client.button_recipients.len() == buttons + 2);
    assert_eq!(&fixture.client.button_recipients[buttons..], &[(Some(second.surface.id()), 272, true), (Some(second.surface.id()), 272, false)]);
    second.role.destroy(); second.xdg.destroy(); second.surface.destroy();
    fixture.until(|fixture| fixture.desktop.server.windows().iter().all(|window| window.id != second_id) && !fixture.desktop.server.pointer_constrained());
    confined.destroy(); pointer.release(); region.destroy(); fixture.exchange();
    fixture.desktop.send(Input::Application(first_id)).unwrap(); settle(&mut fixture);
    let points = assert_warp(&mut fixture, first_id, &first);
    let physical = rendered_center(points);
    let local = clients::geometry(&fixture, first_id).mapping.surface.size().map(|value| value * 0.5);
    clients::pointer_click(&mut fixture, OutputId(1), physical, &first.surface, local);
    fixture.client.keys.clear(); press(&mut fixture, 30);
    fixture.until(|fixture| fixture.client.keys.len() == 2);
    assert_eq!(fixture.client.keys, [(30, true), (30, false)]);
    assert!(fixture.desktop.running && fixture.desktop.keys.is_empty() && fixture.desktop.buttons.is_empty());
    fixture.desktop.worker.stop().unwrap(); drop(buffers);
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn keyboard_tab_reverse_tab_and_long_shortcut_list_reveal_stay_put_under_a_stationary_pointer_and_real_client_typing_recovers() {
    use lince_interface::native::shortcuts::Shortcut;
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("park.json");
    std::fs::write(&path, serde_json::to_vec(&serde_json::json!({"outputs":{(OUTPUT):{"scale":1.5,"magnification":4.0}}})).unwrap()).unwrap();
    let original = std::fs::read(&path).unwrap();
    let mut fixture = create(&runtime, directory.path(), [360, 720], 1.5);
    let bindings = std::fs::read(directory.path().join("interface.lingua")).ok();
    let (window, _buffer, surface) = opening::map(&mut fixture);
    open(&mut fixture);
    let reset = entity(&mut fixture, "Reset draft to 1×");
    focus(&mut fixture, "Cancel draft");
    fixture.desktop.key(42, true, 80).unwrap(); press(&mut fixture, 15); fixture.desktop.key(42, false, 81).unwrap(); settle(&mut fixture);
    assert_eq!(fixture.inspect(|canvas| canvas.world_mut().resource::<bevy::input_focus::InputFocus>().get()), Some(reset));
    focus_entity(&mut fixture, reset, "Reset draft to 1×");
    tap(&mut fixture, 28);
    assert_eq!(value(&mut fixture), "1");
    opening::activate(&mut fixture, "Cancel draft");
    assert_eq!(value(&mut fixture), "4");
    opening::activate(&mut fixture, "Edit all shortcuts");
    let command = Shortcut::ALL.last().unwrap().id();
    let (capture, unbind) = fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        let row = world.query::<(Entity, &Name)>().iter(world).find(|(_, name)| name.as_str() == command).unwrap().0;
        let within = |mut entity: Entity| {
            for _ in 0..native::layout::MAX_DEPTH {
                if entity == row { return true; }
                let Some(parent) = world.get::<ChildOf>(entity) else { return false; }; entity = parent.parent();
            }
            false
        };
        let capture = world.iter_entities().find(|entity| entity.get::<Name>().is_some_and(|name| name.as_str() == "shortcut.capture")
            && within(entity.id())).unwrap().id();
        let unbind = world.iter_entities().find(|entity| entity.get::<bevy::a11y::AccessibilityNode>().is_some_and(|node| node.label() == Some("Unbind"))
            && within(entity.id())).unwrap().id();
        (capture, unbind)
    });
    let viewport = fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().outputs[0].clone();
    fixture.desktop.move_pointer(viewport.raster.logical_point([180.0, 360.0]), 90).unwrap(); fixture.tick(); settle(&mut fixture);
    let stationary = fixture.desktop.displayed_pointer();
    let mut reached = false;
    for _ in 0..512 {
        if fixture.inspect(|canvas| canvas.world_mut().resource::<bevy::input_focus::InputFocus>().get()) == Some(capture) { reached = true; break; }
        fixture.desktop.key(42, true, 91).unwrap(); press(&mut fixture, 15); fixture.desktop.key(42, false, 92).unwrap(); settle(&mut fixture);
    }
    assert!(reached, "Reverse Tab could not reach the final real command row in the full shortcut list");
    focus_entity(&mut fixture, capture, "Last command binding");
    tap(&mut fixture, 15);
    assert_eq!(fixture.inspect(|canvas| canvas.world_mut().resource::<bevy::input_focus::InputFocus>().get()), Some(unbind));
    fixture.desktop.key(42, true, 93).unwrap(); press(&mut fixture, 15); fixture.desktop.key(42, false, 94).unwrap(); settle(&mut fixture);
    assert_eq!(fixture.inspect(|canvas| canvas.world_mut().resource::<bevy::input_focus::InputFocus>().get()), Some(capture));
    focus_entity(&mut fixture, capture, "Last command binding");
    let revealed = fixture.desktop.current.as_ref().unwrap().frame.input.mappings.get(OutputId(1)).unwrap().clone();
    let deadline = Instant::now() + Duration::from_millis(350);
    while Instant::now() < deadline {
        fixture.tick();
        assert_eq!(fixture.desktop.current.as_ref().unwrap().frame.input.mappings.get(OutputId(1)), Some(&revealed),
            "A stationary pointer undid the keyboard's reveal of the scrolled command row");
        assert_eq!(fixture.desktop.displayed_pointer(), stationary);
        std::thread::sleep(Duration::from_millis(2));
    }
    opening::activate(&mut fixture, "Cancel draft");
    opening::close(&mut fixture);
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(std::fs::read(directory.path().join("interface.lingua")).ok(), bindings);
    assert!(saved(&fixture, 4.0));
    fixture.desktop.send(Input::Application(surface)).unwrap(); settle(&mut fixture);
    let item = clients::geometry(&fixture, surface);
    let local = item.mapping.surface.size().map(|value| value * 0.5);
    let point = item.mapping.canvas_point(local).unwrap();
    let frame = &fixture.desktop.current.as_ref().unwrap().frame;
    let physical = frame.input.mappings.get(item.output).unwrap().logical_to_physical(point).unwrap();
    clients::pointer_click(&mut fixture, item.output, physical, &window.surface, local);
    fixture.client.keys.clear(); press(&mut fixture, 30);
    fixture.until(|fixture| fixture.client.keys.len() == 2);
    assert_eq!(fixture.client.keys, [(30, true), (30, false)]);
    assert!(fixture.desktop.keys.is_empty() && fixture.desktop.buttons.is_empty());
    fixture.desktop.worker.stop().unwrap();
}
