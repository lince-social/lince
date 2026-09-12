use bevy::{
    diagnostic::FrameCount,
    input::{ButtonState, mouse::MouseButtonInput},
    math::DVec2,
    prelude::*,
    window::{CursorMoved, PrimaryWindow, WindowEvent},
    winit::WinitSettings,
};
use lince_interface::{
    actions::{Action, dispatch},
    app::interface_app,
    canvas::{CanvasItem, CanvasView},
    container::BoxRoot,
    edit_mode::EditAction,
    inspection::Inspection,
    sand::Square,
    sand_placement::{Pinned, PlacementAction},
};

#[derive(Resource)]
struct Fixture {
    root: Entity,
    sand: Entity,
    position: Vec2,
    anchor: [f64; 2],
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

fn label(world: &mut World, name: &str) -> Vec2 {
    let entity = world
        .query::<(Entity, &Text)>()
        .iter(world)
        .find(|(_, text)| text.0 == name)
        .unwrap()
        .0;
    center(world, entity)
}

fn setup(world: &mut World) {
    let root = world.spawn(BoxRoot).id();
    world.insert_resource(Fixture {
        root,
        sand: Entity::PLACEHOLDER,
        position: Vec2::ZERO,
        anchor: [0.0; 2],
    });
}

fn exercise(world: &mut World) {
    let root = world.resource::<Fixture>().root;
    let sand = world.resource::<Fixture>().sand;
    match world.resource::<FrameCount>().0 {
        4 => {
            let sand = world
                .spawn((
                    Square,
                    CanvasItem {
                        position: DVec2::new(-130.0, -20.0),
                        size: Vec2::splat(100.0),
                    },
                    ChildOf(root),
                    BackgroundColor(Color::srgb(0.5, 0.2, 0.8)),
                ))
                .id();
            world.resource_mut::<Fixture>().sand = sand;
            dispatch(world, root, lince_interface::actions![EditAction::Open]);
        }
        10 => pointer(world, center(world, sand)),
        12 => press(world, MouseButton::Left, true),
        14 => press(world, MouseButton::Left, false),
        18 => {
            assert_eq!(world.get::<Inspection>(root).unwrap().selected, Some(sand));
            world.resource_mut::<Fixture>().position = center(world, sand);
            let position = label(world, "Pin to screen");
            pointer(world, position);
        }
        20 => press(world, MouseButton::Left, true),
        22 => press(world, MouseButton::Left, false),
        26 => {
            assert!(world.get::<Pinned>(sand).is_some());
            assert!(center(world, sand).distance(world.resource::<Fixture>().position) < 0.01);
            let mut view = world.get_mut::<CanvasView>(root).unwrap();
            view.center = DVec2::splat(1e9);
            view.zoom = 2.0;
        }
        30 => {
            assert!(center(world, sand).distance(world.resource::<Fixture>().position) < 0.01);
            assert_eq!(world.get::<UiTransform>(sand).unwrap().scale, Vec2::ONE);
            dispatch(world, root, lince_interface::actions![EditAction::Close]);
            pointer(world, center(world, sand));
        }
        34 => press(world, MouseButton::Right, true),
        36 => pointer(
            world,
            world.resource::<Fixture>().position + Vec2::new(40.0, 30.0),
        ),
        38 => press(world, MouseButton::Right, false),
        42 => {
            assert!(
                center(world, sand)
                    .distance(world.resource::<Fixture>().position + Vec2::new(40.0, 30.0))
                    < 0.01
            );
            world.resource_mut::<Fixture>().position = center(world, sand);
            world.resource_mut::<Fixture>().anchor = world.get::<Pinned>(sand).unwrap().anchor;
            dispatch(world, root, lince_interface::actions![EditAction::Open]);
            world.get_mut::<Inspection>(root).unwrap().selected = Some(sand);
        }
        48 => {
            let position = label(world, "Unpin from screen");
            pointer(world, position);
        }
        50 => press(world, MouseButton::Left, true),
        52 => press(world, MouseButton::Left, false),
        56 => {
            assert!(world.get::<Pinned>(sand).is_none());
            assert!(center(world, sand).distance(world.resource::<Fixture>().position) < 0.01);
            let other = world
                .spawn((
                    Square,
                    CanvasItem {
                        position: world.get::<CanvasItem>(sand).unwrap().position,
                        size: Vec2::splat(100.0),
                    },
                    ChildOf(root),
                ))
                .id();
            world.resource_mut::<Fixture>().sand = other;
            world.get_mut::<Inspection>(root).unwrap().selected = Some(other);
            PlacementAction::Back.apply(world, other);
            assert_eq!(world.get::<ZIndex>(other).unwrap().0, 0);
            assert_eq!(world.get::<ZIndex>(sand).unwrap().0, 1);
        }
        62 => {
            let position = label(world, "Bring to front");
            pointer(world, position);
        }
        64 => press(world, MouseButton::Left, true),
        66 => press(world, MouseButton::Left, false),
        70 => {
            assert_eq!(world.get::<ZIndex>(sand).unwrap().0, 1);
            println!(
                "Placement smoke passed: real menu clicks, pinning, camera independence, right dragging outside Edit mode, unpinning without position jump, and layer controls."
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
