use super::*;

#[test]
fn standalone_log_crud_validates_and_roundtrips_a_running_stopwatch() {
    let mut local = LocalTimer::default();
    let start = chrono::DateTime::parse_from_rfc3339("2026-09-19T10:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    local.toggle(start).unwrap();
    let restored: LocalTimer =
        serde_json::from_str(&serde_json::to_string(&local).unwrap()).unwrap();
    assert_eq!(restored, local);
    assert_eq!(
        local.logs[0].seconds(start + chrono::Duration::seconds(75)),
        75
    );
    local.toggle(start + chrono::Duration::seconds(75)).unwrap();
    assert_eq!(
        local.logs[0].seconds(start + chrono::Duration::hours(1)),
        75
    );
    let id = local.logs[0].id.clone();
    let mut edited = local.logs[0].clone();
    edited.end = Some((start + chrono::Duration::seconds(90)).to_rfc3339());
    local.change(&id, Some(edited.clone())).unwrap();
    assert_eq!(local.logs[0].seconds(start), 90);
    edited.end = Some((start - chrono::Duration::seconds(1)).to_rfc3339());
    assert!(local.change(&id, Some(edited)).is_err());
    assert_eq!(local.logs[0].seconds(start), 90);
    local.change(&id, None).unwrap();
    assert!(local.logs.is_empty());
}

async fn settle(app: &mut App, done: impl Fn(&World) -> bool) {
    for _ in 0..500 {
        app.update();
        if done(app.world()) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("Time Castle did not settle");
}

#[tokio::test]
async fn record_log_crud_uses_shared_mutations_and_reference_changes_preserve_local_time() {
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    let uid = engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: Some("timed-record".into()),
                kind: nucleus::RecordKind::Plain,
                head: "Timed Record".into(),
                body: String::new(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let mut app = App::new();
    app.init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<bevy::input_focus::InputFocus>()
        .insert_resource(crate::wake::WakeSignal::new(|| {}))
        .insert_resource(crate::app::CellHandle(cell::CellRuntime {
            engine: engine.clone(),
            store: engine.store.clone(),
            lanes: std::sync::Arc::new(cell::LaneHub::new()),
            wire: Default::default(),
            fiote: None,
            information: None,
        }))
        .add_plugins((
            crate::cell_bridge::CellBridgePlugin,
            crate::record_binding::RecordBindingPlugin,
            WorkTimerPlugin,
        ));
    let owner = app.world_mut().spawn(Node::default()).id();
    let input = app.world_mut().spawn(EditableText::new("")).id();
    populate(app.world_mut(), owner, None, &Value::Null, Some(input));
    Toggle.apply(app.world_mut(), owner);
    Toggle.apply(app.world_mut(), owner);
    let local = app.world().get::<LocalTimer>(owner).unwrap().clone();
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("timed-record");
    settle(&mut app, |world| {
        world.get::<WorkTimer>(owner).unwrap().binding.is_some()
    })
    .await;
    assert_eq!(
        app.world()
            .get::<WorkTimer>(owner)
            .unwrap()
            .binding
            .as_ref()
            .unwrap()
            .uid,
        uid
    );
    assert!(app.world().get::<WorkTimer>(owner).unwrap().logs.is_empty());
    let id = "work.log:manual";
    let mut entry = Entry {
        id: id.into(),
        start: "2026-09-19T10:00:00Z".into(),
        end: Some("2026-09-19T10:02:00Z".into()),
    };
    change(app.world_mut(), owner, id, Some(entry.clone()), None).unwrap();
    settle(&mut app, |world| {
        let timer = world.get::<WorkTimer>(owner).unwrap();
        timer.pending.is_none() && timer.logs.len() == 1
    })
    .await;
    entry.end = Some("2026-09-19T10:03:00Z".into());
    change(app.world_mut(), owner, id, Some(entry.clone()), None).unwrap();
    settle(&mut app, |world| {
        let timer = world.get::<WorkTimer>(owner).unwrap();
        timer.pending.is_none() && timer.logs == [entry.clone()]
    })
    .await;
    change(app.world_mut(), owner, id, None, None).unwrap();
    settle(&mut app, |world| {
        let timer = world.get::<WorkTimer>(owner).unwrap();
        timer.pending.is_none() && timer.logs.is_empty()
    })
    .await;
    Toggle.apply(app.world_mut(), owner);
    settle(&mut app, |world| {
        let timer = world.get::<WorkTimer>(owner).unwrap();
        timer.pending.is_none() && timer.logs.iter().any(|log| log.end.is_none())
    })
    .await;
    Toggle.apply(app.world_mut(), owner);
    settle(&mut app, |world| {
        let timer = world.get::<WorkTimer>(owner).unwrap();
        timer.pending.is_none() && timer.logs.iter().all(|log| log.end.is_some())
    })
    .await;
    let old_subscription = app
        .world()
        .get::<WorkTimer>(owner)
        .unwrap()
        .subscription
        .clone();
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("");
    assert!(change(app.world_mut(), owner, id, Some(entry), None).is_err());
    app.update();
    receive(
        app.world_mut(),
        &ServerMessage::Snapshot {
            id: old_subscription,
            rows: vec![json!({"uid":uid, "work_logs":[]})],
        },
    );
    let timer = app.world().get::<WorkTimer>(owner).unwrap();
    assert!(timer.binding.is_none());
    assert_eq!(timer.logs, local.logs);
    assert_eq!(app.world().get::<LocalTimer>(owner).unwrap(), &local);
}

#[test]
fn standalone_rejects_multiple_running_entries_and_clamps_clock_reversal() {
    let now = chrono::Utc::now();
    let mut local = LocalTimer::default();
    local.toggle(now).unwrap();
    let before = local.clone();
    let entry = Entry {
        id: "work.log:second".into(),
        start: now.to_rfc3339(),
        end: None,
    };
    assert!(local.change(&entry.id.clone(), Some(entry)).is_err());
    assert_eq!(local, before);
    local.toggle(now - chrono::Duration::hours(1)).unwrap();
    assert_eq!(local.logs[0].seconds(now), 0);
    assert!(local.valid());
}
