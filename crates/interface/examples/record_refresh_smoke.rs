use bevy::{
    diagnostic::FrameCount,
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    winit::WinitSettings,
};
use lince_interface::{
    area::RecordProperties, canvas::CanvasView, container::BoxRoot, kanban,
    protein_area::RecordBinding,
};
use std::sync::Arc;

const LONG_ASSERTION: &str = "assertionwithalongnamethatneedstowrapwithinthewidthoftherecordcastle";

#[derive(Resource, Default)]
struct Trial {
    ready: Option<u32>,
    labels: Vec<Entity>,
    hidden: usize,
}

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    if frame == 20 {
        let root = world
            .query_filtered::<Entity, With<BoxRoot>>()
            .single(world)
            .unwrap();
        kanban::spawn(world, root, 1, DVec2::ZERO).unwrap();
        let mut view = world.get_mut::<CanvasView>(root).unwrap();
        view.zoom = 0.65;
        view.center.y = 0.0;
    }
    let ready = world.resource::<Trial>().ready;
    if ready.is_none() {
        if world
            .query::<(&RecordBinding, &RecordProperties)>()
            .iter(world)
            .count()
            == 2
        {
            world.resource_mut::<Trial>().ready = Some(frame);
        }
        assert!(frame < 3000, "Records did not load");
        return;
    }
    if frame == ready.unwrap() + 60 {
        let labels: Vec<_> = world
            .query::<(Entity, &Text)>()
            .iter(world)
            .filter(|(_, text)| text.0 == "#task")
            .map(|(entity, _)| entity)
            .collect();
        assert_eq!(labels.len(), 2);
        world.resource_mut::<Trial>().labels = labels;
    }
    let elapsed = frame - ready.unwrap();
    if let Some(quantity) = [(60, -2.0), (150, -3.0), (240, -1.0), (330, -2.0)]
        .iter()
        .find_map(|(at, quantity)| (*at == elapsed).then_some(*quantity))
    {
        let uid = world
            .query::<&RecordProperties>()
            .iter(world)
            .find(|record| record.0["head"] == "Moving task")
            .unwrap()
            .0["uid"]
            .as_str()
            .unwrap()
            .to_owned();
        let engine = world
            .resource::<lince_interface::app::CellHandle>()
            .0
            .engine
            .clone();
        tokio::spawn(async move {
            engine
                .act(
                    engine::actions::Action::SetQuantity {
                        target: uid,
                        value: quantity,
                    },
                    None,
                )
                .await
                .unwrap();
        });
    }
    if elapsed == 420 {
        for label in &world.resource::<Trial>().labels {
            assert!(
                world.get::<Text>(*label).is_some(),
                "Unchanged assertion was recreated"
            );
        }
        let next = world
            .query::<(Entity, &lince_interface::area::InfluenceArea)>()
            .iter(world)
            .find(|(_, area)| area.name == "Next")
            .unwrap()
            .0;
        let (entity, record) = world
            .query::<(Entity, &RecordProperties)>()
            .iter(world)
            .find(|(_, record)| record.0["head"] == "Moving task")
            .unwrap();
        assert_eq!(record.0["quantity"], "-2");
        assert_eq!(
            world
                .get::<lince_interface::layout::LayoutRuntime>(entity)
                .unwrap()
                .parent,
            Some(next)
        );
        assert_eq!(
            world.resource::<Trial>().hidden,
            0,
            "Record castles flickered during quantity updates"
        );
        world
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk("/tmp/lince-record-refresh.png"))
            .observe(
                |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                    exit.write(AppExit::Success);
                },
            );
    }
}

fn monitor(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    if world
        .resource::<Trial>()
        .ready
        .is_none_or(|ready| frame <= ready + 60)
    {
        return;
    }
    let hidden: Vec<_> = world
        .query::<(
            &RecordProperties,
            &InheritedVisibility,
            &lince_interface::castle::StartupStatus,
        )>()
        .iter(world)
        .filter(|(_, visible, _)| !visible.get())
        .map(|(record, _, status)| (record.0["head"].clone(), status.0.clone()))
        .collect();
    if !hidden.is_empty() {
        eprintln!("HIDDEN frame {frame}: {hidden:?}");
        world.resource_mut::<Trial>().hidden += hidden.len();
    }
    if hidden.is_empty() {
        for (text, node, layout, visibility, parent) in world
            .query::<(
                &Text,
                &ComputedNode,
                &bevy::text::TextLayoutInfo,
                &InheritedVisibility,
                &ChildOf,
            )>()
            .iter(world)
        {
            if text.0.starts_with('#') {
                assert!(
                    node.size().x > 0.0 && !layout.glyphs.is_empty() && visibility.get(),
                    "Assertion label is missing: {}",
                    text.0
                );
                let chip = world.get::<ComputedNode>(parent.parent()).unwrap();
                let flow = world.get::<ChildOf>(parent.parent()).unwrap().parent();
                assert!(chip.size().x <= world.get::<ComputedNode>(flow).unwrap().size().x);
                assert!(node.size().x <= chip.content_box().width());
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    for name in ["task", "todo", "backlog", LONG_ASSERTION] {
        engine
            .act(
                engine::actions::Action::CreateConcept {
                    lingua: "g_local".into(),
                    name: name.into(),
                    parents: Vec::new(),
                },
                None,
            )
            .await
            .unwrap();
    }
    for (head, quantity, status) in [
        ("Moving task", -1.0, "todo"),
        ("Unchanged task", 0.0, "backlog"),
    ] {
        engine
            .act(
                engine::actions::Action::CreateRecordWithTags {
                    head: head.into(),
                    body: String::new(),
                    quantity,
                    tags: vec!["task".into(), status.into(), LONG_ASSERTION.into()],
                },
                None,
            )
            .await
            .unwrap();
    }
    lince_interface::app::interface_app()
        .add_plugins(bevy::log::LogPlugin::default())
        .add_plugins(lince_interface::cell_bridge::CellBridgePlugin)
        .insert_resource(lince_interface::wake::WakeSignal::new(|| {}))
        .insert_resource(lince_interface::app::CellHandle(cell::CellRuntime {
            commands: Default::default(),
            engine: engine.clone(),
            store: engine.store.clone(),
            lanes: Arc::new(cell::LaneHub::new()),
            wire: Default::default(),
            fiote: None,
            information: None,
        }))
        .init_resource::<Trial>()
        .insert_resource(WinitSettings::continuous())
        .add_systems(
            Startup,
            |mut commands: Commands, mut windows: Query<&mut Window>| {
                windows.single_mut().unwrap().resolution.set(1600.0, 1000.0);
                commands.spawn(BoxRoot);
            },
        )
        .add_systems(Update, exercise)
        .add_systems(Last, monitor.after(lince_interface::castle::PresentCastles))
        .run();
}
