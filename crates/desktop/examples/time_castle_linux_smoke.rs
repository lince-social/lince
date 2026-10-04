use bevy::{
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    winit::WinitSettings,
};
use lince_desktop::{
    actions::ActionButton,
    canvas::{CanvasItem, CanvasView},
    container::BoxRoot,
    sand_store::{SandKind, spawn_sand},
    sound_cues::Native,
    time_castle::TimeSettings,
    topology::presentation::Surface,
};
use std::{sync::Arc, time::Instant};

#[derive(Resource)]
struct Trial {
    minimized: bool,
    title: bool,
    count: usize,
    clock: Option<Entity>,
    frame: u32,
    stage: usize,
    started: Instant,
    last_frame: Instant,
    frame_started: Instant,
    frame_times: Vec<f64>,
    update_times: Vec<f64>,
    baseline: Option<(usize, usize, usize)>,
    finish_ms: i64,
    capturing: bool,
    captured: bool,
}

fn minimize_until(until: i64) {
    use x11rb::{
        connection::Connection,
        protocol::xproto::{AtomEnum, ClientMessageEvent, ConnectionExt, EventMask},
    };
    let (connection, screen) = x11rb::connect(None).unwrap();
    let root = connection.setup().roots[screen].root;
    let atom = |name: &[u8]| {
        connection
            .intern_atom(false, name)
            .unwrap()
            .reply()
            .unwrap()
            .atom
    };
    let name = atom(b"_NET_WM_NAME");
    let list = connection
        .get_property(
            false,
            root,
            atom(b"_NET_CLIENT_LIST"),
            AtomEnum::WINDOW,
            0,
            4096,
        )
        .unwrap()
        .reply()
        .unwrap();
    let window = list
        .value32()
        .unwrap()
        .find(|window| {
            connection
                .get_property(false, *window, name, AtomEnum::ANY, 0, 4096)
                .unwrap()
                .reply()
                .unwrap()
                .value
                == b"Time Castle Linux verification"
        })
        .unwrap();
    connection
        .send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_NOTIFY | EventMask::SUBSTRUCTURE_REDIRECT,
            ClientMessageEvent::new(32, window, atom(b"WM_CHANGE_STATE"), [3, 0, 0, 0, 0]),
        )
        .unwrap();
    connection.flush().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));
    let state = connection
        .get_property(false, window, atom(b"WM_STATE"), AtomEnum::ANY, 0, 2)
        .unwrap()
        .reply()
        .unwrap();
    assert_eq!(state.value32().unwrap().next(), Some(3));
    let minimized_at = chrono::Utc::now().timestamp_millis();
    println!("MINIMIZED state=Iconic at={minimized_at}; UI updates paused");
    while chrono::Utc::now().timestamp_millis() < until {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    connection.map_window(window).unwrap();
    connection.flush().unwrap();
    println!("RESTORED at={}", chrono::Utc::now().timestamp_millis());
}

fn cards(world: &mut World, clock: Entity) -> Vec<(CanvasItem, UVec2, f32)> {
    world
        .query::<(&ActionButton, &CanvasItem, &Surface, Option<&TimeSettings>)>()
        .iter(world)
        .filter(|(button, _, _, settings)| button.target == clock && settings.is_none())
        .map(|(_, item, surface, _)| (item.clone(), surface.pixels, surface.density))
        .collect()
}

