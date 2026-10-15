use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    winit::WinitSettings,
};
use lince_sandbox::{protein_area::Source, shader_effects::Surface};
use nucleus::shader_effect::{Effect, Extension, NAMESPACE};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Resource)]
struct Smoke {
    uid: String,
    started: Instant,
    stage: u8,
    captured: bool,
    changed: bool,
    setup: bool,
}

#[derive(Component)]
struct SolidSurface;

fn setup(world: &mut World) {
    if world.resource::<Smoke>().setup {
        return;
    }
    world
        .query::<&mut Window>()
        .single_mut(world)
        .unwrap()
        .resolution
        .set(1000.0, 700.0);
    let root = world
        .query_filtered::<Entity, With<lince_sandbox::container::BoxRoot>>()
        .single(world)
        .unwrap();
    let uid = world.resource::<Smoke>().uid.clone();
    let area = lince_sandbox::full_record::open(world, root, &uid, Source::Local).unwrap();
    let mut area = world
        .get_mut::<lince_sandbox::area::InfluenceArea>(area)
        .unwrap();
    let config = area.protein.as_mut().unwrap();
    config.shader_effects = true;
    config
        .bindings
        .retain(|binding| matches!(binding.property.as_str(), "head" | "quantity"));
    let solid = Effect { version: 1, mode: Default::default(), programs: [("red".into(), "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { return vec4<f32>(0.85, 0.02, 0.02, 1.0); }".into())].into(), rules: vec![], fallback: "red".into(), inputs: Default::default(), bindings: vec![] };
    let entity = world
        .spawn((
            SolidSurface,
            lince_sandbox::sand::Square,
            ChildOf(root),
            Node {
                position_type: PositionType::Absolute,
                left: px(24),
                top: px(24),
                width: px(220),
                height: px(160),
                ..default()
            },
            BackgroundColor(Color::BLACK),
            Surface {
                effect: solid,
                glow: 0.0,
                motion: false,
                ..default()
            },
        ))
        .id();
    let font = world
        .resource::<lince_sandbox::theme::Typography>()
        .text(14.0);
    world.spawn((
        Text::new("Readable surface text"),
        font,
        TextColor(Color::WHITE),
        Node {
            padding: UiRect::all(px(8)),
            ..default()
        },
        ChildOf(entity),
    ));
    world.resource_mut::<Smoke>().setup = true;
}

