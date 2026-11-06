use super::*;
use bevy::{
    app::{Main, MainScheduleOrder},
    ecs::schedule::{
        MultiThreadedExecutor, Schedule, ScheduleLabel, Schedules, SingleThreadedExecutor,
    },
    input::keyboard::Key,
    input_focus::{FocusCause, InputFocus},
};
use std::{
    hash::{Hash, Hasher},
    sync::Mutex,
};

mod bodies;

#[derive(Clone, Copy, serde::Serialize)]
struct Reading {
    wall_ms: f64,
    process_cpu_ms: f64,
    owner_cpu_ms: f64,
}

struct Clock {
    wall: Instant,
    process: rustix::time::Timespec,
    owner: rustix::time::Timespec,
}

impl Clock {
    fn start() -> Self {
        Self {
            wall: Instant::now(),
            process: rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime),
            owner: rustix::time::clock_gettime(rustix::time::ClockId::ThreadCPUTime),
        }
    }

    fn finish(self) -> Reading {
        let process = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
        let owner = rustix::time::clock_gettime(rustix::time::ClockId::ThreadCPUTime);
        let delta = |after: rustix::time::Timespec, before: rustix::time::Timespec| {
            (after.tv_sec - before.tv_sec) as f64 * 1000.0
                + (after.tv_nsec - before.tv_nsec) as f64 / 1_000_000.0
        };
        Reading {
            wall_ms: self.wall.elapsed().as_secs_f64() * 1000.0,
            process_cpu_ms: delta(process, self.process),
            owner_cpu_ms: delta(owner, self.owner),
        }
    }
}

fn summary(readings: &[Reading]) -> serde_json::Value {
    let stats = |field: fn(&Reading) -> f64| {
        let mut values = readings.iter().map(field).collect::<Vec<_>>();
        values.sort_by(f64::total_cmp);
        serde_json::json!({
            "median":values[values.len()/2],
            "p95":values[(values.len()*95/100).min(values.len()-1)],
            "total":values.iter().sum::<f64>(),
        })
    };
    serde_json::json!({"count":readings.len(),"wall_ms":stats(|r|r.wall_ms),"process_cpu_ms":stats(|r|r.process_cpu_ms),"owner_cpu_ms":stats(|r|r.owner_cpu_ms)})
}

fn pixels(canvas: &mut Presentation) -> Vec<Vec<u8>> {
    let frame = canvas.probe_frame().unwrap();
    (1..=2)
        .map(|id| {
            let image = if frame.output == Some(lince_interface::host::OutputId(id)) {
                frame.dmabuf().unwrap()
            } else {
                frame
                    .outputs
                    .iter()
                    .find(|output| output.id == lince_interface::host::OutputId(id))
                    .unwrap()
                    .dmabuf()
            };
            probe_pixels(image).unwrap()
        })
        .collect()
}

fn input(canvas: &mut Presentation, editor: Entity) {
    canvas
        .world_mut()
        .resource_mut::<InputFocus>()
        .set(editor, FocusCause::Navigated);
    for (physical, value, expected) in [(KeyCode::KeyZ, "z", "z"), (KeyCode::KeyE, "é", "zé")] {
        canvas
            .key(
                physical,
                Key::Character(value.into()),
                Some(value.into()),
                true,
                false,
            )
            .unwrap();
        canvas
            .key(physical, Key::Character(value.into()), None, false, false)
            .unwrap();
        let frame = canvas.probe_frame().unwrap();
        assert_eq!(
            canvas
                .app
                .world()
                .get::<bevy::text::EditableText>(editor)
                .unwrap()
                .value(),
            expected
        );
        assert_eq!(frame.input.editor.as_ref().unwrap().surrounding, expected);
        let bytes = probe_pixels(frame.dmabuf().unwrap()).unwrap();
        assert!(
            bytes
                .chunks_exact(4)
                .filter(|pixel| pixel[0] > 230 && pixel[1] < 30 && pixel[2] > 230)
                .count()
                > 1000
        );
    }
    for _ in 0..2 {
        canvas
            .key(KeyCode::Backspace, Key::Backspace, None, true, false)
            .unwrap();
        canvas
            .key(KeyCode::Backspace, Key::Backspace, None, false, false)
            .unwrap();
        drop(canvas.probe_frame().unwrap());
    }
    assert!(
        canvas
            .app
            .world()
            .get::<bevy::text::EditableText>(editor)
            .unwrap()
            .value()
            .chars()
            .next()
            .is_none()
    );
    canvas.world_mut().resource_mut::<InputFocus>().clear();
    for _ in 0..4 {
        drop(canvas.probe_frame().unwrap());
    }
}

