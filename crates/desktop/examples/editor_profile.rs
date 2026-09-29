use bevy::{
    input_focus::{FocusCause, InputFocus},
    math::DVec2,
    prelude::*,
    text::{EditableText, EditableTextSystems, TextEdit},
    ui::UiSystems,
    window::PrimaryWindow,
    winit::WinitSettings,
};
use lince_desktop::{
    container::BoxRoot,
    file_explorer::FileExplorer,
    ide::Ide,
    workspace::{WorkspaceFile, Workspaces},
};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Resource)]
struct Probe {
    directory: PathBuf,
    phase: usize,
    frames: usize,
    samples: Vec<f64>,
    segments: [Vec<f64>; 5],
    stamps: [Option<Instant>; 4],
    sent: Option<Instant>,
    began: Instant,
    idle_began: Instant,
    open_ready: Option<f64>,
    updates: Arc<AtomicU64>,
    idle_sample: Arc<Mutex<Option<Value>>>,
    results: Vec<Value>,
}

fn cpu_ticks() -> u64 {
    let value = std::fs::read_to_string("/proc/self/stat").expect("Run this probe on Linux");
    let fields: Vec<_> = value
        .rsplit_once(')')
        .unwrap()
        .1
        .split_whitespace()
        .collect();
    fields[11].parse::<u64>().unwrap() + fields[12].parse::<u64>().unwrap()
}

fn rss_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

fn clock_ticks() -> u64 {
    #[cfg(unix)]
    {
        rustix::param::clock_ticks_per_second()
    }
    #[cfg(not(unix))]
    {
        panic!("Run this probe on Linux")
    }
}

fn idle(world: &mut World) {
    world.resource_mut::<InputFocus>().clear();
    world.insert_resource(lince_desktop::theme::idle_settings());
    let wake = world.resource::<lince_desktop::wake::WakeSignal>().clone();
    let mut probe = world.resource_mut::<Probe>();
    probe.frames = 0;
    let sample = probe.idle_sample.clone();
    let updates = probe.updates.clone();
    *sample.lock().unwrap() = None;
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(2));
        let first_update = updates.load(Ordering::Relaxed);
        let first_cpu = cpu_ticks();
        let began = Instant::now();
        std::thread::sleep(Duration::from_secs(10));
        let wall = began.elapsed().as_secs_f64();
        let cpu = (cpu_ticks() - first_cpu) as f64 / clock_ticks() as f64;
        *sample.lock().unwrap() = Some(
            json!({"wall_seconds": wall, "cpu_seconds": cpu, "cpu_percent_one_core": 100.0 * cpu / wall, "rss_kib": rss_kib(), "updates": updates.load(Ordering::Relaxed) - first_update}),
        );
        wake.ring();
    });
}

fn record_idle(world: &mut World, name: &str) {
    let mut probe = world.resource_mut::<Probe>();
    let mut result = probe.idle_sample.lock().unwrap().take().unwrap();
    result["phase"] = json!(name);
    println!("{result}");
    probe.results.push(result);
    probe.frames = 0;
    probe.phase += 1;
    drop(probe);
    world.insert_resource(WinitSettings::continuous());
}

