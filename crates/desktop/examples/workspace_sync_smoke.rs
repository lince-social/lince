use bevy::{
    app::AppExit,
    diagnostic::FrameCount,
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    text::EditableText,
    winit::WinitSettings,
};
use engine::{
    Engine,
    actions::Action,
    workspace_sync::{Client, Command, Request},
};
use lince_desktop::{
    actions::ActionButton,
    app::{CellHandle, interface_app},
    canvas::CanvasView,
    cell_bridge::CellBridgePlugin,
    container::BoxRoot,
    sand_store::{SandKind, spawn_sand},
};
use serde_json::json;

#[derive(Resource)]
struct Exercise {
    path: String,
    workspace: String,
    stage: u8,
    root: Option<Entity>,
    checkpoint: u32,
}

fn click(world: &mut World, title: &str) -> bool {
    let action = world
        .query::<(&ActionButton, &bevy::a11y::AccessibilityNode)>()
        .iter(world)
        .find(|(_, node)| {
            node.label()
                .is_some_and(|label| label == title || label.starts_with(title))
        })
        .map(|(button, _)| (button.target, button.actions.clone()));
    if let Some((target, actions)) = action {
        let before = world
            .query::<(Entity, &Text)>()
            .iter(world)
            .map(|(entity, text)| (entity, text.0.clone()))
            .collect::<std::collections::HashMap<_, _>>();
        actions.run(world, target);
        for (entity, text) in world.query::<(Entity, &Text)>().iter(world) {
            if before
                .get(&entity)
                .is_some_and(|previous| previous != &text.0)
            {
                println!("Workspace Sync smoke clicked {title}: {}", text.0);
            }
        }
        !has(
            world,
            "Wait for the host to acknowledge the current request.",
        )
    } else {
        false
    }
}

fn field(world: &mut World, caption: &str, value: &str) {
    let entity = world
        .query::<(Entity, &bevy::a11y::AccessibilityNode, &EditableText)>()
        .iter(world)
        .find(|(_, node, _)| node.label() == Some(caption))
        .map(|(entity, _, _)| entity)
        .unwrap_or_else(|| panic!("Missing field: {caption}"));
    let mut editable = world.get_mut::<EditableText>(entity).unwrap();
    editable.editor.set_text(value);
}

fn has(world: &mut World, needle: &str) -> bool {
    world
        .query::<&Text>()
        .iter(world)
        .any(|text| text.0.contains(needle))
}

fn responses(mut messages: MessageReader<lince_desktop::cell_bridge::CellMessage>) {
    for message in messages.read() {
        match &message.0 {
            cell::ServerMessage::ActionOk { id, data, .. } => println!(
                "Workspace Sync smoke acknowledgement {id}: {}",
                data.as_ref()
                    .and_then(|data| data["state"].as_str())
                    .unwrap_or("completed")
            ),
            cell::ServerMessage::Error { id, message, .. } => {
                println!("Workspace Sync smoke error {id}: {message}")
            }
            _ => {}
        }
    }
}

