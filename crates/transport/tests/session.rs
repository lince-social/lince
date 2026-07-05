//! Transport session acceptance (blueprint VII.3): one channel carries
//! subscriptions, actions, live updates, and ephemeral lanes — the contract a
//! sand speaks. No socket: messages are driven directly.

use std::sync::Arc;

use engine::actions::Action;
use engine::Engine;
use nucleus::RecordKind;
use transport::{ClientMessage, LaneHub, ServerMessage, Session};

async fn setup() -> (Arc<Engine>, Arc<LaneHub>) {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    (engine, Arc::new(LaneHub::new()))
}

fn subscribe_focus(id: &str) -> ClientMessage {
    ClientMessage::Subscribe { id: id.into(), protein: protein::focus_queue("before") }
}

#[tokio::test]
async fn subscribe_act_and_live_update_over_one_channel() {
    let (engine, hub) = setup().await;
    let mut s = Session::new(engine.clone(), hub, "conn1", None);

    // create two Needs through Actions (the only write path)
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

    // subscribe to the focus queue: immediate snapshot of both Needs
    let out = s.handle(subscribe_focus("q")).await;
    let ServerMessage::Snapshot { rows, .. } = &out[0] else { panic!("expected snapshot") };
    assert_eq!(rows.len(), 2);

    // completing a Need is an Action -> a fact; feeding that fact to the session
    // pushes a live Update with the shrunken queue
    let facts = engine.append_user(
        &store::records::resolve(&engine.store.pool, "exercise").await.unwrap().unwrap().uid,
        1.0, // -1 -> 0: no longer a Need
    ).await.unwrap();
    let updates = s.on_fact(&facts[0]).await;
    let ServerMessage::Update { rows, id } = &updates[0] else { panic!("expected update") };
    assert_eq!(id, "q");
    assert_eq!(rows.len(), 1, "queue recomputed live");
    assert_eq!(rows[0]["slug"], "shower");
}

#[tokio::test]
async fn visibility_subject_gates_the_session() {
    let (engine, hub) = setup().await;
    // a public record and a private one
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

    // a guest session subscribes and sees only the granted record
    let mut guest = Session::new(engine.clone(), hub, "guest-conn", Some("guest".into()));
    let p = protein::Protein {
        source: protein::Source::Record,
        filter: vec![protein::Predicate::QuantityLt(0.0)],
        include: protein::Include::default(),
        aggregate: None,
        order: vec![],
        limit: None,
    };
    let out = guest.handle(ClientMessage::Subscribe { id: "q".into(), protein: p }).await;
    let ServerMessage::Snapshot { rows, .. } = &out[0] else { panic!() };
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["slug"], "public");
}

#[tokio::test]
async fn ephemeral_lanes_fan_out_and_never_persist() {
    let (engine, hub) = setup().await;
    let mut alice = Session::new(engine.clone(), hub.clone(), "alice", None);
    let mut bob = Session::new(engine.clone(), hub.clone(), "bob", None);

    // both join the same room; Bob holds a receiver
    alice.handle(ClientMessage::LaneJoin { room: "doc-42".into() }).await;
    bob.handle(ClientMessage::LaneJoin { room: "doc-42".into() }).await;
    let mut bob_rx = hub.join("doc-42");

    // Alice sends a cursor position; Bob receives it
    alice
        .handle(ClientMessage::LaneSend {
            room: "doc-42".into(),
            payload: serde_json::json!({ "cursor": 12 }),
        })
        .await;
    let event = bob_rx.try_recv().expect("bob sees alice's cursor");
    assert_eq!(event.from, "alice");
    assert_eq!(event.payload["cursor"], 12);

    // nothing about presence touched the Ledger
    let facts = store::facts::for_record(&engine.store.pool, "doc-42", 10).await.unwrap();
    assert!(facts.is_empty(), "lanes never persist");
    // keep bob's subscription alive to the end
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
                ast: serde_json::json!({ "source": "record", "where": [ { "quantity_lt": 0.0 } ] }),
            },
            None,
        )
        .await
        .unwrap();

    let mut s = Session::new(engine, hub, "c", None);
    let out = s
        .handle(ClientMessage::SubscribeSaved { id: "v".into(), name: "views.needs".into() })
        .await;
    let ServerMessage::Snapshot { rows, .. } = &out[0] else { panic!("expected snapshot") };
    assert_eq!(rows.len(), 1);
}
