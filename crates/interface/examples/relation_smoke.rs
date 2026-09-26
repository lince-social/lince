use bevy::{
    diagnostic::FrameCount,
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    winit::WinitSettings,
};
use lince_interface::{
    canvas::{CanvasItem, CanvasView},
    container::BoxRoot,
    protein_area::RecordBinding,
};
use std::sync::Arc;

#[derive(Resource)]
struct Trial {
    ids: Vec<String>,
    owner: Option<Entity>,
    ready: Option<u32>,
    expanded: Option<Entity>,
    captures: usize,
}

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    assert!(frame < 1800, "Relation Castle did not finish loading");
    let root = world
        .query_filtered::<Entity, With<BoxRoot>>()
        .single(world)
        .unwrap();
    if frame == 20 {
        let owner = lince_interface::relation_castle::spawn(world, root).unwrap();
        let ids = world.resource::<Trial>().ids.clone();
        world
            .get_mut::<lince_interface::area::InfluenceArea>(owner)
            .unwrap()
            .protein
            .as_mut()
            .unwrap()
            .draft
            .query["where"] = serde_json::json!([{"any":ids.iter().map(|uid| serde_json::json!({"uid_eq":uid})).collect::<Vec<_>>()}]);
        world.resource_mut::<Trial>().owner = Some(owner);
        world.get_mut::<CanvasView>(root).unwrap().zoom = 0.55;
        return;
    }
    let Some(owner) = world.resource::<Trial>().owner else {
        return;
    };
    let records: Vec<_> = world
        .query::<(Entity, &RecordBinding, &CanvasItem)>()
        .iter(world)
        .filter(|(_, binding, _)| binding.area == owner)
        .map(|(entity, _, item)| (entity, *item))
        .collect();
    if records.len() != 6
        || world
            .query::<&lince_interface::arrow_sand::ArrowSand>()
            .iter(world)
            .count()
            != 5
    {
        return;
    }
    let ready = *world.resource_mut::<Trial>().ready.get_or_insert(frame);
    if frame == ready + 240 {
        assert!(records.iter().all(|(entity, item)| {
            world
                .get::<lince_interface::record_presentation::RecordPresentation>(*entity)
                .is_some_and(|state| state.hide_filled && !state.expanded)
                && item.size.y < 200.0
        }));
        world
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk("/tmp/lince-relation-collapsed.png"));
    }
    if frame == ready + 250 {
        let row = records[0].0;
        let button = world
            .query::<(
                &lince_interface::actions::ActionButton,
                &lince_interface::icons::Tooltip,
            )>()
            .iter(world)
            .find(|(button, tooltip)| {
                button.target == row && tooltip.0 == "Show or hide properties"
            })
            .map(|(button, _)| button.clone())
            .unwrap();
        button.actions.run(world, row);
        world.resource_mut::<Trial>().expanded = Some(row);
    }
    if frame == ready + 310 {
        let row = world.resource::<Trial>().expanded.unwrap();
        assert!(world.get::<CanvasItem>(row).unwrap().size.y > 200.0);
        assert!(
            world
                .get::<lince_interface::record_presentation::RecordPresentation>(row)
                .unwrap()
                .expanded
        );
        world
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk("/tmp/lince-relation-expanded.png"))
            .observe(|_: On<ScreenshotCaptured>, mut trial: ResMut<Trial>| {
                trial.captures += 1;
            });
    }
    if frame > ready + 315 && world.resource::<Trial>().captures > 0 {
        world.write_message(AppExit::Success);
    }
}

#[tokio::main]
async fn main() {
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    engine
        .act(
            engine::actions::Action::CreateConcept {
                lingua: "g_local".into(),
                name: "depends-on".into(),
                parents: vec![],
            },
            None,
        )
        .await
        .unwrap();
    let mut ids = Vec::new();
    for head in [
        "Plan the garden",
        "Prepare the soil",
        "Choose seeds",
        "Plant seedlings",
        "Water the garden",
        "Enjoy the harvest",
    ] {
        ids.push(
            engine
                .act(
                    engine::actions::Action::CreateRecord {
                        slug: None,
                        kind: nucleus::RecordKind::Plain,
                        head: head.into(),
                        body: "A filled description that starts inside the accordion.".into(),
                        quantity: 3.0,
                    },
                    None,
                )
                .await
                .unwrap()
                .created
                .unwrap(),
        );
    }
    for index in 1..ids.len() {
        engine
            .act(
                engine::actions::Action::AssertRecord {
                    subject: ids[index - 1].clone(),
                    predicate: "depends-on".into(),
                    object: Some(ids[index].clone()),
                    quantity: None,
                    unit: None,
                },
                None,
            )
            .await
            .unwrap();
    }
    lince_interface::app::interface_app()
        .add_plugins(bevy::log::LogPlugin::default())
        .add_plugins(lince_interface::cell_bridge::CellBridgePlugin)
        .insert_resource(lince_interface::wake::WakeSignal::new(|| {}))
        .insert_resource(lince_interface::app::CellHandle(cell::CellRuntime {
            commands: Default::default(),
            engine: engine.clone(),
            store: engine.store.clone(),
            lanes: Arc::new(cell::LaneHub::new()),
            wire: Default::default(),
            fiote: None,
            information: None,
        }))
        .insert_resource(Trial {
            ids,
            owner: None,
            ready: None,
            expanded: None,
            captures: 0,
        })
        .insert_resource(WinitSettings::continuous())
        .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_secs_f64(1.0 / 30.0),
        ))
        .add_systems(
            Startup,
            |mut commands: Commands, mut windows: Query<&mut Window>| {
                windows.single_mut().unwrap().resolution.set(1600.0, 1000.0);
                commands.spawn((
                    BoxRoot,
                    CanvasView {
                        center: DVec2::ZERO,
                        ..default()
                    },
                ));
            },
        )
        .add_systems(
            Update,
            exercise.after(lince_interface::physics::SimulateWorkspaces),
        )
        .run();
}
