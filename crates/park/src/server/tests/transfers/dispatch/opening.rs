use super::*;
use lince_interface::native::app::CellHandle;
use nucleus::window_opening::{Dimension, Rule, Window, RULE_NAMESPACE, WINDOW_NAMESPACE};

const APP: &str = "org.example.editor";
const OUTPUT: &str = "Opening-fixture";

pub(super) fn provider(engine: std::sync::Arc<engine::Engine>, directory: &std::path::Path) -> CellHandle {
    CellHandle(cell::CellRuntime { store: engine.store.clone(), engine, commands: cell::terminal::commands::CommandHost::new(directory.into()),
        lanes: std::sync::Arc::new(cell::LaneHub::new()), wire: Default::default(), information: None, speech: None, fiote: None })
}

pub(super) fn persisted(runtime: &tokio::runtime::Runtime, directory: &std::path::Path) -> std::sync::Arc<engine::Engine> {
    let url = format!("sqlite://{}", directory.join("cell.sqlite").display());
    std::sync::Arc::new(runtime.block_on(engine::Engine::open(&url)).unwrap())
}

pub(super) fn released(runtime: &tokio::runtime::Runtime, engine: std::sync::Weak<engine::Engine>) {
    runtime.block_on(async {
        let deadline = Instant::now() + Duration::from_secs(30);
        while engine.strong_count() != 0 {
            assert!(Instant::now() < deadline, "The old opening-rule worker/Cell retained its Engine after disposal");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    });
}

fn create(runtime: &tokio::runtime::Runtime, directory: &std::path::Path, size: [u32; 2], scale: f64, cell: CellHandle) -> Fixture {
    let mut harness = Harness::new();
    harness.server.configure_scaled_outputs(&[Monitor { id: OutputId(1), name: OUTPUT.into(), origin: [0, 0], size: size.map(|value| value as i32) }],
        &std::collections::BTreeMap::from([(OutputId(1), scale)])).unwrap();
    harness.sync();
    let mut fixture = Fixture::with_cell(harness, runtime, directory, size, cell);
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.control.context.is_some()));
    ready(&mut fixture);
    fixture
}

pub(super) fn ready(fixture: &mut Fixture) {
    let id = nucleus::new_uid("opening-fixture");
    let request = id.clone();
    let (sent, received) = std::sync::mpsc::sync_channel(1);
    let sender = fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        let sender = world.get_non_send::<native::cell_bridge::CellBridge>().unwrap().outgoing.clone();
        world.resource_mut::<Schedules>().get_mut(Update).unwrap().add_systems((move |mut messages: MessageReader<native::cell_bridge::CellMessage>| {
            for message in messages.read() {
                if let cell::ServerMessage::Snapshot { id, rows } | cell::ServerMessage::Update { id, rows } = &message.0
                    && rows.iter().any(|row| row.get("extension").is_some())
                { eprintln!("Opening fixture projected rows {id}: {rows:?}"); }
                match &message.0 {
                    cell::ServerMessage::Snapshot { id, .. } if id == &request => { let _ = sent.try_send(Ok(())); }
                    cell::ServerMessage::Error { id, message, .. } if id == &request => { let _ = sent.try_send(Err(message.clone())); }
                    _ => {}
                }
            }
        }).after(native::cell_bridge::ReceiveCell));
        sender
    });
    let mut subscription = native::cell_bridge::records_subscription();
    let cell::ClientMessage::Subscribe { id: subscription_id, .. } = &mut subscription else { unreachable!() };
    *subscription_id = id.clone();
    sender.try_send(subscription).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        fixture.tick();
        if let Ok(result) = received.try_recv() { result.unwrap(); break; }
        assert!(Instant::now() < deadline, "The actual local Cell did not acknowledge its fixture subscription");
        std::thread::sleep(Duration::from_millis(2));
    }
    sender.try_send(cell::ClientMessage::Unsubscribe { id }).unwrap();
    fixture.inspect(|_| ());
}

fn tap(fixture: &mut Fixture, code: u32) {
    fixture.desktop.key(code, true, 10).unwrap();
    fixture.desktop.key(code, false, 11).unwrap();
}

pub(super) fn operation(fixture: &mut Fixture) {
    fixture.desktop.key(125, true, 20).unwrap();
    tap(fixture, 25);
    fixture.desktop.key(125, false, 21).unwrap();
    fixture.until(|fixture| !fixture.desktop.input_pending()
        && fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.modal && frame.frame.input.editor.is_some()));
}

pub(super) fn activate(fixture: &mut Fixture, caption: &str) {
    eprintln!("Activating control: {caption}");
    let entity = available(fixture, caption);
    click_entity(fixture, entity, caption);
}

pub(super) fn available(fixture: &mut Fixture, caption: &str) -> Entity {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(entity) = element(fixture, caption)
            && fixture.inspect(move |canvas| canvas.world_mut().get::<bevy::ui::InteractionDisabled>(entity).is_none()) { return entity; }
        assert!(Instant::now() < deadline, "The ordinary opening control {caption} did not become available");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn element(fixture: &mut Fixture, caption: &str) -> Option<Entity> {
    let caption = caption.to_owned();
    fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        let owner = native::shortcuts::keyboard_owner(world);
        world.query::<(Entity, &bevy::a11y::AccessibilityNode)>().iter(world).find(|(entity, node)| {
            if node.label() != Some(caption.as_str())
                || !world.get::<InheritedVisibility>(*entity).is_some_and(|visible| visible.get())
                || world.get::<native::actions::ActionButton>(*entity).is_none()
                    && world.get::<bevy::ui_widgets::Button>(*entity).is_none()
                    && world.get::<bevy::text::EditableText>(*entity).is_none() { return false; }
            let Some(owner) = owner else { return true; };
            let mut current = Some(*entity);
            for _ in 0..128 {
                let Some(entity) = current else { return false; };
                if entity == owner { return true; }
                current = world.get::<ChildOf>(entity).map(ChildOf::parent);
            }
            false
        }).map(|(entity, _)| entity)
    })
}

