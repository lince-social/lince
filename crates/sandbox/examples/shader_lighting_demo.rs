use bevy::{
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    winit::WinitSettings,
};
use lince_sandbox::{
    canvas::{CanvasItem, CanvasView},
    container::BoxRoot,
    topology::{
        Spatial,
        appearance::{Light, Material},
        view::View,
    },
    workspace::{WorkspaceMember, Workspaces},
};
use nucleus::canvas::appearance as model;
use std::time::{Duration, Instant};

#[derive(Resource)]
struct Demo {
    started: Instant,
    stage: usize,
    capturing: bool,
    root: Option<Entity>,
    emitter: Option<Entity>,
    brightness: Option<u64>,
}

#[derive(Component)]
struct WgslSurface;

fn sand(
    world: &mut World,
    root: Entity,
    title: &str,
    position: DVec2,
    size: Vec2,
    elevation: f64,
    material: model::Material,
) -> Entity {
    let owner = world
        .spawn((
            lince_sandbox::sand::Square,
            CanvasItem { position, size },
            WorkspaceMember(1),
            ChildOf(root),
            Spatial {
                elevation,
                depth: Some(45.0),
                ..default()
            },
            Node {
                position_type: PositionType::Absolute,
                width: px(size.x),
                height: px(size.y),
                padding: UiRect::all(px(16)),
                border_radius: BorderRadius::all(px(24)),
                ..default()
            },
            BackgroundColor(Color::BLACK),
            Material(material),
        ))
        .id();
    let typography = world
        .resource::<lince_sandbox::theme::Typography>()
        .text(20.0);
    world.spawn((
        Text::new(title),
        typography,
        TextColor(Color::WHITE),
        ChildOf(owner),
    ));
    owner
}

