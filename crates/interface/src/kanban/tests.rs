use super::*;

fn fixture() -> (App, Entity, Entity) {
    let mut app = App::new();
    crate::laboratory::isolate(app.world_mut());
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<bevy::input_focus::InputFocus>()
        .add_plugins((
            crate::protein_area::ProteinAreaPlugin,
            crate::area_mutation::AreaMutationPlugin,
            crate::layout::LayoutPlugin,
            KanbanPlugin,
        ));
    let root = app
        .world_mut()
        .spawn((
            crate::container::BoxRoot,
            crate::workspace::Workspaces::default(),
        ))
        .id();
    let board = spawn(app.world_mut(), root, 1, DVec2::ZERO).unwrap();
    (app, root, board)
}

async fn until(app: &mut App, predicate: impl Fn(&World) -> bool) {
    let result = tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            app.update();
            if predicate(app.world()) {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await;
    if result.is_err() {
        let status: Vec<_> = app
            .world_mut()
            .query::<&View>()
            .iter(app.world())
            .map(|view| (view.status.clone(), view.setup))
            .collect();
        let areas: Vec<_> = app
            .world_mut()
            .query::<(Entity, &InfluenceArea)>()
            .iter(app.world())
            .map(|(entity, area)| {
                (
                    area.name.clone(),
                    crate::protein_area::calendar_status(app.world(), entity).to_string(),
                    app.world()
                        .get::<crate::area_mutation::MutationStatus>(entity)
                        .map(|s| s.0.clone()),
                )
            })
            .collect();
        panic!("Kanban did not settle: {status:?}; {areas:?}");
    }
}

#[test]
fn preset_uses_valid_connected_areas_and_editable_task_fields() {
    let (mut app, root, owner) = fixture();
    let board = app.world().get::<Kanban>(owner).unwrap().clone();
    assert!(board.valid());
    let areas: Vec<_> = app
        .world_mut()
        .query::<&InfluenceArea>()
        .iter(app.world())
        .cloned()
        .collect();
    assert_eq!(areas.len(), 8);
    for area in &areas {
        assert!(area.validate(), "{}", area.name);
        if let Some(config) = area.protein.as_ref().or(area.filter.as_ref()) {
            config.query().unwrap();
        }
    }
    assert_eq!(
        config().bindings,
        crate::full_record::config("record", crate::protein_area::Source::Local).bindings
    );
    app.update();
    assert_eq!(
        app.world_mut().query::<&Part>().iter(app.world()).count(),
        7
    );
    assert_eq!(
        app.world_mut()
            .query_filtered::<&crate::castle::Castle, With<CanvasItem>>()
            .iter(app.world())
            .count(),
        7
    );
    assert!(
        !app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0 == "Kanban")
    );
    let source = area(app.world(), owner, &board.source).unwrap();
    assert_eq!(
        crate::topology::groups::members(app.world(), source).len(),
        15
    );
    let saved = snapshot(app.world_mut(), root);
    assert_eq!(saved.len(), 1);
    assert!(saved[0].valid());
}

