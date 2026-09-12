use bevy::{
    diagnostic::FrameCount,
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
    },
    input_focus::{FocusCause, InputFocus},
    prelude::*,
    text::EditableText,
    window::{PrimaryWindow, WindowEvent},
    winit::WinitSettings,
};
use lince_interface::{
    actions::{KeyBinding, KeyBindings, Modifiers},
    app::interface_app,
    canvas::CanvasView,
    canvas_controls::CanvasAction,
    container::BoxRoot,
    edit_mode::{EditAction, EditField, EditMode},
    sand_store::{SandKind, StoredSand},
};

fn key(world: &mut World, code: KeyCode, logical: Key, state: ButtonState, repeat: bool) {
    let window = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
        .unwrap();
    let input = KeyboardInput {
        key_code: code,
        text: if let Key::Character(value) = &logical {
            Some(value.clone())
        } else {
            None
        },
        logical_key: logical,
        state,
        repeat,
        window,
    };
    world.write_message(input.clone());
    world.write_message(WindowEvent::KeyboardInput(input));
}

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    if frame < 12 {
        return;
    }
    let root = world
        .query_filtered::<Entity, With<BoxRoot>>()
        .single(world)
        .unwrap();
    match frame {
        12 | 32 | 42 | 62 => key(
            world,
            KeyCode::AltLeft,
            Key::Alt,
            ButtonState::Pressed,
            false,
        ),
        14 | 34 | 44 => key(
            world,
            KeyCode::KeyE,
            Key::Character("e".into()),
            ButtonState::Pressed,
            false,
        ),
        16 | 36 | 46 => key(
            world,
            KeyCode::KeyE,
            Key::Character("e".into()),
            ButtonState::Released,
            false,
        ),
        18 | 38 | 48 | 68 => key(
            world,
            KeyCode::AltLeft,
            Key::Alt,
            ButtonState::Released,
            false,
        ),
        22 => {
            assert!(
                world.get::<EditMode>(root).unwrap().enabled,
                "Alt+E must open Edit mode from the canvas"
            );
            let editor = world
                .query::<(Entity, &EditField)>()
                .iter(world)
                .find(|(_, field)| **field == EditField::WorkspaceName)
                .unwrap()
                .0;
            world
                .get_mut::<EditableText>(editor)
                .unwrap()
                .editor
                .set_text("Draft");
            world
                .resource_mut::<InputFocus>()
                .set(editor, FocusCause::Navigated);
        }
        24 => key(
            world,
            KeyCode::KeyE,
            Key::Character("e".into()),
            ButtonState::Pressed,
            false,
        ),
        26 => key(
            world,
            KeyCode::KeyE,
            Key::Character("e".into()),
            ButtonState::Released,
            false,
        ),
        30 => {
            assert!(
                world.get::<EditMode>(root).unwrap().enabled,
                "plain typing must not toggle Edit mode"
            );
            let editor = world.resource::<InputFocus>().get().unwrap();
            assert!(
                world
                    .get::<EditableText>(editor)
                    .unwrap()
                    .value()
                    .to_string()
                    .contains('e')
            );
        }
        40 => assert!(
            !world.get::<EditMode>(root).unwrap().enabled,
            "Alt+E must work with an editor focused"
        ),
        50 => {
            assert!(world.get::<EditMode>(root).unwrap().enabled);
            let toggle = world.get::<EditMode>(root).unwrap().toggle;
            world
                .resource_mut::<InputFocus>()
                .set(toggle, FocusCause::Navigated);
        }
        52 => key(
            world,
            KeyCode::Escape,
            Key::Escape,
            ButtonState::Pressed,
            false,
        ),
        54 => key(
            world,
            KeyCode::Escape,
            Key::Escape,
            ButtonState::Released,
            false,
        ),
        58 => {
            assert!(
                !world.get::<EditMode>(root).unwrap().enabled,
                "Escape must use the close action"
            );
            world
                .get_mut::<KeyBindings>(root)
                .unwrap()
                .0
                .push(KeyBinding::new(
                    KeyCode::KeyS,
                    Modifiers::ALT,
                    lince_interface::actions![
                        EditAction::Open,
                        EditAction::Store,
                        EditAction::AddSand(SandKind::Text),
                        CanvasAction::ZoomIn
                    ],
                ));
        }
        64 => key(
            world,
            KeyCode::KeyS,
            Key::Character("s".into()),
            ButtonState::Pressed,
            false,
        ),
        65 => key(
            world,
            KeyCode::KeyS,
            Key::Character("s".into()),
            ButtonState::Pressed,
            true,
        ),
        66 => key(
            world,
            KeyCode::KeyS,
            Key::Character("s".into()),
            ButtonState::Released,
            false,
        ),
        74 => {
            assert!(world.get::<EditMode>(root).unwrap().enabled);
            assert_eq!(
                world.query::<&StoredSand>().iter(world).count(),
                1,
                "the parameterized combination must run once despite key repeat"
            );
            assert_eq!(world.get::<CanvasView>(root).unwrap().zoom, 1.2);
            println!(
                "Actions smoke passed: Alt+E, text editing, Escape and a parameterized action combination."
            );
            world.write_message(AppExit::Success);
        }
        1800 => panic!("Actions smoke timed out"),
        _ => {}
    }
}

fn main() {
    interface_app()
        .insert_resource(WinitSettings::continuous())
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(BoxRoot);
        })
        .add_systems(Update, exercise)
        .run();
}
