use bevy::{
    diagnostic::FrameCount,
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
        mouse::MouseButtonInput,
    },
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    ui_widgets::SliderValue,
    window::{CursorMoved, PrimaryWindow, WindowEvent},
    winit::WinitSettings,
};
use lince_interface::{
    actions::Action,
    app::interface_app,
    container::BoxRoot,
    edit_mode::EditAction,
    slider::SliderSand,
    tokens::{ThemeSettings, Token, TokenValue},
};

#[derive(Resource)]
struct Probe {
    path: String,
    captures: usize,
}

fn slider(world: &mut World) -> Entity {
    world
        .query_filtered::<Entity, With<SliderSand>>()
        .single(world)
        .unwrap()
}

fn mouse(world: &mut World, fraction: Option<f32>, state: Option<ButtonState>) {
    let window = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
        .unwrap();
    if let Some(fraction) = fraction {
        let entity = slider(world);
        let node = world.get::<ComputedNode>(entity).unwrap();
        let size = node.size() * node.inverse_scale_factor();
        let center = world.get::<UiGlobalTransform>(entity).unwrap().translation
            * node.inverse_scale_factor();
        let position = center + Vec2::new((fraction - 0.5) * (size.x - 16.0), 0.0);
        world.write_message(WindowEvent::CursorMoved(CursorMoved {
            window,
            position,
            delta: None,
        }));
    }
    if let Some(state) = state {
        world.write_message(WindowEvent::MouseButtonInput(MouseButtonInput {
            window,
            button: MouseButton::Left,
            state,
        }));
    }
}

fn assert_value(world: &mut World, expected: f32) {
    let entity = slider(world);
    assert_eq!(world.get::<SliderValue>(entity).unwrap().0, expected);
    assert_eq!(
        world.resource::<ThemeSettings>().global.0[&Token::CanvasPattern],
        TokenValue::Number(expected)
    );
    assert!(
        world
            .query::<&Text>()
            .iter(world)
            .any(|text| text.0 == format!("{expected:.0}%"))
    );
}

fn capture(world: &mut World, name: &str) {
    let path = format!("{}.{}.png", world.resource::<Probe>().path, name);
    let name = name.to_string();
    world
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path))
        .observe(
            move |event: On<ScreenshotCaptured>, mut probe: ResMut<Probe>| {
                let red = |x, y| event.image.get_color_at(x, y).unwrap().to_srgba().red * 255.0;
                assert!(red(16, 32) > 40.0, "missing pattern center");
                assert!(red(22, 38) < 20.0, "pattern filled the space between arms");
                match name.as_str() {
                    "dots" => assert!(red(17, 32) < 20.0),
                    "one-percent" => assert!(red(17, 32) < 23.0, "dot-to-cross transition jumped"),
                    "small-crosses" => assert!(red(17, 32) > 35.0 && red(19, 32) < 20.0),
                    "crosses" => assert!(red(22, 32) > 40.0 && red(26, 32) < 20.0),
                    "lines" => assert!(red(32, 32) > 40.0 && red(16, 48) > 40.0),
                    _ => unreachable!(),
                }
                probe.captures += 1;
            },
        );
}

fn exercise(world: &mut World) {
    let root = world
        .query_filtered::<Entity, With<BoxRoot>>()
        .single(world)
        .unwrap();
    match world.resource::<FrameCount>().0 {
        8 => {
            EditAction::Open.apply(world, root);
            EditAction::Customization.apply(world, root);
        }
        16 => mouse(world, Some(0.0), None),
        18 => mouse(world, None, Some(ButtonState::Pressed)),
        20 => mouse(world, None, Some(ButtonState::Released)),
        24 => {
            assert_value(world, 0.0);
            capture(world, "dots");
        }
        28 => {
            let entity = slider(world);
            world
                .resource_mut::<bevy::input_focus::InputFocus>()
                .set(entity, bevy::input_focus::FocusCause::Navigated);
            let window = world
                .query_filtered::<Entity, With<PrimaryWindow>>()
                .single(world)
                .unwrap();
            let input = KeyboardInput {
                window,
                key_code: KeyCode::ArrowRight,
                logical_key: Key::ArrowRight,
                state: ButtonState::Pressed,
                text: None,
                repeat: false,
            };
            world.write_message(input.clone());
            world.write_message(WindowEvent::KeyboardInput(input));
        }
        34 => {
            assert_value(world, 5.0);
            capture(world, "small-crosses");
        }
        36 => mouse(world, Some(0.05), None),
        38 => mouse(world, None, Some(ButtonState::Pressed)),
        40 => mouse(world, Some(0.25), None),
        42 => mouse(world, Some(0.50), None),
        44 => mouse(world, None, Some(ButtonState::Released)),
        48 => {
            assert_value(world, 50.0);
            capture(world, "crosses");
        }
        50 => mouse(world, Some(1.0), None),
        52 => mouse(world, None, Some(ButtonState::Pressed)),
        54 => mouse(world, None, Some(ButtonState::Released)),
        58 => {
            assert_value(world, 100.0);
            capture(world, "lines");
        }
        62 => {
            world
                .resource_mut::<ThemeSettings>()
                .global
                .set(Token::CanvasPattern, TokenValue::Number(1.0));
        }
        66 => capture(world, "one-percent"),
        78 => {
            assert_eq!(world.resource::<Probe>().captures, 5);
            println!(
                "Pattern smoke passed: click, drag, keyboard steps, live numeric readout, dots, crosses and full grid."
            );
            world.write_message(AppExit::Success);
        }
        1800 => panic!("Pattern smoke timed out"),
        _ => {}
    }
}

fn main() {
    interface_app()
        .insert_resource(Probe {
            path: std::env::args().nth(1).expect("provide screenshot prefix"),
            captures: 0,
        })
        .insert_resource(WinitSettings::continuous())
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(BoxRoot);
        })
        .add_systems(Update, exercise)
        .run();
}
