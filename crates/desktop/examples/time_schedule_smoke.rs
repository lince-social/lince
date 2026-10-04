use bevy::{
    diagnostic::FrameCount,
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    winit::WinitSettings,
};
use lince_desktop::{
    actions::ActionButton,
    container::BoxRoot,
    sand_store::{SandKind, spawn_sand},
    time_castle::TimeSettings,
};
use std::sync::Arc;

#[derive(Resource)]
struct Trial {
    clock: Option<Entity>,
    record: String,
    receiver: Option<Entity>,
    selected: String,
    stage: u8,
    started: std::time::Instant,
    capture: Option<(u8, std::time::Instant, u32)>,
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
                                .is_some_and(|text| text.0.contains(caption))
                        })
                    }))
        })
        .unwrap()
        .1
        .clone();
    action.actions.run(world, clock);
}

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    if let Some((stage, at, started)) = world.resource::<Trial>().capture {
        if at.elapsed().as_millis() >= 1000 && frame.saturating_sub(started) >= 16 {
            world.resource_mut::<Trial>().capture = None;
            let path = format!("/tmp/lince-time-schedule-{stage}.png");
            let capture = world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path))
                .id();
            if stage == 12 {
                world.entity_mut(capture).observe(
                    |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                        exit.write(AppExit::Success);
                    },
                );
            }
        }
        return;
    }
    if frame == 20 {
        let root = world
            .query_filtered::<Entity, With<BoxRoot>>()
            .single(world)
            .unwrap();
        let clock = spawn_sand(world, root, 1, SandKind::WorkTimer, "", DVec2::ZERO);
        world.resource_mut::<Trial>().clock = Some(clock);
        let record = world.resource::<Trial>().record.clone();
        let receiver = lince_desktop::full_record::open(
            world,
            root,
            &record,
            lince_desktop::protein_area::Source::Local,
        )
        .unwrap();
        world.resource_mut::<Trial>().receiver = Some(receiver);
        let mut area = world
            .get_mut::<lince_desktop::area::InfluenceArea>(receiver)
            .unwrap();
        area.center = [1800.0, 0.0];
        area.protein.as_mut().unwrap().listen_record_selection = true;
    }
    if world.resource::<Trial>().clock.is_none() || frame < 22 {
        return;
    }
    let stage = world.resource::<Trial>().stage;
    if stage > 12 {
        return;
    }
    if stage == 0
        && !world
            .query::<&Text>()
            .iter(world)
            .any(|text| text.0.contains("Morning task"))
    {
        return;
    }
    if world.resource::<Trial>().started.elapsed().as_secs() < u64::from(stage) * 3 {
        return;
    }
    let clock = world.resource::<Trial>().clock.unwrap();
    if stage == 1 {
        activate(world, clock, "Clock controls");
    }
    if stage == 2 {
        activate(world, clock, "Agenda");
    }
    if stage == 3 {
        activate(world, clock, "Overlapping work");
        activate(world, clock, "Hide clock controls");
    }
    if stage == 4 {
        activate(world, clock, "Clock controls");
        activate(world, clock, "Clock");
        activate(world, clock, "Cursor stays at top");
        activate(world, clock, "Hide clock controls");
        assert_eq!(
            world.get::<TimeSettings>(clock).unwrap().0.cursor,
            lince_interface::time_castle::CursorMode::Fixed
        );
    }
    if stage == 5 {
        use lince_desktop::actions::Action;
        activate(world, clock, "Clock controls");
        activate(world, clock, "Theme and tokens");
        let root = world.get::<ChildOf>(clock).unwrap().parent();
        assert_eq!(
            *world
                .get::<lince_desktop::customization::Scope>(root)
                .unwrap(),
            lince_desktop::customization::Scope::Sand(clock)
        );
        lince_desktop::customization::CustomizationAction::ScopedScheme(Some(
            lince_desktop::tokens::ColorScheme::ComfyPink,
        ))
        .apply(world, root);
        lince_desktop::edit_mode::EditAction::Close.apply(world, root);
        activate(world, clock, "Hide clock controls");
    }
    if stage == 6 {
        let root = world.get::<ChildOf>(clock).unwrap().parent();
        world
            .entity_mut(root)
            .insert(lince_desktop::topology::view::View {
                spatial: true,
                position: [0.0, 400.0, 760.0],
                pitch: -0.55,
                ..default()
            });
    }
    if stage == 7 {
        activate(world, clock, "Clock controls");
        activate(world, clock, "Clock");
    }
    if stage == 8 {
        activate(world, clock, "Untwist / Coil");
        activate(world, clock, "Hide clock controls");
    }
    if stage == 9 {
        assert_eq!(
            world.get::<TimeSettings>(clock).unwrap().0.mode,
            lince_interface::time_castle::Mode::Straight
        );
        let trial = world.resource::<Trial>();
        assert_eq!(
            world
                .get::<lince_desktop::time_castle::SelectedOccurrence>(trial.receiver.unwrap())
                .unwrap()
                .0
                .record_uid,
            trial.selected
        );
        assert!(
            world
                .query::<(&lince_desktop::time_castle::SchedulePick, &ViewVisibility)>()
                .iter(world)
                .any(|(_, visibility)| visibility.get())
        );
    }
    if stage == 10 {
        let root = world.get::<ChildOf>(clock).unwrap().parent();
        world
            .entity_mut(root)
            .insert(lince_desktop::topology::view::View::default());
        activate(world, clock, "Clock controls");
        activate(world, clock, "Clock");
        activate(world, clock, "Untwist / Coil");
        activate(world, clock, "Cursor moves");
        activate(world, clock, "Hide clock controls");
    }
    if stage == 11 {
        activate(world, clock, "Clock controls");
        activate(world, clock, "Stopwatch");
    }
    if stage == 12 {
        activate(world, clock, "Hide clock controls");
        assert_eq!(
            world.get::<TimeSettings>(clock).unwrap().0.mode,
            lince_interface::time_castle::Mode::Coiled
        );
    }
    assert_eq!(
        world
            .get::<lince_desktop::canvas::CanvasItem>(clock)
            .unwrap()
            .size,
        Vec2::splat(420.0)
    );
    if stage == 0 {
        world.resource_mut::<Trial>().started = std::time::Instant::now();
    }
    world.resource_mut::<Trial>().capture = Some((stage, std::time::Instant::now(), frame));
    world.resource_mut::<Trial>().stage += 1;
}

