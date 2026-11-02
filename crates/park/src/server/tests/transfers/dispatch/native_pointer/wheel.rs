use super::*;
use smithay::{backend::input::Axis, input::pointer::AxisFrame};

#[derive(Resource, Default)]
struct Wheels(usize);

#[derive(Resource, Default)]
struct WheelDistance(Vec2);

#[derive(Component)]
struct Observed;

#[derive(Clone, Copy, Debug)]
enum Retire { Same, Action, Modal, Lock }

fn watch(fixture: &mut Fixture) -> (Entity, [f64; 2]) {
    let entity = fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        world.query::<(Entity, &bevy::a11y::AccessibilityNode)>().iter(world)
            .find(|(entity, node)| node.label() == Some("Increase draft by 0.25×")
                && world.get::<InheritedVisibility>(*entity).is_some_and(|visible| visible.get()))
            .map(|(entity, _)| entity).unwrap()
    });
    let point = super::super::opening::point(fixture, entity).unwrap_or_else(|| {
        magnification::focus_entity(fixture, entity, "Increase draft by 0.25×");
        super::super::opening::point(fixture, entity)
            .expect("The wheel fixture must point at its observed control")
    });
    fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        if !world.contains_resource::<Wheels>() {
            world.init_resource::<Wheels>();
            world.init_resource::<WheelDistance>();
            world.add_observer(|event: On<bevy::picking::events::Pointer<bevy::picking::events::Scroll>>,
                parents: Query<&ChildOf>, observed: Query<(), With<Observed>>,
                mut wheels: ResMut<Wheels>, mut distance: ResMut<WheelDistance>| {
                if event.entity != event.original_event_target() { return; }
                let mut current = Some(event.entity);
                while let Some(entity) = current {
                    if observed.contains(entity) {
                        wheels.0 += 1;
                        distance.0 += Vec2::new(event.x, event.y);
                        break;
                    }
                    current = parents.get(entity).ok().map(ChildOf::parent);
                }
            });
        }
        world.entity_mut(entity).insert(Observed);
    });
    (entity, point)
}

