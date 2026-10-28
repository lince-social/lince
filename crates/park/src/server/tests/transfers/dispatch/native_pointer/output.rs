use super::super::*;
use bevy::text::EditableText;
use smithay::{backend::input::Axis, input::pointer::AxisFrame};

fn note(world: &mut World, root: Entity, workspace: u64, text: &'static str) -> ([u8; 16], Entity, u64) {
    let note = native::sand_store::spawn_sand(world, root, workspace, native::sand_store::SandKind::EditableText, "Native handoff editor", bevy::math::DVec2::ZERO);
    let field = world.get::<native::sand_store::StoredSand>(note).unwrap().content.unwrap();
    world.get_mut::<EditableText>(field).unwrap().editor.set_text(text);
    if world.get::<native::layout::LayoutBox>(note).is_none() {
        let size = world.get::<native::canvas::CanvasItem>(note).unwrap().size;
        world.entity_mut(note).insert(native::layout::LayoutBox::new(size));
    }
    (world.get::<native::layout::LayoutBox>(note).unwrap().id, note, field.to_bits())
}

fn point(fixture: &mut Fixture, id: [u8; 16]) -> [f64; 2] {
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|current| current.frame.geometry.as_ref()
        .is_some_and(|geometry| geometry.items.iter().any(|item| item.id == ItemId::Native(id) && item.output == OutputId(2)))));
    let frame = &fixture.desktop.current.as_ref().unwrap().frame;
    let item = frame.geometry.as_ref().unwrap().items.iter().find(|item| item.id == ItemId::Native(id)).unwrap();
    let source = std::array::from_fn(|axis| item.mapping.surface.origin()[axis] + item.mapping.surface.size()[axis] * 0.5);
    let point = item.mapping.canvas_point(source).unwrap();
    assert!(frame.input.native_pointer.target(point).is_some());
    point
}

fn click(fixture: &mut Fixture, point: [f64; 2]) {
    fixture.desktop.move_pointer(point, 20).unwrap();
    fixture.tick();
    fixture.desktop.button(272, true, 21).unwrap();
    fixture.desktop.button(272, false, 22).unwrap();
}

#[derive(Resource, Default)]
struct Scrolls(std::collections::BTreeMap<u64, usize>);

fn observe_scroll(world: &mut World, field: u64) {
    world.init_resource::<Scrolls>();
    world.entity_mut(Entity::from_bits(field)).observe(move |_: On<bevy::picking::events::Pointer<bevy::picking::events::Scroll>>, mut received: ResMut<Scrolls>| {
        *received.0.entry(field).or_default() += 1;
    });
}

