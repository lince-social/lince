use bevy::{
    a11y::AccessibilityNode,
    input_focus::{FocusCause, InputFocus},
    math::DVec2,
    prelude::*,
    render::{
        gpu_readback::{Readback, ReadbackComplete},
        view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    },
    ui_widgets::Activate,
    winit::WinitSettings,
};
use lince_desktop::{
    actions::{Action, ActionButton},
    area::InfluenceArea,
    canvas::{CanvasItem, CanvasView},
    canvas_selection::SandSelection,
    castle::StartupStatus,
    edit_mode::EditAction,
    laboratory::{
        LaboratoryAction,
        components::{descendants, entries},
    },
    protein_area::RecordBinding,
    topology::presentation::Surface,
    workspace::{WorkspaceMember, Workspaces},
};
use serde::Serialize;
use std::{collections::HashSet, path::PathBuf, sync::Arc, time::Instant};

#[derive(Serialize)]
struct ResultRow {
    component: String,
    surfaces: usize,
    colors: Vec<usize>,
    controls: Vec<String>,
    errors: Vec<String>,
    screenshot: String,
}

#[derive(Resource)]
struct Trial {
    root: Option<Entity>,
    names: Vec<String>,
    index: usize,
    stage: u8,
    ticks: usize,
    settled: usize,
    started: Instant,
    items: Vec<Entity>,
    pending: usize,
    results: Vec<ResultRow>,
    output: PathBuf,
    gallery_only: bool,
}

fn items(world: &World, root: Entity) -> Vec<Entity> {
    let active = world.get::<Workspaces>(root).unwrap().active;
    world
        .get::<Children>(root)
        .into_iter()
        .flatten()
        .filter(|entity| {
            world.get::<CanvasItem>(**entity).is_some()
                && world
                    .get::<WorkspaceMember>(**entity)
                    .is_some_and(|member| member.0 == active)
        })
        .copied()
        .collect()
}

fn finish_case(trial: &mut Trial) {
    trial.stage = 3;
    trial.ticks = 0;
}

