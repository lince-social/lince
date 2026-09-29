use bevy::{
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
    },
    input_focus::{FocusCause, InputFocus},
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    text::{EditableText, TextEdit},
    window::{Ime, PrimaryWindow},
    winit::WinitSettings,
};
use lince_desktop::{
    actions::{Action, ActionButton},
    container::BoxRoot,
    file_explorer::{BrowseFor, FileExplorer},
    ide::Ide,
    workspace::{WorkspaceFile, Workspaces},
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Resource)]
struct Trial {
    directory: PathBuf,
    stage: usize,
    frames: usize,
    began: Instant,
    owner: Option<Entity>,
    input: Option<Entity>,
    mode: String,
}

fn descendant(world: &World, mut entity: Entity, owner: Entity) -> bool {
    while let Some(parent) = world.get::<ChildOf>(entity) {
        entity = parent.parent();
        if entity == owner {
            return true;
        }
    }
    false
}

fn editor(world: &mut World, owner: Entity) -> Option<Entity> {
    world
        .query::<(Entity, &EditableText)>()
        .iter(world)
        .find(|(entity, text)| {
            text.max_characters == Some(lince_editor::MAX_WINDOW_BYTES * 2)
                && descendant(world, *entity, owner)
        })
        .map(|(entity, _)| entity)
}

fn assert_glyphs(world: &World, editor: Entity) {
    let input = world.get::<EditableText>(editor).unwrap();
    let layout = input.editor.try_layout().expect("Text must be shaped");
    let mut count = 0;
    for line in layout.lines() {
        for run in line.runs() {
            for cluster in run.clusters() {
                for glyph in cluster.glyphs() {
                    assert_ne!(glyph.id, 0, "Missing glyph in the rendered editor");
                    count += 1;
                }
            }
        }
    }
    assert!(count > 0);
}

fn button(world: &mut World, owner: Entity, caption: &str) -> Entity {
    world
        .query::<(Entity, &lince_desktop::icons::Tooltip, &ActionButton)>()
        .iter(world)
        .find(|(_, tip, button)| button.target == owner && tip.0 == caption)
        .unwrap_or_else(|| panic!("Missing button {caption}"))
        .0
}

fn click(world: &mut World, owner: Entity, caption: &str) {
    let entity = button(world, owner, caption);
    world.trigger(bevy::ui_widgets::Activate { entity });
}

fn has_button(world: &mut World, owner: Entity, caption: &str) -> bool {
    world
        .query::<(&lince_desktop::icons::Tooltip, &ActionButton)>()
        .iter(world)
        .any(|(tip, button)| button.target == owner && tip.0 == caption)
}

fn explorer(world: &mut World, owner: Entity) -> Entity {
    world
        .query_filtered::<Entity, With<FileExplorer>>()
        .iter(world)
        .find(|entity| descendant(world, *entity, owner))
        .unwrap()
}

fn destination(world: &mut World, owner: Entity, value: &str) {
    field(world, owner, "New name or full destination path", value);
}

fn field(world: &mut World, owner: Entity, caption: &str, value: &str) {
    let input = world
        .query::<(Entity, &lince_desktop::icons::Tooltip, &EditableText)>()
        .iter(world)
        .find(|(entity, tip, _)| tip.0 == caption && descendant(world, *entity, owner))
        .unwrap()
        .0;
    world
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text(value);
}

fn advance(world: &mut World) {
    let mut trial = world.resource_mut::<Trial>();
    trial.stage += 1;
    trial.frames = 0;
}

fn capture(world: &mut World, name: &str, exit: bool) {
    let path = world.resource::<Trial>().directory.join(name);
    let mut request = world.spawn(Screenshot::primary_window());
    request.observe(save_to_disk(path));
    if exit {
        request.observe(
            |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                exit.write(AppExit::Success);
            },
        );
    }
}

