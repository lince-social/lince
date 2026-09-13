use super::*;
use serde_json::json;

fn fixture() -> (App, Entity) {
    let mut app = App::new();
    crate::laboratory::isolate(app.world_mut());
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<crate::tokens::ThemeSettings>()
        .add_plugins((crate::protein_area::ProteinAreaPlugin, CalendarPlugin));
    let root = app
        .world_mut()
        .spawn((
            crate::container::BoxRoot,
            crate::workspace::Workspaces::default(),
        ))
        .id();
    (app, root)
}

#[cfg_attr(test, test)]
fn gregorian_months_and_navigation_respect_leap_years_and_bounds() {
    let mut calendar = Calendar {
        year: 2024,
        month: 2,
        ..Calendar::default()
    };
    let days = calendar.days();
    assert_eq!(days.len(), 42);
    assert_eq!(days.iter().flatten().count(), 29);
    assert_eq!(days[3].unwrap().to_string(), "2024-02-01");
    calendar.year = 2100;
    assert_eq!(calendar.days().iter().flatten().count(), 28);
    calendar.year = 2000;
    assert_eq!(calendar.days().iter().flatten().count(), 29);
    calendar.month = 12;
    calendar.move_months(1);
    assert_eq!((calendar.year, calendar.month), (2001, 1));
    calendar.move_months(-1);
    assert_eq!((calendar.year, calendar.month), (2000, 12));
    calendar.year = 1;
    calendar.month = 1;
    calendar.move_months(-12);
    assert_eq!((calendar.year, calendar.month), (1, 1));
    calendar.year = 9999;
    calendar.month = 12;
    calendar.move_months(12);
    assert_eq!((calendar.year, calendar.month), (9999, 12));
    for invalid in ["2026-02-29", "2026-2-01", "0000-01-01", "2026-13-01"] {
        assert!(model::parse(invalid).is_none());
    }
}

#[cfg_attr(test, test)]
fn periods_validate_endpoints_and_records_cover_inclusive_days() {
    let mut calendar = Calendar::default();
    calendar.select("2026-09-12").unwrap();
    calendar.selecting_end = true;
    assert!(calendar.select("2026-09-11").is_err());
    calendar.select("2026-10-02").unwrap();
    calendar.selecting_end = false;
    assert!(calendar.select("2026-10-03").is_err());
    assert!(calendar.valid());
    let span = model::span(&json!({"start_date":"2026-09-30","due_date":"2026-10-02"})).unwrap();
    assert_eq!(span.0.to_string(), "2026-09-30");
    assert_eq!(span.1.to_string(), "2026-10-02");
    assert!(model::span(&json!({"start_date":"2026-10-02","due_date":"2026-09-30"})).is_none());
    let point = model::span(&json!({"due_date":"2026-09-30"})).unwrap();
    assert_eq!(point.0, point.1);
}

#[cfg_attr(test, test)]
fn named_events_stay_in_their_group_and_other_events_pass() {
    use crate::canvas_selection::SandGroup;
    use crate::scoped_events::*;
    #[derive(Resource, Default)]
    struct Received(Vec<Entity>);
    let mut world = World::new();
    crate::laboratory::isolate(&mut world);
    world.init_resource::<Received>();
    world.add_observer(|event: On<SandEvent>, mut received: ResMut<Received>| {
        received.0.push(event.entity)
    });
    let root = world.spawn_empty().id();
    let first = world
        .spawn((
            ChildOf(root),
            WorkspaceMember(1),
            SandGroup([1; 16]),
            EventBoundary(vec![DATE_SELECTED.into()]),
        ))
        .id();
    let inside = world
        .spawn((
            ChildOf(root),
            WorkspaceMember(1),
            SandGroup([1; 16]),
            EventListener(vec![DATE_SELECTED.into(), "Other".into()]),
        ))
        .id();
    let outside = world
        .spawn((
            ChildOf(root),
            WorkspaceMember(1),
            EventListener(vec![DATE_SELECTED.into(), "Other".into()]),
        ))
        .id();
    let source = world.spawn(ChildOf(first)).id();
    emit(&mut world, source, DATE_SELECTED, json!("2026-09-13"));
    assert_eq!(world.resource::<Received>().0, [inside]);
    world.resource_mut::<Received>().0.clear();
    emit(&mut world, source, "Other", json!(true));
    let events = &world.resource::<Received>().0;
    assert!(events.contains(&inside) && events.contains(&outside));
    world.entity_mut(first).remove::<EventBoundary>();
    world.resource_mut::<Received>().0.clear();
    emit(&mut world, source, DATE_SELECTED, json!("2026-09-13"));
    assert_eq!(world.resource::<Received>().0.len(), 2);
}

