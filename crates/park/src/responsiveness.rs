use crate::presentation::{Frame, Output, Presentation};
use bevy::{
    input::keyboard::{Key, KeyCode},
    input_focus::InputFocus,
    math::DVec2,
    prelude::*,
};
use lince_interface::host::performance::{self, Phase};
use lince_interface::{
    host::{DesktopAction, OutputId, OutputViewport, Rectangle, WorkspaceLayout},
    native::{self, app::CellHandle},
};
use serde_json::{Value, json};
use std::{
    io::{self, BufRead},
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

fn error(value: impl std::fmt::Display) -> io::Error {
    io::Error::other(value.to_string())
}

fn completed(canvas: &mut Presentation) -> io::Result<Frame> {
    let frame = canvas.frame().map_err(error)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while !frame.ready().map_err(error)? {
        if Instant::now() >= deadline {
            return Err(error("GPU completion timed out"));
        }
        std::thread::sleep(Duration::from_micros(100));
    }
    if frame.dmabuf().is_none() || frame.geometry.is_none() {
        return Err(error("A completed native frame has no image or geometry"));
    }
    Ok(frame)
}

pub(crate) fn input_point(canvas: &mut Presentation, entity: Entity) -> io::Result<[f32; 2]> {
    let (output, _) = canvas
        .native_test_points(entity)
        .ok_or_else(|| error("Native control is outside presented geometry"))?;
    canvas.focus_output(output).map_err(error)?;
    let frame = completed(canvas)?;
    let (_, corners) = canvas
        .native_test_points(entity)
        .ok_or_else(|| error("Native control disappeared during output focus"))?;
    let center: [f64; 2] =
        std::array::from_fn(|axis| corners.iter().map(|point| point[axis]).sum::<f64>() / 4.0);
    let point = std::iter::once(center)
        .chain(corners.map(|corner| {
            std::array::from_fn(|axis| center[axis] + (corner[axis] - center[axis]) * 0.75)
        }))
        .find(|point| {
            frame
                .input
                .mappings
                .get(output)
                .is_some_and(|mapping| mapping.logical_bounds().contains(*point))
                && frame
                    .input
                    .native_pointer
                    .target(*point)
                    .is_some_and(|target| target.contains(entity))
        })
        .ok_or_else(|| error("Native control has no visible pointer target"))?;
    let viewport = frame
        .geometry
        .as_ref()
        .and_then(|geometry| {
            geometry
                .outputs
                .iter()
                .find(|viewport| viewport.id == output)
        })
        .ok_or_else(|| error("Native control output has no published viewport"))?;
    Ok(std::array::from_fn(|axis| {
        (point[axis] - viewport.bounds.origin()[axis] - frame.origin[axis]) as f32
    }))
}

fn summary(samples: &[f64]) -> Value {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let percentile = |p: usize| sorted[(sorted.len() * p).div_ceil(100).saturating_sub(1)];
    json!({"count":sorted.len(), "p50_ms":percentile(50), "p95_ms":percentile(95), "p99_ms":percentile(99), "worst_ms":sorted.last(), "samples_ms":samples})
}

fn passes(samples: &[f64]) -> bool {
    if samples.len() < 100
        || samples
            .iter()
            .any(|sample| !sample.is_finite() || *sample < 0.0)
    {
        return false;
    }
    let summary = summary(samples);
    summary["p95_ms"].as_f64().unwrap() <= 33.4
        && summary["p99_ms"].as_f64().unwrap() <= 50.0
        && summary["worst_ms"].as_f64().unwrap() <= 100.0
}

fn measurement_coverage(reports: &[Value]) -> Value {
    let phases = [
        "warmup",
        "open-reveal",
        "typing",
        "scroll",
        "active",
        "pan",
        "quiet",
    ];
    let missing_phases: Vec<_> = phases
        .into_iter()
        .filter(|phase| !reports.iter().any(|report| report["phase"] == *phase))
        .collect();
    let missing_bodies: Vec<_> = ["open-reveal", "typing", "scroll", "active", "pan"]
        .into_iter()
        .filter(|phase| {
            !reports.iter().any(|report| {
                report["phase"] == *phase
                    && report["systems"].as_array().is_some_and(|systems| {
                        systems.iter().any(|system| {
                            system["exclusive_owner_cpu"]["count"]
                                .as_u64()
                                .is_some_and(|count| count > 0)
                        })
                    })
            })
        })
        .collect();
    let missing_stages: Vec<_> = [
        "native-main",
        "native-phase-post-update-owner-cpu",
        "gpu-submission-to-completion",
        "cell-query-execute-await",
    ]
    .into_iter()
    .filter(|stage| {
        !reports.iter().any(|report| {
            report["stages"].as_array().is_some_and(|stages| {
                stages.iter().any(|reading| {
                    reading["stage"] == *stage
                        && reading["count"].as_u64().is_some_and(|count| count > 0)
                })
            })
        })
    })
    .collect();
    let hardware_gpu = reports.iter().any(|report| {
        report["render_diagnostics"]
            .as_array()
            .is_some_and(|readings| {
                readings.iter().any(|reading| {
                    reading["path"]
                        .as_str()
                        .is_some_and(|path| path.ends_with("/elapsed_gpu"))
                        && reading["count"].as_u64().is_some_and(|count| count > 0)
                })
            })
    });
    let mut loss = serde_json::Map::new();
    for name in [
        "dropped_reports",
        "dropped_system_samples",
        "dropped_span_trace",
        "dropped_input_trace",
    ] {
        let total: Option<u64> = reports
            .iter()
            .try_fold(0u64, |sum, report| sum.checked_add(report[name].as_u64()?));
        loss.insert(name.into(), json!(total));
    }
    let final_report = reports
        .last()
        .is_some_and(|report| report["final_report"] == true);
    let passed = !reports.is_empty()
        && final_report
        && missing_phases.is_empty()
        && missing_bodies.is_empty()
        && missing_stages.is_empty()
        && hardware_gpu
        && loss.values().all(|value| value == 0);
    json!({"passed":passed,"reports":reports.len(),"final_report":final_report,"missing_phases":missing_phases,"missing_system_bodies":missing_bodies,"missing_stages":missing_stages,"hardware_gpu_execution":hardware_gpu,"loss":loss})
}

fn measurements() -> io::Result<Value> {
    let path = std::env::var_os("LINCE_PERFORMANCE_LOG")
        .ok_or_else(|| error("No retained measurement file"))?;
    let mut reports = Vec::new();
    for line in io::BufReader::new(std::fs::File::open(path)?).lines() {
        let line = line?;
        let record = line
            .strip_prefix("Lince performance ")
            .ok_or_else(|| error("Malformed performance record"))?;
        let value: Value = serde_json::from_str(record).map_err(error)?;
        if value["process"] == std::process::id() {
            reports.push(value);
        }
    }
    Ok(measurement_coverage(&reports))
}

fn output(id: u64, workspace: u64, size: [u32; 2], x: f64) -> Output {
    let bounds = Rectangle::new([x, 0.0], size.map(f64::from)).unwrap();
    Output {
        name: format!("private-{id}"),
        viewport: OutputViewport {
            id: OutputId(id),
            bounds,
            usable: Some(bounds),
            physical_size: size,
            workspace,
            raster: lince_interface::host::raster::RasterMapping {
                origin: [x, 0.0],
                scale: [1.0; 2],
            },
            rotation: Default::default(),
        },
    }
}

fn cell(engine: Arc<engine::Engine>, directory: &Path) -> CellHandle {
    CellHandle(cell::CellRuntime {
        commands: cell::terminal::commands::CommandHost::new(directory.to_path_buf()),
        store: engine.store.clone(),
        engine,
        lanes: Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        information: None,
        fiote: None,
        speech: None,
    })
}

fn interface() -> io::Result<Value> {
    let mut run = native::laboratory::BehaviorRun::start_interface(None)?;
    let deadline = Instant::now() + Duration::from_secs(90);
    while !run.finished() {
        run.poll();
        if Instant::now() >= deadline {
            run.cancel();
            return Err(error("Mandatory interface correctness suite timed out"));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let passed = !run.results.is_empty()
        && run.results.len() == run.total
        && run.results[0]
            .name
            .ends_with("::protein_area_filter_visible_and_selectable")
        && run.results.iter().all(|result| result.error.is_none());
    Ok(json!({"case":"protein-first-interface", "passed":passed, "results":run.results}))
}

fn scene(
    runtime: &tokio::runtime::Runtime,
    name: &str,
    hidden: bool,
    mixed: bool,
) -> io::Result<Value> {
    performance::set_phase(Phase::Warmup);
    let directory = tempfile::tempdir()?;
    let database = directory.path().join("lince.db");
    let engine = Arc::new(
        runtime
            .block_on(engine::Engine::open(&format!(
                "sqlite://{}",
                database.display()
            )))
            .map_err(error)?,
    );
    if hidden {
        eprintln!("Preparing 300 synthetic Records");
        runtime
            .block_on(async {
                let body =
                    "**Private fixture**: rich text, Unicode café 日本語, and local data.\n\n"
                        .repeat(100);
                let first = store::records::create(
                    &engine.store.pool,
                    store::records::NewRecord {
                        slug: None,
                        kind: nucleus::RecordKind::Plain,
                        head: "Synthetic row 0",
                        body: &body,
                        quantity: store::exact::zero(),
                    },
                )
                .await?;
                let organ = first.organ_uid.unwrap();
                let mut transaction = store::write_tx(&engine.store.pool).await?;
                for index in 1..300 {
                    store::records::create_with_uid_on(
                        &mut transaction,
                        &nucleus::new_uid("r"),
                        store::records::NewRecord {
                            slug: None,
                            kind: nucleus::RecordKind::Plain,
                            head: &format!("Synthetic row {index}"),
                            body: &body,
                            quantity: store::exact::zero(),
                        },
                        &organ,
                        None,
                    )
                    .await?;
                }
                transaction.commit().await?;
                Ok::<_, store::StoreError>(())
            })
            .map_err(error)?;
        eprintln!("Synthetic Records ready");
    }
    let _entered = runtime.enter();
    let _observer = crate::presentation::observe_systems();
    let cold = Instant::now();
    let mut canvas = Presentation::new_with_cell(
        [1920, 1080],
        directory.path().join("assets"),
        native::wake::WakeSignal::new(|| {}),
        Some((
            cell(engine.clone(), directory.path()),
            directory.path().to_path_buf(),
        )),
    )
    .map_err(error)?;
    completed(&mut canvas)?;
    let root = {
        let world = canvas.world_mut();
        world
            .query_filtered::<Entity, With<native::workspace::Workspaces>>()
            .iter(world)
            .next()
            .ok_or_else(|| error("Missing canonical canvas"))?
    };
    native::workspace::create(canvas.world_mut(), root);
    native::workspace::create(canvas.world_mut(), root);
    canvas
        .outputs(vec![
            output(1, 1, [1366, 768], 0.0),
            output(2, 2, [1920, 1080], 1366.0),
        ])
        .map_err(error)?;
    canvas
        .move_workspace_to_output(1, OutputId(1))
        .map_err(error)?;
    canvas
        .move_workspace_to_output(2, OutputId(2))
        .map_err(error)?;
    canvas.focus_output(OutputId(2)).map_err(error)?;
    let configured = completed(&mut canvas)?;
    let assignments: std::collections::BTreeMap<_, _> = configured
        .geometry
        .as_ref()
        .unwrap()
        .outputs
        .iter()
        .map(|output| (output.id, output.workspace))
        .collect();
    if assignments.get(&OutputId(1)) != Some(&1) || assignments.get(&OutputId(2)) != Some(&2) {
        return Err(error(
            "Private output fixture has incorrect workspace assignments",
        ));
    }
    canvas
        .desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free))
        .map_err(error)?;
    let mut inactive = Vec::new();
    if hidden {
        for index in 0..8 {
            let mut config = native::protein_area::Config::records();
            config.draft.query["limit"] = json!(300);
            let area = native::area::InfluenceArea {
                name: format!("Synthetic inactive area {index}"),
                protein: Some(config),
                ..native::area::InfluenceArea::new(
                    native::area::AreaShape::Square,
                    DVec2::ZERO,
                    DVec2::splat(800.0),
                )
            };
            inactive.push(
                native::area::spawn_area(canvas.world_mut(), root, 3, area)
                    .ok_or_else(|| error("Invalid inactive area fixture"))?,
            );
        }
    }
    let mut text_item = None;
    let mut scroll_item = None;
    if mixed {
        let kinds = [
            native::sand_store::SandKind::Text,
            native::sand_store::SandKind::EditableText,
            native::sand_store::SandKind::Square,
            native::sand_store::SandKind::WorkTimer,
            native::sand_store::SandKind::Operation,
            native::sand_store::SandKind::AccessControl,
            native::sand_store::SandKind::Visibility,
            native::sand_store::SandKind::Trust,
            native::sand_store::SandKind::Sync,
            native::sand_store::SandKind::Configuration,
            native::sand_store::SandKind::Organ,
            native::sand_store::SandKind::Todo,
            native::sand_store::SandKind::Ontology,
        ];
        for index in 0..30 {
            let kind = kinds[index % kinds.len()];
            let item = native::sand_store::spawn_sand(
                canvas.world_mut(),
                root,
                1 + index as u64 / 15,
                kind,
                if kind == native::sand_store::SandKind::EditableText {
                    ""
                } else {
                    "Synthetic native content"
                },
                DVec2::new(
                    (index % 5) as f64 * 230.0 - 460.0,
                    (index % 15 / 5) as f64 * 210.0 - 210.0,
                ),
            );
            canvas
                .world_mut()
                .get_mut::<native::canvas::CanvasItem>(item)
                .unwrap()
                .size = Vec2::new(210.0, 180.0);
            let mut layout = native::layout::LayoutBox::new(Vec2::new(210.0, 180.0));
            layout.rules = lince_interface::layout::Rules::fixed([210.0, 180.0]);
            canvas.world_mut().entity_mut(item).insert(layout);
            if kind == native::sand_store::SandKind::EditableText && index >= 15 {
                text_item = Some(item);
            }
            if kind == native::sand_store::SandKind::Square && index >= 15 {
                native::scroll_sand::attach(canvas.world_mut(), item);
                canvas.world_mut().spawn((
                    Node {
                        width: percent(100),
                        height: px(2400),
                        min_height: px(2400),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    Text::new("Synthetic scroll content\n".repeat(80)),
                    TextFont::from_font_size(16.0),
                    TextColor(Color::WHITE),
                    ChildOf(item),
                ));
                scroll_item = Some(item);
            }
        }
    }
    for _ in 0..16 {
        completed(&mut canvas)?;
        std::thread::sleep(Duration::from_millis(10));
    }
    let ready_deadline = Instant::now() + Duration::from_secs(30);
    while !inactive.iter().all(|owner| {
        native::protein_area::projection_stats(canvas.world_mut(), *owner)
            .is_some_and(|stats| stats.ready && stats.records >= 300)
    }) {
        if Instant::now() >= ready_deadline {
            return Err(error(
                "Inactive query workload did not finish loading; refusing an empty fast fixture",
            ));
        }
        completed(&mut canvas)?;
        std::thread::sleep(Duration::from_millis(10));
    }
    if inactive.iter().any(|owner| {
        native::protein_area::projection_stats(canvas.world_mut(), *owner)
            .is_none_or(|stats| stats.materialized != 0)
    }) {
        return Err(error(
            "Inactive Protein data unexpectedly materialized native row trees",
        ));
    }
    let cold_ms = cold.elapsed().as_secs_f64() * 1000.0;
    performance::set_phase(Phase::OpenReveal);
    let mut samples = Vec::with_capacity(120);
    for _ in 0..120 {
        let expected = !canvas
            .world_mut()
            .get::<native::edit_mode::EditMode>(root)
            .is_some_and(|mode| mode.enabled);
        let start = Instant::now();
        canvas
            .desktop_action(DesktopAction::ToggleEdit)
            .map_err(error)?;
        let frame = completed(&mut canvas)?;
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
        if canvas
            .world_mut()
            .get::<native::edit_mode::EditMode>(root)
            .is_none_or(|mode| mode.enabled != expected)
            || frame.outputs.len() + usize::from(frame.output.is_some()) != 2
        {
            return Err(error("Edit toggle or two-output publication oracle failed"));
        }
    }
    let mut typing = Vec::new();
    if let Some(item) = text_item {
        let editor = {
            let world = canvas.world_mut();
            world
                .query::<(Entity, &bevy::text::EditableText)>()
                .iter(world)
                .find_map(|(entity, _)| {
                    let mut current = entity;
                    loop {
                        if current == item {
                            return Some(entity);
                        }
                        current = world.get::<ChildOf>(current)?.parent();
                    }
                })
                .ok_or_else(|| error("Mixed fixture has no native text editor"))?
        };
        canvas.focus(true).map_err(error)?;
        let center = input_point(&mut canvas, editor)?;
        canvas.pointer(center).map_err(error)?;
        completed(&mut canvas)?;
        canvas
            .button(bevy::input::mouse::MouseButton::Left, true)
            .map_err(error)?;
        completed(&mut canvas)?;
        canvas
            .button(bevy::input::mouse::MouseButton::Left, false)
            .map_err(error)?;
        completed(&mut canvas)?;
        if canvas.world_mut().resource::<InputFocus>().get() != Some(editor) {
            return Err(error(
                "Pointer click did not focus the presented native editor",
            ));
        }
        performance::set_phase(Phase::Typing);
        let mut expected = canvas
            .world_mut()
            .get::<bevy::text::EditableText>(editor)
            .unwrap()
            .value()
            .to_string();
        for _ in 0..120 {
            let start = Instant::now();
            canvas
                .key(
                    KeyCode::KeyA,
                    Key::Character("é".into()),
                    Some("é".into()),
                    true,
                    false,
                )
                .map_err(error)?;
            canvas
                .key(
                    KeyCode::KeyA,
                    Key::Character("é".into()),
                    None,
                    false,
                    false,
                )
                .map_err(error)?;
            completed(&mut canvas)?;
            typing.push(start.elapsed().as_secs_f64() * 1000.0);
            expected.push('é');
            if canvas
                .world_mut()
                .get::<bevy::text::EditableText>(editor)
                .unwrap()
                .value()
                .to_string()
                != expected
            {
                return Err(error(
                    "Native text input lost, duplicated, or reordered a character",
                ));
            }
            engine.notify_query_changed();
        }
    }
    performance::set_phase(Phase::Scroll);
    let mut scrolling = Vec::new();
    if let Some(item) = scroll_item {
        let center = input_point(&mut canvas, item)?;
        canvas.pointer(center).map_err(error)?;
        completed(&mut canvas)?;
        let initial = canvas.world_mut().get::<ScrollPosition>(item).unwrap().0.y;
        for index in 0..120 {
            let started = Instant::now();
            canvas
                .scroll(
                    [0.0, if index % 2 == 0 { -24.0 } else { 24.0 }],
                    bevy::input::mouse::MouseScrollUnit::Pixel,
                    bevy::input::touch::TouchPhase::Moved,
                )
                .map_err(error)?;
            completed(&mut canvas)?;
            scrolling.push(started.elapsed().as_secs_f64() * 1000.0);
            let position = canvas.world_mut().get::<ScrollPosition>(item).unwrap().0.y;
            if (index % 2 == 0 && position <= initial)
                || (index % 2 == 1 && (position - initial).abs() > 0.5)
            {
                return Err(error(
                    "Pointer-directed scrolling did not update and restore its native viewport",
                ));
            }
        }
    }
    performance::set_phase(Phase::Active);
    let mut dragging = Vec::new();
    if let Some(item) = text_item {
        use crate::presentation::manipulation::{Active, Event, Request};
        let center = input_point(&mut canvas, item)?;
        let target =
            native::layout::desktop::manipulation::pointer::targets(canvas.world_mut(), root)
                .into_iter()
                .find(|target| target.entity == item)
                .ok_or_else(|| error("Native item has no published manipulation target"))?;
        let original = native::canvas::presented(canvas.world_mut(), item).unwrap();
        canvas
            .manipulation(Event::Begin(Request {
                active: Active { token: 1, target },
                operation: lince_interface::host::Manipulation::Move,
                edges: None,
                start: center,
                current: center,
            }))
            .map_err(error)?;
        completed(&mut canvas)?;
        let mut last = original.position;
        for index in 1..=120 {
            let started = Instant::now();
            canvas
                .pointer([center[0] + index as f32 * 0.5, center[1]])
                .map_err(error)?;
            completed(&mut canvas)?;
            dragging.push(started.elapsed().as_secs_f64() * 1000.0);
            let position = native::canvas::presented(canvas.world_mut(), item)
                .unwrap()
                .position;
            if !position.is_finite() || position.distance(last) < 0.01 {
                return Err(error("Native drag did not produce current geometry"));
            }
            last = position;
        }
        canvas
            .manipulation(Event::End {
                token: 1,
                position: [center[0] + 60.0, center[1]],
                commit: true,
            })
            .map_err(error)?;
        completed(&mut canvas)?;
        if canvas
            .world_mut()
            .get::<native::canvas::CanvasItem>(item)
            .unwrap()
            .position
            .distance(last)
            > 0.1
        {
            return Err(error("Native drag did not commit its displayed position"));
        }
    }
    performance::set_phase(Phase::Pan);
    let mut motion = Vec::new();
    if mixed {
        let original = *canvas
            .world_mut()
            .get::<native::canvas::CanvasView>(root)
            .ok_or_else(|| error("Canvas has no view"))?;
        for index in 0..120 {
            let started = Instant::now();
            canvas
                .world_mut()
                .get_mut::<native::canvas::CanvasView>(root)
                .unwrap()
                .center = original.center + DVec2::new(index as f64, 0.0);
            completed(&mut canvas)?;
            motion.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        *canvas
            .world_mut()
            .get_mut::<native::canvas::CanvasView>(root)
            .unwrap() = original;
        completed(&mut canvas)?;
    }
    let motion_gate = !mixed
        || (passes(&motion)
            && motion.iter().filter(|ms| **ms > 1000.0 / 60.0).count() as f64
                / motion.len() as f64
                <= 0.01);
    let passed = passes(&samples)
        && (!mixed || (passes(&typing) && passes(&scrolling) && passes(&dragging)))
        && motion_gate;
    let result = json!({"case":name, "passed":passed, "scope":"native-owner-to-completed-GPU-frame; not physical scanout", "cold_ms":cold_ms, "edit":summary(&samples), "typing":(!typing.is_empty()).then(|| summary(&typing)), "scrolling":(!scrolling.is_empty()).then(|| summary(&scrolling)), "dragging":(!dragging.is_empty()).then(|| summary(&dragging)), "motion":(!motion.is_empty()).then(|| summary(&motion)), "motion_scope":"native transform to GPU completion; 60 Hz render budget, not physical presentation", "outputs":[[1366,768],[1920,1080]], "inactive_areas":if hidden {8} else {0}, "native_items":if mixed {30} else {0}});
    drop(canvas);
    Ok(result)
}

fn worker_idle(runtime: &tokio::runtime::Runtime) -> io::Result<Value> {
    use crate::runtime::{CanvasWorker, Configuration, Input};
    performance::set_phase(Phase::Warmup);
    let directory = tempfile::tempdir()?;
    let engine = Arc::new(
        runtime
            .block_on(engine::Engine::open(&format!(
                "sqlite://{}",
                directory.path().join("lince.db").display()
            )))
            .map_err(error)?,
    );
    let mut worker = CanvasWorker::spawn(Configuration {
        showoff: false,
        size: [1920, 1080],
        directory: directory.path().to_path_buf(),
        runtime: runtime.handle().clone(),
        cell: Some(cell(engine, directory.path())),
    })?;
    let wait = |serial| -> io::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(failure) = worker.failure() {
                return Err(error(failure));
            }
            if let Some(frame) = worker.frame()
                && frame.input_serial >= serial
            {
                if frame.frame.dmabuf().is_none() || !frame.frame.ready().map_err(error)? {
                    return Err(error("Worker published an incomplete image"));
                }
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(error(
                    "Native input did not reach GPU-completed publication",
                ));
            }
            std::thread::sleep(Duration::from_micros(250));
        }
    };
    wait(0)?;
    std::thread::sleep(Duration::from_secs(3));
    while worker.frame().is_some() {}
    performance::set_phase(Phase::Quiet);
    let before = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
    let wall = Instant::now();
    std::thread::sleep(Duration::from_secs(5));
    let after = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
    let cpu = (after.tv_sec - before.tv_sec) as f64 + (after.tv_nsec - before.tv_nsec) as f64 / 1e9;
    let percent = cpu / wall.elapsed().as_secs_f64() * 100.0;
    performance::set_phase(Phase::OpenReveal);
    let mut samples = Vec::new();
    for _ in 0..120 {
        let started = Instant::now();
        let serial = worker
            .input(Input::Action(DesktopAction::ToggleEdit))
            .map_err(|_| error("Native input rejected"))?;
        wait(serial)?;
        samples.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    worker.stop()?;
    Ok(
        json!({"case":"worker-idle-and-input", "passed":percent <= 2.0 && passes(&samples), "idle_cpu_percent_one_core":percent, "input_to_completed_publication":summary(&samples)}),
    )
}

pub fn run() -> io::Result<()> {
    if !std::env::var("LINCE_TRACE_PERFORMANCE")
        .is_ok_and(|value| matches!(value.as_str(), "1" | "true"))
        || std::env::var_os("LINCE_PERFORMANCE_LOG").is_none()
    {
        let directory = tempfile::Builder::new()
            .prefix("lince-park-performance-")
            .tempdir()?
            .keep();
        let log = directory.join("phases.jsonl");
        eprintln!("Detailed performance measurements: {}", log.display());
        let status = std::process::Command::new(std::env::current_exe()?)
            .args(std::env::args_os().skip(1))
            .env("LINCE_TRACE_PERFORMANCE", "1")
            .env("LINCE_PERFORMANCE_LOG", &log)
            .status()?;
        return if status.success() {
            Ok(())
        } else {
            Err(error(format!(
                "Park responsiveness check failed ({status}); phases: {}",
                log.display()
            )))
        };
    }
    if !performance::enabled() {
        return Err(error("Required phase measurements could not start"));
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .thread_stack_size(32 * 1024 * 1024)
        .enable_all()
        .build()?;
    let (done, watchdog) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || {
        if watchdog.recv_timeout(Duration::from_secs(300))
            == Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        {
            eprintln!("Park responsiveness suite exceeded its five-minute hard deadline");
            std::process::exit(124);
        }
    });
    let mut results = vec![interface().unwrap_or_else(|failure| json!({"case":"protein-first-interface", "passed":false, "error":failure.to_string()}))];
    println!("{}", results[0]);
    for (name, hidden, mixed) in [
        ("empty", false, false),
        ("empty-with-inactive-content", true, false),
        ("mixed-content-under-query-pressure", true, true),
    ] {
        eprintln!("Starting private workload: {name}");
        let result = match scene(&runtime, name, hidden, mixed) {
            Ok(result) => result,
            Err(failure) => json!({"case":name, "passed":false, "error":failure.to_string()}),
        };
        println!("{result}");
        results.push(result);
    }
    let idle = worker_idle(&runtime).unwrap_or_else(|failure| json!({"case":"worker-idle-and-input", "passed":false, "error":failure.to_string()}));
    println!("{idle}");
    results.push(idle);
    performance::set_phase(Phase::Warmup);
    let pixels = match crate::presentation::probe() {
        Ok(()) => json!({"case":"native-pixels-resize-and-output-mapping", "passed":true}),
        Err(failure) => {
            json!({"case":"native-pixels-resize-and-output-mapping", "passed":false, "error":failure.to_string()})
        }
    };
    println!("{pixels}");
    results.push(pixels);
    let measured = performance::finish(Duration::from_secs(3));
    let coverage = measurements()
        .unwrap_or_else(|failure| json!({"passed":false,"error":failure.to_string()}));
    let revision = option_env!("LINCE_REVISION").unwrap_or("unknown");
    let identified = revision.len() == 40 && revision.bytes().all(|byte| byte.is_ascii_hexdigit());
    let _ = done.send(());
    let passed = results.len() == 6
        && results.iter().all(|result| result["passed"] == true)
        && !cfg!(debug_assertions)
        && measured
        && coverage["passed"] == true
        && identified;
    println!(
        "{}",
        json!({"suite":"park-responsiveness-v1", "passed":passed, "optimized":!cfg!(debug_assertions), "revision":revision, "measurements_delivered":measured, "coverage":coverage, "physical_daily_acceptance":"not measured by this private suite", "cases":results})
    );
    if passed {
        Ok(())
    } else {
        Err(error(
            "Park responsiveness acceptance failed; see measured results",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incomplete_or_lost_measurements_cannot_pass() {
        assert_eq!(measurement_coverage(&[])["passed"], false);
        let mut reports: Vec<_> = ["warmup", "open-reveal", "typing", "scroll", "active", "pan", "quiet"].into_iter().map(|phase| json!({
            "phase":phase, "systems":[{"exclusive_owner_cpu":{"count":1}}],
            "stages":(["native-main", "native-phase-post-update-owner-cpu", "gpu-submission-to-completion", "cell-query-execute-await"].into_iter().map(|stage| json!({"stage":stage,"count":1})).collect::<Vec<_>>()),
            "render_diagnostics":[{"path":"render/ui/elapsed_gpu","count":1}],
            "dropped_reports":0,"dropped_system_samples":0,"dropped_span_trace":0,"dropped_input_trace":0,
            "final_report":phase == "quiet"
        })).collect();
        assert_eq!(measurement_coverage(&reports)["passed"], true);
        for name in [
            "dropped_reports",
            "dropped_system_samples",
            "dropped_span_trace",
            "dropped_input_trace",
        ] {
            reports[0][name] = json!(1);
            assert_eq!(measurement_coverage(&reports)["passed"], false);
            reports[0][name] = json!(0);
        }
        reports[2]["systems"] = json!([]);
        assert_eq!(measurement_coverage(&reports)["passed"], false);
        reports.remove(2);
        assert_eq!(measurement_coverage(&reports)["passed"], false);
    }

    #[test]
    fn latency_gate_rejects_missing_samples_nonfinite_values_and_a_single_long_freeze() {
        assert!(!passes(&[]));
        assert!(!passes(&vec![1.0; 99]));
        assert!(passes(&vec![10.0; 120]));
        for value in [f64::NAN, f64::INFINITY, -1.0, 101.0] {
            let mut samples = vec![1.0; 120];
            samples[60] = value;
            assert!(!passes(&samples));
        }
        assert!(!passes(&vec![34.0; 120]));
    }
}
