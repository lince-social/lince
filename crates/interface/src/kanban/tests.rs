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
    assert_eq!(areas.len(), 15);
    for area in &areas {
        assert!(area.validate(), "{}", area.name);
        if let Some(config) = area.protein.as_ref().or(area.filter.as_ref()) {
            config.query().unwrap();
        }
    }
    assert_eq!(config().bindings.iter().filter(|b| b.editable).count(), 6);
    app.update();
    assert_eq!(
        app.world_mut().query::<&Part>().iter(app.world()).count(),
        14
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
    Command::Start.apply(app.world_mut(), owner);
    let board = app.world().get::<Kanban>(owner).unwrap().clone();
    let backlog = area(app.world(), owner, &board.columns[0].area).unwrap();
    let todo = area(app.world(), owner, &board.columns[1].area).unwrap();
    until(&mut app, |world| {
        crate::area_mutation::armed(world, backlog) && crate::area_mutation::armed(world, todo)
    })
    .await;
    let editor = app
        .world_mut()
        .query::<(Entity, &EditableText, &ChildOf)>()
        .iter(app.world())
        .find(|(_, _, parent)| {
            app.world()
                .get::<ChildOf>(parent.parent())
                .and_then(|parent| app.world().get::<Part>(parent.parent()))
                .is_some_and(|part| part.owner == owner && part.column == 0 && !part.count)
        })
        .unwrap()
        .0;
    app.world_mut()
        .get_mut::<EditableText>(editor)
        .unwrap()
        .editor
        .set_text("Task card");
    Command::Add(0, editor).apply(app.world_mut(), owner);
    let source = area(app.world(), owner, &board.source).unwrap();
    until(&mut app, |world| {
        crate::protein_area::calendar_feed(world, source)
            .is_some_and(|(rows, _)| rows.iter().any(|r| r["head"] == "Task card"))
    })
    .await;
    let uid = crate::protein_area::calendar_feed(app.world(), source)
        .unwrap()
        .0
        .iter()
        .find(|r| r["head"] == "Task card")
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
    assert!(
        app.world()
            .get::<EditableText>(editor)
            .unwrap()
            .value()
            .to_string()
            .is_empty()
    );
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
    Command::Move(1).apply(app.world_mut(), card);
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
