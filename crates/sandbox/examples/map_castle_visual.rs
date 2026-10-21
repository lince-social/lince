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
use lince_sandbox::{container::BoxRoot, map_castle::MapSand};
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
        .resource::<lince_sandbox::topology::presentation::SceneCamera>()
        .0;
    let background = world
        .resource::<lince_sandbox::topology::presentation::BackgroundCamera>()
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

fn activate(world: &mut World, caption: &str) {
    let label = world
        .query::<(Entity, &Text)>()
        .iter(world)
        .find(|(entity, text)| {
            text.0 == caption
                && world.get::<ChildOf>(*entity).is_some_and(|parent| {
                    world
                        .get::<bevy::ui_widgets::Button>(parent.parent())
                        .is_some()
                })
        })
        .unwrap_or_else(|| panic!("Missing control: {caption}"))
        .0;
    let entity = world.get::<ChildOf>(label).unwrap().parent();
    world.trigger(bevy::ui_widgets::Activate { entity });
    world.flush();
}

fn summary(values: &[u128]) -> serde_json::Value {
    let mut values = values.to_vec();
    values.sort_unstable();
    let middle = values.len() / 2;
    let median = if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) / 2
    } else {
        values[middle]
    };
    serde_json::json!({"samples":values.len(),"median_us":median,"p95_us":values[(values.len()*95).div_ceil(100)-1]})
}

