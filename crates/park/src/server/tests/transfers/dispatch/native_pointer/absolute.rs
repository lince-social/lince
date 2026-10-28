use super::*;
use lince_interface::host::input::{AbsoluteDevice, AbsolutePreference, AbsoluteSnapshot, TabletAxes, TabletKind, TabletPhase};
use smithay::{backend::input::{TabletToolCapabilities, TabletToolDescriptor, TabletToolType}, wayland::tablet_manager::TabletDescriptor};

const FIELD: &str = "Display magnification factor (1–4)";

mod sequence;
mod settings;

#[derive(Clone, Copy)]
enum Device { Touch, Tip, MappedButton }

fn device() -> AbsoluteDevice {
    AbsoluteDevice { id: "fixture-absolute".into(), key: Some("fixture-absolute".into()), generation: 41,
        name: "Absolute input fixture".into(), touch: true, tablet: true, calibration: false,
        default_calibration: None, effective_calibration: None, associated_output: None, buttons: vec![331], error: None }
}

fn prepare(fixture: &mut Fixture) {
    let device = device();
    fixture.desktop.absolute_settings(AbsoluteSnapshot { available: true, revision: 1,
        settings: std::collections::BTreeMap::from([("fixture-absolute".into(), AbsolutePreference {
            buttons: std::collections::BTreeMap::from([(331, 272)]), ..Default::default() })]),
        devices: vec![device.clone()], ..Default::default() }).unwrap();
    assert!(fixture.desktop.server.tablet_device(device.generation, TabletDescriptor { name: device.name, usb_id: None, syspath: None }));
    fixture.inspect(|_| ());
}

fn normalized(fixture: &mut Fixture, entity: Entity) -> [f64; 2] {
    let point = fixture.inspect(move |canvas| canvas.world_mut().get::<UiGlobalTransform>(entity).unwrap().transform_point2(Vec2::ZERO).as_dvec2().to_array());
    let frame = &fixture.desktop.current.as_ref().unwrap().frame;
    let mapping = frame.input.mappings.get(frame.output.unwrap()).unwrap();
    let point = mapping.raster().logical_point(point);
    assert!(frame.input.native_pointer.target(point).is_some());
    let physical = mapping.logical_to_physical(point).unwrap();
    std::array::from_fn(|axis| physical[axis] / f64::from(mapping.physical_size()[axis]))
}

fn report(fixture: &mut Fixture, point: [f64; 2], phase: TabletPhase) {
    assert_eq!(fixture.desktop.tablet_event(crate::session::TabletReport { id: 81, device: device(),
        descriptor: TabletToolDescriptor { tool_type: TabletToolType::Pen, hardware_serial: 0, hardware_id_wacom: 0,
            capabilities: TabletToolCapabilities::PRESSURE | TabletToolCapabilities::TILT },
        axes: TabletAxes { pressure: Some(0.5), tilt: Some([-12.0, 17.0]), ..Default::default() },
        kind: TabletKind::Pen, phase, normalized: point, time: 100,
        proximity: phase != TabletPhase::Leave, held: matches!(phase, TabletPhase::Tip(true) | TabletPhase::Button { pressed: true, .. }) }), None);
}

fn down(fixture: &mut Fixture, kind: Device, point: [f64; 2]) {
    match kind {
        Device::Touch => fixture.desktop.touch_down(&device(), 0, point, 101),
        Device::Tip | Device::MappedButton => {
            report(fixture, point, TabletPhase::Enter);
            fixture.desktop.flush_touch().unwrap();
            report(fixture, point, if matches!(kind, Device::Tip) { TabletPhase::Tip(true) } else { TabletPhase::Button { code: 331, pressed: true } });
        }
    }
    fixture.desktop.flush_touch().unwrap();
}

fn up(fixture: &mut Fixture, kind: Device, point: [f64; 2]) {
    match kind {
        Device::Touch => fixture.desktop.touch_up(&device(), 0, 102),
        Device::Tip | Device::MappedButton => {
            report(fixture, point, if matches!(kind, Device::Tip) { TabletPhase::Tip(false) } else { TabletPhase::Button { code: 331, pressed: false } });
            report(fixture, point, TabletPhase::Leave);
        }
    }
}

fn write(fixture: &mut Fixture, digit: u32) {
    for (code, pressed) in [(29, true), (30, true), (30, false), (29, false), (digit, true), (digit, false)] {
        fixture.desktop.key(code, pressed, 103).unwrap();
    }
}

struct Resume(Option<std::sync::mpsc::SyncSender<()>>);