fn exercise(world: &mut World) {
    let mut trial = world.remove_resource::<Trial>().unwrap();
    trial.ticks += 1;
    if trial.root.is_none() && trial.ticks < 12 {
        world.insert_resource(trial);
        return;
    }

    match trial.stage {
        0 => {
            let root = match trial.root {
                Some(root) => root,
                None => {
                    let Some(root) = world
                        .query_filtered::<Entity, With<Workspaces>>()
                        .iter(world)
                        .next()
                    else {
                        world.insert_resource(trial);
                        return;
                    };
                    lince_desktop::workspace::create(world, root);
                    trial.root = Some(root);
                    root
                }
            };
            EditAction::Open.apply(world, root);
            EditAction::Store.apply(world, root);
            let catalogue = entries(world);
            if trial.names.is_empty() {
                trial.names = catalogue
                    .iter()
                    .map(|(_, component)| component.title.clone())
                    .collect();
                if trial.gallery_only {
                    trial.index = trial.names.len();
                }
            }
            if trial.index == trial.names.len() {
                std::fs::write(
                    trial.output.join("report.json"),
                    serde_json::to_vec_pretty(&trial.results).unwrap(),
                )
                .unwrap();
                EditAction::General.apply(world, root);
                EditAction::Close.apply(world, root);
                LaboratoryAction::Open.apply(world, root);
                LaboratoryAction::Components.apply(world, root);
                for _ in 0..trial.names.len() {
                    if world
                        .query::<&Text>()
                        .iter(world)
                        .any(|text| text.0.ends_with("· Command Castle"))
                    {
                        break;
                    }
                    LaboratoryAction::NextComponent.apply(world, root);
                }
                trial.stage = 7;
                trial.ticks = 0;
            } else {
                let name = &trial.names[trial.index];
                let card = catalogue
                    .iter()
                    .find(|(_, component)| &component.title == name)
                    .unwrap()
                    .0;
                eprintln!("Checking {name}");
                trial.results.push(ResultRow {
                    component: name.clone(),
                    surfaces: 0,
                    colors: Vec::new(),
                    controls: Vec::new(),
                    errors: Vec::new(),
                    screenshot: format!("{:02}.png", trial.index),
                });
                world.trigger(Activate { entity: card });
                trial.ticks = 0;
                trial.started = Instant::now();
                trial.stage = 1;
                trial.settled = 0;
            }
        }
        1 => {
            let root = trial.root.unwrap();
            if trial.ticks == 1 {
                EditAction::Open.apply(world, root);
                EditAction::General.apply(world, root);
                EditAction::Close.apply(world, root);
            }
            if trial.ticks == 1 && trial.names[trial.index] == "Operation" {
                let input = items(world, root)
                    .into_iter()
                    .flat_map(|owner| descendants(world, owner))
                    .find(|entity| world.get::<bevy::text::EditableText>(*entity).is_some())
                    .unwrap();
                world
                    .get_mut::<bevy::text::EditableText>(input)
                    .unwrap()
                    .editor
                    .set_text("@laboratory-sample");
            }

            trial.items = items(world, root);
            let surfaces: Vec<_> = trial
                .items
                .iter()
                .copied()
                .filter(|entity| world.get::<InfluenceArea>(*entity).is_none())
                .collect();
            let bound = world.query::<&RecordBinding>().iter(world).count();
            let needs_record = matches!(
                trial.names[trial.index].as_str(),
                "Command Castle" | "Fiote Castle" | "Relation Castle"
            );
            let ready = !surfaces.is_empty()
                && (!needs_record || bound > 0)
                && surfaces.iter().all(|entity| {
                    world.get::<Surface>(*entity).is_some_and(|surface| {
                        world.get::<Visibility>(surface.visual) == Some(&Visibility::Visible)
                    }) && world
                        .get::<InheritedVisibility>(*entity)
                        .is_some_and(|visible| visible.get())
                        && world
                            .get::<ComputedNode>(*entity)
                            .is_some_and(|node| node.size().min_element() > 0.0)
                        && world
                            .get::<StartupStatus>(*entity)
                            .is_none_or(|status| status.0.is_empty())
                });
            trial.settled = if ready { trial.settled + 1 } else { 0 };
            if trial.settled == 8 {
                let mut min = DVec2::splat(f64::INFINITY);
                let mut max = DVec2::splat(f64::NEG_INFINITY);
                let mut controls = Vec::new();
                for entity in &trial.items {
                    let item = world.get::<CanvasItem>(*entity).unwrap();
                    assert!(item.position.is_finite() && item.size.is_finite());
                    min = min.min(item.position - item.size.as_dvec2() * 0.5);
                    max = max.max(item.position + item.size.as_dvec2() * 0.5);
                }
                for entity in &surfaces {
                    for child in descendants(world, *entity) {
                        if let Some(node) = world.get::<AccessibilityNode>(child)
                            && let Some(label) = node.label()
                        {
                            controls.push(label.into());
                        }
                    }
                }
                let viewport = world
                    .get::<ComputedUiRenderTargetInfo>(root)
                    .unwrap()
                    .logical_size();
                let mut view = world.get_mut::<CanvasView>(root).unwrap();
                view.center = (min + max) * 0.5;
                let size = (max - min).as_vec2();
                view.zoom = ((viewport.x - 100.0) / size.x)
                    .min((viewport.y - 100.0) / size.y)
                    .clamp(0.05, 1.0) as f64;
                let result = trial.results.last_mut().unwrap();
                result.surfaces = surfaces.len();
                result.controls = controls;
                for entity in surfaces
                    .iter()
                    .flat_map(|entity| descendants(world, *entity))
                {
                    if world.get::<ActionButton>(entity).is_none() {
                        continue;
                    }
                    let mut ancestor = Some(entity);
                    let mut displayed = true;
                    while let Some(current) = ancestor {
                        if world
                            .get::<Node>(current)
                            .is_some_and(|node| node.display == Display::None)
                        {
                            displayed = false;
                            break;
                        }
                        ancestor = world.get::<ChildOf>(current).map(ChildOf::parent);
                    }
                    if displayed
                        && world.get::<ComputedNode>(entity).is_some_and(|node| {
                            !node.size().is_finite() || node.size().min_element() <= 0.0
                        })
                    {
                        let label = world
                            .get::<AccessibilityNode>(entity)
                            .and_then(|node| node.label())
                            .unwrap_or("unnamed");
                        result
                            .errors
                            .push(format!("Control has no clickable size: {label}"));
                    }
                }

                for expected in match result.component.as_str() {
                    "Command Castle" => &["Run", "Bash script"][..],
                    "Fiote Castle" => &["Manage Fiote"][..],
                    _ => &[],
                } {
                    let found = surfaces
                        .iter()
                        .flat_map(|entity| descendants(world, *entity))
                        .any(|entity| {
                            world
                                .get::<Text>(entity)
                                .is_some_and(|text| &text.0 == expected)
                        });
                    if !found {
                        result.errors.push(format!("Missing control: {expected}"));
                    }
                }
                trial.pending = surfaces.len();
                for entity in surfaces {
                    let surface = world.get::<Surface>(entity).unwrap();
                    let texture = world
                        .resource::<Assets<StandardMaterial>>()
                        .get(&surface.material)
                        .unwrap()
                        .base_color_texture
                        .clone()
                        .unwrap();
                    world.spawn(Readback::texture(texture)).observe(
                        |event: On<ReadbackComplete>,
                         mut commands: Commands,
                         mut trial: ResMut<Trial>| {
                            let colors: HashSet<_> = event
                                .data
                                .chunks_exact(4)
                                .filter(|pixel| pixel[3] != 0)
                                .collect();
                            let result = trial.results.last_mut().unwrap();
                            result.colors.push(colors.len());
                            let minimum = if result.component == "Square" { 1 } else { 8 };
                            if colors.len() < minimum {
                                result.errors.push(format!(
                                    "Empty rendered surface: {} colors",
                                    colors.len()
                                ));
                            }
                            trial.pending -= 1;
                            if trial.pending == 0 {
                                trial.stage = 2;
                                trial.ticks = 0;
                            }
                            commands.entity(event.entity).despawn();
                        },
                    );
                }
                trial.stage = 5;
            } else if trial.started.elapsed().as_secs() > 20 {
                let issues: Vec<_> = world
                    .query::<(Entity, &StartupStatus)>()
                    .iter(world)
                    .filter(|(_, status)| !status.0.is_empty())
                    .map(|(entity, status)| format!("{entity}: {:?}", status.0))
                    .collect();
                trial.results.last_mut().unwrap().errors.push(format!("Content did not appear; {} canvas items, {} UI surfaces, {bound} record bindings; {issues:?}", trial.items.len(), surfaces.len()));
                finish_case(&mut trial);
            }
        }
        2 if trial.ticks >= 4 => {
            let path = trial.output.join(&trial.results.last().unwrap().screenshot);
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path))
                .observe(|_: On<ScreenshotCaptured>, mut trial: ResMut<Trial>| {
                    if trial.results.last().unwrap().component == "Command Castle" {
                        trial.stage = 9;
                        trial.ticks = 0;
                        trial.started = Instant::now();
                    } else {
                        finish_case(&mut trial);
                    }
                });
            trial.stage = 5;
        }
        3 => {
            let root = trial.root.unwrap();
            EditAction::Open.apply(world, root);
            let selected = items(world, root);
            world.entity_mut(root).insert(SandSelection(selected));
            world
                .resource_mut::<InputFocus>()
                .set(root, FocusCause::Pressed);
            lince_desktop::deletion::DeleteSelected.apply(world, root);
            if let Some(delete) = world
                .query::<(Entity, &AccessibilityNode)>()
                .iter(world)
                .find(|(_, node)| node.label() == Some("Delete"))
                .map(|(entity, _)| entity)
            {
                world.trigger(Activate { entity: delete });
            }
            trial.stage = 4;
            trial.ticks = 0;
        }
        4 if trial.ticks >= 15 => {
            let root = trial.root.unwrap();
            let remaining = items(world, root);
            if !remaining.is_empty() {
                trial
                    .results
                    .last_mut()
                    .unwrap()
                    .errors
                    .push(format!("Deletion left {} canvas items", remaining.len()));
                for entity in remaining {
                    world.despawn(entity);
                }
            }
            let result = trial.results.last().unwrap();
            eprintln!(
                "{} {}: {} rendered surfaces, colors {:?}; {:?}",
                if result.errors.is_empty() {
                    "PASS"
                } else {
                    "FAIL"
                },
                result.component,
                result.surfaces,
                result.colors,
                result.errors
            );
            let workspace = world.get::<Workspaces>(root).unwrap().active;
            lince_desktop::workspace_config::set_physics(world, root, workspace, false);
            trial.index += 1;
            trial.stage = 0;
            trial.ticks = 0;
        }
        9 => {
            if trial.ticks == 1 {
                let run = trial
                    .items
                    .iter()
                    .flat_map(|owner| descendants(world, *owner))
                    .find(|entity| {
                        world
                            .get::<AccessibilityNode>(*entity)
                            .is_some_and(|node| node.label() == Some("Run"))
                    })
                    .expect("Command Run button");
                world.trigger(Activate { entity: run });
            }
            let completed = world
                .query_filtered::<Entity, With<lince_desktop::terminal::TerminalSand>>()
                .iter(world)
                .any(|entity| {
                    lince_desktop::terminal::displayed_text(world, entity).contains("Hello")
                });
            if completed {
                eprintln!("Command Castle Run button produced Hello in its terminal");
                finish_case(&mut trial);
            } else if trial.started.elapsed().as_secs() > 15 {
                trial
                    .results
                    .last_mut()
                    .unwrap()
                    .errors
                    .push("Run did not produce the Command's Hello output".into());
                finish_case(&mut trial);
            }
        }
        7 if trial.ticks >= 15 => {
            let preview = world
                .query::<(Entity, &Name)>()
                .iter(world)
                .find(|(_, name)| name.as_str() == "Laboratory component preview")
                .unwrap()
                .0;
            assert!(
                world
                    .get::<UiTransform>(preview)
                    .unwrap()
                    .scale
                    .min_element()
                    > 0.5
            );
            assert!(
                world
                    .get::<ComputedNode>(preview)
                    .unwrap()
                    .size()
                    .min_element()
                    > 300.0
            );
            assert!(
                world
                    .query::<&Text>()
                    .iter(world)
                    .any(|text| text.0 == "Bash script")
            );
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(trial.output.join("laboratory-gallery.png")))
                .observe(|_: On<ScreenshotCaptured>, mut trial: ResMut<Trial>| {
                    trial.stage = 8;
                });
            trial.stage = 6;
        }
        8 => {
            LaboratoryAction::Close.apply(world, trial.root.unwrap());
            let failures = trial
                .results
                .iter()
                .filter(|row| !row.errors.is_empty())
                .count();
            eprintln!(
                "Component gallery: {} checked, {failures} failed",
                trial.results.len()
            );
            world.write_message(if failures == 0 {
                AppExit::Success
            } else {
                AppExit::error()
            });
            trial.stage = 6;
        }
        _ => {}
    }
    world.insert_resource(trial);
}

