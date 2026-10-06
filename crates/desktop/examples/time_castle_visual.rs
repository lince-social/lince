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
    let started = Instant::now();
    while cards(app.world_mut(), clock).len() != 13 {
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
    settle(&mut app, 4.0);
    let (card, item, _) = cards(app.world_mut(), clock)
        .into_iter()
        .find(|(_, _, title)| title == "Morning task")
        .unwrap();
    let from = screen(app.world(), item.position);
    cursor(&mut app, Some(from));
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
                quantity: -1.0,
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
                    delta: Some(nucleus::fact::zero_delta()),
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
    println!(
        "PASS: schedule feed, past toggle, primary drag, release, clock movement, camera pan, DPI/zoom, hover cards, physics toggle, motion captures"
    );
}
