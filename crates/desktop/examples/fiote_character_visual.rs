use bevy::{
    camera::RenderTarget,
    prelude::*,
    render::{
        render_resource::{PollType, TextureFormat},
        renderer::RenderDevice,
        view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    },
};
use lince_desktop::fiote::character::{Appearance, Avatar};
use lince_desktop::tokens::{ColorScheme, ThemeSettings, Token, TokenOverrides, TokenValue};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Resource)]
struct Output(RenderTarget);

#[derive(Resource, Default)]
struct Captured(bool);

fn output(world: &mut World) {
    let handle = world
        .resource_mut::<Assets<Image>>()
        .add(Image::new_target_texture(
            800,
            640,
            TextureFormat::Bgra8UnormSrgb,
            None,
        ));
    let target = RenderTarget::Image(handle.into());
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

fn frame(app: &mut App) -> Duration {
    let start = Instant::now();
    app.update();
    app.world()
        .resource::<RenderDevice>()
        .wgpu_device()
        .poll(PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .unwrap();
    start.elapsed()
}

fn step(app: &mut App) {
    frame(app);
    std::thread::sleep(Duration::from_millis(34));
}

fn memory_bytes() -> Option<u64> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|line| {
            line.strip_prefix("VmRSS:")
                .and_then(|value| value.split_whitespace().next()?.parse::<u64>().ok())
                .map(|value| value * 1024)
        })
}

fn benchmark(app: &mut App, name: &str) -> serde_json::Value {
    for _ in 0..12 {
        step(app);
    }
    let mut samples = Vec::new();
    for _ in 0..90 {
        samples.push(frame(app).as_secs_f64() * 1000.0);
        std::thread::sleep(Duration::from_millis(34));
    }
    samples.sort_by(f64::total_cmp);
    serde_json::json!({
        "case": name,
        "samples": samples.len(),
        "median_frame_ms": samples[samples.len() / 2],
        "p95_frame_ms": samples[samples.len() * 95 / 100],
        "process_rss_bytes": memory_bytes(),
        "character": lince_desktop::fiote::character::metrics(app.world_mut()),
    })
}

fn ready(app: &mut App, scenes: usize) {
    let start = Instant::now();
    loop {
        step(app);
        let players = app
            .world_mut()
            .query::<&AnimationPlayer>()
            .iter(app.world())
            .filter(|player| player.playing_animations().next().is_some())
            .count();
        if players == scenes {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "Animated Fiote failed to load"
        );
    }
    for _ in 0..8 {
        step(app);
    }
}

fn capture(app: &mut App, path: PathBuf) {
    app.world_mut().resource_mut::<Captured>().0 = false;
    let target = app.world().resource::<Output>().0.clone();
    app.world_mut()
        .spawn(Screenshot(target))
        .observe(save_to_disk(path))
        .observe(
            |event: On<ScreenshotCaptured>, mut captured: ResMut<Captured>| {
                let pixels = event.image.data.as_ref().unwrap();
                let width = event.image.width() as usize;
                let mut colors = std::collections::HashSet::new();
                for y in 200..450 {
                    for x in 270..510 {
                        let offset = (y * width + x) * 4;
                        colors.insert([pixels[offset], pixels[offset + 1], pixels[offset + 2]]);
                    }
                }
                assert!(colors.len() > 64, "The character portrait is blank");
                captured.0 = true;
            },
        );
    let start = Instant::now();
    while !app.world().resource::<Captured>().0 {
        assert!(start.elapsed() < Duration::from_secs(30));
        step(app);
    }
}

fn activate(app: &mut App, caption: &str) {
    let button = app
        .world_mut()
        .query::<(Entity, &lince_desktop::icons::Tooltip)>()
        .iter(app.world())
        .find(|(_, tooltip)| tooltip.0 == caption)
        .map(|(entity, _)| entity)
        .unwrap();
    app.world_mut()
        .trigger(bevy::ui_widgets::Activate { entity: button });
    step(app);
}