fn main() {
    let arguments: Vec<_> = std::env::args().collect();
    let output = PathBuf::from(arguments.get(1).expect("provide output directory"));
    std::fs::create_dir_all(&output).unwrap();
    if let Some(index) = arguments.iter().position(|arg| arg == "--case") {
        let filter = arguments.get(index + 1).expect("provide case name").clone();
        let results = std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .thread_stack_size(32 * 1024 * 1024)
                    .enable_all()
                    .build()
                    .unwrap();
                lince_desktop::laboratory::catalogue()
                    .into_iter()
                    .filter(|case| case.name.contains(&filter))
                    .map(|case| case.execute(&runtime))
                    .collect::<Vec<_>>()
            })
            .unwrap()
            .join()
            .unwrap();
        assert!(!results.is_empty(), "No matching Laboratory cases");
        std::fs::write(
            output.join("cases.json"),
            serde_json::to_vec_pretty(&results).unwrap(),
        )
        .unwrap();
        for result in &results {
            eprintln!("{}: {:?}", result.name, result.error);
        }
        if results.iter().any(|result| result.error.is_some()) {
            std::process::exit(1);
        }
        return;
    }
    if arguments.iter().any(|arg| arg == "--headless") {
        let report =
            lince_desktop::laboratory::run_headless(lince_desktop::laboratory::StressConfig {
                max_sands: 64,
                batch: 32,
                warmup_frames: 2,
                sample_frames: 5,
                budget_ms: 1000.0,
            })
            .unwrap();
        std::fs::write(
            output.join("laboratory.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        let failures = report
            .behavior
            .iter()
            .filter(|row| row.error.is_some())
            .count();
        eprintln!(
            "Laboratory: {} behavior checks, {failures} failed",
            report.behavior.len()
        );
        if failures > 0 {
            std::process::exit(1);
        }
        return;
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let (cell, _directory) = runtime.block_on(async {
        let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
        engine
            .act(
                engine::actions::Action::CreateRecord {
                    slug: Some("laboratory-sample".into()),
                    kind: nucleus::RecordKind::Plain,
                    head: "Laboratory sample Record".into(),
                    body: "Visible content for component checks".into(),
                    quantity: 1.0,
                },
                None,
            )
            .await
            .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let fiote = Arc::new(
            cell::fiote::Host::open(engine.clone(), directory.path().into())
                .await
                .unwrap(),
        );
        (
            cell::CellRuntime {
                store: engine.store.clone(),
                engine,
                lanes: Arc::new(cell::LaneHub::new()),
                wire: Default::default(),
                commands: cell::terminal::commands::CommandHost::new(directory.path().into()),
                speech: None,
                fiote: Some(fiote),
                information: None,
            },
            directory,
        )
    });
    let _entered = runtime.enter();
    runtime.spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(1800)).await;
        eprintln!("All-components smoke timed out");
        std::process::exit(1);
    });
    let mut app = lince_desktop::app::connected_app(cell);
    app.insert_resource(WinitSettings::continuous())
        .insert_resource(Trial {
            root: None,
            names: Vec::new(),
            index: 0,
            stage: 0,
            ticks: 0,
            settled: 0,
            started: Instant::now(),
            items: Vec::new(),
            pending: 0,
            results: Vec::new(),
            output,
            gallery_only: arguments.iter().any(|arg| arg == "--gallery-only"),
        });
    app.add_systems(
        Update,
        exercise.after(lince_desktop::topology::presentation::synchronize),
    );
    app.run();
}
