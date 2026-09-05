use std::sync::Arc;

use engine::Engine;
use engine::actions::Action;
use nucleus::RecordKind;
use transport::{ClientMessage, LaneHub, ServerMessage, Session};

async fn setup() -> (Arc<Engine>, Arc<LaneHub>) {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    (engine, Arc::new(LaneHub::new()))
}

fn subscribe_focus(id: &str) -> ClientMessage {
    ClientMessage::Subscribe {
        id: id.into(),
        protein: protein::focus_queue("before"),
    }
}

#[tokio::test]
async fn subscribe_act_and_live_update_over_one_channel() {
    let (engine, hub) = setup().await;
    let mut s = Session::new(engine.clone(), hub, "conn1", None);

    for slug in ["exercise", "shower"] {
        let out = s
            .handle(ClientMessage::Act {
                id: "a".into(),
                action: Action::CreateRecord {
                    slug: Some(slug.into()),
                    kind: RecordKind::Plain,
                    head: slug.into(),
                    body: String::new(),
                    quantity: -1.0,
                },
            })
            .await;
        assert!(matches!(out.as_slice(), [ServerMessage::ActionOk { .. }]));
    }

    let out = s.handle(subscribe_focus("q")).await;
    let ServerMessage::Snapshot { rows, .. } = &out[0] else {
        panic!("expected snapshot")
    };
    assert_eq!(rows.len(), 2);

    let facts = engine
        .append_user(
            &store::records::resolve(&engine.store.pool, "exercise")
                .await
                .unwrap()
                .unwrap()
                .uid,
            1.0,
        )
        .await
        .unwrap();
    let updates = s.on_fact(&facts[0]).await;
    let ServerMessage::Update { rows, id } = &updates[0] else {
        panic!("expected update")
    };
    assert_eq!(id, "q");
    assert_eq!(rows.len(), 1, "queue recomputed live");
    assert_eq!(rows[0]["slug"], "shower");
}