#[tokio::main]
async fn main() {
    let directory = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "target/fiote-refinement".into()),
    );
    std::fs::create_dir_all(&directory).unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let mut app = lince_desktop::app::offscreen_interface_app();
    app.insert_resource(lince_desktop::workspace::WorkspaceFile::new(
        workspace.path().join("interface.json"),
    ))
    .init_resource::<Captured>()
    .add_systems(PostStartup, output);
    app.finish();
    app.cleanup();
    step(&mut app);
    let window = app
        .world_mut()
        .query::<Entity>()
        .iter(app.world())
        .find(|entity| app.world().get::<Window>(*entity).is_some())
        .unwrap();
    app.world_mut().get_mut::<Window>(window).unwrap().focused = true;
    let root = app
        .world_mut()
        .spawn(Node {
            width: percent(100),
            height: percent(100),
            ..default()
        })
        .id();
    let owner = lince_desktop::fiote::spawn(app.world_mut(), root);
    let bubble = app
        .world()
        .get::<lince_desktop::fiote::Fiote>(owner)
        .unwrap()
        .bubble;
    let typography = app
        .world()
        .resource::<lince_desktop::theme::Typography>()
        .text(18.0);
    app.world_mut().spawn((
        Text::new("Hello, I’m Fiote."),
        typography,
        TextColor(Color::srgb(0.95, 0.95, 0.95)),
        ChildOf(bubble),
    ));
    let content = app
        .world_mut()
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: px(16),
                left: px(16),
                right: px(16),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                ..default()
            },
            ChildOf(root),
        ))
        .id();
    let tabs = app
        .world_mut()
        .spawn((Node::default(), ChildOf(content)))
        .id();
    lince_desktop::fiote::character::settings_button(app.world_mut(), tabs, content);
    activate(&mut app, "Fiote appearance");
    let image = app
        .world_mut()
        .query_filtered::<&ImageNode, With<Avatar>>()
        .single(app.world())
        .unwrap()
        .image
        .clone();
    let large = app
        .world_mut()
        .spawn((
            Avatar,
            ImageNode::new(image),
            Node {
                position_type: PositionType::Absolute,
                left: px(250),
                top: px(180),
                width: px(280),
                height: px(280),
                ..default()
            },
            ChildOf(root),
        ))
        .id();
    for _ in 0..12 {
        step(&mut app);
    }
    capture(&mut app, directory.join("static.png"));
    for entity in app
        .world_mut()
        .query_filtered::<Entity, With<Avatar>>()
        .iter(app.world())
        .collect::<Vec<_>>()
    {
        assert_eq!(
            app.world().get::<Node>(entity).unwrap().border,
            UiRect::ZERO
        );
        assert!(
            app.world()
                .get::<Outline>(entity)
                .is_none_or(|outline| outline.width == px(0))
        );
    }
    activate(&mut app, "Animated");
    assert_eq!(
        lince_desktop::fiote::character::preferences(app.world()).appearance,
        Appearance::Animated
    );
    ready(&mut app, 1);
    assert_eq!(
        lince_desktop::fiote::character::metrics(app.world_mut()).motion_materials,
        8
    );
    for _ in 0..12 {
        step(&mut app);
    }
    capture(&mut app, directory.join("animated.png"));
    for _ in 0..150 {
        step(&mut app);
    }
    capture(&mut app, directory.join("animated-later.png"));
    let image_before = app
        .world_mut()
        .query_filtered::<&ImageNode, With<Avatar>>()
        .iter(app.world())
        .next()
        .unwrap()
        .image
        .clone();
    app.world_mut().resource_mut::<ThemeSettings>().scheme = ColorScheme::Light;
    for _ in 0..12 {
        step(&mut app);
    }
    assert_eq!(
        app.world_mut()
            .query_filtered::<&ImageNode, With<Avatar>>()
            .iter(app.world())
            .next()
            .unwrap()
            .image,
        image_before
    );
    assert_eq!(
        lince_desktop::fiote::character::metrics(app.world_mut()).animated_scenes,
        1
    );
    capture(&mut app, directory.join("animated-light.png"));
    app.world_mut().resource_mut::<ThemeSettings>().scheme = ColorScheme::Dark;
    app.world_mut()
        .entity_mut(large)
        .insert(TokenOverrides(std::collections::BTreeMap::from([
            (Token::SandBackground, TokenValue::Color([25, 58, 48, 255])),
            (Token::SandInk, TokenValue::Color([218, 235, 215, 255])),
            (Token::Accent, TokenValue::Color([218, 146, 63, 255])),
        ])));
    ready(&mut app, 2);
    assert_eq!(
        lince_desktop::fiote::character::metrics(app.world_mut()).animated_scenes,
        2
    );
    capture(&mut app, directory.join("animated-local-override.png"));
    app.world_mut().entity_mut(large).remove::<TokenOverrides>();
    ready(&mut app, 1);
    activate(&mut app, "Pause / resume");
    for _ in 0..8 {
        step(&mut app);
    }
    assert_eq!(
        app.world_mut()
            .query::<&AnimationPlayer>()
            .iter(app.world())
            .count(),
        0
    );
    activate(&mut app, "Pause / resume");
    activate(&mut app, "Reduced motion on / off");
    for _ in 0..8 {
        step(&mut app);
    }
    assert_eq!(
        app.world_mut()
            .query::<&AnimationPlayer>()
            .iter(app.world())
            .count(),
        0
    );
    assert_eq!(
        app.world()
            .get::<lince_desktop::fiote::Fiote>(owner)
            .unwrap()
            .bubble,
        bubble
    );
    capture(&mut app, directory.join("reduced-motion.png"));
    app.world_mut().get_mut::<Node>(large).unwrap().display = Display::None;
    let mut reports = Vec::new();
    activate(&mut app, "Static");
    reports.push(benchmark(&mut app, "static_one"));
    activate(&mut app, "Reduced motion on / off");
    activate(&mut app, "Animated");
    ready(&mut app, 1);
    reports.push(benchmark(&mut app, "animated_one"));
    let image = app
        .world_mut()
        .query_filtered::<&ImageNode, With<Avatar>>()
        .iter(app.world())
        .next()
        .unwrap()
        .image
        .clone();
    let mut additional = Vec::new();
    for index in 0..15 {
        let entity = app
            .world_mut()
            .spawn((
                Avatar,
                ImageNode::new(image.clone()),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(20 + (index % 8) * 88),
                    top: px(260 + (index / 8) * 100),
                    width: px(56),
                    height: px(56),
                    ..default()
                },
                ChildOf(root),
            ))
            .id();
        additional.push(entity);
    }
    ready(&mut app, 1);
    let metrics = lince_desktop::fiote::character::metrics(app.world_mut());
    assert_eq!(metrics.animated_scenes, 1);
    reports.push(benchmark(&mut app, "animated_sixteen_shared"));
    app.world_mut()
        .entity_mut(additional[0])
        .insert(TokenOverrides(std::collections::BTreeMap::from([(
            Token::SandBackground,
            TokenValue::Color([50, 28, 55, 255]),
        )])));
    ready(&mut app, 2);
    reports.push(benchmark(&mut app, "animated_sixteen_two_palettes"));
    activate(&mut app, "Static");
    for _ in 0..12 {
        step(&mut app);
    }
    assert_eq!(
        lince_desktop::fiote::character::metrics(app.world_mut()).animated_scenes,
        0
    );
    reports.push(benchmark(&mut app, "static_sixteen"));
    let adapter = format!(
        "{:?}",
        app.world()
            .resource::<bevy::render::renderer::RenderAdapterInfo>()
            .0
    );
    let report = serde_json::json!({ "adapter": adapter, "measurement": "Application update and GPU completion; excludes frame pacing; process RSS includes the entire native fixture", "cases": reports });
    std::fs::write(
        directory.join("performance.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("{report}");
    println!(
        "Fiote static, animated, reduced-motion, and bubble-preservation checks passed; captures in {}",
        directory.display()
    );
}
