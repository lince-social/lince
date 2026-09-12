use bevy::{
    diagnostic::FrameCount,
    input::{ButtonState, mouse::MouseButtonInput},
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    text::EditableText,
    ui_widgets::Activate,
    window::{CursorMoved, PrimaryWindow, WindowEvent, WindowFocused},
    winit::WinitSettings,
};
use lince_interface::{
    app::interface_app,
    canvas::{CanvasItem, CanvasView},
    container::BoxRoot,
    sand::{Square, button, text_editor},
    theme::{PURPLE, Typography},
};

#[derive(Resource)]
struct Fixture {
    root: Entity,
    sand: Entity,
    button: Entity,
    editor: Entity,
    activations: usize,
    before: Vec2,
    cursor: Vec2,
    path: String,
}

fn setup(world: &mut World) {
    let root = world.spawn(BoxRoot).id();
    let sand = world
        .spawn((
            Square,
            CanvasItem {
                position: DVec2::ZERO,
                size: Vec2::splat(200.0),
            },
            BackgroundColor(PURPLE),
            ChildOf(root),
        ))
        .id();
    let editor = world
        .spawn((
            text_editor("Keep my text", world.resource::<Typography>(), 0),
            ChildOf(sand),
        ))
        .id();
    let button = world
        .spawn((
            Square,
            button(1),
            CanvasItem {
                position: DVec2::new(-230.0, 0.0),
                size: Vec2::new(140.0, 100.0),
            },
            BackgroundColor(PURPLE),
            ChildOf(root),
        ))
        .observe(|_: On<Activate>, mut fixture: ResMut<Fixture>| fixture.activations += 1)
        .id();
    world.insert_resource(Fixture {
        root,
        sand,
        button,
        editor,
        activations: 0,
        before: Vec2::ZERO,
        cursor: Vec2::ZERO,
        path: std::env::args().nth(1).expect("provide screenshot path"),
    });
}

fn pointer(world: &mut World, position: Vec2) {
    let window = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
        .unwrap();
    world.write_message(WindowEvent::CursorMoved(CursorMoved {
        window,
        position,
        delta: None,
    }));
}

fn press(world: &mut World, down: bool) {
    let window = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
        .unwrap();
    world.write_message(WindowEvent::MouseButtonInput(MouseButtonInput {
        window,
        button: MouseButton::Left,
        state: if down {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        },
    }));
}

