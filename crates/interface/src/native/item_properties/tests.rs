use super::*;
use crate::native::{
    area::RecordProperties,
    canvas::CanvasItem,
    cell_bridge::{self, CellMessage},
    workspace::WorkspaceMember,
};
use bevy::math::DVec2;
use serde_json::json;
use std::sync::Arc;

fn runtime(engine: Arc<engine::Engine>) -> cell::CellRuntime {
    cell::CellRuntime {
        store: engine.store.clone(),
        engine,
        commands: Default::default(),
        lanes: Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        information: None,
        speech: None,
        fiote: None,
    }
}

fn scene() -> (App, Entity, Entity) {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, ItemPropertiesPlugin))
        .add_message::<CellMessage>();
    let root = app.world_mut().spawn(Workspaces::default()).id();
    let item = app
        .world_mut()
        .spawn((
            CanvasItem {
                position: DVec2::ZERO,
                size: Vec2::splat(200.0),
            },
            WorkspaceMember(1),
            ChildOf(root),
        ))
        .id();
    (app, root, item)
}

async fn settle(app: &mut App, done: impl Fn(&World) -> bool) {
    for _ in 0..1000 {
        let mut messages = Vec::new();
        if let Some(mut bridge) = app
            .world_mut()
            .get_non_send_mut::<cell_bridge::CellBridge>()
        {
            while let Ok(message) = bridge.incoming.try_recv() {
                messages.push(CellMessage::new(message));
            }
        }
        app.world_mut().write_message_batch(messages);
        app.update();
        if done(app.world()) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    panic!("Record attachment did not settle");
}

async fn record(engine: &engine::Engine, head: &str) -> String {
    engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: head.into(),
                body: String::new(),
                quantity: 3.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap()
}

#[tokio::test]
async fn explicit_attachment_streams_existing_properties_updates_and_detaches_without_writes() {
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let uid = record(&engine, "Existing task").await;
    let runtime = runtime(engine.clone());
    let (mut app, root, item) = scene();
    let baseline = json!({"head":"Canvas item","local":"retained"});
    app.world_mut()
        .entity_mut(item)
        .insert(RecordProperties(baseline.clone()));
    app.world_mut().insert_non_send(cell_bridge::connect(
        runtime.clone(),
        crate::host::WakeSignal::new(|| {}),
    ));
    app.update();
    assert!(app.world().get::<RecordBinding>(item).is_none());
    attach(app.world_mut(), root, item, &uid).unwrap();
    settle(&mut app, |world| {
        world
            .get::<RecordProperties>(item)
            .is_some_and(|properties| properties.0["head"] == "Existing task")
    })
    .await;
    assert_eq!(app.world().get::<RecordBinding>(item).unwrap().uid, uid);
    assert_eq!(
        app.world().get::<RecordProperties>(item).unwrap().0["quantity"],
        "3"
    );
    engine
        .act(
            engine::actions::Action::EditRecordText {
                target: uid.clone(),
                head: Some("Updated task".into()),
                body: None,
            },
            None,
        )
        .await
        .unwrap();
    settle(&mut app, |world| {
        world
            .get::<RecordProperties>(item)
            .is_some_and(|properties| properties.0["head"] == "Updated task")
    })
    .await;
    detach(app.world_mut(), root, item).unwrap();
    assert!(app.world().get::<Attachment>(item).is_none());
    assert!(app.world().get::<RecordBinding>(item).is_none());
    assert_eq!(
        app.world().get::<RecordProperties>(item).unwrap().0,
        baseline
    );
    let mut session = runtime.local_session();
    let messages = session.handle(cell_bridge::records_subscription()).await;
    assert!(messages.iter().any(|message| matches!(message, cell::ServerMessage::Snapshot { rows, .. } if rows.iter().any(|row| row["uid"] == uid && row["head"] == "Updated task"))));
}

#[tokio::test]
async fn closing_items_and_replacing_attachments_never_delete_or_retarget_records() {
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let first = record(&engine, "First").await;
    let second = record(&engine, "Second").await;
    let runtime = runtime(engine);
    let (mut app, root, item) = scene();
    app.world_mut().insert_non_send(cell_bridge::connect(
        runtime.clone(),
        crate::host::WakeSignal::new(|| {}),
    ));
    attach(app.world_mut(), root, item, &first).unwrap();
    settle(&mut app, |world| {
        world
            .get::<RecordBinding>(item)
            .is_some_and(|binding| binding.uid == first)
    })
    .await;
    attach(app.world_mut(), root, item, &second).unwrap();
    settle(&mut app, |world| {
        world
            .get::<RecordBinding>(item)
            .is_some_and(|binding| binding.uid == second)
    })
    .await;
    app.world_mut().despawn(item);
    app.update();
    let mut session = runtime.local_session();
    let messages = session.handle(cell_bridge::records_subscription()).await;
    assert!(messages.iter().any(|message| matches!(message, cell::ServerMessage::Snapshot { rows, .. } if [&first, &second].into_iter().all(|uid| rows.iter().any(|row| row["uid"] == *uid)))));
}

#[test]
fn attachment_persistence_is_metadata_and_rejects_invalid_and_foreign_targets() {
    let (mut app, root, item) = scene();
    let uid = nucleus::new_uid("r");
    assert!(attach(app.world_mut(), root, item, "not-a-record").is_err());
    attach(app.world_mut(), root, item, &uid).unwrap();
    let saved = crate::native::sand_placement::Placement::capture(app.world(), item);
    let document = serde_json::to_value(&saved).unwrap();
    let restored: crate::native::sand_placement::Placement =
        serde_json::from_value(document).unwrap();
    assert!(restored.valid());
    let other = app
        .world_mut()
        .spawn((
            CanvasItem {
                position: DVec2::ZERO,
                size: Vec2::splat(100.0),
            },
            WorkspaceMember(1),
            ChildOf(root),
        ))
        .id();
    restored.restore(app.world_mut(), other);
    assert_eq!(app.world().get::<Attachment>(other).unwrap().uid, uid);
    assert!(app.world().get::<RecordBinding>(other).is_none());
    let foreign = app.world_mut().spawn(Workspaces::default()).id();
    assert!(attach(app.world_mut(), foreign, item, &uid).is_err());
}

#[tokio::test]
async fn deleted_records_and_disconnected_cells_remove_properties_instead_of_leaving_stale_bindings()
 {
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let uid = record(&engine, "Temporary source").await;
    let (mut app, root, item) = scene();
    app.world_mut().insert_non_send(cell_bridge::connect(
        runtime(engine.clone()),
        crate::host::WakeSignal::new(|| {}),
    ));
    attach(app.world_mut(), root, item, &uid).unwrap();
    settle(&mut app, |world| world.get::<RecordBinding>(item).is_some()).await;
    engine
        .act(
            engine::actions::Action::DeleteRecord {
                target: uid.clone(),
            },
            None,
        )
        .await
        .unwrap();
    settle(&mut app, |world| world.get::<RecordBinding>(item).is_none()).await;
    assert!(app.world().get::<RecordProperties>(item).is_none());
    let second = record(&engine, "Other source").await;
    attach(app.world_mut(), root, item, &second).unwrap();
    settle(&mut app, |world| world.get::<RecordBinding>(item).is_some()).await;
    app.world_mut()
        .remove_non_send::<cell_bridge::CellBridge>();
    app.update();
    assert!(app.world().get::<RecordBinding>(item).is_none());
    assert!(app.world().get::<RecordProperties>(item).is_none());
}

#[test]
fn property_controls_attach_without_spawning_a_record_view_and_close_with_the_item() {
    let (mut app, root, item) = scene();
    app.init_resource::<Assets<Font>>()
        .init_resource::<crate::native::theme::Typography>();
    assert!(app.world().get::<Attachment>(item).is_none());
    let inspector = open(app.world_mut(), root, item).unwrap();
    let (page, input) = app
        .world_mut()
        .query::<(Entity, &Page)>()
        .iter(app.world())
        .map(|(entity, page)| (entity, page.input))
        .next()
        .unwrap();
    let uid = nucleus::new_uid("r");
    let mut ancestor = Some(page);
    while let Some(entity) = ancestor {
        if app.world().get::<InheritedVisibility>(entity).is_some() {
            app.world_mut().entity_mut(entity).insert(InheritedVisibility::VISIBLE);
        }
        ancestor = app.world().get::<ChildOf>(entity).map(ChildOf::parent);
    }
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text(&uid);
    Change::Attach.apply(app.world_mut(), page);
    assert_eq!(app.world().get::<Attachment>(item).unwrap().uid, uid);
    assert_eq!(
        app.world_mut()
            .query::<&crate::native::area::InfluenceArea>()
            .iter(app.world())
            .count(),
        0
    );
    Change::Detach.apply(app.world_mut(), page);
    assert!(app.world().get::<Attachment>(item).is_none());
    app.world_mut().despawn(item);
    app.update();
    assert!(app.world().get_entity(inspector).is_err());
}

#[test]
fn locked_or_practice_items_cannot_attach_personal_records() {
    let (mut app, root, item) = scene();
    let uid = nucleus::new_uid("r");
    let activity = crate::host::input::InputActivity::default();
    activity.set_active(false);
    app.world_mut().insert_resource(activity.clone());
    assert!(attach(app.world_mut(), root, item, &uid).is_err());
    activity.set_active(true);
    app.world_mut()
        .entity_mut(item)
        .insert(crate::native::practice_cells::PracticeSource(
            "isolated".into(),
        ));
    assert!(attach(app.world_mut(), root, item, &uid).is_err());
    assert!(app.world().get::<Attachment>(item).is_none());
}