#[tokio::main]
async fn main() {
    let directory = tempfile::tempdir().unwrap();
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let now = chrono::Utc::now();
    let mut record = String::new();
    let mut selected = String::new();
    for (index, (head, start, duration, all_day, overdue)) in [
        ("Morning task", 10, Some(10), false, false),
        ("Overlapping work", 15, Some(10), false, false),
        ("Send update", 15, None, false, false),
        ("Stretch break", 15, None, false, false),
        ("Review design details", 18, Some(6), false, false),
        ("Appointment", 40, None, false, false),
        (
            "Prepare a thoughtful response to the design review",
            48,
            Some(8),
            false,
            false,
        ),
        ("Later today", 150, Some(15), false, false),
        ("All-day reminder", 0, None, true, false),
        ("Unfinished deadline", -5, None, false, true),
    ]
    .into_iter()
    .enumerate()
    {
        let uid = engine
            .act(
                engine::actions::Action::CreateRecord {
                    slug: Some(format!("smoke-{index}")),
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
        if index == 0 {
            record = uid.clone();
        }
        if index == 1 {
            selected = uid.clone();
        }
        let at = now + chrono::TimeDelta::minutes(start);
        let work = if all_day {
            serde_json::json!({"due":now.format("%Y-%m-%d").to_string()})
        } else if overdue {
            serde_json::json!({"due":at.to_rfc3339()})
        } else {
            serde_json::json!({"start":at.to_rfc3339(),"estimate_min":duration})
        };
        engine
            .act(
                engine::actions::Action::SetExtension {
                    target: uid,
                    namespace: "work".into(),
                    fds: work,
                },
                None,
            )
            .await
            .unwrap();
    }
    lince_desktop::app::interface_app()
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
        .insert_resource(Trial {
            clock: None,
            record,
            receiver: None,
            selected,
            stage: 0,
            started: std::time::Instant::now(),
            capture: None,
        })
        .insert_resource(lince_desktop::workspace::WorkspaceFile::new(
            directory.path().join("interface.json"),
        ))
        .insert_resource(WinitSettings::continuous())
        .add_systems(
            Startup,
            |mut commands: Commands, mut windows: Query<&mut Window>| {
                windows.single_mut().unwrap().resolution.set(1600.0, 1200.0);
                commands.spawn(BoxRoot);
            },
        )
        .add_systems(Update, exercise)
        .run();
}
