use super::*;
use bevy::{
    app::SubApp,
    input::keyboard::Key,
    input_focus::{FocusCause, InputFocus},
};
use lince_interface::host::{ItemId, OutputId};
use std::{
    sync::mpsc::{Receiver, SyncSender, sync_channel},
    thread::JoinHandle,
    time::Duration,
};

struct RenderWorker {
    sender: Option<SyncSender<(SubApp, usize)>>,
    receiver: Receiver<(SubApp, usize, [f64; 2])>,
    worker: Option<JoinHandle<()>>,
}

impl RenderWorker {
    fn new(origin: Instant, runtime: tokio::runtime::Handle) -> Self {
        let (sender, incoming) = sync_channel::<(SubApp, usize)>(1);
        let (outgoing, receiver) = sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("park-overlap-probe".into())
            .stack_size(32 * 1024 * 1024)
            .spawn(move || {
                let _entered = runtime.enter();
                while let Ok((mut app, epoch)) = incoming.recv() {
                    let start = origin.elapsed().as_secs_f64() * 1000.0;
                    app.update();
                    let end = origin.elapsed().as_secs_f64() * 1000.0;
                    if outgoing.send((app, epoch, [start, end])).is_err() {
                        break;
                    }
                }
            })
            .unwrap();
        Self {
            sender: Some(sender),
            receiver,
            worker: Some(worker),
        }
    }

    fn submit(&self, app: SubApp, epoch: usize) {
        assert_send_resources(&app);
        self.sender.as_ref().unwrap().send((app, epoch)).unwrap();
    }

    fn receive(&self) -> (SubApp, usize, [f64; 2]) {
        let result = self
            .receiver
            .recv_timeout(Duration::from_secs(30))
            .expect("Render handoff did not return");
        assert_send_resources(&result.0);
        result
    }
}

impl Drop for RenderWorker {
    fn drop(&mut self) {
        drop(self.sender.take());
        self.worker.take().unwrap().join().unwrap();
    }
}

fn assert_send_resources(app: &SubApp) {
    for info in app.world().components().iter_registered() {
        assert!(
            info.is_send_and_sync() || app.world().get_non_send_by_id(info.id()).is_none(),
            "Render resource cannot move: {}",
            info.name()
        );
    }
}

fn palette(length: usize) -> [u8; 3] {
    [
        [255, 0, 0],
        [0, 255, 0],
        [0, 0, 255],
        [0, 255, 255],
        [255, 0, 255],
        [255, 255, 0],
    ][length % 6]
}

struct Epoch {
    number: usize,
    started: Instant,
    main: [f64; 2],
    frame: Frame,
    targets: Vec<(OutputId, Handle<Image>, [f64; 2], bool)>,
    primary: Handle<Image>,
}

fn prepare(
    canvas: &mut Presentation,
    editor: Entity,
    owner: Entity,
    original: bevy::math::DVec2,
    number: usize,
    origin: Instant,
) -> Epoch {
    assert!(canvas.windows.is_empty() && canvas.uploads.is_empty() && canvas.records.is_none());
    assert!(
        canvas
            .app
            .world()
            .get_resource::<lince_interface::host::capture::Demand>()
            .is_none_or(|demand| demand.outputs.is_empty() && demand.windows.is_empty())
    );
    let started = Instant::now();
    canvas
        .world_mut()
        .get_mut::<native::canvas::CanvasItem>(owner)
        .unwrap()
        .position = original + bevy::math::DVec2::new((number + 1) as f64 * 3.0, 0.0);
    canvas
        .key(
            KeyCode::KeyA,
            Key::Character("a".into()),
            Some("a".into()),
            true,
            false,
        )
        .unwrap();
    canvas
        .key(
            KeyCode::KeyA,
            Key::Character("a".into()),
            None,
            false,
            false,
        )
        .unwrap();
    canvas.synchronize_input_activity().unwrap();
    canvas.refresh_viewport().unwrap();
    let view = canvas.native_input_view(None);
    crate::native_input::projection::prepare(canvas.app.world_mut(), view);
    canvas.prepare_capture_targets();
    let main_start = origin.elapsed().as_secs_f64() * 1000.0;
    canvas.app.main_mut().run_default_schedule();
    let main_end = origin.elapsed().as_secs_f64() * 1000.0;
    canvas.capture_settings();
    canvas.publish_capture_policy();
    assert!(
        canvas
            .app
            .world()
            .get_resource::<lince_interface::host::capture::PolicyGate>()
            .and_then(|gate| gate.read())
            .is_none_or(|policy| !policy.requires_filter())
    );
    let geometry = canvas.frame_geometry().map(Arc::new);
    let input = Arc::new(canvas.finish_input(geometry.as_deref()).unwrap());
    let expected = "a".repeat(number + 1);
    assert_eq!(input.editor.as_ref().unwrap().surrounding, expected);
    assert_eq!(
        canvas
            .app
            .world()
            .get::<bevy::text::EditableText>(editor)
            .unwrap()
            .value(),
        expected.as_str()
    );
    Epoch {
        number,
        started,
        main: [main_start, main_end],
        primary: canvas.target.clone(),
        targets: canvas
            .output_state
            .targets
            .iter()
            .filter(|(id, _)| Some(**id) != canvas.output_state.assignments.focused())
            .map(|(id, target)| (*id, target.image.clone(), target.origin, target.fullscreen))
            .collect(),
        frame: Frame {
            client_formats: canvas.client_formats.clone(),
            encoding: canvas.native_encoding(),
            output: canvas.output_state.assignments.focused(),
            image: None,
            geometry,
            input,
            windows: canvas.window_configurations(),
            previewed: Vec::new(),
            fullscreen: canvas.render_fullscreen,
            origin: canvas.render_origin,
            outputs: Vec::new(),
            capture: None,
        },
    }
}

