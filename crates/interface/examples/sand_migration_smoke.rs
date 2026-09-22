use bevy::{
    app::AppExit,
    diagnostic::FrameCount,
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    winit::WinitSettings,
};
use lince_interface::{
    actions::ActionButton,
    app::{CellHandle, interface_app},
    canvas::{CanvasItem, CanvasView},
    cell_bridge::CellBridgePlugin,
    container::BoxRoot,
    sand_store::{SandKind, spawn_sand},
};

#[derive(Resource)]
struct Capture {
    path: String,
    owners: Vec<Entity>,
    started: bool,
    captured: bool,
}

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    if frame == 10 {
        let root = world
            .query_filtered::<Entity, With<BoxRoot>>()
            .single(world)
            .unwrap();
        let previous: Vec<_> = world
            .query_filtered::<Entity, With<CanvasItem>>()
            .iter(world)
            .collect();
        for entity in previous {
            world.despawn(entity);
        }
        world.get_mut::<CanvasView>(root).unwrap().zoom = 0.75;
        let mut owners = Vec::new();
        for (index, kind) in [
            SandKind::Freedoom,
            SandKind::Terminal,
            SandKind::Configuration,
            SandKind::Todo,
        ]
        .into_iter()
        .enumerate()
        {
            let position = DVec2::new(
                if index % 2 == 0 { -330.0 } else { 330.0 },
                if index < 2 { -295.0 } else { 295.0 },
            );
            let owner = spawn_sand(world, root, 1, kind, "", position);
            world.get_mut::<CanvasItem>(owner).unwrap().size = Vec2::new(620.0, 560.0);
            owners.push(owner);
        }
        world.resource_mut::<Capture>().owners = owners;
    }
    if frame >= 30 && !world.resource::<Capture>().started {
        let owners = world.resource::<Capture>().owners.clone();
        let actions: Vec<_> = world
            .query::<(&ActionButton, &bevy::a11y::AccessibilityNode)>()
            .iter(world)
            .filter(|(button, node)| {
                owners.contains(&button.target)
                    && matches!(node.label(), Some("Start / restart" | "Open shell"))
            })
            .map(|(button, _)| (button.target, button.actions.clone()))
            .collect();
        assert_eq!(actions.len(), 2);
        for (owner, action) in actions {
            action.run(world, owner);
        }
        world.resource_mut::<Capture>().started = true;
    }
    if frame >= 150 && !world.resource::<Capture>().captured {
        for owner in &world.resource::<Capture>().owners {
            let size = world.get::<ComputedNode>(*owner).unwrap().size();
            assert!(size.x > 400.0 && size.y > 350.0);
        }
        let path = world.resource::<Capture>().path.clone();
        world
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path))
            .observe(
                |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                    exit.write(AppExit::Success);
                },
            );
        world.resource_mut::<Capture>().captured = true;
    }
    assert!(frame < 600, "Sand migration screenshot timed out");
}

#[tokio::main]
async fn main() {
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "Try the native Sands".into(),
                body: "A task from the local focus queue.".into(),
                quantity: -1.0,
            },
            None,
        )
        .await
        .unwrap();
    let runtime = cell::CellRuntime {
        store: engine.store.clone(),
        engine,
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        information: None,
        fiote: None,
    };
    let mut app = interface_app();
    app.insert_resource(CellHandle(runtime))
        .insert_resource(Capture {
            path: std::env::args().nth(1).expect("provide screenshot path"),
            owners: Vec::new(),
            started: false,
            captured: false,
        })
        .insert_resource(WinitSettings::continuous())
        .add_plugins(CellBridgePlugin)
        .add_systems(
            Startup,
            |mut commands: Commands, mut windows: Query<&mut Window>| {
                commands.spawn(BoxRoot);
                for mut window in &mut windows {
                    window.resolution.set(1400.0, 1000.0);
                }
            },
        )
        .add_systems(Update, exercise)
        .run();
}
