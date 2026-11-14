use super::*;
use rustix::time::{ClockId, clock_gettime};
use std::sync::atomic::AtomicU64;

fn quiet(fixture: &mut Fixture, updates: &AtomicU64) {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut previous = updates.load(Ordering::Acquire);
    let mut stable = Instant::now();
    loop {
        fixture.tick();
        let current = updates.load(Ordering::Acquire);
        if current != previous {
            previous = current;
            stable = Instant::now();
        }
        if fixture.desktop.worker.test_activity().0
            && stable.elapsed() >= Duration::from_millis(250)
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "The native worker did not settle after pointer delivery"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn cpu() -> f64 {
    let value = clock_gettime(ClockId::ProcessCPUTime);
    value.tv_sec as f64 * 1000.0 + value.tv_nsec as f64 / 1_000_000.0
}

#[test]
#[ignore = "requires hardware Vulkan/EGL; measures real-client motion and native work admission"]
fn external_pointer_motion_preserves_client_delivery_without_native_updates() {
    let checks = native::laboratory::run_interface_checks().unwrap();
    assert!(
        checks
            .first()
            .unwrap()
            .name
            .ends_with("::protein_area_filter_visible_and_selectable")
    );
    assert!(
        checks.iter().all(|check| check.error.is_none()),
        "{checks:?}"
    );
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut harness = Harness::new();
    let window = harness.window();
    let _red = crate::server::tests::pixel_tests::colored(&mut harness, &window, [255, 0, 0]);
    let mut fixture = Fixture::new(harness, &runtime, directory.path());
    fixture.until(|fixture| {
        fixture.desktop.current.as_ref().is_some_and(|frame| {
            frame.windows.len() == 1
                && frame.frame.geometry.as_ref().is_some_and(|geometry| {
                    geometry
                        .items
                        .iter()
                        .any(|item| item.id == ItemId::Application(frame.windows[0].id))
                })
        })
    });
    let updates = Arc::new(AtomicU64::new(0));
    let count = updates.clone();
    let observed = Arc::new(std::sync::Mutex::new(None));
    let pointer = observed.clone();
    fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        let root = world
            .query_filtered::<Entity, With<native::workspace::Workspaces>>()
            .single(world)
            .unwrap();
        let ids = world
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .entries
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>();
        for id in ids {
            native::workspace_config::set_physics(world, root, id, false);
        }
        world
            .resource_mut::<Schedules>()
            .get_mut(Update)
            .unwrap()
            .add_systems(
                move |windows: Query<&bevy::window::Window, With<bevy::window::PrimaryWindow>>| {
                    count.fetch_add(1, Ordering::Release);
                    *pointer.lock().unwrap() = windows
                        .single()
                        .unwrap()
                        .cursor_position()
                        .map(|point| point.to_array());
                },
            );
    });
    let item = fixture
        .desktop
        .current
        .as_ref()
        .unwrap()
        .frame
        .geometry
        .as_ref()
        .unwrap()
        .items
        .iter()
        .find(|item| matches!(item.id, ItemId::Application(_)))
        .unwrap()
        .clone();
    let origin = item.mapping.surface.origin();
    let size = item.mapping.surface.size();
    let center = [origin[0] + size[0] * 0.5, origin[1] + size[1] * 0.5];
    fixture
        .desktop
        .move_pointer(item.mapping.canvas_point(center).unwrap(), 1)
        .unwrap();
    quiet(&mut fixture, &updates);
    let before = updates.load(Ordering::Acquire);
    let positions = fixture.client.pointer_positions.len();
    let started = Instant::now();
    let started_cpu = cpu();
    for index in 0..120 {
        let when = started + Duration::from_millis(index * 8);
        std::thread::sleep(when.saturating_duration_since(Instant::now()));
        let local = [center[0] + (index % 20 + 1) as f64, center[1]];
        fixture
            .desktop
            .move_pointer(item.mapping.canvas_point(local).unwrap(), index as u32 + 2)
            .unwrap();
        fixture.tick();
    }
    fixture.exchange();
    let delivered = fixture.client.pointer_positions.len() - positions;
    let process_cpu_ms = cpu() - started_cpu;
    quiet(&mut fixture, &updates);
    let native_updates = updates.load(Ordering::Acquire) - before;
    let result = serde_json::json!({"motions":120,"client_positions":delivered,"native_updates":native_updates,"process_cpu_ms":process_cpu_ms,
        "offered_interval_ms":8,"wall_ms":started.elapsed().as_secs_f64()*1000.0});
    eprintln!("External pointer motion: {result}");
    if let Some(path) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
        std::fs::write(
            std::path::PathBuf::from(path).join("external-motion.json"),
            serde_json::to_vec_pretty(&result).unwrap(),
        )
        .unwrap();
    }
    assert_eq!(
        delivered, 120,
        "Every external motion must reach the client"
    );
    assert_eq!(
        native_updates, 0,
        "External-only pointer motion woke the full native scene"
    );
    let expected = fixture.desktop.pointer.map(|value| value as f32);
    let actual = fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        world
            .query_filtered::<&bevy::window::Window, With<bevy::window::PrimaryWindow>>()
            .single(world)
            .unwrap()
            .cursor_position()
            .unwrap()
            .to_array()
    });
    assert_eq!(
        actual, expected,
        "The next native action must see the latest passive pointer"
    );
    let buttons = fixture.client.button_recipients.len();
    fixture.desktop.button(272, true, 130).unwrap();
    fixture.desktop.button(272, false, 131).unwrap();
    fixture.until(|fixture| fixture.client.button_recipients.len() == buttons + 2);
    assert_eq!(
        &fixture.client.button_recipients[buttons..],
        &[
            (Some(window.surface.id()), 272, true),
            (Some(window.surface.id()), 272, false)
        ]
    );
    quiet(&mut fixture, &updates);
    let before = updates.load(Ordering::Acquire);
    let mut buffers = Vec::new();
    for (index, color) in [[0, 255, 0, 255], [255, 0, 0, 255], [0, 0, 255, 255]]
        .into_iter()
        .cycle()
        .take(9)
        .enumerate()
    {
        buffers.push(privacy::buffer(&fixture, [320, 240], color));
        window.surface.attach(buffers.last(), 0, 0);
        window.surface.damage_buffer(0, 0, 320, 240);
        window.surface.commit();
        fixture.exchange();
        let version = fixture.desktop.server.windows()[0].nodes[0].content.version;
        let local = [center[0] + index as f64 + 1.0, center[1] + 10.0];
        fixture
            .desktop
            .move_pointer(
                item.mapping.canvas_point(local).unwrap(),
                index as u32 + 140,
            )
            .unwrap();
        fixture.until(|fixture| {
            fixture
                .desktop
                .current
                .as_ref()
                .is_some_and(|frame| frame.windows[0].nodes[0].content.version == version)
        });
        let frame = &fixture.desktop.current.as_ref().unwrap().frame;
        let pixels = crate::presentation::probe_pixels(frame.dmabuf().unwrap()).unwrap();
        assert!(
            pixels
                .chunks_exact(4)
                .filter(|pixel| pixel[..3]
                    .iter()
                    .zip([color[2], color[1], color[0]])
                    .all(|(actual, expected)| actual.abs_diff(expected) < 15))
                .count()
                > 20_000,
            "The latest client color must be visible"
        );
    }
    quiet(&mut fixture, &updates);
    assert_eq!(
        updates.load(Ordering::Acquire),
        before,
        "Passive pointer samples turned client pixel commits into full native updates"
    );
    let native_point = (10..fixture.size[1])
        .step_by(20)
        .flat_map(|y| {
            (10..fixture.size[0])
                .step_by(20)
                .map(move |x| [f64::from(x), f64::from(y)])
        })
        .find(|point| {
            matches!(
                fixture.desktop.target_at(*point),
                crate::desktop::input::Target::Native
            )
        })
        .expect("The actual presented workspace must expose a native pointer target");
    fixture.desktop.move_pointer(native_point, 160).unwrap();
    assert!(matches!(
        fixture.desktop.target(),
        crate::desktop::input::Target::Native
    ));
    fixture.until(|_| updates.load(Ordering::Acquire) > before);
    quiet(&mut fixture, &updates);
    let before = updates.load(Ordering::Acquire);
    fixture
        .desktop
        .move_pointer(item.mapping.canvas_point(center).unwrap(), 161)
        .unwrap();
    fixture.until(|_| updates.load(Ordering::Acquire) > before);
    quiet(&mut fixture, &updates);
    let root = fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world
            .query_filtered::<Entity, With<native::workspace::Workspaces>>()
            .single(world)
            .unwrap();
        world
            .entity_mut(root)
            .insert(bevy::ui::RelativeCursorPosition::default());
        root
    });
    assert!(
        fixture
            .desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .input
            .pointer_motion
    );
    quiet(&mut fixture, &updates);
    let before_motion = updates.load(Ordering::Acquire);
    fixture
        .desktop
        .move_pointer(
            item.mapping
                .canvas_point([center[0] + 30.0, center[1]])
                .unwrap(),
            162,
        )
        .unwrap();
    fixture.until(|_| updates.load(Ordering::Acquire) > before_motion);
    quiet(&mut fixture, &updates);
    assert_eq!(
        *observed.lock().unwrap(),
        Some(fixture.desktop.pointer.map(|value| value as f32)),
        "A registered native cursor observer must receive the new coordinates without another native command"
    );
    fixture.inspect(move |canvas| {
        canvas
            .world_mut()
            .entity_mut(root)
            .remove::<bevy::ui::RelativeCursorPosition>();
    });
    assert!(
        !fixture
            .desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .input
            .pointer_motion
    );
    fixture
        .desktop
        .move_pointer(item.mapping.canvas_point(center).unwrap(), 163)
        .unwrap();
    quiet(&mut fixture, &updates);
    let before = updates.load(Ordering::Acquire);
    fixture
        .desktop
        .move_pointer(
            item.mapping
                .canvas_point([center[0] + 5.0, center[1]])
                .unwrap(),
            164,
        )
        .unwrap();
    quiet(&mut fixture, &updates);
    assert_eq!(
        updates.load(Ordering::Acquire),
        before,
        "Removing the last native motion observer must restore sleep"
    );
    fixture.desktop.worker.stop().unwrap();
    eprintln!(
        "Nine client pixel commits retain native sleep and exact colors; native entry/exit, current action position, clicks and cursor-dependent surfaces pass"
    );
}
