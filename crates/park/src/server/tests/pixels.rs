use super::Window;
use super::*;
use crate::presentation::Presentation;
use bevy::{ecs::schedule::Schedules, prelude::*};
use lince_interface::native;
use std::time::Instant;

#[derive(Resource, Default)]
struct Updates(u64);

pub(super) fn colored(
    harness: &mut Harness,
    window: &Window,
    color: [u8; 3],
) -> (std::fs::File, wl_shm_pool::WlShmPool, wl_buffer::WlBuffer) {
    use std::io::Write;
    let mut file = tempfile::tempfile().unwrap();
    let size = [320, 240];
    let pixels = [color[2], color[1], color[0], 255].repeat((size[0] * size[1]) as usize);
    file.write_all(&pixels).unwrap();
    let qh = harness.queue.handle();
    let pool = harness.client.shm.as_ref().unwrap().create_pool(
        file.as_fd(),
        pixels.len() as i32,
        &qh,
        (),
    );
    let buffer = pool.create_buffer(
        0,
        size[0],
        size[1],
        size[0] * 4,
        wl_shm::Format::Argb8888,
        &qh,
        (),
    );
    window.surface.attach(Some(&buffer), 0, 0);
    window.surface.damage_buffer(0, 0, size[0], size[1]);
    window.surface.commit();
    harness.sync();
    (file, pool, buffer)
}

fn cpu() -> f64 {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
    time.tv_sec as f64 * 1000.0 + time.tv_nsec as f64 / 1_000_000.0
}

#[test]
fn pixel_eligibility_preserves_semantic_changes_and_accepts_new_attachments() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _first = colored(&mut harness, &window, [255, 0, 0]);
    let initial = harness.server.windows().remove(0);
    let _next = colored(&mut harness, &window, [0, 255, 0]);
    let next = harness.server.windows().remove(0);
    assert!(initial.same_semantics(&next));
    assert_ne!(
        initial.nodes[0].content.version,
        next.nodes[0].content.version
    );
    let changes: Vec<Box<dyn Fn(&mut WindowSnapshot)>> = vec![
        Box::new(|window| window.title.push('!')),
        Box::new(|window| window.app_id.push('!')),
        Box::new(|window| window.parent = Some(window.id)),
        Box::new(|window| window.mapped = false),
        Box::new(|window| window.minimum[0] += 1),
        Box::new(|window| window.maximum[1] += 1),
        Box::new(|window| {
            window.geometry = Some(smithay::utils::Rectangle::from_size((300, 200).into()))
        }),
        Box::new(|window| window.nodes[0].origin[0] += 1),
        Box::new(|window| window.nodes[0].layout.size[0] += 1.0),
        Box::new(|window| window.nodes[0].layout.uv_origin[0] += 0.1),
        Box::new(|window| window.nodes[0].scale += 1),
        Box::new(|window| window.nodes[0].transform = smithay::utils::Transform::_90),
        Box::new(|window| window.nodes[0].input = Some(Default::default())),
        Box::new(|window| window.nodes[0].opaque = Some(Default::default())),
        Box::new(|window| window.nodes.push(window.nodes[0].clone())),
        Box::new(|window| {
            window.popups.push(crate::server::PopupSnapshot {
                surface: window.surface.clone(),
                origin: [1, 2],
                nodes: window.nodes.clone(),
            })
        }),
    ];
    for (index, change) in changes.iter().enumerate() {
        let mut changed = next.clone();
        change(&mut changed);
        assert!(
            !initial.same_semantics(&changed),
            "Semantic change {index} was ignored"
        );
    }
}

