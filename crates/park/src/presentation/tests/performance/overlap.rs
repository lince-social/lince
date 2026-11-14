use super::*;

mod consumer;
mod offered;
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
    sender: Option<SyncSender<(SubApp, usize, Option<Epoch>)>>,
    receiver: Receiver<(SubApp, usize, [f64; 2])>,
    worker: Option<JoinHandle<()>>,
}

impl RenderWorker {
    fn new(
        origin: Instant,
        runtime: tokio::runtime::Handle,
        exporter: Option<(Arc<crate::graphics::GpuContext>, SyncSender<Publication>)>,
        notification: Option<SyncSender<offered::Signal>>,
    ) -> Self {
        let (sender, incoming) = sync_channel::<(SubApp, usize, Option<Epoch>)>(1);
        let (outgoing, receiver) = sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("park-overlap-probe".into())
            .stack_size(32 * 1024 * 1024)
            .spawn(move || {
                let _entered = runtime.enter();
                while let Ok((mut app, number, epoch)) = incoming.recv() {
                    let start = origin.elapsed().as_secs_f64() * 1000.0;
                    app.update();
                    let end = origin.elapsed().as_secs_f64() * 1000.0;
                    if let Some(epoch) = epoch {
                        let (gpu, publisher) = exporter.as_ref().unwrap();
                        if publisher.send(export(gpu, &app, epoch, origin)).is_err() {
                            break;
                        }
                    }
                    if outgoing.send((app, number, [start, end])).is_err() {
                        break;
                    }
                    if let Some(notification) = &notification
                        && notification.send(offered::Signal::Rendered).is_err()
                    {
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

    fn submit(&self, app: SubApp, number: usize, epoch: Option<Epoch>) {
        assert_send_resources(&app);
        self.sender
            .as_ref()
            .unwrap()
            .send((app, number, epoch))
            .unwrap();
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
        if let Err(error) = self.worker.take().unwrap().join()
            && !std::thread::panicking()
        {
            std::panic::resume_unwind(error);
        }
    }
}

type Publication = (Frame, serde_json::Value);

struct Publisher {
    sender: Option<SyncSender<Publication>>,
    worker: Option<JoinHandle<Vec<Publication>>>,
}

impl Publisher {
    fn new(origin: Instant) -> Self {
        Self::start(origin, None)
    }

    fn consuming(origin: Instant, warm: Frame, item: ItemId, original: bevy::math::DVec2) -> Self {
        Self::start(origin, Some((warm, item, original)))
    }

    fn start(origin: Instant, consume: Option<(Frame, ItemId, bevy::math::DVec2)>) -> Self {
        let (sender, incoming) = sync_channel::<Publication>(1);
        let (ready, initialized) = sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("park-publication-probe".into())
            .spawn(move || {
                let mut consumer = consume
                    .map(|(warm, item, original)| consumer::Consumer::new(&warm, item, original));
                ready.send(()).unwrap();
                let mut received = Vec::new();
                while let Ok((frame, mut sample)) = incoming.recv() {
                    sample["published_ms"] =
                        serde_json::json!(origin.elapsed().as_secs_f64() * 1000.0);
                    assert!(frame.ready().unwrap());
                    if let Some(consumer) = consumer.as_mut() {
                        sample["gles"] = consumer.consume(&frame, origin);
                    }
                    received.push((frame, sample));
                }
                if let Some(consumer) = consumer.as_mut() {
                    let (frame, sample) = received.last_mut().unwrap();
                    sample["consumer_finished_ms"] =
                        serde_json::json!(origin.elapsed().as_secs_f64() * 1000.0);
                    sample["consumer_finished_cpu_ns"] =
                        serde_json::json!(consumer::process_cpu_ns());
                    sample["gles_pixels"] = consumer.check(frame, sample);
                }
                received
            })
            .unwrap();
        initialized
            .recv_timeout(Duration::from_secs(30))
            .expect("Publication consumer did not initialize");
        Self {
            sender: Some(sender),
            worker: Some(worker),
        }
    }

    fn sender(&self) -> SyncSender<Publication> {
        self.sender.as_ref().unwrap().clone()
    }

    fn finish(mut self) -> Vec<Publication> {
        drop(self.sender.take());
        self.worker.take().unwrap().join().unwrap()
    }
}

impl Drop for Publisher {
    fn drop(&mut self) {
        drop(self.sender.take());
        if let Some(worker) = self.worker.take()
            && let Err(error) = worker.join()
            && !std::thread::panicking()
        {
            std::panic::resume_unwind(error);
        }
    }
}

enum Pending {
    Native(Epoch),
    Exported(usize),
}

fn receive(
    worker: &RenderWorker,
    pending: Pending,
    gpu: &crate::graphics::GpuContext,
    publisher: &Publisher,
    origin: Instant,
    intervals: &mut Vec<(usize, [f64; 2])>,
) -> SubApp {
    let (app, number, interval) = worker.receive();
    intervals.push((number, interval));
    match pending {
        Pending::Native(epoch) => {
            assert_eq!(epoch.number, number);
            publisher
                .sender()
                .send(export(gpu, &app, epoch, origin))
                .unwrap();
        }
        Pending::Exported(expected) => assert_eq!(number, expected),
    }
    app
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
    first: usize,
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
    prepare_inputs(canvas, editor, owner, original, number, number, 3.0, origin)
}

fn prepare_inputs(
    canvas: &mut Presentation,
    editor: Entity,
    owner: Entity,
    original: bevy::math::DVec2,
    first: usize,
    number: usize,
    movement: f64,
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
        .position = original + bevy::math::DVec2::new((number + 1) as f64 * movement, 0.0);
    for _ in first..=number {
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
    }
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
        first,
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

fn export(
    gpu: &crate::graphics::GpuContext,
    render: &SubApp,
    mut epoch: Epoch,
    origin: Instant,
) -> Publication {
    let export_start = origin.elapsed().as_secs_f64() * 1000.0;
    let assets = render.world().resource::<RenderAssets<GpuImage>>();
    let mut textures = epoch
        .targets
        .iter()
        .map(|(_, image, _, _)| wgpu::Texture::clone(&assets.get(image.id()).unwrap().texture))
        .collect::<Vec<_>>();
    textures.push(wgpu::Texture::clone(
        &assets.get(epoch.primary.id()).unwrap().texture,
    ));
    let mut images = gpu.export_render_targets(&textures).unwrap();
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
        serde_json::json!({"epoch":epoch.number,"first":epoch.first,"main_ms":epoch.main,"input_to_ready_ms":ready_ms,"export_ms":[export_start,origin.elapsed().as_secs_f64()*1000.0]}),
    )
}

struct Scene {
    canvas: Presentation,
    owner: Entity,
    editor: Entity,
    original: bevy::math::DVec2,
    item: ItemId,
}

fn scene(directory: &std::path::Path, mode: &str) -> Scene {
    let (mut canvas, owners) = super::offscreen::fixture(directory, mode, 0);
    canvas
        .world_mut()
        .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            Duration::from_millis(16),
        ));
    let owner = owners[0].0;
    let world = canvas.world_mut();
    let bundle =
        native::sand::single_line_editor("", world.resource::<native::theme::Typography>(), 0, 32);
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
    Scene {
        canvas,
        owner,
        editor,
        original,
        item,
    }
}

fn check_pixels(
    frames: &[Frame],
    samples: &[serde_json::Value],
    item: ItemId,
    original: bevy::math::DVec2,
    movement: f64,
    mode: &str,
    secondary_reference: &mut Option<u64>,
) -> Vec<serde_json::Value> {
    frames
        .iter()
        .zip(samples)
        .map(|(frame, sample)| {
            let primary = probe_pixels(output_buffer(frame, OutputId(1))).unwrap();
            let secondary = probe_pixels(output_buffer(frame, OutputId(2))).unwrap();
            check_frame_pixels(
                frame,
                sample,
                item,
                original,
                movement,
                mode,
                secondary_reference,
                [&primary, &secondary],
            )
        })
        .collect()
}

fn output_buffer(frame: &Frame, id: OutputId) -> &smithay::backend::allocator::dmabuf::Dmabuf {
    if frame.output == Some(id) {
        frame.dmabuf().unwrap()
    } else {
        frame
            .outputs
            .iter()
            .find(|output| output.id == id)
            .unwrap()
            .dmabuf()
    }
}

fn check_frame_pixels(
    frame: &Frame,
    sample: &serde_json::Value,
    item: ItemId,
    original: bevy::math::DVec2,
    movement: f64,
    mode: &str,
    secondary_reference: &mut Option<u64>,
    [bytes, secondary]: [&[u8]; 2],
) -> serde_json::Value {
    let number = sample["epoch"].as_u64().unwrap() as usize;
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
        640.0 / 2.0 + original.x - 68.0 / 2.0 + 20.0 + (number + 1) as f64 * movement,
        480.0 / 2.0 + original.y - 80.0 / 2.0 + 12.0,
    ];
    assert!(
        point
            .iter()
            .zip(expected_point)
            .all(|(actual, expected)| (actual - expected).abs() < 0.01),
        "{mode} epoch {number}: geometry {point:?}, expected {expected_point:?}"
    );
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
    serde_json::json!({"epoch":number,"point":point,"rgb":actual,"geometry_revision":geometry.revision,"secondary_hash":hash,"secondary_green":green,"secondary_glyphs":glyphs})
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
    let mut modes = [
        "serial",
        "threaded-serial",
        "overlap",
        "overlap-worker-export",
    ];
    if std::env::var_os("LINCE_PARK_OVERLAP_REVERSE").is_some() {
        modes.reverse();
    }
    let mut secondary_reference = None;
    for mode in modes {
        let Scene {
            mut canvas,
            owner,
            editor,
            original,
            item,
        } = scene(directory.path(), mode);
        assert_eq!(canvas.app.sub_apps().sub_apps.len(), 1);
        let mut render = Some(canvas.app.remove_sub_app(RenderApp).unwrap());
        assert!(canvas.app.sub_apps().sub_apps.is_empty());
        assert_send_resources(render.as_ref().unwrap());
        let origin = Instant::now();
        let before = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
        let publisher = Publisher::new(origin);
        let worker = (mode != "serial").then(|| {
            RenderWorker::new(
                origin,
                runtime.handle().clone(),
                (mode == "overlap-worker-export")
                    .then(|| (canvas.renderer.gpu.clone(), publisher.sender())),
                None,
            )
        });
        let mut pending = None;
        let mut render_intervals = Vec::new();
        for number in 0..12 {
            let epoch = prepare(&mut canvas, editor, owner, original, number, origin);
            if let Some(previous) = pending.take() {
                render = Some(receive(
                    worker.as_ref().unwrap(),
                    previous,
                    &canvas.renderer.gpu,
                    &publisher,
                    origin,
                    &mut render_intervals,
                ));
            }
            let mut app = render.take().unwrap();
            app.extract(canvas.app.world_mut());
            canvas.app.world_mut().clear_trackers();
            if let Some(worker) = &worker {
                if mode == "overlap-worker-export" {
                    worker.submit(app, number, Some(epoch));
                    pending = Some(Pending::Exported(number));
                } else {
                    worker.submit(app, number, None);
                    if mode == "overlap" {
                        pending = Some(Pending::Native(epoch));
                    } else {
                        render = Some(receive(
                            worker,
                            Pending::Native(epoch),
                            &canvas.renderer.gpu,
                            &publisher,
                            origin,
                            &mut render_intervals,
                        ));
                    }
                }
            } else {
                let start = origin.elapsed().as_secs_f64() * 1000.0;
                app.update();
                let end = origin.elapsed().as_secs_f64() * 1000.0;
                render_intervals.push((number, [start, end]));
                publisher
                    .sender()
                    .send(export(&canvas.renderer.gpu, &app, epoch, origin))
                    .unwrap();
                render = Some(app);
            }
        }
        if let Some(epoch) = pending.take() {
            render = Some(receive(
                worker.as_ref().unwrap(),
                epoch,
                &canvas.renderer.gpu,
                &publisher,
                origin,
                &mut render_intervals,
            ));
        }
        drop(worker);
        let (frames, samples): (Vec<_>, Vec<_>) = publisher.finish().into_iter().unzip();
        let elapsed = origin.elapsed().as_secs_f64() * 1000.0;
        let after = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
        let cpu = (after.tv_sec - before.tv_sec) as f64 * 1000.0
            + (after.tv_nsec - before.tv_nsec) as f64 / 1_000_000.0;
        assert_eq!(frames.len(), 12);
        let pixels = check_pixels(
            &frames,
            &samples,
            item,
            original,
            3.0,
            mode,
            &mut secondary_reference,
        );
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
        if mode.starts_with("overlap") {
            assert!(overlap > 0.0, "No main/render temporal overlap occurred");
        } else {
            assert_eq!(overlap, 0.0);
        }
        let publications_during_next_main = samples
            .windows(2)
            .filter(|pair| {
                let published = pair[0]["published_ms"].as_f64().unwrap();
                let main = pair[1]["main_ms"].as_array().unwrap();
                published >= main[0].as_f64().unwrap() && published < main[1].as_f64().unwrap()
            })
            .count();
        if mode == "overlap-worker-export" {
            assert!(
                publications_during_next_main > 0,
                "No ready frame was published during the next main update"
            );
        }
        reports.push(serde_json::json!({"mode":mode,"frames":12,"wall_ms":elapsed,"process_cpu_ms":cpu,"overlapped_wall_ms":overlap,"publications_during_next_main":publications_during_next_main,"samples":samples,"render_intervals":render_intervals,"pixels":pixels}));
        canvas.app.insert_sub_app(RenderApp, render.take().unwrap());
        canvas.finish().unwrap();
    }
    let path = std::path::PathBuf::from(
        std::env::var_os("LINCE_PARK_TEST_ARTIFACTS").expect("Set private artifacts"),
    );
    std::fs::create_dir_all(&path).unwrap();
    let result = serde_json::json!({"scope":"Private debug native-only full-scene main n+1/render n prototype, two outputs, no external clients/records/capture demand. One persistent render worker, one render job and one GPU frame at a time. Compare native-owner export with worker-owned export/readiness through the same bounded publication consumer. Immutable GPU context shares existing device/queue/completion; thread-local import reactor stays native. Epoch input/geometry/target handles saved before advancing main. Retained-image readback follows timing. Direct per-epoch key delivery differs between serial and overlap; ready age is diagnostic and not a matched offered-load latency result. No desktop FPS, physical presentation or production pipeline claim. Filtered capture, client imports/uploads, privacy/output replacement, cancellation and production worker feedback remain required before shipping.","reports":reports});
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