fn report(world: &mut World, clock: Entity) {
    let cards = cards(world, clock);
    for (index, (item, _, _)) in cards.iter().enumerate() {
        for (other, _, _) in &cards[..index] {
            let delta = (item.position - other.position).abs().as_vec2();
            let extent = (item.size + other.size) * 0.5;
            assert!(delta.x >= extent.x || delta.y >= extent.y, "Cards overlap");
        }
    }
    let (viewport, scale) = {
        let mut windows = world.query::<&Window>();
        let window = windows.single(world).unwrap();
        (
            Vec2::new(window.width(), window.height()),
            window.scale_factor() as f32,
        )
    };
    let root = world.get::<ChildOf>(clock).unwrap().parent();
    let canvas = *world.get::<CanvasView>(root).unwrap();
    let half = viewport / canvas.zoom as f32 * 0.5;
    let on_screen = cards
        .iter()
        .filter(|(item, _, _)| {
            let delta = (item.position - canvas.center).abs().as_vec2();
            let extent = delta + item.size * 0.5;
            extent.x <= half.x && extent.y <= half.y
        })
        .count();
    let pixels: u64 = world
        .query::<&Surface>()
        .iter(world)
        .map(|surface| u64::from(surface.pixels.x) * u64::from(surface.pixels.y))
        .sum();
    assert!(
        pixels <= 32 * 1024 * 1024,
        "Capture pixel budget exceeded: {pixels}"
    );
    assert!(
        cards.iter().all(|(_, pixels, density)| pixels.x <= 4096
            && pixels.y <= 4096
            && density.is_finite())
    );
    let assets = (
        world.resource::<Assets<Image>>().len(),
        world.resource::<Assets<Mesh>>().len(),
        world.resource::<Assets<StandardMaterial>>().len(),
    );
    let font_atlases = world
        .resource::<bevy::text::FontAtlasSet>()
        .values()
        .map(Vec::len)
        .sum::<usize>();
    let font_bytes = world
        .resource::<bevy::text::FontAtlasSet>()
        .total_bytes(world.resource::<Assets<Image>>());
    let retained = (assets.0.saturating_sub(font_atlases), assets.1, assets.2);
    let sample = scale * canvas.zoom as f32;
    let undersampled = cards
        .iter()
        .filter(|(item, _, density)| {
            let delta = (item.position - canvas.center).abs().as_vec2();
            let extent = delta + item.size * 0.5;
            extent.x <= half.x && extent.y <= half.y && *density < sample
        })
        .count();
    assert_eq!(
        undersampled, 0,
        "Visible card text must sample at least one pixel per display pixel"
    );
    assert_eq!(
        world.get::<CanvasItem>(clock).unwrap().size,
        Vec2::splat(420.0)
    );
    assert!(world.get::<TimeSettings>(clock).unwrap().0.valid());
    let mut trial = world.resource_mut::<Trial>();
    let mut samples = trial.frame_times.clone();
    samples.sort_by(f64::total_cmp);
    let mut updates = trial.update_times.clone();
    updates.sort_by(f64::total_cmp);
    let percentile = |samples: &[f64], p: f64| {
        samples
            .get(((samples.len() as f64 * p).ceil() as usize).saturating_sub(1))
            .copied()
            .unwrap_or_default()
    };
    println!(
        "LOAD stage={} requested={} cards={} fully_visible={} samples={} p50_ms={:.2} p95_ms={:.2} update_p50_ms={:.2} update_p95_ms={:.2} capture_pixels={} assets={assets:?} font_atlases={font_atlases} font_bytes={font_bytes} scale={scale} zoom={}",
        trial.stage,
        trial.count,
        cards.len(),
        on_screen,
        samples.len(),
        percentile(&samples, 0.5),
        percentile(&samples, 0.95),
        percentile(&updates, 0.5),
        percentile(&updates, 0.95),
        pixels,
        canvas.zoom
    );
    if trial.stage == 0 {
        assert_eq!(cards.len(), trial.count);
        trial.baseline = Some(retained);
    }
    if trial.stage == 5 {
        let baseline = trial.baseline.unwrap();
        assert!(
            retained.0 <= baseline.0 + 10
                && retained.1 <= baseline.1 + 10
                && retained.2 <= baseline.2 + 10,
            "Restoring scale leaked retained assets: {baseline:?} -> {retained:?}"
        );
    }
}