fn named(fixture: &mut Fixture, name: &str) -> Entity {
    let name = name.to_owned();
    fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        world.query::<(Entity, &Name)>().iter(world).find(|(_, candidate)| candidate.as_str() == name).unwrap().0
    })
}

fn control(fixture: &mut Fixture, uid: &str, action: usize) {
    let name = format!("opening.rule.{uid}.control.{action}");
    let entity = named(fixture, &name);
    click_entity(fixture, entity, &name);
}

fn point(fixture: &mut Fixture, entity: Entity) -> Option<[f64; 2]> {
    let (output, points) = fixture.inspect(move |canvas| canvas.native_test_points(entity))?;
    let frame = &fixture.desktop.current.as_ref()?.frame;
    let center: [f64; 2] = std::array::from_fn(|axis| points.iter().map(|point| point[axis]).sum::<f64>() * 0.25);
    let extent: [f64; 2] = std::array::from_fn(|axis| points.iter().map(|point| point[axis]).fold(f64::NEG_INFINITY, f64::max)
        - points.iter().map(|point| point[axis]).fold(f64::INFINITY, f64::min));
    if extent.into_iter().any(|value| value < 1.0) { return None; }
    std::iter::once(center).chain(points.map(|corner| std::array::from_fn(|axis| center[axis] + (corner[axis] - center[axis]) * 0.75)))
        .find(|point| frame.input.mappings.get(output).is_some_and(|mapping| mapping.logical_bounds().contains(*point))
            && matches!(fixture.desktop.target_at(*point), crate::desktop::input::Target::Native)
            && frame.input.native_pointer.target(*point).is_some_and(|target| target.contains(entity)))
}

fn visible(fixture: &mut Fixture, entity: Entity) -> bool { point(fixture, entity).is_some() }

fn scroll_toward(fixture: &mut Fixture, entity: Entity) -> bool {
    let request = fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        let point = world.get::<UiGlobalTransform>(entity)?.translation;
        let mut ancestor = world.get::<ChildOf>(entity).map(ChildOf::parent);
        for _ in 0..native::layout::MAX_DEPTH {
            let current = ancestor?;
            ancestor = world.get::<ChildOf>(current).map(ChildOf::parent);
            let Some(node) = world.get::<Node>(current) else { continue; };
            let (Some(computed), Some(transform)) = (world.get::<ComputedNode>(current), world.get::<UiGlobalTransform>(current)) else { continue; };
            let mut bounds = Rect::from_center_size(transform.translation, computed.size());
            if let Some(clip) = world.get::<bevy::ui::CalculatedClip>(current) { bounds = bounds.intersect(clip.clip); }
            if bounds.size().min_element() <= 0.0 { continue; }
            let axes = [node.overflow.x == OverflowAxis::Scroll, node.overflow.y == OverflowAxis::Scroll];
            let delta: [f64; 2] = std::array::from_fn(|axis| {
                if axes[axis] && (point[axis] < bounds.min[axis] || point[axis] > bounds.max[axis]) {
                    f64::from((point[axis] - bounds.center()[axis]) * computed.inverse_scale_factor()).clamp(-4096.0, 4096.0)
                } else { 0.0 }
            });
            if delta != [0.0; 2] { return Some((current, delta)); }
        }
        None
    });
    let Some((scroll, delta)) = request else { return false; };
    let Some((output, points)) = fixture.inspect(move |canvas| canvas.native_test_points(scroll)) else { return false; };
    let point = [points.iter().map(|point| point[0]).fold(f64::NEG_INFINITY, f64::max) - 3.0,
        points.iter().map(|point| point[1]).sum::<f64>() * 0.25];
    let frame = &fixture.desktop.current.as_ref().unwrap().frame;
    if !frame.input.mappings.get(output).is_some_and(|mapping| mapping.logical_bounds().contains(point))
        || frame.input.native_pointer.target(point).is_none() { return false; }
    fixture.desktop.move_pointer(point, 20).unwrap();
    fixture.tick();
    use smithay::{backend::input::Axis, input::pointer::AxisFrame};
    fixture.desktop.axis(delta, AxisFrame::new(21).value(Axis::Horizontal, delta[0]).value(Axis::Vertical, delta[1])).unwrap();
    fixture.inspect(|_| ());
    true
}

pub(super) fn click_entity(fixture: &mut Fixture, entity: Entity, caption: &str) {
    eprintln!("Clicking control: {caption} ({entity:?})");
    for _ in 0..16 {
        if visible(fixture, entity) || !scroll_toward(fixture, entity) { break; }
    }
    if !visible(fixture, entity) { magnification::focus_entity(fixture, entity, caption); }
    let point = point(fixture, entity).unwrap_or_else(|| panic!("The visible opening control {caption} needs its own actual native input recipient"));
    fixture.desktop.move_pointer(point, 20).unwrap();
    fixture.tick();
    fixture.desktop.button(272, true, 21).unwrap();
    fixture.tick();
    fixture.desktop.button(272, false, 22).unwrap();
    fixture.inspect(|_| ());
}

pub(super) fn click(fixture: &mut Fixture, caption: &str) {
    let entity = element(fixture, caption).unwrap_or_else(|| panic!("The opening control {caption} is unavailable"));
    click_entity(fixture, entity, caption);
}

