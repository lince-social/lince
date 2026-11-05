use bevy::{
    camera::{ImageRenderTarget, RenderTarget},
    prelude::*,
    render::{
        render_resource::{PollType, TextureFormat},
        renderer::RenderDevice,
        view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    },
};
use lince_sandbox::{container::BoxRoot, source_view::SourceView};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

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
    std::thread::sleep(Duration::from_millis(5));
}

fn wait(app: &mut App, ready: impl Fn(&mut World) -> bool) {
    let started = Instant::now();
    loop {
        step(app);
        if ready(app.world_mut()) {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "Native Trail workflow timed out"
        );
    }
    for _ in 0..8 {
        step(app);
    }
}

fn label(world: &mut World, caption: &str) -> bool {
    world
        .query::<&Text>()
        .iter(world)
        .any(|text| text.0.contains(caption))
}

fn control(world: &mut World, caption: &str) -> Entity {
    world
        .query::<(Entity, &Text)>()
        .iter(world)
        .find_map(|(entity, text)| {
            if text.0 != caption {
                return None;
            }
            let parent = world.get::<ChildOf>(entity)?.parent();
            world
                .get::<bevy::ui_widgets::Button>(parent)
                .map(|_| parent)
        })
        .unwrap_or_else(|| panic!("Missing native control: {caption}"))
}

fn activate(world: &mut World, caption: &str) {
    let entity = control(world, caption);
    world.trigger(bevy::ui_widgets::Activate { entity });
    world.flush();
}

fn capture(app: &mut App, path: PathBuf) {
    for _ in 0..8 {
        step(app);
    }
    app.world_mut().resource_mut::<Captured>().0 = false;
    let target = app.world().resource::<Output>().0.clone();
    app.world_mut()
        .spawn(Screenshot(target))
        .observe(save_to_disk(path.clone()))
        .observe(
            |event: On<ScreenshotCaptured>, mut captured: ResMut<Captured>| {
                let bytes = event.image.data.as_ref().unwrap();
                assert!(bytes.chunks_exact(4).any(|pixel| pixel != &bytes[..4]));
                captured.0 = true;
            },
        );
    wait(app, |world| world.resource::<Captured>().0);
    println!("Captured {}", path.display());
}

#[tokio::main]
async fn main() {
    let directory = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("lince-sync-outline"));
    std::fs::create_dir_all(&directory).unwrap();
    let temporary = tempfile::tempdir().unwrap();
    utils::config::set_lince_data_dir_override(temporary.path().join("data")).unwrap();
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let runtime = cell::CellRuntime {
        commands: Default::default(),
        store: engine.store.clone(),
        engine: engine.clone(),
        lanes: Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        fiote: None,
        speech: None,
        information: None,
    };
    let mut app = lince_sandbox::app::offscreen_interface_app();
    app.insert_resource(lince_sandbox::workspace::WorkspaceFile::new(
        temporary.path().join("interface.json"),
    ))
    .insert_resource(lince_sandbox::app::CellHandle(runtime))
    .init_resource::<Captured>()
    .add_plugins(lince_sandbox::cell_bridge::CellBridgePlugin)
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
    step(&mut app);
    let root = app
        .world_mut()
        .query_filtered::<Entity, With<BoxRoot>>()
        .single(app.world())
        .unwrap();
    let uri = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../institute/trails/scholazein/philosophy/Philosophy.lingua")
        .to_string_lossy()
        .into_owned();
    let owner = lince_sandbox::sync_castle::open_source(app.world_mut(), root, &uri);
    wait(&mut app, |world| {
        label(world, "Add to Cell") && label(world, "Philosophy")
    });
    let mut descendants = vec![owner];
    let mut count = 0;
    while let Some(entity) = descendants.pop() {
        count += usize::from(app.world().get::<Text>(entity).is_some());
        if let Some(children) = app.world().get::<Children>(entity) {
            descendants.extend(children.iter());
        }
    }
    assert!(count < 50, "Collapsed source rendered {count} text nodes");
    capture(&mut app, directory.join("sync-collapsed.png"));
    activate(app.world_mut(), "▸");
    capture(&mut app, directory.join("sync-first-level.png"));
    activate(app.world_mut(), "Expand all");
    capture(&mut app, directory.join("sync-expanded.png"));
    let branch = app
        .world_mut()
        .query::<(Entity, &lince_sandbox::icons::Tooltip)>()
        .iter(app.world())
        .find(|(_, hint)| hint.0 == "Collapse children of Karmic philosophy: Buddhism and Yoga")
        .map(|(entity, _)| entity)
        .unwrap();
    app.world_mut()
        .trigger(bevy::ui_widgets::Activate { entity: branch });
    for _ in 0..8 {
        step(&mut app);
    }
    assert!(label(
        app.world_mut(),
        "Karmic philosophy: Buddhism and Yoga"
    ));
    assert!(!label(
        app.world_mut(),
        "Buddhism: from the path to sustained inquiry"
    ));
    activate(app.world_mut(), "Collapse all");
    activate(app.world_mut(), "Add to Cell");
    wait(&mut app, |world| label(world, "In Cell"));
    let added: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record_source_location")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    assert!(added > 20);
    capture(&mut app, directory.join("sync-added.png"));
    assert!(app.world().get::<SourceView>(owner).is_some());
    let book: String = store::sqlx::query_scalar(
        "SELECT uid FROM record WHERE head='Apology, Crito, and Phaedo of Socrates'",
    )
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    lince_sandbox::full_record::open(
        app.world_mut(),
        root,
        &book,
        lince_sandbox::protein_area::Source::Local,
    )
    .unwrap();
    wait(&mut app, |world| {
        label(world, "Download and read Public EPUB")
    });
    activate(app.world_mut(), "Download and read Public EPUB");
    wait(&mut app, |world| label(world, "Available offline"));
    let resource = app
        .world_mut()
        .query::<&lince_sandbox::document_viewer::DocumentViewer>()
        .iter(app.world())
        .filter_map(|viewer| viewer.resource.as_ref())
        .find(|resource| {
            resource.uri.starts_with("https://www.gutenberg.org/") && resource.hash.is_some()
        })
        .unwrap()
        .clone();
    let library = engine::sources::library().unwrap();
    let saved = library.find(&resource).unwrap().unwrap();
    assert!(saved.path.is_file());
    assert!(resource.hash.is_some());
    let mut offline = resource.clone();
    offline.uri = "https://offline.invalid/book.epub".into();
    let reader = lince_document::Reader::open_resource(library.remote(), &offline).unwrap();
    assert!(!reader.info.sections.is_empty());
    capture(&mut app, directory.join("public-book-offline.png"));
    println!(
        "Compact source browsing and one-click Philosophy import passed: {}",
        directory.display()
    );
}