fn exercise(world: &mut World) {
    {
        let mut trial = world.resource_mut::<Trial>();
        trial.frame += 1;
        let elapsed = trial.last_frame.elapsed().as_secs_f64() * 1000.0;
        if trial.started.elapsed().as_secs_f64() > 1.0 {
            trial.frame_times.push(elapsed);
            let update = trial.frame_started.elapsed().as_secs_f64() * 1000.0;
            trial.update_times.push(update);
        }
        trial.last_frame = Instant::now();
    }
    if world.resource::<Trial>().frame == 10 {
        if let Some(adapter) = world.get_resource::<bevy::render::renderer::RenderAdapterInfo>() {
            println!("ADAPTER {:?}", adapter.0);
        }
        let root = world
            .query_filtered::<Entity, With<BoxRoot>>()
            .single(world)
            .unwrap();
        let clock = spawn_sand(world, root, 1, SandKind::WorkTimer, "", DVec2::ZERO);
        let minimized = world.resource::<Trial>().minimized;
        if minimized {
            spawn_sand(
                world,
                root,
                1,
                SandKind::WorkTimer,
                "",
                DVec2::new(1000.0, 0.0),
            );
        }
        world.resource_mut::<Trial>().clock = Some(clock);
    }
    let Some(clock) = world.resource::<Trial>().clock else {
        return;
    };
    if world.resource::<Trial>().minimized {
        if world.resource::<Trial>().stage == 0 {
            let clocks: Vec<_> = world
                .query_filtered::<Entity, (With<TimeSettings>, With<CanvasItem>)>()
                .iter(world)
                .collect();
            let mode = if world.resource::<Trial>().title {
                lince_interface::sound::Mode::Title
            } else {
                lince_interface::sound::Mode::Blip
            };
            for owner in &clocks {
                let mut settings = world.get_mut::<TimeSettings>(*owner).unwrap();
                if settings.0.sound.mode != mode {
                    settings.0.sound.mode = mode;
                    settings.0.sound.volume = 20;
                }
            }
            if clocks.len() != 2 || clocks.iter().any(|owner| cards(world, *owner).len() != 3) {
                assert!(
                    chrono::Utc::now().timestamp_millis()
                        < world.resource::<Trial>().finish_ms - 10_000,
                    "Recurring projection did not arrive before the alerts"
                );
                return;
            }
            if world
                .get_resource::<Native>()
                .is_none_or(|native| native.next_ms.is_none())
            {
                return;
            }
            let next = world.resource::<Native>().next_ms.unwrap();
            assert!(
                next > chrono::Utc::now().timestamp_millis() + 1000,
                "Alert was not armed early enough"
            );
            let until = world.resource::<Trial>().finish_ms;
            minimize_until(until);
            world.resource_mut::<Native>().poll();
            let native = world.resource::<Native>();
            assert!(native.error.is_none(), "{:?}", native.error);
            assert_eq!(
                native.started, 3,
                "All three scheduled and recurring alerts must fire while minimized: {:?}",
                native.recent
            );
            println!("MINIMIZED ALERTS {:?}", native.recent);
            let title = world.resource::<Trial>().title;
            assert!(
                native.recent.iter().enumerate().all(|(index, event)| {
                    event.started_ms - event.at_ms < if title && index > 0 { 5000 } else { 500 }
                }),
                "Idle playback must start promptly; queued titles must follow in order"
            );
            assert_eq!(
                native.recent.iter().filter(|event| event.projected).count(),
                1
            );
            world.resource_mut::<Trial>().stage = 1;
            world.resource_mut::<Trial>().started = Instant::now();
            return;
        }
        if world.resource::<Trial>().started.elapsed().as_secs() >= 3 {
            let native = world.resource::<Native>();
            assert_eq!(native.started, 3, "Restoring must not replay alerts");
            assert!(native.error.is_none(), "{:?}", native.error);
            if world.resource::<Trial>().title {
                assert_eq!(native.completed, 3, "Every spoken title must complete");
            }
            println!(
                "MINIMIZED PASS starts={} completed={}",
                native.started, native.completed
            );
            world.write_message(AppExit::Success);
        }
        return;
    }
    if world.resource::<Trial>().capturing && !world.resource::<Trial>().captured {
        return;
    }
    if !world.resource::<Trial>().captured
        && (cards(world, clock).is_empty()
            || world.resource::<Trial>().started.elapsed().as_secs() < 4)
    {
        assert!(
            world.resource::<Trial>().frame < 10000,
            "Schedule failed to arrive"
        );
        return;
    }
    let stage = world.resource::<Trial>().stage;
    if stage == 5 && world.resource::<Trial>().captured {
        return;
    }
    if !world.resource::<Trial>().captured {
        report(world, clock);
        let count = world.resource::<Trial>().count;
        let capture = world
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(format!(
                "/tmp/time-castle-linux-{count}-{stage}.png"
            )))
            .observe(|_: On<ScreenshotCaptured>, mut trial: ResMut<Trial>| {
                trial.captured = true;
            })
            .id();
        if stage == 5 {
            world.entity_mut(capture).observe(
                |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                    println!("LOAD PASS all six stages captured");
                    exit.write(AppExit::Success);
                },
            );
        }
        world.resource_mut::<Trial>().capturing = true;
        return;
    }
    let root = world.get::<ChildOf>(clock).unwrap().parent();
    match stage {
        0 => {
            world
                .get_mut::<TimeSettings>(clock)
                .unwrap()
                .0
                .set_aperture(36_000_000);
        }
        1 => {
            world
                .get_mut::<TimeSettings>(clock)
                .unwrap()
                .0
                .set_aperture(3_600_000);
            world.get_mut::<CanvasView>(root).unwrap().zoom = 0.75;
            world
                .query::<&mut Window>()
                .single_mut(world)
                .unwrap()
                .resolution
                .set_scale_factor_override(Some(1.5));
        }
        2 => {
            world.get_mut::<CanvasView>(root).unwrap().zoom = 1.25;
            world
                .query::<&mut Window>()
                .single_mut(world)
                .unwrap()
                .resolution
                .set_scale_factor_override(Some(2.0));
        }
        3 => {
            world
                .get_mut::<TimeSettings>(clock)
                .unwrap()
                .0
                .set_aperture(60_000);
        }
        4 => {
            world
                .get_mut::<TimeSettings>(clock)
                .unwrap()
                .0
                .set_aperture(3_600_000);
            world.get_mut::<CanvasView>(root).unwrap().zoom = 1.0;
            world
                .query::<&mut Window>()
                .single_mut(world)
                .unwrap()
                .resolution
                .set_scale_factor_override(None);
        }
        _ => unreachable!(),
    }
    let mut trial = world.resource_mut::<Trial>();
    trial.stage += 1;
    trial.capturing = false;
    trial.captured = false;
    trial.started = Instant::now();
    trial.frame_times.clear();
    trial.update_times.clear();
}