fn exercise(world: &mut World) {
    assert!(
        world.resource::<Trial>().began.elapsed() < Duration::from_secs(180),
        "Editor smoke timed out at stage {}",
        world.resource::<Trial>().stage
    );
    world.resource_mut::<Trial>().frames += 1;
    if world.resource::<Trial>().frames == 6 {
        match world.resource::<Trial>().stage {
            7 => capture(world, "cjk-preedit.png", false),
            80 => capture(world, "cjk-selection.png", false),
            11 => capture(world, "scrolled.png", false),
            12 => capture(world, "grid.png", false),
            _ => {}
        }
    }
    if world.resource::<Trial>().frames < 12 {
        return;
    }
    let stage = world.resource::<Trial>().stage;
    let directory = world.resource::<Trial>().directory.clone();
    let Ok(root) = world
        .query_filtered::<Entity, (With<BoxRoot>, With<Workspaces>)>()
        .single(world)
    else {
        return;
    };
    if stage == 0 {
        if world.resource::<Trial>().mode == "recover" {
            let owner = world
                .query_filtered::<Entity, With<Ide>>()
                .iter(world)
                .next();
            let Some(owner) = owner else {
                return;
            };
            world.resource_mut::<Trial>().owner = Some(owner);
            advance(world);
            return;
        }
        let sample = directory.join("sample.txt");
        let owner = lince_desktop::ide::spawn(
            world,
            root,
            1,
            DVec2::ZERO,
            Ide {
                explorer: FileExplorer {
                    roots: vec![directory.clone()],
                    ..default()
                },
                paths: vec![sample.clone(), directory.join("large.txt")],
                active: Some(sample),
                ..default()
            },
        );
        world.resource_mut::<Trial>().owner = Some(owner);
        advance(world);
        return;
    }
    let owner = world.resource::<Trial>().owner.unwrap();
    let Some(editor) = editor(world, owner) else {
        return;
    };
    if world.resource::<Trial>().frames == 120 {
        println!("Waiting at stage {stage}");
        let mut entity = editor;
        loop {
            println!(
                "Node {entity:?}: size={:?}, display={:?}, camera={:?}",
                world.get::<ComputedNode>(entity).map(ComputedNode::size),
                world.get::<Node>(entity).map(|node| node.display),
                world.get::<UiTargetCamera>(entity)
            );
            let Some(parent) = world.get::<ChildOf>(entity) else {
                break;
            };
            entity = parent.parent();
        }
        for (entity, camera, target) in world
            .query::<(Entity, &Camera, &bevy::camera::RenderTarget)>()
            .iter(world)
        {
            println!(
                "Camera {entity:?}: active={}, viewport={:?}, target={target:?}",
                camera.is_active,
                camera.physical_viewport_size()
            );
        }
        capture(world, &format!("waiting-{stage}.png"), false);
    }
    let value = world
        .get::<EditableText>(editor)
        .unwrap()
        .value()
        .to_string();
    let window = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
        .unwrap();
    match stage {
        1 => {
            if world.resource::<Trial>().mode == "recover" {
                if value != "Xalpha\nexternal\n" {
                    return;
                }
                assert_eq!(
                    std::fs::read_to_string(directory.join("sample.txt")).unwrap(),
                    "alpha\nexternal\n"
                );
                click(world, owner, "Save");
                world.resource_mut::<Trial>().stage = 20;
                world.resource_mut::<Trial>().frames = 0;
                return;
            }
            if value != "alpha\nbeta\n" {
                return;
            }
            if world.get::<ComputedNode>(editor).unwrap().size().y <= 20.0 {
                return;
            }
            world
                .resource_mut::<InputFocus>()
                .set(editor, FocusCause::Navigated);
            world
                .get_mut::<EditableText>(editor)
                .unwrap()
                .queue_edit(TextEdit::TextStart(false));
            advance(world);
        }
        2 => {
            for state in [ButtonState::Pressed, ButtonState::Released] {
                world.write_message(KeyboardInput {
                    window,
                    state,
                    key_code: KeyCode::KeyX,
                    logical_key: Key::Character("X".into()),
                    text: Some("X".into()),
                    repeat: false,
                });
            }
            advance(world);
        }
        3 => {
            assert_eq!(
                value, "Xalpha\nbeta\n",
                "Native keyboard input must reach the buffer"
            );
            if world.resource::<Trial>().mode == "crash" {
                let durable = std::fs::read_dir(directory.join("editor-drafts"))
                    .unwrap()
                    .filter_map(Result::ok)
                    .filter(|entry| {
                        entry
                            .path()
                            .extension()
                            .is_some_and(|extension| extension == "draft")
                    })
                    .any(|entry| {
                        std::fs::read(entry.path()).is_ok_and(|bytes| {
                            bytes
                                .windows(b"Xalpha\nbeta\n".len())
                                .any(|slice| slice == b"Xalpha\nbeta\n")
                        })
                    });
                if !durable {
                    return;
                }
                println!(
                    "Recovery checkpoint present; terminating without application shutdown or destructors"
                );
                std::process::exit(86);
            }
            std::fs::write(directory.join("sample.txt"), "alpha\nexternal\n").unwrap();
            advance(world);
        }
        4 => {
            if value != "Xalpha\nexternal\n" {
                return;
            }
            capture(world, "merged.png", false);
            click(world, owner, "Save");
            advance(world);
        }
        5 => {
            if std::fs::read_to_string(directory.join("sample.txt")).unwrap()
                != "Xalpha\nexternal\n"
            {
                return;
            }
            world
                .get_mut::<EditableText>(editor)
                .unwrap()
                .queue_edit(TextEdit::SelectAll);
            advance(world);
        }
        6 => {
            assert!(
                world
                    .resource_mut::<bevy::text::FontCx>()
                    .collection
                    .family_id("Noto Sans Mono CJK JP")
                    .is_none(),
                "CJK must remain unloaded until the interface needs it"
            );
            let preedit = "猫한글かな中文";
            world.write_message(Ime::Enabled { window });
            world.write_message(Ime::Preedit {
                window,
                value: preedit.into(),
                cursor: Some((preedit.len(), preedit.len())),
            });
            advance(world);
        }
        7 => {
            assert!(world.get::<EditableText>(editor).unwrap().is_composing());
            assert_glyphs(world, editor);
            world.write_message(Ime::Commit {
                window,
                value: "猫한글かな中文\n".into(),
            });
            advance(world);
        }
        8 => {
            assert_eq!(
                value, "猫한글かな中文\n",
                "IME must replace the entire selection"
            );
            assert_glyphs(world, editor);
            let mut input = world.get_mut::<EditableText>(editor).unwrap();
            input.queue_edit(TextEdit::TextEnd(false));
            input.queue_edit(TextEdit::Left(false));
            input.queue_edit(TextEdit::Left(true));
            world.resource_mut::<Trial>().stage = 80;
            world.resource_mut::<Trial>().frames = 0;
        }
        80 => {
            assert_eq!(
                world
                    .get::<EditableText>(editor)
                    .unwrap()
                    .editor
                    .selected_text(),
                Some("文")
            );
            click(world, owner, "Undo");
            world.resource_mut::<Trial>().stage = 9;
            world.resource_mut::<Trial>().frames = 0;
        }
        9 => {
            assert_eq!(value, "Xalpha\nexternal\n");
            click(world, owner, "Save");
            let tab = world
                .query::<(Entity, &lince_desktop::icons::Tooltip, &ActionButton)>()
                .iter(world)
                .find(|(_, tip, action)| {
                    action.target == owner
                        && tip.0 == directory.join("large.txt").display().to_string()
                })
                .unwrap()
                .0;
            world.trigger(bevy::ui_widgets::Activate { entity: tab });
            advance(world);
        }
        10 => {
            if !value.starts_with("line 00000") {
                return;
            }
            let content = world.get::<ChildOf>(editor).unwrap().parent();
            let viewport = world.get::<ChildOf>(content).unwrap().parent();
            world.get_mut::<ScrollPosition>(viewport).unwrap().y = 20_000.0 * 24.0;
            advance(world);
        }
        11 => {
            assert!(
                value.contains("line 20000"),
                "Scrolling must project the selected part of the file"
            );
            assert!(value.len() <= lince_editor::MAX_WINDOW_BYTES);
            let explorer = world
                .query_filtered::<Entity, With<FileExplorer>>()
                .iter(world)
                .find(|entity| descendant(world, *entity, owner))
                .unwrap();
            click(world, explorer, "Grid");
            advance(world);
        }
        12 => {
            let input = world
                .spawn((
                    lince_desktop::sand::text_editor(
                        "",
                        world.resource::<lince_desktop::theme::Typography>(),
                        0,
                    ),
                    ChildOf(root),
                ))
                .id();
            world.resource_mut::<Trial>().input = Some(input);
            world
                .get_mut::<EditableText>(input)
                .unwrap()
                .editor
                .set_text(&directory.join("asset.glb").display().to_string());
            BrowseFor {
                input,
                extensions: vec!["glb".into()],
                directories: false,
            }
            .apply(world, root);
            advance(world);
        }
        13 => {
            let Some((entity, _)) = world
                .query::<(Entity, &lince_desktop::icons::Tooltip)>()
                .iter(world)
                .find(|(entity, tip)| tip.0 == "asset.glb" && !descendant(world, *entity, owner))
            else {
                return;
            };
            world.trigger(bevy::ui_widgets::Activate { entity });
            advance(world);
        }
        14 => {
            let input = world.resource::<Trial>().input.unwrap();
            assert_eq!(
                world
                    .get::<EditableText>(input)
                    .unwrap()
                    .value()
                    .to_string(),
                directory.join("asset.glb").display().to_string()
            );
            assert_eq!(world.query::<&Ide>().iter(world).count(), 1);
            let explorer = explorer(world, owner);
            click(world, explorer, "Tree");
            click(world, explorer, "Files…");
            destination(
                world,
                explorer,
                &directory.join("new-folder").display().to_string(),
            );
            click(world, explorer, "New folder");
            advance(world);
        }
        15 => {
            if !directory.join("new-folder").is_dir() {
                return;
            }
            let explorer = explorer(world, owner);
            if !has_button(world, explorer, "▸ new-folder") {
                return;
            }
            click(world, explorer, "▸ new-folder");
            destination(world, explorer, "renamed-folder");
            click(world, explorer, "Rename");
            advance(world);
        }
        16 => {
            if !directory.join("renamed-folder").is_dir() {
                return;
            }
            let explorer = explorer(world, owner);
            if !has_button(world, explorer, "▸ renamed-folder") {
                return;
            }
            click(world, explorer, "▸ renamed-folder");
            destination(
                world,
                explorer,
                &directory.join("moved-folder").display().to_string(),
            );
            click(world, explorer, "Move");
            advance(world);
        }
        17 => {
            if !directory.join("moved-folder").is_dir() {
                return;
            }
            let explorer = explorer(world, owner);
            if !has_button(world, explorer, "▸ moved-folder") {
                return;
            }
            click(world, explorer, "▸ moved-folder");
            click(world, explorer, "Delete…");
            advance(world);
        }
        18 => {
            assert!(directory.join("moved-folder").is_dir());
            if !world
                .query::<&Text>()
                .iter(world)
                .any(|text| text.0.contains("Press Delete… again"))
            {
                return;
            }
            let explorer = explorer(world, owner);
            click(world, explorer, "Delete…");
            advance(world);
        }
        19 => {
            if directory.join("moved-folder").exists() {
                return;
            }
            if !world.query::<&Text>().iter(world).any(|text| {
                text.0.starts_with("Moved to ") && text.0.ends_with("Undo delete restores it.")
            }) {
                return;
            }
            let explorer = explorer(world, owner);
            click(world, explorer, "Undo delete");
            world.resource_mut::<Trial>().stage = 30;
            world.resource_mut::<Trial>().frames = 0;
        }
        30 => {
            if !directory.join("moved-folder").is_dir() {
                return;
            }
            click(
                world,
                owner,
                &directory.join("sample.txt").display().to_string(),
            );
            advance(world);
        }
        31 => {
            if value != "Xalpha\nexternal\n" {
                return;
            }
            click(world, owner, "Settings");
            click(world, owner, "Autosave: off");
            click(world, owner, "Find");
            field(world, owner, "Find text", "Xalpha");
            field(world, owner, "Replacement text", "welcome");
            click(world, owner, "Replace all");
            advance(world);
        }
        32 => {
            if std::fs::read_to_string(directory.join("sample.txt")).unwrap()
                != "welcome\nexternal\n"
            {
                return;
            }
            assert_eq!(value, "welcome\nexternal\n");
            click(world, owner, "Autosave: 2 s");
            click(world, owner, "Autosave: 5 s");
            world
                .resource_mut::<InputFocus>()
                .set(editor, FocusCause::Navigated);
            world
                .get_mut::<EditableText>(editor)
                .unwrap()
                .queue_edit(TextEdit::TextStart(false));
            advance(world);
        }
        33 => {
            for state in [ButtonState::Pressed, ButtonState::Released] {
                world.write_message(KeyboardInput {
                    window,
                    state,
                    key_code: KeyCode::Tab,
                    logical_key: Key::Tab,
                    text: None,
                    repeat: false,
                });
            }
            advance(world);
        }
        34 => {
            assert_eq!(value, "    welcome\nexternal\n");
            click(world, owner, "Close tab");
            advance(world);
        }
        35 => {
            assert!(
                world
                    .get::<Ide>(owner)
                    .unwrap()
                    .paths
                    .contains(&directory.join("sample.txt"))
            );
            click(world, owner, "Cancel");
            assert_eq!(value, "    welcome\nexternal\n");
            click(world, owner, "Save");
            advance(world);
        }
        36 => {
            if std::fs::read_to_string(directory.join("sample.txt")).unwrap()
                != "    welcome\nexternal\n"
            {
                return;
            }
            println!(
                "Graphical checks passed: input, IME, merge, save, undo, tabs, scrolling, grid, picker, file operations, settings, replace all, autosave, Tab indentation, and close cancellation."
            );
            capture(world, "complete.png", true);
            advance(world);
        }
        20 => {
            if std::fs::read_to_string(directory.join("sample.txt")).unwrap()
                != "Xalpha\nexternal\n"
            {
                return;
            }
            println!(
                "Crash recovery passed: unsaved text restored, later disk change merged, source untouched until Save."
            );
            capture(world, "recovered.png", true);
            advance(world);
        }
        _ => {}
    }
}

