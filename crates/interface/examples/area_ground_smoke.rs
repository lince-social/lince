use bevy::{
    diagnostic::FrameCount,
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    winit::WinitSettings,
};
use lince_interface::{
    actions::Action,
    area::{AreaShape, InfluenceArea, spawn_area},
    container::BoxRoot,
    edit_mode::EditAction,
    topology::view::View,
};

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    let Some(root) = world
        .query_filtered::<Entity, With<BoxRoot>>()
        .iter(world)
        .next()
    else {
        return;
    };
    if frame == 12 {
        EditAction::Close.apply(world, root);
        for (shape, center, color) in [
            (AreaShape::Square, DVec2::new(-270.0, 0.0), [70, 130, 255]),
            (AreaShape::Circle, DVec2::new(40.0, 0.0), [255, 80, 80]),
            (
                AreaShape::Polygon(vec![
                    [-0.5, -0.5],
                    [0.5, -0.5],
                    [0.5, -0.1],
                    [-0.1, -0.1],
                    [-0.1, 0.5],
                    [-0.5, 0.5],
                    [-0.5, -0.5],
                ]),
                DVec2::new(350.0, 0.0),
                [70, 230, 130],
            ),
        ] {
            let mut area = InfluenceArea::new(shape, center, DVec2::splat(260.0));
            area.color = color;
            area.opacity = 0.3;
            spawn_area(world, root, 1, area).unwrap();
        }
        world.entity_mut(root).insert(View {
            spatial: true,
            position: [0.0, 650.0, 1000.0],
            pitch: -0.55,
            ..default()
        });
    }
    if frame == 90 {
        world.entity_mut(root).insert(View {
            spatial: true,
            position: [850.0, 650.0, 800.0],
            yaw: 0.65,
            pitch: -0.55,
            ..default()
        });
    }
    if frame == 150 {
        world.get_mut::<View>(root).unwrap().spatial = false;
    }
    if matches!(frame, 60 | 120 | 180) {
        let path = format!("/tmp/lince-area-ground-{frame}.png");
        let mut capture = world.spawn(Screenshot::primary_window());
        capture.observe(save_to_disk(path));
        capture.observe(|event: On<ScreenshotCaptured>| {
            let mut colors = [0; 3];
            for y in (80..event.image.height()).step_by(4) {
                for x in (0..event.image.width()).step_by(4) {
                    let pixel = event.image.get_color_at(x, y).unwrap().to_srgba();
                    for (index, (main, a, b)) in [
                        (pixel.red, pixel.green, pixel.blue),
                        (pixel.green, pixel.red, pixel.blue),
                        (pixel.blue, pixel.red, pixel.green),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        if main > a * 1.3 && main > b * 1.3 && main > 0.15 {
                            colors[index] += 1;
                        }
                    }
                }
            }
            assert!(
                colors.into_iter().all(|count| count > 100),
                "All three Area fills must render in normal mode: {colors:?}"
            );
        });
        if frame == 180 {
            capture.observe(
                |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                    exit.write(AppExit::Success);
                },
            );
        }
    }
    assert!(frame < 600, "Area ground smoke timed out");
}

fn main() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _guard = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    lince_interface::app::interface_app()
        .add_plugins(bevy::log::LogPlugin::default())
        .insert_resource(lince_interface::workspace::WorkspaceFile::new(
            directory.path().join("workspace.json"),
        ))
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
