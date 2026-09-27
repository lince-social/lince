use super::*;
use crate::{
    canvas_selection::SandSelection,
    edit_mode::{EditAction, EditModePlugin},
    sand_panel::tests::{app, connect as connect_cell, settle},
    sand_store::{SandKind, StoredSand},
    workspace::WorkspacePlugin,
};
use bevy::{math::DVec2, text::EditableText};
use std::sync::Arc;

fn fixture(engine: Arc<engine::Engine>) -> (App, Entity, Entity) {
    let mut app = app();
    crate::laboratory::isolate(app.world_mut());
    connect_cell(&mut app, engine);
    app.add_plugins((WorkspacePlugin, EditModePlugin));
    let root = app.world_mut().spawn(crate::container::BoxRoot).id();
    app.update();
    EditAction::Open.apply(app.world_mut(), root);
    let parent = app.world_mut().spawn(Node::default()).id();
    show(app.world_mut(), root, parent);
    let field = app.world_mut().spawn(EditableText::new("Planning")).id();
    (app, root, field)
}

fn note(world: &mut World, root: Entity, value: &str) -> Entity {
    let note =
        crate::sand_store::spawn_sand(world, root, 1, SandKind::EditableText, value, DVec2::ZERO);
    world.entity_mut(root).insert(SandSelection(vec![note]));
    note
}

async fn saved(app: &mut App, root: Entity, name: &str) {
    settle(app, |world| {
        let library = world.get::<Library>(root).unwrap();
        library.pending.is_none()
            && library.entries.iter().any(|row| row["head"] == name)
            && library
                .castle
                .as_ref()
                .is_some_and(|castle| castle.name == name)
    })
    .await;
}

#[tokio::test]
async fn remote_library_routes_changes_to_organ_and_adds_only_local_canvas_objects() {
    let local = Arc::new(engine::Engine::open_memory().await.unwrap());
    let remote_engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let (mut app, root, field) = fixture(local.clone());
    let (outgoing, mut requests) = tokio::sync::mpsc::channel(32);
    let (responses, incoming) = tokio::sync::mpsc::channel(32);
    let mut session = cell::Session::local(
        remote_engine.clone(),
        Arc::new(cell::LaneHub::new()),
        "remote-component-test",
    );
    let task = tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            for message in session.handle(request).await {
                if responses.send(message).await.is_err() {
                    return;
                }
            }
        }
    });
    let mut library = app.world_mut().get_mut::<Library>(root).unwrap();
    library.organ = Some("remote-organ".into());
    library.remote = Some(crate::protein_area::Remote {
        outgoing,
        incoming,
        task,
    });
    library.subscribed = false;
    library.list_id = nucleus::new_uid("remote-components");
    drop(library);
    settle(&mut app, |world| {
        world.get::<Library>(root).unwrap().subscribed
    })
    .await;
    note(app.world_mut(), root, "Shared board");
    Command::Save(field).apply(app.world_mut(), root);
    saved(&mut app, root, "Planning").await;
    assert!(
        protein::execute(&local.store, &query(None))
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        protein::execute(&remote_engine.store, &query(None))
            .await
            .unwrap()
            .len(),
        1
    );
    let before = app
        .world_mut()
        .query::<&StoredSand>()
        .iter(app.world())
        .count();
    Command::Add.apply(app.world_mut(), root);
    assert_eq!(
        app.world_mut()
            .query::<&StoredSand>()
            .iter(app.world())
            .count(),
        before + 1
    );
    assert!(
        protein::execute(&local.store, &query(None))
            .await
            .unwrap()
            .is_empty()
    );
    let stale = app.world().get::<Library>(root).unwrap().list_id.clone();
    Command::Organ(None).apply(app.world_mut(), root);
    receive(
        app.world_mut(),
        root,
        &ServerMessage::Snapshot {
            id: stale,
            rows: vec![json!({"uid":"stale","head":"Old Organ"})],
        },
    );
    assert!(app.world().get::<Library>(root).unwrap().entries.is_empty());
    assert!(app.world().get::<Library>(root).unwrap().castle.is_none());
}

#[test]
fn imported_components_validate_contents_and_list_queries_omit_payloads() {
    let mut world = World::new();
    world.init_resource::<Assets<Font>>();
    world.init_resource::<crate::theme::Typography>();
    let root = world.spawn(crate::workspace::Workspaces::default()).id();
    note(&mut world, root, "Reusable");
    let castle = CustomCastle::capture(&world, root, "Original").unwrap();
    let body = encode(castle).unwrap();
    let mut row = json!({"kind":"sand", "head":"Renamed", "body":body});
    assert_eq!(decode(&row).unwrap().name, "Renamed");
    row["kind"] = json!("plain");
    assert!(decode(&row).is_err());
    row["kind"] = json!("sand");
    let mut document: Value = serde_json::from_str(&body).unwrap();
    document["castle"]["parts"][0]["size"] = json!([-1, 100]);
    row["body"] = json!(document.to_string());
    assert!(decode(&row).is_err());
    row["body"] = json!("x".repeat(MAX_BYTES + 1));
    assert!(decode(&row).is_err());
    assert_eq!(query(None).fields.unwrap(), ["uid", "head"]);
    protein::validate(&query(None)).unwrap();
}