fn wait_control(fixture: &mut Fixture, uid: &str, action: usize, caption: &str) {
    let name = format!("opening.rule.{uid}.control.{action}");
    let caption = caption.to_owned();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let name = name.clone();
        let caption = caption.clone();
        let available = fixture.inspect(move |canvas| {
            let world = canvas.world_mut();
            world.query::<(Entity, &Name)>().iter(world).any(|(entity, name_value)| name_value.as_str() == name
                && world.get::<bevy::ui::InteractionDisabled>(entity).is_none()
                && world.get::<bevy::a11y::AccessibilityNode>(entity).is_some_and(|node| node.label() == Some(caption.as_str())))
        });
        if available { break; }
        assert!(Instant::now() < deadline, "The saved rule did not reach its actual rendered control state");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn write_entity(fixture: &mut Fixture, entity: Entity, value: &str) {
    click_entity(fixture, entity, "opening field");
    assert_eq!(fixture.inspect(move |canvas| canvas.world_mut().resource::<bevy::input_focus::InputFocus>().get()), Some(entity));
    fixture.desktop.key(29, true, 12).unwrap();
    tap(fixture, 30);
    fixture.desktop.key(29, false, 13).unwrap();
    tap(fixture, 14);
    for (index, character) in value.chars().enumerate() {
        let (code, shift) = match character {
            'a' => (30, false), 'b' => (48, false), 'c' => (46, false), 'd' => (32, false), 'e' => (18, false), 'f' => (33, false),
            'g' => (34, false), 'h' => (35, false), 'i' => (23, false), 'j' => (36, false), 'k' => (37, false), 'l' => (38, false),
            'm' => (50, false), 'n' => (49, false), 'o' => (24, false), 'p' => (25, false), 'q' => (16, false), 'r' => (19, false),
            's' => (31, false), 't' => (20, false), 'u' => (22, false), 'v' => (47, false), 'w' => (17, false), 'x' => (45, false),
            'y' => (21, false), 'z' => (44, false), '0' => (11, false), '1'..='9' => (2 + character as u32 - '1' as u32, false),
            ' ' => (57, false), '.' => (52, false), '/' => (53, false), '-' => (12, false), '_' => (12, true),
            _ => panic!("The fixture needs an explicit US key for {character:?}"),
        };
        if shift { fixture.desktop.key(42, true, 14).unwrap(); }
        tap(fixture, code);
        if shift { fixture.desktop.key(42, false, 15).unwrap(); }
        if index % 16 == 15 { fixture.inspect(|_| ()); }
    }
    fixture.inspect(|_| ());
    let expected = value.to_owned();
    assert_eq!(fixture.inspect(move |canvas| canvas.world_mut().get::<bevy::text::EditableText>(entity).unwrap().value().to_string()), expected);
}

pub(super) fn write(fixture: &mut Fixture, caption: &str, value: &str) {
    let entity = available(fixture, caption);
    write_entity(fixture, entity, value);
}

fn open(fixture: &mut Fixture) {
    fixture.desktop.key(125, true, 1).unwrap();
    tap(fixture, 18);
    fixture.desktop.key(125, false, 2).unwrap();
    fixture.inspect(|_| ());
    activate(fixture, "Park");
    activate(fixture, "Edit opening rules");
}

pub(super) fn close(fixture: &mut Fixture) {
    activate(fixture, lince_interface::i18n::Token::CloseEditMode.text());
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| !frame.frame.input.modal && !frame.frame.input.keyboard_modal));
}