#[tokio::test]
async fn visibility_subject_gates_the_session() {
    let (engine, hub) = setup().await;
    let public = engine
        .act(
            Action::CreateRecord {
                slug: Some("public".into()),
                kind: RecordKind::Plain,
                head: "p".into(),
                body: String::new(),
                quantity: -1.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    engine
        .act(
            Action::CreateRecord {
                slug: Some("private".into()),
                kind: RecordKind::Plain,
                head: "s".into(),
                body: String::new(),
                quantity: -1.0,
            },
            None,
        )
        .await
        .unwrap();
    engine
        .act(
            Action::GrantVisibility {
                subject_kind: "actor".into(),
                subject: Some("guest".into()),
                target: public,
            },
            None,
        )
        .await
        .unwrap();

    let mut guest = Session::new(engine.clone(), hub, "guest-conn", Some("guest".into()));
    let p = protein::Protein {
        source: protein::Source::Record,
        filter: vec![protein::Predicate::QuantityLt(0.0)],
        fields: None,
        include: protein::Include::default(),
        aggregate: None,
        order: vec![],
        limit: None,
    };
    let out = guest
        .handle(ClientMessage::Subscribe {
            id: "q".into(),
            protein: p,
        })
        .await;
    let ServerMessage::Snapshot { rows, .. } = &out[0] else {
        panic!()
    };
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["slug"], "public");
}

#[tokio::test]
async fn ephemeral_lanes_fan_out_and_never_persist() {
    let (engine, hub) = setup().await;
    let mut alice = Session::new(engine.clone(), hub.clone(), "alice", None);
    let mut bob = Session::new(engine.clone(), hub.clone(), "bob", None);

    alice
        .handle(ClientMessage::LaneJoin {
            room: "doc-42".into(),
        })
        .await;
    bob.handle(ClientMessage::LaneJoin {
        room: "doc-42".into(),
    })
    .await;
    let mut bob_rx = hub.join("doc-42");

    alice
        .handle(ClientMessage::LaneSend {
            room: "doc-42".into(),
            payload: serde_json::json!({ "cursor": 12 }),
            organ: Some("o_marcia".into()),
        })
        .await;
    let event = bob_rx.try_recv().expect("bob sees alice's cursor");
    assert_eq!(event.from, "alice");
    assert_eq!(event.organ.as_deref(), Some("o_marcia"));
    assert_eq!(event.payload["cursor"], 12);

    let facts = store::facts::for_record(&engine.store.pool, "doc-42", 10)
        .await
        .unwrap();
    assert!(facts.is_empty(), "lanes never persist");
    let _ = &mut bob;
}

#[tokio::test]
async fn saved_protein_subscription() {
    let (engine, hub) = setup().await;
    engine
        .act(
            Action::CreateRecord {
                slug: Some("need".into()),
                kind: RecordKind::Plain,
                head: "n".into(),
                body: String::new(),
                quantity: -1.0,
            },
            None,
        )
        .await
        .unwrap();
    engine
        .act(
            Action::SaveProtein {
                slug: "views.needs".into(),
                head: "Needs".into(),
                ast: serde_json::json!({
                    "source": "record",
                    "where": [{ "all": [{ "quantity_lt": 0.0 }] }]
                }),
            },
            None,
        )
        .await
        .unwrap();

    let mut s = Session::new(engine, hub, "c", None);
    let out = s
        .handle(ClientMessage::SubscribeSaved {
            id: "v".into(),
            name: "views.needs".into(),
        })
        .await;
    let ServerMessage::Snapshot { rows, .. } = &out[0] else {
        panic!("expected snapshot")
    };
    assert_eq!(rows.len(), 1);
}

#[tokio::test]
async fn collab_join_is_refused_for_a_record_the_subject_cannot_see() {
    let (engine, hub) = setup().await;

    let uid = store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: Some("private-note"),
            kind: RecordKind::Plain,
            head: "Private",
            body: "not yours",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record")
    .uid;

    let mut remote = Session::new(
        engine.clone(),
        hub.clone(),
        "conn-remote",
        Some("p-someone".into()),
    );
    let out = remote
        .handle(ClientMessage::CollabJoin {
            id: "j".into(),
            record_uid: uid.clone(),
        })
        .await;
    match out.first() {
        Some(ServerMessage::Error { code, .. }) => {
            assert_eq!(code.as_deref(), Some("collab_not_visible"));
        }
        other => panic!("a remote subject must not join an invisible doc: {other:?}"),
    }

    let out = remote
        .handle(ClientMessage::CollabUpdate {
            id: "u".into(),
            record_uid: uid.clone(),
            update_base64: String::new(),
        })
        .await;
    assert!(
        matches!(out.first(), Some(ServerMessage::Error { code, .. }) if code.as_deref() == Some("collab_not_visible")),
        "collab writes must be gated independently of the join"
    );

    let mut local = Session::new(engine.clone(), hub, "conn-local", None);
    let out = local
        .handle(ClientMessage::CollabJoin {
            id: "j".into(),
            record_uid: uid,
        })
        .await;
    assert!(
        matches!(out.first(), Some(ServerMessage::CollabState { .. })),
        "the local Cell must still join: {out:?}"
    );
}

#[tokio::test]
async fn presence_lane_events_carry_the_sender_subject_for_gating() {
    let (engine, hub) = setup().await;
    let mut s = Session::new(
        engine.clone(),
        hub.clone(),
        "conn-a",
        Some("p-alice".into()),
    );
    let mut rx = hub.join("room-1");

    s.handle(ClientMessage::LaneJoin {
        room: "room-1".into(),
    })
    .await;
    s.handle(ClientMessage::LaneSend {
        room: "room-1".into(),
        payload: serde_json::json!({ "cursor": 42 }),
        organ: None,
    })
    .await;

    let event = rx.try_recv().expect("lane event");
    assert_eq!(event.payload, serde_json::json!({ "cursor": 42 }));
    assert_eq!(
        event.from_subject.as_deref(),
        Some("p-alice"),
        "the sender's subject must travel so the RECEIVER can decide whether to name it"
    );
}

#[tokio::test]
async fn the_ephemeral_tick_pushes_only_when_the_answer_changed() {
    let (engine, hub) = setup().await;
    let nearby = engine::wire::Nearby::default();
    nearby.observe("aaa".into(), "AAA".into(), "Laptop".into());
    engine.attach_nearby(nearby.clone());

    let mut s = Session::new(engine.clone(), hub, "conn-nearby", None);
    let out = s
        .handle(ClientMessage::Subscribe {
            id: "nb".into(),
            protein: serde_json::from_value(serde_json::json!({ "source": "nearby" })).unwrap(),
        })
        .await;
    let ServerMessage::Snapshot { rows, .. } = &out[0] else {
        panic!("expected snapshot")
    };
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["known"], false);

    assert!(
        s.tick_ephemeral().await.is_empty(),
        "a quiet network must cost no traffic at all"
    );

    nearby.observe("bbb".into(), "BBB".into(), "Phone".into());
    let updates = s.tick_ephemeral().await;
    let ServerMessage::Update { rows, id } = &updates[0] else {
        panic!("expected update")
    };
    assert_eq!(id, "nb");
    assert_eq!(rows.len(), 2);
    assert!(s.tick_ephemeral().await.is_empty(), "and then quiet again");

    nearby.forget("bbb");
    let updates = s.tick_ephemeral().await;
    let ServerMessage::Update { rows, .. } = &updates[0] else {
        panic!("expected update")
    };
    assert_eq!(rows.len(), 1);

    store::organs::add_contact(&engine.store.pool, "o-friend", None, "Marcia", "", 1)
        .await
        .unwrap();
    store::organs::set_node_id(&engine.store.pool, "o-friend", Some("aaa"))
        .await
        .unwrap();
    let updates = s.tick_ephemeral().await;
    let ServerMessage::Update { rows, .. } = &updates[0] else {
        panic!("expected update")
    };
    assert_eq!(rows[0]["known"], true);
    assert_eq!(rows[0]["name"], "Marcia");
}

#[tokio::test]
async fn a_session_without_an_ephemeral_source_arms_no_tick() {
    let (engine, hub) = setup().await;
    let mut s = Session::new(engine, hub, "conn-plain", None);
    assert!(!s.has_ephemeral_subscriptions());
    s.handle(subscribe_focus("q")).await;
    assert!(!s.has_ephemeral_subscriptions());
    assert!(s.tick_ephemeral().await.is_empty());
}

#[test]
fn a_lane_frame_without_an_organ_still_parses() {
    let old: ClientMessage = serde_json::from_str(
        r#"{"type":"lane_send","room":"recordClicked","payload":{"uid":"r1"}}"#,
    )
    .expect("a board that predates the field is still understood");
    let ClientMessage::LaneSend { organ, payload, .. } = old else {
        panic!("expected a lane send")
    };
    assert_eq!(
        organ, None,
        "no organ named means the Cell hosting the lane"
    );
    assert_eq!(payload, serde_json::json!({ "uid": "r1" }));

    let sent = serde_json::to_value(ServerMessage::LaneEvent {
        room: "recordClicked".into(),
        from: "conn-1".into(),
        payload: serde_json::json!({ "uid": "r1" }),
        identity: None,
        organ: Some("o_marcia".into()),
    })
    .unwrap();
    assert_eq!(sent["payload"], serde_json::json!({ "uid": "r1" }));
    assert_eq!(sent["organ"], "o_marcia");
}
