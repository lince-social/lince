use super::*;
use bevy::input::mouse::MouseButton;

mod output;
mod wheel;
#[cfg(feature = "session")]
mod absolute;

#[derive(Resource, Default)]
struct Clicks(usize);

#[derive(Clone)]
struct Replacement;

impl native::actions::Action for Replacement {
    fn apply(&self, world: &mut World, _: Entity) { world.resource_mut::<Clicks>().0 += 1; }
}

fn target(fixture: &mut Fixture) -> (Entity, [f64; 2], crate::native_input::PointerTarget) {
    let entity = magnification::focus(fixture, "Increase draft by 0.25×");
    let point = fixture.inspect(move |canvas| canvas.world_mut().get::<UiGlobalTransform>(entity).unwrap().transform_point2(Vec2::ZERO).as_dvec2().to_array());
    let frame = &fixture.desktop.current.as_ref().unwrap().frame;
    let mapping = frame.input.mappings.get(frame.output.unwrap()).unwrap();
    let point = mapping.raster().logical_point(point);
    let target = frame.input.native_pointer.target(point).expect("A rendered button needs its actual captured input recipient");
    (entity, point, target)
}

fn queue_press(fixture: &Fixture, point: [f64; 2], target: crate::native_input::PointerTarget) -> u64 {
    let position = point.map(|value| value as f32);
    fixture.desktop.worker.input(Input::CapturedButton(MouseButton::Left, true, position, 0, target)).unwrap();
    fixture.desktop.worker.input(Input::Button(MouseButton::Left, false, position, 0)).unwrap()
}

fn fill(fixture: &Fixture, mut serial: u64, mut queued: usize) -> u64 {
    while let Ok(next) = fixture.desktop.worker.input(Input::Pointer([0.0; 2])) { queued += 1; serial = next; assert!(queued <= 128); }
    assert_eq!(queued, 128);
    serial
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn captured_native_press_survives_queue_pressure_but_cannot_activate_a_replaced_action() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut fixture = magnification::create(&runtime, directory.path(), [1000, 720], 1.25);
    magnification::open(&mut fixture);
    let (_, point, captured) = target(&mut fixture);
    let (_, release) = fixture.pause();
    let serial = fill(&fixture, queue_press(&fixture, point, captured), 2);
    release.send(()).unwrap();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.input_serial >= serial));
    assert_eq!(magnification::value(&mut fixture), "1.25");
    let (entity, point, captured) = target(&mut fixture);
    let (_, release) = fixture.pause();
    fixture.desktop.worker.input(Input::Test(Box::new(move |canvas| {
        let world = canvas.world_mut();
        world.init_resource::<Clicks>();
        world.entity_mut(entity).insert(native::actions::ActionButton::new(entity, native::actions::ActionSequence::default().then(Replacement)));
    }))).unwrap();
    let serial = fill(&fixture, queue_press(&fixture, point, captured), 3);
    release.send(()).unwrap();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.input_serial >= serial));
    assert_eq!(fixture.inspect(|canvas| canvas.world_mut().resource::<Clicks>().0), 0);
    assert_eq!(magnification::value(&mut fixture), "1.25");
    fixture.desktop.move_pointer(point, 10).unwrap();
    fixture.tick();
    fixture.desktop.button(272, true, 11).unwrap();
    fixture.tick();
    fixture.desktop.button(272, false, 12).unwrap();
    fixture.until(|fixture| fixture.desktop.buttons.is_empty());
    assert_eq!(fixture.inspect(|canvas| canvas.world_mut().resource::<Clicks>().0), 1);
    fixture.desktop.button(272, true, 13).unwrap();
    fixture.tick();
    assert!(fixture.inspect(|canvas| canvas.world_mut().resource::<ButtonInput<MouseButton>>().pressed(MouseButton::Left)));
    let (_, release) = fixture.pause();
    fixture.desktop.worker.input(Input::Test(Box::new(move |canvas| {
        let world = canvas.world_mut();
        let action = world.get::<native::actions::ActionButton>(entity).unwrap().clone();
        world.entity_mut(entity).insert(action);
    }))).unwrap();
    fixture.desktop.button(272, false, 14).unwrap();
    release.send(()).unwrap();
    assert_eq!(fixture.inspect(|canvas| canvas.world_mut().resource::<Clicks>().0), 1);
    assert!(!fixture.inspect(|canvas| canvas.world_mut().resource::<ButtonInput<MouseButton>>().pressed(MouseButton::Left)));
    assert!(fixture.desktop.buttons.is_empty());
    fixture.desktop.button(272, true, 15).unwrap();
    fixture.tick();
    fixture.desktop.button(272, false, 16).unwrap();
    assert_eq!(fixture.inspect(|canvas| canvas.world_mut().resource::<Clicks>().0), 2);
    fixture.desktop.worker.stop().unwrap();
}
