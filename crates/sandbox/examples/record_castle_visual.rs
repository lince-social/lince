use bevy::{
    diagnostic::FrameCount,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    text::EditableText,
    winit::WinitSettings,
};
use lince_sandbox::{
    actions::ActionButton,
    area::InfluenceArea,
    calendar::CalendarSand,
    canvas::{CanvasItem, CanvasView},
    container::BoxRoot,
    icons::Tooltip,
    protein_area::{RecordBinding, Source},
};
use std::{path::PathBuf, sync::Arc};

#[derive(Resource)]
struct Trial {
    uid: String,
    output: PathBuf,
    source: Option<Entity>,
    row: Option<Entity>,
    phase: u8,
    frame: u32,
    captured: bool,
    placement: Option<CanvasItem>,
}

fn invoke(world: &mut World, owner: Entity, caption: &str) {
    let action = world
        .query::<(Entity, &Tooltip, &ActionButton)>()
        .iter(world)
        .find_map(|(entity, tip, action)| {
            if tip.0 != caption {
                return None;
            }
            let mut cursor = entity;
            loop {
                if cursor == owner {
                    return Some(action.clone());
                }
                cursor = world.get::<ChildOf>(cursor)?.parent();
            }
        })
        .unwrap_or_else(|| panic!("Missing control: {caption}"));
    action.actions.run(world, action.target);
}

fn capture(world: &mut World, name: &str) {
    world.resource_mut::<Trial>().captured = true;
    let path = world.resource::<Trial>().output.join(name);
    world
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path))
        .observe(|_: On<ScreenshotCaptured>, mut trial: ResMut<Trial>| {
            trial.phase += 1;
            trial.captured = false;
        });
}

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    let root = world
        .query_filtered::<Entity, With<BoxRoot>>()
        .single(world)
        .unwrap();
    if frame < 20 {
        return;
    }
    if world.resource::<Trial>().source.is_none() {
        let uid = world.resource::<Trial>().uid.clone();
        let source = lince_sandbox::full_record::open(world, root, &uid, Source::Local).unwrap();
        let mut area = world.get_mut::<InfluenceArea>(source).unwrap();
        let config = area.protein.as_mut().unwrap();
        config.width = 320.0;
        config.hide_filled = true;
        world.resource_mut::<Trial>().source = Some(source);
    }
    if world.resource::<Trial>().row.is_none() {
        let uid = world.resource::<Trial>().uid.clone();
        let row = world
            .query::<(Entity, &RecordBinding, &CanvasItem)>()
            .iter(world)
            .find(|(_, binding, _)| binding.uid == uid)
            .map(|(entity, _, _)| entity);
        let Some(row) = row else {
            assert!(frame < 1500, "Record fixture did not load");
            return;
        };
        let mut trial = world.resource_mut::<Trial>();
        trial.row = Some(row);
        trial.frame = frame + 50;
    }
    let trial = world.resource::<Trial>();
    if frame < trial.frame || trial.captured {
        return;
    }
    let phase = trial.phase;
    let row = trial.row.unwrap();
    match phase {
        0 => {
            let item = world.get::<CanvasItem>(row).unwrap();
            assert!(
                item.size.y < 110.0,
                "Title-only Record took extra space: {:?}",
                item.size
            );
            capture(world, "native-minimal.png");
        }
        1 => {
            invoke(world, row, "Show or hide properties");
            world.resource_mut::<Trial>().phase = 2;
            world.resource_mut::<Trial>().frame = frame + 60;
        }
        2 => {
            capture(world, "native-expanded-320.png");
        }
        3 => {
            let source = world.resource::<Trial>().source.unwrap();
            world
                .get_mut::<InfluenceArea>(source)
                .unwrap()
                .protein
                .as_mut()
                .unwrap()
                .width = 640.0;
            world.resource_mut::<Trial>().phase = 4;
            world.resource_mut::<Trial>().frame = frame + 100;
        }
        4 => {
            capture(world, "native-expanded-640.png");
        }
        5 => {
            let item = *world.get::<CanvasItem>(row).unwrap();
            world.resource_mut::<Trial>().placement = Some(item);
            for _ in 0..12 {
                invoke(world, row, "Choose start date");
                invoke(world, row, "Choose due date");
            }
            assert_eq!(world.query::<&CalendarSand>().iter(world).count(), 1);
            world.resource_mut::<Trial>().phase = 6;
            world.resource_mut::<Trial>().frame = frame + 60;
        }
        6 => {
            let before = world.resource::<Trial>().placement.unwrap();
            let after = world.get::<CanvasItem>(row).unwrap();
            assert!(
                before.position.distance(after.position) < 0.01,
                "Calendar moved Record"
            );
            assert!(
                before.size.distance(after.size) < 0.01,
                "Calendar resized Record"
            );
            let popup = world
                .query_filtered::<Entity, With<CalendarSand>>()
                .single(world)
                .unwrap();
            assert!(world.get::<CanvasItem>(popup).is_none());
            let days: Vec<_> = world
                .query::<(&Tooltip, &ComputedNode)>()
                .iter(world)
                .filter(|(tip, _)| tip.0.starts_with("Select 2026-10-"))
                .map(|(_, node)| node.size() * node.inverse_scale_factor())
                .collect();
            std::fs::write(
                world.resource::<Trial>().output.join("native-geometry.json"),
                serde_json::to_string_pretty(&serde_json::json!({
                    "calendar_day_sizes": days.iter().map(|size| [size.x, size.y]).collect::<Vec<_>>(),
                    "record_position_stable": true,
                    "record_size_stable": true,
                    "picker_count": 1,
                })).unwrap(),
            ).unwrap();
            assert_eq!(days.len(), 31);
            assert!(
                days.iter().all(|size| size.min_element() >= 28.0),
                "Calendar days collapsed: {days:?}"
            );
            assert!(
                days.iter().all(|size| (size.y - days[0].y).abs() <= 1.0),
                "Unequal calendar days: {days:?}"
            );
            capture(world, "native-calendar.png");
        }
        7 => {
            let popup = world
                .query_filtered::<Entity, With<CalendarSand>>()
                .single(world)
                .unwrap();
            invoke(world, popup, "Choose Start date");
            invoke(world, popup, "Select 2026-10-11");
            world.resource_mut::<Trial>().phase = 8;
            world.resource_mut::<Trial>().frame = frame + 120;
        }
        8 => {
            let popup = world
                .query_filtered::<Entity, With<CalendarSand>>()
                .single(world)
                .unwrap();
            invoke(world, popup, "Choose End date");
            invoke(world, popup, "Select 2026-10-23");
            world.resource_mut::<Trial>().phase = 9;
            world.resource_mut::<Trial>().frame = frame + 120;
        }
        9 => {
            let popup = world
                .query_filtered::<Entity, With<CalendarSand>>()
                .single(world)
                .unwrap();
            invoke(world, popup, "Close calendar");
            assert_eq!(world.query::<&CalendarSand>().iter(world).count(), 0);
            let editor = world
                .query::<(Entity, &EditableText)>()
                .iter(world)
                .find(|(_, text)| text.value() == "2026-10-11")
                .unwrap()
                .0;
            world
                .get_mut::<EditableText>(editor)
                .unwrap()
                .editor
                .set_text("2026-10-12");
            world.resource_mut::<Trial>().phase = 10;
            world.resource_mut::<Trial>().frame = frame + 100;
        }
        10 => {
            assert!(
                world
                    .query::<&EditableText>()
                    .iter(world)
                    .any(|text| text.value() == "2026-10-12")
            );
            capture(world, "native-typed-after-range.png");
        }
        _ => {
            world.write_message(AppExit::Success);
        }
    }
}

