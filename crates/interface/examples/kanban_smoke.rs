use bevy::{
    diagnostic::FrameCount,
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    winit::WinitSettings,
};
use lince_interface::{
    area::InfluenceArea,
    canvas::{CanvasItem, CanvasView},
    container::BoxRoot,
    kanban::{self, Kanban},
    layout::LayoutRuntime,
    protein_area::RecordBinding,
};
use std::sync::Arc;

#[derive(Resource)]
struct Trial {
    started: std::time::Instant,
    scrolled: Option<u32>,
    captured: bool,
    picker: Option<(Entity, u32, Option<Vec2>)>,
}

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    let root = world
        .query_filtered::<Entity, With<BoxRoot>>()
        .single(world)
        .unwrap();
    if frame == 20 {
        kanban::spawn(world, root, 1, DVec2::ZERO).unwrap();
        let mut view = world.get_mut::<CanvasView>(root).unwrap();
        view.zoom = 0.58;
        view.center.y = -60.0;
        world.insert_resource(Trial {
            started: std::time::Instant::now(),
            scrolled: None,
            captured: false,
            picker: None,
        });
        return;
    }
    let Some(trial) = world.get_resource::<Trial>() else {
        return;
    };
    let scrolled = trial.scrolled;
    if scrolled.is_none() {
        let board = world.query::<&Kanban>().single(world).unwrap();
        let first = board.columns[0].area.clone();
        let area = world
            .query::<(Entity, &InfluenceArea)>()
            .iter(world)
            .find(|(_, a)| a.id == first)
            .unwrap()
            .0;
        let layout = *world.get::<LayoutRuntime>(area).unwrap();
        let owner = world
            .query_filtered::<Entity, With<Kanban>>()
            .single(world)
            .unwrap();
        let records = world
            .query::<(&CanvasItem, &RecordBinding)>()
            .iter(world)
            .count();
        assert!(
            world.resource::<Trial>().started.elapsed().as_secs() < 60,
            "Tasks must overflow the column: {layout:?}; {records} bindings; {}",
            kanban::status(world, owner)
        );
        if records != 16 || layout.content.y <= layout.size.y {
            return;
        }
        let card = world
            .query::<(
                Entity,
                &LayoutRuntime,
                &lince_interface::topology::presentation::Surface,
            )>()
            .iter(world)
            .find(|(_, layout, _)| layout.parent == Some(area))
            .map(|(entity, _, surface)| (entity, surface.camera, surface.image.clone()));
        let Some(card) = card else {
            return;
        };
        world.trigger(Pointer::new(
            lince_interface::topology::input::CONTENT_POINTER,
            bevy::picking::pointer::Location {
                target: bevy::camera::NormalizedRenderTarget::Image(card.2.into()),
                position: Vec2::ZERO,
            },
            bevy::picking::events::Scroll {
                unit: bevy::input::mouse::MouseScrollUnit::Pixel,
                x: 0.0,
                y: -150.0,
                phase: bevy::input::touch::TouchPhase::Moved,
                hit: bevy::picking::backend::HitData::new(card.1, 0.0, None, None),
            },
            card.0,
        ));
        assert_eq!(world.get::<LayoutRuntime>(area).unwrap().scroll.y, 150.0);
        world.resource_mut::<Trial>().scrolled = Some(frame);
    }
    if scrolled.is_some_and(|start| frame > start + 30) && !world.resource::<Trial>().captured {
        world.resource_mut::<Trial>().captured = true;
        let cards = world
            .query::<(&CanvasItem, &RecordBinding)>()
            .iter(world)
            .count();
        assert_eq!(cards, 16);
        assert!(
            world
                .query::<(&bevy::text::EditableText, &ComputedNode)>()
                .iter(world)
                .any(|(text, node)| {
                    text.value().to_string().starts_with("Task ")
                        && node.size().y * node.inverse_scale_factor() > 30.0
                }),
            "Long Task titles must wrap and grow their cards"
        );
        for (text, scroll) in world
            .query::<(&bevy::text::EditableText, &bevy::ui::widget::TextScroll)>()
            .iter(world)
        {
            if text.value().to_string().starts_with("Task ") {
                assert!(
                    scroll.0.y.abs() < 1.0,
                    "Growing titles must show their first line: {:?}",
                    scroll.0
                );
            }
        }
        let cropped = world
            .query::<&lince_interface::topology::presentation::Surface>()
            .iter(world)
            .filter(|surface| surface.uv.min.y > 0.0 || surface.uv.max.y < 1.0)
            .count();
        assert!(cropped > 0, "Scrolling must crop Task surfaces");
        world
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk("/tmp/lince-kanban.png"));
        let row = world
            .query::<(
                Entity,
                &CanvasItem,
                &lince_interface::area::RecordProperties,
            )>()
            .iter(world)
            .find(|(_, _, properties)| {
                properties.0["head"]
                    .as_str()
                    .is_some_and(|head| head.starts_with("Task 11 "))
            })
            .unwrap()
            .0;
        click_label(world, row, "Empty properties");
        world.resource_mut::<Trial>().picker = Some((row, frame, None));
    }
    if let Some((row, started, size)) = world.resource::<Trial>().picker {
        if size.is_none() && frame > started + 20 {
            click_label(world, row, "Assignees ▾");
            let size = world.get::<CanvasItem>(row).unwrap().size;
            world.resource_mut::<Trial>().picker = Some((row, frame, Some(size)));
        } else if let Some(size) = size
            && frame > started + 30
        {
            let current = world.get::<CanvasItem>(row).unwrap().size;
            assert!(
                (current - size).length() < 1.0,
                "Assignee popup resized the Record: {size:?} -> {current:?}"
            );
            let popup = world
                .query::<(&GlobalZIndex, &ChildOf, &Node)>()
                .iter(world)
                .find(|(z, _, _)| z.0 == 90)
                .unwrap();
            assert_eq!(popup.1.parent(), root);
            assert_eq!(popup.2.position_type, PositionType::Absolute);
            world.resource_mut::<Trial>().picker = None;
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk("/tmp/lince-record-assignees.png"))
                .observe(
                    |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                        exit.write(AppExit::Success);
                    },
                );
        }
    }
}

