use super::*;
use bevy::math::DVec2;
use serde_json::json;

#[test]
fn growing_nested_records_keep_valid_layout_and_expand_column_content() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, crate::layout::LayoutPlugin));
    let root = app.world_mut().spawn_empty().id();
    let mut items = Vec::new();
    for height in [300.0, 160.0] {
        items.push(
            app.world_mut()
                .spawn((
                    crate::canvas::CanvasItem {
                        position: DVec2::ZERO,
                        size: Vec2::new(300.0, height),
                    },
                    ChildOf(root),
                    crate::workspace::WorkspaceMember(1),
                ))
                .id(),
        );
    }
    let [column, card] = items.try_into().unwrap();
    let mut rules = crate::layout::Rules::fixed(Vec2::splat(300.0));
    rules.arrangement = crate::layout::Arrangement::Column;
    rules.axes[1].overflow = crate::layout::Overflow::Scroll;
    crate::layout::configure(app.world_mut(), column, rules).unwrap();
    crate::layout::attach(app.world_mut(), card, column).unwrap();
    let mut rules = crate::layout::Rules::fixed(Vec2::new(300.0, 160.0));
    rules.axes[1].sizing = crate::layout::Sizing::Fit;
    crate::layout::configure(app.world_mut(), card, rules).unwrap();
    rows::place(app.world_mut(), card, DVec2::ZERO, Vec2::new(300.0, 600.0));
    assert!(
        app.world()
            .get::<crate::layout::LayoutBox>(card)
            .unwrap()
            .valid()
    );
    app.update();
    let runtime = app
        .world()
        .get::<crate::layout::LayoutRuntime>(column)
        .unwrap();
    assert_eq!(runtime.size.y, 300.0);
    assert!(runtime.content.y >= 600.0);
    assert_eq!(
        app.world()
            .get::<crate::layout::LayoutRuntime>(card)
            .unwrap()
            .parent,
        Some(column)
    );
}

