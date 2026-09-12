use bevy::{
    diagnostic::FrameCount,
    input::{ButtonState, mouse::MouseButtonInput},
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    ui_widgets::Activate,
    window::{CursorMoved, PrimaryWindow, WindowEvent},
    winit::WinitSettings,
};
use lince_interface::{
    app::interface_app,
    area::{AreaForces, InfluenceArea, Property, ShapeKind},
    area_panel::{AreaAction, AreaEditor},
    container::BoxRoot,
    edit_mode::{EditAction, EditControl},
};

#[derive(Resource)]
struct CapturePath(String);

fn activate(world: &mut World, action: EditAction) {
    let entity = world
        .query::<(Entity, &EditControl)>()
        .iter(world)
        .find(|(_, control)| control.action == action)
        .unwrap()
        .0;
    world.trigger(Activate { entity });
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
    let window = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
        .unwrap();
    let position = match frame {
        20 => Some(Vec2::new(70.0, 150.0)),
        24 => Some(Vec2::new(330.0, 410.0)),
        36 => Some(Vec2::new(60.0, 500.0)),
        41..=45 => Some(Vec2::new(
            60.0 + (frame - 40) as f32 * 40.0,
            500.0 - (frame - 40) as f32 * 2.0,
        )),
        46..=49 => Some(Vec2::new(
            260.0 - (frame - 45) as f32 * 27.5,
            490.0 + (frame - 45) as f32 * 40.0,
        )),
        126 => Some(Vec2::new(200.0, 100.0)),
        66 => Some(Vec2::new(90.0, 170.0)),
        69 => Some(Vec2::new(130.0, 210.0)),
        84 => Some(Vec2::new(111.0, 300.0)),
        87 => Some(Vec2::new(91.0, 300.0)),
        _ => None,
    };
    if let Some(position) = position {
        world.write_message(WindowEvent::CursorMoved(CursorMoved {
            window,
            position,
            delta: None,
        }));
    }
    let state = match frame {
        22 | 38 | 50 | 68 | 86 => Some(ButtonState::Pressed),
        26 | 40 | 52 | 70 | 88 => Some(ButtonState::Released),
        _ => None,
    };
    if let Some(state) = state {
        world.write_message(WindowEvent::MouseButtonInput(MouseButtonInput {
            window,
            button: MouseButton::Left,
            state,
        }));
    }
    if (92..=102).contains(&frame) {
        world
            .get_mut::<lince_interface::canvas::CanvasView>(root)
            .unwrap()
            .center += bevy::math::DVec2::new(1.25, 0.75);
    }
    match frame {
        47 | 98 => {
            let path = format!("{}.{}.png", world.resource::<CapturePath>().0, frame);
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path));
        }
        12 => activate(world, EditAction::Toggle),
        14 => activate(world, EditAction::Areas),
        16 => activate(world, EditAction::Area(AreaAction::Draw(ShapeKind::Square))),
        30 => {
            assert_eq!(world.query::<&InfluenceArea>().iter(world).count(), 1);
            let area = world.query::<&InfluenceArea>().single(world).unwrap();
            assert_eq!(area.size, [260.0; 2]);
            assert_eq!(area.shape.kind(), ShapeKind::Square);
            activate(world, EditAction::Area(AreaAction::Draw(ShapeKind::Drawn)));
        }
        62 => {
            assert_eq!(world.query::<&InfluenceArea>().iter(world).count(), 2);
            assert!(
                world
                    .query::<&InfluenceArea>()
                    .iter(world)
                    .any(|area| area.shape.kind() == ShapeKind::Drawn)
            );
            activate(world, EditAction::Area(AreaAction::Add(ShapeKind::Circle)));
        }
        74 => {
            let selected = world.get::<AreaEditor>(root).unwrap().selected.unwrap();
            assert_eq!(
                world.get::<InfluenceArea>(selected).unwrap().shape.kind(),
                ShapeKind::Square
            );
            assert_eq!(
                world.get::<InfluenceArea>(selected).unwrap().center,
                [-310.0, -80.0]
            );
            activate(world, EditAction::Area(AreaAction::AddRule));
        }
        78 => activate(
            world,
            EditAction::Area(AreaAction::Property(0, Property::Quantity)),
        ),
        82 => {
            let selected = world.get::<AreaEditor>(root).unwrap().selected.unwrap();
            let area = world.get::<InfluenceArea>(selected).unwrap().clone();
            let position =
                bevy::math::DVec2::from_array(area.center) + bevy::math::DVec2::new(80.0, 0.0);
            let sand = lince_interface::sand_store::spawn_sand(
                world,
                root,
                1,
                lince_interface::sand_store::SandKind::Text,
                "Matching Record",
                position,
            );
            world
                .get_mut::<lince_interface::canvas::CanvasItem>(sand)
                .unwrap()
                .size = Vec2::new(125.0, 65.0);
            world
                .entity_mut(sand)
                .insert(lince_interface::area::RecordProperties(
                    serde_json::json!({"quantity":0}),
                ));
        }
        90 => {
            assert_eq!(world.query::<&InfluenceArea>().iter(world).count(), 3);
            assert!(
                world
                    .query::<&AreaForces>()
                    .iter(world)
                    .any(|forces| !forces.0.is_empty())
            );
            let selected = world.get::<AreaEditor>(root).unwrap().selected.unwrap();
            assert_eq!(
                world.get::<InfluenceArea>(selected).unwrap().size,
                [280.0; 2]
            );
            let buttons: Vec<_> = world
                .query::<(
                    &lince_interface::icons::IconButton,
                    &lince_interface::actions::ActionButton,
                )>()
                .iter(world)
                .filter(|(_, button)| button.target == selected)
                .map(|(icon, _)| icon.label.clone())
                .collect();
            assert_eq!(buttons, vec!["Delete"]);
            let path = world.resource::<CapturePath>().0.clone();
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path));
        }
        106 => {
            let selected = world.get::<AreaEditor>(root).unwrap().selected.unwrap();
            let button = world
                .query::<(
                    Entity,
                    &lince_interface::icons::IconButton,
                    &lince_interface::actions::ActionButton,
                )>()
                .iter(world)
                .find(|(_, icon, button)| icon.label == "Delete" && button.target == selected)
                .unwrap()
                .0;
            world.trigger(Activate { entity: button });
        }
        110 => {
            assert_eq!(world.query::<&InfluenceArea>().iter(world).count(), 2);
            activate(world, EditAction::General);
            let sand = world
                .query_filtered::<Entity, With<lince_interface::sand_store::StoredSand>>()
                .single(world)
                .unwrap();
            world
                .get_mut::<lince_interface::inspection::Inspection>(root)
                .unwrap()
                .selected = Some(sand);
        }
        112 => {
            let sand = world
                .query_filtered::<Entity, With<lince_interface::sand_store::StoredSand>>()
                .single(world)
                .unwrap();
            let button = world
                .query::<(
                    Entity,
                    &lince_interface::icons::IconButton,
                    &lince_interface::actions::ActionButton,
                )>()
                .iter(world)
                .find(|(_, icon, button)| icon.label == "Delete" && button.target == sand)
                .unwrap()
                .0;
            world.trigger(Activate { entity: button });
        }
        114 => assert!(
            world
                .query::<&lince_interface::sand_store::StoredSand>()
                .iter(world)
                .next()
                .is_none()
        ),
        118 => {
            let button = world
                .query::<(Entity, &EditControl)>()
                .iter(world)
                .find(|(_, control)| control.action == EditAction::General)
                .unwrap()
                .0;
            let position = world.get::<UiGlobalTransform>(button).unwrap().translation
                * world
                    .get::<ComputedNode>(button)
                    .unwrap()
                    .inverse_scale_factor();
            world.write_message(WindowEvent::CursorMoved(CursorMoved {
                window,
                position,
                delta: None,
            }));
        }
        124 | 132 => {
            let button = world
                .query::<(Entity, &EditControl)>()
                .iter(world)
                .find(|(_, control)| control.action == EditAction::General)
                .unwrap()
                .0;
            let tabs = world.get::<ChildOf>(button).unwrap().parent();
            assert_eq!(
                world.get::<Node>(tabs).unwrap().max_height,
                if frame == 124 { Val::Auto } else { px(40) }
            );
            let path = format!(
                "{}.general-{}.png",
                world.resource::<CapturePath>().0,
                frame
            );
            let mut screenshot = world.spawn(Screenshot::primary_window());
            screenshot.observe(save_to_disk(path));
            if frame == 132 {
                screenshot.observe(|_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                    println!("Areas smoke passed: continuous freehand drawing, area movement and resizing, camera motion, Delete, and General tabs expanding and collapsing.");
                    exit.write(AppExit::Success);
                });
            }
        }
        1200 => panic!("Areas smoke timed out"),
        _ => {}
    }
}

fn main() {
    interface_app()
        .insert_resource(CapturePath(
            std::env::args().nth(1).expect("provide screenshot path"),
        ))
        .insert_resource(WinitSettings::continuous())
        .add_systems(
            Startup,
            |mut commands: Commands, mut windows: Query<&mut Window>| {
                windows.single_mut().unwrap().resolution.set(1100.0, 800.0);
                commands.spawn(BoxRoot);
            },
        )
        .add_systems(Update, exercise)
        .add_systems(
            Last,
            (|world: &mut World| {
                if world.resource::<FrameCount>().0 < 16 {
                    return;
                }
                for mode in world
                    .query::<&lince_interface::edit_mode::EditMode>()
                    .iter(world)
                {
                    if mode.enabled {
                        assert!(
                            world.get::<InheritedVisibility>(mode.panel).unwrap().get(),
                            "Edit panel vanished during interaction"
                        );
                    }
                }
            })
            .after(lince_interface::castle::PresentCastles),
        )
        .run();
}