fn inspect(world: &mut World) {
    assert!(
        world.resource::<Smoke>().started.elapsed() < Duration::from_secs(60),
        "Effect rendering timed out"
    );
    if world.resource::<Smoke>().captured {
        return;
    }
    let stage = world.resource::<Smoke>().stage;
    if stage == 1 && !world.resource::<Smoke>().changed {
        let uid = world.resource::<Smoke>().uid.clone();
        let sender = &world
            .get_non_send::<lince_sandbox::cell_bridge::CellBridge>()
            .unwrap()
            .outgoing;
        sender
            .try_send(cell::ClientMessage::Act {
                id: "shader-smoke-quantity".into(),
                action: engine::actions::Action::SetQuantity {
                    target: uid,
                    value: 0.0,
                },
            })
            .unwrap();
        world.resource_mut::<Smoke>().changed = true;
        world.resource_mut::<Smoke>().started = Instant::now();
        return;
    }
    if world.resource::<Smoke>().started.elapsed() < Duration::from_secs(5) {
        return;
    }
    let mut samples = Vec::new();
    let camera = world.query_filtered::<(&Camera, &GlobalTransform), With<lince_sandbox::topology::presentation::WorldCamera>>().iter(world).next().map(|(camera, transform)| (camera.clone(), *transform));
    for (entity, image, node, transform) in world
        .query::<(Entity, &ImageNode, &ComputedNode, &UiGlobalTransform)>()
        .iter(world)
    {
        let Some(asset) = world.resource::<Assets<Image>>().get(&image.image) else {
            continue;
        };
        if asset.width() != 256 || asset.height() != 256 || node.size().min_element() < 30.0 {
            continue;
        }
        let mut parent = entity;
        let mut solid = false;
        let mut anchor = None;
        while let Some(child) = world.get::<ChildOf>(parent) {
            parent = child.parent();
            solid |= world.get::<SolidSurface>(parent).is_some();
            if anchor.is_none()
                && world
                    .get::<lince_sandbox::canvas::CanvasItem>(parent)
                    .is_some()
            {
                anchor = Some(parent);
            }
        }
        let point = if let Some(anchor) = anchor
            && !solid
        {
            let Some((camera, transform)) = &camera else {
                continue;
            };
            let root = world.get::<ChildOf>(anchor).unwrap().parent();
            let position = lince_sandbox::topology::position(world, anchor).unwrap()
                - lince_sandbox::topology::presentation::origin(world, root);
            let Ok(point) = camera.world_to_viewport(transform, position.as_vec3()) else {
                continue;
            };
            point
        } else {
            transform.affine().translation
                + if solid {
                    node.size() * Vec2::new(0.43, 0.3)
                } else {
                    Vec2::ZERO
                }
        };
        samples.push((point, solid));
    }
    if samples.len() != 2 {
        return;
    }
    assert!(
        !world
            .query::<&Text>()
            .iter(world)
            .any(|text| text.0.starts_with("WGSL:"))
    );
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/shader-effects-smoke.png".into());
    let path = if stage == 0 {
        path
    } else {
        format!("{path}.calm.png")
    };
    world.resource_mut::<Smoke>().captured = true;
    world
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path))
        .observe(
            move |event: On<ScreenshotCaptured>,
                  mut smoke: ResMut<Smoke>,
                  mut exit: MessageWriter<AppExit>| {
                let image = event.image.clone().try_into_dynamic().unwrap().to_rgba8();
                for (point, solid) in &samples {
                    let pixel = image.get_pixel(point.x as u32, point.y as u32);
                    if *solid {
                        assert!(
                            pixel[0] > 130 && pixel[0].saturating_sub(pixel[1]) > 40,
                            "Surface shader did not render: {pixel:?}"
                        );
                    } else if stage == 0 {
                        assert!(
                            pixel[2] > pixel[0] + 10,
                            "Negative quantity did not select energy: {pixel:?}"
                        );
                    } else {
                        assert!(
                            pixel[0].abs_diff(pixel[2]) < 8 && pixel[0] > 70,
                            "Zero quantity did not select calm: {pixel:?}"
                        );
                    }
                }
                if stage == 0 {
                    smoke.stage = 1;
                    smoke.captured = false;
                } else {
                    exit.write(AppExit::Success);
                }
            },
        );
}

fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _guard = runtime.enter();
    let (engine, uid) = runtime.block_on(async {
        let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
        let organ = store::organs::ensure_local(&engine.store.pool, "http://shader-effects-smoke")
            .await
            .unwrap()
            .uid;
        engine
            .set_signer(engine::trust::Signer::generate(&organ, "key"))
            .await
            .unwrap();
        let uid = engine
            .act(
                engine::actions::Action::CreateRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: "Quantity shader".into(),
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
                    namespace: NAMESPACE.into(),
                    fds: serde_json::to_value(Extension::single(1, Effect::default())).unwrap(),
                },
                None,
            )
            .await
            .unwrap();
        (engine, uid)
    });
    let mut app = lince_sandbox::app::interface_app();
    app.insert_resource(lince_sandbox::app::CellHandle(cell::CellRuntime {
        commands: default(),
        store: engine.store.clone(),
        engine,
        lanes: Arc::new(cell::LaneHub::new()),
        wire: default(),
        fiote: None,
        speech: None,
        information: None,
    }))
    .add_plugins(lince_sandbox::cell_bridge::CellBridgePlugin)
    .insert_resource(WinitSettings::continuous())
    .insert_resource(Smoke {
        uid,
        started: Instant::now(),
        stage: 0,
        captured: false,
        changed: false,
        setup: false,
    })
    .add_systems(Startup, |mut commands: Commands| {
        commands.spawn(lince_sandbox::container::BoxRoot);
    })
    .add_systems(
        Update,
        setup
            .after(lince_sandbox::workspace::PrepareWorkspaces)
            .before(lince_sandbox::protein_area::UpdateProteinAreas),
    )
    .add_systems(Last, inspect)
    .run();
}
