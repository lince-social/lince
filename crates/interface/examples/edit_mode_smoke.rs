use bevy::{
    diagnostic::FrameCount,
    input::{ButtonState, mouse::MouseButtonInput},
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    window::{CursorMoved, PrimaryWindow, WindowEvent},
    winit::WinitSettings,
};
use lince_interface::{app::interface_app, container::BoxRoot, edit_mode::EditMode};

#[derive(Resource)]
struct CapturePath(String);

fn exercise(world: &mut World) {
    if world.resource::<FrameCount>().0 < 12 {
        return;
    }
    let root = world
        .query_filtered::<Entity, With<BoxRoot>>()
        .single(world)
        .unwrap();
    let mode = world.get::<EditMode>(root).unwrap();
    let toggle = mode.toggle;
    let panel = mode.panel;
    let window = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
        .unwrap();
    match world.resource::<FrameCount>().0 {
        12 | 26 | 56 => {
            let position = world.get::<UiGlobalTransform>(toggle).unwrap().translation
                * world
                    .get::<ComputedNode>(toggle)
                    .unwrap()
                    .inverse_scale_factor();
            world.write_message(WindowEvent::CursorMoved(CursorMoved {
                window,
                position,
                delta: None,
            }));
        }
        14 | 28 | 58 => {
            if world.resource::<FrameCount>().0 == 14 {
                assert!(
                    world
                        .get::<Children>(toggle)
                        .unwrap()
                        .iter()
                        .any(|child| world.get::<ImageNode>(child).is_some()),
                    "Edit mode must display an icon"
                );
                assert!(
                    world
                        .query::<&Text>()
                        .iter(world)
                        .any(|text| text.0 == "Edit mode"),
                    "hovering the icon must show its label"
                );
            }
            world.write_message(WindowEvent::MouseButtonInput(MouseButtonInput {
                window,
                button: MouseButton::Left,
                state: ButtonState::Pressed,
            }));
        }
        16 | 30 | 60 => {
            world.write_message(WindowEvent::MouseButtonInput(MouseButtonInput {
                window,
                button: MouseButton::Left,
                state: ButtonState::Released,
            }));
        }
        22 | 68 => {
            assert!(
                world.get::<EditMode>(root).unwrap().enabled,
                "a real click must open Edit mode"
            );
            let button = world.get::<ComputedNode>(toggle).unwrap();
            let button_position = world.get::<UiGlobalTransform>(toggle).unwrap().translation;
            let node = world.get::<ComputedNode>(panel).unwrap();
            let position = world.get::<UiGlobalTransform>(panel).unwrap().translation;
            assert!(node.size().min_element() > 0.0);
            assert!(
                position.y + node.size().y * 0.5 < button_position.y - button.size().y * 0.5,
                "panel must open above its button"
            );
            assert!(
                (position.x + node.size().x * 0.5 - button_position.x - button.size().x * 0.5)
                    .abs()
                    < 9.0,
                "panel must align with its button: panel right {}, button right {}",
                position.x + node.size().x * 0.5,
                button_position.x + button.size().x * 0.5,
            );
            if world.resource::<FrameCount>().0 == 68 {
                assert!(world.query_filtered::<&Visibility, With<lince_interface::time_limit::TimeLimit>>().iter(world).all(|visibility| *visibility == Visibility::Hidden), "leaving the icon must hide its tooltip");
                let path = world.resource::<CapturePath>().0.clone();
                world.spawn(Screenshot::primary_window()).observe(save_to_disk(path)).observe(|_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                    println!("Edit mode smoke passed: real clicks open, close and reopen the panel above its button after resizing.");
                    exit.write(AppExit::Success);
                });
            }
        }
        64 => {
            world.write_message(WindowEvent::CursorMoved(CursorMoved {
                window,
                position: Vec2::new(10.0, 10.0),
                delta: None,
            }));
        }
        36 => assert!(
            !world.get::<EditMode>(root).unwrap().enabled,
            "a second click must close Edit mode"
        ),
        38 => world
            .get_mut::<Window>(window)
            .unwrap()
            .resolution
            .set(460.0, 500.0),
        1800 => panic!("Edit mode smoke timed out"),
        _ => {}
    }
}

fn main() {
    interface_app()
        .insert_resource(CapturePath(
            std::env::args().nth(1).expect("provide screenshot path"),
        ))
        .insert_resource(WinitSettings::continuous())
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(BoxRoot);
        })
        .add_systems(Update, exercise)
        .run();
}
