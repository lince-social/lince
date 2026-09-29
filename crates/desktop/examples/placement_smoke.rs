use bevy::{
    diagnostic::FrameCount,
    input::{ButtonState, mouse::MouseButtonInput},
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
    window::{CursorMoved, PrimaryWindow, WindowEvent},
    winit::WinitSettings,
};
use lince_desktop::{
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
    if let Some(bounds) = lince_desktop::topology::presentation::bounds(world, entity) {
        return bounds.center();
    }
    world.get::<UiGlobalTransform>(entity).unwrap().translation
        * world
            .get::<ComputedNode>(entity)
            .unwrap()
            .inverse_scale_factor()
}

fn label(world: &mut World, name: &str) -> Vec2 {
    let entity = world
        .query::<(Entity, &lince_desktop::icons::IconButton)>()
        .iter(world)
        .find(|(_, icon)| icon.label == name)
        .unwrap()
        .0;
    center(world, entity)
}

fn check_menu(world: &mut World, sand: Entity) {
    let control = world
        .query::<(Entity, &lince_desktop::icons::IconButton)>()
        .iter(world)
        .find(|(_, icon)| icon.label == "Pin to screen")
        .unwrap()
        .0;
    let panel = world.get::<ChildOf>(control).unwrap().parent();
    assert_eq!(
        world.get::<ChildOf>(panel).unwrap().parent(),
        world.resource::<Fixture>().root
    );
    let bounds = |entity| {
        if let Some(bounds) = lince_desktop::topology::presentation::bounds(world, entity) {
            return bounds;
        }
        let node = world.get::<ComputedNode>(entity).unwrap();
        Rect::from_center_size(
            center(world, entity),
            node.size() * node.inverse_scale_factor(),
        )
    };
    let sand = bounds(sand);
    let menu = bounds(panel);
    assert!(
        menu.max.x <= sand.min.x - 7.5
            || menu.min.x >= sand.max.x + 7.5
            || menu.max.y <= sand.min.y - 7.5
            || menu.min.y >= sand.max.y + 7.5,
        "the menu must float outside the Sand: {menu:?}, {sand:?}"
    );
    assert_eq!(world.get::<Visibility>(panel), Some(&Visibility::Inherited));
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
            dispatch(world, root, lince_desktop::actions![EditAction::Open]);
        }
        10 => pointer(world, center(world, sand)),
        12 => press(world, MouseButton::Left, true),
        14 => press(world, MouseButton::Left, false),
        18 => {
            assert_eq!(world.get::<Inspection>(root).unwrap().selected, Some(sand));
            check_menu(world, sand);
            world.resource_mut::<Fixture>().position = center(world, sand);
            let position = label(world, "Pin to screen");
            pointer(world, position);
        }
        20 => {
            assert!(
                world
                    .query::<&Text>()
                    .iter(world)
                    .any(|text| text.0 == "Pin to screen")
            );
            if let Some(path) = std::env::args().nth(1) {
                world
                    .spawn(Screenshot::primary_window())
                    .observe(save_to_disk(path));
            }
            press(world, MouseButton::Left, true);
        }
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
            dispatch(world, root, lince_desktop::actions![EditAction::Close]);
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
            dispatch(world, root, lince_desktop::actions![EditAction::Open]);
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
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _runtime = runtime.enter();
    interface_app()
        .insert_resource(WinitSettings::continuous())
        .add_systems(Startup, setup)
        .add_systems(Update, exercise)
        .run();
}
