//! Run with a fresh output directory to exercise create/edit/save and reopen in separate native processes.
#[path = "workspace_scene/bridge.rs"]
mod bridge;
#[path = "workspace_scene/pointer.rs"]
mod pointer;
use bevy::{
    a11y::AccessibilityNode,
    diagnostic::FrameCount,
    input_focus::{FocusCause, InputFocus},
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    text::EditableText,
    ui_widgets::Activate,
    winit::WinitSettings,
};
use lince_sandbox::{
    actions::{Action, ActionButton},
    container::BoxRoot,
    edit_mode::{EditAction, EditControl, EditMode},
    workspace::WorkspaceFile,
};
use nucleus::canvas::scene::Handle;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Resource)]
struct Replay {
    directory: PathBuf,
    reopen: bool,
    bridge: bool,
    showcase: bool,
    complete: Arc<AtomicBool>,
}

#[derive(Serialize, Deserialize)]
struct Expected {
    first: Handle,
    second: Handle,
    part: String,
    record: String,
}

fn control(world: &mut World, caption: &str) -> Entity {
    world
        .query_filtered::<(Entity, &AccessibilityNode), With<ActionButton>>()
        .iter(world)
        .find(|(_, n)| n.label() == Some(caption))
        .unwrap_or_else(|| panic!("Missing native action {caption}"))
        .0
}

fn click(world: &mut World, caption: &str) {
    let entity = control(world, caption);
    world.trigger(Activate { entity });
}

fn input(world: &mut World, caption: &str) -> Entity {
    world
        .query_filtered::<(Entity, &AccessibilityNode), With<EditableText>>()
        .iter(world)
        .find(|(_, n)| n.label() == Some(caption))
        .unwrap_or_else(|| panic!("Missing input {caption}"))
        .0
}

fn fill(world: &mut World, caption: &str, text: &str) {
    let e = input(world, caption);
    world
        .get_mut::<EditableText>(e)
        .unwrap()
        .editor
        .set_text(text);
}

fn inspect(world: &mut World, caption: &str) -> String {
    let e = input(world, caption);
    world.get::<EditableText>(e).unwrap().value().to_string()
}

fn selected(world: &mut World) -> String {
    world
        .query_filtered::<&AccessibilityNode, With<ActionButton>>()
        .iter(world)
        .filter_map(|n| {
            n.label()?
                .strip_prefix("Selected slot_")
                .map(|s| format!("slot_{s}"))
        })
        .next()
        .expect("selected slot in the scene tree")
}

fn screenshot(world: &mut World, path: PathBuf) {
    world
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path))
        .observe(
            |_: On<ScreenshotCaptured>, replay: Res<Replay>, mut exit: MessageWriter<AppExit>| {
                replay.complete.store(true, Ordering::Release);
                exit.write(AppExit::Success);
            },
        );
}

fn create(world: &mut World, frame: u32, directory: &Path) {
    let file = directory.join("expected.json");
    match frame {
        24 => click(world, "New scene"),
        28 => {
            fill(world, "New part name", "Screw");
            click(world, "Create part and place it");
        }
        32 => click(world, "Place another copy of this part"),
        36 => click(world, "Save scene"),
        40 => {
            let saved =
                store::workspace_scene::open(&directory.join("workspaces/1/scene.workspace"))
                    .unwrap()
                    .scene;
            assert_eq!(saved.parts.len(), 1);
            assert_eq!(saved.slots.len(), 2);
            assert_eq!(saved.parts[0].name, "Screw");
            assert_eq!(saved.slots[0].part, saved.slots[1].part);
            let first = saved.handle(&saved.slots[0]);
            let second = saved.handle(&saved.slots[1]);
            assert_ne!(first, second);
            assert_eq!(selected(world), second.slot.as_str());
            control(world, &format!("Placement {}", first.slot.as_str()));
            let expected = Expected {
                first,
                second,
                part: saved.parts[0].id.as_str().into(),
                record: nucleus::id::new_uid("r"),
            };
            fill(world, "X (m)", "0.125");
            fill(world, "Y (m)", "-0.002");
            fill(world, "Record UID (optional)", &expected.record);
            fs::write(file, serde_json::to_vec_pretty(&expected).unwrap()).unwrap();
            click(world, "Apply to selected placement");
        }
        44 => {
            assert_eq!(inspect(world, "X (m)"), "0.125");
            click(world, "Undo scene edit");
        }
        48 => {
            assert_eq!(inspect(world, "X (m)"), "0");
            click(world, "Redo scene edit");
        }
        52 => {
            assert_eq!(inspect(world, "Y (m)"), "-0.002");
            click(world, "Save scene");
        }
        56 => {
            assert!(
                world
                    .query::<&Text>()
                    .iter(world)
                    .any(|t| t.0 == "Saved the scene.")
            );
            world
                .resource::<Replay>()
                .complete
                .store(true, Ordering::Release);
            world.write_message(AppExit::Success);
        }
        _ => {}
    }
}

