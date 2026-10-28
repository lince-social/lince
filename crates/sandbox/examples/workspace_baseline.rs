use avian3d::prelude::Sleeping;
use bevy::{
    diagnostic::FrameCount,
    input::{ButtonState, mouse::MouseButtonInput},
    input_focus::InputFocus,
    math::DVec2,
    picking::pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput},
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    time::TimeUpdateStrategy,
    ui_widgets::Activate,
    window::{CursorMoved, PrimaryWindow, WindowEvent},
    winit::WinitSettings,
};
use lince_sandbox::{
    area::{AreaShape, InfluenceArea, Property, PropertyRule, ReachMode, RecordProperties},
    canvas::CanvasItem,
    container::BoxRoot,
    edit_mode::{EditAction, EditControl},
    sand_store::{SandKind, spawn_sand},
    topology::{Spatial, physics::Body},
    wake::WakeSignal,
    workspace::{WorkspaceFile, Workspaces},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

const AREA_TARGET: DVec2 = DVec2::new(-400.0, 0.0);
const SAND_START: DVec2 = DVec2::new(-300.0, -200.0);
const AREA_STRENGTH: f64 = 100_000.0;

#[derive(Resource)]
struct Baseline {
    directory: PathBuf,
    root: Option<Entity>,
    sand: Option<Entity>,
    falling: Option<Entity>,
    foreign: Option<Entity>,
    foreign_position: DVec2,
    drag_screen: Vec2,
    drag_before: DVec2,
    drag_after: DVec2,
    drag_requested: Option<Instant>,
    drag_presented_ms: Option<f64>,
    started: Instant,
    frame_started: Instant,
    previous_frame: Option<Instant>,
    active_frame_ms: Vec<f64>,
    active_update_ms: Vec<f64>,
    settled: Option<DVec2>,
    idle_started: Option<Instant>,
    idle_cpu_ticks: Option<u64>,
    idle_native_wakes: u64,
    idle_ready: Arc<AtomicBool>,
    native_wakes: Arc<AtomicU64>,
    control_wake: WakeSignal,
    steps: Vec<Value>,
    sampled_resources: Option<Value>,
    captured: bool,
}

fn activate(world: &mut World, root: Entity, action: EditAction) {
    let entity = world
        .query::<(Entity, &EditControl)>()
        .iter(world)
        .find(|(_, control)| control.root == root && control.action == action)
        .unwrap_or_else(|| panic!("Missing native control: {action:?}"))
        .0;
    world.trigger(Activate { entity });
    world.resource::<WakeSignal>().ring();
}

fn pointer(world: &mut World, position: Vec2) {
    let window = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
        .unwrap();
    let previous = world
        .get::<Window>(window)
        .unwrap()
        .cursor_position()
        .unwrap_or(position);
    world
        .get_mut::<Window>(window)
        .unwrap()
        .bypass_change_detection()
        .set_cursor_position(Some(position));
    world.write_message(WindowEvent::CursorMoved(CursorMoved {
        window,
        position,
        delta: None,
    }));
    mouse_input(
        world,
        window,
        position,
        PointerAction::Move {
            delta: position - previous,
        },
    );
    world.resource::<WakeSignal>().ring();
}

fn mouse_input(world: &mut World, window: Entity, position: Vec2, action: PointerAction) {
    let target = bevy::camera::RenderTarget::Window(bevy::window::WindowRef::Entity(window))
        .normalize(Some(window))
        .unwrap();
    world.write_message(PointerInput::new(
        PointerId::Mouse,
        Location { target, position },
        action,
    ));
}

fn button(world: &mut World, pressed: bool) {
    let window = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
        .unwrap();
    world.write_message(WindowEvent::MouseButtonInput(MouseButtonInput {
        window,
        button: MouseButton::Left,
        state: if pressed {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        },
    }));
    let position = world
        .get::<Window>(window)
        .unwrap()
        .cursor_position()
        .unwrap();
    mouse_input(
        world,
        window,
        position,
        if pressed {
            PointerAction::Press(PointerButton::Primary)
        } else {
            PointerAction::Release(PointerButton::Primary)
        },
    );
    world.resource::<WakeSignal>().ring();
}

fn record(world: &mut World, action: &str, observed: Value) {
    let state = world.resource::<Baseline>();
    let event = json!({
        "action": action,
        "frame": world.resource::<FrameCount>().0,
        "elapsed_ms": state.started.elapsed().as_secs_f64() * 1000.0,
        "observed": observed
    });
    let mut trace = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(state.directory.join("trace.jsonl"))
        .unwrap();
    writeln!(trace, "{}", serde_json::to_string(&event).unwrap()).unwrap();
    world.resource_mut::<Baseline>().steps.push(event);
}

fn cpu_ticks() -> Option<u64> {
    fs::read_to_string("/proc/self/stat")
        .ok()
        .and_then(|stat| stat.rsplit_once(") ").map(|(_, fields)| fields.to_owned()))
        .and_then(|fields| {
            let mut values = fields.split_whitespace().skip(11);
            let user = values.next()?.parse::<u64>().ok()?;
            let system = values.next()?.parse::<u64>().ok()?;
            user.checked_add(system)
        })
}

fn proc_kib(field: &str) -> Option<u64> {
    fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|line| {
            line.strip_prefix(field)?
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        })
}

