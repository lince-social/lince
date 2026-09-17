use bevy::{
    app::AppExit,
    diagnostic::FrameCount,
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    winit::WinitSettings,
};
use lince_interface::{
    access_control::AccessControlSand,
    actions::ActionButton,
    app::{CellHandle, interface_app},
    cell_bridge::CellBridgePlugin,
    container::BoxRoot,
    sand_store::{SandKind, spawn_sand},
};

#[derive(Resource)]
struct Capture {
    path: String,
    stage: u8,
}

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    if frame == 10 {
        let root = world
            .query_filtered::<Entity, With<BoxRoot>>()
            .single(world)
            .unwrap();
        spawn_sand(world, root, 1, SandKind::AccessControl, "", DVec2::ZERO);
    }
    if frame > 30 && world.resource::<Capture>().stage == 0 {
        let ready = world
            .query::<&Text>()
            .iter(world)
            .any(|text| text.0.starts_with("Alice ·"));
        if ready {
            let button = world
                .query::<(&ActionButton, &bevy::a11y::AccessibilityNode)>()
                .iter(world)
                .find(|(_, node)| node.label() == Some("Roles"))
                .map(|(button, _)| (button.target, button.actions.clone()))
                .unwrap();
            button.1.run(world, button.0);
            world.resource_mut::<Capture>().stage = 1;
        }
    }
    if frame > 35 && world.resource::<Capture>().stage == 1 {
        let button = world
            .query::<(&ActionButton, &bevy::a11y::AccessibilityNode)>()
            .iter(world)
            .find(|(_, node)| node.label() == Some("editor"))
            .map(|(button, _)| (button.target, button.actions.clone()));
        if let Some((target, actions)) = button {
            actions.run(world, target);
            world.resource_mut::<Capture>().stage = 2;
        }
    }
    if frame > 60 && world.resource::<Capture>().stage == 2 {
        let sand = world
            .query_filtered::<Entity, With<AccessControlSand>>()
            .single(world)
            .unwrap();
        let bounds = world.get::<ComputedNode>(sand).unwrap().size();
        assert!(bounds.x > 400.0 && bounds.y > 400.0);
        assert!(
            world
                .query::<&Text>()
                .iter(world)
                .any(|text| text.0 == "On record:read")
        );
        for title in ["Rename Role", "Delete Role…"] {
            assert!(
                world
                    .query::<(&ActionButton, &bevy::a11y::AccessibilityNode)>()
                    .iter(world)
                    .any(|(_, node)| node.label() == Some(title))
            );
        }
        let path = world.resource::<Capture>().path.clone();
        world.spawn(Screenshot::primary_window()).observe(save_to_disk(path)).observe(
            |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                println!("Access Control smoke passed: live Auth query, Role selection, rename, delete and permission controls.");
                exit.write(AppExit::Success);
            },
        );
        world.resource_mut::<Capture>().stage = 3;
    }
    assert!(frame < 1200, "Access Control smoke timed out");
}

#[tokio::main]
async fn main() {
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    for name in ["editor", "reader"] {
        engine
            .act(
                engine::actions::Action::CreateRole { name: name.into() },
                None,
            )
            .await
            .unwrap();
    }
    for permission in ["record:read", "record:update"] {
        engine
            .act(
                engine::actions::Action::GrantPermission {
                    role: "editor".into(),
                    permission: permission.into(),
                },
                None,
            )
            .await
            .unwrap();
    }
    engine
        .act(
            engine::actions::Action::CreateUser {
                username: "alice".into(),
                name: "Alice".into(),
                password: "temporary-smoke-password".into(),
                role: "editor".into(),
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
    };
    interface_app()
        .insert_resource(CellHandle(runtime))
        .insert_resource(Capture {
            path: std::env::args().nth(1).expect("provide screenshot path"),
            stage: 0,
        })
        .insert_resource(WinitSettings::continuous())
        .add_plugins(CellBridgePlugin)
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(BoxRoot);
        })
        .add_systems(Update, exercise)
        .run();
}
