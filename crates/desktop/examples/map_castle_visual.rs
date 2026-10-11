use bevy::{
    camera::{ImageRenderTarget, RenderTarget},
    math::DVec2,
    prelude::*,
    render::{
        render_resource::{PollType, TextureFormat},
        renderer::RenderDevice,
        view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    },
};
use lince_desktop::{container::BoxRoot, map_castle::MapSand};
use std::{path::PathBuf, time::Instant};

#[derive(Resource)]
struct Output(RenderTarget);

#[derive(Resource, Default)]
struct Captured(bool);

fn output(world: &mut World) {
    let target = RenderTarget::Image(ImageRenderTarget {
        handle: world
            .resource_mut::<Assets<Image>>()
            .add(Image::new_target_texture(
                1280,
                960,
                TextureFormat::Bgra8UnormSrgb,
                None,
            )),
        scale_factor: 1.0,
    });
    let scene = world
        .resource::<lince_desktop::topology::presentation::SceneCamera>()
        .0;
    let background = world
        .resource::<lince_desktop::topology::presentation::BackgroundCamera>()
        .0;
    world.entity_mut(scene).insert(target.clone());
    world.entity_mut(background).insert(target.clone());
    world.insert_resource(Output(target));
}

fn step(app: &mut App) {
    app.update();
    app.world()
        .resource::<RenderDevice>()
        .wgpu_device()
        .poll(PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .unwrap();
}

fn settle(app: &mut App, seconds: u64) {
    let started = Instant::now();
    let mut frames = 0;
    while started.elapsed().as_secs() < seconds || frames < 8 {
        step(app);
        frames += 1;
    }
}

fn capture(app: &mut App, path: PathBuf) {
    app.world_mut().resource_mut::<Captured>().0 = false;
    let target = app.world().resource::<Output>().0.clone();
    app.world_mut()
        .spawn(Screenshot(target))
        .observe(save_to_disk(path.clone()))
        .observe(
            |capture: On<ScreenshotCaptured>, mut captured: ResMut<Captured>| {
                let data = capture.image.data.as_ref().unwrap();
                assert!(data.windows(4).any(|pixel| pixel != &data[..4]));
                captured.0 = true;
            },
        );
    let started = Instant::now();
    while !app.world().resource::<Captured>().0 {
        assert!(started.elapsed().as_secs() < 30, "Map capture timed out");
        step(app);
    }
    println!("Captured {}", path.display());
}

#[tokio::main]
async fn main() {
    let directory = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "target/map-visual-verification".into()),
    );
    let detail = std::env::args().any(|argument| argument == "--detail");
    std::fs::create_dir_all(&directory).unwrap();
    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(workspace.path().join("maps")).unwrap();
    std::fs::write(
        workspace.path().join("maps/settings.json"),
        serde_json::to_vec(&lince_map::Settings {
            online: detail,
            ..Default::default()
        })
        .unwrap(),
    )
    .unwrap();
    let mut app = lince_desktop::app::offscreen_interface_app();
    app.insert_resource(lince_desktop::workspace::WorkspaceFile::new(
        workspace.path().join("interface.json"),
    ))
    .init_resource::<Captured>()
    .add_systems(
        Startup,
        |mut commands: Commands, mut windows: Query<&mut Window>| {
            windows.single_mut().unwrap().resolution.set(1280.0, 960.0);
            commands.spawn(BoxRoot);
        },
    )
    .add_systems(PostStartup, output);
    app.finish();
    app.cleanup();
    settle(&mut app, 1);
    let root = app
        .world_mut()
        .query_filtered::<Entity, With<BoxRoot>>()
        .single(app.world())
        .unwrap();
    let map = lince_desktop::map_castle::spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        lince_map::Camera::default(),
        Vec::new(),
        String::new(),
    );
    settle(&mut app, 3);
    capture(&mut app, directory.join("flat-world.png"));
    if detail {
        app.world_mut().get_mut::<MapSand>(map).unwrap().camera = lince_map::Camera {
            center: lince_map::Coordinate {
                latitude: -23.5505,
                longitude: -46.6333,
            },
            zoom: 14.0,
        };
        let started = Instant::now();
        while started.elapsed().as_secs() < 45 {
            step(&mut app);
            let loaded = app
                .world_mut()
                .query::<&Text>()
                .iter(app.world())
                .any(|text| {
                    text.0.starts_with("Zoom 14.0") && text.0.contains("tiles available locally")
                });
            if loaded {
                break;
            }
        }
        settle(&mut app, 3);
        capture(&mut app, directory.join("flat-roads.png"));
    }
}