fn bounds(world: &World, entity: Entity) -> Rect {
    let node = world.get::<ComputedNode>(entity).unwrap();
    Rect::from_center_size(
        world.get::<UiGlobalTransform>(entity).unwrap().translation,
        node.size(),
    )
}

fn verify_layout(world: &mut World, directory: &Path) {
    let (root, mode) = world.query::<(Entity, &EditMode)>().single(world).unwrap();
    let (panel, toggle) = (bounds(world, mode.panel), bounds(world, mode.toggle));
    let viewport = bounds(world, root);
    let apply = control(world, "Apply to selected placement");
    let visible = bounds(world, apply);
    assert!(
        (panel.min.x - viewport.min.x).abs() <= 1. && (panel.min.y - viewport.min.y).abs() <= 1.,
        "panel {panel:?}, viewport {viewport:?}"
    );
    assert!(panel.max.y <= toggle.min.y + 1. && panel.max.x <= viewport.max.x + 1.);
    assert!(
        visible.min.x >= panel.min.x - 1.
            && visible.max.x <= panel.max.x + 1.
            && visible.min.y >= panel.min.y - 1.
            && visible.max.y <= panel.max.y + 1.,
        "focused control {visible:?}, panel {panel:?}"
    );
    let corners = |r: Rect| [r.min.to_array(), r.max.to_array()];
    fs::write(directory.join("layout.json"),serde_json::to_vec_pretty(&serde_json::json!({
        "viewport":corners(viewport),"edit_panel":corners(panel),"edit_toggle":corners(toggle),"focused_apply":corners(visible)
    })).unwrap()).unwrap();
}

fn reopen(world: &mut World, frame: u32, directory: &Path) {
    if ![24, 28, 32, 36, 42, 50, 58].contains(&frame) {
        return;
    }
    let expected: Expected =
        serde_json::from_slice(&fs::read(directory.join("expected.json")).unwrap()).unwrap();
    match frame {
        24 => click(world, "Open directory"),
        28 => {
            assert_eq!(selected(world), expected.first.slot.as_str());
            assert_eq!(inspect(world, "X (m)"), "0");
            assert_eq!(inspect(world, "Y (m)"), "0");
            assert_eq!(inspect(world, "Record UID (optional)"), "");
            click(
                world,
                &format!("Placement {}", expected.second.slot.as_str()),
            );
        }
        32 => {
            assert_eq!(selected(world), expected.second.slot.as_str());
            assert_eq!(inspect(world, "X (m)"), "0.125");
            assert_eq!(inspect(world, "Y (m)"), "-0.002");
            assert_eq!(inspect(world, "Record UID (optional)"), expected.record);
            let saved =
                store::workspace_scene::open(&directory.join("workspaces/1/scene.workspace"))
                    .unwrap();
            assert_eq!(saved.scene.parts.len(), 1);
            assert_eq!(saved.scene.slots.len(), 2);
            for handle in [&expected.first, &expected.second] {
                assert_eq!(
                    saved.scene.slot(handle).unwrap().part.as_str(),
                    expected.part
                );
            }
            assert_eq!(saved.scene.slots[0].revision, saved.scene.slots[1].revision);
            for label in [expected.part.as_str(), expected.second.root.as_str()] {
                assert!(
                    world
                        .query::<&Text>()
                        .iter(world)
                        .any(|text| text.0.contains(label))
                );
            }
            fs::write(directory.join("reopened.json"),serde_json::to_vec_pretty(&serde_json::json!({
                "head":saved.revision, "document":saved.scene.document, "definitions":saved.scene.parts, "placements":saved.scene.slots,
                "root":saved.scene.root,"selected_in_inspector":selected(world)
            })).unwrap()).unwrap();
        }
        36 => {
            let entity = control(
                world,
                &format!("Placement {}", expected.first.slot.as_str()),
            );
            world
                .resource_mut::<InputFocus>()
                .set(entity, FocusCause::Navigated);
        }
        42 => {
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(directory.join("tree.png")));
        }
        50 => {
            let entity = control(world, "Apply to selected placement");
            world
                .resource_mut::<InputFocus>()
                .set(entity, FocusCause::Navigated);
        }
        58 => {
            verify_layout(world, directory);
            screenshot(world, directory.join("inspector.png"));
        }
        _ => {}
    }
}

fn edit_action(world: &mut World, action: EditAction) {
    let entity = world
        .query::<(Entity, &EditControl)>()
        .iter(world)
        .find(|(_, c)| c.action == action)
        .unwrap()
        .0;
    world.trigger(Activate { entity });
}