fn saved(fixture: &mut Fixture) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let accepted = fixture.inspect(|canvas| {
            let world = canvas.world_mut();
            let owner = world.query::<(Entity, &Name)>().iter(world).find(|(_, name)| name.as_str() == "park.opening-editor").unwrap().0;
            world.query::<(Entity, &Text)>().iter(world).any(|(entity, text)| {
                if !text.0.starts_with("Opening preferences saved.") { return false; }
                let mut current = Some(entity);
                for _ in 0..128 {
                    let Some(entity) = current else { return false; };
                    if entity == owner { return true; }
                    current = world.get::<ChildOf>(entity).map(ChildOf::parent);
                }
                false
            })
        });
        if accepted { return; }
        if Instant::now() >= deadline {
            let messages = fixture.inspect(|canvas| {
                let world = canvas.world_mut();
                let owner = world.query::<(Entity, &Name)>().iter(world).find(|(_, name)| name.as_str() == "park.opening-editor").unwrap().0;
                world.query::<(Entity, &Text)>().iter(world).filter_map(|(entity, text)| {
                    let mut current = Some(entity);
                    for _ in 0..128 {
                        let entity = current?;
                        if entity == owner { return Some(text.0.clone()); }
                        current = world.get::<ChildOf>(entity).map(ChildOf::parent);
                    }
                    None
                }).collect::<Vec<_>>()
            });
            panic!("The rendered editor did not receive a successful real Record acknowledgement: {messages:?}");
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn rule(runtime: &tokio::runtime::Runtime, engine: &engine::Engine, title: &str) -> (String, Rule) {
    let records = runtime.block_on(store::records::list_all(&engine.store.pool)).unwrap();
    let records = records.iter().filter(|record| record.head == title).collect::<Vec<_>>();
    assert_eq!(records.len(), 1);
    let uid = records[0].uid.clone();
    let value = runtime.block_on(store::records::get_extension(&engine.store.pool, &uid, RULE_NAMESPACE)).unwrap().unwrap();
    (uid, serde_json::from_value(value).unwrap())
}

fn new_rule(fixture: &mut Fixture, title: &str, width: &str, workspace: u64, conditions: usize) {
    activate(fixture, "New opening rule");
    write(fixture, "New rule name", title);
    write(fixture, "Exact matching value", APP);
    for _ in 1..conditions {
        click(fixture, "Add condition");
        let input = fixture.inspect(|canvas| {
            let world = canvas.world_mut();
            let mut inputs = world.query::<(Entity, &bevy::a11y::AccessibilityNode, &UiGlobalTransform)>().iter(world)
                .filter(|(entity, node, _)| node.label() == Some("Exact matching value") && world.get::<bevy::text::EditableText>(*entity).is_some())
                .map(|(entity, _, transform)| (transform.transform_point2(Vec2::ZERO).y, entity)).collect::<Vec<_>>();
            inputs.sort_by(|a, b| a.0.total_cmp(&b.0));
            inputs.last().unwrap().1
        });
        write_entity(fixture, input, APP);
    }
    activate(fixture, "Initial workspace");
    activate(fixture, &format!("Work · {workspace}"));
    activate(fixture, "Floating: Inherit");
    activate(fixture, "Floating: Yes");
    activate(fixture, "Initial focus: Inherit");
    activate(fixture, "Initial focus: Yes");
    write(fixture, "Initial width (blank inherits)", "0");
    let save = element(fixture, "Save opening preferences").unwrap();
    assert!(fixture.inspect(move |canvas| canvas.world_mut().get::<bevy::ui::InteractionDisabled>(save).is_some()));
    write(fixture, "Initial width (blank inherits)", width);
    click(fixture, "Save opening preferences");
    saved(fixture);
    click(fixture, "Cancel");
}

pub(super) fn map(fixture: &mut Fixture) -> (ClientWindow, wl_buffer::WlBuffer, SurfaceId) {
    let before = fixture.desktop.server.windows().iter().map(|window| window.id).collect::<std::collections::HashSet<_>>();
    let qh = fixture.queue.handle();
    let surface = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
    let xdg = fixture.client.shell.as_ref().unwrap().get_xdg_surface(&surface, &qh, ());
    let role = xdg.get_toplevel(&qh, ());
    role.set_title("editor one".into());
    role.set_app_id(APP.into());
    role.set_min_size(150, 80);
    surface.commit();
    fixture.exchange();
    let buffer = privacy::buffer(fixture, [320, 240], [30, 200, 60, 255]);
    surface.attach(Some(&buffer), 0, 0);
    surface.damage_buffer(0, 0, 320, 240);
    surface.commit();
    fixture.until(|fixture| fixture.desktop.server.windows().iter().any(|window| window.mapped && !before.contains(&window.id)));
    let id = fixture.desktop.server.windows().iter().find(|window| window.mapped && !before.contains(&window.id)).unwrap().id;
    fixture.inspect(|_| ());
    (ClientWindow { surface, xdg, role }, buffer, id)
}

fn decision(fixture: &mut Fixture, id: SurfaceId) -> native::layout::desktop::opening::Decision {
    fixture.inspect(move |canvas| {
        let (entity, _, _) = member(canvas, id);
        canvas.world_mut().get::<native::layout::desktop::opening::Decision>(entity).unwrap().clone()
    })
}

fn repaint(fixture: &mut Fixture, window: &ClientWindow, id: SurfaceId, retained: &mut Vec<wl_buffer::WlBuffer>) -> [i32; 2] {
    for _ in 0..8 {
        fixture.inspect(|_| ());
        let size = fixture.client.window_configurations[&window.role.id()].0;
        assert!(size.iter().all(|value| *value > 0));
        let mapped = fixture.desktop.server.windows().into_iter().find(|window| window.id == id).unwrap();
        if mapped.nodes[0].layout.size == size.map(f64::from) { return size; }
        let buffer = privacy::buffer(fixture, size.map(|value| value as u32), [30, 200, 60, 255]);
        window.surface.attach(Some(&buffer), 0, 0);
        window.surface.damage_buffer(0, 0, size[0], size[1]);
        window.surface.commit();
        retained.push(buffer);
        fixture.until(|fixture| fixture.desktop.server.windows().iter().find(|window| window.id == id)
            .is_some_and(|window| window.nodes[0].layout.size == size.map(f64::from)));
    }
    panic!("The real opening-rule client did not settle its configure/ack/buffer size")
}

fn action(fixture: &mut Fixture, action: DesktopAction) {
    fixture.desktop.send(Input::Action(action)).unwrap();
    fixture.inspect(|_| ());
    fixture.until(|fixture| fixture.desktop.pending_output.is_none());
}

fn request(fixture: &mut Fixture, id: SurfaceId) -> lince_interface::host::OpeningRequest {
    fixture.inspect(move |canvas| {
        let (_, workspace, placement) = member(canvas, id);
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        let position = world.get::<native::canvas::CanvasView>(root).unwrap().center.to_array();
        native::layout::desktop::opening::request(world, root, lince_interface::host::WindowInspection {
            owner: root.to_bits(), workspace, position, surface: id, placement: placement.0,
        }).unwrap()
    })
}

fn retired(fixture: &mut Fixture, request: &lince_interface::host::OpeningRequest) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let request = request.clone();
        if !fixture.inspect(move |canvas| {
            let world = canvas.world_mut();
            let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
            native::layout::desktop::opening::request_current(world, root, &request)
        }) { return; }
        assert!(Instant::now() < deadline, "A real Record/output change did not retire its older captured Reapply");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn arrangement(fixture: &mut Fixture) -> (u64, Vec<(u64, lince_interface::layout::desktop::Workspace)>, Option<native::output_views::OutputViews>) {
    fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let spaces = world.query::<&native::workspace::Workspaces>().single(world).unwrap();
        (spaces.active, spaces.entries.iter().map(|space| (space.id, space.desktop.clone())).collect(), world.get_resource::<native::output_views::OutputViews>().cloned())
    })
}

fn queued_stale(fixture: &mut Fixture, request: lince_interface::host::OpeningRequest, live: SurfaceId) {
    let before = arrangement(fixture);
    let before_decision = decision(fixture, live);
    let focus = fixture.client.keyboard_focus.as_ref().map(|surface| surface.id());
    let progress = fixture.inspect(|canvas| canvas.world_mut().resource::<crate::runtime::InputProgress>().clone());
    let (_, release) = fixture.pause();
    let mut serial = fixture.desktop.worker.input(Input::ReapplyOpening(request)).unwrap();
    let mut queued = 1;
    while let Ok(next) = fixture.desktop.worker.input(Input::Pointer([0.0; 2])) { serial = next; queued += 1; assert!(queued <= 128); }
    assert_eq!(queued, 128);
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !progress.reached(serial) {
        assert!(fixture.desktop.worker.failure().is_none());
        assert!(Instant::now() < deadline, "The stale Reapply input queue did not drain before presenting recovery frames");
        std::thread::sleep(Duration::from_millis(2));
    }
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.input_serial >= serial));
    assert_eq!(arrangement(fixture), before);
    assert_eq!(decision(fixture, live), before_decision);
    assert_eq!(fixture.client.keyboard_focus.as_ref().map(|surface| surface.id()), focus);
    assert!(fixture.desktop.buttons.is_empty());
}