#[cfg_attr(test, test)]
fn calendar_persists_and_quiet_updates_keep_the_same_nodes() {
    let (mut app, root) = fixture();
    let calendar = Calendar {
        year: 2026,
        month: 9,
        ..Calendar::default()
    };
    let entity = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::new(30.0, 40.0),
        calendar.clone(),
    );
    let body = app.world().get::<View>(entity).unwrap().body;
    for _ in 0..10 {
        app.update();
    }
    assert_eq!(app.world().get::<View>(entity).unwrap().body, body);
    app.world_mut()
        .get_mut::<crate::canvas::CanvasItem>(entity)
        .unwrap()
        .size = Vec2::new(1000.0, 800.0);
    let saved = snapshot(app.world_mut(), root);
    assert_eq!(saved.len(), 1);
    let encoded = serde_json::to_string(&saved[0]).unwrap();
    let restored: SavedCalendar = serde_json::from_str(&encoded).unwrap();
    assert!(restored.valid());
    app.world_mut().despawn(entity);
    restored.restore(app.world_mut(), root);
    let (sand, item) = app
        .world_mut()
        .query::<(&CalendarSand, &crate::canvas::CanvasItem)>()
        .single(app.world())
        .unwrap();
    assert_eq!(sand.0, calendar);
    assert_eq!(item.size, Vec2::new(1000.0, 800.0));
}

#[cfg_attr(test, test)]
fn large_months_limit_rendered_records_and_keep_every_page_reachable() {
    let calendar = Calendar {
        year: 2026,
        month: 9,
        ..Calendar::default()
    };
    let data: Vec<_> = (0..10_000).map(|index| json!({"uid":index.to_string(), "head":"Work", "start_date":"2026-08-01", "due_date":"2026-10-01"})).collect();
    let first = ui::records(&calendar, &data, 0);
    assert_eq!(first.maximum, 10_000);
    assert_eq!(
        first.days.iter().map(Vec::len).sum::<usize>(),
        30 * ui::PER_DAY
    );
    let last = ui::records(&calendar, &data, usize::MAX);
    assert_eq!(last.page, 9_999 / ui::PER_DAY);
    assert_eq!(last.days.iter().map(Vec::len).sum::<usize>(), 30);
    assert!(last.days.iter().flatten().all(|entry| entry.0 == "9999"));
}