fn wheel(fixture: &mut Fixture, time: u32) {
    fixture.desktop.axis([0.0, 48.0], AxisFrame::new(time).value(Axis::Vertical, 48.0)).unwrap();
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn ordinary_native_wheel_uses_the_presented_recipient_under_real_queue_pressure_and_retires_changed_action_modal_and_lock_without_resetting_valid_ime() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for retirement in [Retire::Same, Retire::Action, Retire::Modal, Retire::Lock] {
        let path = directory.path().join(format!("{retirement:?}")); std::fs::create_dir(&path).unwrap();
        let mut fixture = magnification::create(&runtime, &path, [1000, 720], 1.25);
        magnification::open(&mut fixture);
        let (entity, point) = watch(&mut fixture);
        let mut ime = if matches!(retirement, Retire::Same) {
            let field = fixture.inspect(|canvas| {
                let world = canvas.world_mut();
                world.query::<(Entity, &bevy::a11y::AccessibilityNode)>().iter(world)
                    .find(|(_, node)| node.label() == Some("Display magnification factor (1–4)"))
                    .map(|(entity, _)| entity).unwrap()
            });
            super::super::opening::click_entity(&mut fixture, field, "Display magnification factor (1–4)");
            let mut ime = super::super::ime::Ime::connect(&mut fixture);
            ime.until(&mut fixture, |fixture, ime| ime.active() && fixture.desktop.current.as_ref().is_some_and(|frame|
                frame.frame.input.editor.as_ref().is_some_and(|editor| editor.owner == field.to_bits())));
            ime.method.set_preedit_string("界".into(), 0, 3); ime.method.commit(ime.serial());
            ime.until(&mut fixture, |fixture, _| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.text_composing));
            Some((ime, field))
        } else { None };
        let ime_serial = ime.as_ref().map(|(ime, _)| ime.serial());
        fixture.desktop.move_pointer(point, 20).unwrap(); fixture.tick();
        let (_, release) = fixture.pause();
        match retirement {
            Retire::Action => {
                fixture.desktop.worker.input(Input::Test(Box::new(move |canvas| {
                    canvas.world_mut().init_resource::<Clicks>();
                    canvas.world_mut().entity_mut(entity).insert(native::actions::ActionButton::new(entity, native::actions::ActionSequence::default().then(Replacement)));
                }))).unwrap();
            }
            Retire::Modal => {
                fixture.desktop.worker.input(Input::Action(DesktopAction::ToggleEdit)).unwrap();
                fixture.desktop.worker.input(Input::Action(DesktopAction::OpenOperation)).unwrap();
            }
            Retire::Same | Retire::Lock => {}
        }
        wheel(&mut fixture, 21);
        let mut lock = None;
        if matches!(retirement, Retire::Lock) {
            lock = Some(fixture.client.lock_manager.as_ref().unwrap().lock(&fixture.queue.handle(), ()));
            fixture.exchange(); fixture.desktop.dispatch().unwrap();
            assert!(fixture.desktop.server.locked());
        }
        let mut serial = 0;
        let mut filled = 0;
        loop {
            match fixture.desktop.worker.input(Input::Pointer([0.0; 2])) {
                Ok(next) => { serial = next; filled += 1; assert!(filled <= 128); }
                Err(std::sync::mpsc::TrySendError::Full(_)) => break,
                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => panic!("The native wheel worker disconnected before its FIFO filled"),
            }
        }
        assert!(filled > 0);
        release.send(()).unwrap();
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.input_serial >= serial));
        assert_eq!(fixture.inspect(|canvas| canvas.world_mut().resource::<Wheels>().0), usize::from(matches!(retirement, Retire::Same)));
        if let Some((mut ime, field)) = ime.take() {
            ime.exchange(&mut fixture);
            assert!(ime.active());
            assert_eq!(Some(ime.serial()), ime_serial);
            assert!(fixture.desktop.current.as_ref().unwrap().frame.input.text_composing);
            assert_eq!(fixture.inspect(move |canvas| canvas.world_mut().get::<bevy::text::EditableText>(field).unwrap().value().to_string()), "1");
            ime.destroy(&mut fixture);
        }
        if matches!(retirement, Retire::Lock) {
            assert!(lock.as_ref().unwrap().is_alive());
            assert!(fixture.client.keyboard_focus.is_none());
            fixture.desktop.worker.stop().unwrap();
            continue;
        }
        if matches!(retirement, Retire::Modal) {
            let operation = super::super::compact::named(&mut fixture, "operation.input");
            assert!(fixture.inspect(move |canvas| canvas.world_mut().get_resource::<bevy::input_focus::InputFocus>().unwrap().get() == Some(operation)));
            fixture.desktop.key(1, true, 30).unwrap(); fixture.desktop.key(1, false, 31).unwrap();
            fixture.inspect(|_| ());
            magnification::open(&mut fixture);
        }
        let (_, point) = watch(&mut fixture);
        let before = fixture.inspect(|canvas| canvas.world_mut().resource::<Wheels>().0);
        fixture.desktop.move_pointer(point, 40).unwrap(); fixture.tick();
        wheel(&mut fixture, 41);
        assert_eq!(fixture.inspect(|canvas| canvas.world_mut().resource::<Wheels>().0), before + 1);
        assert!(fixture.desktop.buttons.is_empty());
        fixture.desktop.worker.stop().unwrap();
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn native_scroll_bursts_survive_a_full_worker_queue_and_preserve_the_captured_recipient() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut fixture = magnification::create(&runtime, directory.path(), [1000, 720], 1.25);
    magnification::open(&mut fixture);
    let (_, point) = watch(&mut fixture);
    fixture.desktop.move_pointer(point, 20).unwrap();
    fixture.until(|fixture| !fixture.desktop.input_pending());
    assert!(matches!(fixture.desktop.target(), crate::desktop::input::Target::Native));
    assert!(fixture.desktop.frame_at(fixture.desktop.pointer).unwrap().frame.input.native_pointer.target(fixture.desktop.pointer).is_some());
    let position = point.map(|value| value as f32);
    let (_, release) = fixture.pause();
    let mut filled = 0;
    while fixture.desktop.worker.input(Input::Pointer(position)).is_ok() {
        filled += 1;
        assert!(filled <= 128);
    }
    assert_eq!(filled, 128);
    for time in 21..1021 { wheel(&mut fixture, time); }
    assert!(!fixture.desktop.scroll_saturated);
    assert!(fixture.desktop.worker.is_running());
    release.send(()).unwrap();
    let (count, distance) = fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        (world.resource::<Wheels>().0, world.resource::<WheelDistance>().0)
    });
    assert_eq!(count, 1);
    assert_eq!(distance, Vec2::new(0.0, -48000.0));
    fixture.desktop.worker.stop().unwrap();
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn incompatible_scroll_overflow_keeps_the_session_and_control_queue_alive() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut fixture = magnification::create(&runtime, directory.path(), [1000, 720], 1.25);
    magnification::open(&mut fixture);
    let (_, point) = watch(&mut fixture);
    fixture.desktop.move_pointer(point, 20).unwrap();
    fixture.until(|fixture| !fixture.desktop.input_pending());
    let (_, release) = fixture.pause();
    for time in 21..1021 {
        let delta = if time % 2 == 0 { 48.0 } else { -48.0 };
        fixture.desktop.axis([0.0, delta], AxisFrame::new(time).value(Axis::Vertical, delta)).unwrap();
    }
    assert!(fixture.desktop.scroll_saturated);
    assert!(fixture.desktop.worker.is_running());
    let serial = fixture.desktop.worker.input(Input::CancelKeys).unwrap();
    release.send(()).unwrap();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.input_serial >= serial));
    assert!(fixture.desktop.worker.is_running());
    fixture.desktop.worker.stop().unwrap();
}
