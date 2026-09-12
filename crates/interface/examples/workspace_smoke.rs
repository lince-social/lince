use bevy::{
    diagnostic::FrameCount,
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
    },
    input_focus::{FocusCause, InputFocus},
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    text::{EditableText, TextEdit},
    ui_widgets::Activate,
    window::PrimaryWindow,
    winit::WinitSettings,
};
use lince_interface::{
    app::interface_app,
    canvas::{CanvasItem, CanvasView},
    canvas_controls::{CanvasAction, CanvasControl},
    container::BoxRoot,
    edit_mode::{EditAction, EditControl, EditField, EditMode, EditPopupToggle},
    sand_store::{SandKind, StoredSand},
    workspace::{WorkspaceFile, Workspaces},
};

#[derive(Resource)]
struct Exercise {
    path: String,
    toggles: Vec<bool>,
    note: Option<Entity>,
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("provide a screenshot output path");
    let directory = tempfile::tempdir().unwrap();
    let mut app = interface_app();
    app.insert_resource(WorkspaceFile::new(directory.path().join("interface.json")))
        .insert_resource(Exercise {
            path,
            toggles: Vec::new(),
            note: None,
        })
        .insert_resource(WinitSettings::continuous())
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(BoxRoot);
        })
        .add_observer(|event: On<EditPopupToggle>, mut test: ResMut<Exercise>| {
            test.toggles.push(event.enabled);
        })
        .add_systems(Update, exercise);
    app.run();
}

fn activate(world: &mut World, action: EditAction) {
    let entity = world
        .query::<(Entity, &EditControl)>()
        .iter(world)
        .find(|(_, control)| control.action == action)
        .unwrap()
        .0;
    world.trigger(Activate { entity });
}

fn edit(world: &mut World, field: EditField, value: &str) {
    let entity = world
        .query::<(Entity, &EditField)>()
        .iter(world)
        .find(|(_, candidate)| **candidate == field)
        .unwrap()
        .0;
    let mut text = world.get_mut::<EditableText>(entity).unwrap();
    text.queue_edit(TextEdit::SelectAll);
    text.queue_edit(TextEdit::Insert(value.into()));
}

fn key(world: &mut World, code: KeyCode, logical_key: Key, state: ButtonState) {
    let window = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
        .unwrap();
    world.write_message(KeyboardInput {
        key_code: code,
        logical_key,
        state,
        text: None,
        repeat: false,
        window,
    });
}

fn capture(world: &mut World, suffix: &str) {
    let path = format!("{}{}", world.resource::<Exercise>().path, suffix);
    world
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path));
}

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    let Ok(root) = world
        .query_filtered::<Entity, With<BoxRoot>>()
        .single(world)
    else {
        return;
    };
    match frame {
        15 => {
            let toggle = world.get::<EditMode>(root).unwrap().toggle;
            world
                .resource_mut::<InputFocus>()
                .set(toggle, FocusCause::Navigated);
            key(world, KeyCode::Enter, Key::Enter, ButtonState::Pressed);
        }
        16 => key(world, KeyCode::Enter, Key::Enter, ButtonState::Released),
        20 => {
            assert!(world.get::<EditMode>(root).unwrap().enabled);
            assert_eq!(world.resource::<Exercise>().toggles, vec![true]);
            activate(world, EditAction::CreateWorkspace);
        }
        24 => {
            edit(world, EditField::WorkspaceName, "Writing");
        }
        30 => {
            assert_eq!(
                world.get::<Workspaces>(root).unwrap().entries[1].name,
                "Writing"
            );
            capture(world, ".workspaces.png");
        }
        34 => activate(world, EditAction::Store),
        38 => {
            edit(world, EditField::StartingText, "A note from the Sand store");
            activate(world, EditAction::AddSand(SandKind::EditableText));
        }
        44 => {
            let (entity, sand) = world
                .query::<(Entity, &StoredSand)>()
                .single(world)
                .unwrap();
            let editor = sand.content.unwrap();
            assert_eq!(
                world
                    .get::<EditableText>(editor)
                    .unwrap()
                    .value()
                    .to_string(),
                "A note from the Sand store"
            );
            world.resource_mut::<Exercise>().note = Some(entity);
            world
                .get_mut::<EditableText>(editor)
                .unwrap()
                .queue_edit(TextEdit::Insert(" — draft kept".into()));
            world.get_mut::<CanvasView>(root).unwrap().center = DVec2::splat(500.0);
            world.get_mut::<CanvasView>(root).unwrap().zoom = 1.2;
            world.get_mut::<CanvasItem>(entity).unwrap().position = DVec2::splat(500.0);
            activate(world, EditAction::Workspaces);
        }
        48 => activate(world, EditAction::SwitchWorkspace(1)),
        52 => {
            let note = world.resource::<Exercise>().note.unwrap();
            assert_eq!(world.get::<Node>(note).unwrap().display, Display::None);
            assert_eq!(world.get::<CanvasView>(root).unwrap().center, DVec2::ZERO);
            let bring = world
                .query::<(Entity, &CanvasControl)>()
                .iter(world)
                .find(|(_, control)| control.action == CanvasAction::BringHere)
                .unwrap()
                .0;
            world.trigger(Activate { entity: bring });
            assert_eq!(
                world.get::<CanvasItem>(note).unwrap().position,
                DVec2::splat(500.0)
            );
            activate(world, EditAction::SwitchWorkspace(2));
        }
        58 => {
            let note = world.resource::<Exercise>().note.unwrap();
            let editor = world.get::<StoredSand>(note).unwrap().content.unwrap();
            assert_eq!(world.get::<Node>(note).unwrap().display, Display::Flex);
            assert_eq!(world.get::<CanvasView>(root).unwrap().zoom, 1.2);
            assert!(
                world
                    .get::<EditableText>(editor)
                    .unwrap()
                    .value()
                    .to_string()
                    .contains("draft kept")
            );
            activate(world, EditAction::Store);
        }
        62 => {
            let credits = world
                .query::<(Entity, &EditControl)>()
                .iter(world)
                .find(|(_, control)| control.action == EditAction::Credits)
                .unwrap()
                .0;
            world
                .resource_mut::<InputFocus>()
                .set(credits, FocusCause::Navigated);
        }
        68 => {
            let panel = world.get::<EditMode>(root).unwrap().panel;
            assert!(world.get::<ScrollPosition>(panel).unwrap().0.y > 0.0);
            capture(world, ".store.png");
        }
        72 => activate(world, EditAction::Credits),
        78 => {
            assert!(
                world
                    .query::<&Text>()
                    .iter(world)
                    .any(|text| text.0.contains("Permission is hereby granted"))
            );
            key(world, KeyCode::Escape, Key::Escape, ButtonState::Pressed);
        }
        79 => key(world, KeyCode::Escape, Key::Escape, ButtonState::Released),
        84 => {
            assert!(!world.get::<EditMode>(root).unwrap().enabled);
            assert_eq!(world.resource::<Exercise>().toggles, vec![true, false]);
            let path = world.resource::<Exercise>().path.clone();
            world.spawn(Screenshot::primary_window()).observe(save_to_disk(path)).observe(|_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                println!("Workspace smoke passed: keyboard edit mode, same-frame rename and Sand text, workspace isolation, draft retention, scrolling, licenses and Escape.");
                exit.write(AppExit::Success);
            });
        }
        240 => panic!("Workspace smoke timed out"),
        _ => {}
    }
}
