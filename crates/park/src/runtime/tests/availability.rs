use super::super::{CanvasWorker, Completed, Configuration, Input};
use bevy::{
    ecs::schedule::Schedules,
    prelude::{
        ChildOf, ComputedNode, Display, Entity, IntoScheduleConfigs, Name, Node, PostUpdate, Rect,
        Text, Vec2, World,
    },
    text::EditableText,
    ui::UiSystems,
};
use lince_interface::{
    host::{DesktopAction, WakeSignal, WorkspaceLayout},
    native::{
        self,
        protein_area::{RecordBinding, Source},
    },
};
use std::{
    io::Write,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

const BODY: &str = "Quiet inspector owner description";

mod permission;

fn provider(engine: Arc<engine::Engine>) -> native::app::CellHandle {
    native::app::CellHandle(cell::CellRuntime {
        store: engine.store.clone(),
        engine,
        commands: Default::default(),
        lanes: Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        information: None,
        speech: None,
        fiote: None,
    })
}

fn output() -> crate::presentation::Output {
    use lince_interface::host::{OutputId, OutputViewport, Rectangle, raster::RasterMapping};

    let bounds = Rectangle::new([0.0; 2], [1000.0, 700.0]).unwrap();
    crate::presentation::Output {
        name: "Inspector fixture".into(),
        viewport: OutputViewport { rotation: Default::default(),
            id: OutputId(1),
            bounds,
            usable: Some(bounds),
            physical_size: [1000, 700],
            raster: RasterMapping {
                origin: [0.0; 2],
                scale: [1.0; 2],
            },
            workspace: 1,
        },
    }
}

#[derive(Clone, Debug, Default)]
struct Observed {
    row: Option<u64>,
    status: String,
    status_visible: bool,
    preview_visible: bool,
    body: Option<(u64, String, bool)>,
}

fn belongs(world: &World, mut entity: Entity, area: Entity, uid: &str) -> bool {
    for _ in 0..native::layout::MAX_DEPTH {
        if let Some(binding) = world.get::<RecordBinding>(entity) {
            return binding.area == area && binding.uid == uid;
        }
        let Some(parent) = world.get::<ChildOf>(entity) else {
            return false;
        };
        entity = parent.parent();
    }
    false
}

fn visible(world: &World, entity: Entity) -> bool {
    let mut ancestor = Some(entity);
    for _ in 0..native::layout::MAX_DEPTH {
        let Some(current) = ancestor else { break };
        if world
            .get::<Node>(current)
            .is_some_and(|node| node.display == Display::None)
        {
            return false;
        }
        ancestor = world.get::<ChildOf>(current).map(ChildOf::parent);
    }
    world
        .get::<ComputedNode>(entity)
        .is_some_and(|node| node.size().min_element() > 2.0)
        && native::topology::presentation::bounds(world, entity).is_some_and(|bounds| {
            bounds
                .intersect(Rect::from_corners(Vec2::ZERO, Vec2::new(1000.0, 700.0)))
                .size()
                .min_element()
                > 2.0
        })
}

fn observe(world: &mut World, area: Entity, uid: &str) -> Observed {
    let row = world
        .query::<(Entity, &RecordBinding)>()
        .iter(world)
        .find(|(entity, binding)| {
            binding.area == area
                && binding.uid == uid
                && world.get::<native::canvas::CanvasItem>(*entity).is_some()
        })
        .map(|(entity, _)| entity.to_bits());
    let status = world
        .query::<(Entity, &Name, &Text)>()
        .iter(world)
        .find(|(entity, name, _)| {
            name.as_str() == "record.availability.status" && belongs(world, *entity, area, uid)
        })
        .map(|(entity, _, text)| (text.0.clone(), visible(world, entity)));
    let preview_visible = world
        .query::<(Entity, &native::description::Description)>()
        .iter(world)
        .any(|(entity, description)| {
            description.source == BODY
                && belongs(world, entity, area, uid)
                && visible(world, entity)
        });
    let body = world
        .query::<(Entity, &EditableText)>()
        .iter(world)
        .find(|(entity, text)| {
            text.value().to_string() == BODY && belongs(world, *entity, area, uid)
        })
        .map(|(entity, text)| {
            (
                entity.to_bits(),
                text.value().to_string(),
                visible(world, entity),
            )
        });
    Observed {
        row,
        status: status
            .as_ref()
            .map_or_else(String::new, |(text, _)| text.clone()),
        status_visible: status.is_some_and(|(_, visible)| visible),
        preview_visible,
        body,
    }
}

fn watch(world: &mut World, area: Entity, uid: String, observed: Arc<Mutex<Observed>>) {
    world
        .resource_mut::<Schedules>()
        .get_mut(PostUpdate)
        .unwrap()
        .add_systems(
            (move |world: &mut World| {
                *observed.lock().unwrap() = observe(world, area, &uid);
            })
            .after(UiSystems::PostLayout),
        );
}

fn quiet(worker: &CanvasWorker, phase: &str, serial: u64, limit: usize, timeout: u64) -> Completed {
    quiet_until(worker, phase, serial, limit, timeout, || true)
}

fn quiet_until(
    worker: &CanvasWorker,
    phase: &str,
    serial: u64,
    limit: usize,
    timeout: u64,
    ready: impl Fn() -> bool,
) -> Completed {
    eprintln!("Quiet inspector phase={phase} input_serial={serial}");
    let deadline = Instant::now() + Duration::from_secs(timeout);
    let mut latest = None;
    let mut changed = Instant::now();
    let mut publications = 0;
    let mut idle_since = None;
    loop {
        if let Some(failure) = worker.failure() {
            panic!("The quiet worker failed: {failure}");
        }
        assert!(worker.is_running());
        if let Some(completed) = worker.frame() {
            if completed.input_serial < serial {
                continue;
            }
            assert_eq!(
                completed.input_serial, serial,
                "A backend transition received new test input"
            );
            assert!(completed.frame.ready().unwrap());
            publications += 1;
            if publications > limit.max(64) {
                pixels(&completed, &format!("{phase}-unsettled"));
                panic!(
                    "The worker exceeded its diagnostic cap without settling: {phase}; gate={limit}"
                );
            }
            latest = Some(completed);
            changed = Instant::now();
        }
        if worker.idle.load(Ordering::Acquire) && ready() {
            idle_since.get_or_insert_with(Instant::now);
        } else {
            idle_since = None;
        }
        if latest.is_some()
            && changed.elapsed() >= Duration::from_millis(500)
            && idle_since.is_some_and(|since| since.elapsed() >= Duration::from_millis(500))
        {
            eprintln!("Quiet inspector phase={phase} publications={publications}");
            if publications > limit {
                pixels(latest.as_ref().unwrap(), &format!("{phase}-over-budget"));
                panic!(
                    "The worker settled after {publications} publications, exceeding {limit}: {phase}"
                );
            }
            return latest.unwrap();
        }
        if Instant::now() >= deadline {
            if let Some(frame) = &latest {
                pixels(frame, &format!("{phase}-unready"));
            }
            panic!(
                "The worker did not publish and settle without further input: {phase}; publications={publications}"
            );
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn pixels(frame: &Completed, phase: &str) -> Vec<u8> {
    let pixels = crate::presentation::probe_pixels(frame.frame.dmabuf().unwrap()).unwrap();
    assert_eq!(pixels.len(), 1000 * 700 * 4);
    if let Some(directory) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        let mut file =
            std::fs::File::create(directory.join(format!("quiet-inspector-{phase}.ppm"))).unwrap();
        file.write_all(b"P6\n1000 700\n255\n").unwrap();
        for row in pixels.chunks_exact(1000 * 4) {
            for pixel in row.chunks_exact(4) {
                file.write_all(&pixel[..3]).unwrap();
            }
        }
    }
    pixels
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn record_loading_and_deletion_wake_a_quiet_worker_without_test_input() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let engine = Arc::new(runtime.block_on(engine::Engine::open_memory()).unwrap());
    let uid = nucleus::new_uid("r");
    runtime
        .block_on(engine.act(
            engine::actions::Action::CreateRecordDraft {
                draft: engine::record_creation::Draft {
                    uid: uid.clone(),
                    head: "Quiet inspector".into(),
                    body: BODY.into(),
                    ..Default::default()
                },
            },
            None,
        ))
        .unwrap();
    let warmup = nucleus::new_uid("r");
    runtime
        .block_on(engine.act(
            engine::actions::Action::CreateRecordDraft {
                draft: engine::record_creation::Draft {
                    uid: warmup.clone(),
                    head: "Editor setup".into(),
                    body: BODY.into(),
                    ..Default::default()
                },
            },
            None,
        ))
        .unwrap();
    let cell = provider(engine.clone());
    let notifications = Arc::new(AtomicUsize::new(0));
    let notified = notifications.clone();
    let mut worker = CanvasWorker::spawn_notifying(
        Configuration {
            showoff: false,
            size: [1000, 700],
            directory: directory.path().into(),
            runtime: runtime.handle().clone(),
            cell: Some(cell),
        },
        Some(WakeSignal::new(move || {
            notified.fetch_add(1, Ordering::SeqCst);
        })),
    )
    .unwrap();
    let warmup_sample = Arc::new(Mutex::new(Observed::default()));
    let warmed = warmup_sample.clone();
    let startup = worker
        .input(Input::Test(Box::new(move |canvas| {
            canvas.outputs(vec![output()]).unwrap();
            canvas
                .desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free))
                .unwrap();
            let world = canvas.world_mut();
            let roots: Vec<_> = world
                .query_filtered::<Entity, bevy::prelude::With<native::workspace::Workspaces>>()
                .iter(world)
                .collect();
            assert_eq!(roots.len(), 1);
            let root = roots[0];
            let workspace = world
                .get::<native::workspace::Workspaces>(root)
                .unwrap()
                .active;
            assert!(native::workspace_config::set_physics(
                world, root, workspace, false
            ));
            let area = native::full_record::open(world, root, &warmup, Source::Local).unwrap();
            watch(world, area, warmup, warmed);
        })))
        .unwrap();
    let startup = quiet_until(&worker, "startup", startup, 64, 40, || {
        let observed = warmup_sample.lock().unwrap();
        observed.preview_visible && observed.body.is_some()
    });
    pixels(&startup, "startup");
    let warmed = warmup_sample.lock().unwrap().clone();
    assert!(
        warmed.preview_visible && warmed.body.is_some(),
        "{warmed:?}"
    );
    let workspace = worker
        .input(Input::Action(DesktopAction::Workspace(2)))
        .unwrap();
    quiet(&worker, "workspace-setup", workspace, 64, 40);
    let setup = worker
        .input(Input::Test(Box::new(|canvas| {
            canvas
                .desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free))
                .unwrap();
            let world = canvas.world_mut();
            let root = world
                .query_filtered::<Entity, bevy::prelude::With<native::workspace::Workspaces>>()
                .single(world)
                .unwrap();
            let workspace = world
                .get::<native::workspace::Workspaces>(root)
                .unwrap()
                .active;
            assert_eq!(workspace, 2);
            assert!(native::workspace_config::set_physics(
                world, root, workspace, false
            ));
        })))
        .unwrap();
    quiet(&worker, "target-setup", setup, 64, 40);
    let connection = runtime.block_on(engine.store.pool.acquire()).unwrap();
    let sample = Arc::new(Mutex::new(Observed::default()));
    let observed = sample.clone();
    let expected = uid.clone();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let serial = worker
        .input(Input::Test(Box::new(move |canvas| {
            let world = canvas.world_mut();
            let roots: Vec<_> = world
                .query_filtered::<Entity, bevy::prelude::With<native::workspace::Workspaces>>()
                .iter(world)
                .collect();
            assert_eq!(roots.len(), 1);
            let root = roots[0];
            let area = native::full_record::open(world, root, &expected, Source::Local).unwrap();
            sender.send(area.to_bits()).unwrap();
            watch(world, area, expected, observed);
        })))
        .unwrap();
    let loading = quiet_until(&worker, "loading", serial, 64, 40, || {
        let observed = sample.lock().unwrap();
        observed.status_visible && observed.status.contains("Loading")
    });
    let area = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
    let initial = sample.lock().unwrap().clone();
    assert!(
        initial.row.is_some(),
        "The inspector never created its retained loading card: {initial:?}; area={area}"
    );
    assert!(
        initial.status_visible && initial.status.contains("Loading"),
        "{initial:?}"
    );
    assert!(
        !initial.preview_visible && initial.body.is_none(),
        "{initial:?}"
    );
    assert!(loading.frame.input.editor.is_none());
    let loading_pixels = pixels(&loading, "loading");
    let notified_before = notifications.load(Ordering::SeqCst);
    let published_before = worker.publications.load(Ordering::Acquire);
    drop(connection);
    let loaded = quiet_until(&worker, "loaded", serial, 8, 10, || {
        let observed = sample.lock().unwrap();
        observed.preview_visible && observed.body.is_some()
    });
    let ready = sample.lock().unwrap().clone();
    assert_eq!(ready.row, initial.row);
    assert!(!ready.status_visible && ready.preview_visible, "{ready:?}");
    let body = ready
        .body
        .as_ref()
        .expect("The actual Cell body never reached the retained inspector");
    assert_eq!(body.1, BODY);
    assert!(loaded.frame.input.editor.is_none());
    assert!(notifications.load(Ordering::SeqCst) > notified_before);
    let loaded_pixels = pixels(&loaded, "loaded");
    let published = worker.publications.load(Ordering::Acquire) - published_before;
    assert!(
        (1..=8).contains(&published),
        "Body arrival published {published} frames"
    );
    assert!(
        loaded_pixels != loading_pixels,
        "The loading feedback never changed its published pixels"
    );
    let notified_before = notifications.load(Ordering::SeqCst);
    let published_before = worker.publications.load(Ordering::Acquire);
    runtime
        .block_on(engine.act(engine::actions::Action::DeleteRecord { target: uid }, None))
        .unwrap();
    let deleted = quiet_until(&worker, "deleted", serial, 8, 10, || {
        let observed = sample.lock().unwrap();
        observed.status_visible
            && observed.status.contains("unavailable")
            && !observed.preview_visible
    });
    let missing = sample.lock().unwrap().clone();
    assert_eq!(missing.row, initial.row);
    assert!(
        missing.status_visible && missing.status.contains("unavailable"),
        "{missing:?}"
    );
    assert!(!missing.preview_visible, "{missing:?}");
    let retained = missing
        .body
        .as_ref()
        .expect("Deletion removed the original draft/editor");
    assert_eq!(retained.0, body.0);
    assert_eq!(retained.1, BODY);
    assert!(
        !retained.2,
        "A missing Record's cached editor remained visible"
    );
    assert!(deleted.frame.input.editor.is_none());
    assert!(notifications.load(Ordering::SeqCst) > notified_before);
    let deleted_pixels = pixels(&deleted, "deleted");
    let published = worker.publications.load(Ordering::Acquire) - published_before;
    assert!(
        (1..=8).contains(&published),
        "Deletion published {published} frames"
    );
    assert!(
        deleted_pixels != loaded_pixels,
        "The unavailable feedback never changed its published pixels"
    );
    worker.stop().unwrap();
}
