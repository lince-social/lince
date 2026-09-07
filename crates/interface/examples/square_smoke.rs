use bevy::{
    diagnostic::FrameCount,
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
    },
    input_focus::{FocusCause, InputFocus},
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    text::{EditableText, TextEdit},
    ui_widgets::Activate,
    window::PrimaryWindow,
    winit::WinitSettings,
};
use lince_interface::native::{BoxRoot, SendBoxEvent, Square, square_app};

#[derive(Resource)]
struct CapturePath(String);

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("provide a screenshot output path");
    square_app()
        .insert_resource(WinitSettings::continuous())
        .insert_resource(CapturePath(path))
        .add_systems(Update, exercise)
        .run();
}

fn exercise(
    frame: Res<FrameCount>,
    path: Res<CapturePath>,
    triggers: Query<(Entity, &SendBoxEvent)>,
    squares: Query<&Visibility, With<Square>>,
    boxes: Query<&BoxRoot>,
    mut editors: Query<(Entity, &mut EditableText)>,
    mut focus: ResMut<InputFocus>,
    mut keyboard: MessageWriter<KeyboardInput>,
    window: Single<Entity, With<PrimaryWindow>>,
    mut commands: Commands,
) {
    let Ok((trigger, effect)) = triggers.single() else {
        return;
    };
    match frame.0 {
        10 => {
            let (entity, mut editor) = editors.single_mut().unwrap();
            editor.queue_edit(TextEdit::SelectAll);
            focus.set(entity, FocusCause::Navigated);
        }
        12 => {
            keyboard.write(KeyboardInput {
                key_code: KeyCode::KeyH,
                logical_key: Key::Character("Hello, Bevy!".into()),
                state: ButtonState::Pressed,
                text: Some("Hello, Bevy!".into()),
                repeat: false,
                window: *window,
            });
        }
        16 => {
            assert_eq!(
                editors.single().unwrap().1.value().to_string(),
                "Hello, Bevy!"
            );
        }
        20 | 40 => commands.trigger(Activate { entity: trigger }),
        30 => {
            assert_eq!(*squares.get(effect.square).unwrap(), Visibility::Hidden);
            assert_eq!(boxes.get(effect.box_entity).unwrap().received_events, 1);
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(format!("{}.hidden.png", path.0)));
        }
        60 => {
            assert_eq!(
                editors.single().unwrap().1.value().to_string(),
                "Hello, Bevy!"
            );
            assert_eq!(*squares.get(effect.square).unwrap(), Visibility::Inherited);
            assert_eq!(boxes.get(effect.box_entity).unwrap().received_events, 2);
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path.0.clone()))
                .observe(
                    |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                        println!("Square smoke passed: keyboard text editing, hide, show, render and screenshot.");
                        exit.write(AppExit::Success);
                    },
                );
        }
        1800 => panic!("Square screenshot did not complete"),
        _ => {}
    }
}
