// non-lean-yet
// Scope: const PAGE_SIZE, const ORIGINAL, const DRAFT, fn area_app.
// BeautifulRust: No site flagged by this pass. Check contracts and feature coverage.

use super::*;
use crate::native::{
    app::CellHandle,
    cell_bridge::CellBridgePlugin,
    record_binding::{self, RecordBindingPlugin, TextBinding},
    wake::WakeSignal,
};
use bevy::{
    input_focus::{FocusCause, InputFocus},
    text::EditableText,
};
use engine::{Engine, actions::Action};
use std::{sync::Arc, time::Duration};

const PAGE_SIZE: usize = 200;
const ORIGINAL: &str = "Original body";
const DRAFT: &str = "Dirty virtualized café 👩‍💻";

fn area_app(engine: Arc<Engine>, config: Config) -> (App, Entity) {
    let (mut app, _, owner) = fixture();
    app.insert_resource(CellHandle(cell::CellRuntime {
        commands: Default::default(),
        store: engine.store.clone(),
        engine,
        lanes: Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        fiote: None,
        speech: None,
        information: None,
    }))
    .insert_resource(WakeSignal::new(|| {}))
    .add_plugins((CellBridgePlugin, RecordBindingPlugin));
    set_configuration(app.world_mut(), owner, Some(config));
    (app, owner)
}

fn body_editor(world: &World, owner: Entity, uid: &str) -> Option<Entity> {
    let row = *world
        .resource::<Runtime>()
        .areas
        .get(&owner)?
        .row_entities
        .get(uid)?;
    let mut nodes = vec![row];
    while let Some(entity) = nodes.pop() {
        if let Some(binding) = world.get::<TextBinding>(entity)
            && binding.property == "body"
        {
            assert_eq!(binding.record.area, owner);
            assert_eq!(binding.record.uid, uid);
            assert_eq!(binding.record.source, Source::Local);
            return Some(entity);
        }
        if let Some(children) = world.get::<Children>(entity) {
            nodes.extend(children.iter());
        }
    }
    None
}

async fn focus_body(app: &mut App, owner: Entity, uid: &str) -> Entity {
    until(app, |world| body_editor(world, owner, uid).is_some()).await;
    let editor = body_editor(app.world(), owner, uid).unwrap();
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(editor, FocusCause::Pressed);
    until(app, |world| record_binding::can_edit(world, editor)).await;
    editor
}

fn body(world: &World, editor: Entity) -> String {
    world
        .get::<EditableText>(editor)
        .unwrap()
        .value()
        .to_string()
}

fn assert_draft(recovered: &engine::record_edit::Recovered, uid: &str) {
    let doc = loro::LoroDoc::new();
    doc.import(&recovered.snapshot).unwrap();
    assert_eq!(doc.get_text("body").to_string(), DRAFT);
    assert_eq!(recovered.pending.len(), 1);
    assert_eq!(recovered.pending[0].record_uid, uid);
}