fn reveal(world: &mut World, needle: &str) -> bool {
    let target = world
        .query::<(Entity, &Text)>()
        .iter(world)
        .find(|(_, text)| text.0.contains(needle))
        .map(|(entity, _)| entity);
    let Some(target) = target else { return false };
    let Some(node) = world.get::<ComputedNode>(target) else {
        return false;
    };
    if node.size().y <= 0.0 {
        return false;
    }
    let top = world
        .get::<UiGlobalTransform>(target)
        .unwrap()
        .translation
        .y
        - node.size().y / 2.0;
    let mut viewport = target;
    while world
        .get::<Node>(viewport)
        .is_none_or(|node| node.overflow.y != OverflowAxis::Scroll)
    {
        let Some(parent) = world.get::<ChildOf>(viewport) else {
            return false;
        };
        viewport = parent.parent();
    }
    let node = world.get::<ComputedNode>(viewport).unwrap();
    let viewport_top = world
        .get::<UiGlobalTransform>(viewport)
        .unwrap()
        .translation
        .y
        - node.size().y / 2.0;
    let delta = (top - viewport_top) * node.inverse_scale_factor() - 24.0;
    println!(
        "Workspace Sync smoke review viewport: top={top}, viewport_top={viewport_top}, size={:?}, content={:?}, overflow={:?}, delta={delta}",
        node.size(),
        node.content_size,
        world.get::<Node>(viewport).unwrap().overflow
    );
    let mut scroll = world.get_mut::<ScrollPosition>(viewport).unwrap();
    scroll.0.y = (scroll.0.y + delta).max(0.0);
    true
}

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    let stage = world.resource::<Exercise>().stage;
    let mut advanced = false;
    match stage {
        0 if frame >= 10 => {
            let root = world
                .query_filtered::<Entity, With<BoxRoot>>()
                .single(world)
                .unwrap();
            spawn_sand(world, root, 1, SandKind::Sync, "", DVec2::ZERO);
            world.resource_mut::<Exercise>().root = Some(root);
            advanced = click(world, "Connect / reload workspaces");
        }
        1 if frame > world.resource::<Exercise>().checkpoint + 5 => {
            advanced = click(world, "Rendered joint");
        }
        2 if has(world, "Shared editor · revision 1") => {
            let root = world.resource::<Exercise>().root.unwrap();
            world.get_mut::<CanvasView>(root).unwrap().center = DVec2::new(35.0, 47.0);
            field(
                world,
                "Workspace name, Text content or Record UID",
                "Rendered shared text",
            );
            advanced = click(world, "Add Text");
        }
        3 if frame > world.resource::<Exercise>().checkpoint + 10
            && has(world, "Rendered shared text")
            && (has(world, "revision 2") || has(world, "Host saved: applied")) =>
        {
            field(
                world,
                "Workspace operation / offline draft",
                "{\"operation\":\"rename\",\"name\":\"Recovered draft\"}",
            );
            advanced = click(world, "Save draft on this computer");
        }
        4 if has(world, "Draft saved on this computer") => {
            advanced = click(world, "Recover local drafts");
        }
        5 => {
            let workspace = world.resource::<Exercise>().workspace.clone();
            advanced = click(world, &workspace);
        }
        6 if has(world, "Draft loaded") || frame > world.resource::<Exercise>().checkpoint + 15 => {
            field(world, "Workspace policy", &json!({"required_capabilities":[],"ceiling":{"read":{"all":[]},"grants":[{"operation":"update","selector":{"kind_eq":"plain"},"properties":["quantity"],"assertions_add":[],"assertions_remove":[]}]}}).to_string());
            advanced = click(world, "Propose policy change");
        }
        7 if has(world, "Host saved: pending") => {
            advanced = click(world, "Review changes and history");
        }
        8 => {
            advanced = click(world, "Preview as original Actor");
        }
        9 if has(world, "review required") => {
            assert!(has(world, "current revision"));
            advanced = click(world, "Review changes and history");
        }
        10 => {
            advanced = click(world, "Approve against current revision");
        }
        11 if frame > world.resource::<Exercise>().checkpoint + 10
            && (has(world, "revision 3") || has(world, "Host saved: applied")) =>
        {
            let root = world.resource::<Exercise>().root.unwrap();
            assert_eq!(
                world.get::<CanvasView>(root).unwrap().center,
                DVec2::new(35.0, 47.0)
            );
            for title in [
                "Propose head",
                "Propose body",
                "Propose restoring Record",
                "Validate local workspace for publishing",
                "Toggle extension field authority",
                "Allow adding identity",
            ] {
                assert!(
                    world
                        .query::<(&ActionButton, &bevy::a11y::AccessibilityNode)>()
                        .iter(world)
                        .any(|(_, node)| node.label() == Some(title)),
                    "Missing native control: {title}"
                );
            }
            advanced = click(world, "Review changes and history");
        }
        12 if has(world, "Change workspace admission") => {
            assert!(has(world, "· text"));
            assert!(
                world
                    .query::<(&ActionButton, &bevy::a11y::AccessibilityNode)>()
                    .iter(world)
                    .any(|(_, node)| node.label() == Some("Revise and resubmit as my change"))
            );
            advanced = reveal(world, "Change workspace admission");
            if advanced {
                let root = world.resource::<Exercise>().root.unwrap();
                let sand = world
                    .query::<(Entity, &lince_desktop::sand_store::StoredSand, &ChildOf)>()
                    .iter(world)
                    .find(|(_, sand, parent)| {
                        sand.kind == SandKind::Sync && parent.parent() == root
                    })
                    .map(|(entity, _, _)| entity)
                    .unwrap();
                world
                    .get_mut::<lince_desktop::canvas::CanvasItem>(sand)
                    .unwrap()
                    .position = DVec2::new(-450.0, 0.0);
            }
        }
        13 if frame > world.resource::<Exercise>().checkpoint + 12 => {
            let path = world.resource::<Exercise>().path.clone();
            world.spawn(Screenshot::primary_window()).observe(save_to_disk(path)).observe(|_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| { println!("Workspace Sync rendered smoke passed: admission, shared edits, durable drafts, policy review, readable preview/history and personal camera."); exit.write(AppExit::Success); });
            advanced = true;
        }
        _ => {}
    }
    if advanced {
        let mut exercise = world.resource_mut::<Exercise>();
        exercise.stage += 1;
        exercise.checkpoint = frame;
        println!(
            "Workspace Sync smoke stage {} at frame {frame}",
            exercise.stage
        );
    }
    if frame % 120 == 0 {
        println!("Workspace Sync smoke waiting at stage {stage}, frame {frame}");
    }
    assert!(
        frame < world.resource::<Exercise>().checkpoint + 600,
        "Workspace Sync smoke stalled at stage {stage}"
    );
}

#[tokio::main]
async fn main() {
    let engine = std::sync::Arc::new(Engine::open_memory().await.unwrap());
    let workspace = engine.act(Action::Workspace { request: Request { client: Client::default(), command: Command::Create { name: "Rendered joint".into(), policy: json!({"required_capabilities":[],"ceiling":{"read":{"all":[]},"grants":[]}}) } } }, None).await.unwrap().created.unwrap();
    let runtime = cell::CellRuntime {
        commands: Default::default(),
        store: engine.store.clone(),
        engine,
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        fiote: None,
        speech: None,
        information: None,
    };
    interface_app()
        .insert_resource(CellHandle(runtime))
        .insert_resource(Exercise {
            path: std::env::args().nth(1).expect("provide screenshot path"),
            workspace,
            stage: 0,
            root: None,
            checkpoint: 0,
        })
        .insert_resource(WinitSettings::continuous())
        .add_plugins(CellBridgePlugin)
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(BoxRoot);
        })
        .add_systems(Update, exercise)
        .add_systems(Update, responses)
        .run();
}