fn percentile(values: &[f64], fraction: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let index = (fraction * sorted.len() as f64).ceil() as usize;
    sorted.get(index.saturating_sub(1)).copied()
}

fn timings(values: &[f64]) -> Value {
    json!({
        "samples": values.len(),
        "p50_ms": percentile(values, 0.5),
        "p95_ms": percentile(values, 0.95),
        "p99_ms": percentile(values, 0.99)
    })
}

fn machine() -> Value {
    let cpu = fs::read_to_string("/proc/cpuinfo").ok().and_then(|text| {
        text.lines().find_map(|line| {
            line.strip_prefix("model name")
                .and_then(|value| value.split_once(':'))
                .map(|(_, value)| value.trim().to_owned())
        })
    });
    let memory_kib = fs::read_to_string("/proc/meminfo").ok().and_then(|text| {
        text.lines().find_map(|line| {
            line.strip_prefix("MemTotal:")?
                .split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()
        })
    });
    json!({
        "os":std::env::consts::OS,
        "architecture":std::env::consts::ARCH,
        "kernel":fs::read_to_string("/proc/sys/kernel/osrelease").ok().map(|value|value.trim().to_owned()),
        "cpu":cpu,
        "available_parallelism":std::thread::available_parallelism().ok().map(usize::from),
        "system_memory_kib":memory_kib,
        "build_profile":if cfg!(debug_assertions) { "development" } else { "optimized" }
    })
}

fn frame_start(mut state: ResMut<Baseline>) {
    let now = Instant::now();
    if state.settled.is_none()
        && state.sand.is_some()
        && let Some(previous) = state.previous_frame
    {
        state
            .active_frame_ms
            .push(now.duration_since(previous).as_secs_f64() * 1000.0);
    }
    state.frame_started = now;
    state.previous_frame = Some(now);
}

fn frame_end(mut state: ResMut<Baseline>) {
    if state.settled.is_none() && state.sand.is_some() {
        let elapsed = state.frame_started.elapsed().as_secs_f64() * 1000.0;
        state.active_update_ms.push(elapsed);
    }
}

