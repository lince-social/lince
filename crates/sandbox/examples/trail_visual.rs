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
use lince_sandbox::{container::BoxRoot, document_viewer::DocumentViewer, source_view::SourceView};
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

fn scroll(app: &mut App, caption: &str, down: bool) {
    let target = control(app.world_mut(), caption);
    let mut viewport = target;
    while app.world().get::<ScrollPosition>(viewport).is_none() {
        viewport = app.world().get::<ChildOf>(viewport).unwrap().parent();
    }
    let computed = app.world().get::<ComputedNode>(viewport).unwrap();
    assert!(computed.content_size.y > computed.size().y + 1.0);
    app.world_mut().trigger(Pointer::new(
        bevy::picking::pointer::PointerId::Mouse,
        bevy::picking::pointer::Location {
            target: bevy::camera::NormalizedRenderTarget::Image(Handle::<Image>::default().into()),
            position: Vec2::ZERO,
        },
        bevy::picking::events::Scroll {
            unit: bevy::input::mouse::MouseScrollUnit::Pixel,
            x: 0.0,
            y: if down { -100_000.0 } else { 100_000.0 },
            phase: bevy::input::touch::TouchPhase::Moved,
            hit: bevy::picking::backend::HitData::new(viewport, 0.0, None, None),
        },
        target,
    ));
    app.world_mut().flush();
    for _ in 0..8 {
        step(app);
    }
    if down {
        let world = app.world();
        let panel = world.get::<ComputedNode>(viewport).unwrap();
        let button = world.get::<ComputedNode>(target).unwrap();
        let center = world
            .get::<UiGlobalTransform>(viewport)
            .unwrap()
            .translation
            .y;
        let button_center = world
            .get::<UiGlobalTransform>(target)
            .unwrap()
            .translation
            .y;
        assert!(button_center - button.size().y * 0.5 >= center - panel.size().y * 0.5 - 2.0);
        assert!(button_center + button.size().y * 0.5 <= center + panel.size().y * 0.5 + 2.0);
    }
}