fn export(canvas: &Presentation, render: &SubApp, mut epoch: Epoch) -> (Frame, serde_json::Value) {
    let assets = render.world().resource::<RenderAssets<GpuImage>>();
    let mut textures = epoch
        .targets
        .iter()
        .map(|(_, image, _, _)| wgpu::Texture::clone(&assets.get(image.id()).unwrap().texture))
        .collect::<Vec<_>>();
    textures.push(wgpu::Texture::clone(
        &assets.get(epoch.primary.id()).unwrap().texture,
    ));
    let mut images = canvas.renderer.export_render_targets(&textures).unwrap();
    epoch.frame.image = images.pop();
    epoch.frame.outputs = epoch
        .targets
        .into_iter()
        .zip(images)
        .map(|((id, _, origin, fullscreen), image)| OutputFrame {
            id,
            image,
            origin,
            fullscreen,
        })
        .collect();
    epoch.frame.wait_for_probe().unwrap();
    let ready_ms = epoch.started.elapsed().as_secs_f64() * 1000.0;
    (
        epoch.frame,
        serde_json::json!({"epoch":epoch.number,"main_ms":epoch.main,"input_to_ready_ms":ready_ms}),
    )
}

#[test]
#[ignore = "private Vulkan/EGL native main/render overlap prototype; no concurrent compilation"]
fn native_epoch_pixels_remain_coherent_while_the_next_main_update_overlaps_rendering() {
    let checks = native::laboratory::run_interface_checks().unwrap();
    assert!(
        checks[0]
            .name
            .ends_with("::protein_area_filter_visible_and_selectable")
    );
    assert!(
        checks.iter().all(|result| result.error.is_none()),
        "{checks:?}"
    );
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut reports = Vec::new();
    let mut modes = ["serial", "threaded-serial", "overlap"];
    if std::env::var_os("LINCE_PARK_OVERLAP_REVERSE").is_some() {
        modes.reverse();
    }
    let mut secondary_reference = None;
    for mode in modes {
        let (mut canvas, owners) = super::offscreen::fixture(directory.path(), mode, 0);
        canvas
            .world_mut()
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                Duration::from_millis(16),
            ));
        let owner = owners[0].0;
        let world = canvas.world_mut();
        let bundle = native::sand::single_line_editor(
            "",
            world.resource::<native::theme::Typography>(),
            0,
            32,
        );
        let editor = world.spawn((bundle, ChildOf(owner))).id();
        canvas.world_mut().entity_mut(editor).insert(Node {
            position_type: PositionType::Absolute,
            left: px(4),
            top: px(48),
            width: px(60),
            height: px(28),
            ..default()
        });
        let marker = canvas
            .world_mut()
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(8),
                    top: px(4),
                    width: px(52),
                    height: px(16),
                    ..default()
                },
                BackgroundColor(Color::srgb(1.0, 0.0, 0.0)),
                ChildOf(owner),
            ))
            .id();
        let original = canvas
            .app
            .world()
            .get::<native::canvas::CanvasItem>(owner)
            .unwrap()
            .position;
        canvas.app.add_systems(
            PostUpdate,
            (move |texts: Query<&bevy::text::EditableText>,
                   mut colors: Query<&mut BackgroundColor>| {
                let length = texts.get(editor).unwrap().value().chars().count();
                let rgb = palette(length);
                colors
                    .get_mut(marker)
                    .unwrap()
                    .set_if_neq(BackgroundColor(Color::srgb(
                        f32::from(rgb[0]) / 255.0,
                        f32::from(rgb[1]) / 255.0,
                        f32::from(rgb[2]) / 255.0,
                    )));
            })
            .after(bevy::text::EditableTextSystems)
            .before(bevy::ui::UiSystems::Layout),
        );
        for _ in 0..12 {
            drop(canvas.probe_frame().unwrap());
        }
        canvas
            .world_mut()
            .resource_mut::<InputFocus>()
            .set(editor, FocusCause::Navigated);
        let item = ItemId::Native(
            canvas
                .app
                .world()
                .get::<native::layout::LayoutBox>(owner)
                .unwrap()
                .id,
        );
        assert_eq!(canvas.app.sub_apps().sub_apps.len(), 1);
        let mut render = Some(canvas.app.remove_sub_app(RenderApp).unwrap());
        assert!(canvas.app.sub_apps().sub_apps.is_empty());
        assert_send_resources(render.as_ref().unwrap());
        let origin = Instant::now();
        let before = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
        let worker =
            (mode != "serial").then(|| RenderWorker::new(origin, runtime.handle().clone()));
        let mut pending = None;
        let mut frames = Vec::new();
        let mut samples = Vec::new();
        let mut render_intervals = Vec::new();
        for number in 0..12 {
            let epoch = prepare(&mut canvas, editor, owner, original, number, origin);
            if let Some(previous) = pending.take() {
                let (app, returned, interval) = worker.as_ref().unwrap().receive();
                let previous: Epoch = previous;
                assert_eq!(previous.number, returned);
                render_intervals.push((returned, interval));
                let (frame, sample) = export(&canvas, &app, previous);
                frames.push(frame);
                samples.push(sample);
                render = Some(app);
            }
            let mut app = render.take().unwrap();
            app.extract(canvas.app.world_mut());
            canvas.app.world_mut().clear_trackers();
            if let Some(worker) = &worker {
                worker.submit(app, number);
                if mode == "overlap" {
                    pending = Some(epoch);
                } else {
                    let (app, returned, interval) = worker.receive();
                    assert_eq!(number, returned);
                    render_intervals.push((returned, interval));
                    let (frame, sample) = export(&canvas, &app, epoch);
                    frames.push(frame);
                    samples.push(sample);
                    render = Some(app);
                }
            } else {
                let start = origin.elapsed().as_secs_f64() * 1000.0;
                app.update();
                let end = origin.elapsed().as_secs_f64() * 1000.0;
                render_intervals.push((number, [start, end]));
                let (frame, sample) = export(&canvas, &app, epoch);
                frames.push(frame);
                samples.push(sample);
                render = Some(app);
            }
        }
        if let Some(epoch) = pending.take() {
            let (app, returned, interval) = worker.as_ref().unwrap().receive();
            assert_eq!(epoch.number, returned);
            render_intervals.push((returned, interval));
            let (frame, sample) = export(&canvas, &app, epoch);
            frames.push(frame);
            samples.push(sample);
            render = Some(app);
        }
        let elapsed = origin.elapsed().as_secs_f64() * 1000.0;
        let after = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
        let cpu = (after.tv_sec - before.tv_sec) as f64 * 1000.0
            + (after.tv_nsec - before.tv_nsec) as f64 / 1_000_000.0;
        assert_eq!(frames.len(), 12);
        let mut pixels = Vec::new();
        for (number, frame) in frames.iter().enumerate() {
            assert_eq!(
                frame.input.editor.as_ref().unwrap().surrounding,
                "a".repeat(number + 1)
            );
            let geometry = frame.geometry.as_ref().unwrap();
            let mapping = geometry
                .items
                .iter()
                .find(|entry| entry.id == item && entry.output == OutputId(1))
                .unwrap()
                .mapping;
            let point = mapping
                .canvas_point([
                    mapping.surface.origin()[0] + mapping.surface.size()[0] * 20.0 / 68.0,
                    mapping.surface.origin()[1] + mapping.surface.size()[1] * 12.0 / 80.0,
                ])
                .unwrap();
            let expected_point = [
                640.0 / 2.0 + original.x - 68.0 / 2.0 + 20.0 + (number + 1) as f64 * 3.0,
                480.0 / 2.0 + original.y - 80.0 / 2.0 + 12.0,
            ];
            assert!(
                point
                    .iter()
                    .zip(expected_point)
                    .all(|(actual, expected)| (actual - expected).abs() < 0.01),
                "{mode} epoch {number}: geometry {point:?}, expected {expected_point:?}"
            );
            let image = if frame.output == Some(OutputId(1)) {
                frame.dmabuf().unwrap()
            } else {
                frame
                    .outputs
                    .iter()
                    .find(|output| output.id == OutputId(1))
                    .unwrap()
                    .dmabuf()
            };
            let bytes = probe_pixels(image).unwrap();
            let offset = (point[1].round() as usize * 640 + point[0].round() as usize) * 4;
            let actual = &bytes[offset..offset + 3];
            let expected = palette(number + 1);
            assert!(
                actual
                    .iter()
                    .zip(expected)
                    .all(|(actual, expected)| actual.abs_diff(expected) < 30),
                "{mode} epoch {number} at {point:?}: {actual:?}, expected {expected:?}"
            );
            let secondary = if frame.output == Some(OutputId(2)) {
                frame.dmabuf().unwrap()
            } else {
                frame
                    .outputs
                    .iter()
                    .find(|output| output.id == OutputId(2))
                    .unwrap()
                    .dmabuf()
            };
            let secondary = probe_pixels(secondary).unwrap();
            let green = secondary
                .chunks_exact(4)
                .filter(|pixel| pixel[0] < 30 && pixel[1] > 230 && pixel[2] < 30)
                .count();
            let glyphs = (0..8)
                .flat_map(|index| {
                    (200..280).flat_map(move |y| {
                        ((34 + index * 72)..(102 + index * 72)).map(move |x| (y * 640 + x) * 4)
                    })
                })
                .filter(|offset| {
                    secondary[*offset..*offset + 3]
                        .iter()
                        .all(|channel| *channel < 180)
                })
                .count();
            assert!(
                green > 20000 && glyphs > 200,
                "{mode} epoch {number}: secondary green {green}, glyphs {glyphs}"
            );
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            std::hash::Hash::hash(&secondary, &mut hash);
            let hash = std::hash::Hasher::finish(&hash);
            assert_eq!(
                *secondary_reference.get_or_insert(hash),
                hash,
                "The quiet output changed: {mode}, epoch {number}"
            );
            pixels.push(serde_json::json!({"epoch":number,"point":point,"rgb":actual,"geometry_revision":geometry.revision,"secondary_hash":hash,"secondary_green":green,"secondary_glyphs":glyphs}));
        }
        let overlap = samples
            .windows(2)
            .map(|pair| {
                let n = pair[0]["epoch"].as_u64().unwrap() as usize;
                let r = render_intervals
                    .iter()
                    .find(|(epoch, _)| *epoch == n)
                    .unwrap()
                    .1;
                let main = pair[1]["main_ms"].as_array().unwrap();
                (r[1].min(main[1].as_f64().unwrap()) - r[0].max(main[0].as_f64().unwrap())).max(0.0)
            })
            .sum::<f64>();
        if mode == "overlap" {
            assert!(overlap > 0.0, "No main/render temporal overlap occurred");
        } else {
            assert_eq!(overlap, 0.0);
        }
        reports.push(serde_json::json!({"mode":mode,"frames":12,"wall_ms":elapsed,"process_cpu_ms":cpu,"overlapped_wall_ms":overlap,"samples":samples,"render_intervals":render_intervals,"pixels":pixels}));
        canvas.app.insert_sub_app(RenderApp, render.take().unwrap());
        drop(worker);
        canvas.finish().unwrap();
    }
    let path = std::path::PathBuf::from(
        std::env::var_os("LINCE_PARK_TEST_ARTIFACTS").expect("Set private artifacts"),
    );
    std::fs::create_dir_all(&path).unwrap();
    let result = serde_json::json!({"scope":"Private debug native-only full-scene main n+1/render n prototype, two outputs, no external clients/records/capture demand. One persistent render worker, one render job, GPU export/readiness stays on native owner and one frame at a time. Epoch input/geometry/target handles saved before advancing main. Retained-image readback follows timing. Direct per-epoch key delivery differs between serial and overlap; ready age is diagnostic and not a matched offered-load latency result. No desktop FPS, physical presentation or production pipeline claim. Filtered capture, client imports/uploads, privacy/output replacement, cancellation and production worker feedback remain required before shipping.","reports":reports});
    std::fs::write(
        path.join("native-render-overlap.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
    for report in result["reports"].as_array().unwrap() {
        eprintln!(
            "native-overlap: {}",
            serde_json::json!({"mode":report["mode"],"wall_ms":report["wall_ms"],"process_cpu_ms":report["process_cpu_ms"],"overlapped_wall_ms":report["overlapped_wall_ms"]})
        );
    }
}
