use super::*;
use crate::native::full_record::window::{Creation, CreationState, WindowRecord};
use bevy::text::EditableText;

fn pending() -> (App, Entity, Entity, String) {
    let (mut app, root, owner) = super::super::tests::fixture();
    let uid = nucleus::new_uid("r");
    let mut config = crate::native::full_record::config(&uid, Source::Local);
    config.window_record = Some(WindowRecord {
        placement: [3; 16],
        uid: uid.clone(),
        title: "Captured application".into(),
        app_id: "test.application".into(),
        creating: true,
    });
    app.world_mut()
        .get_mut::<InfluenceArea>(owner)
        .unwrap()
        .protein = Some(config.clone());
    app.world_mut()
        .entity_mut(owner)
        .insert(Creation::new(CreationState::Pending));
    start(app.world_mut(), owner, config);
    rows::reconcile(app.world_mut(), owner);
    (app, root, owner, uid)
}

#[test]
fn pending_creation_has_visible_status_and_metadata_but_no_query_or_editors() {
    let (mut app, _, owner, uid) = pending();
    let state = &app.world().resource::<Runtime>().areas[&owner];
    assert!(state.subscription.is_none());
    assert!(state.pending.is_empty());
    assert!(!state.ready);
    assert!(state.data.is_empty());
    let row = state.row_entities[&uid];
    assert!(app.world().get::<rows::Loading>(row).is_some());
    assert!(app.world().get::<RecordBinding>(row).is_none());
    assert_eq!(app.world().get::<WindowRecord>(row).unwrap().uid, uid);
    assert!(
        app.world()
            .get::<crate::native::area::RecordProperties>(row)
            .is_none()
    );
    let panel = app.world().get::<RowPanel>(row).unwrap().0;
    let marker = app.world().get::<Panel>(panel).unwrap();
    assert_eq!(
        app.world().get::<Text>(marker.label).unwrap().0,
        "Creating window Record…"
    );
    assert_eq!(
        app.world().get::<Node>(panel).unwrap().display,
        Display::Flex
    );
    assert_eq!(
        app.world().get::<Node>(marker.retry).unwrap().display,
        Display::None
    );
    assert!(
        app.world_mut()
            .query::<&EditableText>()
            .iter(app.world())
            .next()
            .is_none()
    );
    assert!(
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0.contains("Captured application"))
    );
}

#[test]
fn creation_retry_uses_the_normal_control_once_without_subscribing_or_retargeting() {
    let (mut app, _, owner, uid) = pending();
    app.world_mut()
        .get_mut::<Creation>(owner)
        .unwrap()
        .set_state(CreationState::Failed(
            "Could not save the window profile".into(),
        ));
    rows::reconcile(app.world_mut(), owner);
    let row = app.world().resource::<Runtime>().areas[&owner].row_entities[&uid];
    let marker = app
        .world()
        .get::<Panel>(app.world().get::<RowPanel>(row).unwrap().0)
        .unwrap();
    let retry = marker.retry;
    assert_eq!(
        app.world().get::<Text>(marker.retry_label).unwrap().0,
        "Retry Record creation"
    );
    assert_eq!(
        app.world().get::<Node>(retry).unwrap().display,
        Display::Flex
    );
    assert!(app.world().get::<bevy::ui_widgets::Button>(retry).is_some());
    let action = app.world().get::<ActionButton>(retry).unwrap().clone();
    for _ in 0..8 {
        action.actions.run(app.world_mut(), action.target);
    }
    assert!(
        app.world()
            .get::<Creation>(owner)
            .unwrap()
            .retry_requested()
    );
    assert!(
        app.world_mut()
            .get_mut::<Creation>(owner)
            .unwrap()
            .take_retry()
    );
    assert!(
        !app.world_mut()
            .get_mut::<Creation>(owner)
            .unwrap()
            .take_retry()
    );
    let state = &app.world().resource::<Runtime>().areas[&owner];
    assert!(state.pending.is_empty());
    assert!(state.subscription.is_none());
    assert_eq!(state.row_entities[&uid], row);
    assert!(app.world().get::<RecordBinding>(row).is_none());
    assert_eq!(app.world().get::<WindowRecord>(row).unwrap().uid, uid);
    rows::reconcile(app.world_mut(), owner);
    assert_eq!(
        app.world().resource::<Runtime>().areas[&owner].row_entities[&uid],
        row
    );
    let error = execute_action(
        app.world_mut(),
        &RecordBinding {
            area: owner,
            uid: uid.clone(),
            source: Source::Local,
        },
        row,
        engine::actions::Action::EditRecordText {
            target: uid,
            head: Some("Forbidden premature edit".into()),
            body: None,
        },
    );
    assert!(error.is_err());
}

