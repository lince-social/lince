use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    window::{FileDragAndDrop, PrimaryWindow},
    winit::WinitSettings,
};
use lince_desktop::{
    actions::ActionButton,
    container::BoxRoot,
    icons::Tooltip,
    media_sand::MediaSand,
    workspace::{WorkspaceFile, Workspaces},
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Resource)]
struct Exercise {
    directory: PathBuf,
    started: Instant,
    delay: Instant,
    index: usize,
    stage: u8,
}

const FILES: [(&str, &str); 4] = [
    ("picture.png", "Image Sand"),
    ("sample.pdf", "Document Viewer Castle"),
    ("sample.epub", "Document Viewer Castle"),
    ("scene.gltf", "3D model"),
];

fn capture(world: &mut World, name: &str, exit: bool) {
    let path = world.resource::<Exercise>().directory.join(name);
    let mut entity = world.spawn(Screenshot::primary_window());
    entity.observe(save_to_disk(path));
    if name == "drop-3-hover.png" || name == "drop-3-accepted.png" {
        entity.observe(|event: On<ScreenshotCaptured>| {
            use bevy::render::render_resource::TextureFormat;
            let blue_first = matches!(
                event.image.texture_descriptor.format,
                TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb
            );
            let width = event.image.width() as usize;
            let red = event
                .image
                .data
                .as_ref()
                .unwrap()
                .chunks_exact(4)
                .enumerate()
                .filter(|(index, pixel)| {
                    let (x, y) = (index % width, index / width);
                    let (red, green, blue) = if blue_first {
                        (pixel[2], pixel[1], pixel[0])
                    } else {
                        (pixel[0], pixel[1], pixel[2])
                    };
                    (950..1350).contains(&x)
                        && (250..650).contains(&y)
                        && red > 50
                        && f32::from(red) > f32::from(green) * 1.5
                        && f32::from(red) > f32::from(blue) * 1.2
                })
                .count();
            assert!(
                red > 1000,
                "The model must be visibly rendered at the drop point: {red}"
            );
        });
    }
    if exit {
        entity.observe(
            |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                exit.write(AppExit::Success);
            },
        );
    }
}