fn races(fixture: &mut Fixture, runtime: &tokio::runtime::Runtime, engine: &engine::Engine, window: &ClientWindow, id: SurfaceId, half: &str) -> (ClientWindow, wl_buffer::WlBuffer) {
    eprintln!("Checking stale Reapply after a rule revision");
    let captured = request(fixture, id);
    let (_, mut rule) = rule(runtime, engine, "editor half");
    rule.revision += 1;
    runtime.block_on(engine.act(engine::actions::Action::SetExtension {
        target: half.into(), namespace: RULE_NAMESPACE.into(), fds: serde_json::to_value(rule).unwrap(),
    }, None)).unwrap();
    retired(fixture, &captured);
    queued_stale(fixture, captured, id);

    eprintln!("Checking stale Reapply after a window override revision");
    let captured = request(fixture, id);
    let overrides = runtime.block_on(store::records::all_extensions(&engine.store.pool, WINDOW_NAMESPACE)).unwrap();
    assert_eq!(overrides.len(), 1);
    let (uid, value) = overrides.into_iter().next().unwrap();
    let mut value: Window = serde_json::from_value(value).unwrap();
    value.revision += 1;
    value.properties.width = Some(Dimension::Fraction([1, 3]));
    runtime.block_on(engine.act(engine::actions::Action::SetExtension { target: uid, namespace: WINDOW_NAMESPACE.into(), fds: serde_json::to_value(value).unwrap() }, None)).unwrap();
    retired(fixture, &captured);
    queued_stale(fixture, captured, id);

    eprintln!("Checking stale Reapply after an output scale change");
    let captured = request(fixture, id);
    let scale = fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().outputs[0].raster.scale[0];
    fixture.desktop.server.configure_scaled_outputs(&[Monitor { id: OutputId(1), name: OUTPUT.into(), origin: [40, 0], size: fixture.size.map(|value| value as i32) }],
        &std::collections::BTreeMap::from([(OutputId(1), scale + 0.25)])).unwrap();
    fixture.desktop.server.registered_output_catalog().unwrap();
    fixture.desktop.invalidate_outputs().unwrap();
    retired(fixture, &captured);
    queued_stale(fixture, captured, id);
    fixture.desktop.server.configure_scaled_outputs(&[Monitor { id: OutputId(1), name: OUTPUT.into(), origin: [0, 0], size: fixture.size.map(|value| value as i32) }],
        &std::collections::BTreeMap::from([(OutputId(1), scale)])).unwrap();
    fixture.desktop.server.registered_output_catalog().unwrap();
    fixture.desktop.invalidate_outputs().unwrap();
    fixture.inspect(|_| ());
    action(fixture, DesktopAction::ReapplyOpeningRules);
    let deadline = Instant::now() + Duration::from_secs(30);
    while decision(fixture, id).properties.width != Some(Dimension::Fraction([1, 3])) {
        assert!(Instant::now() < deadline, "A new explicit Reapply could not recover after stale queued requests");
        fixture.tick();
    }
    let mut retained = Vec::new();
    repaint(fixture, window, id, &mut retained);

    eprintln!("Checking stale Reapply after window replacement");
    let captured = request(fixture, id);
    window.role.destroy();
    window.xdg.destroy();
    window.surface.destroy();
    fixture.until(|fixture| fixture.desktop.server.windows().iter().all(|window| window.id != id));
    fixture.inspect(|_| ());
    let (replacement, buffer, replacement_id) = map(fixture);
    assert_ne!(replacement_id, id);
    queued_stale(fixture, captured, replacement_id);
    fixture.desktop.send(Input::Activate(replacement_id)).unwrap();
    fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&replacement.surface));
    action(fixture, DesktopAction::ReapplyOpeningRules);
    let deadline = Instant::now() + Duration::from_secs(30);
    while decision(fixture, replacement_id).properties.width != Some(Dimension::Fraction([1, 2])) {
        assert!(Instant::now() < deadline);
        fixture.tick();
    }
    let keys = fixture.client.key_recipients.len();
    tap(fixture, 45);
    fixture.until(|fixture| fixture.client.key_recipients.len() >= keys + 2);
    assert_eq!(&fixture.client.key_recipients[keys..], &[(Some(replacement.surface.id()), 45, true), (Some(replacement.surface.id()), 45, false)]);
    (replacement, buffer)
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn rendered_cell_rule_order_and_window_overrides_survive_startup_and_reapply_to_the_same_real_wayland_window() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for (size, scale) in [([1000, 720], 1.25)] {
        let profile = directory.path().join(size[0].to_string());
        std::fs::create_dir_all(&profile).unwrap();
        let engine = persisted(&runtime, &profile);
        let cell = provider(engine.clone(), &profile);
        let mut fixture = create(&runtime, &profile, size, scale, cell.clone());
        let (home, work) = fixture.inspect(|canvas| {
            let world = canvas.world_mut();
            let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
            let home = world.get::<native::workspace::Workspaces>(root).unwrap().active;
            assert!(native::workspace::rename(world, root, "Home"));
            canvas.create_workspace(root.to_bits()).unwrap();
            let work = canvas.world_mut().get::<native::workspace::Workspaces>(root).unwrap().active;
            assert!(native::workspace::rename(canvas.world_mut(), root, "Work"));
            for workspace in [home, work] { native::workspace_config::set_physics(canvas.world_mut(), root, workspace, false); }
            canvas.activate_workspace(root.to_bits(), home).unwrap();
            (home, work)
        });
        open(&mut fixture);
        new_rule(&mut fixture, "editor half", "1/2", work, 2);
        let (half, half_rule) = rule(&runtime, &engine, "editor half");
        wait_control(&mut fixture, &half, 2, "Disable");
        assert_eq!(half_rule.properties.width, Some(Dimension::Fraction([1, 2])));
        assert_eq!(half_rule.properties.floating, Some(false));
        assert_eq!(half_rule.properties.focus, Some(false));
        assert_eq!(half_rule.conditions.len(), 2);
        new_rule(&mut fixture, "editor third", "1/3", work, 1);
        let (third, _) = rule(&runtime, &engine, "editor third");
        wait_control(&mut fixture, &third, 2, "Disable");
        control(&mut fixture, &third, 3);
        activate(&mut fixture, "Save rule order");
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let matched = fixture.inspect(|canvas| native::layout::desktop::opening::capture(canvas.world_mut(), &serde_json::json!({"app_id": APP}), None).matched);
            if matched.is_some_and(|matched| matched.uid == third) { break; }
            assert!(Instant::now() < deadline, "The ordinary Save action did not activate the requested rule order");
            fixture.tick();
        }
        let saved_configuration = std::fs::read(profile.join("interface.lingua")).unwrap();
        control(&mut fixture, &half, 3);
        activate(&mut fixture, "Cancel order changes");
        let deadline = Instant::now() + Duration::from_secs(30);
        while !fixture.inspect(|canvas| {
            let world = canvas.world_mut();
            world.query::<&Text>().iter(world).any(|text| text.0.starts_with("1. editor third ·"))
        }) {
            assert!(Instant::now() < deadline, "Cancel did not restore the displayed saved rule order");
            fixture.tick();
        }
        assert_eq!(std::fs::read(profile.join("interface.lingua")).unwrap(), saved_configuration);
        control(&mut fixture, &third, 2);
        let deadline = Instant::now() + Duration::from_secs(30);
        while rule(&runtime, &engine, "editor third").1.enabled {
            fixture.tick();
            assert!(Instant::now() < deadline);
        }
        wait_control(&mut fixture, &third, 2, "Enable");
        control(&mut fixture, &third, 2);
        let deadline = Instant::now() + Duration::from_secs(30);
        while !rule(&runtime, &engine, "editor third").1.enabled {
            fixture.tick();
            assert!(Instant::now() < deadline);
        }
        wait_control(&mut fixture, &third, 2, "Disable");
        close(&mut fixture);
        fixture.desktop.worker.stop().unwrap();
        let old = std::sync::Arc::downgrade(&engine);
        drop(fixture);
        drop(cell);
        drop(engine);
        released(&runtime, old);
        let engine = persisted(&runtime, &profile);
        let cell = provider(engine.clone(), &profile);
        let mut restarted = create(&runtime, &profile, size, scale, cell);
        restarted.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.opening_ready));
        let (window, _buffer, id) = map(&mut restarted);
        let original = restarted.inspect(move |canvas| member(canvas, id));
        assert_eq!(original.1, work);
        let initial = decision(&mut restarted, id);
        assert_eq!(initial.matched.as_ref().unwrap().uid, third);
        assert_eq!(initial.properties.width, Some(Dimension::Fraction([1, 3])));
        assert_eq!(restarted.inspect(|canvas| {
            let world = canvas.world_mut();
            world.query::<&native::workspace::Workspaces>().single(world).unwrap().active
        }), home);
        assert!(restarted.client.keyboard_focus.is_none());
        restarted.desktop.send(Input::Activate(id)).unwrap();
        restarted.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&window.surface));
        let queued_order = request(&mut restarted, id);
        open(&mut restarted);
        control(&mut restarted, &half, 3);
        activate(&mut restarted, "Save rule order");
        close(&mut restarted);
        queued_stale(&mut restarted, queued_order, id);
        assert_eq!(decision(&mut restarted, id), initial);
        restarted.desktop.send(Input::Activate(id)).unwrap();
        restarted.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&window.surface));
        action(&mut restarted, DesktopAction::ReapplyOpeningRules);
        let deadline = Instant::now() + Duration::from_secs(30);
        while decision(&mut restarted, id).matched.as_ref().is_none_or(|matched| matched.uid != half) {
            restarted.tick();
            assert!(Instant::now() < deadline, "The registered Reapply did not use the new saved rule order");
        }
        assert_eq!(decision(&mut restarted, id).properties.width, Some(Dimension::Fraction([1, 2])));
        let mut retained = Vec::new();
        let half_size = repaint(&mut restarted, &window, id, &mut retained);
        action(&mut restarted, DesktopAction::InspectWindowRecord);
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let available = restarted.inspect(|canvas| {
                let world = canvas.world_mut();
                world.query::<&bevy::a11y::AccessibilityNode>().iter(world).any(|node| node.label() == Some("Edit this window's opening overrides"))
            });
            if available { break; }
            assert!(Instant::now() < deadline, "The actual window Record did not expose its ordinary opening controls");
            std::thread::sleep(Duration::from_millis(2));
        }
        activate(&mut restarted, "Edit this window's opening overrides");
        write(&mut restarted, "Initial width (blank inherits)", "1/1");
        click(&mut restarted, "Save opening preferences");
        saved(&mut restarted);
        let overrides = runtime.block_on(store::records::all_extensions(&engine.store.pool, WINDOW_NAMESPACE)).unwrap();
        assert_eq!(overrides.len(), 1);
        let value: Window = serde_json::from_value(overrides.values().next().unwrap().clone()).unwrap();
        assert_eq!(value.properties.width, Some(Dimension::Fraction([1, 1])));
        assert_eq!(decision(&mut restarted, id).properties.width, Some(Dimension::Fraction([1, 2])));
        activate(&mut restarted, "Cancel");
        click(&mut restarted, "Reapply opening preferences to this window");
        let deadline = Instant::now() + Duration::from_secs(30);
        while decision(&mut restarted, id).properties.width != Some(Dimension::Fraction([1, 1])) {
            restarted.tick();
            assert!(Instant::now() < deadline, "The captured window control did not apply its real saved override");
        }
        let full_size = repaint(&mut restarted, &window, id, &mut retained);
        assert!(full_size[0] > half_size[0], "A saved full-width override did not change the real client's constrained half-width configure");
        assert_eq!(restarted.inspect(move |canvas| member(canvas, id)), original);
        let committed = decision(&mut restarted, id);
        restarted.desktop.send(Input::Activate(id)).unwrap();
        restarted.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&window.surface));
        for index in 0..3 {
            window.role.set_title(format!("editor title {index}"));
            restarted.exchange();
            restarted.desktop.send(Input::Activate(id)).unwrap();
            restarted.inspect(|_| ());
            assert_eq!(decision(&mut restarted, id), committed);
            assert_eq!(restarted.inspect(move |canvas| member(canvas, id)), original);
        }
        assert!(restarted.client.window_configurations[&window.role.id()].0[0] >= 150);
        let point = {
            let item = restarted.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().items.iter().find(|item| item.id == ItemId::Application(id)).unwrap();
            let origin = item.mapping.surface.origin();
            let size = item.mapping.surface.size();
            item.mapping.canvas_point([origin[0] + size[0] * 0.5, origin[1] + size[1] * 0.5]).unwrap()
        };
        restarted.desktop.move_pointer(point, 40).unwrap();
        restarted.tick();
        assert_eq!(restarted.desktop.server.capture_pointer_owner(), crate::server::CaptureContentOwner::Window(id));
        let keys = restarted.client.key_recipients.len();
        let buttons = restarted.client.button_recipients.len();
        restarted.desktop.button(272, true, 41).unwrap();
        restarted.desktop.button(272, false, 42).unwrap();
        tap(&mut restarted, 45);
        restarted.until(|fixture| fixture.client.key_recipients.len() >= keys + 2 && fixture.client.button_recipients.len() >= buttons + 2);
        assert_eq!(&restarted.client.key_recipients[keys..], &[(Some(window.surface.id()), 45, true), (Some(window.surface.id()), 45, false)]);
        assert_eq!(&restarted.client.button_recipients[buttons..], &[(Some(window.surface.id()), 272, true), (Some(window.surface.id()), 272, false)]);
        assert_eq!(runtime.block_on(store::records::all_extensions(&engine.store.pool, RULE_NAMESPACE)).unwrap().len(), 2);
        let (_replacement, _replacement_buffer) = races(&mut restarted, &runtime, &engine, &window, id, &half);
        let saved_overrides = runtime.block_on(store::records::all_extensions(&engine.store.pool, WINDOW_NAMESPACE)).unwrap();
        restarted.desktop.worker.stop().unwrap();
        let old = std::sync::Arc::downgrade(&engine);
        drop(restarted);
        drop(engine);
        released(&runtime, old);
        let engine = persisted(&runtime, &profile);
        assert_eq!(runtime.block_on(store::records::all_extensions(&engine.store.pool, WINDOW_NAMESPACE)).unwrap(), saved_overrides);
        let mut reopened = create(&runtime, &profile, size, scale, provider(engine.clone(), &profile));
        reopened.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.opening_ready));
        assert!(reopened.desktop.server.windows().is_empty());
        let (_new_window, _new_buffer, new_id) = map(&mut reopened);
        let new_decision = decision(&mut reopened, new_id);
        assert_eq!(new_decision.matched.as_ref().unwrap().uid, half);
        assert_eq!(new_decision.properties.width, Some(Dimension::Fraction([1, 2])));
        assert_eq!(reopened.inspect(move |canvas| member(canvas, new_id)).1, work);
        assert_eq!(rule(&runtime, &engine, "editor half").1.properties, half_rule.properties);
        assert!(reopened.client.keyboard_focus.is_none());
        reopened.desktop.worker.stop().unwrap();
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn configured_rules_gate_presented_readiness_reject_late_snapshots_and_leave_a_quiet_worker_after_recovery() {
    use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
    use crate::desktop::Ready;
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let environment = ["WAYLAND_DISPLAY", "DISPLAY", "XAUTHORITY", "DBUS_SESSION_BUS_ADDRESS", "XDG_CURRENT_DESKTOP", "XDG_SESSION_DESKTOP"]
        .map(|key| (key, std::env::var_os(key)));
    for timeout in [false, true] {
        let profile = directory.path().join(if timeout { "timed-out" } else { "loaded" });
        std::fs::create_dir_all(&profile).unwrap();
        let engine = persisted(&runtime, &profile);
        let cell = provider(engine.clone(), &profile);
        let size = [1000, 720];
        let mut seed = create(&runtime, &profile, size, 1.25, cell.clone());
        let workspace = seed.inspect(|canvas| {
            let world = canvas.world_mut();
            let (root, spaces) = world.query::<(Entity, &native::workspace::Workspaces)>().single(world).unwrap();
            let workspace = spaces.active;
            assert!(native::workspace::rename(world, root, "Work"));
            native::workspace_config::set_physics(world, root, workspace, false);
            workspace
        });
        open(&mut seed);
        new_rule(&mut seed, "ready rule", "1/2", workspace, 1);
        let (uid, _) = rule(&runtime, &engine, "ready rule");
        close(&mut seed);
        action(&mut seed, DesktopAction::Unfocus);
        seed.desktop.worker.stop().unwrap();
        drop(seed);

        let marker = profile.join("ready-count");
        let command_file = profile.join("park.json");
        std::fs::write(&command_file, serde_json::to_vec(&serde_json::json!({
            "ready": {"program":"dd", "arguments":["if=/dev/zero", format!("of={}", marker.display()), "oflag=append", "conv=notrunc", "bs=1", "count=1", "status=none"]}
        })).unwrap()).unwrap();
        let (outgoing, mut requests) = tokio::sync::mpsc::channel::<cell::ClientMessage>(64);
        let (responses, incoming) = tokio::sync::mpsc::channel(64);
        let (release, receive_release) = tokio::sync::oneshot::channel::<()>();
        let (send_wake, receive_wake) = tokio::sync::oneshot::channel::<WakeSignal>();
        let waiting = Arc::new(AtomicBool::new(false));
        let returned = Arc::new(AtomicBool::new(false));
        let provider_waiting = waiting.clone();
        let provider_returned = returned.clone();
        let provider_cell = cell.clone();
        let provider_task = runtime.spawn(async move {
            let mut session = provider_cell.0.local_session();
            let wake = receive_wake.await.unwrap();
            let mut release = Some(receive_release);
            while let Some(request) = requests.recv().await {
                let rule_query = matches!(&request, cell::ClientMessage::Subscribe { protein, .. }
                    if protein.include.extension.as_ref().is_some_and(|extension| extension.namespace == RULE_NAMESPACE));
                let mutation = matches!(&request, cell::ClientMessage::Act { .. });
                let mut messages = session.handle(request).await;
                let held = rule_query && release.is_some();
                if held {
                    assert!(messages.iter().any(|message| matches!(message, cell::ServerMessage::Snapshot { .. })));
                    provider_waiting.store(true, Ordering::Release);
                    if release.take().unwrap().await.is_err() { break; }
                }
                if mutation { messages.extend(session.refresh().await); }
                for message in messages {
                    if responses.send(message).await.is_err() { return; }
                    wake.ring();
                }
                if held { provider_returned.store(true, Ordering::Release); }
            }
        });
        let worker = CanvasWorker::spawn_with_setup(Configuration { showoff: false, size, directory: profile.clone(), runtime: runtime.handle().clone(), cell: Some(cell.clone()) }, move |canvas| {
            let world = canvas.world_mut();
            send_wake.send(world.resource::<WakeSignal>().clone()).unwrap_or_else(|_| panic!("The real rule provider lost its worker wake handoff"));
            let mut schedules = world.resource_mut::<Schedules>();
            if schedules.get(PostStartup).is_none() { let _ = schedules.insert(bevy::ecs::schedule::Schedule::new(PostStartup)); }
            let mut incoming = Some(incoming);
            schedules.get_mut(PostStartup).unwrap().add_systems(move |mut bridge: NonSendMut<native::cell_bridge::CellBridge>| {
                bridge.outgoing = outgoing.clone();
                bridge.incoming = incoming.take().unwrap();
                bridge.outgoing.try_send(native::cell_bridge::records_subscription()).unwrap();
            });
        }).unwrap();
        let mut harness = Harness::new();
        harness.server.configure_scaled_outputs(&[Monitor { id: OutputId(1), name: "Opening fixture".into(), origin: [0, 0], size: size.map(|value| value as i32) }],
            &std::collections::BTreeMap::from([(OutputId(1), 1.25)])).unwrap();
        harness.sync();
        let mut fixture = Fixture::with_worker(harness, size, worker);
        let socket = profile.join("wayland-ready-fixture");
        fixture.listen(&socket);
        fixture.desktop.commands(command_file, socket);
        fixture.until(|fixture| waiting.load(Ordering::Acquire) && fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.control.context.is_some()));
        assert_eq!(fixture.desktop.ready(), Ready::Waiting);
        assert!(!marker.exists());
        let mut release = Some(release);
        if !timeout { release.take().unwrap().send(()).unwrap(); }
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.opening_ready));
        assert_eq!(fixture.desktop.ready(), Ready::Dispatched);
        fixture.until(|_| std::fs::metadata(&marker).is_ok_and(|metadata| metadata.len() == 1));
        let (window, _buffer, id) = map(&mut fixture);
        if timeout {
            let fallback = decision(&mut fixture, id);
            assert!(fallback.matched.is_none());
            assert!(fallback.warning.as_deref().is_some_and(|warning| warning.contains("timed out")));
            release.take().unwrap().send(()).unwrap();
            fixture.until(|_| returned.load(Ordering::Acquire));
            fixture.inspect(|_| ());
            assert_eq!(decision(&mut fixture, id), fallback);
            open(&mut fixture);
            click(&mut fixture, "Refresh rule Records");
            wait_control(&mut fixture, &uid, 2, "Disable");
            close(&mut fixture);
            assert_eq!(decision(&mut fixture, id), fallback);
            fixture.desktop.send(Input::Activate(id)).unwrap();
            fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&window.surface));
            let deadline = Instant::now() + Duration::from_secs(30);
            while !fixture.inspect(move |canvas| {
                let (entity, _, _) = member(canvas, id);
                let world = canvas.world_mut();
                let Some(binding) = world.get::<native::protein_area::RecordBinding>(entity) else { return false; };
                native::layout::desktop::opening::capture(world, &serde_json::json!({"app_id":APP}), Some(&binding.uid)).warning.is_none()
            }) {
                assert!(Instant::now() < deadline, "The real window override subscription did not recover before explicit Reapply");
            }
            action(&mut fixture, DesktopAction::ReapplyOpeningRules);
            let deadline = Instant::now() + Duration::from_secs(30);
            while decision(&mut fixture, id).matched.as_ref().is_none_or(|matched| matched.uid != uid) {
                assert!(Instant::now() < deadline);
                fixture.tick();
            }
        } else {
            assert_eq!(decision(&mut fixture, id).matched.as_ref().unwrap().uid, uid);
        }
        for _ in 0..20 { assert_eq!(fixture.desktop.ready(), Ready::Dispatched); fixture.tick(); }
        assert_eq!(std::fs::metadata(&marker).unwrap().len(), 1);
        action(&mut fixture, DesktopAction::Unfocus);
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut stable = None;
        loop {
            fixture.tick();
            let (idle, publications) = fixture.desktop.worker.test_activity();
            if idle {
                match stable {
                    Some((previous, since)) if previous == publications && Instant::now().duration_since(since) >= Duration::from_millis(600) => break,
                    Some((previous, _)) if previous == publications => {}
                    _ => stable = Some((publications, Instant::now())),
                }
            } else { stable = None; }
            assert!(Instant::now() < deadline, "Recovered opening rules kept publishing frames or waking the quiet worker");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(fixture.desktop.ready(), Ready::Dispatched);
        assert_eq!(std::fs::metadata(marker).unwrap().len(), 1);
        fixture.desktop.worker.stop().unwrap();
        drop(fixture);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !provider_task.is_finished() {
            assert!(Instant::now() < deadline, "The disposed worker retained its real rule provider connection");
            std::thread::sleep(Duration::from_millis(5));
        }
        runtime.block_on(provider_task).unwrap();
        for (key, value) in &environment { assert_eq!(std::env::var_os(key), *value); }
    }
}
