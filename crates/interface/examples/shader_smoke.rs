use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    winit::WinitSettings,
};
use lince_interface::{description::Description, protein_area::Source};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Resource)]
struct Smoke {
    started: Instant,
    captured: bool,
}

fn setup(world: &mut World) {
    world
        .query::<&mut Window>()
        .single_mut(world)
        .unwrap()
        .resolution
        .set(1540.0, 900.0);
    let root = world.spawn(lince_interface::container::BoxRoot).id();
    let shader = lince_interface::shader_castle::spawn(world, root, 1, default(), "shader-smoke");
    world
        .entity_mut(shader)
        .remove::<lince_interface::canvas::CanvasItem>();
    let mut node = world.get_mut::<Node>(shader).unwrap();
    node.position_type = PositionType::Absolute;
    node.left = px(12);
    node.top = px(12);
    node.width = px(920);
    node.height = px(850);
    let record =
        lince_interface::full_record::open(world, root, "shader-smoke", Source::Local).unwrap();
    let config = world
        .get_mut::<lince_interface::area::InfluenceArea>(record)
        .unwrap()
        .protein
        .as_mut()
        .unwrap()
        .clone();
    let mut config = config;
    config.bindings.retain(|binding| binding.property == "body");
    world
        .get_mut::<lince_interface::area::InfluenceArea>(record)
        .unwrap()
        .protein = Some(config);
}

fn inspect(world: &mut World) {
    let state = world.resource::<Smoke>();
    assert!(
        state.started.elapsed() < Duration::from_secs(45),
        "Shader preview timed out"
    );
    if state.captured {
        return;
    }
    let standalone: Vec<_> = world
        .query::<(Entity, &lince_interface::protein_area::RecordBinding)>()
        .iter(world)
        .filter(|(entity, _)| {
            world
                .get::<lince_interface::canvas::CanvasItem>(*entity)
                .is_some()
                && world.get::<ChildOf>(*entity).is_some_and(|parent| {
                    world
                        .get::<lince_interface::container::BoxRoot>(parent.parent())
                        .is_some()
                })
        })
        .map(|(entity, _)| entity)
        .collect();
    for entity in standalone {
        world
            .entity_mut(entity)
            .remove::<lince_interface::canvas::CanvasItem>();
        let mut node = world.get_mut::<Node>(entity).unwrap();
        node.position_type = PositionType::Absolute;
        node.left = px(950);
        node.top = px(12);
        node.width = px(560);
        node.height = px(850);
    }
    if world.resource::<Smoke>().started.elapsed() < Duration::from_secs(5) {
        return;
    }
    let previews: Vec<_> = world
        .query::<(Entity, &Description)>()
        .iter(world)
        .filter(|(_, description)| description.source.contains("```wgsl"))
        .map(|(entity, _)| entity)
        .collect();
    if previews.len() != 2 {
        return;
    }
    let mut centers = Vec::new();
    for description in previews {
        let block = world.get::<Children>(description).unwrap()[1];
        let surface = world.get::<Children>(block).unwrap()[0];
        assert!(
            world
                .get::<ComputedNode>(surface)
                .unwrap()
                .size()
                .min_element()
                > 100.0
        );
        centers.push(
            world
                .get::<UiGlobalTransform>(surface)
                .unwrap()
                .affine()
                .translation,
        );
    }
    assert!(
        !world
            .query::<&Text>()
            .iter(world)
            .any(|text| text.0.starts_with("WGSL:"))
    );
    let path = std::env::args().nth(1).expect("screenshot output path");
    world.resource_mut::<Smoke>().captured = true;
    world
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path))
        .observe(
            move |event: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                let image = event.image.clone().try_into_dynamic().unwrap().to_rgba8();
                for center in &centers {
                    let pixel = image.get_pixel(center.x as u32, center.y as u32);
                    assert!(
                        pixel[2] > 180 && pixel[1] > 150,
                        "Expected a glowing ball at {center:?}, got {pixel:?}"
                    );
                }
                exit.write(AppExit::Success);
            },
        );
}

fn main() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _guard = runtime.enter();
    let engine = runtime.block_on(async {
        let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
        engine
            .act(
                engine::actions::Action::CreateRecord {
                    slug: Some("shader-smoke".into()),
                    kind: nucleus::RecordKind::Plain,
                    head: "Shader smoke".into(),
                    body: format!(
                        "Hello, world\n\n```wgsl\n{}```\n\nTesting123",
                        lince_interface::description::SHADER_EXAMPLE
                    ),
                    quantity: 0.0,
                },
                None,
            )
            .await
            .unwrap();
        engine
    });
    let mut app = lince_interface::app::interface_app();
    app.insert_resource(lince_interface::app::CellHandle(cell::CellRuntime {
        commands: Default::default(),
        store: engine.store.clone(),
        engine,
        lanes: Arc::new(cell::LaneHub::new()),
        wire: default(),
        fiote: None,
        information: None,
    }))
    .add_plugins(lince_interface::cell_bridge::CellBridgePlugin)
    .insert_resource(WinitSettings::continuous())
    .insert_resource(Smoke {
        started: Instant::now(),
        captured: false,
    })
    .add_systems(Startup, setup)
    .add_systems(Last, inspect)
    .run();
}
