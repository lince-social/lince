use super::*;
use crate::sand_panel::tests::{app, connect, settle};

#[tokio::test]
async fn tasks_create_complete_undo_and_preserve_later_typing() {
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    let mut app = app();
    connect(&mut app, engine);
    app.add_plugins(TodoPlugin);
    let owner = app.world_mut().spawn(Node::default()).id();
    populate(app.world_mut(), owner, owner);
    settle(&mut app, |world| {
        world.get::<TodoSand>(owner).unwrap().ready
    })
    .await;
    let input = app.world().get::<TodoSand>(owner).unwrap().input;
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("First task");
    Command::Add.apply(app.world_mut(), owner);
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("Next draft");
    settle(&mut app, |world| {
        let view = world.get::<TodoSand>(owner).unwrap();
        view.pending.is_none() && view.rows.iter().any(|row| row["head"] == "First task")
    })
    .await;
    assert_eq!(panel::value(app.world(), input).unwrap(), "Next draft");
    let uid = app
        .world()
        .get::<TodoSand>(owner)
        .unwrap()
        .rows
        .iter()
        .find(|row| row["head"] == "First task")
        .unwrap()["uid"]
        .as_str()
        .unwrap()
        .to_owned();
    Command::Complete(uid.clone()).apply(app.world_mut(), owner);
    settle(&mut app, |world| {
        let view = world.get::<TodoSand>(owner).unwrap();
        view.pending.is_none()
            && view.undo.len() == 1
            && !view.rows.iter().any(|row| row["uid"] == uid)
    })
    .await;
    Command::Undo.apply(app.world_mut(), owner);
    settle(&mut app, |world| {
        let view = world.get::<TodoSand>(owner).unwrap();
        view.pending.is_none()
            && view.undo.is_empty()
            && view.rows.iter().any(|row| row["uid"] == uid)
    })
    .await;
    let saved = snapshot(app.world(), owner).unwrap();
    assert!(saved.valid());
    assert_eq!(saved.draft, "Next draft");
    app.world_mut().despawn(owner);
    settle(&mut app, |world| {
        world.resource::<Subscriptions>().0.is_empty()
    })
    .await;
}

#[test]
fn long_queues_have_bounded_rows_and_reject_stale_updates() {
    let mut app = app();
    let owner = app.world_mut().spawn(Node::default()).id();
    populate(app.world_mut(), owner, owner);
    let id = app
        .world()
        .get::<TodoSand>(owner)
        .unwrap()
        .subscription
        .clone();
    let rows: Vec<_> = (0..1000).map(|index| serde_json::json!({"uid":nucleus::new_uid("r"),"head":format!("Task {index}"),"quantity":"-9007199254740993"})).collect();
    receive(
        app.world_mut(),
        owner,
        &ServerMessage::Snapshot {
            id,
            rows: rows.clone(),
        },
    );
    render(app.world_mut(), owner);
    let count = app.world().entities().count_spawned();
    assert!(count < 700);
    assert_eq!(quantity(&rows[0]).unwrap(), "-9007199254740993");
    Command::Move(40).apply(app.world_mut(), owner);
    render(app.world_mut(), owner);
    assert_eq!(app.world().entities().count_spawned(), count);
    receive(
        app.world_mut(),
        owner,
        &ServerMessage::Update {
            id: "stale".into(),
            rows: Vec::new(),
        },
    );
    assert_eq!(app.world().get::<TodoSand>(owner).unwrap().rows.len(), 1000);
    assert!(quantity(&serde_json::json!({"quantity":"NaN"})).is_err());
}

#[test]
fn drafts_preferences_and_query_restore_without_starting_a_subscription() {
    let mut app = app();
    let first = app.world_mut().spawn(Node::default()).id();
    populate(app.world_mut(), first, first);
    let saved = SavedTodo {
        protein: "my-tasks".into(),
        show_ids: true,
        draft: "Draft task".into(),
    };
    restore(
        app.world_mut(),
        first,
        serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
    );
    let view = app.world().get::<TodoSand>(first).unwrap();
    assert!(view.show_ids);
    assert_eq!(view.protein, "my-tasks");
    assert!(!view.requested);
    assert_eq!(panel::value(app.world(), view.input).unwrap(), "Draft task");
}