fn replay(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    let state = world.resource::<Replay>();
    let directory = state.directory.clone();
    let opening = state.reopen;
    let showcase = state.showcase;
    if showcase && frame < 14 {
        match frame {
            4 => {
                let root = world
                    .query_filtered::<Entity, With<BoxRoot>>()
                    .single(world)
                    .unwrap();
                lince_sandbox::laboratory::LaboratoryAction::Open.apply(world, root);
            }
            9 => click(world, "showoff workspace"),
            _ => {}
        }
        return;
    }
    let frame = if showcase { frame - 14 } else { frame };
    if showcase && !opening && frame >= 21 {
        match frame {
            22 => click(world, "Try the bracket example"),
            25 => {
                assert_eq!(inspect(world, "X (m)"), "5");
                fill(
                    world,
                    "Scene directory",
                    directory.join("sample.workspace").to_str().unwrap(),
                );
                click(world, "Save to a new directory");
            }
            29 => {
                let scene = store::workspace_scene::open(&directory.join("sample.workspace"))
                    .unwrap()
                    .scene;
                assert_eq!(scene.slots.len(), 2);
                assert!(scene.canvas.is_some());
                let root = world
                    .query_filtered::<Entity, With<BoxRoot>>()
                    .single(world)
                    .unwrap();
                let active = world
                    .get::<lince_sandbox::workspace::Workspaces>(root)
                    .unwrap()
                    .active;
                fill(
                    world,
                    "Scene directory",
                    directory
                        .join(format!("workspaces/{active}/scene.workspace"))
                        .to_str()
                        .unwrap(),
                );
            }
            _ => {}
        }
        if frame < 36 {
            return;
        }
    }
    let frame = if showcase && !opening && frame >= 36 {
        frame - 12
    } else {
        frame
    };
    match frame {
        10 | 16 => {
            let action = if frame == 10 {
                EditAction::Toggle
            } else {
                EditAction::Workspaces
            };
            edit_action(world, action);
        }
        20 => click(world, "Scene document"),
        _ if showcase || world.resource::<Replay>().bridge => {
            bridge::replay(world, frame, &directory, opening)
        }
        _ if opening => reopen(world, frame, &directory),
        _ => create(world, frame, &directory),
    }
}

fn main() -> AppExit {
    let directory = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .expect("new output directory, or previous output with --reopen"),
    );
    let phase = std::env::args().nth(2);
    let bridge = phase.as_ref().is_some_and(|s| s.starts_with("--bridge"));
    let showcase = phase.as_ref().is_some_and(|s| s.starts_with("--showcase"));
    let reopen = match phase.as_deref() {
        None | Some("--bridge" | "--showcase") => {
            for phase in if showcase {
                ["--showcase-create", "--showcase-reopen"]
            } else if bridge {
                ["--bridge-create", "--bridge-reopen"]
            } else {
                ["--create", "--reopen"]
            } {
                let result = std::process::Command::new(std::env::current_exe().unwrap())
                    .arg(&directory)
                    .arg(phase)
                    .status()
                    .expect("start native scene replay");
                if !result.success() {
                    std::process::exit(result.code().unwrap_or(1));
                }
            }
            println!(
                "Scene replay passed in both native processes. Evidence is in {}.",
                directory.display()
            );
            return AppExit::Success;
        }
        Some("--reopen" | "--bridge-reopen" | "--showcase-reopen") => true,
        Some("--create" | "--bridge-create" | "--showcase-create") => false,
        Some(arg) => panic!("Unknown scene replay phase {arg}"),
    };
    if !reopen {
        fs::create_dir(&directory).expect("use a fresh output directory");
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(120));
        eprintln!("Scene replay timed out");
        std::process::exit(1);
    });
    let complete = Arc::new(AtomicBool::new(false));
    let mut app = lince_sandbox::app::interface_app();
    app.add_plugins(bevy::log::LogPlugin::default())
        .insert_resource(WorkspaceFile::new(directory.join("interface.json")))
        .insert_resource(Replay {
            directory,
            reopen,
            bridge,
            showcase,
            complete: complete.clone(),
        })
        .insert_resource(WinitSettings::continuous())
        .add_systems(Startup, move |world: &mut World| {
            let (width, height) = if std::env::var_os("LINCE_SCENE_REPLAY_NARROW").is_some() {
                (360.0, 700.0)
            } else if bridge || showcase {
                (1680.0, 960.0)
            } else {
                (1000.0, 900.0)
            };
            world
                .query::<&mut Window>()
                .single_mut(world)
                .unwrap()
                .resolution
                .set(width, height);
            world.spawn(BoxRoot);
        })
        .add_systems(Update, replay);
    let exit = app.run();
    if exit.is_error() {
        return exit;
    }
    assert!(
        complete.load(Ordering::Acquire),
        "Native scene replay exited before completing its checks"
    );
    AppExit::Success
}
