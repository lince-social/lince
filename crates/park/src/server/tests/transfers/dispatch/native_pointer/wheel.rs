use super::*;
use smithay::{backend::input::Axis, input::pointer::AxisFrame};

#[derive(Resource, Default)]
struct Wheels(usize);

#[derive(Component)]
struct Observed;

#[derive(Clone, Copy, Debug)]
enum Retire { Same, Action, Modal, Lock }

fn watch(fixture: &mut Fixture) -> (Entity, [f64; 2]) {
    let (entity, point, _) = target(fixture);
    fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        world.init_resource::<Wheels>();
        if world.get::<Observed>(entity).is_none() {
            world.entity_mut(entity).insert(Observed).observe(|_: On<bevy::picking::events::Pointer<bevy::picking::events::Scroll>>, mut wheels: ResMut<Wheels>| wheels.0 += 1);
        }
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
            let field = magnification::focus(&mut fixture, "Display magnification factor (1–4)");
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