#[test]
fn captured_pending_creation_survives_serialization_and_cannot_query_after_runtime_state_is_lost() {
    let (mut app, root, owner, uid) = pending();
    let snapshot = crate::native::canvas_host::capture(app.world_mut(), root).unwrap();
    let snapshot: nucleus::canvas::Snapshot =
        serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
    let identity = &app
        .world()
        .get::<crate::native::canvas_host::Identity>(owner)
        .unwrap()
        .0;
    let placement = snapshot
        .placements
        .iter()
        .find(|placement| &placement.id == identity)
        .unwrap();
    let nucleus::canvas::Component::Native {
        kind,
        settings,
        bindings,
    } = &placement.component
    else {
        panic!("The pending inspector lost its Area")
    };
    assert_eq!(kind, "area");
    assert!(bindings.contains(&uid));
    let value: Value = serde_json::from_str(settings["configuration"].as_str().unwrap()).unwrap();
    let config: Config = serde_json::from_value(value["protein"].clone()).unwrap();
    assert!(config.creating());
    assert_eq!(config.window_record.as_ref().unwrap().uid, uid);
    app.world_mut()
        .entity_mut(owner)
        .remove::<Creation>()
        .remove::<crate::native::full_record::window::Metadata>();
    start(app.world_mut(), owner, config);
    rows::reconcile(app.world_mut(), owner);
    let state = &app.world().resource::<Runtime>().areas[&owner];
    assert!(state.subscription.is_none());
    assert!(state.pending.is_empty());
    let row = state.row_entities[&uid];
    assert!(app.world().get::<RecordBinding>(row).is_none());
    assert_eq!(
        crate::native::canvas_host::authoring::owner(app.world(), row),
        owner
    );
    let marker = app
        .world()
        .get::<Panel>(app.world().get::<RowPanel>(row).unwrap().0)
        .unwrap();
    assert!(
        app.world()
            .get::<Text>(marker.label)
            .unwrap()
            .0
            .contains("original Park session")
    );
    assert_eq!(
        app.world().get::<Node>(marker.retry).unwrap().display,
        Display::None
    );
    assert!(
        app.world()
            .get::<crate::native::full_record::window::Metadata>(owner)
            .is_some()
    );
}

#[test]
fn a_window_creation_dependency_requires_its_exact_local_single_record_query() {
    let (app, _, owner, _) = pending();
    let config = app
        .world()
        .get::<InfluenceArea>(owner)
        .unwrap()
        .protein
        .clone()
        .unwrap();
    assert!(config.valid());
    for mutation in 0..4 {
        let mut invalid = config.clone();
        match mutation {
            0 => invalid.window_record.as_mut().unwrap().uid = nucleus::new_uid("r"),
            1 => invalid.source = Source::Organ("remote".into()),
            2 => invalid.draft.query["limit"] = serde_json::json!(2),
            _ => invalid.window_record.as_mut().unwrap().placement = [0; 16],
        }
        assert!(!invalid.valid());
        assert!(invalid.query().is_err());
    }
}

#[tokio::test]
async fn confirmation_populates_the_same_pending_card_only_after_an_actual_cell_read() {
    let (mut app, _, owner, uid) = pending();
    let row = app.world().resource::<Runtime>().areas[&owner].row_entities[&uid];
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    engine
        .act(
            engine::actions::Action::CreateRecordDraft {
                draft: engine::record_creation::Draft {
                    uid: uid.clone(),
                    head: "Confirmed application Record".into(),
                    ..default()
                },
            },
            None,
        )
        .await
        .unwrap();
    let provider = cell::CellRuntime {
        store: engine.store.clone(),
        engine,
        commands: default(),
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: default(),
        information: None,
        speech: None,
        fiote: None,
    };
    app.world_mut()
        .insert_non_send(crate::native::cell_bridge::connect(
            provider,
            crate::native::wake::WakeSignal::new(|| {}),
        ));
    app.world_mut()
        .get_mut::<InfluenceArea>(owner)
        .unwrap()
        .protein
        .as_mut()
        .unwrap()
        .window_record
        .as_mut()
        .unwrap()
        .creating = false;
    app.world_mut().entity_mut(owner).remove::<Creation>();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let mut incoming = Vec::new();
        while let Ok(message) = app
            .world_mut()
            .non_send_mut::<CellBridge>()
            .incoming
            .try_recv()
        {
            incoming.push(CellMessage(message));
        }
        app.world_mut().write_message_batch(incoming);
        app.update();
        if app.world().resource::<Runtime>().areas[&owner]
            .data
            .iter()
            .any(|data| data["uid"] == uid)
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "The confirmed Record did not load through its actual Cell query"
        );
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    assert_eq!(
        app.world().resource::<Runtime>().areas[&owner].row_entities[&uid],
        row
    );
    assert!(app.world().get::<rows::Loading>(row).is_none());
    assert_eq!(
        app.world()
            .get::<crate::native::area::RecordProperties>(row)
            .unwrap()
            .0["head"],
        "Confirmed application Record"
    );
    assert!(
        app.world_mut()
            .query::<(&EditableText, &RecordBinding)>()
            .iter(app.world())
            .any(|(_, binding)| binding.uid == uid)
    );
}
