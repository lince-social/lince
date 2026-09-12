use bevy::{
    diagnostic::FrameCount,
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
        mouse::MouseButtonInput,
    },
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    window::{CursorMoved, PrimaryWindow, WindowEvent},
    winit::WinitSettings,
};
use lince_interface::{
    actions::dispatch,
    app::interface_app,
    canvas::CanvasItem,
    castle::Castle,
    container::BoxRoot,
    edit_mode::{EditAction, EditMode},
    effect::SendBoxEvent,
    icons::Tooltip,
    inspection::{Inspection, InspectionOverlay},
    sand::{InBox, Square},
    theme::{INK, PAPER, Typography},
};

#[derive(Resource)]
struct Fixture {
    root: Entity,
    group: Entity,
    hidden: Entity,
    overlays: Vec<Entity>,
    captured: bool,
}

fn window(world: &mut World) -> Entity {
    world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
        .unwrap()
}

fn pointer(world: &mut World, position: Vec2) {
    let window = window(world);
    world.write_message(WindowEvent::CursorMoved(CursorMoved {
        window,
        position,
        delta: None,
    }));
}

fn press(world: &mut World, down: bool) {
    let window = window(world);
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

fn escape(world: &mut World, down: bool) {
    let window = window(world);
    let input = KeyboardInput {
        key_code: KeyCode::Escape,
        logical_key: Key::Escape,
        text: None,
        state: if down {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        },
        repeat: false,
        window,
    };
    world.write_message(input.clone());
    world.write_message(WindowEvent::KeyboardInput(input));
}

fn overlays(world: &mut World) -> Vec<Entity> {
    let mut entities: Vec<_> = world
        .query_filtered::<Entity, With<InspectionOverlay>>()
        .iter(world)
        .collect();
    entities.sort();
    entities
}

fn setup(world: &mut World) {
    let root = world.spawn(BoxRoot).id();
    world.insert_resource(Fixture {
        root,
        group: Entity::PLACEHOLDER,
        hidden: Entity::PLACEHOLDER,
        overlays: Vec::new(),
        captured: false,
    });
}

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    let root = world.resource::<Fixture>().root;
    match frame {
        4 => {
            let group = world
                .spawn((
                    Castle,
                    ChildOf(root),
                    CanvasItem {
                        position: DVec2::new(-200.0, -70.0),
                        size: Vec2::new(220.0, 130.0),
                    },
                    Node::default(),
                ))
                .id();
            world.spawn((
                Square,
                ChildOf(group),
                BackgroundColor(PAPER),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(10),
                    top: px(10),
                    width: px(90),
                    height: px(90),
                    border_radius: BorderRadius::all(px(12)),
                    ..default()
                },
            ));
            let hidden = world
                .spawn((
                    Square,
                    InBox(root),
                    ChildOf(root),
                    Visibility::Hidden,
                    BackgroundColor(PAPER),
                    CanvasItem {
                        position: DVec2::new(-200.0, 100.0),
                        size: Vec2::new(200.0, 60.0),
                    },
                ))
                .id();
            let font = world.resource::<Typography>().text(18.0);
            world.spawn((
                Text::new("A hidden reply"),
                font,
                TextColor(INK),
                ChildOf(hidden),
            ));
            world.spawn((
                Square,
                InBox(root),
                ChildOf(group),
                Tooltip("Show the reply".into()),
                SendBoxEvent {
                    box_entity: root,
                    square: hidden,
                },
                lince_interface::sand::button(0),
                BackgroundColor(PAPER),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(120),
                    top: px(10),
                    width: px(90),
                    height: px(90),
                    ..default()
                },
            ));
            world.resource_mut::<Fixture>().group = group;
            world.resource_mut::<Fixture>().hidden = hidden;
            dispatch(world, root, lince_interface::actions![EditAction::Open]);
            pointer(world, Vec2::new(10.0, 610.0));
        }
        12 => {
            assert!(overlays(world).is_empty());
            let state = world.get::<Inspection>(root).unwrap();
            assert!(state.hover && !state.contours && !state.events && !state.hidden);
            pointer(world, Vec2::new(140.0, 230.0));
        }
        18 => {
            assert!(!overlays(world).is_empty());
            let hidden = world.resource::<Fixture>().hidden;
            assert_eq!(world.get::<Visibility>(hidden), Some(&Visibility::Hidden));
            assert!(
                world
                    .query_filtered::<&Text, With<InspectionOverlay>>()
                    .iter(world)
                    .any(|text| text.0 == "Sand Clicked Toggle")
            );
            press(world, true);
        }
        20 => press(world, false),
        22 => pointer(world, Vec2::new(10.0, 610.0)),
        26 => {
            assert!(world.get::<Inspection>(root).unwrap().selected.is_some());
            world.resource_mut::<Fixture>().overlays = overlays(world);
        }
        30 => {
            assert_eq!(overlays(world), world.resource::<Fixture>().overlays);
            let path = std::env::args().nth(1).expect("screenshot path");
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path))
                .observe(|_: On<ScreenshotCaptured>, mut fixture: ResMut<Fixture>| {
                    fixture.captured = true
                });
        }
        34 => escape(world, true),
        36 => escape(world, false),
        40 => {
            assert!(world.get::<EditMode>(root).unwrap().enabled);
            assert!(world.get::<Inspection>(root).unwrap().selected.is_none());
            assert!(overlays(world).is_empty());
            pointer(world, Vec2::new(140.0, 230.0));
        }
        44 => press(world, true),
        46 => press(world, false),
        48 => pointer(world, Vec2::new(10.0, 610.0)),
        50 => press(world, true),
        52 => press(world, false),
        56 => {
            assert!(world.get::<Inspection>(root).unwrap().selected.is_none());
            assert!(overlays(world).is_empty());
            let label = world
                .query::<(Entity, &Text)>()
                .iter(world)
                .find(|(_, text)| text.0 == "Hidden: off")
                .unwrap()
                .0;
            let position = world.get::<UiGlobalTransform>(label).unwrap().translation
                * world
                    .get::<ComputedNode>(label)
                    .unwrap()
                    .inverse_scale_factor();
            pointer(world, position);
        }
        58 => press(world, true),
        60 => press(world, false),
        64 => {
            assert!(world.get::<Inspection>(root).unwrap().hidden);
            assert!(!overlays(world).is_empty());
            let hidden = world.resource::<Fixture>().hidden;
            assert_eq!(world.get::<Visibility>(hidden), Some(&Visibility::Hidden));
            dispatch(world, root, lince_interface::actions![EditAction::Close]);
        }
        70 => {
            assert!(overlays(world).is_empty());
            assert!(world.resource::<Fixture>().captured);
            println!(
                "Inspection smoke passed: hover, group relations, real event labels, hidden previews without activation, stable idle drawings, click selection, Escape, background deselection and cleanup."
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