fn field_point(fixture: &mut Fixture, field: u64) -> [f64; 2] {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some((output, points)) = fixture.inspect(move |canvas| canvas.native_test_points(Entity::from_bits(field))) {
            let point = std::array::from_fn(|axis| points.iter().map(|point| point[axis]).sum::<f64>() * 0.25);
            if output == OutputId(2) && fixture.desktop.current.as_ref().unwrap().frame.input.native_pointer.target(point).is_some() { return point; }
        }
        assert!(Instant::now() < deadline, "The actual companion editor did not publish an eligible field target");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[derive(Clone, Copy, Debug)]
enum ScrollRetire { Same, Replace, Modal, Output, Lock }

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn real_output_handoff_keeps_deferred_wheel_with_its_presented_editor_and_retires_replacement_modal_output_and_lock_before_fresh_input() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for retirement in [ScrollRetire::Same, ScrollRetire::Replace, ScrollRetire::Modal, ScrollRetire::Output, ScrollRetire::Lock] {
        let path = directory.path().join(format!("wheel-{retirement:?}")); std::fs::create_dir(&path).unwrap();
        let monitors = [Monitor { id: OutputId(1), name: "Native-A".into(), origin: [0, 0], size: [500, 720] },
            Monitor { id: OutputId(2), name: "Native-B".into(), origin: [500, 0], size: [500, 720] }];
        let mut harness = Harness::new();
        harness.server.configure_outputs(&monitors).unwrap(); harness.sync();
        let client = harness.window();
        let _buffer = paint(&mut harness, &client, [320, 240], [40, 140, 220, 255]);
        let mut fixture = Fixture::new(harness, &runtime, &path);
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry| geometry.outputs.len() == 2)));
        let workspace = fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().outputs.iter().find(|output| output.id == OutputId(2)).unwrap().workspace;
        let (root, original_id, original_note, original_field) = fixture.inspect(move |canvas| {
            let world = canvas.world_mut();
            let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
            let workspaces = world.get::<native::workspace::Workspaces>(root).unwrap().entries.iter().map(|workspace| workspace.id).collect::<Vec<_>>();
            for workspace in workspaces { native::workspace_config::set_physics(world, root, workspace, false); }
            let (id, entity, field) = note(world, root, workspace, "Deferred original editor");
            observe_scroll(world, field);
            (root, id, entity, field)
        });
        let serial = fixture.desktop.worker.input(Input::Output(OutputId(1))).unwrap();
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.input_serial >= serial && frame.frame.output == Some(OutputId(1))));
        let destination = field_point(&mut fixture, original_field);
        fixture.desktop.move_pointer(destination, 30).unwrap(); fixture.tick();
        let (_, release) = fixture.pause();
        let (replacement, received) = std::sync::mpsc::sync_channel(1);
        match retirement {
            ScrollRetire::Replace => {
                fixture.desktop.worker.input(Input::Test(Box::new(move |canvas| {
                    let world = canvas.world_mut(); assert!(world.despawn(original_note));
                    let next = note(world, root, workspace, "Deferred replacement editor"); observe_scroll(world, next.2); replacement.send(next).unwrap();
                }))).unwrap();
            }
            ScrollRetire::Modal => { fixture.desktop.worker.input(Input::Action(DesktopAction::OpenOperation)).unwrap(); }
            ScrollRetire::Same | ScrollRetire::Output | ScrollRetire::Lock => {}
        }
        fixture.desktop.button(272, true, 31).unwrap(); fixture.desktop.button(272, false, 32).unwrap();
        assert!(fixture.desktop.pending_output.is_some());
        fixture.desktop.axis([0.0, 48.0], AxisFrame::new(33).value(Axis::Vertical, 48.0)).unwrap();
        assert!(fixture.desktop.deferred.iter().any(|event| matches!(event, crate::desktop::input::Deferred::Axis(..))));
        let mut lock = None;
        if matches!(retirement, ScrollRetire::Output) {
            fixture.desktop.server.configure_outputs(&monitors[..1]).unwrap(); fixture.desktop.server.registered_output_catalog().unwrap();
            fixture.desktop.invalidate_outputs().unwrap(); fixture.desktop.dispatch().unwrap();
        }
        if matches!(retirement, ScrollRetire::Lock) {
            lock = Some(fixture.client.lock_manager.as_ref().unwrap().lock(&fixture.queue.handle(), ()));
            fixture.exchange(); fixture.desktop.dispatch().unwrap(); assert!(fixture.desktop.server.locked());
        }
        let mut filled = 0;
        let mut serial = 0;
        loop {
            match fixture.desktop.worker.input(Input::Pointer([0.0; 2])) {
                Ok(next) => { serial = next; filled += 1; assert!(filled <= 128); }
                Err(std::sync::mpsc::TrySendError::Full(_)) => break,
                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => panic!("The deferred wheel worker disconnected before its FIFO filled"),
            }
        }
        assert!(filled > 0); release.send(()).unwrap();
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.input_serial >= serial)
            && fixture.desktop.pending_output.is_none() && fixture.desktop.deferred.is_empty());
        assert_eq!(fixture.inspect(move |canvas| canvas.world_mut().resource::<Scrolls>().0.get(&original_field).copied().unwrap_or(0)), usize::from(matches!(retirement, ScrollRetire::Same)));
        let (id, field) = if matches!(retirement, ScrollRetire::Replace) {
            let (id, _, field) = received.recv_timeout(Duration::from_secs(1)).unwrap();
            assert_ne!(field, original_field);
            assert_eq!(fixture.inspect(move |canvas| canvas.world_mut().resource::<Scrolls>().0.get(&field).copied().unwrap_or(0)), 0);
            assert_eq!(fixture.inspect(move |canvas| canvas.world_mut().get::<EditableText>(Entity::from_bits(field)).unwrap().value().to_string()), "Deferred replacement editor");
            (id, field)
        } else { (original_id, original_field) };
        assert!(fixture.desktop.buttons.is_empty());
        if matches!(retirement, ScrollRetire::Lock) {
            assert!(lock.as_ref().unwrap().is_alive()); assert!(fixture.client.keyboard_focus.is_none()); fixture.desktop.worker.stop().unwrap(); continue;
        }
        if matches!(retirement, ScrollRetire::Modal) {
            let operation = super::super::compact::named(&mut fixture, "operation.input");
            assert!(fixture.inspect(move |canvas| canvas.world_mut().get_resource::<bevy::input_focus::InputFocus>().unwrap().get() == Some(operation)));
            fixture.desktop.key(1, true, 40).unwrap(); fixture.desktop.key(1, false, 41).unwrap(); fixture.inspect(|_| ());
        }
        if matches!(retirement, ScrollRetire::Output) {
            fixture.desktop.server.configure_outputs(&monitors).unwrap(); fixture.desktop.server.registered_output_catalog().unwrap(); fixture.desktop.invalidate_outputs().unwrap();
            fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry| geometry.outputs.len() == 2)));
        }
        let serial = fixture.desktop.worker.input(Input::Output(OutputId(2))).unwrap();
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.input_serial >= serial && frame.frame.output == Some(OutputId(2))));
        let point = field_point(&mut fixture, field);
        click(&mut fixture, point);
        let before = fixture.inspect(move |canvas| canvas.world_mut().resource::<Scrolls>().0.get(&field).copied().unwrap_or(0));
        fixture.desktop.axis([0.0, 48.0], AxisFrame::new(50).value(Axis::Vertical, 48.0)).unwrap();
        assert_eq!(fixture.inspect(move |canvas| canvas.world_mut().resource::<Scrolls>().0.get(&field).copied().unwrap_or(0)), before + 1);
        fixture.desktop.key(45, true, 51).unwrap(); fixture.desktop.key(45, false, 52).unwrap();
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.editor.as_ref().is_some_and(|editor| editor.owner == field && editor.surrounding.contains('x')))
            && fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().is_some_and(|geometry| geometry.items.iter().any(|item| item.id == ItemId::Native(id))));
        fixture.desktop.worker.stop().unwrap();
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn actual_output_handoff_replays_a_valid_native_click_and_refuses_a_replacement_editor() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for replace in [false, true] {
        let profile = directory.path().join(replace.to_string()); std::fs::create_dir(&profile).unwrap();
        let mut harness = Harness::new();
        harness.server.configure_outputs(&[
            Monitor { id: OutputId(1), name: "Native-A".into(), origin: [0, 0], size: [500, 720] },
            Monitor { id: OutputId(2), name: "Native-B".into(), origin: [500, 0], size: [500, 720] },
        ]).unwrap();
        harness.sync();
        let application = harness.window();
        let _buffer = paint(&mut harness, &application, [320, 240], [40, 140, 220, 255]);
        let mut fixture = Fixture::new(harness, &runtime, &profile);
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry| geometry.outputs.len() == 2)));
        let workspace = fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().outputs.iter().find(|output| output.id == OutputId(2)).unwrap().workspace;
        let (root, id, old_note, old_field) = fixture.inspect(move |canvas| {
            let world = canvas.world_mut();
            let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
            let workspaces = world.get::<native::workspace::Workspaces>(root).unwrap().entries.iter().map(|workspace| workspace.id).collect::<Vec<_>>();
            for workspace in workspaces { native::workspace_config::set_physics(world, root, workspace, false); }
            let (id, note, field) = note(world, root, workspace, "Original editor");
            (root, id, note, field)
        });
        let serial = fixture.desktop.worker.input(Input::Output(OutputId(1))).unwrap();
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.input_serial >= serial && frame.frame.output == Some(OutputId(1))));
        let destination = point(&mut fixture, id);
        fixture.desktop.move_pointer(destination, 23).unwrap();
        fixture.tick();
        let (_, release) = fixture.pause();
        let (replacement, observed) = std::sync::mpsc::sync_channel(1);
        if replace {
            fixture.desktop.worker.input(Input::Test(Box::new(move |canvas| {
                let world = canvas.world_mut();
                assert!(world.despawn(old_note));
                replacement.send(note(world, root, workspace, "Replacement editor")).unwrap();
            }))).unwrap();
        }
        fixture.desktop.button(272, true, 24).unwrap();
        fixture.desktop.button(272, false, 25).unwrap();
        assert!(fixture.desktop.pending_output.is_some());
        release.send(()).unwrap();
        fixture.until(|fixture| fixture.desktop.pending_output.is_none() && fixture.desktop.deferred.is_empty()
            && fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.output == Some(OutputId(2))));
        let expected = if replace {
            let (replacement_id, _, field) = observed.recv_timeout(Duration::from_secs(1)).unwrap();
            assert_ne!(field, old_field);
            assert!(fixture.desktop.current.as_ref().unwrap().frame.input.editor.as_ref().is_none_or(|editor| editor.owner != field));
            assert_eq!(fixture.inspect(move |canvas| canvas.world_mut().get::<EditableText>(Entity::from_bits(field)).unwrap().value().to_string()), "Replacement editor");
            let fresh = point(&mut fixture, replacement_id);
            click(&mut fixture, fresh);
            field
        } else { old_field };
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.editor.as_ref().is_some_and(|editor| editor.owner == expected)));
        fixture.desktop.key(45, true, 26).unwrap();
        fixture.desktop.key(45, false, 27).unwrap();
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.editor.as_ref().is_some_and(|editor| editor.owner == expected && editor.surrounding.contains('x'))));
        assert!(fixture.desktop.buttons.is_empty());
        assert!(fixture.desktop.running);
        fixture.desktop.worker.stop().unwrap();
    }
}
