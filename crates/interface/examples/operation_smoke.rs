use bevy::{
    app::AppExit,
    diagnostic::FrameCount,
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
    },
    input_focus::InputFocus,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    text::EditableText,
    window::{PrimaryWindow, WindowEvent},
    winit::WinitSettings,
};
use lince_interface::{app::interface_app, operation::OperationSand};

#[derive(Resource)]
struct Capture(String);

fn keyboard(world: &mut World, code: KeyCode, key: Key, state: ButtonState) {
    let window = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
        .unwrap();
    let input = KeyboardInput {
        key_code: code,
        logical_key: key,
        state,
        window,
        text: None,
        repeat: false,
    };
    world.write_message(input.clone());
    world.write_message(WindowEvent::KeyboardInput(input));
}

fn exercise(world: &mut World) {
    match world.resource::<FrameCount>().0 {
        12 => keyboard(
            world,
            KeyCode::ControlLeft,
            Key::Control,
            ButtonState::Pressed,
        ),
        14 => keyboard(
            world,
            KeyCode::KeyK,
            Key::Character("k".into()),
            ButtonState::Pressed,
        ),
        16 => keyboard(
            world,
            KeyCode::ControlLeft,
            Key::Control,
            ButtonState::Released,
        ),
        20 => {
            assert_eq!(world.query::<&OperationSand>().iter(world).count(), 1);
            let input = world.resource::<InputFocus>().get().unwrap();
            let node = world.get::<ComputedNode>(input).unwrap();
            assert!(node.size().x > 100.0 && node.size().y > 20.0);
            world
                .get_mut::<EditableText>(input)
                .unwrap()
                .editor
                .set_text("/he");
        }
        24 => keyboard(world, KeyCode::Tab, Key::Tab, ButtonState::Pressed),
        28 => {
            let input = world.resource::<InputFocus>().get().unwrap();
            assert_eq!(
                world
                    .get::<EditableText>(input)
                    .unwrap()
                    .value()
                    .to_string(),
                "/help"
            );
        }
        30 => keyboard(world, KeyCode::Enter, Key::Enter, ButtonState::Pressed),
        34 => {
            assert_eq!(world.query::<&OperationSand>().iter(world).count(), 0);
            assert!(
                world
                    .query::<&Text>()
                    .iter(world)
                    .any(|text| text.0 == "Cheat sheet")
            );
        }
        36 => keyboard(
            world,
            KeyCode::ControlLeft,
            Key::Control,
            ButtonState::Pressed,
        ),
        38 => keyboard(
            world,
            KeyCode::KeyK,
            Key::Character("k".into()),
            ButtonState::Pressed,
        ),
        40 => keyboard(
            world,
            KeyCode::ControlLeft,
            Key::Control,
            ButtonState::Released,
        ),
        44 => {
            let input = world.resource::<InputFocus>().get().unwrap();
            world
                .get_mut::<EditableText>(input)
                .unwrap()
                .editor
                .set_text("@ap");
        }
        50 => {
            assert_eq!(world.query::<&OperationSand>().iter(world).count(), 1);
            let path = world.resource::<Capture>().0.clone();
            world.spawn(Screenshot::primary_window()).observe(save_to_disk(path)).observe(
                |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                    println!("Operation smoke passed: Ctrl+K, focus, Tab completion, command submission and cheat-sheet navigation.");
                    exit.write(AppExit::Success);
                },
            );
        }
        1200 => panic!("Operation smoke timed out"),
        _ => {}
    }
}

#[tokio::main]
async fn main() {
    interface_app()
        .insert_resource(Capture(
            std::env::args().nth(1).expect("provide screenshot path"),
        ))
        .insert_resource(WinitSettings::continuous())
        .add_systems(Startup, |mut commands: Commands, mut messages: MessageWriter<lince_interface::cell_bridge::CellMessage>| {
            commands.spawn(lince_interface::container::BoxRoot);
            messages.write(lince_interface::cell_bridge::CellMessage(cell::ServerMessage::Snapshot {
                id: lince_interface::cell_bridge::RECORDS.into(),
                rows: vec![
                    serde_json::json!({"uid":"apple", "slug":"apple", "head":"Apples"}),
                    serde_json::json!({"uid":"apricot", "slug":"apricot", "head":"Apricots"}),
                ],
            }));
        })
        .add_systems(Update, exercise)
        .run();
}