fn exercise(world: &mut World) {
    assert!(
        world.resource::<Probe>().began.elapsed() < Duration::from_secs(240),
        "Editor profile timed out"
    );
    world.resource_mut::<Probe>().frames += 1;
    world
        .resource::<Probe>()
        .updates
        .fetch_add(1, Ordering::Relaxed);
    let phase = world.resource::<Probe>().phase;
    let frames = world.resource::<Probe>().frames;
    let directory = world.resource::<Probe>().directory.clone();
    let idle_ready = world
        .resource::<Probe>()
        .idle_sample
        .lock()
        .unwrap()
        .is_some();
    match phase {
        0 if frames > 60 => {
            world.resource_mut::<Probe>().phase = 1;
            idle(world);
        }
        1 | 4 if idle_ready => {
            record_idle(
                world,
                if phase == 1 {
                    "empty_app_idle"
                } else {
                    "large_project_idle"
                },
            );
        }
        2 => {
            let root = world
                .query_filtered::<Entity, (With<BoxRoot>, With<Workspaces>)>()
                .single(world)
                .unwrap();
            let path = directory.join("project/large.txt");
            lince_desktop::ide::spawn(
                world,
                root,
                1,
                DVec2::ZERO,
                Ide {
                    explorer: FileExplorer {
                        roots: vec![directory.join("project")],
                        ..default()
                    },
                    paths: vec![path.clone()],
                    active: Some(path),
                    ..default()
                },
            );
            let mut probe = world.resource_mut::<Probe>();
            probe.idle_began = Instant::now();
            probe.phase = 3;
            probe.frames = 0;
        }
        3 => {
            let ready = world.query::<&EditableText>().iter(world).any(|text| {
                text.max_characters == Some(lince_editor::MAX_WINDOW_BYTES * 2)
                    && text.value().to_string().starts_with("large file")
            });
            if ready && world.resource::<Probe>().open_ready.is_none() {
                let elapsed = world.resource::<Probe>().idle_began.elapsed().as_secs_f64();
                world.resource_mut::<Probe>().open_ready = Some(elapsed);
            }
            if !ready || frames < 120 {
                return;
            }
            let result = json!({"phase": "large_project_open_and_settle", "file_ready_seconds": world.resource::<Probe>().open_ready, "wall_seconds": world.resource::<Probe>().idle_began.elapsed().as_secs_f64(), "rss_kib": rss_kib(), "entities": world.entities().len(), "file_bytes": std::fs::metadata(directory.join("project/large.txt")).unwrap().len(), "directory_entries": 10_001});
            println!("{result}");
            world.resource_mut::<Probe>().results.push(result);
            world.resource_mut::<Probe>().phase = 4;
            idle(world);
        }
        5 => {
            if world.resource::<Probe>().samples.len() < 120 {
                let editor = world
                    .query::<(Entity, &EditableText)>()
                    .iter(world)
                    .find(|(_, text)| {
                        text.max_characters == Some(lince_editor::MAX_WINDOW_BYTES * 2)
                    })
                    .unwrap()
                    .0;
                world
                    .resource_mut::<InputFocus>()
                    .set(editor, FocusCause::Navigated);
                world
                    .get_mut::<EditableText>(editor)
                    .unwrap()
                    .queue_edit(TextEdit::Insert("x".into()));
                world.resource_mut::<Probe>().sent = Some(Instant::now());
                world.resource_mut::<Probe>().stamps = [None; 4];
            }
            if world.resource::<Probe>().samples.len() >= 120 {
                let mut probe = world.resource_mut::<Probe>();
                probe.samples.sort_by(f64::total_cmp);
                let result = json!({"phase": "typing", "input_to_layout_ms_p50": probe.samples[60], "input_to_layout_ms_p95": probe.samples[114], "input_to_layout_ms_max": probe.samples.last(), "rss_kib": rss_kib(), "samples": probe.samples.len(), "measurement": "Main-thread input dispatch through UI layout; excludes presentation latency"});
                println!("{result}");
                probe.results.push(result);
                for (index, name) in [
                    "before_text_edit",
                    "text_edit",
                    "remaining_content",
                    "layout",
                    "after_layout",
                ]
                .into_iter()
                .enumerate()
                {
                    let segment = &mut probe.segments[index];
                    segment.sort_by(f64::total_cmp);
                    let result = json!({"phase": name, "ms_p50": segment[60], "ms_p95": segment[114], "ms_max": segment.last()});
                    println!("{result}");
                    probe.results.push(result);
                }
                std::fs::write(
                    directory.join("results.json"),
                    serde_json::to_vec_pretty(&probe.results).unwrap(),
                )
                .unwrap();
                probe.phase = 6;
                drop(probe);
                let owner = world
                    .query_filtered::<Entity, With<Ide>>()
                    .single(world)
                    .unwrap();
                let action = world
                    .query::<(
                        &lince_desktop::icons::Tooltip,
                        &lince_desktop::actions::ActionButton,
                    )>()
                    .iter(world)
                    .find(|(tip, button)| tip.0 == "Discard…" && button.target == owner)
                    .unwrap()
                    .1
                    .actions
                    .clone();
                action.run(world, owner);
                action.run(world, owner);
            }
        }
        6 if frames > 125 => {
            world.write_message(AppExit::Success);
        }
        _ => {}
    }
}