#[test]
#[ignore = "private Vulkan/EGL main-schedule attribution and executor comparison; no concurrent compilation"]
fn native_main_phases_attribute_cpu_and_compare_the_largest_executor() {
    let checks = native::laboratory::run_interface_checks().unwrap();
    for name in [
        "::layout_snapshots_track_optional_components_commands_and_removals",
        "::presented_castles_keep_their_surface_while_replacement_content_settles",
    ] {
        assert!(checks.iter().any(|check| check.name.ends_with(name)));
    }
    assert!(
        checks[0]
            .name
            .ends_with("::protein_area_filter_visible_and_selectable")
    );
    assert!(
        checks.iter().all(|check| check.error.is_none()),
        "{checks:?}"
    );
    let bodies = Arc::new(bodies::Probe::default());
    if std::env::var_os("LINCE_PARK_TRACE_SYSTEMS").is_some() {
        use bevy::log::tracing_subscriber::{Layer, layer::SubscriberExt};
        let layer = bevy::log::tracing_subscriber::fmt::layer()
            .with_span_events(bevy::log::tracing_subscriber::fmt::format::FmtSpan::CLOSE)
            .with_ansi(false)
            .with_timer(())
            .with_writer(std::io::stderr)
            .with_filter(bevy::log::tracing_subscriber::filter::filter_fn(
                |metadata| metadata.is_span() && metadata.name() == "system",
            ));
        let subscriber = bevy::log::tracing_subscriber::registry().with(layer).with(
            bodies::Layer(bodies.clone()).with_filter(
                bevy::log::tracing_subscriber::filter::filter_fn(|metadata| {
                    metadata.is_span() && metadata.name() == "system"
                }),
            ),
        );
        bevy::log::tracing::subscriber::set_global_default(subscriber).unwrap();
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let (mut canvas, owners) = super::offscreen::fixture(directory.path(), "visible-only", 0);
    let world = canvas.world_mut();
    let editor = world
        .spawn((
            native::sand::single_line_editor(
                "",
                world.resource::<native::theme::Typography>(),
                0,
                32,
            ),
            ChildOf(owners[0].0),
        ))
        .id();
    world.entity_mut(editor).insert(Node {
        position_type: PositionType::Absolute,
        left: px(4),
        top: px(48),
        width: px(60),
        height: px(28),
        ..default()
    });
    let marker = owners[0].0;
    canvas.app.add_systems(
        PostUpdate,
        (move |texts: Query<&bevy::text::EditableText>,
               mut colors: Query<&mut BackgroundColor>| {
            colors.get_mut(marker).unwrap().set_if_neq(BackgroundColor(
                if texts.get(editor).unwrap().value().chars().next().is_none() {
                    Color::srgb(0.0, 1.0, 0.0)
                } else {
                    Color::srgb(1.0, 0.0, 1.0)
                },
            ));
        })
        .after(bevy::text::EditableTextSystems)
        .before(bevy::ui::UiSystems::Layout),
    );
    for _ in 0..12 {
        drop(canvas.probe_frame().unwrap());
    }
    let reference = pixels(&mut canvas);
    for bytes in &reference {
        assert!(
            bytes
                .chunks_exact(4)
                .filter(|pixel| pixel[0] < 30 && pixel[1] > 230 && pixel[2] < 30)
                .count()
                > 20000
        );
        let glyphs = (0..8)
            .flat_map(|index| {
                (200..280).flat_map(move |y| {
                    ((34 + index * 72)..(102 + index * 72)).map(move |x| (y * 640 + x) * 4)
                })
            })
            .filter(|offset| {
                bytes[*offset..*offset + 3]
                    .iter()
                    .all(|channel| *channel < 180)
            })
            .count();
        assert!(glyphs > 200, "Reference labels are not drawn: {glyphs}");
    }
    let mut original = Vec::new();
    bodies.start();
    for _ in 0..24 {
        let clock = Clock::start();
        drop(canvas.probe_frame().unwrap());
        original.push(clock.finish());
    }
    let warmed_bodies = bodies.stop();
    if std::env::var_os("LINCE_PARK_TRACE_SYSTEMS").is_some() {
        for name in [
            "bevy_ui::layout::ui_layout_system",
            "lince_interface::native::layout::engine::resolve",
            "lince_interface::native::topology::presentation::synchronize",
        ] {
            assert_eq!(
                warmed_bodies
                    .iter()
                    .filter(|(body, _)| body == name)
                    .count(),
                24,
                "Missing warmed system executions: {name}"
            );
        }
    }
    let labels = canvas
        .app
        .world()
        .resource::<MainScheduleOrder>()
        .labels
        .clone();
    let inventory = labels.iter().map(|label| {
        let schedules = canvas.app.world().resource::<Schedules>();
        let systems = schedules.get(*label).map(|schedule| schedule.systems().unwrap().map(|(_,system)| {
            serde_json::json!({"name":system.name().to_string(),"exclusive":system.is_exclusive(),"send":system.is_send()})
        }).collect::<Vec<_>>()).unwrap_or_default();
        serde_json::json!({"schedule":format!("{label:?}"), "systems":systems})
    }).collect::<Vec<_>>();
    let main = canvas
        .world_mut()
        .resource_mut::<Schedules>()
        .remove(Main)
        .unwrap();
    let main_systems = main
        .systems()
        .unwrap()
        .map(|(_, system)| system.name().to_string())
        .collect::<Vec<_>>();
    assert_eq!(main_systems.len(), 1);
    assert!(
        main_systems[0].ends_with("Main::run_main")
            || main_systems[0] == "<Enable the debug feature to see the name>",
        "{main_systems:?}"
    );
    let observations = Arc::new(Mutex::new(Vec::<Vec<Reading>>::new()));
    let sink = observations.clone();
    let mut observed_main = Schedule::new(Main);
    observed_main.set_executor(SingleThreadedExecutor::new());
    observed_main.add_systems(move |world: &mut World| {
        let readings = world.resource_scope(|world, order: Mut<MainScheduleOrder>| {
            order
                .labels
                .iter()
                .map(|label| {
                    let clock = Clock::start();
                    let _ = world.try_run_schedule(*label);
                    clock.finish()
                })
                .collect::<Vec<_>>()
        });
        sink.lock().unwrap().push(readings);
    });
    canvas
        .world_mut()
        .resource_mut::<Schedules>()
        .insert(observed_main);
    for _ in 0..8 {
        drop(canvas.probe_frame().unwrap());
    }
    observations.lock().unwrap().clear();
    let mut observed = Vec::new();
    for _ in 0..24 {
        let clock = Clock::start();
        drop(canvas.probe_frame().unwrap());
        observed.push(clock.finish());
    }
    let baseline = std::mem::take(&mut *observations.lock().unwrap());
    assert_eq!(baseline.len(), 24);
    let attribution = labels.iter().enumerate().map(|(index,label)| {
        let values = baseline.iter().map(|frame|frame[index]).collect::<Vec<_>>();
        serde_json::json!({"schedule":format!("{label:?}"),"summary":summary(&values),"samples":values})
    }).collect::<Vec<_>>();
    let selected = attribution
        .iter()
        .enumerate()
        .max_by(|(_, left), (_, right)| {
            left["summary"]["wall_ms"]["median"]
                .as_f64()
                .unwrap()
                .total_cmp(&right["summary"]["wall_ms"]["median"].as_f64().unwrap())
        })
        .unwrap()
        .0;
    let label = labels[selected];
    assert!(
        [
            First.intern(),
            PreUpdate.intern(),
            Update.intern(),
            PostUpdate.intern(),
            Last.intern()
        ]
        .contains(&label),
        "Selected phase does not use the default executor: {label:?}"
    );
    let mut comparisons = Vec::new();
    for block in 0..8 {
        for single in if block % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            {
                let mut schedules = canvas.world_mut().resource_mut::<Schedules>();
                let schedule = schedules.get_mut(label).unwrap();
                if single {
                    schedule.set_executor(SingleThreadedExecutor::new());
                } else {
                    schedule.set_executor(MultiThreadedExecutor::new());
                }
            }
            for _ in 0..4 {
                drop(canvas.probe_frame().unwrap());
            }
            observations.lock().unwrap().clear();
            let mut full = Vec::new();
            for _ in 0..8 {
                let clock = Clock::start();
                drop(canvas.probe_frame().unwrap());
                full.push(clock.finish());
            }
            let phases = std::mem::take(&mut *observations.lock().unwrap());
            assert_eq!(phases.len(), 8);
            assert_eq!(
                pixels(&mut canvas),
                reference,
                "Static pixels changed: block {block}, single {single}"
            );
            comparisons.push(
                serde_json::json!({"block":block,"single":single,"full":full,"phases":phases}),
            );
        }
    }
    for single in [false, true] {
        let mut schedules = canvas.world_mut().resource_mut::<Schedules>();
        let schedule = schedules.get_mut(label).unwrap();
        if single {
            schedule.set_executor(SingleThreadedExecutor::new());
        } else {
            schedule.set_executor(MultiThreadedExecutor::new());
        }
        drop(schedules);
        input(&mut canvas, editor);
        assert_eq!(
            pixels(&mut canvas),
            reference,
            "Input did not restore static pixels: single {single}"
        );
    }
    canvas.world_mut().resource_mut::<Schedules>().insert(main);
    for _ in 0..4 {
        drop(canvas.probe_frame().unwrap());
    }
    assert_eq!(pixels(&mut canvas), reference);
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    reference.hash(&mut hash);
    let result = serde_json::json!({
        "scope":"Private debug native fixture, two 640x480 outputs, 16 visible 34-node native trees and one editor. Full update/render/export/completion. Phase timings include clock overhead and instrumented Main dispatcher. Process CPU includes other renderer workers, not only the phase. Scheduling variants change only the largest main phase. No desktop FPS, optimized gain, or hardware GPU timing claim.",
        "compute_workers":bevy::tasks::ComputeTaskPool::get().thread_num(),
        "system_names_available":main_systems[0].ends_with("Main::run_main"),"inventory":inventory,"original_full":summary(&original),"original_samples":original,
        "instrumented_full":summary(&observed),"instrumented_samples":observed,
        "warmed_system_bodies":warmed_bodies,
        "attribution":attribution,"selected_index":selected,"selected_schedule":format!("{label:?}"),
        "comparisons":comparisons,"reference_hash":hash.finish(),"interface_cases":checks.iter().map(|check|&check.name).collect::<Vec<_>>(),
        "correctness":"Exact pixels including glyphs on both outputs at every variant block; ordered ASCII and Unicode input reflected in editor state and same-frame marker pixels under both executors; baseline restored after deletion and original Main restored without rerunning startup. Marker is not a glyph-paint oracle."
    });
    let path = std::path::PathBuf::from(
        std::env::var_os("LINCE_PARK_TEST_ARTIFACTS").expect("Set a private artifact directory"),
    );
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(
        path.join("native-main-schedules.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
    eprintln!(
        "main-attribution: {}",
        serde_json::json!({"selected":result["selected_schedule"],"attribution":result["attribution"],"original":result["original_full"],"instrumented":result["instrumented_full"]})
    );
    canvas.finish().unwrap();
}
