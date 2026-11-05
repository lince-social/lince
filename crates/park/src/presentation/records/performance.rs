use super::*;

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn typing_proceeds_during_a_blocked_save_and_successful_completion_does_not_wake_native_work() {
    use crate::runtime::{CanvasWorker, Completed, Configuration, Input};
    use bevy::{
        input::keyboard::{Key, KeyCode},
        input_focus::{FocusCause, InputFocus},
    };
    use std::sync::{atomic::AtomicU64, mpsc};

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut worker = CanvasWorker::spawn(Configuration {
        showoff: false,
        size: [640, 480],
        directory: directory.path().into(),
        runtime: runtime.handle().clone(),
        cell: None,
    })
    .unwrap();
    let wait = |serial, text: Option<&str>| -> Completed {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut observed = None;
        loop {
            assert!(worker.failure().is_none());
            assert!(worker.is_running());
            if let Some(frame) = worker.frame() {
                observed = Some((
                    frame.input_serial,
                    frame.frame.input.editor.clone(),
                    frame.frame.input.keyboard,
                ));
                if frame.input_serial >= serial
                    && text.is_none_or(|text| {
                        frame
                            .frame
                            .input
                            .editor
                            .as_ref()
                            .is_some_and(|editor| editor.surrounding == text)
                    })
                {
                    assert!(frame.frame.ready().unwrap());
                    return frame;
                }
            }
            assert!(
                Instant::now() < deadline,
                "The worker did not deliver input {serial} with editor text {text:?}; last={observed:?}"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    };
    wait(0, None);
    let serial = worker
        .input(Input::Test(Box::new(|canvas| {
            use lince_interface::host::{
                OutputId, OutputViewport, Rectangle, raster::RasterMapping,
            };
            let bounds = Rectangle::new([0.0; 2], [640.0, 480.0]).unwrap();
            canvas
                .viewport(OutputViewport {
                    rotation: Default::default(),
                    id: OutputId(1),
                    bounds,
                    usable: Some(bounds),
                    physical_size: [640, 480],
                    raster: RasterMapping {
                        origin: [0.0; 2],
                        scale: [1.0; 2],
                    },
                    workspace: 1,
                })
                .unwrap();
            let root = canvas.canvas_root().unwrap();
            let world = canvas.world_mut();
            let bundle = native::sand::single_line_editor(
                "",
                world.resource::<native::theme::Typography>(),
                0,
                32,
            );
            let editor = world.spawn((bundle, ChildOf(root))).id();
            world.entity_mut(editor).insert(Node {
                position_type: PositionType::Absolute,
                left: px(100),
                top: px(100),
                width: px(300),
                height: px(40),
                ..default()
            });
            world
                .resource_mut::<InputFocus>()
                .set(editor, FocusCause::Navigated);
        })))
        .unwrap();
    wait(serial, Some(""));

    let path = directory.path().join("private-profiles.json");
    let mut records = Records::load(path.clone()).unwrap();
    let layout = native::layout::LayoutBox::new(Vec2::splat(300.0));
    records.saved.insert(
        PlacementId(layout.id),
        Saved {
            layout,
            workspace: 1,
            position: [200.0, 0.0],
            size: [300.0; 2],
            tokens: Default::default(),
            launch: None,
        },
    );
    records.dirty = true;
    records.last_write = Instant::now() - Duration::from_secs(2);
    let (entered, started) = mpsc::sync_channel(1);
    let (release, resumed) = mpsc::sync_channel(1);
    let (completed, finished) = mpsc::sync_channel(1);
    let target = path.clone();
    records.writer = Some(
        storage::Writer::with_writer(move |profiles| {
            entered.send(()).map_err(io::Error::other)?;
            resumed
                .recv_timeout(Duration::from_secs(30))
                .map_err(io::Error::other)?;
            storage::save(&target, profiles)?;
            completed.send(()).map_err(io::Error::other)
        })
        .unwrap(),
    );
    let wakes = Arc::new(AtomicU64::new(0));
    let counted = wakes.clone();
    let serial = worker
        .input(Input::Test(Box::new(move |canvas| {
            let world = canvas.world_mut();
            let wake = world.resource::<WakeSignal>().clone();
            let forwarded = wake.clone();
            world.insert_resource(WakeSignal::new(move || {
                counted.fetch_add(1, Ordering::AcqRel);
                forwarded.ring();
            }));
            records.tick(world, &HashMap::new(), &HashMap::new());
            world.insert_resource(wake);
            canvas.records = Some(records);
        })))
        .unwrap();
    started.recv_timeout(Duration::from_secs(20)).unwrap();
    wait(serial, Some(""));
    for pressed in [true, false] {
        let serial = worker
            .input(Input::Key {
                physical: KeyCode::KeyQ,
                logical: Key::Character("q".into()),
                text: pressed.then(|| "q".into()),
                pressed,
                repeat: false,
            })
            .unwrap();
        wait(serial, Some("q"));
    }
    assert!(!path.exists(), "The write gate was not held during typing");
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut quiet_since = Instant::now();
    while quiet_since.elapsed() < Duration::from_millis(500) {
        assert!(worker.failure().is_none());
        if worker.frame().is_some() {
            quiet_since = Instant::now();
        }
        assert!(
            Instant::now() < deadline,
            "The private editor did not settle while saving"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    release.send(()).unwrap();
    finished.recv_timeout(Duration::from_secs(5)).unwrap();
    let deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < deadline {
        assert!(worker.failure().is_none());
        let _ = worker.frame();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        wakes.load(Ordering::Acquire),
        0,
        "Successful routine saving requested unnecessary native work"
    );
    worker.stop().unwrap();
    assert_eq!(
        Records::load(path)
            .unwrap()
            .saved
            .values()
            .next()
            .unwrap()
            .position[0],
        200.0
    );
}

#[test]
#[ignore = "private filesystem diagnostic; run without concurrent compilation or GPU tests"]
fn automatic_profile_saves_record_caller_cost_and_durable_contents() {
    let directory = tempfile::tempdir().unwrap();
    let mut reports = Vec::new();
    for count in [16, 128, 1024, 4096] {
        let path = directory.path().join(format!("profiles-{count}.json"));
        let mut records = Records::load(path.clone()).unwrap();
        for _ in 0..count {
            let layout = native::layout::LayoutBox::new(Vec2::splat(300.0));
            records.saved.insert(
                PlacementId(layout.id),
                Saved {
                    layout,
                    workspace: 1,
                    position: [0.0; 2],
                    size: [300.0; 2],
                    tokens: Default::default(),
                    launch: None,
                },
            );
        }
        let mut world = World::new();
        world.init_resource::<Messages<native::cell_bridge::CellMessage>>();
        let mut elapsed = Vec::new();
        let mut cpu = Vec::new();
        for round in 0..16 {
            records.saved.values_mut().next().unwrap().position[0] += 1.0;
            records.dirty = true;
            records.last_write = Instant::now() - Duration::from_secs(2);
            let started = Instant::now();
            let cpu_started = rustix::time::clock_gettime(rustix::time::ClockId::ThreadCPUTime);
            records.tick(&mut world, &HashMap::new(), &HashMap::new());
            let cpu_ended = rustix::time::clock_gettime(rustix::time::ClockId::ThreadCPUTime);
            if round >= 4 {
                elapsed.push(started.elapsed().as_secs_f64() * 1000.0);
                cpu.push(
                    (cpu_ended.tv_sec - cpu_started.tv_sec) as f64 * 1000.0
                        + (cpu_ended.tv_nsec - cpu_started.tv_nsec) as f64 / 1_000_000.0,
                );
            }
            records.flush().unwrap();
        }
        let restored = Records::load(path.clone()).unwrap();
        assert_eq!(restored.saved.len(), count);
        assert!(restored.saved == records.saved);
        elapsed.sort_by(f64::total_cmp);
        cpu.sort_by(f64::total_cmp);
        reports.push(serde_json::json!({"profiles":count,"warmup":4,"rounds":12,
            "caller_median_ms":elapsed[6],"caller_p95_ms":elapsed[11],
            "caller_cpu_median_ms":cpu[6],"caller_cpu_total_ms":cpu.iter().sum::<f64>(),
            "bytes":std::fs::metadata(path).unwrap().len()}));
    }
    let result = serde_json::json!({"scope":"Private profile autosave caller, validation/serialization/durability on the caller or worker; explicit flush/readback outside timing. No physical input latency or desktop FPS acceptance.","reports":reports});
    eprintln!("profile-save-cost: {result}");
    if let Some(path) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS").map(PathBuf::from) {
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            path.join("profile-save-cost.json"),
            serde_json::to_vec_pretty(&result).unwrap(),
        )
        .unwrap();
    }
}