fn exercise(world: &mut World) {
    assert!(
        world.resource::<Demo>().started.elapsed() < Duration::from_secs(90),
        "Lighting demonstration timed out"
    );
    let root = world
        .query_filtered::<Entity, With<BoxRoot>>()
        .single(world)
        .unwrap();
    if world.resource::<Demo>().root.is_none() {
        if world.get::<Workspaces>(root).is_none() {
            return;
        }
        sand(
            world,
            root,
            "Lit tile surface · text stays unlit",
            DVec2::ZERO,
            Vec2::new(850.0, 560.0),
            -45.0,
            model::Material {
                lit: true,
                color: [0.7, 0.75, 0.8, 1.0],
                texture: model::Texture::Tiles,
                repeat: [4.0, 3.0],
                ..default()
            },
        );
        sand(
            world,
            root,
            "Shadow caster",
            DVec2::new(0.0, 20.0),
            Vec2::new(130.0, 130.0),
            70.0,
            model::Material {
                lit: true,
                color: [0.2, 0.5, 0.9, 1.0],
                texture: model::Texture::Brushed,
                ..default()
            },
        );
        let emitter = sand(
            world,
            root,
            "Glowing Sand\nAttached light",
            DVec2::new(-260.0, -90.0),
            Vec2::new(180.0, 140.0),
            45.0,
            model::Material {
                lit: true,
                color: [0.9, 0.65, 0.25, 1.0],
                emissive: 0.9,
                ..default()
            },
        );
        world.entity_mut(emitter).insert(Light(model::Light {
            position: [0.0, 140.0, 0.0],
            color: [1.0, 0.55, 0.15],
            intensity: 80000.0,
            range: 1800.0,
            shadows: model::ShadowQuality::High,
            ..default()
        }));
        let shader = sand(
            world,
            root,
            "WGSL surface\nLit image, unlit text",
            DVec2::new(250.0, -90.0),
            Vec2::new(210.0, 180.0),
            25.0,
            model::Material {
                lit: true,
                color: [0.12, 0.18, 0.3, 1.0],
                emissive: 0.2,
                ..default()
            },
        );
        let effect = nucleus::shader_effect::Effect {
            programs: [("surface".into(), "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { return vec4<f32>(0.2 + uv.x * 0.6, 0.1, 0.9, 1.0); }".into())].into(),
            rules: vec![], fallback: "surface".into(), ..default()
        };
        world.entity_mut(shader).insert((
            WgslSurface,
            lince_sandbox::shader_effects::Surface {
                effect,
                motion: false,
                glow: 0.0,
                ..default()
            },
        ));
        world
            .get_resource_or_init::<GlobalAmbientLight>()
            .brightness = 20.0;
        world.resource_mut::<Demo>().root = Some(root);
        world.resource_mut::<Demo>().emitter = Some(emitter);
        world.resource_mut::<Demo>().started = Instant::now();
        return;
    }
    if world.resource::<Demo>().capturing
        || world.resource::<Demo>().started.elapsed() < Duration::from_secs(5)
    {
        return;
    }
    let stage = world.resource::<Demo>().stage;
    let scene = world
        .resource::<lince_sandbox::topology::presentation::SceneCamera>()
        .0;
    let face = world
        .query_filtered::<&lince_sandbox::topology::presentation::Surface, With<WgslSurface>>()
        .single(world)
        .unwrap()
        .face;
    let center = world.get::<GlobalTransform>(face).unwrap().translation();
    let camera = world.get::<Camera>(scene).unwrap();
    let camera_transform = world.get::<GlobalTransform>(scene).unwrap();
    let probe = camera.world_to_viewport(camera_transform, center).unwrap()
        * world
            .query::<&Window>()
            .single(world)
            .unwrap()
            .resolution
            .scale_factor();
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/shader-lighting".into());
    std::fs::create_dir_all(&path).unwrap();
    world.resource_mut::<Demo>().capturing = true;
    world
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(format!("{path}/lighting-{stage}.png")))
        .observe(
            move |event: On<ScreenshotCaptured>,
                  mut demo: ResMut<Demo>,
                  mut commands: Commands,
                  mut lights: Query<&mut Light>| {
                let image = event.image.clone().try_into_dynamic().unwrap().to_rgba8();
                let pixel = image.get_pixel(probe.x as u32, probe.y as u32);
                assert!(pixel[2] > 45 && pixel[2] < 165 && pixel[2] > pixel[1], "Expected a lit WGSL image with no duplicate unlit copy at {probe:?}, got {pixel:?}");
                let brightness: u64 = image
                    .pixels()
                    .map(|pixel| u64::from(pixel[0]) + u64::from(pixel[1]) + u64::from(pixel[2]))
                    .sum();
                eprintln!("Lighting stage {stage}: total RGB {brightness}");
                let root = demo.root.unwrap();
                let emitter = demo.emitter.unwrap();
                match stage {
                    0 => {
                        demo.brightness = Some(brightness);
                        lights.get_mut(emitter).unwrap().0.shadows = model::ShadowQuality::Disabled
                    }
                    1 => {
                        assert!(
                            brightness > demo.brightness.unwrap() + 500,
                            "Removing shadows must brighten the receiving surface"
                        );
                        lights.get_mut(emitter).unwrap().0.shadows = model::ShadowQuality::High;
                        commands.entity(root).insert(View {
                            spatial: true,
                            position: [620.0, 650.0, 850.0],
                            yaw: 0.63,
                            pitch: -0.55,
                            ..default()
                        });
                    }
                    2 => {
                        demo.brightness = Some(brightness);
                        lights.get_mut(emitter).unwrap().0.enabled = false;
                    }
                    _ => {
                        assert!(
                            brightness < demo.brightness.unwrap(),
                            "Disabling the light must darken the scene"
                        );
                        commands.write_message(AppExit::Success);
                    }
                }
                demo.stage += 1;
                demo.started = Instant::now();
                demo.capturing = false;
            },
        );
}

fn main() -> AppExit {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _guard = runtime.enter();
    lince_sandbox::app::interface_app()
        .add_plugins(bevy::log::LogPlugin::default())
        .insert_resource(WinitSettings::continuous())
        .insert_resource(Demo {
            started: Instant::now(),
            stage: 0,
            capturing: false,
            root: None,
            emitter: None,
            brightness: None,
        })
        .add_systems(
            Startup,
            |mut commands: Commands, mut windows: Query<&mut Window>| {
                let mut window = windows.single_mut().unwrap();
                window.title = "Sand lighting · 2D shadows → 3D materials → Disable".into();
                window.resolution.set(1200.0, 850.0);
                commands.spawn((BoxRoot, CanvasView::default()));
            },
        )
        .add_systems(
            Update,
            exercise.after(lince_sandbox::topology::appearance::update),
        )
        .run()
}
