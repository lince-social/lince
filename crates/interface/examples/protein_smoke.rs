use bevy::{
    diagnostic::FrameCount,
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    ui_widgets::Activate,
    winit::WinitSettings,
};
use lince_interface::{
    icons::IconButton,
    protein_castle::{ProteinCastle, ProteinDraft, ProteinResults},
};
use serde_json::json;
use std::sync::Arc;

#[derive(Resource)]
struct Capture {
    path: String,
    phase: u8,
    frame: u32,
}

fn main() {
    let path = std::env::args().nth(1).expect("screenshot path");
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _entered = runtime.enter();
    let engine = Arc::new(runtime.block_on(async {
        let engine = engine::Engine::open_memory().await.unwrap();
        for (head, quantity) in [("Apple", 2.0), ("Apricot", 5.0), ("Pear", 3.0)] {
            engine
                .act(
                    engine::actions::Action::CreateRecord {
                        slug: None,
                        kind: nucleus::RecordKind::Plain,
                        head: head.into(),
                        body: String::new(),
                        quantity,
                    },
                    None,
                )
                .await
                .unwrap();
        }
        engine
    }));
    let cell = cell::CellRuntime {
        store: engine.store.clone(),
        engine,
        lanes: Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        information: None,
    };
    let directory = tempfile::tempdir().unwrap();
    let mut app = lince_interface::app::interface_app();
    app.insert_resource(lince_interface::app::CellHandle(cell))
        .insert_resource(lince_interface::workspace::WorkspaceFile::new(
            directory.path().join("interface.json"),
        ))
        .add_plugins(lince_interface::cell_bridge::CellBridgePlugin)
        .insert_resource(WinitSettings::continuous())
        .insert_resource(Capture {
            path,
            phase: 0,
            frame: 0,
        })
        .add_systems(Startup, setup)
        .add_systems(Update, exercise);
    app.run();
}

fn setup(world: &mut World) {
    let root = world.spawn(lince_interface::container::BoxRoot).id();
    world
        .query::<&mut Window>()
        .single_mut(world)
        .unwrap()
        .resolution
        .set(1100.0, 840.0);
    let query = serde_json::from_value(json!({"source":"record","where":[{"all":[{"text_contains":"Ap"},{"quantity_gte":"1"}]}],"order":[{"desc":"quantity"}],"fields":["head","quantity"],"limit":100})).unwrap();
    lince_interface::protein_castle::spawn(
        world,
        root,
        1,
        DVec2::ZERO,
        ProteinDraft::from_protein("Fruit".into(), "fruit".into(), query),
    );
}

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    assert!(frame < 600, "Protein preview timed out");
    if frame < 20 {
        return;
    }
    let phase = world.resource::<Capture>().phase;
    if phase == 0 {
        let button = world
            .query::<(Entity, &IconButton)>()
            .iter(world)
            .find(|(_, button)| button.label.starts_with("Run this query"))
            .unwrap()
            .0;
        world.trigger(Activate { entity: button });
        world.resource_mut::<Capture>().phase = 1;
    } else if phase == 1 {
        let result = world
            .query_filtered::<&ProteinResults, With<ProteinCastle>>()
            .single(world)
            .unwrap();
        if !result.current {
            return;
        }
        assert_eq!(result.rows.len(), 2);
        assert_eq!(result.rows[0]["head"], "Apricot");
        let path = world.resource::<Capture>().path.clone();
        world
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path))
            .observe(
                |_: On<ScreenshotCaptured>, mut state: ResMut<Capture>, frame: Res<FrameCount>| {
                    state.phase = 2;
                    state.frame = frame.0;
                },
            );
        world.resource_mut::<Capture>().phase = 4;
    } else if phase == 2 && frame > world.resource::<Capture>().frame + 10 {
        for (node, mut scroll) in world
            .query::<(&Node, &mut ScrollPosition)>()
            .iter_mut(world)
        {
            if node.width == percent(55) {
                scroll.0.y = 650.0;
            }
        }
        world.resource_mut::<Capture>().phase = 3;
        world.resource_mut::<Capture>().frame = frame;
    } else if phase == 3 && frame > world.resource::<Capture>().frame + 15 {
        let path = format!("{}.options.png", world.resource::<Capture>().path);
        world
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path))
            .observe(
                |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                    exit.write(AppExit::Success);
                },
            );
        world.resource_mut::<Capture>().phase = 4;
    }
}
