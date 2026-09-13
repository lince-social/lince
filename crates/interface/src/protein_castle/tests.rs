use super::*;
use crate::actions::Action as InterfaceAction;
use serde_json::json;

fn fixture() -> (App, Entity, Entity) {
    let mut app = App::new();
    crate::laboratory::isolate(app.world_mut());
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<bevy::input_focus::InputFocus>()
        .add_plugins(ProteinCastlePlugin);
    let root = app
        .world_mut()
        .spawn((
            crate::container::BoxRoot,
            crate::workspace::Workspaces::default(),
        ))
        .id();
    let castle = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        ProteinDraft::default(),
    );
    (app, root, castle)
}

async fn connected() -> (App, Entity, Entity, std::sync::Arc<engine::Engine>) {
    let (mut app, root, castle) = fixture();
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    let runtime = cell::CellRuntime {
        store: engine.store.clone(),
        engine: engine.clone(),
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        information: None,
    };
    app.insert_resource(crate::app::CellHandle(runtime))
        .insert_resource(crate::wake::WakeSignal::new(|| {}))
        .add_plugins(crate::cell_bridge::CellBridgePlugin);
    app.update();
    (app, root, castle, engine)
}

async fn until(app: &mut App, predicate: impl Fn(&World) -> bool) {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            app.update();
            if predicate(app.world()) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

#[cfg_attr(test, test)]
fn nested_filters_keep_exact_quantities_and_reject_incomplete_drafts() {
    let mut draft = ProteinDraft::default();
    draft.query["where"] = json!([{"all":[{"any":[{"quantity_eq":"9007199254740993.125"},{"not":{"relation":{"kind":"assigned-to","direction":"out","other":"alice"}}}]},{"work_date":{"field":"due","op":"gte","value":"2026-09-13"}}]}]);
    let query = draft.compile().unwrap();
    let restored = ProteinDraft::from_protein("Deadline".into(), "deadline".into(), query);
    assert_eq!(restored.query["where"], draft.query["where"]);
    draft.query["limit"] = json!("twelve");
    assert!(draft.compile().is_err());
    assert!(draft.valid_storage());
    draft.query["limit"] = json!("");
    assert_eq!(draft.compile().unwrap().limit, None);
    draft.query["limit"] = json!("0");
    assert!(draft.compile().is_err());
    draft.query["limit"] = Value::Null;
    draft.query["where"] = json!([{"all":[{"text_contains":""}]}]);
    assert!(draft.compile().is_err());
    draft.query["where"] = json!([{"all":[{"any":[]}]}]);
    assert!(draft.compile().is_err());
}

#[cfg_attr(test, test)]
fn editor_controls_restore_drafts_and_keep_live_results_separate() {
    let (mut app, root, castle) = fixture();
    ui::Command::Append("/where/0/all".into(), model::condition("relation"))
        .apply(app.world_mut(), castle);
    ui::Command::Negate("/where/0/all/0".into()).apply(app.world_mut(), castle);
    ui::Command::Set("/where/0/all/0/not/relation/other".into(), json!("alice"))
        .apply(app.world_mut(), castle);
    ui::Command::Append("/order".into(), json!({"asc":"due_date"})).apply(app.world_mut(), castle);
    ui::Command::Append("/order".into(), json!({"desc":"quantity"})).apply(app.world_mut(), castle);
    ui::Command::Move("/order".into(), 1, false).apply(app.world_mut(), castle);
    let draft = app
        .world()
        .get::<ProteinCastle>(castle)
        .unwrap()
        .draft
        .clone();
    assert_eq!(draft.query["order"][0], json!({"desc":"quantity"}));
    assert!(draft.compile().is_ok());
    ui::Command::Set("/limit".into(), json!("unfinished")).apply(app.world_mut(), castle);
    let saved = snapshot(app.world_mut(), root);
    assert_eq!(saved.len(), 1);
    assert!(saved[0].valid());
    let encoded = serde_json::to_vec(&saved).unwrap();
    let saved: Vec<SavedProteinCastle> = serde_json::from_slice(&encoded).unwrap();
    let restored = saved
        .into_iter()
        .next()
        .unwrap()
        .restore(app.world_mut(), root);
    assert_eq!(
        app.world()
            .get::<ProteinCastle>(restored)
            .unwrap()
            .draft
            .query["limit"],
        "unfinished"
    );
    assert!(
        app.world()
            .get::<ProteinResults>(restored)
            .unwrap()
            .rows
            .is_empty()
    );
    assert!(app.world().resource::<Requests>().outgoing.is_empty());
    ui::Command::Source("promise".into()).apply(app.world_mut(), restored);
    assert!(
        app.world()
            .get::<ProteinCastle>(restored)
            .unwrap()
            .draft
            .compile()
            .is_ok()
    );
    app.update();
}

#[cfg_attr(test, tokio::test)]
async fn castle_filters_sorts_saves_loads_and_tracks_backend_changes() {
    let (mut app, _, castle, engine) = connected().await;
    let mut ids = Vec::new();
    for (head, quantity) in [("Apple", 1.0), ("Apricot", 3.0), ("Kiwi", 2.0)] {
        ids.push(
            engine
                .act(
                    engine::actions::Action::CreateRecord {
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
    let query = json!({"source":"record","where":[{"all":[{"kind_eq":"plain"},{"text_contains":"Ap"},{"quantity_gte":"1"}]}],"order":[{"desc":"quantity"}],"fields":["uid","head","quantity"],"limit":100});
    app.world_mut()
        .get_mut::<ProteinCastle>(castle)
        .unwrap()
        .draft = ProteinDraft::from_protein(
        "Fruit".into(),
        "fruit-query".into(),
        serde_json::from_value(query).unwrap(),
    );
    run(app.world_mut(), castle);
    until(&mut app, |world| {
        world.get::<ProteinResults>(castle).unwrap().current
    })
    .await;
    let result = app.world().get::<ProteinResults>(castle).unwrap();
    assert_eq!(result.rows.len(), 2);
    assert_eq!(result.rows[0]["head"], "Apricot");
    assert_eq!(result.rows[1]["head"], "Apple");
    assert_eq!(result.columns, vec!["head", "kind", "quantity", "uid"]);
    save(app.world_mut(), castle);
    until(&mut app, |world| !world.get::<View>(castle).unwrap().saving).await;
    assert_eq!(app.world().get::<View>(castle).unwrap().status, "Saved");
    library(app.world_mut(), castle);
    until(&mut app, |world| {
        !world.get::<View>(castle).unwrap().library.is_empty()
    })
    .await;
    ui::Command::Load(0).apply(app.world_mut(), castle);
    assert_eq!(
        app.world().get::<ProteinCastle>(castle).unwrap().draft.slug,
        "fruit-query"
    );
    let revision = app.world().get::<ProteinResults>(castle).unwrap().revision;
    engine
        .act(
            engine::actions::Action::SetQuantityExact {
                target: ids[0].clone(),
                amount: "5".into(),
            },
            None,
        )
        .await
        .unwrap();
    until(&mut app, |world| {
        world.get::<ProteinResults>(castle).unwrap().revision > revision
    })
    .await;
    assert_eq!(
        app.world().get::<ProteinResults>(castle).unwrap().rows[0]["head"],
        "Apple"
    );
    ui::Command::Set("/order".into(), json!([])).apply(app.world_mut(), castle);
    ui::Command::Set("/aggregate".into(), json!({"op":"sum","by":"total"}))
        .apply(app.world_mut(), castle);
    run(app.world_mut(), castle);
    until(&mut app, |world| {
        world.get::<ProteinResults>(castle).unwrap().current
    })
    .await;
    let result = app.world().get::<ProteinResults>(castle).unwrap();
    assert_eq!(result.rows[0]["value"], "8");
    assert_eq!(result.rows[0]["count"], 2);
    ui::Command::Delete.apply(app.world_mut(), castle);
    app.update();
    assert!(app.world().resource::<Requests>().owners.is_empty());
}

#[cfg_attr(test, test)]
fn late_replies_errors_and_large_results_do_not_replace_drafts_or_grow_the_view() {
    let (mut app, _, castle) = fixture();
    let id = request(app.world_mut(), castle, RequestKind::Results, |id| {
        ClientMessage::Subscribe {
            id,
            protein: ProteinDraft::default().compile().unwrap(),
        }
    });
    cancel(app.world_mut(), castle, Some(RequestKind::Results));
    receive_message(
        app.world_mut(),
        ServerMessage::Snapshot {
            id,
            rows: vec![json!({"head":"stale"})],
        },
    );
    assert!(
        app.world()
            .get::<ProteinResults>(castle)
            .unwrap()
            .rows
            .is_empty()
    );
    let id = request(app.world_mut(), castle, RequestKind::Results, |id| {
        ClientMessage::Subscribe {
            id,
            protein: ProteinDraft::default().compile().unwrap(),
        }
    });
    receive_message(
        app.world_mut(),
        ServerMessage::Snapshot {
            id: id.clone(),
            rows: (0..1000)
                .map(|i| json!({"head":format!("Row {i}")}))
                .collect(),
        },
    );
    ui::output(app.world_mut(), castle);
    let count = app
        .world_mut()
        .query_filtered::<Entity, With<Node>>()
        .iter(app.world())
        .count();
    receive_message(
        app.world_mut(),
        ServerMessage::Update {
            id: id.clone(),
            rows: (0..10_000)
                .map(|i| json!({"head":format!("Row {i}")}))
                .collect(),
        },
    );
    ui::output(app.world_mut(), castle);
    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<Node>>()
            .iter(app.world())
            .count(),
        count
    );
    let draft = app
        .world()
        .get::<ProteinCastle>(castle)
        .unwrap()
        .draft
        .clone();
    receive_message(
        app.world_mut(),
        ServerMessage::Error {
            id,
            message: "Denied".into(),
            code: None,
        },
    );
    assert_eq!(
        app.world().get::<ProteinCastle>(castle).unwrap().draft,
        draft
    );
    assert_eq!(
        app.world()
            .get::<ProteinResults>(castle)
            .unwrap()
            .rows
            .len(),
        10_000
    );
    assert_eq!(
        app.world()
            .get::<ProteinResults>(castle)
            .unwrap()
            .error
            .as_deref(),
        Some("Denied")
    );
    assert!(!app.world().get::<ProteinResults>(castle).unwrap().current);
    disconnected(app.world_mut());
    assert!(app.world().resource::<Requests>().owners.is_empty());
}

crate::laboratory_cases! {
    nested_filters_keep_exact_quantities_and_reject_incomplete_drafts,
    editor_controls_restore_drafts_and_keep_live_results_separate,
    async castle_filters_sorts_saves_loads_and_tracks_backend_changes,
    late_replies_errors_and_large_results_do_not_replace_drafts_or_grow_the_view,
}