fn screen(world: &World, entity: Entity) -> Vec2 {
    world.get::<UiGlobalTransform>(entity).unwrap().translation
        * world
            .get::<ComputedNode>(entity)
            .unwrap()
            .inverse_scale_factor()
}

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    let root = world.resource::<Fixture>().root;
    let sand = world.resource::<Fixture>().sand;
    let button = world.resource::<Fixture>().button;
    match frame {
        10 => {
            world.resource_mut::<Fixture>().before = screen(world, sand);
            let path = world.resource::<Fixture>().path.clone();
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(format!("{path}.before.png")));
        }
        12 => pointer(world, Vec2::new(650.0, 200.0)),
        14 => press(world, true),
        16 => pointer(world, Vec2::new(730.0, 245.0)),
        18 => {
            let expected = world.resource::<Fixture>().before + Vec2::new(80.0, 45.0);
            assert!(
                (screen(world, sand) - expected).length() < 0.1,
                "drag must move the rendered Sand"
            );
            assert_eq!(world.get::<CanvasItem>(sand).unwrap().position, DVec2::ZERO);
        }
        20 => press(world, false),
        22 => pointer(world, Vec2::new(740.0, 255.0)),
        24 => assert_eq!(
            world.get::<CanvasView>(root).unwrap().center,
            DVec2::new(-80.0, -45.0)
        ),
        28 => {
            let mut view = world.get_mut::<CanvasView>(root).unwrap();
            view.center = DVec2::ZERO;
            view.set_zoom(2.0);
        }
        30 => {
            world.resource_mut::<Fixture>().before = screen(world, sand);
            let editor = world.resource::<Fixture>().editor;
            let cursor = screen(world, editor);
            world.resource_mut::<Fixture>().cursor = cursor;
            pointer(world, cursor);
            world
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(KeyCode::ControlLeft);
        }
        32 => press(world, true),
        34 => pointer(
            world,
            world.resource::<Fixture>().cursor + Vec2::new(60.0, -30.0),
        ),
        36 => {
            assert_eq!(
                world.get::<CanvasView>(root).unwrap().center,
                DVec2::new(-30.0, 15.0)
            );
            assert!(
                (screen(world, sand) - world.resource::<Fixture>().before - Vec2::new(60.0, -30.0))
                    .length()
                    < 0.1
            );
            let editor = world.resource::<Fixture>().editor;
            assert_eq!(
                world
                    .get::<EditableText>(editor)
                    .unwrap()
                    .value()
                    .to_string(),
                "Keep my text"
            );
        }
        38 => press(world, false),
        40 => {
            world
                .resource_mut::<ButtonInput<KeyCode>>()
                .release(KeyCode::ControlLeft);
            *world.get_mut::<CanvasView>(root).unwrap() = CanvasView::default();
        }
        42 => pointer(world, screen(world, button)),
        44 => press(world, true),
        46 => press(world, false),
        48 => {
            assert_eq!(world.resource::<Fixture>().activations, 1);
            assert_eq!(world.get::<CanvasView>(root).unwrap().center, DVec2::ZERO);
            world.resource_mut::<Fixture>().before = screen(world, button);
            world
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(KeyCode::ControlRight);
        }
        50 => press(world, true),
        52 => pointer(
            world,
            world.resource::<Fixture>().before + Vec2::new(45.0, 20.0),
        ),
        54 => press(world, false),
        56 => {
            assert_eq!(
                world.resource::<Fixture>().activations,
                1,
                "Ctrl-drag must not activate a Sand button"
            );
            assert_eq!(
                world.get::<CanvasView>(root).unwrap().center,
                DVec2::new(-45.0, -20.0)
            );
            world
                .resource_mut::<ButtonInput<KeyCode>>()
                .release(KeyCode::ControlRight);
            pointer(world, Vec2::new(650.0, 150.0));
        }
        58 => press(world, true),
        60 => pointer(world, Vec2::new(670.0, 160.0)),
        62 => {
            let window = world
                .query_filtered::<Entity, With<PrimaryWindow>>()
                .single(world)
                .unwrap();
            world.write_message(WindowEvent::WindowFocused(WindowFocused {
                window,
                focused: false,
            }));
        }
        64 => pointer(world, Vec2::new(700.0, 190.0)),
        66 => {
            assert_eq!(
                world.get::<CanvasView>(root).unwrap().center,
                DVec2::new(-65.0, -30.0)
            );
            press(world, false);
            let window = world
                .query_filtered::<Entity, With<PrimaryWindow>>()
                .single(world)
                .unwrap();
            world.write_message(WindowEvent::WindowFocused(WindowFocused {
                window,
                focused: true,
            }));
            *world.get_mut::<CanvasView>(root).unwrap() = CanvasView {
                center: DVec2::ZERO,
                zoom: 2.0,
            };
            world.get_mut::<CanvasItem>(sand).unwrap().position = DVec2::new(-100.0, 0.0);
            lince_interface::actions::dispatch(
                world,
                root,
                lince_interface::actions![lince_interface::edit_mode::EditAction::Open],
            );
        }
        70 => {
            let editor = world.resource::<Fixture>().editor;
            let cursor = screen(world, editor);
            world.resource_mut::<Fixture>().cursor = cursor;
            pointer(world, cursor);
        }
        72 => press(world, true),
        74 => pointer(
            world,
            world.resource::<Fixture>().cursor + Vec2::new(40.0, 20.0),
        ),
        76 => press(world, false),
        78 => {
            assert_eq!(
                world.get::<CanvasItem>(sand).unwrap().position,
                DVec2::new(-80.0, 10.0)
            );
            assert_eq!(world.get::<CanvasView>(root).unwrap().center, DVec2::ZERO);
            let editor = world.resource::<Fixture>().editor;
            assert_eq!(
                world
                    .get::<EditableText>(editor)
                    .unwrap()
                    .value()
                    .to_string(),
                "Keep my text"
            );
            let path = world.resource::<Fixture>().path.clone();
            world.spawn(Screenshot::primary_window()).observe(save_to_disk(path)).observe(
                |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                    println!("Canvas pan smoke passed: real pointer picking, zoom, text, click routing, Ctrl-drag, focus loss and Sand dragging in Edit mode.");
                    exit.write(AppExit::Success);
                },
            );
        }
        1800 => panic!("canvas pan smoke timed out"),
        _ => {}
    }
}

fn main() {
    interface_app()
        .insert_resource(WinitSettings::continuous())
        .add_systems(Startup, setup)
        .add_systems(Update, exercise)
        .run();
}