#[tokio::main]
async fn main() {
    let output = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("Provide screenshot directory"),
    );
    std::fs::create_dir_all(&output).unwrap();
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let uid = engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: Some("varrer-casa".into()),
                kind: nucleus::RecordKind::Plain,
                head: "Varrer Casa".into(),
                body: "Sweep the rooms and put everything back.".into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    engine.act(engine::actions::Action::SetExtension {
        target: uid.clone(), namespace: "work".into(),
        fds: serde_json::json!({"start":"2026-10-08", "due":"2026-10-23", "estimate_min":10}),
    }, None).await.unwrap();
    lince_sandbox::app::interface_app()
        .add_plugins(bevy::log::LogPlugin::default())
        .add_plugins(lince_sandbox::cell_bridge::CellBridgePlugin)
        .insert_resource(lince_sandbox::wake::WakeSignal::new(|| {}))
        .insert_resource(lince_sandbox::app::CellHandle(cell::CellRuntime {
            commands: Default::default(),
            engine: engine.clone(),
            store: engine.store.clone(),
            lanes: Arc::new(cell::LaneHub::new()),
            wire: Default::default(),
            fiote: None,
            speech: None,
            information: None,
        }))
        .insert_resource(WinitSettings::continuous())
        .insert_resource(Trial {
            uid,
            output,
            source: None,
            row: None,
            phase: 0,
            frame: 0,
            captured: false,
            placement: None,
        })
        .add_systems(
            Startup,
            |mut commands: Commands, mut windows: Query<&mut Window>| {
                windows.single_mut().unwrap().resolution.set(1000.0, 1100.0);
                commands.spawn((BoxRoot, CanvasView::default()));
            },
        )
        .add_systems(Update, exercise)
        .run();
}
