use super::*;
use crate::native_input::absolute::{Contacts, Pens};
use lince_interface::host::input::TabletInput;

fn settled(scene: &mut Scene) -> [usize; 4] {
    scene.fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        [world.get_resource::<Pens>().map_or(0, |pens| pens.tools.len()), world.get_resource::<Pens>().map_or(0, |pens| pens.retired.len()),
            world.get_resource::<Contacts>().map_or(0, |contacts| contacts.0.len()), world.query::<&TabletInput>().iter(world).count()]
    })
}

#[test]
#[ignore = "requires hardware Vulkan/EGL; supplied pen reports do not measure physical libinput lifetimes"]
fn twenty_native_pen_device_lifetimes_settle_host_worker_contacts_and_pointer_entities_after_held_or_ended_removal() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut scene = create(&runtime, directory.path());
    scene.fixture.desktop.tablet_device_removed(&scene.devices[1].id, scene.devices[1].generation, 399);
    let endings = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let observed = endings.clone();
    scene.fixture.inspect(move |canvas| {
        canvas.world_mut().resource_mut::<Schedules>().get_mut(Update).unwrap().add_systems(move |mut events: MessageReader<TabletInput>| {
            for event in events.read() {
                if event.tool == 92 && matches!(event.phase, TabletPhase::Leave | TabletPhase::Cancel) {
                    observed.lock().unwrap().push(event.phase);
                }
            }
        });
    });
    let baseline = settled(&mut scene);
    assert_eq!(baseline, [0; 4]);
    for cycle in 0..20 {
        scene.devices[1].generation += 1;
        scene.devices[1].associated_output = Some(B_NAME.into());
        install_devices(&mut scene.fixture, &scene.devices, cycle + 10);
        let device = scene.devices[1].clone();
        assert!(scene.fixture.desktop.server.tablet_device(device.generation, TabletDescriptor { name: device.name.clone(), usb_id: None, syspath: None }));
        let point = native_point(&mut scene);
        pen(&mut scene.fixture, &device, point, TabletPhase::Enter);
        pen(&mut scene.fixture, &device, point, TabletPhase::Tip(true));
        scene.fixture.desktop.flush_touch().unwrap();
        scene.fixture.until(|fixture| !fixture.desktop.input_pending());
        assert_eq!(scene.fixture.desktop.tablet_test_counts()[0], 1);
        assert_eq!(settled(&mut scene)[0], 1);
        endings.lock().unwrap().clear();
        if cycle % 2 == 0 {
            pen(&mut scene.fixture, &device, point, TabletPhase::Tip(false));
            pen(&mut scene.fixture, &device, point, TabletPhase::Leave);
        }
        scene.fixture.desktop.tablet_device_removed(&device.id, device.generation, 400);
        scene.fixture.until(|fixture| !fixture.desktop.input_pending());
        let worker = settled(&mut scene);
        scene.fixture.desktop.reconcile_tablet();
        assert_eq!(scene.fixture.desktop.tablet_test_counts(), [0; 2], "Host lifetime leaked in cycle {cycle}");
        assert_eq!(worker, baseline, "Worker pointer lifetime leaked in cycle {cycle}");
        assert_eq!(endings.lock().unwrap().len(), 1, "Native pen termination must be delivered once in cycle {cycle}");
        assert!(scene.fixture.desktop.server.tablet_cursors().is_empty());
        assert!(scene.fixture.desktop.keys.is_empty());
        assert!(scene.fixture.desktop.running && scene.fixture.desktop.worker.is_running());
    }
    scene.fixture.desktop.worker.stop().unwrap();
}