fn worker_frame(
    worker: &crate::runtime::CanvasWorker,
    ready: impl Fn(&crate::runtime::Completed) -> bool,
) -> crate::runtime::Completed {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        assert!(worker.failure().is_none());
        assert!(worker.is_running());
        if let Some(frame) = worker.frame()
            && ready(&frame)
        {
            frame.frame.wait_for_probe().unwrap();
            return frame;
        }
        assert!(
            Instant::now() < deadline,
            "The pixel worker did not publish its expected frame"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn quiet_worker(worker: &crate::runtime::CanvasWorker, updates: &std::sync::atomic::AtomicU64) {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut since = Instant::now();
    let mut previous = updates.load(Ordering::Acquire);
    loop {
        assert!(worker.failure().is_none());
        let current = updates.load(Ordering::Acquire);
        if worker.frame().is_some() || current != previous || !worker.test_activity().0 {
            since = Instant::now();
            previous = current;
        }
        if since.elapsed() > Duration::from_millis(300) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "The pixel worker did not become quiet"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
#[ignore = "requires hardware Vulkan/EGL; private DMA-BUF producer fence and asynchronous worker"]
fn unavailable_client_pixels_sleep_while_ready_clients_input_and_fence_completion_progress() {
    use crate::{graphics::{VulkanRenderer, import_gate}, runtime::{CanvasWorker, Configuration, Input, Scene}};
    use std::sync::atomic::AtomicU64;
    let checks = native::laboratory::run_interface_checks().unwrap();
    assert!(checks.first().unwrap().name.ends_with("::protein_area_filter_visible_and_selectable"));
    assert!(checks.iter().all(|check| check.error.is_none()), "{checks:?}");
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut worker = CanvasWorker::spawn(Configuration {
        showoff: false, size: [640, 480], directory: directory.path().into(), runtime: runtime.handle().clone(), cell: None,
    }).unwrap();
    worker_frame(&worker, |_| true);
    let updates = Arc::new(AtomicU64::new(0));
    let counted = updates.clone();
    let serial = worker.input(Input::Test(Box::new(move |canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        native::workspace_config::set_physics(world, root, 1, false);
        world.resource_mut::<Schedules>().get_mut(Update).unwrap().add_systems(move || { counted.fetch_add(1, Ordering::Release); });
    }))).unwrap();
    worker_frame(&worker, |frame| frame.input_serial >= serial);
    let renderer = VulkanRenderer::new().unwrap();
    let mut harness = Harness::new();
    use smithay::backend::allocator::{Format, Fourcc, Modifier};
    harness.server.enable_dmabuf_formats([Format { code: Fourcc::Argb8888, modifier: Modifier::Linear }], || None).unwrap();
    harness.sync();
    let held = harness.window();
    let ready = harness.window();
    let _initial_held = colored(&mut harness, &held, [0, 255, 0]);
    let _initial_ready = colored(&mut harness, &ready, [0, 0, 255]);
    let scene = |harness: &Harness| Scene {
        revision: 1,
        outputs: harness.server.output_viewports().into_iter().map(|(name, viewport)| crate::presentation::Output { name, viewport }).collect(),
        windows: Arc::new(harness.server.windows()), display: None,
    };
    worker.scene(scene(&harness)).unwrap();
    let initial = worker_frame(&worker, |frame| frame.windows.len() == 2);
    quiet_worker(&worker, &updates);
    worker.scene(scene(&harness)).unwrap();
    let retained = worker_frame(&worker, |frame| frame.windows.len() == 2);
    let held_id = initial.windows.iter().find(|window| smithay::reexports::wayland_server::Resource::id(&window.surface).protocol_id() == held.surface.id().protocol_id()).unwrap().id;
    let ready_id = initial.windows.iter().find(|window| window.id != held_id).unwrap().id;
    let old_version = initial.windows.iter().find(|window| window.id == held_id).unwrap().nodes[0].content.version;
    let before_main = updates.load(Ordering::Acquire);
    let before_frames = worker.test_activity().1;
    let (image, gate) = import_gate::pixels(&renderer, wgpu::Color::RED);
    let buffer = super::dmabuf_tests::buffer(&harness.client, &harness.queue.handle(), &image.dmabuf);
    let held_done = Arc::new(AtomicBool::new(false));
    held.surface.frame(&harness.queue.handle(), held_done.clone());
    held.surface.attach(Some(&buffer), 0, 0);
    held.surface.damage_buffer(0, 0, 320, 240);
    held.surface.commit();
    harness.sync();
    let expected_held = harness.server.windows().iter().find(|window| window.id == held_id).unwrap().nodes[0].content.version;
    for _ in 0..4 {
        worker.scene(scene(&harness)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut quiet = Instant::now();
        loop {
            assert!(worker.failure().is_none());
            if worker.frame().is_some() || !worker.test_activity().0 { quiet = Instant::now(); }
            if quiet.elapsed() >= Duration::from_millis(30) { break; }
            assert!(Instant::now() < deadline, "The worker did not idle behind a private producer fence");
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(!import_gate::ready(&image.dmabuf));
    }
    let redundant_frames = worker.test_activity().1 - before_frames;
    let ready_done = Arc::new(AtomicBool::new(false));
    ready.surface.frame(&harness.queue.handle(), ready_done.clone());
    let _next_ready = colored(&mut harness, &ready, [255, 255, 0]);
    let expected_ready = harness.server.windows().iter().find(|window| window.id == ready_id).unwrap().nodes[0].content.version;
    worker.scene(scene(&harness)).unwrap();
    let partial = worker_frame(&worker, |frame| frame.windows.iter().any(|window| window.id == ready_id && window.nodes[0].content.version == expected_ready));
    assert_eq!(partial.windows.iter().find(|window| window.id == held_id).unwrap().nodes[0].content.version, old_version);
    assert!(Arc::ptr_eq(&partial.frame.input, &retained.frame.input));
    for window in partial.windows.iter() {
        harness.server.frame_started_nodes(&harness.server.callback_nodes(window));
    }
    harness.sync();
    assert!(ready_done.load(Ordering::Acquire));
    assert!(!held_done.load(Ordering::Acquire));
    let pixels = crate::presentation::probe_pixels(partial.frame.dmabuf().unwrap()).unwrap();
    assert!(pixels.chunks_exact(4).filter(|pixel| pixel[0] > 225 && pixel[1] > 225 && pixel[2] < 30).count() > 20_000);
    assert_eq!(updates.load(Ordering::Acquire), before_main, "Unavailable pixels restarted native semantic work");
    assert!(!import_gate::ready(&image.dmabuf));
    gate.release();
    let completed = worker_frame(&worker, |frame| frame.windows.iter().any(|window| window.id == held_id && window.nodes[0].content.version == expected_held));
    assert!(import_gate::ready(&image.dmabuf));
    assert!(Arc::ptr_eq(&completed.frame.input, &retained.frame.input));
    assert_eq!(updates.load(Ordering::Acquire), before_main, "Producer readiness restarted native semantic work");
    for window in completed.windows.iter() {
        harness.server.frame_started_nodes(&harness.server.callback_nodes(window));
    }
    harness.sync();
    assert!(held_done.load(Ordering::Acquire));
    assert_eq!(completed.windows.iter().find(|window| window.id == ready_id).unwrap().nodes[0].content.version, expected_ready);
    let (input_image, input_gate) = import_gate::pixels(&renderer, wgpu::Color::GREEN);
    let input_buffer = super::dmabuf_tests::buffer(&harness.client, &harness.queue.handle(), &input_image.dmabuf);
    held.surface.attach(Some(&input_buffer), 0, 0);
    held.surface.damage_buffer(0, 0, 320, 240);
    held.surface.commit();
    harness.sync();
    let expected_input = harness.server.windows().iter().find(|window| window.id == held_id).unwrap().nodes[0].content.version;
    worker.scene(scene(&harness)).unwrap();
    let serial = worker.input(Input::Pointer([40.0, 40.0])).unwrap();
    let input = worker_frame(&worker, |frame| frame.input_serial >= serial);
    assert_eq!(input.windows.iter().find(|window| window.id == held_id).unwrap().nodes[0].content.version, expected_held);
    assert!(!import_gate::ready(&input_image.dmabuf));
    input_gate.release();
    worker_frame(&worker, |frame| frame.windows.iter().any(|window| window.id == held_id && window.nodes[0].content.version == expected_input));
    eprintln!("Private unavailable-pixel scenes: 4, redundant published frames: {redundant_frames}; independent ready pixels, callbacks, input and fence wake passed");
    worker.stop().unwrap();
    assert_eq!(redundant_frames, 0, "Unavailable client pixels caused redundant native frame preparation");
}

#[test]
#[ignore = "requires hardware Vulkan/EGL; real asynchronous worker and private client"]
fn a_quiet_worker_retains_client_frames_and_resumes_native_work_for_input_metadata_and_wakes() {
    use crate::runtime::{CanvasWorker, Configuration, Input, Scene};
    use std::sync::atomic::AtomicU64;
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut worker = CanvasWorker::spawn(Configuration {
        showoff: false,
        size: [640, 480],
        directory: directory.path().into(),
        runtime: runtime.handle().clone(),
        cell: None,
    })
    .unwrap();
    worker_frame(&worker, |_| true);
    let updates = Arc::new(AtomicU64::new(0));
    let counted = updates.clone();
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    let serial = worker
        .input(Input::Test(Box::new(move |canvas| {
            let world = canvas.world_mut();
            let root = world
                .query_filtered::<Entity, With<native::workspace::Workspaces>>()
                .single(world)
                .unwrap();
            native::workspace_config::set_physics(world, root, 1, false);
            send.send(
                world
                    .resource::<lince_interface::host::WakeSignal>()
                    .clone(),
            )
            .unwrap();
            world
                .resource_mut::<Schedules>()
                .get_mut(Update)
                .unwrap()
                .add_systems(move || {
                    counted.fetch_add(1, Ordering::Release);
                });
        })))
        .unwrap();
    worker_frame(&worker, |frame| frame.input_serial >= serial);
    let wake = receive.recv_timeout(Duration::from_secs(1)).unwrap();
    let mut harness = Harness::new();
    let window = harness.window();
    let mut buffers = vec![colored(&mut harness, &window, [255, 0, 0])];
    let scene = |harness: &Harness| {
        use lince_interface::host::{OutputId, OutputViewport, Rectangle, raster::RasterMapping};
        let bounds = Rectangle::new([0.0; 2], [640.0, 480.0]).unwrap();
        Scene {
            revision: 1,
            outputs: vec![crate::presentation::Output {
                name: "Client pixel fixture".into(),
                viewport: OutputViewport {
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
                },
            }],
            windows: Arc::new(harness.server.windows()),
            display: None,
        }
    };
    worker.scene(scene(&harness)).unwrap();
    worker_frame(&worker, |frame| !frame.windows.is_empty());
    quiet_worker(&worker, &updates);
    let before = updates.load(Ordering::Acquire);
    let initial = {
        buffers.push(colored(&mut harness, &window, [0, 255, 0]));
        let expected = harness.server.windows()[0].nodes[0].content.version;
        worker.scene(scene(&harness)).unwrap();
        worker_frame(&worker, |frame| {
            frame
                .windows
                .first()
                .is_some_and(|window| window.nodes[0].content.version == expected)
        })
    };
    for color in [[0, 0, 255], [255, 0, 0], [0, 255, 0]] {
        buffers.push(colored(&mut harness, &window, color));
        let expected = harness.server.windows()[0].nodes[0].content.version;
        worker.scene(scene(&harness)).unwrap();
        let frame = worker_frame(&worker, |frame| {
            frame
                .windows
                .first()
                .is_some_and(|window| window.nodes[0].content.version == expected)
        });
        assert!(Arc::ptr_eq(&frame.frame.input, &initial.frame.input));
        let pixels = crate::presentation::probe_pixels(frame.frame.dmabuf().unwrap()).unwrap();
        assert!(
            pixels
                .chunks_exact(4)
                .filter(|pixel| pixel[..3]
                    .iter()
                    .zip(color)
                    .all(|(actual, expected)| actual.abs_diff(expected) < 30))
                .count()
                > 20_000
        );
    }
    quiet_worker(&worker, &updates);
    assert_eq!(
        updates.load(Ordering::Acquire),
        before,
        "Client pixels ran the native schedule"
    );
    let serial = worker.input(Input::Pointer([40.0, 40.0])).unwrap();
    worker_frame(&worker, |frame| frame.input_serial >= serial);
    quiet_worker(&worker, &updates);
    assert!(updates.load(Ordering::Acquire) > before);
    let before = updates.load(Ordering::Acquire);
    window.role.set_title("Changed native window title".into());
    harness.sync();
    worker.scene(scene(&harness)).unwrap();
    worker_frame(&worker, |frame| {
        frame
            .windows
            .first()
            .is_some_and(|window| window.title == "Changed native window title")
    });
    let settling = updates.load(Ordering::Acquire);
    for color in [[255, 0, 0], [0, 255, 0], [0, 0, 255]]
        .into_iter()
        .cycle()
        .take(9)
    {
        buffers.push(colored(&mut harness, &window, color));
        let expected = harness.server.windows()[0].nodes[0].content.version;
        worker.scene(scene(&harness)).unwrap();
        worker_frame(&worker, |frame| {
            frame
                .windows
                .first()
                .is_some_and(|window| window.nodes[0].content.version == expected)
        });
    }
    quiet_worker(&worker, &updates);
    assert!(updates.load(Ordering::Acquire) > before);
    assert!(
        updates.load(Ordering::Acquire) - settling <= 3,
        "Continuous client frames restarted native settling"
    );
    let before = updates.load(Ordering::Acquire);
    wake.ring();
    worker_frame(&worker, |_| true);
    quiet_worker(&worker, &updates);
    assert!(updates.load(Ordering::Acquire) > before);
    worker.stop().unwrap();
}

#[test]
#[ignore = "records a warmed real-client pixel stream; hardware Vulkan/EGL, no concurrent compilation"]
fn unchanged_native_scene_records_client_pixel_stream_cost_and_visible_colors() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut canvas = Presentation::new(
        [640, 480],
        directory.path().into(),
        lince_interface::host::WakeSignal::new(|| {}),
    )
    .unwrap();
    canvas.world_mut().init_resource::<Updates>();
    canvas
        .world_mut()
        .resource_mut::<Schedules>()
        .get_mut(Update)
        .unwrap()
        .add_systems(|mut updates: ResMut<Updates>| {
            updates.0 += 1;
        });
    for _ in 0..8 {
        canvas.probe_frame().unwrap();
    }
    let root = {
        let world = canvas.world_mut();
        world
            .query_filtered::<Entity, With<native::workspace::Workspaces>>()
            .single(world)
            .unwrap()
    };
    canvas.create_workspace(root.to_bits()).unwrap();
    let workspace = canvas
        .world_mut()
        .get::<native::workspace::Workspaces>(root)
        .unwrap()
        .active;
    for index in 0..64 {
        native::sand_store::spawn_sand(
            canvas.world_mut(),
            root,
            workspace,
            native::sand_store::SandKind::Text,
            "Retained native label",
            bevy::math::DVec2::new(f64::from(index % 8) * 300.0, f64::from(index / 8) * 200.0),
        );
    }
    native::workspace_config::set_physics(canvas.world_mut(), root, workspace, false);
    native::workspace_config::set_physics(canvas.world_mut(), root, 1, false);
    canvas.activate_workspace(root.to_bits(), 1).unwrap();
    let mut harness = Harness::new();
    let window = harness.window();
    let mut buffers = vec![colored(&mut harness, &window, [255, 0, 0])];
    assert!(!canvas.sync_windows(&harness.server.windows()).unwrap());
    for _ in 0..12 {
        canvas.probe_frame().unwrap();
    }
    let initial = canvas.probe_frame().unwrap();
    let geometry = initial.geometry.clone();
    let retained = std::env::var("LINCE_PARK_PIXEL_PATH").as_deref() == Ok("retained");
    let rounds = std::env::var("LINCE_PARK_PIXEL_ROUNDS").ok()
        .map(|value| value.parse::<usize>().unwrap()).unwrap_or(24);
    assert!((3..=600).contains(&rounds) && rounds.is_multiple_of(3));
    let before = canvas.world().resource::<Updates>().0;
    let mut elapsed = Vec::new();
    let mut cpu_time = Vec::new();
    eprintln!("pixel-stream measuring {rounds} rounds");
    for index in 0..rounds {
        let color = [[255, 0, 0], [0, 255, 0], [0, 0, 255]][index % 3];
        buffers.push(colored(&mut harness, &window, color));
        let snapshots = harness.server.windows();
        let started = Instant::now();
        let cpu_started = cpu();
        let frame = if retained {
            assert_eq!(
                canvas.sync_pixels(&snapshots).unwrap(),
                crate::presentation::PixelSync::Ready
            );
            let frame = canvas.pixel_frame().unwrap();
            frame.wait_for_probe().unwrap();
            frame
        } else {
            assert!(!canvas.sync_windows(&snapshots).unwrap());
            canvas.probe_frame().unwrap()
        };
        cpu_time.push(cpu() - cpu_started);
        elapsed.push(started.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(frame.geometry, geometry);
        if retained {
            assert!(Arc::ptr_eq(
                frame.geometry.as_ref().unwrap(),
                initial.geometry.as_ref().unwrap()
            ));
            assert!(Arc::ptr_eq(&frame.input, &initial.input));
        }
        let pixels = crate::presentation::probe_pixels(frame.dmabuf().unwrap()).unwrap();
        let count = pixels
            .chunks_exact(4)
            .filter(|pixel| {
                pixel[..3]
                    .iter()
                    .zip(color)
                    .all(|(actual, expected)| actual.abs_diff(expected) < 30)
            })
            .count();
        assert!(
            count > 20_000,
            "New client color was not visible: {color:?}, pixels={count}"
        );
        lince_interface::host::performance::flush_if_due();
    }
    let updates = canvas.world().resource::<Updates>().0 - before;
    assert_eq!(updates, if retained { 0 } else { rounds as u64 });
    if retained {
        use std::io::{Seek, SeekFrom, Write};
        let (file, _, buffer) = buffers.last_mut().unwrap();
        assert!(harness.server.windows()[0].nodes[0].content.is_released());
        file.seek(SeekFrom::Start(0)).unwrap();
        file.write_all(&[255, 0, 255, 255].repeat(320 * 240))
            .unwrap();
        window.surface.damage_buffer(0, 0, 320, 240);
        window.surface.commit();
        harness.sync();
        assert_eq!(
            canvas.sync_pixels(&harness.server.windows()).unwrap(),
            crate::presentation::PixelSync::Ready
        );
        let frame = canvas.pixel_frame().unwrap();
        frame.wait_for_probe().unwrap();
        let pixels = crate::presentation::probe_pixels(frame.dmabuf().unwrap()).unwrap();
        assert!(
            pixels
                .chunks_exact(4)
                .filter(|pixel| pixel[0] < 30 && pixel[1] < 30 && pixel[2] > 225)
                .count()
                > 20_000,
            "Damage without reattachment read released client storage"
        );
        window.surface.attach(Some(buffer), 0, 0);
        window.surface.damage_buffer(0, 0, 320, 240);
        window.surface.commit();
        harness.sync();
        assert_eq!(
            canvas.sync_pixels(&harness.server.windows()).unwrap(),
            crate::presentation::PixelSync::Ready
        );
        let frame = canvas.pixel_frame().unwrap();
        frame.wait_for_probe().unwrap();
        let pixels = crate::presentation::probe_pixels(frame.dmabuf().unwrap()).unwrap();
        let magenta = |pixels: &[u8]| {
            pixels
                .chunks_exact(4)
                .filter(|pixel| pixel[0] > 225 && pixel[1] < 30 && pixel[2] > 225)
                .count()
        };
        assert!(
            magenta(&pixels) > 20_000,
            "Reattached client storage did not show its new pixels"
        );
        let world = canvas.world_mut();
        let application = world
            .query_filtered::<Entity, With<native::layout::desktop::ApplicationWindow>>()
            .single(world)
            .unwrap();
        world
            .get_mut::<native::layout::LayoutBox>(application)
            .unwrap()
            .capture_excluded = true;
        let gate = lince_interface::host::capture::PolicyGate::default();
        world.insert_resource(gate.clone());
        world.insert_resource(lince_interface::host::capture::Demand {
            outputs: std::collections::BTreeSet::from([lince_interface::host::OutputId(1)]),
            ..Default::default()
        });
        for _ in 0..3 {
            canvas.probe_frame().unwrap();
        }
        buffers.push(colored(&mut harness, &window, [255, 0, 255]));
        assert_eq!(
            canvas.sync_pixels(&harness.server.windows()).unwrap(),
            crate::presentation::PixelSync::Ready
        );
        let frame = canvas.pixel_frame().unwrap();
        frame.wait_for_probe().unwrap();
        let source = frame.capture_source().unwrap();
        let captured = frame
            .capture_dmabuf(lince_interface::host::OutputId(1), source.revision, 0)
            .unwrap();
        assert!(
            magenta(&crate::presentation::probe_pixels(captured).unwrap()) < 100,
            "Retained rendering leaked a private application"
        );
        assert!(
            magenta(&crate::presentation::probe_pixels(frame.dmabuf().unwrap()).unwrap()) > 20_000
        );
        gate.invalidate();
        let frame = canvas.pixel_frame().unwrap();
        frame.wait_for_probe().unwrap();
        assert!(
            frame.capture_source().is_none(),
            "An invalidated capture policy was reused"
        );
    }
    elapsed.sort_by(f64::total_cmp);
    cpu_time.sort_by(f64::total_cmp);
    let result = serde_json::json!({"scope":"Private warmed real-client stream with 64 native text Sands on another workspace; diagnostic build, no physical scanout/input acceptance", "retained":retained, "rounds":elapsed.len(), "native_updates":updates,"elapsed_median_ms":elapsed[elapsed.len()/2],"elapsed_p95_ms":elapsed[elapsed.len()*95/100],"cpu_median_ms":cpu_time[cpu_time.len()/2],"cpu_total_ms":cpu_time.iter().sum::<f64>(),"elapsed_ms":elapsed,"cpu_ms":cpu_time});
    if let Some(path) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            path.join("pixel-stream.json"),
            serde_json::to_vec_pretty(&result).unwrap(),
        )
        .unwrap();
    }
    eprintln!("pixel-stream: {result}");
}