fn capture(app: &mut App, path: PathBuf) {
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

async fn authority(engine: &engine::Engine) {
    use engine::{
        roster::{CellEntry, ROOT_KEY_ID},
        trust::Signer,
    };
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap();
    let cell = store::cells::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap();
    let root = Signer::generate(&organ.uid, ROOT_KEY_ID);
    let key = Signer::generate(&organ.uid, &engine::roster::cell_key_id(&cell.uid));
    engine.set_signer(key.clone()).await.unwrap();
    engine.publish_root_key(&root).await.unwrap();
    let mut roster = engine
        .publish_roster(
            &root,
            vec![CellEntry {
                cell_uid: cell.uid,
                node_id: "trail-native-fixture".into(),
                label: cell.label,
                operational_key: key.public_key_b64(),
                sealing_key: None,
                front_door: false,
                capabilities: engine::roster::full_capabilities(),
            }],
        )
        .await
        .unwrap();
    roster.roster.version += 1;
    roster.roster.not_after = "2100-01-01T00:00:00Z".into();
    roster.signature =
        root.sign_bytes(&engine::roster::roster_signing_payload(&roster.roster).unwrap());
    assert_eq!(
        engine.adopt_roster(&roster).await.unwrap(),
        engine::roster::RosterOutcome::Accepted
    );
}

async fn points(app: &mut App, engine: &engine::Engine, record: &str, expected: &str) {
    let started = Instant::now();
    loop {
        step(app);
        let report = engine.run_database_effects().await.unwrap();
        assert!(
            report.outcomes.iter().all(|outcome| outcome.ok),
            "{:?}",
            report.outcomes
        );
        if store::facts::level(&engine.store.pool, record)
            .await
            .unwrap()
            .to_string()
            == expected
        {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "Authored answer points did not settle"
        );
    }
}

#[tokio::main]
async fn main() {
    let directory = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("lince-trail-visual"));
    std::fs::create_dir_all(&directory).unwrap();
    let temporary = tempfile::tempdir().unwrap();
    utils::config::set_lince_data_dir_override(temporary.path().join("lince")).unwrap();
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    authority(&engine).await;
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
    let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    for (name, title, path) in [
        ("Philosophy", "Philosophy", "Philosophy.lingua"),
        (
            "Plato",
            "Plato",
            "ancient-greek-philosophy/plato/Plato.lingua",
        ),
    ] {
        let uri = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../institute/trails/philosophy")
            .join(path)
            .to_string_lossy()
            .into_owned();
        let browser = lince_sandbox::source_view::spawn(
            app.world_mut(),
            root,
            1,
            DVec2::ZERO,
            SourceView {
                uri,
                ..Default::default()
            },
        );
        activate(app.world_mut(), "Read source");
        wait(&mut app, |world| {
            label(world, &format!("{title} · Source document"))
        });
        capture(
            &mut app,
            directory.join(format!("trail-browse-{}.png", name.to_ascii_lowercase())),
        );
        app.world_mut()
            .get_mut::<lince_sandbox::canvas::CanvasItem>(browser)
            .unwrap()
            .size = Vec2::new(360.0, 760.0);
        for _ in 0..8 {
            step(&mut app);
        }
        capture(
            &mut app,
            directory.join(format!(
                "trail-browse-{}-narrow.png",
                name.to_ascii_lowercase()
            )),
        );
        app.world_mut().despawn(browser);
        step(&mut app);
    }
    let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    let uri = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../institute/trails/shared/Assessment-example.lingua")
        .to_string_lossy()
        .into_owned();
    let castle = lince_sandbox::sync_castle::open_source(app.world_mut(), root, &uri);
    activate(app.world_mut(), "Preview import");
    wait(&mut app, |world| label(world, "Add to Cell"));
    wait(&mut app, |world| label(world, "Proposed Karma:"));
    capture(&mut app, directory.join("trail-preview-wide.png"));
    scroll(&mut app, "Add to Cell", true);
    capture(&mut app, directory.join("trail-rules-wide.png"));
    scroll(&mut app, "Add to Cell", false);
    app.world_mut()
        .get_mut::<lince_sandbox::canvas::CanvasItem>(castle)
        .unwrap()
        .size = Vec2::new(360.0, 760.0);
    for _ in 0..8 {
        step(&mut app);
    }
    capture(&mut app, directory.join("trail-preview-narrow.png"));
    scroll(&mut app, "Add to Cell", true);
    capture(&mut app, directory.join("trail-rules-narrow.png"));
    app.world_mut()
        .get_mut::<lince_sandbox::canvas::CanvasItem>(castle)
        .unwrap()
        .size = Vec2::new(680.0, 760.0);
    for _ in 0..8 {
        step(&mut app);
    }
    scroll(&mut app, "Add to Cell", true);
    activate(app.world_mut(), "Add to Cell");
    wait(&mut app, |world| {
        world.get::<SourceView>(castle).unwrap().binding.is_some()
    });
    let response: String = store::sqlx::query_scalar("SELECT uid FROM record WHERE head=?")
        .bind("Which method examines whether a conclusion follows from its premises?")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    let score: String =
        store::sqlx::query_scalar("SELECT uid FROM record WHERE head='Argument question points'")
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
    wait(&mut app, |world| label(world, "Enable reviewed Karma"));
    activate(app.world_mut(), "Enable reviewed Karma");
    let started = Instant::now();
    loop {
        step(&mut app);
        if !store::recurrence::for_record(&engine.store.pool, &score)
            .await
            .unwrap()
            .is_empty()
        {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "Reviewed Karma was not enabled"
        );
    }
    app.world_mut()
        .get_mut::<SourceView>(castle)
        .unwrap()
        .current = Some(response);
    activate(app.world_mut(), "Preview import");
    wait(&mut app, |world| label(world, "Answer: Reasoning"));
    for (caption, expected) in [("Answer: Reasoning", "5"), ("Clear answer", "0")] {
        activate(app.world_mut(), caption);
        points(&mut app, &engine, &score, expected).await;
        activate(app.world_mut(), "Refresh state");
        wait(&mut app, |world| {
            label(world, &format!("Argument question points ({expected})"))
        });
        capture(
            &mut app,
            directory.join(format!("trail-answer-{expected}.png")),
        );
    }
    for name in [
        "philosophy/ancient-greek-philosophy/plato/Plato.lingua",
        "philosophy/ancient-greek-philosophy/sources/Socratic-dialogues-Cary.lingua",
    ] {
        let uri = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../institute/trails")
            .join(name)
            .to_string_lossy()
            .into_owned();
        let preview = engine
            .act(
                engine::actions::Action::Sources {
                    request: engine::sources::Command::Preview {
                        uri: uri.clone(),
                        selected: vec![],
                        bindings: Default::default(),
                        refresh: false,
                    },
                },
                None,
            )
            .await
            .unwrap()
            .data
            .unwrap();
        engine
            .act(
                engine::actions::Action::Sources {
                    request: engine::sources::Command::Import {
                        uri,
                        selected: vec![],
                        bindings: Default::default(),
                        revision: preview["revision"].as_str().unwrap().into(),
                        update: false,
                    },
                },
                None,
            )
            .await
            .unwrap();
    }
    let plato: String = store::sqlx::query_scalar("SELECT uid FROM record WHERE head='Plato'")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    let body: String = store::sqlx::query_scalar("SELECT body FROM record WHERE uid=?")
        .bind(&plato)
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    assert!(body.is_empty());
    app.world_mut()
        .get_mut::<SourceView>(castle)
        .unwrap()
        .current = Some(plato);
    activate(app.world_mut(), "Preview import");
    wait(&mut app, |world| label(world, "Download / keep offline"));
    capture(&mut app, directory.join("trail-plato-reference.png"));
    activate(app.world_mut(), "Open source");
    wait(&mut app, |world| {
        world
            .query::<&DocumentViewer>()
            .iter(world)
            .next()
            .is_some()
            && label(world, "Chapter")
            && !label(world, "Rendering…")
    });
    app.world_mut().despawn(castle);
    capture(&mut app, directory.join("trail-book-viewer.png"));
    activate(app.world_mut(), "Keep offline");
    wait(&mut app, |world| label(world, "Available offline"));
    capture(&mut app, directory.join("trail-book-offline.png"));
    activate(app.world_mut(), "Next");
    wait(&mut app, |world| {
        label(world, "Chapter 2 / 4") && !label(world, "Rendering…")
    });
    capture(&mut app, directory.join("trail-book-reading.png"));
    let document = app
        .world_mut()
        .query::<&DocumentViewer>()
        .single(app.world())
        .unwrap()
        .clone();
    let saved: DocumentViewer =
        serde_json::from_value(serde_json::to_value(&document).unwrap()).unwrap();
    assert_eq!(saved.path, document.path);
    assert_eq!(saved.resource, document.resource);
    assert_eq!(saved.position(), document.position());
    assert_eq!(saved.position().section, 1);
    std::fs::write(directory.join("verification.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "native_controls":["Preview import","Add to Cell","Enable reviewed Karma","Answer: Reasoning","Clear answer","Refresh state","Open source","Keep offline","Next"],
        "widths":[360,680],"scroll_reaches_import_control":true,"plato_description_empty":true,"document_state_round_trip":true,
        "authored_points":[5,0],
        "scope":"Native Bevy controls, local Cell import, authored answer/points rules, book reference, offline retention and offscreen rendering. The fixture runs the ordinary database effect runner explicitly. Physical keyboard, audio hardware and display presentation are unmeasured."
    })).unwrap()).unwrap();
}