fn finish(mut probe: ResMut<Probe>) {
    if let Some(sent) = probe.sent.take() {
        let finished = Instant::now();
        probe
            .samples
            .push(finished.duration_since(sent).as_secs_f64() * 1000.0);
        let stamps = probe.stamps.map(Option::unwrap);
        let mut previous = sent;
        for (index, stamp) in stamps.into_iter().chain([finished]).enumerate() {
            probe.segments[index].push(stamp.duration_since(previous).as_secs_f64() * 1000.0);
            previous = stamp;
        }
    }
}

fn stamp<const N: usize>(mut probe: ResMut<Probe>) {
    if probe.sent.is_some() {
        probe.stamps[N] = Some(Instant::now());
    }
}

#[tokio::main]
async fn main() {
    let directory = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("provide an empty artifact directory"),
    );
    let file_bytes = std::env::args()
        .nth(2)
        .map(|value| value.parse::<usize>().expect("file size in bytes"))
        .unwrap_or(10 * 1024 * 1024);
    assert!((1..=lince_editor::MAX_FILE_BYTES - 128).contains(&file_bytes));
    std::fs::create_dir_all(&directory).unwrap();
    assert!(
        std::fs::read_dir(&directory).unwrap().next().is_none(),
        "Use an empty artifact directory"
    );
    std::fs::create_dir(directory.join("project")).unwrap();
    let text = "large file — Unicode 猫\n";
    std::fs::write(
        directory.join("project/large.txt"),
        text.repeat(file_bytes.div_ceil(text.len())),
    )
    .unwrap();
    for n in 0..10_000 {
        std::fs::write(directory.join(format!("project/item-{n:05}.txt")), []).unwrap();
    }
    let mut app = lince_desktop::app::interface_app();
    app.insert_resource(WorkspaceFile::new(directory.join("interface.json")))
        .insert_resource(WinitSettings::continuous())
        .insert_resource(Probe {
            directory,
            phase: 0,
            frames: 0,
            samples: Vec::new(),
            segments: std::array::from_fn(|_| Vec::new()),
            stamps: [None; 4],
            sent: None,
            began: Instant::now(),
            idle_began: Instant::now(),
            open_ready: None,
            updates: Arc::new(AtomicU64::new(0)),
            idle_sample: Arc::new(Mutex::new(None)),
            results: Vec::new(),
        })
        .add_systems(Startup, |world: &mut World| {
            world.spawn(BoxRoot);
            world
                .query_filtered::<&mut Window, With<PrimaryWindow>>()
                .single_mut(world)
                .unwrap()
                .resolution
                .set(1640.0, 1020.0);
        })
        .add_systems(
            Update,
            exercise.after(lince_desktop::workspace::PrepareWorkspaces),
        )
        .add_systems(
            PostUpdate,
            (
                stamp::<0>
                    .after(UiSystems::Propagate)
                    .before(EditableTextSystems),
                stamp::<1>
                    .after(EditableTextSystems)
                    .before(UiSystems::Layout),
                stamp::<2>
                    .after(UiSystems::Content)
                    .before(UiSystems::Layout),
                stamp::<3>
                    .after(UiSystems::Layout)
                    .before(UiSystems::PostLayout),
            )
                .chain(),
        )
        .add_systems(Last, finish);
    app.run();
}