fn replay(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    let Some(root) = world.resource::<Baseline>().root else {
        return;
    };
    match frame {
        12 => {
            assert!(lince_sandbox::workspace_config::set_physics(
                world, root, 1, false
            ));
            let mut area = InfluenceArea::new(AreaShape::Square, AREA_TARGET, DVec2::splat(900.0));
            area.name = "Bring matching work here".into();
            area.strength = AREA_STRENGTH;
            area.attraction_enabled = true;
            area.reach.mode = ReachMode::Unlimited;
            area.rules.push(PropertyRule {
                property: Property::Quantity,
                value: "-1".into(),
            });
            lince_sandbox::area::spawn_area(world, root, 1, area).unwrap();
            let sand = spawn_sand(world, root, 1, SandKind::Text, "Move this work", SAND_START);
            world.get_mut::<CanvasItem>(sand).unwrap().size = Vec2::new(160.0, 80.0);
            world.entity_mut(sand).insert((
                Spatial {
                    depth: Some(1.0),
                    ..default()
                },
                RecordProperties(json!({"quantity":-1})),
            ));
            let falling = spawn_sand(
                world,
                root,
                1,
                SandKind::Text,
                "Gravity and settling",
                DVec2::new(-350.0, 200.0),
            );
            world.entity_mut(falling).insert(Spatial {
                elevation: 80.0,
                depth: Some(10.0),
                ..default()
            });
            world
                .entity_mut(falling)
                .insert(RecordProperties(json!({"quantity":7})));
            let mut state = world.resource_mut::<Baseline>();
            state.sand = Some(sand);
            state.falling = Some(falling);
            record(
                world,
                "prepared_isolated_native_canvas",
                json!({"physics_enabled":false,"area_target":AREA_TARGET.to_array(),"area_strength":AREA_STRENGTH,"area_side_canvas_units":900.0,"area_reach":"unlimited","sand_start":SAND_START.to_array(),"sand_depth_canvas_units":1.0,"falling_start_elevation_canvas_units":80.0,"falling_depth_canvas_units":10.0}),
            );
        }
        14 => activate(world, root, EditAction::Toggle),
        16 => activate(world, root, EditAction::Workspaces),
        18 => activate(world, root, EditAction::CreateWorkspace),
        20 => {
            let workspace = world.get::<Workspaces>(root).unwrap().active;
            assert_ne!(workspace, 1);
            let position = SAND_START;
            let foreign = spawn_sand(
                world,
                root,
                workspace,
                SandKind::Text,
                "Other workspace stays still",
                position,
            );
            world
                .entity_mut(foreign)
                .insert(RecordProperties(json!({"quantity":-1})));
            assert!(lince_sandbox::workspace_config::set_physics(
                world, root, workspace, false
            ));
            let mut state = world.resource_mut::<Baseline>();
            state.foreign = Some(foreign);
            state.foreign_position = position;
            activate(world, root, EditAction::SwitchWorkspace(1));
        }
        22 => activate(world, root, EditAction::General),
        24 => {
            let sand = world.resource::<Baseline>().sand.unwrap();
            let screen = if world
                .get::<lince_sandbox::topology::presentation::SpatialRoot>(root)
                .is_some()
            {
                let corners = lince_sandbox::topology::presentation::projected_corners(world, sand)
                    .expect("Native spatial Sand surface is not ready for pointer input");
                (corners
                    .iter()
                    .map(|point| DVec2::from_array(*point))
                    .sum::<DVec2>()
                    * 0.25)
                    .as_vec2()
            } else {
                world.get::<UiGlobalTransform>(sand).unwrap().translation
                    * world
                        .get::<ComputedNode>(sand)
                        .unwrap()
                        .inverse_scale_factor()
            };
            let before = world.get::<CanvasItem>(sand).unwrap().position;
            let mut state = world.resource_mut::<Baseline>();
            state.drag_screen = screen;
            state.drag_before = before;
            pointer(world, screen);
            record(
                world,
                "native_pointer_target",
                json!({"screen":screen.to_array(),"position":before.to_array()}),
            );
            let path = world
                .resource::<Baseline>()
                .directory
                .join("before-drag.png");
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path));
        }
        26 => {
            let hit = world
                .resource::<lince_sandbox::topology::input::PointerState>()
                .hit;
            let cursor = world
                .query_filtered::<&Window, With<PrimaryWindow>>()
                .single(world)
                .unwrap()
                .cursor_position();
            let editing = world
                .get::<lince_sandbox::edit_mode::EditMode>(root)
                .unwrap()
                .enabled;
            record(
                world,
                "native_pointer_press",
                json!({"hit":hit.map(|(entity,point)|json!({"entity":entity.to_bits(),"point":point.to_array()})),"window_cursor":cursor.map(|position|position.to_array()),"edit_mode_enabled":editing}),
            );
            assert_eq!(
                hit.map(|(entity, _)| entity),
                world.resource::<Baseline>().sand,
                "Native pointer must hit the visible Sand before dragging"
            );
            button(world, true);
        }
        28 => {
            world.resource_mut::<Baseline>().drag_requested = Some(Instant::now());
            let position = world.resource::<Baseline>().drag_screen + Vec2::new(64.0, 24.0);
            pointer(world, position);
        }
        30 => button(world, false),
        32 => {
            let sand = world.resource::<Baseline>().sand.unwrap();
            let after = world.get::<CanvasItem>(sand).unwrap().position;
            let before = world.resource::<Baseline>().drag_before;
            record(
                world,
                "native_drag_observed",
                json!({"before":before.to_array(),"after":after.to_array()}),
            );
            assert!(
                after.distance(before) > 40.0,
                "Native pointer drag did not move the Sand"
            );
            world.resource_mut::<Baseline>().drag_after = after;
            record(
                world,
                "native_pointer_drag",
                json!({"before":before.to_array(),"after":after.to_array()}),
            );
        }
        34 => {
            world.resource_mut::<InputFocus>().clear();
            activate(world, root, EditAction::TogglePhysics);
        }
        36 => {
            assert!(lince_sandbox::workspace_config::enabled(world, root, 1));
            record(world, "native_physics_toggle", json!({"enabled":true}));
        }
        _ => {}
    }
}