async fn task(
    engine: &engine::Engine,
    title: String,
    start: chrono::DateTime<chrono::Utc>,
    estimate: Option<f64>,
) -> String {
    let uid = engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: title,
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
                fds: serde_json::json!({"start":start.to_rfc3339(), "estimate_min":estimate}),
            },
            None,
        )
        .await
        .unwrap();
    uid
}

#[tokio::main]
async fn main() {
    let directory = tempfile::tempdir().unwrap();
    let mode = std::env::args().nth(1).unwrap_or_else(|| "load".into());
    let minimized = mode.starts_with("minimized");
    let title = mode.ends_with("title");
    let count: usize = std::env::args()
        .nth(2)
        .and_then(|value| value.parse().ok())
        .unwrap_or(48);
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
    let signer =
        engine::trust::Signer::generate(&organ.uid, &engine::roster::cell_key_id(&cell.uid));
    engine.set_signer(signer.clone()).await.unwrap();
    engine.publish_root_key(&root).await.unwrap();
    engine
        .publish_roster(
            &root,
            vec![engine::roster::CellEntry {
                cell_uid: cell.uid,
                node_id: "linux-clock-smoke".into(),
                label: "Linux clock smoke".into(),
                operational_key: signer.public_key_b64(),
                sealing_key: None,
                front_door: false,
                capabilities: engine::roster::full_capabilities(),
            }],
        )
        .await
        .unwrap();
    let now = chrono::Utc::now();
    if minimized {
        task(
            &engine,
            "Scheduled point".into(),
            now + chrono::TimeDelta::seconds(24),
            None,
        )
        .await;
        task(
            &engine,
            "Scheduled range".into(),
            now + chrono::TimeDelta::seconds(27),
            Some(1.0),
        )
        .await;
        let stock = engine
            .act(
                engine::actions::Action::CreateRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: "Recurring work".into(),
                    body: String::new(),
                    quantity: 1.0,
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
                    target: stock.clone(),
                    namespace: "work".into(),
                    fds: serde_json::json!({"estimate_min":1}),
                },
                None,
            )
            .await
            .unwrap();
        engine
            .act(
                engine::actions::Action::CreateFrequency {
                    slug: "linux-smoke".into(),
                    head: None,
                    every: nucleus::karma::CadenceStep {
                        days: 1,
                        ..Default::default()
                    },
                    anchor_at: Some((now + chrono::TimeDelta::seconds(30)).to_rfc3339()),
                    request_id: None,
                },
                None,
            )
            .await
            .unwrap();
        engine
            .act(
                engine::actions::Action::CreateRecurrence {
                    target: stock,
                    consequences: vec![nucleus::karma::Consequence::AddQuantity {
                        delta: Some(nucleus::fact::zero_delta()),
                    }],
                    condition: Some("freq(@linux-smoke)".into()),
                    gate: Some("!=0".into()),
                    carry: Some("value".into()),
                    note: None,
                    cadence: nucleus::karma::Cadence::every_days(1),
                    anchor_at: Some(now.to_rfc3339()),
                    request_id: None,
                },
                None,
            )
            .await
            .unwrap();
    } else {
        for index in 0..count {
            let offset = 30 + (index / 3) as i64 * 2400 / (count / 3).max(1) as i64;
            let estimate = (index % 3 != 0).then_some(8.0 + (index % 5) as f64);
            task(
                &engine,
                format!("Load {index:03} · café ação 日本語 — review the scheduled work"),
                now + chrono::TimeDelta::seconds(offset),
                estimate,
            )
            .await;
        }
    }
    let director = minimized.then(|| {
        engine.clone().start_karma_deadline_director(
            engine::karma_runtime::KarmaDeadlineDirectorConfig::for_host(
                "linux-clock-smoke".into(),
            )
            .unwrap(),
        )
    });
    let exit = lince_desktop::app::interface_app()
        .add_plugins((
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
            directory.path().join("interface.json"),
        ))
        .insert_resource(Trial {
            minimized,
            title,
            count,
            clock: None,
            frame: 0,
            stage: 0,
            started: Instant::now(),
            last_frame: Instant::now(),
            frame_started: Instant::now(),
            frame_times: Vec::new(),
            update_times: Vec::new(),
            baseline: None,
            finish_ms: now.timestamp_millis() + 33_000,
            capturing: false,
            captured: false,
        })
        .insert_resource(WinitSettings::continuous())
        .add_systems(
            Startup,
            |mut commands: Commands, mut windows: Query<&mut Window>| {
                let mut window = windows.single_mut().unwrap();
                window.title = "Time Castle Linux verification".into();
                window.resolution.set(3600.0, 2000.0);
                commands.spawn(BoxRoot);
            },
        )
        .add_systems(Last, exercise)
        .add_systems(First, |mut trial: ResMut<Trial>| {
            trial.frame_started = Instant::now();
        })
        .run();
    if let Some(director) = director {
        director.abort();
    }
    engine.store.pool.close().await;
    assert!(exit.is_success(), "Native verification exited {exit:?}");
}
