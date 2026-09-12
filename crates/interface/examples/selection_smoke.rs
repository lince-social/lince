use bevy::{
    diagnostic::FrameCount,
    input::{ButtonState, mouse::MouseButtonInput},
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    window::{CursorMoved, PrimaryWindow, WindowEvent},
    winit::WinitSettings,
};
use lince_interface::{
    actions::{Action, dispatch},
    app::interface_app,
    canvas::{CanvasItem, CanvasView},
    canvas_selection::{SandGroup, SandSelection},
    container::BoxRoot,
    edit_mode::EditAction,
    icons::IconButton,
    sand_store::{SandKind, spawn_sand},
};

#[derive(Resource)]
struct Fixture {
    root: Entity,
    first: Entity,
    second: Entity,
    start: Vec2,
    end: Vec2,
    before: [DVec2; 2],
    screenshots: usize,
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

fn press(world: &mut World, button: MouseButton, down: bool) {
    let window = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
        .unwrap();
    world.write_message(WindowEvent::MouseButtonInput(MouseButtonInput {
        window,
        button,
        state: if down {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        },
    }));
}

fn center(world: &World, entity: Entity) -> Vec2 {
    world.get::<UiGlobalTransform>(entity).unwrap().translation
        * world
            .get::<ComputedNode>(entity)
            .unwrap()
            .inverse_scale_factor()
}

fn button(world: &mut World, name: &str) -> Vec2 {
    let entity = world
        .query::<(Entity, &IconButton)>()
        .iter(world)
        .find(|(_, icon)| icon.label == name)
        .unwrap()
        .0;
    center(world, entity)
}

fn capture(world: &mut World, name: &str) {
    let path = std::env::args().nth(1).expect("provide screenshot prefix");
    world
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(format!("{path}.{name}.png")))
        .observe(|_: On<ScreenshotCaptured>, mut fixture: ResMut<Fixture>| {
            fixture.screenshots += 1
        });
}

fn setup(world: &mut World) {
    world
        .query::<&mut Window>()
        .single_mut(world)
        .unwrap()
        .resolution
        .set(1000.0, 700.0);
    let root = world.spawn(BoxRoot).id();
    world.insert_resource(Fixture {
        root,
        first: Entity::PLACEHOLDER,
        second: Entity::PLACEHOLDER,
        start: Vec2::ZERO,
        end: Vec2::ZERO,
        before: [DVec2::ZERO; 2],
        screenshots: 0,
    });
}

fn exercise(world: &mut World) {
    let Fixture {
        root,
        first,
        second,
        start,
        end,
        before,
        ..
    } = *world.resource::<Fixture>();
    match world.resource::<FrameCount>().0 {
        4 => {
            let first = spawn_sand(
                world,
                root,
                1,
                SandKind::Square,
                "",
                DVec2::new(-290.0, -30.0),
            );
            let second = spawn_sand(
                world,
                root,
                1,
                SandKind::Square,
                "",
                DVec2::new(-110.0, -30.0),
            );
            for entity in [first, second] {
                world.get_mut::<CanvasItem>(entity).unwrap().size = Vec2::splat(80.0);
            }
            world.resource_mut::<Fixture>().first = first;
            world.resource_mut::<Fixture>().second = second;
            EditAction::Open.apply(world, root);
        }
        12 => {
            let start = center(world, first) - Vec2::splat(50.0);
            let end = center(world, second) + Vec2::splat(50.0);
            world.resource_mut::<Fixture>().start = start;
            world.resource_mut::<Fixture>().end = end;
            world
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(KeyCode::ControlLeft);
            pointer(world, start);
        }
        14 => press(world, MouseButton::Right, true),
        17 => pointer(world, end),
        20 => {
            assert_eq!(world.get::<SandSelection>(root).unwrap().0.len(), 2);
            assert_eq!(world.get::<CanvasView>(root).unwrap().center, DVec2::ZERO);
            capture(world, "rectangle");
        }
        23 => press(world, MouseButton::Right, false),
        26 => {
            world
                .resource_mut::<ButtonInput<KeyCode>>()
                .release(KeyCode::ControlLeft);
            let position = button(world, "Group selected Sands");
            pointer(world, position);
        }
        28 => press(world, MouseButton::Left, true),
        30 => press(world, MouseButton::Left, false),
        34 => {
            assert!(world.get::<SandGroup>(first).is_some());
            assert_eq!(
                world.get::<SandGroup>(first),
                world.get::<SandGroup>(second)
            );
            world.resource_mut::<Fixture>().before = [
                world.get::<CanvasItem>(first).unwrap().position,
                world.get::<CanvasItem>(second).unwrap().position,
            ];
            pointer(world, center(world, first));
        }
        36 => press(world, MouseButton::Left, true),
        39 => pointer(world, center(world, first) + Vec2::new(35.0, 45.0)),
        41 => press(world, MouseButton::Left, false),
        45 => {
            for (entity, original) in [(first, before[0]), (second, before[1])] {
                assert!(
                    (world.get::<CanvasItem>(entity).unwrap().position
                        - original
                        - DVec2::new(35.0, 45.0))
                    .length()
                        < 0.01
                );
            }
            capture(world, "grouped");
            let position = button(world, "Ungroup Sands");
            pointer(world, position);
        }
        47 => press(world, MouseButton::Left, true),
        49 => press(world, MouseButton::Left, false),
        53 => {
            assert!(world.get::<SandGroup>(first).is_none());
            assert!(world.get::<SandGroup>(second).is_none());
            pointer(world, start - Vec2::splat(20.0));
        }
        55 => press(world, MouseButton::Left, true),
        57 => press(world, MouseButton::Left, false),
        61 => {
            assert!(world.get::<SandSelection>(root).unwrap().0.is_empty());
            world.resource_mut::<Fixture>().before = [
                world.get::<CanvasItem>(first).unwrap().position,
                world.get::<CanvasItem>(second).unwrap().position,
            ];
            pointer(world, center(world, first));
        }
        63 => press(world, MouseButton::Left, true),
        66 => pointer(world, center(world, first) + Vec2::new(15.0, 0.0)),
        68 => press(world, MouseButton::Left, false),
        72 => {
            assert_eq!(world.get::<CanvasItem>(second).unwrap().position, before[1]);
            assert!(
                (world.get::<CanvasItem>(first).unwrap().position
                    - before[0]
                    - DVec2::new(15.0, 0.0))
                .length()
                    < 0.01
            );
            dispatch(world, root, lince_interface::actions![EditAction::Close]);
        }
        85 => {
            assert!(world.get::<SandSelection>(root).unwrap().0.is_empty());
            assert_eq!(world.resource::<Fixture>().screenshots, 2);
            println!(
                "Selection smoke passed: real Ctrl-right-drag, retained rectangle, group and ungroup button clicks, group movement, independent movement after ungrouping, and edit-mode cleanup."
            );
            world.write_message(AppExit::Success);
        }
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