#[tokio::test]
async fn setup_creation_transfer_and_exit_preserve_card_identity() {
    let (mut app, _, owner) = fixture();
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    app.insert_resource(crate::app::CellHandle(cell::CellRuntime {
        engine: engine.clone(),
        store: engine.store.clone(),
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        information: None,
    }))
    .insert_resource(crate::wake::WakeSignal::new(|| {}))
    .add_plugins(crate::cell_bridge::CellBridgePlugin);
    app.update();
    let board = app.world().get::<Kanban>(owner).unwrap().clone();
    let backlog = area(app.world(), owner, &board.columns[0].area).unwrap();
    let todo = area(app.world(), owner, &board.columns[1].area).unwrap();
    until(&mut app, |world| {
        crate::area_mutation::armed(world, backlog) && crate::area_mutation::armed(world, todo)
    })
    .await;
    Command::Add(0).apply(app.world_mut(), owner);
    let source = area(app.world(), owner, &board.source).unwrap();
    until(&mut app, |world| {
        crate::protein_area::calendar_feed(world, source)
            .is_some_and(|(rows, _)| rows.iter().any(|r| r["head"] == ""))
    })
    .await;
    let uid = crate::protein_area::calendar_feed(app.world(), source)
        .unwrap()
        .0
        .iter()
        .find(|r| r["head"] == "")
        .unwrap()["uid"]
        .as_str()
        .unwrap()
        .to_string();
    until(&mut app, |world| {
        world
            .get::<Children>(world.get::<ChildOf>(owner).unwrap().parent())
            .unwrap()
            .iter()
            .any(|e| world.get::<RecordBinding>(e).is_some_and(|r| r.uid == uid))
    })
    .await;
    let card = app
        .world_mut()
        .query::<(Entity, &RecordBinding)>()
        .iter(app.world())
        .find(|(e, b)| b.uid == uid && app.world().get::<CanvasItem>(*e).is_some())
        .unwrap()
        .0;
    for _ in 0..4 {
        app.update();
    }
    assert_eq!(app.world().get::<Card>(card).unwrap().column, Some(backlog));
    let earlier = engine
        .act(
            engine::actions::Action::CreateRecordWithTags {
                head: "Earlier task".into(),
                body: String::new(),
                quantity: 0.0,
                tags: vec!["task".into(), "backlog".into()],
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let today = chrono::Utc::now().date_naive();
    for (target, due) in [
        (&uid, (today + chrono::TimeDelta::days(10)).to_string()),
        (&earlier, (today + chrono::TimeDelta::days(1)).to_string()),
    ] {
        engine
            .act(
                engine::actions::Action::SetExtension {
                    target: target.clone(),
                    namespace: "work".into(),
                    fds: json!({"due":due}),
                },
                None,
            )
            .await
            .unwrap();
    }
    until(&mut app, |world| {
        world
            .get::<LayoutBox>(card)
            .is_some_and(|layout| layout.order == 1)
    })
    .await;
    let destination = crate::topology::position(app.world(), todo).unwrap();
    app.world_mut()
        .init_resource::<crate::topology::input::PointerState>();
    let width = app.world().get::<CanvasItem>(card).unwrap().size.x;
    assert!(app.world().get::<RecordCard>(card).is_some());
    begin_drag(app.world_mut(), card, destination);
    crate::topology::set_position(app.world_mut(), card, destination);
    app.update();
    assert_eq!(app.world().get::<CanvasItem>(card).unwrap().size.x, width);
    assert_eq!(app.world().get::<Card>(card).unwrap().column, Some(backlog));
    assert_eq!(
        app.world().get::<RecordProperties>(card).unwrap().0["quantity_exact"],
        "0"
    );
    app.world_mut()
        .resource_mut::<crate::topology::input::PointerState>()
        .drag = None;
    until(&mut app, |world| {
        world
            .get::<Card>(card)
            .is_some_and(|c| c.column == Some(todo))
    })
    .await;
    assert!(crate::area_mutation::armed(app.world(), todo));
    let properties = &app.world().get::<RecordProperties>(card).unwrap().0;
    assert_eq!(properties["quantity_exact"], "-1");
    assert_eq!(column_index(properties), Some(1));
    assert!(
        !properties["assertions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["predicate"] == "backlog")
    );
    crate::layout::detach(app.world_mut(), card);
    crate::topology::set_position(app.world_mut(), card, DVec3::new(5000.0, 0.0, 0.0));
    until(&mut app, |world| {
        world
            .get::<RecordProperties>(card)
            .is_some_and(|r| r.0["quantity_exact"] == "0" && column_index(&r.0).is_none())
    })
    .await;
    assert!(app.world().get::<RecordBinding>(card).is_some());
}

#[test]
fn columns_touch_after_resize_and_source_unlock_preserves_its_connection() {
    let (mut app, _, owner) = fixture();
    app.update();
    let board = app.world().get::<Kanban>(owner).unwrap().clone();
    let columns: Vec<_> = board
        .columns
        .iter()
        .map(|c| area(app.world(), owner, &c.area).unwrap())
        .collect();
    let source = area(app.world(), owner, &board.source).unwrap();
    let mut rules = app
        .world()
        .get::<LayoutBox>(columns[1])
        .unwrap()
        .rules
        .clone();
    rules.axes[0].size = 420.0;
    crate::layout::configure(app.world_mut(), columns[1], rules).unwrap();
    app.update();
    for pair in columns.windows(2) {
        let first = app.world().get::<CanvasItem>(pair[0]).unwrap();
        let second = app.world().get::<CanvasItem>(pair[1]).unwrap();
        let edge = crate::topology::position(app.world(), pair[0]).unwrap().x
            + f64::from(first.size.x) * 0.5;
        let next = crate::topology::position(app.world(), pair[1]).unwrap().x
            - f64::from(second.size.x) * 0.5;
        assert!((edge - next).abs() < 1e-8);
        let boundary = DVec2::new(edge, 0.0);
        assert!(
            !app.world()
                .get::<InfluenceArea>(pair[0])
                .unwrap()
                .contains(boundary)
        );
        assert!(
            app.world()
                .get::<InfluenceArea>(pair[1])
                .unwrap()
                .contains(boundary)
        );
    }
    let before = crate::topology::position(app.world(), source).unwrap();
    let translation = DVec3::new(30.0, 0.0, 50.0);
    crate::topology::groups::transform(
        app.world_mut(),
        owner,
        translation,
        bevy::math::DQuat::IDENTITY,
    );
    app.update();
    assert_eq!(
        crate::topology::position(app.world(), source),
        Some(before + translation)
    );
    let source_position = crate::topology::position(app.world(), source).unwrap();
    assert!(source_position.z > 360.0);
    crate::canvas_selection::detach(app.world_mut(), source);
    assert!(
        app.world()
            .get::<crate::canvas_selection::SandGroup>(columns[0])
            .is_some()
    );
    assert!(
        crate::sand_placement::Placement::capture(app.world(), source)
            .group
            .is_none()
    );
    let detached = source_position + DVec3::new(400.0, 0.0, 200.0);
    crate::topology::set_position(app.world_mut(), source, detached);
    app.update();
    assert_eq!(
        crate::topology::position(app.world(), source),
        Some(detached)
    );
    let config = app
        .world()
        .get::<InfluenceArea>(source)
        .unwrap()
        .protein
        .as_ref()
        .unwrap();
    assert_eq!(
        config.spawn_targets,
        board
            .columns
            .iter()
            .map(|c| c.area.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        config.placement,
        crate::protein_area::SpawnPlacement::MatchingAreas
    );
    let original_root = app.world().get::<ChildOf>(owner).unwrap().parent();
    let saved_board = snapshot(app.world_mut(), original_root).pop().unwrap();
    let saved_areas: Vec<_> = board
        .ids()
        .map(|id| {
            let entity = area(app.world(), owner, id).unwrap();
            (
                app.world().get::<InfluenceArea>(entity).unwrap().clone(),
                crate::sand_placement::Placement::capture(app.world(), entity),
            )
        })
        .collect();
    let restored_root = app
        .world_mut()
        .spawn((
            crate::container::BoxRoot,
            crate::workspace::Workspaces::default(),
        ))
        .id();
    let mut restored_source = None;
    for (saved, placement) in saved_areas {
        let is_source = saved.id == board.source;
        let entity = crate::area::spawn_area(app.world_mut(), restored_root, 1, saved).unwrap();
        placement.restore(app.world_mut(), entity);
        if is_source {
            restored_source = Some(entity);
        }
    }
    saved_board.restore(app.world_mut(), restored_root);
    app.update();
    let restored_source = restored_source.unwrap();
    assert_eq!(
        crate::topology::position(app.world(), restored_source),
        Some(detached)
    );
    assert!(
        app.world()
            .get::<crate::canvas_selection::SandGroup>(restored_source)
            .is_none()
    );
}
