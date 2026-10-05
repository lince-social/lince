use bevy::{
    a11y::AccessibilityNode,
    prelude::*,
    render::{
        gpu_readback::{Readback, ReadbackComplete},
        view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    },
    ui_widgets::Activate,
};
use lince_desktop::{
    actions::{Action, ActionButton},
    area::InfluenceArea,
    castle::StartupStatus,
    edit_mode::EditAction,
    protein_area::RecordBinding,
    workspace::Workspaces,
};
use std::{collections::HashSet, sync::Arc};

#[derive(Resource)]
struct Trial {
    stage: u8,
    ticks: usize,
    root: Option<Entity>,
    area: Option<Entity>,
    saved: Option<InfluenceArea>,
    restored: bool,
    path: String,
}

fn exercise(world: &mut World) {
    let mut trial = world.remove_resource::<Trial>().unwrap();
    trial.ticks += 1;
    if trial.ticks.is_multiple_of(10) {
        eprintln!("Fiote stage {} tick {}", trial.stage, trial.ticks);
    }
    if trial.ticks.is_multiple_of(120) {
        for (entity, binding, item, surface) in world
            .query::<(
                Entity,
                &RecordBinding,
                &lince_desktop::canvas::CanvasItem,
                Option<&lince_desktop::topology::presentation::Surface>,
            )>()
            .iter(world)
        {
            if Some(binding.area) == trial.area {
                eprintln!(
                    "Fiote row {entity}: {:?} {:?}, surface {:?}, visibility {:?}",
                    item.position,
                    item.size,
                    surface.map(|s| (s.visible, world.get::<Visibility>(s.visual))),
                    world.get::<InheritedVisibility>(entity)
                );
            }
        }
        for (entity, status) in world.query::<(Entity, &StartupStatus)>().iter(world) {
            if !status.0.is_empty() {
                eprintln!("Startup {entity}: {:?}", status.0);
            }
        }
    }
    assert!(
        trial.ticks < 1200,
        "Fiote Castle did not finish stage {}",
        trial.stage
    );
    match trial.stage {
        0 => {
            if let Some(root) = world
                .query_filtered::<Entity, With<Workspaces>>()
                .iter(world)
                .next()
            {
                lince_desktop::workspace::create(world, root);
                world
                    .get_mut::<lince_desktop::canvas::CanvasView>(root)
                    .unwrap()
                    .zoom = 0.6;
                EditAction::Open.apply(world, root);
                EditAction::Store.apply(world, root);
                let card = world
                    .query::<(Entity, &ActionButton, &Children)>()
                    .iter(world)
                    .find(|(_, _, children)| {
                        children.iter().any(|child| {
                            world.get::<Children>(child).is_some_and(|children| {
                                children.iter().any(|child| {
                                    world
                                        .get::<Text>(child)
                                        .is_some_and(|text| text.0 == "Fiote Castle")
                                })
                            })
                        })
                    })
                    .map(|(entity, _, _)| entity)
                    .unwrap();
                world.trigger(Activate { entity: card });
                trial.root = Some(root);
                trial.stage = 1;
                trial.ticks = 0;
            }
        }
        1 => {
            let area = world
                .query::<(Entity, &InfluenceArea)>()
                .iter(world)
                .find(|(_, area)| area.name == "Fiote Castle")
                .map(|(entity, _)| entity);
            if let Some(area) = area {
                trial.area = Some(area);
                let row = world
                    .query::<(
                        Entity,
                        &RecordBinding,
                        &InheritedVisibility,
                        &ComputedNode,
                        &StartupStatus,
                        &lince_desktop::topology::presentation::Surface,
                    )>()
                    .iter(world)
                    .find(|(_, binding, visible, node, status, surface)| {
                        binding.area == area
                            && visible.get()
                            && node.size().min_element() > 0.0
                            && status.0.is_empty()
                            && world.get::<Visibility>(surface.visual) == Some(&Visibility::Visible)
                    })
                    .map(|(entity, _, _, _, _, _)| entity);
                if let Some(row) = row
                    && world
                        .query::<&Text>()
                        .iter(world)
                        .any(|text| text.0 == "Fiotes")
                {
                    if trial.saved.is_none() {
                        trial.saved = Some(
                            serde_json::from_value(
                                serde_json::to_value(world.get::<InfluenceArea>(area).unwrap())
                                    .unwrap(),
                            )
                            .unwrap(),
                        );
                    }
                    eprintln!("Fiote Castle visible: {row}");
                    let item = world.get::<lince_desktop::canvas::CanvasItem>(row).unwrap();
                    eprintln!("Fiote geometry: {:?}, {:?}", item.position, item.size);
                    if trial.restored {
                        assert_eq!(
                            item.position.to_array(),
                            trial.saved.as_ref().unwrap().center
                        );
                    }
                    let surface = world
                        .get::<lince_desktop::topology::presentation::Surface>(row)
                        .unwrap();
                    let image = world
                        .resource::<Assets<StandardMaterial>>()
                        .get(&surface.material)
                        .unwrap()
                        .base_color_texture
                        .clone()
                        .unwrap();
                    world.spawn(Readback::texture(image)).observe(
                        |event: On<ReadbackComplete>,
                         mut commands: Commands,
                         mut trial: ResMut<Trial>| {
                            let colors: HashSet<_> = event
                                .data
                                .chunks_exact(4)
                                .filter(|pixel| pixel[3] != 0)
                                .collect();
                            assert!(colors.len() > 8, "Fiote's rendered surface is empty");
                            eprintln!("Fiote controls rendered with {} colors", colors.len());
                            commands.entity(event.entity).despawn();
                            trial.stage = 5;
                            trial.ticks = 0;
                        },
                    );
                    trial.stage = 3;
                }
            }
        }
        5 if trial.ticks >= 3 => {
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(trial.path.clone()))
                .observe(|_: On<ScreenshotCaptured>, mut trial: ResMut<Trial>| {
                    trial.stage = 2;
                    trial.ticks = 0;
                });
            trial.stage = 3;
        }
        2 => {
            EditAction::Areas.apply(world, trial.root.unwrap());
            EditAction::Area(lince_desktop::area_panel::AreaAction::Select(
                trial.area.unwrap(),
            ))
            .apply(world, trial.root.unwrap());
            EditAction::Area(lince_desktop::area_panel::AreaAction::Remove)
                .apply(world, trial.root.unwrap());
            let delete = world
                .query::<(Entity, &AccessibilityNode)>()
                .iter(world)
                .find(|(_, node)| node.label() == Some("Delete"))
                .map(|(entity, _)| entity)
                .unwrap();
            world.trigger(Activate { entity: delete });
            trial.stage = 4;
            trial.ticks = 0;
        }
        4 if trial.ticks > 10 => {
            assert!(world.get_entity(trial.area.unwrap()).is_err());
            assert!(
                !world
                    .query::<&RecordBinding>()
                    .iter(world)
                    .any(|binding| binding.area == trial.area.unwrap())
            );
            eprintln!("Fiote Castle and its spawned content deleted");
            if trial.restored {
                eprintln!("Saved Fiote Castle reopened, rendered and deleted successfully");
                world.write_message(AppExit::Success);
            } else {
                let root = trial.root.unwrap();
                let workspace = world.get::<Workspaces>(root).unwrap().active;
                trial.area = Some(
                    lince_desktop::area::spawn_area(
                        world,
                        root,
                        workspace,
                        trial.saved.clone().unwrap(),
                    )
                    .unwrap(),
                );
                trial.restored = true;
                trial.stage = 1;
                trial.ticks = 0;
            }
        }
        _ => {}
    }
    if trial.stage != 1 {
        world.resource::<lince_desktop::wake::WakeSignal>().ring();
    }
    world.insert_resource(trial);
}

fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let (cell, _directory) = runtime.block_on(async {
        let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
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
                commands: Default::default(),
                speech: None,
                fiote: Some(fiote),
                information: None,
            },
            directory,
        )
    });
    let _entered = runtime.enter();
    let mut app = lince_desktop::app::connected_app(cell);
    runtime.spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(180)).await;
        eprintln!("Fiote Castle smoke timed out");
        std::process::exit(1);
    });
    app.insert_resource(Trial {
        stage: 0,
        ticks: 0,
        root: None,
        area: None,
        saved: None,
        restored: false,
        path: std::env::args().nth(1).expect("screenshot path"),
    });
    app.add_systems(
        Update,
        exercise.after(lince_desktop::topology::presentation::synchronize),
    );
    app.run();
}