#[tokio::main]
async fn main() {
    let directory = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("provide an empty artifact directory"),
    );
    let mode = std::env::args().nth(2).unwrap_or_else(|| "check".into());
    assert!(matches!(mode.as_str(), "check" | "crash" | "recover"));
    std::fs::create_dir_all(&directory).unwrap();
    if mode == "recover" {
        assert!(directory.join("editor-drafts").is_dir() && directory.join("sample.txt").is_file());
        std::fs::write(directory.join("sample.txt"), "alpha\nexternal\n").unwrap();
    } else {
        assert!(
            std::fs::read_dir(&directory).unwrap().next().is_none(),
            "Use an empty artifact directory"
        );
        std::fs::write(directory.join("sample.txt"), "alpha\nbeta\n").unwrap();
        std::fs::write(
            directory.join("large.txt"),
            (0..40_000)
                .map(|n| format!("line {n:05} — Unicode 猫\n"))
                .collect::<String>(),
        )
        .unwrap();
        std::fs::write(directory.join("asset.glb"), []).unwrap();
    }
    let mut app = lince_desktop::app::interface_app();
    app.insert_resource(WorkspaceFile::new(directory.join("interface.json")))
        .insert_resource(Trial {
            directory,
            stage: 0,
            frames: 0,
            began: Instant::now(),
            owner: None,
            input: None,
            mode,
        })
        .insert_resource(WinitSettings::continuous())
        .add_systems(Startup, |world: &mut World| {
            world.spawn(BoxRoot);
            world
                .query_filtered::<&mut Window, With<PrimaryWindow>>()
                .single_mut(world)
                .unwrap()
                .resolution
                .set(1640.0, 1020.0);
        })
        .add_systems(
            Update,
            exercise.after(lince_desktop::workspace::PrepareWorkspaces),
        );
    app.run();
}
