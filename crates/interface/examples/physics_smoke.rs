use bevy::{
    diagnostic::FrameCount,
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    time::TimeUpdateStrategy,
    ui_widgets::Activate,
    winit::WinitSettings,
};
use lince_interface::{
    app::interface_app,
    area::{AreaShape, InfluenceArea, Property, PropertyRule, RecordProperties},
    canvas::CanvasItem,
    container::BoxRoot,
    edit_mode::{EditAction, EditControl},
    sand_store::{SandKind, spawn_sand},
    workspace::{WorkspaceFile, Workspaces},
};
use std::{path::PathBuf, time::Duration};

#[derive(Resource)]
struct Exercise {
    directory: PathBuf,
    sand: Option<Entity>,
    paused: DVec2,
}

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
    match frame {
        12 => {
            let mut area = InfluenceArea::new(
                AreaShape::Square,
                DVec2::new(-220.0, 0.0),
                DVec2::splat(500.0),
            );
            area.name = "Pull matching work".into();
            area.rules.push(PropertyRule {
                property: Property::Quantity,
                value: "-3".into(),
            });
            area.strength = 400.0;
            lince_interface::area::spawn_area(world, root, 1, area).unwrap();
            let sand = spawn_sand(
                world,
                root,
                1,
                SandKind::Text,
                "Moving work",
                DVec2::new(-20.0, 0.0),
            );
            world.get_mut::<CanvasItem>(sand).unwrap().size = Vec2::new(155.0, 90.0);
            world
                .entity_mut(sand)
                .insert(RecordProperties(serde_json::json!({"quantity":-3})));
            world.resource_mut::<Exercise>().sand = Some(sand);
            activate(world, EditAction::Toggle);
        }
        16 => {
            assert!(!lince_interface::workspace_config::enabled(world, root, 1));
            let path = lince_interface::workspace_config::path(world, 1).unwrap();
            assert!(
                std::fs::read_to_string(path)
                    .unwrap()
                    .contains("enabled = false")
            );
            activate(world, EditAction::TogglePhysics);
            world.insert_resource(lince_interface::theme::idle_settings());
        }
        100 => {
            let sand = world.resource::<Exercise>().sand.unwrap();
            assert!(world.get::<CanvasItem>(sand).unwrap().position.x < -100.0);
            let path = lince_interface::workspace_config::path(world, 1).unwrap();
            assert!(
                std::fs::read_to_string(path)
                    .unwrap()
                    .contains("enabled = true")
            );
            world.insert_resource(WinitSettings::continuous());
            activate(world, EditAction::TogglePhysics);
        }
        102 => {
            let sand = world.resource::<Exercise>().sand.unwrap();
            world.resource_mut::<Exercise>().paused =
                world.get::<CanvasItem>(sand).unwrap().position;
            assert!(!lince_interface::workspace_config::enabled(world, root, 1));
        }
        120 => {
            let state = world.resource::<Exercise>();
            assert_eq!(
                world
                    .get::<CanvasItem>(state.sand.unwrap())
                    .unwrap()
                    .position,
                state.paused
            );
            activate(world, EditAction::CreateWorkspace);
        }
        124 => {
            let active = world.get::<Workspaces>(root).unwrap().active;
            assert_ne!(active, 1);
            assert!(!lince_interface::workspace_config::enabled(
                world, root, active
            ));
            assert!(
                lince_interface::workspace_config::path(world, active)
                    .unwrap()
                    .is_file()
            );
            activate(world, EditAction::SwitchWorkspace(1));
        }
        128 => {
            activate(world, EditAction::TogglePhysics);
            world.insert_resource(lince_interface::theme::idle_settings());
        }
        140 => {
            let state = world.resource::<Exercise>();
            assert!(
                world
                    .get::<CanvasItem>(state.sand.unwrap())
                    .unwrap()
                    .position
                    .x
                    < state.paused.x
            );
            let path = state.directory.join("physics.png");
            world.spawn(Screenshot::primary_window()).observe(save_to_disk(path))
                .observe(|capture: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                    let pixels = capture.image.data.as_ref().unwrap();
                    assert!(pixels.chunks_exact(4).any(|pixel| pixel[0] > 40 && pixel[1] > 40 && pixel[2] > 40), "The native window did not render");
                    println!("Physics smoke passed: per-workspace TOML, toggle, idle-window movement, pause and resume.");
                    exit.write(AppExit::Success);
                });
        }
        1200 => panic!("Physics smoke timed out"),
        _ => {}
    }
}

fn main() {
    let directory = PathBuf::from(std::env::args().nth(1).expect("provide output directory"));
    std::fs::create_dir_all(&directory).unwrap();
    interface_app()
        .add_plugins(bevy::log::LogPlugin::default())
        .insert_resource(WorkspaceFile::new(directory.join("interface.json")))
        .insert_resource(Exercise {
            directory,
            sand: None,
            paused: DVec2::ZERO,
        })
        .insert_resource(WinitSettings::continuous())
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )))
        .add_systems(
            Startup,
            |mut commands: Commands, mut windows: Query<&mut Window>| {
                windows.single_mut().unwrap().resolution.set(1100.0, 800.0);
                commands.spawn(BoxRoot);
            },
        )
        .add_systems(
            Update,
            exercise.before(lince_interface::physics::SimulateWorkspaces),
        )
        .run();
}