fn fixture() -> (App, Entity, Entity) {
    let mut app = App::new();
    crate::laboratory::isolate(app.world_mut());
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<bevy::input_focus::InputFocus>()
        .add_plugins(ProteinAreaPlugin);
    let root = app
        .world_mut()
        .spawn((
            crate::container::BoxRoot,
            crate::workspace::Workspaces::default(),
        ))
        .id();
    let mut area = InfluenceArea::new(
        crate::area::AreaShape::Square,
        DVec2::ZERO,
        DVec2::splat(800.0),
    );
    area.protein = Some(Config::default());
    let owner = crate::area::spawn_area(app.world_mut(), root, 1, area).unwrap();
    (app, root, owner)
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

#[cfg_attr(test, test)]
fn templates_validate_and_request_only_bound_properties_even_before_rows_exist() {
    let mut config = Config::default();
    config.bindings.push(Binding::new("quantity_exact"));
    let query = config.query().unwrap();
    assert_eq!(
        query.fields.unwrap(),
        ["body", "head", "kind", "organ", "quantity_exact", "uid"]
    );
    let saved = serde_json::to_string(&config).unwrap();
    assert_eq!(serde_json::from_str::<Config>(&saved).unwrap(), config);
    config.bindings[0].width = f32::NAN;
    assert!(!config.valid());
    config.bindings[0].width = 280.0;
    config.draft.query["aggregate"] = json!({"op":"count","group":"total"});
    assert!(config.query().is_err());
    config = Config::default();
    let mut date = Binding::new("due_date");
    date.editable = true;
    config.bindings.push(date);
    assert_eq!(
        config.query().unwrap().include.extension.unwrap().namespace,
        "work"
    );
}

#[cfg_attr(test, tokio::test)]
async fn area_reads_real_records_preserves_entities_edits_and_deletes_through_actions() {
    let (mut app, _, owner) = fixture();
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    let uid = engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "Task".into(),
                body: "Details".into(),
                quantity: 1.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let cell = cell::CellRuntime {
        engine: engine.clone(),
        store: engine.store.clone(),
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        information: None,
    };
    app.insert_resource(crate::app::CellHandle(cell))
        .insert_resource(crate::wake::WakeSignal::new(|| {}))
        .add_plugins(crate::cell_bridge::CellBridgePlugin);
    {
        let mut area = app.world_mut().get_mut::<InfluenceArea>(owner).unwrap();
        let config = area.protein.as_mut().unwrap();
        config.enabled = true;
        config.bindings[0].editable = true;
        config.draft.query["where"] = json!([{"all":[{"uid_eq":uid}]}]);
    }
    until(&mut app, |world| {
        world
            .resource::<Runtime>()
            .areas
            .get(&owner)
            .is_some_and(|state| state.row_entities.len() == 1)
    })
    .await;
    let row = app.world().resource::<Runtime>().areas[&owner].row_entities[&uid];
    app.world_mut()
        .get_mut::<crate::canvas::CanvasItem>(row)
        .unwrap()
        .position = DVec2::new(700.0, 800.0);
    engine
        .act(
            engine::actions::Action::EditRecordText {
                target: uid.clone(),
                head: None,
                body: Some("Updated".into()),
            },
            None,
        )
        .await
        .unwrap();
    until(&mut app, |world| {
        world
            .get::<crate::area::RecordProperties>(row)
            .is_some_and(|row| row.0["body"] == "Updated")
    })
    .await;
    assert_eq!(
        app.world().resource::<Runtime>().areas[&owner].row_entities[&uid],
        row
    );
    assert_eq!(
        app.world()
            .get::<crate::canvas::CanvasItem>(row)
            .unwrap()
            .position,
        DVec2::new(700.0, 800.0)
    );
    let binding = app.world().get::<RecordBinding>(row).unwrap().clone();
    let editor = app
        .world_mut()
        .query::<(Entity, &rows::PropertyEditor)>()
        .iter(app.world())
        .map(|(entity, _)| entity)
        .next()
        .unwrap();
    app.world_mut()
        .get_mut::<bevy::text::EditableText>(editor)
        .unwrap()
        .editor
        .set_text("Renamed");
    rows::save_field(app.world_mut(), editor);
    until(&mut app, |world| {
        world
            .get::<crate::area::RecordProperties>(row)
            .is_some_and(|row| row.0["head"] == "Renamed")
    })
    .await;
    assert!(
        execute(
            app.world_mut(),
            &binding,
            row,
            engine::actions::Action::DeleteRecord {
                target: "another-record".into()
            }
        )
        .is_err()
    );
    execute(
        app.world_mut(),
        &binding,
        row,
        engine::actions::Action::DeleteRecord {
            target: uid.clone(),
        },
    )
    .unwrap();
    until(&mut app, |world| {
        world.resource::<Runtime>().areas[&owner].data.is_empty()
    })
    .await;
    assert!(app.world().get_entity(row).is_err());
    assert!(
        execute(
            app.world_mut(),
            &binding,
            row,
            engine::actions::Action::DeleteRecord { target: uid }
        )
        .is_err()
    );
}

#[cfg_attr(test, test)]
fn remote_rows_are_isolated_cancelled_replies_are_ignored_and_large_views_are_bounded() {
    let (mut app, _, owner) = fixture();
    let config = Config {
        source: Source::Organ("remote".into()),
        enabled: true,
        ..default()
    };
    app.world_mut().resource_mut::<Runtime>().areas.insert(
        owner,
        State {
            applied: Some(config),
            subscription: Some("current".into()),
            ready: true,
            ..default()
        },
    );
    receive(
        app.world_mut(),
        owner,
        ServerMessage::Snapshot {
            id: "cancelled".into(),
            rows: vec![json!({"uid":"wrong"})],
        },
    );
    assert!(
        app.world().resource::<Runtime>().areas[&owner]
            .data
            .is_empty()
    );
    let rows: Vec<_> = (0..1000)
        .map(|index| json!({"uid":format!("record-{index}"), "head":"Task", "body":"Details"}))
        .collect();
    receive(
        app.world_mut(),
        owner,
        ServerMessage::Error {
            id: "another-sand".into(),
            message: "Unrelated failure".into(),
            code: None,
        },
    );
    assert!(
        app.world().resource::<Runtime>().areas[&owner]
            .status
            .is_empty()
    );
    receive(
        app.world_mut(),
        owner,
        ServerMessage::Snapshot {
            id: "current".into(),
            rows: rows.clone(),
        },
    );
    rows::reconcile(app.world_mut(), owner);
    let before = app.world_mut().query::<&Node>().iter(app.world()).count();
    assert_eq!(
        app.world().resource::<Runtime>().areas[&owner]
            .row_entities
            .len(),
        200
    );
    let row = *app.world().resource::<Runtime>().areas[&owner]
        .row_entities
        .values()
        .next()
        .unwrap();
    assert!(app.world().get::<RemoteRecord>(row).is_some());
    let mut binding = app.world().get::<RecordBinding>(row).unwrap().clone();
    binding.source = Source::Local;
    assert!(
        execute(
            app.world_mut(),
            &binding,
            row,
            engine::actions::Action::DeleteRecord {
                target: binding.uid.clone()
            }
        )
        .is_err()
    );
    receive(
        app.world_mut(),
        owner,
        ServerMessage::Update {
            id: "current".into(),
            rows,
        },
    );
    rows::reconcile(app.world_mut(), owner);
    assert_eq!(
        before,
        app.world_mut().query::<&Node>().iter(app.world()).count()
    );
    receive(
        app.world_mut(),
        owner,
        ServerMessage::Error {
            id: "connection".into(),
            message: "Revoked".into(),
            code: None,
        },
    );
    rows::reconcile(app.world_mut(), owner);
    assert!(app.world().get_entity(row).is_err());
    assert!(!app.world().resource::<Runtime>().areas[&owner].ready);
}

crate::laboratory_cases! {
    async independent_sorting_uses_live_protein_order_without_spawning_and_clears_on_stop,
    async protein_pull_filters_query_real_properties_without_spawning_and_isolate_sources,
    protein_filter_errors_clear_membership_and_quiet_updates_reuse_the_cache,
    async grouping_updates_from_live_record_actions_without_displaying_the_group_property,
    grouped_cards_follow_physics_and_pinning,
    grouping_orders_hidden_properties_sets_dates_and_exact_numbers,
    grouping_reuses_areas_limits_forces_to_members_and_cleans_up,
    grouping_pages_are_bounded_and_direction_changes_keep_query_order,
    async relationship_and_work_log_controls_apply_backend_actions,
    clicking_any_property_reports_the_record_and_source_once,
    area_configuration_roundtrips_without_rows_or_credentials_and_rejects_invalid_shapes,
    templates_validate_and_request_only_bound_properties_even_before_rows_exist,
    async area_reads_real_records_preserves_entities_edits_and_deletes_through_actions,
    remote_rows_are_isolated_cancelled_replies_are_ignored_and_large_views_are_bounded,
}

#[cfg_attr(test, test)]
fn clicking_any_property_reports_the_record_and_source_once() {
    #[derive(Resource, Default)]
    struct Clicks(Vec<RecordClicked>);
    let (mut app, root, owner) = fixture();
    app.init_resource::<Clicks>().add_observer(
        |event: On<RecordClicked>, mut clicks: ResMut<Clicks>| {
            clicks.0.push(event.event().clone());
        },
    );
    app.world_mut().resource_mut::<Runtime>().areas.insert(
        owner,
        State {
            applied: Some(Config::default()),
            data: vec![json!({"uid":"record","head":"Title","body":"Details"})],
            dirty: true,
            ..default()
        },
    );
    rows::reconcile(app.world_mut(), owner);
    let row = app.world().resource::<Runtime>().areas[&owner].row_entities["record"];
    let property = app.world().get::<Children>(row).unwrap()[0];
    let text = app.world().get::<Children>(property).unwrap()[0];
    let window = app.world_mut().spawn(Window::default()).id();
    let location = bevy::picking::pointer::Location {
        target: bevy::camera::RenderTarget::Window(bevy::window::WindowRef::Entity(window))
            .normalize(None)
            .unwrap(),
        position: Vec2::ZERO,
    };
    app.world_mut().trigger(bevy::picking::events::Pointer::new(
        bevy::picking::pointer::PointerId::Mouse,
        location,
        bevy::picking::events::Click {
            button: bevy::picking::pointer::PointerButton::Primary,
            hit: bevy::picking::backend::HitData::new(window, 0.0, None, None),
            duration: std::time::Duration::from_millis(20),
            count: 1,
        },
        text,
    ));
    app.world_mut().flush();
    let clicks = &app.world().resource::<Clicks>().0;
    assert_eq!(clicks.len(), 1);
    assert_eq!(clicks[0].entity, root);
    assert_eq!(clicks[0].uid, "record");
    assert_eq!(clicks[0].source, Source::Local);
}

#[cfg_attr(test, test)]
fn area_configuration_roundtrips_without_rows_or_credentials_and_rejects_invalid_shapes() {
    let (mut app, _, owner) = fixture();
    let mut area = app.world().get::<InfluenceArea>(owner).unwrap().clone();
    let config = area.protein.as_mut().unwrap();
    config.source = Source::Organ("o_remote".into());
    config.bindings[0].editable = true;
    config.bindings[1].overflow = OverflowMode::GrowDown;
    config.bindings.push(Binding::new("assignees"));
    let json = serde_json::to_string(&area).unwrap();
    assert!(!json.contains("password"));
    let restored: InfluenceArea = serde_json::from_str(&json).unwrap();
    assert_eq!(area, restored);
    assert!(restored.validate());
    app.world_mut().entity_mut(owner).insert(restored);
    let config = &mut app
        .world_mut()
        .get_mut::<InfluenceArea>(owner)
        .unwrap()
        .protein
        .as_mut()
        .unwrap()
        .clone();
    config.columns = usize::MAX;
    assert!(!config.valid());
}

#[cfg_attr(test, tokio::test)]
async fn relationship_and_work_log_controls_apply_backend_actions() {
    use crate::actions::Action as _;
    use property_actions::{Command, Form};
    let (mut app, _, owner) = fixture();
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    let mut ids = Vec::new();
    for (head, kind) in [
        ("Task", nucleus::RecordKind::Plain),
        ("Person", nucleus::RecordKind::Person),
    ] {
        ids.push(
            engine
                .act(
                    engine::actions::Action::CreateRecord {
                        slug: None,
                        kind,
                        head: head.into(),
                        body: String::new(),
                        quantity: 1.0,
                    },
                    None,
                )
                .await
                .unwrap()
                .created
                .unwrap(),
        );
    }
    for name in ["assigned-to", "planned"] {
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
    let cell = cell::CellRuntime {
        engine: engine.clone(),
        store: engine.store.clone(),
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        information: None,
    };
    app.insert_resource(crate::app::CellHandle(cell))
        .insert_resource(crate::wake::WakeSignal::new(|| {}))
        .add_plugins(crate::cell_bridge::CellBridgePlugin);
    {
        let mut area = app.world_mut().get_mut::<InfluenceArea>(owner).unwrap();
        let config = area.protein.as_mut().unwrap();
        config.enabled = true;
        config.bindings = ["assignees", "assertions", "work_logs"]
            .into_iter()
            .map(|key| {
                let mut binding = Binding::new(key);
                binding.editable = true;
                binding
            })
            .collect();
        config.draft.query["where"] = json!([{"all":[{"uid_eq":ids[0]}]}]);
    }
    until(&mut app, |world| {
        world
            .resource::<Runtime>()
            .areas
            .get(&owner)
            .is_some_and(|state| !state.data.is_empty())
    })
    .await;
    let find = |world: &mut World, property: &str| {
        world
            .query::<(Entity, &Form)>()
            .iter(world)
            .find(|(_, form)| form.property == property)
            .map(|(entity, _)| entity)
            .unwrap()
    };
    let form = find(app.world_mut(), "assignees");
    let field = app.world().get::<Form>(form).unwrap().fields[0].0;
    app.world_mut()
        .get_mut::<bevy::text::EditableText>(field)
        .unwrap()
        .editor
        .set_text(&ids[1]);
    Command::AddRelation.apply(app.world_mut(), form);
    until(&mut app, |world| {
        world.resource::<Runtime>().areas[&owner].data[0]["assignees"]
            .as_array()
            .is_some_and(|values| values.len() == 1)
    })
    .await;
    let assertion =
        app.world().resource::<Runtime>().areas[&owner].data[0]["assignees"][0]["assertion"]
            .as_str()
            .unwrap()
            .to_string();
    let form = find(app.world_mut(), "assignees");
    Command::RemoveRelation(assertion).apply(app.world_mut(), form);
    until(&mut app, |world| {
        world.resource::<Runtime>().areas[&owner].data[0]["assignees"] == json!([])
    })
    .await;
    let form = find(app.world_mut(), "assertions");
    let field = app.world().get::<Form>(form).unwrap().fields[0].0;
    app.world_mut()
        .get_mut::<bevy::text::EditableText>(field)
        .unwrap()
        .editor
        .set_text("planned");
    Command::AddRelation.apply(app.world_mut(), form);
    until(&mut app, |world| {
        world.resource::<Runtime>().areas[&owner].data[0]["assertions"]
            .as_array()
            .is_some_and(|values| values.iter().any(|v| v["predicate"] == "planned"))
    })
    .await;
    let form = find(app.world_mut(), "work_logs");
    let fields = app.world().get::<Form>(form).unwrap().fields.clone();
    for ((entity, _), value) in fields
        .iter()
        .zip(["2026-09-13T10:00:00Z", "2026-09-13T10:30:00Z"])
    {
        app.world_mut()
            .get_mut::<bevy::text::EditableText>(*entity)
            .unwrap()
            .editor
            .set_text(value);
    }
    Command::AddLog.apply(app.world_mut(), form);
    until(&mut app, |world| {
        world.resource::<Runtime>().areas[&owner].data[0]["work_logs"]
            .as_array()
            .is_some_and(|values| values.len() == 1)
    })
    .await;
    let form = find(app.world_mut(), "work_logs");
    Command::RemoveLog.apply(app.world_mut(), form);
    until(&mut app, |world| {
        world.resource::<Runtime>().areas[&owner].data[0]["work_logs"] == json!([])
    })
    .await;
}

#[cfg_attr(test, test)]
fn grouping_orders_hidden_properties_sets_dates_and_exact_numbers() {
    let mut config = Config {
        bindings: vec![Binding::new("head")],
        ..default()
    };
    config.grouping.horizontal = Some(GroupAxis::new("due_date"));
    config.grouping.vertical = Some(GroupAxis::new("assignees"));
    let fields = config.query().unwrap().fields.unwrap();
    assert!(fields.iter().any(|key| key == "due_date"));
    assert!(fields.iter().any(|key| key == "assignees"));
    assert!(!fields.iter().any(|key| key == "body"));
    let data = vec![
        json!({"uid":"bob", "assignees":[{"uid":"b","head":"Bob"}], "due_date":"2026-09-01"}),
        json!({"uid":"late", "assignees":[{"uid":"a","head":"alice"}], "due_date":"2026-10-01"}),
        json!({"uid":"unset", "assignees":[], "due_date":null}),
        json!({"uid":"early", "assignees":[{"uid":"a","head":"alice"}], "due_date":"2026-09-01"}),
    ];
    let page = grouping::page(&data, &config, 0);
    assert_eq!(
        page.iter()
            .map(|v| v["uid"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["early", "late", "bob", "unset"]
    );
    config.grouping.vertical = None;
    config.grouping.horizontal = Some(GroupAxis::new("quantity_exact"));
    let amounts = [
        "10",
        "2",
        "-3.5",
        "-12",
        "9007199254740993.125",
        "9007199254740993.124",
    ];
    let data: Vec<_> = amounts
        .iter()
        .map(|value| json!({"uid":value,"quantity_exact":value}))
        .collect();
    let page = grouping::page(&data, &config, 0);
    assert_eq!(
        page.iter()
            .map(|v| v["uid"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "-12",
            "-3.5",
            "2",
            "10",
            "9007199254740993.124",
            "9007199254740993.125"
        ]
    );
    config.grouping.horizontal.as_mut().unwrap().descending = true;
    let page = grouping::page(&data, &config, 0);
    assert_eq!(page[0]["uid"], "9007199254740993.125");
    let saved = serde_json::to_string(&config).unwrap();
    assert_eq!(serde_json::from_str::<Config>(&saved).unwrap(), config);
    config.grouping.horizontal.as_mut().unwrap().property = "secret".into();
    assert!(!config.valid());
}

#[cfg_attr(test, test)]
fn grouping_reuses_areas_limits_forces_to_members_and_cleans_up() {
    use crate::canvas::CanvasItem;
    use grouping::{Cell, GeneratedGroup};
    let (mut app, root, owner) = fixture();
    app.world_mut()
        .get_mut::<InfluenceArea>(owner)
        .unwrap()
        .reach
        .mode = crate::area::ReachMode::Unlimited;
    let mut config = Config {
        bindings: vec![Binding::new("head")],
        ..default()
    };
    config.grouping.horizontal = Some(GroupAxis::new("due_date"));
    config.grouping.vertical = Some(GroupAxis::new("assignees"));
    let people = json!([{"uid":"a","head":"Alex"},{"uid":"b","head":"Bea"}]);
    let reversed = json!([{"uid":"b","head":"Bea"},{"uid":"a","head":"Alex"}]);
    let data = vec![
        json!({"uid":"a", "head":"First", "due_date":"2026-09-01", "assignees":people}),
        json!({"uid":"b", "head":"Second", "due_date":"2026-09-01", "assignees":reversed}),
        json!({"uid":"c", "head":"Third", "due_date":"2026-10-01", "assignees":[]}),
    ];
    app.world_mut().resource_mut::<Runtime>().areas.insert(
        owner,
        State {
            applied: Some(config.clone()),
            data: data.clone(),
            dirty: true,
            ..default()
        },
    );
    rows::reconcile(app.world_mut(), owner);
    rows::layout(app.world_mut());
    let state = &app.world().resource::<Runtime>().areas[&owner];
    assert_eq!(state.groups.len(), 4);
    assert_eq!(state.row_entities.len(), 3);
    let a = state.row_entities["a"];
    let b = state.row_entities["b"];
    let c = state.row_entities["c"];
    let first = *app.world().get::<Cell>(a).unwrap();
    let second = *app.world().get::<Cell>(b).unwrap();
    assert_eq!((first.x, first.y), (second.x, second.y));
    assert_ne!(first.slot, second.slot);
    assert!(
        app.world().get::<CanvasItem>(b).unwrap().position.y
            > app.world().get::<CanvasItem>(a).unwrap().position.y
    );
    let groups = state.groups.clone();
    let outsider = app
        .world_mut()
        .spawn((
            crate::area::RecordProperties(data[0].clone()),
            ChildOf(root),
        ))
        .id();
    for entity in groups.values() {
        assert!(!crate::area_panel::owns(app.world(), root, *entity));
        let group = app.world().get::<GeneratedGroup>(*entity).unwrap();
        let area = app.world().get::<InfluenceArea>(*entity).unwrap();
        assert!(area.validate());
        assert_eq!(
            group.force(area, outsider, DVec2::splat(500.0)),
            DVec2::ZERO
        );
        for (sand, target) in &group.targets {
            let force = group.force(area, *sand, *target + DVec2::splat(100.0));
            assert_eq!(
                if group.horizontal { force } else { force.yx() },
                DVec2::new(-100.0, 0.0)
            );
            assert_eq!(group.force(area, *sand, *target), DVec2::ZERO);
        }
    }
    {
        let state = app
            .world_mut()
            .resource_mut::<Runtime>()
            .into_inner()
            .areas
            .get_mut(&owner)
            .unwrap();
        state.dirty = true;
    }
    rows::reconcile(app.world_mut(), owner);
    assert_eq!(
        app.world().resource::<Runtime>().areas[&owner].groups,
        groups
    );
    {
        let state = app
            .world_mut()
            .resource_mut::<Runtime>()
            .into_inner()
            .areas
            .get_mut(&owner)
            .unwrap();
        state.data[2]["assignees"] = people;
        state.data[2]["due_date"] = json!("2026-09-01");
        state.dirty = true;
    }
    rows::reconcile(app.world_mut(), owner);
    rows::layout(app.world_mut());
    assert_eq!(
        app.world().resource::<Runtime>().areas[&owner].row_entities["c"],
        c
    );
    assert_eq!(
        app.world().resource::<Runtime>().areas[&owner].groups.len(),
        2
    );
    assert_eq!(app.world().get::<Cell>(c).unwrap().y, first.y);
    stop(app.world_mut(), owner);
    assert!(app.world().get_entity(a).is_err());
    assert!(
        app.world_mut()
            .query::<&GeneratedGroup>()
            .iter(app.world())
            .next()
            .is_none()
    );
}

#[cfg_attr(test, test)]
fn grouping_pages_are_bounded_and_direction_changes_keep_query_order() {
    let (mut app, _, owner) = fixture();
    let mut config = Config {
        bindings: vec![Binding::new("head")],
        ..default()
    };
    config.grouping.horizontal = Some(GroupAxis::new("quantity_exact"));
    let data: Vec<_> = (0..1000)
        .rev()
        .map(|n| json!({"uid":format!("r-{n}"),"quantity_exact":n.to_string(),"head":"Task"}))
        .collect();
    let page = grouping::page(&data, &config, 1);
    assert_eq!(page.len(), 200);
    assert_eq!(page[0]["uid"], "r-200");
    config.grouping.horizontal.as_mut().unwrap().reverse = true;
    assert_eq!(grouping::page(&data, &config, 1), page);
    app.world_mut().resource_mut::<Runtime>().areas.insert(
        owner,
        State {
            applied: Some(config),
            data,
            dirty: true,
            page: 1,
            ..default()
        },
    );
    rows::reconcile(app.world_mut(), owner);
    rows::layout(app.world_mut());
    let state = &app.world().resource::<Runtime>().areas[&owner];
    assert_eq!(state.groups.len(), 200);
    assert_eq!(state.row_entities.len(), 200);
    let first = state.row_entities["r-200"];
    let last = state.row_entities["r-399"];
    assert!(
        app.world()
            .get::<crate::canvas::CanvasItem>(first)
            .unwrap()
            .position
            .x
            > app
                .world()
                .get::<crate::canvas::CanvasItem>(last)
                .unwrap()
                .position
                .x
    );
    let before = app.world().entities().len();
    app.world_mut()
        .resource_mut::<Runtime>()
        .areas
        .get_mut(&owner)
        .unwrap()
        .dirty = true;
    rows::reconcile(app.world_mut(), owner);
    assert_eq!(app.world().entities().len(), before);
}

#[cfg_attr(test, tokio::test)]
async fn grouping_updates_from_live_record_actions_without_displaying_the_group_property() {
    let (mut app, _, owner) = fixture();
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    let uid = engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "Grouped task".into(),
                body: String::new(),
                quantity: 1.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let cell = cell::CellRuntime {
        engine: engine.clone(),
        store: engine.store.clone(),
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        information: None,
    };
    app.insert_resource(crate::app::CellHandle(cell))
        .insert_resource(crate::wake::WakeSignal::new(|| {}))
        .add_plugins(crate::cell_bridge::CellBridgePlugin);
    {
        let mut area = app.world_mut().get_mut::<InfluenceArea>(owner).unwrap();
        let config = area.protein.as_mut().unwrap();
        config.enabled = true;
        config.bindings = vec![Binding::new("head")];
        config.grouping.vertical = Some(GroupAxis::new("due_date"));
        config.draft.query["where"] = json!([{"all":[{"uid_eq":uid}]}]);
    }
    until(&mut app, |world| {
        world
            .resource::<Runtime>()
            .areas
            .get(&owner)
            .is_some_and(|s| s.row_entities.len() == 1)
    })
    .await;
    let row = app.world().resource::<Runtime>().areas[&owner].row_entities[&uid];
    let binding = app.world().get::<RecordBinding>(row).unwrap().clone();
    execute(
        app.world_mut(),
        &binding,
        row,
        engine::actions::Action::SetExtension {
            target: uid.clone(),
            namespace: "work".into(),
            fds: json!({"due":"2026-09-30"}),
        },
    )
    .unwrap();
    until(&mut app, |world| {
        world
            .get::<crate::area::RecordProperties>(row)
            .is_some_and(|p| p.0["due_date"] == "2026-09-30")
    })
    .await;
    let state = &app.world().resource::<Runtime>().areas[&owner];
    assert_eq!(state.row_entities[&uid], row);
    assert_eq!(state.groups.len(), 1);
    let group = *state.groups.values().next().unwrap();
    assert_eq!(
        app.world().get::<InfluenceArea>(group).unwrap().name,
        "2026-09-30"
    );
    assert!(
        app.world()
            .get::<crate::area::RecordProperties>(row)
            .unwrap()
            .0
            .get("body")
            .is_none()
    );
}

#[cfg_attr(test, test)]
fn grouped_cards_follow_physics_and_pinning() {
    use crate::canvas::CanvasItem;
    let (mut app, root, owner) = fixture();
    app.add_plugins(crate::physics::WorkspacePhysicsPlugin)
        .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(16),
        ));
    app.finish();
    let mut config = Config {
        bindings: vec![Binding::new("head")],
        ..default()
    };
    config.grouping.horizontal = Some(GroupAxis::new("kind"));
    config.grouping.strength = 400.0;
    {
        let mut area = app.world_mut().get_mut::<InfluenceArea>(owner).unwrap();
        area.protein = Some(config.clone());
        area.reach.mode = crate::area::ReachMode::Unlimited;
    }
    app.world_mut().resource_mut::<Runtime>().areas.insert(
        owner,
        State {
            applied: Some(config),
            data: vec![json!({"uid":"a","head":"Card","kind":"plain"})],
            dirty: true,
            ..default()
        },
    );
    rows::reconcile(app.world_mut(), owner);
    rows::layout(app.world_mut());
    let row = app.world().resource::<Runtime>().areas[&owner].row_entities["a"];
    let target = app.world().get::<CanvasItem>(row).unwrap().position;
    let displaced = target + DVec2::new(300.0, 0.0);
    app.world_mut().get_mut::<CanvasItem>(row).unwrap().position = displaced;
    crate::workspace_config::set_physics(app.world_mut(), root, 1, true);
    for _ in 0..120 {
        app.update();
    }
    let moved = app.world().get::<CanvasItem>(row).unwrap().position;
    assert!(moved.x < displaced.x - 50.0);
    assert!((moved.y - displaced.y).abs() < 0.01);
    app.world_mut()
        .entity_mut(row)
        .insert(crate::sand_placement::Pinned {
            anchor: [0.5, 0.5],
            scale: 1.0,
        });
    for _ in 0..60 {
        app.update();
    }
    assert_eq!(app.world().get::<CanvasItem>(row).unwrap().position, moved);
}

#[cfg_attr(test, tokio::test)]
async fn protein_pull_filters_query_real_properties_without_spawning_and_isolate_sources() {
    use crate::{
        area::{AreaForces, RecordProperties},
        canvas::CanvasItem,
        workspace::WorkspaceMember,
    };
    use engine::actions::Action as Backend;
    let (mut app, root, owner) = fixture();
    app.add_systems(PostUpdate, crate::area::forces);
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    let mut ids = Vec::new();
    for (head, slug, quantity) in [
        ("Need", "need", -3.0),
        ("Priority", "priority-task", 2.0),
        ("Excluded", "excluded", -10.0),
    ] {
        ids.push(
            engine
                .act(
                    Backend::CreateRecord {
                        slug: Some(slug.into()),
                        kind: nucleus::RecordKind::Plain,
                        head: head.into(),
                        body: "Private details".into(),
                        quantity,
                    },
                    None,
                )
                .await
                .unwrap()
                .created
                .unwrap(),
        );
    }
    engine
        .act(
            Backend::CreateConcept {
                lingua: "g_local".into(),
                name: "priority".into(),
                parents: vec![],
            },
            None,
        )
        .await
        .unwrap();
    let assertion = engine
        .act(
            Backend::AssertRecord {
                subject: ids[1].clone(),
                predicate: "priority".into(),
                object: None,
                quantity: None,
                unit: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let cell = cell::CellRuntime {
        engine: engine.clone(),
        store: engine.store.clone(),
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        information: None,
    };
    app.insert_resource(crate::app::CellHandle(cell))
        .insert_resource(crate::wake::WakeSignal::new(|| {}))
        .add_plugins(crate::cell_bridge::CellBridgePlugin);
    let mut config = Config {
        enabled: true,
        bindings: Vec::new(),
        ..default()
    };
    config.draft.query["limit"] = json!(1);
    config.draft.query["where"] = json!([{"all":[{"any":[{"concept_in":"priority"},{"quantity_lt":"0"}]},{"not":{"slug_eq":"excluded"}}]}]);
    let queried = protein::execute(&engine.store, &filter::query(&config).unwrap())
        .await
        .unwrap();
    assert_eq!(queried.len(), 2, "{queried:?}");
    {
        let mut area = app.world_mut().get_mut::<InfluenceArea>(owner).unwrap();
        area.protein = None;
        area.filter = Some(config);
        area.strength = 100.0;
    }
    let mut sands = Vec::new();
    for (index, uid) in ids.iter().enumerate() {
        sands.push(
            app.world_mut()
                .spawn((
                    CanvasItem {
                        position: DVec2::new(100.0 + index as f64 * 50.0, 0.0),
                        size: Vec2::splat(20.0),
                    },
                    RecordProperties(json!({"uid":uid})),
                    ChildOf(root),
                    WorkspaceMember(1),
                ))
                .id(),
        );
    }
    let remote = app
        .world_mut()
        .spawn((
            CanvasItem {
                position: DVec2::new(100.0, 200.0),
                size: Vec2::splat(20.0),
            },
            RecordProperties(json!({"uid":ids[0]})),
            RecordBinding {
                area: owner,
                uid: ids[0].clone(),
                source: Source::Organ("remote".into()),
            },
            RemoteRecord,
            ChildOf(root),
            WorkspaceMember(1),
        ))
        .id();
    until(&mut app, |world| {
        world
            .get::<filter::Matches>(owner)
            .is_some_and(|m| m.uids.len() == 2)
    })
    .await;
    app.update();
    assert!(app.world().get::<AreaForces>(sands[0]).unwrap().total().x < 0.0);
    assert!(app.world().get::<AreaForces>(sands[1]).unwrap().total().x < 0.0);
    assert_eq!(
        app.world().get::<AreaForces>(sands[2]).unwrap().total(),
        DVec2::ZERO
    );
    assert_eq!(
        app.world().get::<AreaForces>(remote).unwrap().total(),
        DVec2::ZERO
    );
    assert_eq!(
        app.world_mut()
            .query::<&CanvasItem>()
            .iter(app.world())
            .count(),
        5
    );
    let subscription = app
        .world_mut()
        .query::<(Entity, &filter::Subscription)>()
        .iter(app.world())
        .find(|(_, s)| s.0 == owner)
        .unwrap()
        .0;
    let state = &app.world().resource::<Runtime>().areas[&subscription];
    assert!(state.row_entities.is_empty());
    assert!(
        state
            .data
            .iter()
            .all(|row| row.get("body").is_none() && row.get("head").is_none())
    );
    engine
        .act(Backend::RetractAssertion { assertion }, None)
        .await
        .unwrap();
    until(&mut app, |world| {
        world
            .get::<filter::Matches>(owner)
            .is_some_and(|m| m.uids.len() == 1)
    })
    .await;
    app.update();
    assert_eq!(
        app.world().get::<AreaForces>(sands[1]).unwrap().total(),
        DVec2::ZERO
    );
    let old_id = app.world().resource::<Runtime>().areas[&subscription]
        .subscription
        .clone()
        .unwrap();
    {
        let mut area = app.world_mut().get_mut::<InfluenceArea>(owner).unwrap();
        area.filter.as_mut().unwrap().draft.query["where"] = json!([{"all":[{"uid_eq":ids[2]}]}]);
    }
    app.update();
    assert!(
        !app.world()
            .get::<filter::Matches>(owner)
            .unwrap()
            .uids
            .contains(&ids[0])
    );
    receive(
        app.world_mut(),
        subscription,
        ServerMessage::Update {
            id: old_id,
            rows: vec![json!({"uid":ids[0]})],
        },
    );
    until(&mut app, |world| {
        world
            .get::<filter::Matches>(owner)
            .is_some_and(|m| m.uids.contains(&ids[2]))
    })
    .await;
    app.world_mut()
        .get_mut::<InfluenceArea>(owner)
        .unwrap()
        .filter
        .as_mut()
        .unwrap()
        .enabled = false;
    app.update();
    assert_eq!(
        app.world().get::<AreaForces>(sands[2]).unwrap().total(),
        DVec2::ZERO
    );
    app.world_mut()
        .get_mut::<InfluenceArea>(owner)
        .unwrap()
        .filter = None;
    app.update();
    assert!(app.world().get_entity(subscription).is_err());
    assert!(app.world().get_entity(sands[0]).is_ok());
}

#[cfg_attr(test, test)]
fn protein_filter_errors_clear_membership_and_quiet_updates_reuse_the_cache() {
    let (mut app, _, owner) = fixture();
    let mut config = Config {
        enabled: true,
        source: Source::Organ("remote".into()),
        ..default()
    };
    config.draft.query["limit"] = json!(1);
    let query = filter::query(&config).unwrap();
    assert_eq!(query.limit, None);
    assert_eq!(query.fields, Some(vec!["uid".into()]));
    app.world_mut()
        .get_mut::<InfluenceArea>(owner)
        .unwrap()
        .filter = Some(config.clone());
    filter::maintain(app.world_mut());
    let subscription = app
        .world_mut()
        .query::<(Entity, &filter::Subscription)>()
        .iter(app.world())
        .next()
        .unwrap()
        .0;
    app.world_mut().resource_mut::<Runtime>().areas.insert(
        subscription,
        State {
            applied: Some(config),
            ready: true,
            subscription: Some("remote-current".into()),
            ..default()
        },
    );
    receive(
        app.world_mut(),
        subscription,
        ServerMessage::Snapshot {
            id: "remote-current".into(),
            rows: vec![json!({"uid":"a"})],
        },
    );
    filter::publish(app.world_mut());
    let matches = app.world().get::<filter::Matches>(owner).unwrap();
    let record = crate::area::RecordProperties(json!({"uid":"a"}));
    let binding = RecordBinding {
        area: owner,
        uid: "a".into(),
        source: Source::Organ("remote".into()),
    };
    assert!(matches.allows(&record, Some(&binding)));
    assert!(!matches.allows(&record, None));
    let before = app
        .world()
        .entity(owner)
        .get_ref::<filter::Matches>()
        .unwrap()
        .last_changed();
    for _ in 0..10 {
        app.world_mut().increment_change_tick();
        filter::publish(app.world_mut());
    }
    assert_eq!(
        app.world()
            .entity(owner)
            .get_ref::<filter::Matches>()
            .unwrap()
            .last_changed(),
        before
    );
    receive(
        app.world_mut(),
        subscription,
        ServerMessage::Error {
            id: "connection".into(),
            message: "Revoked".into(),
            code: None,
        },
    );
    filter::publish(app.world_mut());
    assert!(
        !app.world()
            .get::<filter::Matches>(owner)
            .unwrap()
            .allows(&record, Some(&binding))
    );
}

#[cfg_attr(test, tokio::test)]
async fn independent_sorting_uses_live_protein_order_without_spawning_and_clears_on_stop() {
    use crate::{
        area::{AreaForces, RecordProperties},
        canvas::CanvasItem,
        workspace::WorkspaceMember,
    };
    use engine::actions::Action as Backend;
    let (mut app, root, owner) = fixture();
    app.add_systems(PostUpdate, crate::area::forces);
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    let mut ids = Vec::new();
    for (head, quantity) in [("A", 10.0), ("B", -2.0), ("C", 1.0)] {
        ids.push(
            engine
                .act(
                    Backend::CreateRecord {
                        slug: None,
                        kind: nucleus::RecordKind::Plain,
                        head: head.into(),
                        body: String::new(),
                        quantity,
                    },
                    None,
                )
                .await
                .unwrap()
                .created
                .unwrap(),
        );
    }
    let cell = cell::CellRuntime {
        engine: engine.clone(),
        store: engine.store.clone(),
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        information: None,
    };
    app.insert_resource(crate::app::CellHandle(cell))
        .insert_resource(crate::wake::WakeSignal::new(|| {}))
        .add_plugins(crate::cell_bridge::CellBridgePlugin);
    let mut config = Config {
        enabled: true,
        bindings: Vec::new(),
        ..default()
    };
    config.draft.query["order"] = json!([{"asc":"quantity"}]);
    config.draft.query["limit"] = json!(1);
    {
        let mut area = app.world_mut().get_mut::<InfluenceArea>(owner).unwrap();
        area.protein = None;
        area.filter = Some(config);
        area.sorting = Some(crate::area_effects::Sorting::default());
        area.reach.mode = crate::area::ReachMode::Unlimited;
    }
    let sands: Vec<_> = ids
        .iter()
        .map(|uid| {
            app.world_mut()
                .spawn((
                    CanvasItem {
                        position: DVec2::ZERO,
                        size: Vec2::splat(40.0),
                    },
                    RecordProperties(json!({"uid":uid})),
                    WorkspaceMember(1),
                    ChildOf(root),
                ))
                .id()
        })
        .collect();
    let remote = app
        .world_mut()
        .spawn((
            CanvasItem {
                position: DVec2::ZERO,
                size: Vec2::splat(40.0),
            },
            RecordProperties(json!({"uid":ids[1]})),
            RecordBinding {
                area: owner,
                uid: ids[1].clone(),
                source: Source::Organ("elsewhere".into()),
            },
            WorkspaceMember(1),
            ChildOf(root),
        ))
        .id();
    until(&mut app, |world| {
        world
            .get::<AreaForces>(sands[0])
            .is_some_and(|f| f.total().y > 0.0)
    })
    .await;
    assert!(app.world().get::<AreaForces>(sands[1]).unwrap().total().y < 0.0);
    assert_eq!(
        app.world().get::<AreaForces>(sands[2]).unwrap().total(),
        DVec2::ZERO
    );
    assert_eq!(
        app.world().get::<AreaForces>(remote).unwrap().total(),
        DVec2::ZERO
    );
    assert_eq!(
        app.world_mut()
            .query::<&CanvasItem>()
            .iter(app.world())
            .count(),
        5
    );
    engine
        .act(
            Backend::SetQuantityExact {
                target: ids[0].clone(),
                amount: "-5".into(),
            },
            None,
        )
        .await
        .unwrap();
    until(&mut app, |world| {
        world
            .get::<AreaForces>(sands[0])
            .is_some_and(|f| f.total().y < 0.0)
    })
    .await;
    assert!(app.world().get::<AreaForces>(sands[2]).unwrap().total().y > 0.0);
    app.world_mut()
        .get_mut::<InfluenceArea>(owner)
        .unwrap()
        .filter
        .as_mut()
        .unwrap()
        .enabled = false;
    app.update();
    for sand in sands {
        assert_eq!(
            app.world().get::<AreaForces>(sand).unwrap().total(),
            DVec2::ZERO
        );
    }
}