impl Drop for Resume {
    fn drop(&mut self) { if let Some(sender) = self.0.take() { let _ = sender.try_send(()); } }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn touch_pen_and_mapped_tool_press_then_typing_keep_the_real_editor_under_worker_pressure() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut fixture = magnification::create(&runtime, directory.path(), [1000, 720], 1.25);
    magnification::open(&mut fixture);
    prepare(&mut fixture);
    for kind in [Device::Touch, Device::Tip, Device::MappedButton] {
        for (pressure, digit, expected) in [(false, 3, "2"), (true, 4, "3")] {
            let field = magnification::focus(&mut fixture, FIELD);
            magnification::focus(&mut fixture, "Increase draft by 0.25×");
            let point = normalized(&mut fixture, field);
            let mut resume = if pressure {
                let (_, release) = fixture.pause();
                fill(&fixture, 0, 0);
                Some(Resume(Some(release)))
            } else { None };
            down(&mut fixture, kind, point);
            write(&mut fixture, digit);
            up(&mut fixture, kind, point);
            assert!(fixture.desktop.input_pending());
            assert!(fixture.desktop.keys.is_empty());
            if let Some(release) = resume.as_mut().and_then(|resume| resume.0.take()) { release.send(()).unwrap(); }
            fixture.until(|fixture| !fixture.desktop.input_pending());
            assert_eq!(magnification::value(&mut fixture), expected);
            assert!(fixture.desktop.keys.is_empty());
            assert!(!fixture.inspect(|canvas| canvas.world_mut().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::ControlLeft)));
        }
    }
    fixture.desktop.worker.stop().unwrap();
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn rejected_absolute_press_cannot_type_or_fire_a_replacement_and_clears_an_old_modifier() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut fixture = magnification::create(&runtime, directory.path(), [1000, 720], 1.25);
    magnification::open(&mut fixture);
    prepare(&mut fixture);
    let (entity, _, _) = target(&mut fixture);
    let point = normalized(&mut fixture, entity);
    fixture.desktop.key(29, true, 1).unwrap();
    fixture.inspect(|_| ());
    let (_, release) = fixture.pause();
    let mut resume = Resume(Some(release));
    let serial = fixture.desktop.worker.input(Input::Test(Box::new(move |canvas| {
        let world = canvas.world_mut();
        world.init_resource::<Clicks>();
        world.entity_mut(entity).insert(native::actions::ActionButton::new(entity, native::actions::ActionSequence::default().then(Replacement)));
    }))).unwrap();
    fill(&fixture, serial, 1);
    down(&mut fixture, Device::Touch, point);
    write(&mut fixture, 3);
    up(&mut fixture, Device::Touch, point);
    resume.0.take().unwrap().send(()).unwrap();
    fixture.until(|fixture| !fixture.desktop.input_pending());
    assert_eq!(fixture.inspect(|canvas| canvas.world_mut().resource::<Clicks>().0), 0);
    assert_eq!(magnification::value(&mut fixture), "1");
    assert!(fixture.desktop.keys.is_empty());
    assert!(!fixture.inspect(|canvas| canvas.world_mut().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::ControlLeft)));
    let point = normalized(&mut fixture, entity);
    down(&mut fixture, Device::Touch, point);
    up(&mut fixture, Device::Touch, point);
    fixture.until(|fixture| !fixture.desktop.input_pending());
    assert_eq!(fixture.inspect(|canvas| canvas.world_mut().resource::<Clicks>().0), 1);
    fixture.desktop.worker.stop().unwrap();
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn full_absolute_deferred_input_cancels_without_stopping_park_and_fresh_input_recovers() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut fixture = magnification::create(&runtime, directory.path(), [1000, 720], 1.25);
    magnification::open(&mut fixture);
    prepare(&mut fixture);
    let field = magnification::focus(&mut fixture, FIELD);
    magnification::focus(&mut fixture, "Increase draft by 0.25×");
    let point = normalized(&mut fixture, field);
    let (_, release) = fixture.pause();
    let mut resume = Resume(Some(release));
    fill(&fixture, 0, 0);
    down(&mut fixture, Device::Touch, point);
    for _ in 0..129 { fixture.desktop.key(30, true, 1).unwrap(); }
    fixture.desktop.key(30, false, 2).unwrap();
    up(&mut fixture, Device::Touch, point);
    resume.0.take().unwrap().send(()).unwrap();
    fixture.until(|fixture| !fixture.desktop.input_pending());
    assert!(fixture.desktop.running && fixture.desktop.worker.is_running());
    assert_eq!(magnification::value(&mut fixture), "1");
    assert!(fixture.desktop.keys.is_empty());
    let point = normalized(&mut fixture, field);
    down(&mut fixture, Device::Touch, point);
    write(&mut fixture, 5);
    up(&mut fixture, Device::Touch, point);
    fixture.until(|fixture| !fixture.desktop.input_pending());
    assert_eq!(magnification::value(&mut fixture), "4");
    fixture.desktop.worker.stop().unwrap();
}