fn click_label(world: &mut World, row: Entity, label: &str) {
    let button = world
        .query::<(&Text, &ChildOf)>()
        .iter(world)
        .find_map(|(text, parent)| {
            if !text.0.contains(label) {
                return None;
            }
            let button = world.get::<lince_interface::actions::ActionButton>(parent.parent())?;
            let mut cursor = parent.parent();
            loop {
                if cursor == row {
                    return Some(button.clone());
                }
                cursor = world.get::<ChildOf>(cursor)?.parent();
            }
        })
        .unwrap();
    button.actions.run(world, button.target);
}

#[tokio::main]
async fn main() {
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    for name in [
        "task",
        "backlog",
        "todo",
        "next",
        "wip",
        "review",
        "done",
        "documented",
    ] {
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
    for head in ["Ada", "Bea"] {
        engine
            .act(
                engine::actions::Action::CreateRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Person,
                    head: head.into(),
                    body: String::new(),
                    quantity: 0.0,
                },
                None,
            )
            .await
            .unwrap();
    }
    for index in 0..16 {
        let (status, quantity) = if index < 10 {
            ("backlog", 0.0)
        } else {
            ("todo", -1.0)
        };
        engine
            .act(
                engine::actions::Action::CreateRecordWithTags {
                    head: format!(
                        "Task {} · A title that can grow inside its column",
                        index + 1
                    ),
                    body: String::new(),
                    quantity,
                    tags: vec!["task".into(), status.into()],
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
            engine: engine.clone(),
            store: engine.store.clone(),
            lanes: Arc::new(cell::LaneHub::new()),
            wire: Default::default(),
            information: None,
        }))
        .insert_resource(WinitSettings::continuous())
        .add_systems(
            Startup,
            |mut commands: Commands, mut windows: Query<&mut Window>| {
                windows.single_mut().unwrap().resolution.set(1600.0, 1000.0);
                commands.spawn(BoxRoot);
            },
        )
        .add_systems(Update, exercise)
        .run();
}