async fn until(app: &mut App, predicate: impl Fn(&World) -> bool) {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            app.update();
            if predicate(app.world()) {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

#[cfg_attr(test, tokio::test)]
async fn real_protein_dates_feed_calendar_and_both_picker_fields_save_through_cell() {
    use crate::protein_area::{Binding, Config, RecordBinding};
    use engine::actions::Action as BackendAction;
    let (mut app, root) = fixture();
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    let uid = engine
        .act(
            BackendAction::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "Calendar task".into(),
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
            BackendAction::SetExtension {
                target: uid.clone(),
                namespace: "work".into(),
                fds: json!({"start":"2026-09-01", "due":"2026-09-03", "estimate_min":17}),
            },
            None,
        )
        .await
        .unwrap();
    let runtime = cell::CellRuntime {
        engine: engine.clone(),
        store: engine.store.clone(),
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        information: None,
    };
    app.insert_resource(crate::app::CellHandle(runtime))
        .insert_resource(crate::wake::WakeSignal::new(|| {}))
        .add_plugins(crate::cell_bridge::CellBridgePlugin);
    let mut influence = InfluenceArea::new(
        crate::area::AreaShape::Square,
        DVec2::ZERO,
        DVec2::splat(800.0),
    );
    let mut config = Config {
        enabled: true,
        bindings: vec![
            Binding::new("head"),
            Binding::new("start_date"),
            Binding::new("due_date"),
        ],
        ..Config::default()
    };
    config.bindings[1].editable = true;
    config.bindings[2].editable = true;
    config.draft.query["where"] = json!([{"all":[{"uid_eq":uid}]}]);
    influence.protein = Some(config);
    let id = influence.id.clone();
    let source = crate::area::spawn_area(app.world_mut(), root, 1, influence).unwrap();
    let calendar = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        Calendar {
            year: 2026,
            month: 9,
            ..Calendar::default()
        },
    );
    Command::Source(Some(id)).apply(app.world_mut(), calendar);
    until(&mut app, |world| {
        crate::protein_area::calendar_feed(world, source)
            .is_some_and(|(rows, _)| rows.iter().any(|r| r["start_date"] == "2026-09-01"))
    })
    .await;
    let editors: Vec<_> = app
        .world_mut()
        .query::<(Entity, &bevy::text::EditableText, &RecordBinding)>()
        .iter(app.world())
        .filter(|(_, _, binding)| binding.uid == uid)
        .map(|(entity, text, _)| (entity, text.value().to_string()))
        .collect();
    let start = editors
        .iter()
        .find(|(_, text)| text == "2026-09-01")
        .unwrap()
        .0;
    let end = editors
        .iter()
        .find(|(_, text)| text == "2026-09-03")
        .unwrap()
        .0;
    picker::OpenDate(start).apply(app.world_mut(), start);
    let popup = app
        .world_mut()
        .query_filtered::<Entity, With<Picker>>()
        .single(app.world())
        .unwrap();
    Command::Select("2026-09-02".into()).apply(app.world_mut(), popup);
    app.world_mut().flush();
    picker::OpenDate(end).apply(app.world_mut(), end);
    Command::Select("2026-09-05".into()).apply(app.world_mut(), popup);
    app.world_mut().flush();
    assert!(app.world().get::<View>(popup).unwrap().error.is_some());
    assert_eq!(
        app.world()
            .get::<bevy::text::EditableText>(end)
            .unwrap()
            .value()
            .to_string(),
        "2026-09-03"
    );
    until(&mut app, |world| {
        crate::protein_area::calendar_feed(world, source)
            .is_some_and(|(rows, _)| rows.iter().any(|r| r["start_date"] == "2026-09-02"))
    })
    .await;
    picker::OpenDate(end).apply(app.world_mut(), end);
    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<Picker>>()
            .single(app.world())
            .unwrap(),
        popup
    );
    Command::Select("2026-09-05".into()).apply(app.world_mut(), popup);
    app.world_mut().flush();
    until(&mut app, |world| {
        crate::protein_area::calendar_feed(world, source)
            .is_some_and(|(rows, _)| rows.iter().any(|r| r["due_date"] == "2026-09-05"))
    })
    .await;
    let (data, _) = crate::protein_area::calendar_feed(app.world(), source).unwrap();
    assert_eq!(data[0]["extension"]["estimate_min"], 17);
    let shown = ui::records(
        &app.world().get::<CalendarSand>(calendar).unwrap().0,
        data,
        0,
    );
    assert_eq!(shown.days.iter().map(Vec::len).sum::<usize>(), 4);
    let titles = app
        .world_mut()
        .query::<(Entity, &Text)>()
        .iter(app.world())
        .filter(|(entity, text)| {
            let mut parent = *entity;
            while let Some(link) = app.world().get::<ChildOf>(parent) {
                parent = link.parent();
                if parent == calendar {
                    return text.0 == "Calendar task";
                }
            }
            false
        })
        .count();
    assert_eq!(titles, 4);
    Command::Select("2026-09-01".into()).apply(app.world_mut(), popup);
    assert!(app.world().get::<View>(popup).unwrap().error.is_some());
    assert_eq!(
        app.world()
            .get::<bevy::text::EditableText>(end)
            .unwrap()
            .value()
            .to_string(),
        "2026-09-05"
    );
    assert_eq!(snapshot(app.world_mut(), root).len(), 1);
    engine
        .act(BackendAction::DeleteRecord { target: uid }, None)
        .await
        .unwrap();
    until(&mut app, |world| {
        crate::protein_area::calendar_feed(world, source).is_some_and(|(rows, _)| rows.is_empty())
    })
    .await;
    assert!(app.world().get_entity(popup).is_err());
}

crate::laboratory_cases! {
    gregorian_months_and_navigation_respect_leap_years_and_bounds,
    periods_validate_endpoints_and_records_cover_inclusive_days,
    named_events_stay_in_their_group_and_other_events_pass,
    calendar_persists_and_quiet_updates_keep_the_same_nodes,
    large_months_limit_rendered_records_and_keep_every_page_reachable,
    async real_protein_dates_feed_calendar_and_both_picker_fields_save_through_cell,
    wheel_navigation_accumulates_trackpad_pixels,
}

#[cfg_attr(test, test)]
fn wheel_navigation_accumulates_trackpad_pixels() {
    use bevy::input::mouse::MouseScrollUnit;
    let mut total = 0.0;
    assert_eq!(scroll_steps(&mut total, -20.0, MouseScrollUnit::Pixel), 0);
    assert_eq!(scroll_steps(&mut total, -20.0, MouseScrollUnit::Pixel), 0);
    assert_eq!(scroll_steps(&mut total, -20.0, MouseScrollUnit::Pixel), 1);
    assert_eq!(scroll_steps(&mut total, 1.0, MouseScrollUnit::Line), -1);
}