#[tokio::test]
async fn dirty_body_survives_virtualized_protein_sort_filter_and_cold_recovery() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        directory.path().join("virtualized.sqlite").display()
    );
    let engine = Arc::new(Engine::open(&url).await.unwrap());
    let mut ids = Vec::with_capacity(PAGE_SIZE + 1);
    for index in 1..=PAGE_SIZE + 1 {
        ids.push(
            engine
                .act(
                    Action::CreateRecord {
                        slug: None,
                        kind: nucleus::RecordKind::Plain,
                        head: format!("Virtualized fixture {index}"),
                        body: ORIGINAL.into(),
                        quantity: index as f64,
                    },
                    None,
                )
                .await
                .unwrap()
                .created
                .unwrap(),
        );
    }
    let [uid, neighbor, ..] = ids.as_slice() else {
        unreachable!()
    };
    let mut config = Config::default();
    config.bindings[1].editable = true;
    config.draft.query["limit"] = json!(PAGE_SIZE + 1);
    config.draft.query["where"] = json!([{"text_contains":"Virtualized fixture"}]);
    config.draft.query["order"] = json!([{"asc":"quantity"}]);
    let (mut app, owner) = area_app(engine.clone(), config.clone());
    let editor = focus_body(&mut app, owner, uid).await;
    let state = &app.world().resource::<Runtime>().areas[&owner];
    assert_eq!(state.data.len(), PAGE_SIZE + 1);
    assert_eq!(state.row_entities.len(), PAGE_SIZE);
    assert_eq!(state.order.first(), Some(uid));
    assert_eq!(body(app.world(), editor), ORIGINAL);

    let mut held = record_binding::hold_canonical_requests(app.world_mut(), editor);
    app.world_mut()
        .get_mut::<EditableText>(editor)
        .unwrap()
        .editor
        .set_text(DRAFT);
    let storage = serde_json::to_string(&Source::Local).unwrap();
    let pending = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            app.update();
            if let Some(recovered) = engine
                .load_record_edit_draft(&storage, uid)
                .await
                .unwrap()
                .recovered
            {
                assert_draft(&recovered, uid);
                break recovered.pending[0].id.clone();
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("The private draft did not reach the journal");
    assert_eq!(engine.doc_text(uid).await.unwrap().1, ORIGINAL);

    let mut descending = config.clone();
    descending.draft.query["order"] = json!([{"desc":"quantity"}]);
    set_configuration(app.world_mut(), owner, Some(descending.clone()));
    until(&mut app, |world| {
        let Some(state) = world.resource::<Runtime>().areas.get(&owner) else {
            return false;
        };
        state.ready && state.order.last() == Some(uid) && !state.row_entities.contains_key(uid)
    })
    .await;
    assert!(app.world().get_entity(editor).is_err());
    let replacement = body_editor(app.world(), owner, ids.last().unwrap()).unwrap();
    assert_eq!(body(app.world(), replacement), ORIGINAL);
    assert_eq!(
        app.world().resource::<Runtime>().areas[&owner]
            .row_entities
            .len(),
        PAGE_SIZE
    );

    let mut excluded = descending;
    excluded.draft.query["where"] = json!([
        {"text_contains":"Virtualized fixture"},
        {"quantity_gte":"2"}
    ]);
    set_configuration(app.world_mut(), owner, Some(excluded.clone()));
    until(&mut app, |world| {
        let Some(state) = world.resource::<Runtime>().areas.get(&owner) else {
            return false;
        };
        state.ready && state.data.len() == PAGE_SIZE && !state.order.contains(uid)
    })
    .await;
    assert!(body_editor(app.world(), owner, uid).is_none());
    assert_eq!(
        body(
            app.world(),
            body_editor(app.world(), owner, neighbor).unwrap()
        ),
        ORIGINAL
    );

    set_configuration(app.world_mut(), owner, Some(config.clone()));
    let returned = focus_body(&mut app, owner, uid).await;
    assert_ne!(returned, editor);
    let world = app.world();
    let editor_text = world.get::<EditableText>(returned).unwrap();
    assert_eq!(editor_text.value().to_string(), DRAFT);
    assert!(
        world
            .get::<TextBinding>(returned)
            .unwrap()
            .editor_unsaved(editor_text)
    );
    let recovered = engine
        .load_record_edit_draft(&storage, uid)
        .await
        .unwrap()
        .recovered
        .unwrap();
    assert_draft(&recovered, uid);
    assert_eq!(recovered.pending[0].id, pending);
    assert!(
        matches!(held.try_recv().unwrap(), ClientMessage::CollabLeave { record_uid } if record_uid == *uid)
    );
    assert!(held.try_recv().is_err());
    assert_eq!(engine.doc_text(uid).await.unwrap().1, ORIGINAL);
    assert_eq!(engine.doc_text(neighbor).await.unwrap().1, ORIGINAL);
    drop(app);
    drop(engine);

    let reopened = Arc::new(Engine::open(&url).await.unwrap());
    let (mut app, owner) = area_app(reopened.clone(), excluded);
    until(&mut app, |world| {
        body_editor(world, owner, neighbor).is_some()
    })
    .await;
    assert!(body_editor(app.world(), owner, uid).is_none());
    set_configuration(app.world_mut(), owner, Some(config));
    let restored = focus_body(&mut app, owner, uid).await;
    until(&mut app, |world| {
        let text = world.get::<EditableText>(restored).unwrap();
        text.value() == DRAFT
            && !world
                .get::<TextBinding>(restored)
                .unwrap()
                .editor_unsaved(text)
    })
    .await;
    assert_eq!(reopened.doc_text(uid).await.unwrap().1, DRAFT);
    assert_eq!(reopened.doc_text(neighbor).await.unwrap().1, ORIGINAL);
    let newest = ids.last().unwrap();
    assert_eq!(reopened.doc_text(newest).await.unwrap().1, ORIGINAL);
}