fn exercise(world: &mut World) {
    assert!(
        world.resource::<Exercise>().started.elapsed() < Duration::from_secs(120),
        "Drop smoke timed out"
    );
    let Some(root) = world
        .query_filtered::<Entity, (With<BoxRoot>, With<Workspaces>)>()
        .iter(world)
        .next()
    else {
        return;
    };
    let Some(window) = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .iter(world)
        .next()
    else {
        return;
    };
    world
        .get_mut::<Window>(window)
        .unwrap()
        .set_cursor_position(Some(Vec2::new(1080.0, 420.0)));
    let test = world.resource::<Exercise>();
    let (index, stage, delay) = (test.index, test.stage, test.delay);
    let path = test.directory.join(FILES[index].0);
    let (name, kind) = FILES[index];
    if stage == 0 && delay.elapsed() > Duration::from_millis(500) {
        world
            .get_mut::<lince_desktop::canvas::CanvasView>(root)
            .unwrap()
            .zoom = 0.65;
        world.write_message(FileDragAndDrop::HoveredFile {
            window,
            path_buf: path,
        });
        let mut test = world.resource_mut::<Exercise>();
        test.stage = 1;
        test.delay = Instant::now();
    } else if stage == 1 && delay.elapsed() > Duration::from_secs(4) {
        if index == 3 {
            use lince_desktop::topology::assets::{Bounds, CenteredAsset, ImportedAsset, Ready};
            let (owner, bounds, asset) = world
                .query_filtered::<(Entity, &Bounds, &ImportedAsset), (With<CenteredAsset>, With<Ready>)>()
                .single(world)
                .unwrap();
            assert!((bounds.min + bounds.max).length() < 0.001);
            assert!(((bounds.max - bounds.min).max_element() * asset.scale - 300.0).abs() < 0.001);
            assert!(
                world
                    .get::<lince_desktop::topology::physics::MeshCollider>(owner)
                    .unwrap()
                    .ray_distance(
                        bevy::math::DVec3::new(0.0, 1000.0, -20.0),
                        bevy::math::DVec3::NEG_Y
                    )
                    .is_some()
            );
            let mesh = lince_desktop::topology::assets::mesh_parts(world, owner).unwrap()[0].0;
            assert!(world.get::<InheritedVisibility>(mesh).unwrap().get());
            let actual_center = world
                .get::<GlobalTransform>(mesh)
                .unwrap()
                .transform_point(Vec3::new(0.5, 0.0, 0.5));
            let expected_center = (lince_desktop::topology::position(world, owner).unwrap()
                - lince_desktop::topology::presentation::origin(world, root))
            .as_vec3();
            assert!(actual_center.distance(expected_center) < 0.01);
            let material = world.get::<MeshMaterial3d<StandardMaterial>>(mesh).unwrap();
            assert!(
                (world
                    .resource::<Assets<StandardMaterial>>()
                    .get(&material.0)
                    .unwrap()
                    .base_color
                    .alpha()
                    - 0.45)
                    .abs()
                    < 0.001
            );
        }
        capture(world, &format!("drop-{index}-hover.png"), false);
        let path = world.resource::<Exercise>().directory.join(name);
        world.write_message(FileDragAndDrop::DroppedFile {
            window,
            path_buf: path,
        });
        world.write_message(FileDragAndDrop::HoveredFileCanceled { window });
        let mut test = world.resource_mut::<Exercise>();
        test.stage = 2;
        test.delay = Instant::now();
    } else if stage == 2 {
        let title = format!("Display as {kind}");
        let action = world
            .query::<(&Tooltip, &ActionButton)>()
            .iter(world)
            .find(|(tip, action)| tip.0 == title && action.target == root)
            .map(|(_, action)| action.actions.clone());
        let Some(action) = action else {
            assert!(
                delay.elapsed() < Duration::from_secs(15),
                "The dropped file must offer its Sand choice"
            );
            return;
        };
        action.run(world, root);
        let mut test = world.resource_mut::<Exercise>();
        test.stage = 3;
        test.delay = Instant::now();
    } else if stage == 3 && delay.elapsed() > Duration::from_secs(2) {
        assert_eq!(
            world
                .query::<&lince_desktop::workspace::RecordPlacement>()
                .iter(world)
                .count(),
            0
        );
        capture(
            world,
            &format!("drop-{index}-accepted.png"),
            index == FILES.len() - 1,
        );
        if index == FILES.len() - 1 {
            assert_eq!(world.query::<&MediaSand>().iter(world).count(), 1);
            assert_eq!(
                world
                    .query::<&lince_desktop::document_viewer::DocumentViewer>()
                    .iter(world)
                    .count(),
                2
            );
            assert_eq!(
                world
                    .query::<&lince_desktop::topology::assets::ImportedAsset>()
                    .iter(world)
                    .count(),
                1
            );
            let owner = world
                .query_filtered::<Entity, With<lince_desktop::topology::assets::ImportedAsset>>()
                .single(world)
                .unwrap();
            let mesh = lince_desktop::topology::assets::mesh_parts(world, owner).unwrap()[0].0;
            let material = world.get::<MeshMaterial3d<StandardMaterial>>(mesh).unwrap();
            assert_eq!(
                world
                    .resource::<Assets<StandardMaterial>>()
                    .get(&material.0)
                    .unwrap()
                    .base_color
                    .alpha(),
                1.0
            );
            world.resource_mut::<Exercise>().stage = 4;
        } else {
            world
                .query::<&mut lince_desktop::canvas::CanvasItem>()
                .iter_mut(world)
                .for_each(|mut item| item.position.x -= 650.0);
            let mut test = world.resource_mut::<Exercise>();
            test.index += 1;
            test.stage = 0;
        }
    }
}

#[tokio::main]
async fn main() {
    let directory = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("provide the generated PDF/EPUB fixture directory"),
    );
    image::RgbaImage::from_fn(240, 160, |x, y| image::Rgba([x as u8, y as u8, 180, 255]))
        .save(directory.join("picture.png"))
        .unwrap();
    let mut scene: serde_json::Value =
        serde_json::from_slice(include_bytes!("../fixtures/open_scene.gltf")).unwrap();
    scene["materials"] = serde_json::json!([{ "doubleSided": true, "extensions": { "KHR_materials_unlit": {} }, "pbrMetallicRoughness": { "baseColorFactor": [0.8, 0.1, 0.2, 1.0] } }]);
    scene["extensionsUsed"] = serde_json::json!(["KHR_materials_unlit"]);
    scene["meshes"][0]["primitives"][0]["material"] = 0.into();
    std::fs::write(
        directory.join("scene.gltf"),
        serde_json::to_vec(&scene).unwrap(),
    )
    .unwrap();
    let mut app = lince_desktop::app::interface_app();
    app.insert_resource(WorkspaceFile::new(directory.join("drop-workspace.json")))
        .insert_resource(WinitSettings::continuous())
        .insert_resource(Exercise {
            directory,
            started: Instant::now(),
            delay: Instant::now(),
            index: 0,
            stage: 0,
        })
        .add_systems(
            Startup,
            |mut commands: Commands, mut windows: Query<&mut Window, With<PrimaryWindow>>| {
                commands.spawn(BoxRoot);
                if let Ok(mut window) = windows.single_mut() {
                    window.resolution.set(1640.0, 1020.0);
                }
            },
        )
        .add_systems(
            Update,
            exercise.after(lince_desktop::workspace::PrepareWorkspaces),
        )
        .run();
}