fn observe(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    if let Some(sand) = world.resource::<Baseline>().sand {
        let state = world.resource::<Baseline>();
        if state.drag_presented_ms.is_none()
            && let Some(requested) = state.drag_requested
            && lince_sandbox::canvas::presented(world, sand)
                .is_some_and(|item| item.position.distance(state.drag_before) > 1.0)
        {
            world.resource_mut::<Baseline>().drag_presented_ms =
                Some(requested.elapsed().as_secs_f64() * 1000.0);
        }
    }
    if frame < 70 || world.resource::<Baseline>().captured {
        return;
    }
    assert!(
        frame < 2400,
        "Native Area/gravity did not settle within 40 simulated seconds"
    );
    let (bodies, awake) = world.query::<(&Body, Has<Sleeping>)>().iter(world).fold(
        (0, 0),
        |(count, active), (body, sleeping)| {
            (count + 1, active + usize::from(!body.held && !sleeping))
        },
    );
    let state = world.resource::<Baseline>();
    let sand = state.sand.unwrap();
    let point = world.get::<CanvasItem>(sand).unwrap().position;
    let foreign = state.foreign.unwrap();
    assert_eq!(
        world.get::<CanvasItem>(foreign).unwrap().position,
        state.foreign_position
    );
    if state.settled.is_none() {
        if bodies == 0 || awake != 0 {
            return;
        }
        assert!(
            point.distance(AREA_TARGET) <= 1.0,
            "Area settled away from its target: {point:?}"
        );
        let falling = state.falling.unwrap();
        let elevation = world.get::<Spatial>(falling).unwrap().elevation;
        assert!(
            elevation < 60.0,
            "Existing gravity did not lower the elevated Sand"
        );
        let resources = lince_sandbox::laboratory::resources::capture(world);
        record(
            world,
            "settled",
            json!({"bodies":bodies,"awake_bodies":awake,"position":point.to_array(),"falling_elevation":elevation}),
        );
        let mut state = world.resource_mut::<Baseline>();
        state.sampled_resources = Some(serde_json::to_value(resources).unwrap());
        state.settled = Some(point);
        state.idle_started = Some(Instant::now());
        state.idle_cpu_ticks = cpu_ticks();
        state.idle_native_wakes = state.native_wakes.load(Ordering::Relaxed);
        let ready = state.idle_ready.clone();
        let wake = state.control_wake.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(1500));
            ready.store(true, Ordering::Release);
            wake.ring();
        });
        world.insert_resource(lince_sandbox::theme::idle_settings());
        return;
    }
    assert_eq!(
        point,
        state.settled.unwrap(),
        "Settled native position drifted"
    );
    assert_eq!(
        awake, 0,
        "Settled native bodies woke without a relevant change"
    );
    if !state.idle_ready.load(Ordering::Acquire) {
        return;
    }
    let idle_ms = state.idle_started.unwrap().elapsed().as_secs_f64() * 1000.0;
    let viewport = world
        .iter_entities()
        .find_map(|entity| entity.get::<Window>())
        .map(|window| json!({"logical_width":window.width(),"logical_height":window.height(),"scale_factor":window.scale_factor()}));
    let report = json!({
        "workflow":"01.W01",
        "status":"observed",
        "scope":"Existing native canvas only; future scale, hosting and scientific capabilities are not exercised.",
        "version":env!("CARGO_PKG_VERSION"),
        "machine":machine(),
        "requested_viewport":[1280,720],
        "observed_viewport":viewport,
        "clock":{"physics":"manual 1/60 s per update; existing fixed-step schedule","timings":"measured wall-clock intervals; update includes native UI and extraction preparation, not GPU duration"},
        "canvas_units":{"current":"logical canvas units; no physical calibration declared","display":"zoom maps canvas units to logical pixels","existing_gravity_canvas_units_per_second_squared":981.0},
        "steps":state.steps,
        "active_frame_intervals":timings(&state.active_frame_ms),
        "active_update":timings(&state.active_update_ms),
        "native_input_response":{"first_presented_drag_change_ms":state.drag_presented_ms,"samples":usize::from(state.drag_presented_ms.is_some()),"method":"Adapter-level synthetic window cursor state and native pointer events to presented-state change; no physical OS cursor warp, hardware input or GPU scanout measurement."},
        "idle":{"observed_ms":idle_ms,"native_wake_signals":state.native_wakes.load(Ordering::Relaxed)-state.idle_native_wakes,"measurement_wake_signals":1,"process_cpu_ticks":cpu_ticks().zip(state.idle_cpu_ticks).map(|(end,start)|end.saturating_sub(start)),"awake_bodies":awake,"position_unchanged":true},
        "process":{"resident_kib":proc_kib("VmRSS:"),"peak_resident_kib":proc_kib("VmHWM:")},
        "resources":state.sampled_resources,
        "measurement_limits":["Managed texture sizes and retained payloads are estimates, not total GPU residency.","CPU ticks are Linux process counters; no per-Sand CPU attribution.","Native wake signals exclude the single measurement wake and are not all OS wake-ups.","Active timing includes fixture setup, edit-panel/workspace switching and a pre-drag screenshot; it is not an isolated navigation benchmark.","Shared development-machine load, including concurrent compilation, is uncontrolled and can affect wall-clock timing.","Development run is baseline evidence, not a future-stage performance pass."],
        "qualitative_review":{"status":"pending","image":"settled.png"}
    });
    let directory = state.directory.clone();
    fs::write(
        directory.join("baseline.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    world.resource_mut::<Baseline>().captured = true;
    world.insert_resource(WinitSettings::continuous());
    world.spawn(Screenshot::primary_window())
        .observe(save_to_disk(directory.join("settled.png")))
        .observe(|capture: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
            assert!(capture.image.data.as_ref().unwrap().chunks_exact(4).any(|pixel| pixel[0] > 40 && pixel[1] > 40 && pixel[2] > 40), "Native baseline did not render");
            println!("Workspace baseline observed: native drag, Area attraction, gravity, settling, workspace isolation and idle sampling.");
            exit.write(AppExit::Success);
        });
}

fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .expect("provide a new output directory"),
    );
    fs::create_dir(&directory).expect("baseline requires a new output directory");
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(120));
        eprintln!("Workspace baseline exceeded its wall-time limit");
        std::process::exit(1);
    });
    let mut app = lince_sandbox::app::interface_app();
    let control_wake = app.world().resource::<WakeSignal>().clone();
    let native_wakes = Arc::new(AtomicU64::new(0));
    let counter = native_wakes.clone();
    let target = control_wake.clone();
    app.insert_resource(WakeSignal::new(move || {
        counter.fetch_add(1, Ordering::Relaxed);
        target.ring();
    }));
    let now = Instant::now();
    app.add_plugins(bevy::log::LogPlugin::default())
        .insert_resource(WorkspaceFile::new(directory.join("interface.json")))
        .insert_resource(Baseline {
            directory,
            root: None,
            sand: None,
            falling: None,
            foreign: None,
            foreign_position: DVec2::ZERO,
            drag_screen: Vec2::ZERO,
            drag_before: DVec2::ZERO,
            drag_after: DVec2::ZERO,
            drag_requested: None,
            drag_presented_ms: None,
            started: now,
            frame_started: now,
            previous_frame: None,
            active_frame_ms: Vec::new(),
            active_update_ms: Vec::new(),
            settled: None,
            idle_started: None,
            idle_cpu_ticks: None,
            idle_native_wakes: 0,
            idle_ready: Arc::new(AtomicBool::new(false)),
            native_wakes,
            control_wake,
            steps: Vec::new(),
            sampled_resources: None,
            captured: false,
        })
        .insert_resource(WinitSettings::continuous())
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )))
        .add_systems(Startup, |world: &mut World| {
            world
                .query::<&mut Window>()
                .single_mut(world)
                .unwrap()
                .resolution
                .set(1280.0, 720.0);
            let root = world.spawn(BoxRoot).id();
            world.resource_mut::<Baseline>().root = Some(root);
        })
        .add_systems(First, frame_start)
        .add_systems(
            Update,
            replay.before(lince_sandbox::physics::SimulateWorkspaces),
        )
        .add_systems(PostUpdate, observe)
        .add_systems(Last, frame_end);
    app.run();
}
