use bevy::{
    diagnostic::FrameCount,
    math::{DQuat, DVec2, DVec3},
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    winit::WinitSettings,
};
use lince_interface::{
    actions::Action,
    area::{AreaShape, AttractionTarget, InfluenceArea},
    area_panel::AreaAction,
    container::BoxRoot,
    edit_mode::EditAction,
    topology::{Spatial, presentation::SceneCamera, view::View},
};

#[derive(Resource)]
struct Target(Entity, Entity);

fn cursor(world: &mut World, position: Vec2) {
    let window = world
        .query_filtered::<Entity, With<Window>>()
        .single(world)
        .unwrap();
    world
        .get_mut::<Window>(window)
        .unwrap()
        .bypass_change_detection()
        .set_cursor_position(Some(position));
    let event = bevy::window::CursorMoved {
        window,
        position,
        delta: None,
    };
    world.write_message(event.clone());
    world.write_message(bevy::window::WindowEvent::CursorMoved(event));
}

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    if frame == 12 {
        let root = world
            .query_filtered::<Entity, With<BoxRoot>>()
            .single(world)
            .unwrap();
        let mut area = InfluenceArea::new(
            AreaShape::Circle,
            DVec2::new(-150.0, -50.0),
            DVec2::splat(240.0),
        );
        area.name = "Spatial destination".into();
        area.depth = 80.0;
        area.target = AttractionTarget::Point([30.0, 20.0]);
        let area = lince_interface::area::spawn_area(world, root, 1, area).unwrap();
        world.entity_mut(area).insert(Spatial {
            elevation: 35.0,
            rotation: DQuat::from_rotation_z(0.35).to_array(),
            depth: Some(80.0),
            ..default()
        });
        world.entity_mut(root).insert(View {
            spatial: true,
            position: [-150.0, 400.0, 500.0],
            pitch: -0.65,
            ..default()
        });
        EditAction::Open.apply(world, root);
        EditAction::Areas.apply(world, root);
        EditAction::Area(AreaAction::Select(area)).apply(world, root);
        world.insert_resource(Target(root, area));
    }
    if matches!(frame, 40 | 50) {
        let Target(root, area) = *world.resource::<Target>();
        let camera = world.resource::<SceneCamera>().0;
        let offset = if frame == 40 {
            DVec3::new(30.0, 0.0, 20.0)
        } else {
            DVec3::new(90.0, 0.0, 50.0)
        };
        let placement = lince_interface::topology::spatial(world, area);
        let point = placement.position(DVec2::from_array(
            world.get::<InfluenceArea>(area).unwrap().center,
        )) + placement.rotation() * offset
            - lince_interface::topology::presentation::origin(world, root);
        let position = world
            .get::<Camera>(camera)
            .unwrap()
            .world_to_viewport(
                world.get::<GlobalTransform>(camera).unwrap(),
                point.as_vec3(),
            )
            .unwrap();
        cursor(world, position);
    }
    if matches!(frame, 45 | 55) {
        let window = world
            .query_filtered::<Entity, With<Window>>()
            .single(world)
            .unwrap();
        let event = bevy::input::mouse::MouseButtonInput {
            window,
            button: MouseButton::Left,
            state: if frame == 45 {
                bevy::input::ButtonState::Pressed
            } else {
                bevy::input::ButtonState::Released
            },
        };
        world.write_message(event);
        world.write_message(bevy::window::WindowEvent::MouseButtonInput(event));
    }
    if frame == 65 {
        let area = world.resource::<Target>().1;
        let saved = world.get::<InfluenceArea>(area).unwrap();
        let offset = saved.target_position() - DVec2::from_array(saved.center);
        assert!(
            (offset - DVec2::new(90.0, 50.0)).length() < 0.01,
            "Actual spatial target drag: {offset:?}"
        );
        assert_eq!(saved.center, [-150.0, -50.0]);
        assert_eq!(saved.depth, 80.0);
        println!("Spatial target pointer drag passed");
        cursor(world, Vec2::new(500.0, 100.0));
    }
    if frame == 70 {
        world
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk("/tmp/lince-area-spatial.png"))
            .observe(
                |_: On<ScreenshotCaptured>, mut exits: MessageWriter<AppExit>| {
                    exits.write(AppExit::Success);
                },
            );
    }
    assert!(frame < 300, "Spatial target smoke timed out");
}

fn main() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _guard = runtime.enter();
    lince_interface::app::interface_app()
        .add_plugins(bevy::log::LogPlugin::default())
        .insert_resource(WinitSettings::continuous())
        .add_systems(
            Startup,
            |mut commands: Commands, mut windows: Query<&mut Window>| {
                windows.single_mut().unwrap().resolution.set(1400.0, 900.0);
                commands.spawn(BoxRoot);
            },
        )
        .add_systems(Update, exercise)
        .run();
}
