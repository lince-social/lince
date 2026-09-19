use base64::Engine as _;
use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    text::EditableText,
    ui_widgets::Activate,
};
use lince_interface::{
    app::connected_app, description::Description, protein_area::Source, thread_castle::ThreadCastle,
};
use std::sync::Arc;

#[derive(Resource)]
struct Exercise {
    record: String,
    stage: u8,
    ticks: usize,
    path: String,
}

fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let (cell, record) = runtime.block_on(async {
        let mut png = std::io::Cursor::new(Vec::new());
        image::load_from_memory(include_bytes!("../../../assets/logo/black_in_white.png")).unwrap().thumbnail(64, 64).write_to(&mut png, image::ImageFormat::Png).unwrap();
        let embedded_logo = base64::engine::general_purpose::STANDARD.encode(png.into_inner());
        let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
        let record = engine.act(engine::actions::Action::CreateRecord { slug: Some("thread-smoke".into()), kind: nucleus::RecordKind::Plain, head: "Thread test".into(), body: String::new(), quantity: 1.0 }, None).await.unwrap().created.unwrap();
        for (title, count) in [("Discussion", 55), ("Planning", 1)] {
            let thread = engine.act(engine::actions::Action::CreateThread { target: record.clone(), head: title.into() }, None).await.unwrap().created.unwrap();
            for index in 0..count {
                engine.act(engine::actions::Action::CreateMessage { thread: thread.clone(), body: if index == count - 1 { format!("## {title}\n\nA **rich** message with _formatting_.\n\n```mermaid\ngraph LR\n A[Record] --> B[Thread]\n```\n\n![Lince logo](data:image/png;base64,{embedded_logo})\n\nMessage {index}") } else { format!("Message {index}") }, author: None, state: nucleus::MessageState::Finished, parent: None, references: Vec::new() }, None).await.unwrap();
            }
        }
        (cell::CellRuntime { store: engine.store.clone(), engine, lanes: Arc::new(cell::LaneHub::new()), wire: Default::default(), fiote: None, information: None }, record)
    });
    let _entered = runtime.enter();
    runtime.spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(90)).await;
        eprintln!("Thread smoke timed out");
        std::process::exit(1);
    });
    let mut app = connected_app(cell);
    app.insert_resource(bevy::winit::WinitSettings::continuous());
    app.insert_resource(Exercise {
        record,
        stage: 0,
        ticks: 0,
        path: std::env::args().nth(1).expect("screenshot path"),
    });
    app.add_systems(Update, exercise);
    app.run();
}

fn button(world: &mut World, text: &str) -> Option<Entity> {
    world
        .query::<(Entity, &lince_interface::icons::Tooltip)>()
        .iter(world)
        .find(|(_, tooltip)| tooltip.0 == text)
        .map(|(entity, _)| entity)
}

fn descriptions(world: &mut World) -> usize {
    world
        .query_filtered::<Entity, With<Description>>()
        .iter(world)
        .filter(|entity| {
            let mut cursor = Some(*entity);
            while let Some(entity) = cursor {
                if world.get::<ThreadCastle>(entity).is_some() {
                    return true;
                }
                cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
            }
            false
        })
        .count()
}

fn exercise(world: &mut World) {
    let welcome: Vec<_> = world
        .query_filtered::<Entity, With<lince_interface::instinct::Instinct>>()
        .iter(world)
        .collect();
    for entity in welcome {
        world.despawn(entity);
    }
    let mut test = world.remove_resource::<Exercise>().unwrap();
    let previous = test.stage;
    test.ticks += 1;
    if test.stage == 0 {
        if let Some(root) = world
            .query_filtered::<Entity, With<lince_interface::workspace::Workspaces>>()
            .iter(world)
            .next()
        {
            lince_interface::thread_castle::open(world, root, &test.record, Source::Local).unwrap();
            test.stage = 1;
        }
    } else if test.stage == 1
        && world.query::<&ThreadCastle>().iter(world).count() == 1
        && descriptions(world) == 51
    {
        let viewport = world
            .query::<(Entity, &ScrollPosition, &ComputedNode, &Node)>()
            .iter(world)
            .find(|(_, _, computed, node)| computed.size().y > 100.0 && node.height == px(360))
            .map(|(entity, _, _, _)| entity)
            .unwrap();
        world.get_mut::<ScrollPosition>(viewport).unwrap().0.y = 0.0;
        world.trigger(Pointer::new(
            bevy::picking::pointer::PointerId::Mouse,
            bevy::picking::pointer::Location {
                target: bevy::camera::NormalizedRenderTarget::Image(
                    Handle::<Image>::default().into(),
                ),
                position: Vec2::ZERO,
            },
            bevy::picking::events::Scroll {
                unit: bevy::input::mouse::MouseScrollUnit::Line,
                x: 0.0,
                y: 1.0,
                phase: bevy::input::touch::TouchPhase::Moved,
                hit: bevy::picking::backend::HitData::new(viewport, 0.0, None, None),
            },
            viewport,
        ));
        test.stage = 2;
    } else if test.stage == 2 && descriptions(world) == 56 {
        let tab = button(world, "Planning").unwrap();
        world.trigger(Activate { entity: tab });
        test.stage = 3;
        test.ticks = 0;
    } else if test.stage == 3 && test.ticks > 30 {
        let tab = button(world, "Discussion").unwrap();
        world.trigger(Activate { entity: tab });
        test.stage = 4;
        test.ticks = 0;
    } else if test.stage == 4
        && test.ticks > 30
        && !world
            .query::<&Text>()
            .iter(world)
            .any(|text| text.0 == "Rendering diagram…" || text.0 == "Lince logo")
    {
        let delete = button(world, "Delete message").unwrap();
        world.trigger(Activate { entity: delete });
        test.stage = 5;
    } else if test.stage == 5 {
        let delete = button(world, "Delete Record").unwrap();
        world.trigger(Activate { entity: delete });
        test.stage = 6;
    } else if test.stage == 6 && descriptions(world) == 55 {
        for (mut position, node) in world
            .query::<(&mut ScrollPosition, &Node)>()
            .iter_mut(world)
        {
            if node.height == px(360) {
                position.0.y = 1_000_000.0;
            }
        }
        test.stage = 7;
        test.ticks = 0;
    } else if test.stage == 7 && test.ticks > 30 {
        let input_count = world.query::<&EditableText>().iter(world).count();
        assert!(input_count >= 55);
        world
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(test.path.clone()))
            .observe(
                |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                    exit.write(AppExit::Success);
                },
            );
        test.stage = 8;
    }
    if test.stage != previous {
        eprintln!("Thread smoke stage {}", test.stage);
    }
    if test.ticks % 300 == 0 && test.stage < 8 {
        let descriptions = descriptions(world);
        eprintln!(
            "Thread smoke stage {}, descriptions {descriptions}",
            test.stage
        );
        world
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(format!("{}.progress.png", test.path)));
    }
    world.insert_resource(test);
}