#[tokio::main]
async fn main() {
    let directory = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "target/map-visual-verification".into()),
    );
    let detail = std::env::args().any(|argument| argument == "--detail");
    let verify = std::env::args().any(|argument| argument == "--verify");
    let source = std::env::args().find_map(|argument| {
        argument
            .strip_prefix("--source-settings=")
            .map(str::to_owned)
    });
    std::fs::create_dir_all(&directory).unwrap();
    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(workspace.path().join("maps")).unwrap();
    if verify {
        use tokio::{
            io::{AsyncReadExt, AsyncWriteExt},
            net::TcpListener,
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        lince_map::navigation::Settings {
            router: format!("http://{}", listener.local_addr().unwrap()),
            online: true,
            credits: "Synthetic route fixture".into(),
            ..Default::default()
        }
        .save_atomic(&workspace.path().join("maps/navigation.json"))
        .unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") && request.len() < 8192 {
                request.push(socket.read_u8().await.unwrap());
            }
            assert!(String::from_utf8_lossy(&request).contains("/route/v1/driving/"));
            let body = r#"{"code":"Ok","routes":[{"distance":2500,"duration":600,"geometry":{"coordinates":[[-46.6333,-23.5505],[-46.61,-23.54]]}}]}"#;
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        });
    }
    std::fs::write(
        workspace.path().join("maps/settings.json"),
        serde_json::to_vec(
            &source
                .map(|path| {
                    serde_json::from_slice::<lince_map::Settings>(&std::fs::read(path).unwrap())
                        .unwrap()
                })
                .unwrap_or_else(|| {
                    if detail {
                        lince_map::Settings::browsing()
                    } else {
                        lince_map::Settings::default()
                    }
                }),
        )
        .unwrap(),
    )
    .unwrap();
    let mut app = lince_sandbox::app::offscreen_interface_app();
    app.insert_resource(lince_sandbox::workspace::WorkspaceFile::new(
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
    let map = lince_sandbox::map_castle::spawn(
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
                .any(|text| text.0.starts_with("Map ready"));
            if loaded {
                break;
            }
        }
        settle(&mut app, 3);
        capture(&mut app, directory.join("flat-roads.png"));
    }
    if verify {
        activate(app.world_mut(), "Search / directions");
        activate(app.world_mut(), "Use point as start");
        app.world_mut()
            .get_mut::<MapSand>(map)
            .unwrap()
            .camera
            .center = lince_map::Coordinate {
            latitude: -23.54,
            longitude: -46.61,
        };
        step(&mut app);
        activate(app.world_mut(), "Use point as destination");
        activate(app.world_mut(), "Find route");
        settle(&mut app, 2);
        assert!(
            app.world_mut()
                .query::<&Text>()
                .iter(app.world())
                .any(|text| text.0.starts_with("Driving:")),
            "The reviewed route must be rendered"
        );
        activate(app.world_mut(), "Close search / directions");
        capture(&mut app, directory.join("route-dark.png"));
        let image = app
            .world()
            .resource::<lince_interface::map_view::Scenes>()
            .0[&map]
            .image;
        let viewport = app
            .world_mut()
            .query::<(Entity, &ImageNode)>()
            .iter(app.world())
            .find(|(_, node)| node.image.id() == image)
            .unwrap()
            .0;
        let window = app
            .world_mut()
            .query_filtered::<Entity, With<Window>>()
            .single(app.world())
            .unwrap();
        app.world_mut().trigger(bevy::picking::events::Pointer::new(
            bevy::picking::pointer::PointerId::Mouse,
            bevy::picking::pointer::Location {
                target: RenderTarget::Window(bevy::window::WindowRef::Entity(window))
                    .normalize(None)
                    .unwrap(),
                position: Vec2::ZERO,
            },
            bevy::picking::events::Drag {
                button: bevy::picking::pointer::PointerButton::Primary,
                distance: Vec2::new(128.0, 0.0),
                delta: Vec2::new(128.0, 0.0),
            },
            viewport,
        ));
        app.world_mut().flush();
        step(&mut app);
        let mut input_completion = Vec::new();
        let mut queue_wait = Vec::new();
        for caption in ["+", "−", "+", "−", "+", "−", "+", "−"] {
            let input = Instant::now();
            activate(app.world_mut(), caption);
            app.update();
            let submitted = Instant::now();
            app.world()
                .resource::<RenderDevice>()
                .wgpu_device()
                .poll(PollType::Wait {
                    submission_index: None,
                    timeout: None,
                })
                .unwrap();
            input_completion.push(input.elapsed().as_micros());
            queue_wait.push(submitted.elapsed().as_micros());
        }
        settle(&mut app, 2);
        let revision = |app: &App| {
            app.world()
                .resource::<lince_interface::map_view::Scenes>()
                .0
                .values()
                .map(|target| {
                    target
                        .scene
                        .lock()
                        .unwrap()
                        .revision
                        .load(std::sync::atomic::Ordering::Relaxed)
                })
                .sum::<u64>()
        };
        let before = revision(&app);
        for _ in 0..8 {
            step(&mut app);
        }
        let idle_revisions = revision(&app).saturating_sub(before);
        assert_eq!(idle_revisions, 0);
        app.world_mut()
            .resource_mut::<lince_sandbox::tokens::ThemeSettings>()
            .scheme = lince_sandbox::tokens::ColorScheme::Light;
        settle(&mut app, 1);
        capture(&mut app, directory.join("map-light.png"));
        app.world_mut()
            .resource_mut::<lince_sandbox::tokens::ThemeSettings>()
            .global
            .set(
                lince_sandbox::tokens::Token::FontSize,
                lince_sandbox::tokens::TokenValue::Number(22.4),
            );
        activate(app.world_mut(), "Map Settings");
        settle(&mut app, 1);
        capture(&mut app, directory.join("map-settings-large.png"));
        activate(app.world_mut(), "Map Settings");
        activate(app.world_mut(), "Search / directions");
        let query = app
            .world_mut()
            .query::<(Entity, &lince_interface::controls::Editor)>()
            .iter(app.world())
            .find(|(_, editor)| editor.title == "Place search")
            .unwrap()
            .0;
        app.world_mut()
            .get_mut::<bevy::text::EditableText>(query)
            .unwrap()
            .editor
            .set_text("רחוב ארוך מאוד · A long place name for the selected reference point");
        settle(&mut app, 1);
        capture(&mut app, directory.join("map-search-large-rtl.png"));
        activate(app.world_mut(), "Providers / regional packs");
        settle(&mut app, 1);
        capture(&mut app, directory.join("map-providers-large.png"));
        let panel = app.world().get::<ChildOf>(query).unwrap().parent();
        app.world_mut().trigger(bevy::picking::events::Pointer::new(
            bevy::picking::pointer::PointerId::Mouse,
            bevy::picking::pointer::Location {
                target: RenderTarget::Window(bevy::window::WindowRef::Entity(window))
                    .normalize(None)
                    .unwrap(),
                position: Vec2::ZERO,
            },
            bevy::picking::events::Scroll {
                unit: bevy::input::mouse::MouseScrollUnit::Pixel,
                x: 0.0,
                y: -10000.0,
                phase: bevy::input::touch::TouchPhase::Moved,
                hit: bevy::picking::backend::HitData::new(panel, 0.0, None, None),
            },
            query,
        ));
        app.world_mut().flush();
        step(&mut app);
        assert!(app.world().get::<ScrollPosition>(panel).unwrap().0.y > 0.0);
        capture(&mut app, directory.join("map-providers-bottom.png"));
        app.world_mut().despawn(map);
        for kind in [
            lince_sandbox::sand_store::SandKind::Trust,
            lince_sandbox::sand_store::SandKind::Visibility,
        ] {
            let sand = lince_sandbox::sand_store::spawn_sand(
                app.world_mut(),
                root,
                1,
                kind,
                "",
                DVec2::ZERO,
            );
            app.world_mut()
                .get_mut::<lince_sandbox::canvas::CanvasItem>(sand)
                .unwrap()
                .size = Vec2::new(850.0, 640.0);
            let field = app
                .world_mut()
                .query::<(Entity, &lince_interface::controls::Editor)>()
                .iter(app.world())
                .find(|(entity, _)| {
                    let mut cursor = *entity;
                    while let Some(parent) = app.world().get::<ChildOf>(cursor) {
                        cursor = parent.parent();
                        if cursor == sand {
                            return true;
                        }
                    }
                    false
                })
                .map(|(entity, _)| entity);
            if let Some(field) = field {
                app.world_mut().get_mut::<bevy::text::EditableText>(field).unwrap().editor.set_text("רחוב ארוך מאוד · A long private draft used only for presentation verification");
            }
            settle(&mut app, 1);
            capture(
                &mut app,
                directory.join(format!("{}-large.png", kind.name().to_lowercase())),
            );
            app.world_mut().despawn(sand);
        }
        let report = serde_json::json!({
            "input_action_to_queue_completion":summary(&input_completion),
            "queue_completion_wait":summary(&queue_wait),"idle_scene_revisions":idle_revisions,
            "scope":"Native Bevy controls and GPU offscreen output. Completion is a device queue fence including CPU work and all queued GPU work, not exclusive GPU execution. Pipelining is disabled in this fixture. OS input and display presentation latency are unmeasured. Android hardware is deferred.",
            "adapter":app.world().resource::<bevy::render::renderer::RenderAdapterInfo>().name
        });
        std::fs::write(
            directory.join("presentation.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
    }
}
