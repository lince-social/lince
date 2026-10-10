use bevy::{
    camera::{ImageRenderTarget, RenderTarget},
    core_pipeline::tonemapping::DebandDither,
    input::ButtonState,
    math::{DQuat, DVec2, DVec3},
    prelude::*,
    render::{
        render_resource::{PollType, TextureFormat},
        renderer::RenderDevice,
        view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    },
    window::WindowEvent,
};
use lince_desktop::{
    actions::ActionButton,
    canvas::{CanvasItem, CanvasView},
    container::BoxRoot,
    sand_store::{SandKind, spawn_sand},
    time_castle::TimeSettings,
    topology::{input::PointerState, presentation::Surface},
};
use std::{path::PathBuf, sync::Arc, time::Instant};

#[derive(Resource)]
struct Output(RenderTarget);

#[derive(Resource, Default)]
struct Captured(bool);

fn output(world: &mut World) {
    let window = world.query::<&Window>().single(world).unwrap();
    let width = window.physical_width();
    let height = window.physical_height();
    let scale_factor = window.scale_factor();
    let target = ImageRenderTarget {
        handle: world
            .resource_mut::<Assets<Image>>()
            .add(Image::new_target_texture(
                width,
                height,
                TextureFormat::Bgra8UnormSrgb,
                None,
            )),
        scale_factor,
    };
    let target = RenderTarget::Image(target);
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

fn settle(app: &mut App, seconds: f32) {
    let started = Instant::now();
    let mut frames = 0;
    while started.elapsed().as_secs_f32() < seconds || frames < 8 {
        step(app);
        frames += 1;
    }
}

fn capture(app: &mut App, directory: &std::path::Path, name: &str) {
    app.world_mut().resource_mut::<Captured>().0 = false;
    let target = app.world().resource::<Output>().0.clone();
    let path = directory.join(format!("{name}.png"));
    app.world_mut()
        .spawn(Screenshot(target))
        .observe(save_to_disk(path.clone()))
        .observe(
            |capture: On<ScreenshotCaptured>, mut ready: ResMut<Captured>| {
                let data = capture.image.data.as_ref().unwrap();
                assert!(data.windows(4).any(|pixel| pixel != &data[..4]));
                ready.0 = true;
            },
        );
    let started = Instant::now();
    while !app.world().resource::<Captured>().0 {
        assert!(
            started.elapsed().as_secs() < 30,
            "Capture timed out: {name}"
        );
        step(app);
    }
    let clock = app
        .world_mut()
        .query_filtered::<Entity, With<TimeSettings>>()
        .single(app.world())
        .unwrap();
    let positions: Vec<_> = cards(app.world_mut(), clock).into_iter().map(|(_, item, title)| {
        serde_json::json!({"title": title, "center": item.position.to_array(), "size": item.size.to_array()})
    }).collect();
    std::fs::write(
        directory.join(format!("{name}.json")),
        serde_json::to_vec_pretty(&positions).unwrap(),
    )
    .unwrap();
    println!("Captured {}", path.display());
}

fn activate(world: &mut World, clock: Entity, caption: &str) {
    let action = world
        .query::<(
            Entity,
            &ActionButton,
            Option<&Children>,
            Option<&lince_desktop::icons::IconButton>,
            Option<&lince_desktop::icons::Tooltip>,
        )>()
        .iter(world)
        .find(|(entity, button, children, icon, tooltip)| {
            let mut ancestor = Some(*entity);
            while let Some(entity) = ancestor {
                if world
                    .get::<Node>(entity)
                    .is_some_and(|node| node.display == Display::None)
                {
                    return false;
                }
                ancestor = world.get::<ChildOf>(entity).map(ChildOf::parent);
            }
            button.target == clock
                && (tooltip.is_some_and(|tooltip| tooltip.0 == caption)
                    || icon.is_some_and(|icon| icon.label == caption)
                    || children.is_some_and(|children| {
                        children.iter().any(|child| {
                            world
                                .get::<Text>(child)
                                .is_some_and(|text| text.0 == caption)
                        })
                    }))
        })
        .unwrap_or_else(|| panic!("Missing action: {caption}"))
        .1
        .clone();
    action.actions.run(world, clock);
}

fn cards(world: &mut World, clock: Entity) -> Vec<(Entity, CanvasItem, String)> {
    world
        .query::<(Entity, &CanvasItem, &ActionButton, &Children)>()
        .iter(world)
        .filter(|(entity, _, action, _)| *entity != clock && action.target == clock)
        .map(|(entity, item, _, children)| {
            let title = children
                .iter()
                .find_map(|child| world.get::<Text>(child))
                .unwrap()
                .0
                .clone();
            (entity, *item, title)
        })
        .collect()
}

fn check_card_bounds(world: &mut World, clock: Entity) {
    let cards = cards(world, clock);
    for (index, (entity, a, title)) in cards.iter().enumerate() {
        for (_, b, other) in &cards[index + 1..] {
            let overlap = (a.size + b.size).as_dvec2() * 0.5 - (a.position - b.position).abs();
            assert!(
                overlap.min_element() <= 0.1,
                "Cards overlap: {title}, {other}"
            );
        }
        for child in world.get::<Children>(*entity).unwrap().iter() {
            if world.get::<Text>(child).is_some() {
                let node = world.get::<ComputedNode>(child).unwrap();
                let width = node.size().x * node.inverse_scale_factor();
                assert!(
                    width <= a.size.x - 12.0,
                    "Card text exceeds its width: {title}: {width} > {}",
                    a.size.x
                );
            }
        }
    }
}

fn size_probe(app: &mut App, clock: Entity, directory: &std::path::Path) {
    let panel = app
        .world_mut()
        .query::<(&Text, &ChildOf)>()
        .iter(app.world())
        .find_map(|(text, parent)| {
            let button = parent.parent();
            let panel = app.world().get::<ChildOf>(button)?.parent();
            (text.0.contains('\n')
                && app.world().get::<CanvasItem>(button).is_none()
                && app
                    .world()
                    .get::<Node>(panel)
                    .is_some_and(|node| node.display != Display::None)
                && app
                    .world()
                    .get::<ChildOf>(panel)
                    .is_some_and(|parent| parent.parent() == clock)
                && app
                    .world()
                    .get::<ActionButton>(button)
                    .is_some_and(|action| action.target == clock))
            .then_some(panel)
        })
        .unwrap();
    let skull = app
        .world_mut()
        .query::<(&lince_desktop::icons::Tooltip, &ActionButton, &ChildOf)>()
        .iter(app.world())
        .find(|(tooltip, action, parent)| {
            tooltip.0 == "Clock controls"
                && action.target == clock
                && app
                    .world()
                    .get::<Node>(parent.parent())
                    .is_some_and(|node| node.display != Display::None)
        })
        .unwrap()
        .2
        .parent();
    app.world_mut()
        .get_mut::<TimeSettings>(clock)
        .unwrap()
        .0
        .card_physics = false;
    let mut report = Vec::new();
    for (width, height) in [
        (800, 800),
        (640, 640),
        (500, 500),
        (420, 420),
        (320, 320),
        (260, 260),
        (220, 220),
        (180, 180),
        (120, 120),
        (80, 80),
        (640, 260),
        (260, 640),
        (420, 420),
    ] {
        app.world_mut().get_mut::<CanvasItem>(clock).unwrap().size =
            Vec2::new(width as f32, height as f32);
        settle(app, 1.0);
        capture(app, directory, &format!("size-{width}x{height}"));
        check_card_bounds(app.world_mut(), clock);
        let tasks = app.world().get::<Node>(panel).unwrap().display != Display::None;
        let memento = app.world().get::<Node>(skull).unwrap().display != Display::None;
        assert!(!memento || tasks);
        if width.min(height) <= 180 {
            assert!(!tasks && !memento);
        }
        if width.min(height) == 260 {
            assert!(tasks && !memento);
        }
        if width.min(height) >= 420 {
            assert!(tasks && memento);
        }
        report.push(serde_json::json!({"size": [width, height], "tasks": tasks, "memento": memento, "cards_do_not_overlap": true}));
    }
    app.world_mut()
        .get_mut::<TimeSettings>(clock)
        .unwrap()
        .0
        .card_physics = true;
    let started = Instant::now();
    while started.elapsed().as_secs_f32() < 9.0 {
        step(app);
        check_card_bounds(app.world_mut(), clock);
    }
    capture(app, directory, "physics-settled");
    std::fs::write(
        directory.join("sizes.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("Size probe passed: 13 resize cases, both physics settings, no card overlaps");
}

fn cursor(app: &mut App, position: Option<Vec2>) {
    let window = app
        .world_mut()
        .query::<(Entity, &mut Window)>()
        .single_mut(app.world_mut())
        .unwrap()
        .0;
    app.world_mut()
        .get_mut::<Window>(window)
        .unwrap()
        .set_cursor_position(position);
    if let Some(position) = position {
        let event = bevy::window::CursorMoved {
            window,
            position,
            delta: None,
        };
        app.world_mut().write_message(event.clone());
        app.world_mut()
            .write_message(WindowEvent::CursorMoved(event));
    }
    step(app);
}

fn button(app: &mut App, state: ButtonState) {
    let window = app
        .world_mut()
        .query_filtered::<Entity, With<Window>>()
        .single(app.world())
        .unwrap();
    let event = bevy::input::mouse::MouseButtonInput {
        window,
        button: MouseButton::Left,
        state,
    };
    app.world_mut().write_message(event);
    app.world_mut()
        .write_message(WindowEvent::MouseButtonInput(event));
    step(app);
}

fn screen(world: &World, position: DVec2) -> Vec2 {
    let camera = world
        .resource::<lince_desktop::topology::presentation::SceneCamera>()
        .0;
    world
        .get::<Camera>(camera)
        .unwrap()
        .world_to_viewport(
            world.get::<GlobalTransform>(camera).unwrap(),
            Vec3::new(position.x as f32, 0.0, position.y as f32),
        )
        .unwrap()
}

async fn fixture() -> Arc<engine::Engine> {
    if let Some(path) = std::env::args()
        .find_map(|argument| argument.strip_prefix("--fixture-copy=").map(str::to_owned))
    {
        assert!(std::path::Path::new(&path).starts_with("/tmp"));
        return Arc::new(
            engine::Engine::open(&format!("sqlite://{path}"))
                .await
                .unwrap(),
        );
    }
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap();
    let cell = store::cells::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap();
    let root = engine::trust::Signer::generate(&organ.uid, engine::roster::ROOT_KEY_ID);
    let operational =
        engine::trust::Signer::generate(&organ.uid, &engine::roster::cell_key_id(&cell.uid));
    engine.set_signer(operational.clone()).await.unwrap();
    engine.publish_root_key(&root).await.unwrap();
    engine
        .publish_roster(
            &root,
            vec![engine::roster::CellEntry {
                cell_uid: cell.uid,
                node_id: "time-visual".into(),
                label: "Clock visual verification".into(),
                operational_key: operational.public_key_b64(),
                sealing_key: None,
                front_door: false,
                capabilities: engine::roster::full_capabilities(),
            }],
        )
        .await
        .unwrap();
    let now = chrono::Utc::now();
    for (index, (head, start, duration)) in [
        ("Morning task", 10, Some(10)),
        ("Overlapping work", 15, Some(10)),
        ("Send update", 15, None),
        ("Stretch break", 15, None),
        ("Review design details", 18, Some(6)),
        ("Appointment", 40, None),
        (
            "Prepare a thoughtful response to the design review",
            48,
            Some(8),
        ),
        ("Future point", 30, None),
        ("Later work", 34, Some(5)),
        ("Final check", 55, None),
        ("Current task", -2, Some(9)),
        ("Completed review", -20, Some(8)),
        ("Past point", -10, None),
    ]
    .into_iter()
    .enumerate()
    {
        let uid = engine
            .act(
                engine::actions::Action::CreateRecord {
                    slug: Some(format!("clock-visual-{index}")),
                    kind: nucleus::RecordKind::Plain,
                    head: head.into(),
                    body: String::new(),
                    quantity: -1.0,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        engine.act(engine::actions::Action::SetExtension {
            target: uid,
            namespace: "work".into(),
            fds: serde_json::json!({"start": (now + chrono::TimeDelta::minutes(start)).to_rfc3339(), "estimate_min":duration}),
        }, None).await.unwrap();
    }
    engine
}

#[tokio::main]
async fn main() {
    let directory = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "target/clock-visual-verification".into()),
    );
    std::fs::create_dir_all(&directory).unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let engine = fixture().await;
    let mut app = lince_desktop::app::offscreen_interface_app();
    app.add_plugins((
        bevy::log::LogPlugin::default(),
        lince_desktop::cell_bridge::CellBridgePlugin,
    ))
    .insert_resource(lince_desktop::app::CellHandle(cell::CellRuntime {
        commands: Default::default(),
        engine: engine.clone(),
        store: engine.store.clone(),
        lanes: Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        fiote: None,
        speech: None,
        information: None,
    }))
    .insert_resource(lince_desktop::workspace::WorkspaceFile::new(
        workspace.path().join("interface.json"),
    ))
    .init_resource::<Captured>()
    .add_systems(
        Startup,
        |mut commands: Commands, mut windows: Query<&mut Window>| {
            windows.single_mut().unwrap().resolution.set(1440.0, 1080.0);
            commands.spawn(BoxRoot);
        },
    )
    .add_systems(PostStartup, output);
    app.finish();
    app.cleanup();
    println!(
        "Renderer: {:?}",
        app.world()
            .resource::<bevy::render::renderer::RenderAdapterInfo>()
            .0
    );
    settle(&mut app, 0.5);
    let root = app
        .world_mut()
        .query_filtered::<Entity, With<BoxRoot>>()
        .single(app.world())
        .unwrap();
    let clock = spawn_sand(
        app.world_mut(),
        root,
        1,
        SandKind::WorkTimer,
        "",
        DVec2::ZERO,
    );
    let refresh_probe = std::env::args().any(|argument| argument == "--refresh-probe");
    if let Some(path) = std::env::args()
        .find_map(|argument| argument.strip_prefix("--settings-copy=").map(str::to_owned))
    {
        assert!(std::path::Path::new(&path).starts_with("/tmp"));
        let settings = serde_json::from_slice::<lince_interface::time_castle::Settings>(
            &std::fs::read(path).unwrap(),
        )
        .unwrap();
        assert!(settings.source.is_none() && settings.area.is_none());
        app.world_mut()
            .entity_mut(clock)
            .insert(TimeSettings(settings));
    }
    let started = Instant::now();
    let copied = std::env::args().any(|argument| argument.starts_with("--fixture-copy="));
    while if copied {
        cards(app.world_mut(), clock).is_empty()
    } else {
        cards(app.world_mut(), clock).len() != 13
    } {
        assert!(
            started.elapsed().as_secs() < 60,
            "Schedule data did not arrive"
        );
        step(&mut app);
    }
    settle(&mut app, 7.0);
    assert!(app.world().get::<TimeSettings>(clock).unwrap().0.past_tasks);
    assert_eq!(
        app.world().get::<CanvasItem>(clock).unwrap().position,
        DVec2::ZERO
    );
    let camera = app
        .world()
        .resource::<lince_desktop::topology::presentation::SceneCamera>()
        .0;
    assert!(matches!(
        app.world().get::<DebandDither>(camera),
        Some(DebandDither::Disabled)
    ));
    assert!(
        !app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0.starts_with("Next ") && text.0.ends_with(" things"))
    );
    capture(&mut app, &directory, "01-default");
    if std::env::args().any(|argument| argument == "--size-probe") {
        size_probe(&mut app, clock, &directory);
        return;
    }
    if refresh_probe {
        let settings = app.world().get::<TimeSettings>(clock).unwrap().0.clone();
        let now = chrono::Utc::now().timestamp_millis();
        let context = nucleus::projection::Context {
            actor: None,
            window: settings.window(now).unwrap(),
        };
        let query = protein::schedule::query(context.window.clone(), Vec::new());
        engine.request_projection(context.clone()).await.unwrap();
        let started = Instant::now();
        loop {
            let rows = protein::execute(&engine.store, &query).await.unwrap();
            if rows.last().unwrap()["status"]["kind"] == "ready" {
                break;
            }
            assert!(started.elapsed().as_secs() < 60);
            step(&mut app);
        }
        settle(&mut app, 10.0);
        let before = cards(app.world_mut(), clock);
        assert_eq!(before.len(), 13);
        let snapshots = engine
            .projection
            .metrics
            .snapshots
            .load(std::sync::atomic::Ordering::Relaxed);
        let source = store::projection::revision(&engine.store.pool)
            .await
            .unwrap();
        for attempt in 0..3 {
            store::peer_delivery::note(
                &engine.store.pool,
                "visual-organ",
                "visual-cell",
                "visual-node",
                Err(format!("retry-{attempt}")),
            )
            .await
            .unwrap();
            settle(&mut app, 1.0);
            assert_eq!(
                store::projection::revision(&engine.store.pool)
                    .await
                    .unwrap(),
                source
            );
            assert_eq!(cards(app.world_mut(), clock).len(), before.len());
        }
        assert_eq!(
            engine
                .projection
                .metrics
                .snapshots
                .load(std::sync::atomic::Ordering::Relaxed),
            snapshots
        );
        engine
            .act(
                engine::actions::Action::CreateRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: "Clock refresh probe".into(),
                    body: String::new(),
                    quantity: 0.0,
                },
                None,
            )
            .await
            .unwrap();
        let rows = protein::execute(&engine.store, &query).await.unwrap();
        assert_eq!(rows.last().unwrap()["status"]["kind"], "updating");
        assert_eq!(
            rows.iter()
                .filter(|row| row["origin"]["kind"] == "projection")
                .count(),
            5
        );
        engine.request_projection(context.clone()).await.unwrap();
        let started = Instant::now();
        let mut frames = 0;
        let mut updating_frames = 0;
        let mut maximum_frame_ms = 0.0_f64;
        while started.elapsed().as_secs_f32() < 24.0 {
            let frame = Instant::now();
            step(&mut app);
            maximum_frame_ms = maximum_frame_ms.max(frame.elapsed().as_secs_f64() * 1000.0);
            let current = cards(app.world_mut(), clock);
            assert_eq!(
                current.len(),
                before.len(),
                "Forecast blinked during refresh"
            );
            for (entity, _, _) in &before {
                assert!(current.iter().any(|(card, _, _)| card == entity));
            }
            assert!(
                !app.world_mut()
                    .query::<&Text>()
                    .iter(app.world())
                    .any(|text| text.0 == "No upcoming work")
            );
            if app
                .world_mut()
                .query::<&Text>()
                .iter(app.world())
                .any(|text| text.0 == "Updating future simulation")
            {
                updating_frames += 1;
            }
            frames += 1;
        }
        let rows = protein::execute(&engine.store, &query).await.unwrap();
        assert_eq!(rows.last().unwrap()["status"]["kind"], "ready");
        assert!(updating_frames > 0, "Refresh state was not exercised");
        let after = cards(app.world_mut(), clock);
        for (entity, item, title) in &before {
            let next = &after.iter().find(|(card, _, _)| card == entity).unwrap().1;
            let past = app
                .world()
                .get::<Children>(*entity)
                .unwrap()
                .iter()
                .any(|child| {
                    app.world()
                        .get::<Text>(child)
                        .is_some_and(|text| text.0.starts_with("Needed:"))
                });
            if past {
                assert!((next.position.length() - item.position.length()).abs() < 0.1);
            } else {
                assert_eq!(next.position, item.position, "Future card moved: {title}");
            }
        }
        capture(&mut app, &directory, "refresh-stable");
        std::fs::write(directory.join("refresh-report.json"), serde_json::to_vec_pretty(&serde_json::json!({"settings": settings, "cards": before.len(), "forecast": 5, "refresh_frames": frames, "updating_frames": updating_frames, "seconds": started.elapsed().as_secs_f64(), "maximum_frame_ms": maximum_frame_ms, "forecast_retained": true, "stable_entities": true})).unwrap()).unwrap();
        println!("Refresh probe passed: {frames} frames, all 13 cards retained");
        return;
    }
    if std::env::args().any(|argument| argument == "--aperture-probe") {
        let initial: Vec<_> = cards(app.world_mut(), clock)
            .into_iter()
            .map(|(_, item, title)| (title, item.position.to_array()))
            .collect();
        println!("Initial cards: {initial:?}");
        let minutes = app
            .world()
            .get::<TimeSettings>(clock)
            .unwrap()
            .0
            .aperture_ms
            / 60_000;
        let apertures = [
            minutes.to_string(),
            (minutes + 60).to_string(),
            "1440".into(),
            minutes.to_string(),
        ];
        for pair in apertures.windows(2) {
            let (before, after) = (&pair[0], &pair[1]);
            activate(app.world_mut(), clock, "Clock controls");
            let field = app
                .world_mut()
                .query::<(Entity, &bevy::text::EditableText)>()
                .iter(app.world())
                .find(|(_, text)| text.value().to_string() == *before)
                .unwrap()
                .0;
            app.world_mut()
                .get_mut::<bevy::text::EditableText>(field)
                .unwrap()
                .editor
                .set_text(after);
            activate(app.world_mut(), clock, "Apply settings");
            activate(app.world_mut(), clock, "Hide clock controls");
            let started = Instant::now();
            while started.elapsed().as_secs_f32() < 10.0 {
                step(&mut app);
                assert!(
                    cards(app.world_mut(), clock).len() >= initial.len(),
                    "Cards disappeared while changing aperture"
                );
            }
            let titles: Vec<_> = cards(app.world_mut(), clock)
                .into_iter()
                .map(|(_, _, title)| title)
                .collect();
            println!("Aperture {after}: {titles:?}");
            capture(&mut app, &directory, &format!("aperture-{after}"));
        }
        println!("Aperture probe completed");
        return;
    }
    settle(&mut app, 4.0);
    let stopped = cards(app.world_mut(), clock);
    settle(&mut app, 3.0);
    for (card, item, _) in &stopped {
        let position = app.world().get::<CanvasItem>(*card).unwrap().position;
        if position.distance(item.position) > 0.001 {
            assert!((position.length() - item.position.length()).abs() < 0.1);
        }
    }
    for (index, (_, a, title)) in stopped.iter().enumerate() {
        for (_, b, other) in &stopped[index + 1..] {
            let overlap = (a.size + b.size).as_dvec2() * 0.5
                - (a.position - b.position).abs()
                - DVec2::splat(14.0);
            assert!(
                overlap.min_element() <= 0.0,
                "Settled cards overlap: {title} and {other}"
            );
        }
    }
    capture(&mut app, &directory, "01c-cooled");
    let upcoming = app
        .world_mut()
        .query::<(Entity, &ChildOf, &ScrollPosition)>()
        .iter(app.world())
        .find(|(_, parent, _)| parent.parent() == clock)
        .unwrap()
        .0;
    app.world_mut()
        .get_mut::<ScrollPosition>(upcoming)
        .unwrap()
        .0
        .y = 1000.0;
    settle(&mut app, 0.5);
    capture(&mut app, &directory, "01b-upcoming-scrolled");
    app.world_mut()
        .get_mut::<ScrollPosition>(upcoming)
        .unwrap()
        .0
        .y = 0.0;
    activate(app.world_mut(), clock, "Clock controls");
    settle(&mut app, 0.5);
    capture(&mut app, &directory, "02-controls");
    activate(app.world_mut(), clock, "Past tasks: on");
    activate(app.world_mut(), clock, "Hide clock controls");
    settle(&mut app, 2.0);
    assert_eq!(cards(app.world_mut(), clock).len(), 11);
    capture(&mut app, &directory, "03-past-disabled");
    activate(app.world_mut(), clock, "Clock controls");
    activate(app.world_mut(), clock, "Past tasks: off");
    activate(app.world_mut(), clock, "Hide clock controls");
    settle(&mut app, 10.0);
    let (card, item, _) = cards(app.world_mut(), clock)
        .into_iter()
        .find(|(_, _, title)| title == "Morning task")
        .unwrap();
    let from = screen(app.world(), item.position);
    cursor(&mut app, Some(from));
    capture(&mut app, &directory, "03b-past-restored");
    assert_eq!(
        app.world()
            .resource::<PointerState>()
            .hit
            .map(|(entity, _)| entity),
        Some(card)
    );
    button(&mut app, ButtonState::Pressed);
    button(&mut app, ButtonState::Released);
    settle(&mut app, 1.0);
    assert_eq!(
        app.world().get::<CanvasItem>(clock).unwrap().position,
        DVec2::ZERO
    );
    button(&mut app, ButtonState::Pressed);
    cursor(&mut app, Some(from + Vec2::new(90.0, -65.0)));
    assert_eq!(
        app.world()
            .resource::<PointerState>()
            .drag
            .map(|(entity, _)| entity),
        Some(card)
    );
    let held = app.world().get::<CanvasItem>(card).unwrap().position;
    settle(&mut app, 0.8);
    assert!(
        app.world()
            .get::<CanvasItem>(card)
            .unwrap()
            .position
            .distance(held)
            < 0.1
    );
    assert!(held.distance(item.position) > 75.0);
    assert_eq!(
        app.world().get::<CanvasItem>(clock).unwrap().position,
        DVec2::ZERO
    );
    capture(&mut app, &directory, "04-card-held");
    let corner = app.world().get::<CanvasItem>(clock).unwrap().position + DVec2::splat(195.0);
    let point = screen(app.world(), corner);
    cursor(&mut app, Some(point));
    cursor(&mut app, Some(point));
    assert!(
        app.world()
            .get::<CanvasItem>(card)
            .unwrap()
            .position
            .distance(corner)
            < 0.1
    );
    let hit = app.world().resource::<PointerState>().hit.unwrap().0;
    assert!(
        cards(app.world_mut(), clock)
            .iter()
            .any(|(card, _, _)| *card == hit)
    );
    capture(&mut app, &directory, "04b-card-over-transparent-corner");
    button(&mut app, ButtonState::Released);
    cursor(&mut app, None);
    settle(&mut app, 7.0);
    assert_eq!(
        app.world().get::<CanvasItem>(clock).unwrap().position,
        DVec2::ZERO
    );
    capture(&mut app, &directory, "05-card-released");
    lince_desktop::topology::groups::transform(
        app.world_mut(),
        clock,
        DVec3::new(75.0, 0.0, -45.0),
        DQuat::IDENTITY,
    );
    settle(&mut app, 0.2);
    capture(&mut app, &directory, "06-clock-moving");
    settle(&mut app, 20.0);
    capture(&mut app, &directory, "07-clock-settled");
    let before = cards(app.world_mut(), clock);
    app.world_mut().get_mut::<CanvasView>(root).unwrap().center = DVec2::new(900.0, 0.0);
    settle(&mut app, 2.0);
    let after = cards(app.world_mut(), clock);
    for (entity, item, _) in &before {
        let (_, next, _) = after.iter().find(|(next, _, _)| next == entity).unwrap();
        assert!(
            item.position.distance(next.position) < 5.0,
            "Camera panning moved a card"
        );
    }
    capture(&mut app, &directory, "08-camera-panned");
    app.world_mut().get_mut::<CanvasView>(root).unwrap().center = DVec2::ZERO;
    app.world_mut().get_mut::<CanvasView>(root).unwrap().zoom = 1.25;
    app.world_mut()
        .query::<&mut Window>()
        .single_mut(app.world_mut())
        .unwrap()
        .resolution
        .set_scale_factor_override(Some(2.0));
    let window = app
        .world_mut()
        .query::<&Window>()
        .single(app.world())
        .unwrap();
    let size = UVec2::new(window.physical_width(), window.physical_height());
    let RenderTarget::Image(mut target) = app.world().resource::<Output>().0.clone() else {
        unreachable!()
    };
    app.world_mut()
        .resource_mut::<Assets<Image>>()
        .get_mut(&target.handle)
        .unwrap()
        .resize(bevy::render::render_resource::Extent3d {
            width: size.x,
            height: size.y,
            depth_or_array_layers: 1,
        });
    target.scale_factor = 2.0;
    let target = RenderTarget::Image(target);
    let background = app
        .world()
        .resource::<lince_desktop::topology::presentation::BackgroundCamera>()
        .0;
    app.world_mut().entity_mut(camera).insert(target.clone());
    app.world_mut()
        .entity_mut(background)
        .insert(target.clone());
    app.world_mut().resource_mut::<Output>().0 = target;
    settle(&mut app, 2.0);
    assert!(app.world().get::<Surface>(clock).unwrap().density >= 2.5);
    capture(&mut app, &directory, "09-zoom-dpi");
    app.world_mut()
        .query::<&mut Window>()
        .single_mut(app.world_mut())
        .unwrap()
        .resolution
        .set_scale_factor_override(Some(1.0));
    app.world_mut().get_mut::<CanvasView>(root).unwrap().zoom = 1.0;
    let RenderTarget::Image(mut target) = app.world().resource::<Output>().0.clone() else {
        unreachable!()
    };
    target.scale_factor = 1.0;
    let target = RenderTarget::Image(target);
    app.world_mut().entity_mut(camera).insert(target.clone());
    app.world_mut()
        .entity_mut(background)
        .insert(target.clone());
    app.world_mut().resource_mut::<Output>().0 = target;
    activate(app.world_mut(), clock, "Clock controls");
    activate(app.world_mut(), clock, "Cards: floating");
    activate(app.world_mut(), clock, "Hide clock controls");
    cursor(&mut app, None);
    settle(&mut app, 1.0);
    assert!(cards(app.world_mut(), clock).is_empty());
    capture(&mut app, &directory, "10-hover-only-idle");
    let settings = &app.world().get::<TimeSettings>(clock).unwrap().0;
    let now = chrono::Utc::now().timestamp_millis();
    let point = settings.position(now, now, [420.0; 2], 0.0);
    let center = app.world().get::<CanvasItem>(clock).unwrap().position;
    let hover = screen(
        app.world(),
        center + DVec2::new(f64::from(point[0]), f64::from(point[2])),
    );
    cursor(&mut app, Some(hover));
    settle(&mut app, 1.0);
    assert_eq!(cards(app.world_mut(), clock).len(), 1);
    capture(&mut app, &directory, "11-hover-card");
    cursor(&mut app, None);
    settle(&mut app, 0.5);
    assert!(cards(app.world_mut(), clock).is_empty());
    activate(app.world_mut(), clock, "Clock controls");
    activate(app.world_mut(), clock, "Cards: on hover");
    activate(app.world_mut(), clock, "Card physics: on");
    activate(app.world_mut(), clock, "Hide clock controls");
    settle(&mut app, 1.0);
    assert!(
        !app.world()
            .get::<TimeSettings>(clock)
            .unwrap()
            .0
            .card_physics
    );
    capture(&mut app, &directory, "12-physics-disabled");
    for index in 0..4 {
        lince_desktop::topology::groups::transform(
            app.world_mut(),
            clock,
            DVec3::new(0.25, 0.0, 0.25),
            DQuat::IDENTITY,
        );
        step(&mut app);
        capture(&mut app, &directory, &format!("13-motion-{index}"));
    }
    activate(app.world_mut(), clock, "Clock controls");
    activate(app.world_mut(), clock, "Card physics: off");
    activate(app.world_mut(), clock, "Hide clock controls");
    let uid = engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: Some("karma-clock-visual".into()),
                kind: nucleus::RecordKind::Plain,
                head: "Karma scheduled task".into(),
                body: String::new(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    engine
        .act(
            engine::actions::Action::SetExtension {
                target: uid.clone(),
                namespace: "work".into(),
                fds: serde_json::json!({"estimate_min":3}),
            },
            None,
        )
        .await
        .unwrap();
    engine
        .act(
            engine::actions::Action::CreateRecurrence {
                target: uid,
                consequences: vec![nucleus::karma::Consequence::AddQuantity {
                    delta: Some(nucleus::DecimalValue::parse_inferred("-1").unwrap()),
                }],
                condition: None,
                gate: None,
                carry: None,
                note: None,
                cadence: nucleus::karma::Cadence::once(),
                anchor_at: Some((chrono::Utc::now() + chrono::TimeDelta::minutes(20)).to_rfc3339()),
                request_id: Some("karma-clock-visual".into()),
            },
            None,
        )
        .await
        .unwrap();
    let started = Instant::now();
    while !cards(app.world_mut(), clock)
        .iter()
        .any(|(_, _, title)| title == "Karma scheduled task")
    {
        assert!(
            started.elapsed().as_secs() < 60,
            "Karma schedule did not arrive"
        );
        step(&mut app);
    }
    capture(&mut app, &directory, "14-karma-added");
    settle(&mut app, 20.0);
    capture(&mut app, &directory, "15-karma-settled");
    let floss = engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: Some("floss-clock-visual".into()),
                kind: nucleus::RecordKind::Plain,
                head: "Passar Fio Dental".into(),
                body: String::new(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let due = chrono::DateTime::from_timestamp_millis(
        (chrono::Utc::now().timestamp_millis().div_euclid(1000) + 30) * 1000,
    )
    .unwrap();
    engine
        .act(
            engine::actions::Action::CreateRecurrence {
                target: floss.clone(),
                consequences: vec![nucleus::karma::Consequence::SetQuantity {
                    value: Some(nucleus::DecimalValue::parse_inferred("-1").unwrap()),
                }],
                condition: None,
                gate: None,
                carry: None,
                note: None,
                cadence: nucleus::karma::Cadence::every_days(1),
                anchor_at: Some(due.to_rfc3339()),
                request_id: Some("floss-clock-visual".into()),
            },
            None,
        )
        .await
        .unwrap();
    engine.advance_karma_time(chrono::Utc::now()).await.unwrap();
    let started = Instant::now();
    while !cards(app.world_mut(), clock)
        .iter()
        .any(|(_, _, title)| title == "Passar Fio Dental")
    {
        assert!(
            started.elapsed().as_secs() < 60,
            "Future floss need did not arrive"
        );
        step(&mut app);
    }
    assert!(
        chrono::Utc::now() < due,
        "Future need was not shown before its time"
    );
    capture(&mut app, &directory, "16-floss-future");
    while chrono::Utc::now() <= due {
        step(&mut app);
    }
    let started = Instant::now();
    while store::facts::level(&engine.store.pool, &floss)
        .await
        .unwrap()
        .is_zero()
    {
        assert!(
            started.elapsed().as_secs() < 10,
            "Daily Karma need did not execute"
        );
        engine.advance_karma_time(chrono::Utc::now()).await.unwrap();
        step(&mut app);
    }
    assert_eq!(
        store::facts::level(&engine.store.pool, &floss)
            .await
            .unwrap()
            .to_string(),
        "-1"
    );
    let started = Instant::now();
    loop {
        let matching: Vec<_> = cards(app.world_mut(), clock)
            .into_iter()
            .filter(|(_, _, title)| title == "Passar Fio Dental")
            .collect();
        if matching.len() == 1 && card_text(app.world(), matching[0].0, "Needed: 1") {
            break;
        }
        assert!(
            started.elapsed().as_secs() < 60,
            "Outstanding floss need did not reach the cursor"
        );
        step(&mut app);
    }
    settle(&mut app, 10.0);
    capture(&mut app, &directory, "17-floss-outstanding");
    engine
        .act(
            engine::actions::Action::AddQuantity {
                target: floss,
                delta: 1.0,
            },
            None,
        )
        .await
        .unwrap();
    let started = Instant::now();
    loop {
        let matching: Vec<_> = cards(app.world_mut(), clock)
            .into_iter()
            .filter(|(_, _, title)| title == "Passar Fio Dental")
            .collect();
        if matching
            .iter()
            .all(|(card, _, _)| !card_text(app.world(), *card, "Needed: 1"))
        {
            break;
        }
        assert!(
            started.elapsed().as_secs() < 60,
            "Satisfied need retained its outstanding quantity"
        );
        step(&mut app);
    }
    capture(&mut app, &directory, "18-floss-satisfied");
    activate(app.world_mut(), clock, "Clock controls");
    activate(app.world_mut(), clock, "Schedule source");
    settle(&mut app, 1.0);
    let editor = app
        .world_mut()
        .query_filtered::<Entity, With<lince_desktop::protein_castle::ProteinCastle>>()
        .single(app.world())
        .unwrap();
    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<lince_desktop::protein_castle::ProteinCastle>>()
            .iter(app.world())
            .count(),
        1
    );
    capture(&mut app, &directory, "19-source-editor");
    app.world_mut()
        .get_mut::<lince_desktop::protein_castle::ProteinCastle>(editor)
        .unwrap()
        .draft
        .query["where"] = serde_json::json!([{"all":[{"text_contains":"Passar Fio Dental"}]}]);
    activate(app.world_mut(), editor, "Apply schedule source");
    assert!(app.world().get_entity(editor).is_err());
    let started = Instant::now();
    while cards(app.world_mut(), clock).len() != 1 {
        assert!(
            started.elapsed().as_secs() < 60,
            "Configured source filter did not reach the clock"
        );
        step(&mut app);
    }
    assert_eq!(cards(app.world_mut(), clock)[0].2, "Passar Fio Dental");
    settle(&mut app, 2.0);
    capture(&mut app, &directory, "20-source-applied");
    println!(
        "PASS: schedule feed, past toggle, primary drag, release, clock movement, camera pan, DPI/zoom, hover cards, physics toggle, cooling, future Karma need becoming outstanding and satisfied, temporary source editor and applied filter"
    );
}

fn card_text(world: &World, card: Entity, value: &str) -> bool {
    world.get::<Children>(card).is_some_and(|children| {
        children
            .iter()
            .any(|child| world.get::<Text>(child).is_some_and(|text| text.0 == value))
    })
}
