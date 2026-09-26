use std::sync::Arc;

use engine::{
    Engine,
    calls::{Identity, Operation, Request, Signal, Tracks},
    wire::{Reach, Wire},
};

async fn fixture() -> (Arc<Engine>, Arc<Wire>, String, String, Vec<String>) {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let wire = Arc::new(
        Wire::bind(
            engine.clone(),
            iroh::SecretKey::from_bytes(&[93; 32]),
            Reach::Local,
        )
        .await
        .unwrap(),
    );
    wire.serve_enrolment();
    let root = store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Plain,
            head: "Private",
            body: "",
            quantity: store::exact::one(),
        },
    )
    .await
    .unwrap();
    store::replica::make_own_root(&engine.store.pool, &root.uid)
        .await
        .unwrap();
    let thread = engine.open_thread(&root.uid, "Discussion").await.unwrap();
    let role = store::auth::ensure_role(&engine.store.pool, "call-person")
        .await
        .unwrap();
    let permission = store::auth::ensure_permission(&engine.store.pool, "record", "read")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, permission)
        .await
        .unwrap();
    store::role_policies::set(
        &engine.store.pool,
        role,
        &serde_json::json!({"read":{"all":[]},"grants":[]}),
        0,
    )
    .await
    .unwrap();
    let mut people = Vec::new();
    for index in 0..7 {
        let person = store::records::create(
            &engine.store.pool,
            store::records::NewRecord {
                slug: None,
                kind: nucleus::RecordKind::Person,
                head: &format!("Person {index}"),
                body: "",
                quantity: store::exact::one(),
            },
        )
        .await
        .unwrap();
        store::auth::compare_and_set_role(&engine.store.pool, &person.uid, Some(role), 0)
            .await
            .unwrap();
        people.push(person.uid);
    }
    for person in &people {
        store::visibility::grant(&engine.store.pool, "actor", Some(person), &thread)
            .await
            .unwrap();
    }
    (engine, wire, organ, thread, people)
}

async fn call(
    engine: &Engine,
    thread: &str,
    person: &str,
    device: &str,
    operation: Operation,
) -> Result<engine::calls::Snapshot, engine::EngineError> {
    engine
        .call_for_session(thread.into(), Some(person.into()), None, device, operation)
        .await
}

#[tokio::test]
async fn simultaneous_starts_share_one_call_and_enforce_device_and_screen_limits() {
    let (engine, _wire, _, thread, people) = fixture().await;
    let (a, b) = tokio::join!(
        call(&engine, &thread, &people[0], "a", Operation::Start),
        call(&engine, &thread, &people[1], "b", Operation::Start)
    );
    let id = a.unwrap().call.unwrap();
    assert_eq!(b.unwrap().call.as_ref(), Some(&id));
    assert!(
        call(
            &engine,
            &thread,
            &people[0],
            "duplicate",
            Operation::Join { call: id.clone() }
        )
        .await
        .is_err()
    );
    for (index, person) in people.iter().enumerate().take(6).skip(2) {
        call(
            &engine,
            &thread,
            person,
            &format!("device-{index}"),
            Operation::Join { call: id.clone() },
        )
        .await
        .unwrap();
    }
    assert!(
        call(
            &engine,
            &thread,
            &people[6],
            "seventh",
            Operation::Join { call: id.clone() }
        )
        .await
        .is_err()
    );
    call(
        &engine,
        &thread,
        &people[0],
        "a",
        Operation::Tracks {
            call: id.clone(),
            tracks: Tracks {
                screen: true,
                ..Default::default()
            },
        },
    )
    .await
    .unwrap();
    assert!(
        call(
            &engine,
            &thread,
            &people[1],
            "b",
            Operation::Tracks {
                call: id.clone(),
                tracks: Tracks {
                    screen: true,
                    ..Default::default()
                }
            }
        )
        .await
        .is_err()
    );
    assert!(
        call(
            &engine,
            &thread,
            &people[1],
            "b",
            Operation::End { call: id.clone() }
        )
        .await
        .is_err()
    );
    let snapshot = call(
        &engine,
        &thread,
        &people[0],
        "a",
        Operation::Leave { call: id.clone() },
    )
    .await
    .unwrap();
    assert_eq!(snapshot.participants.len(), 5);
    call(
        &engine,
        &thread,
        &people[1],
        "b",
        Operation::Poll { call: id },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn signaling_is_bound_to_admission_and_never_enters_sync() {
    let (engine, _wire, organ, thread, people) = fixture().await;
    let id = call(&engine, &thread, &people[0], "a", Operation::Start)
        .await
        .unwrap()
        .call
        .unwrap();
    call(
        &engine,
        &thread,
        &people[1],
        "b",
        Operation::Join { call: id.clone() },
    )
    .await
    .unwrap();
    let to = Identity {
        organ: organ.clone(),
        person: people[1].clone(),
        device: "b".into(),
    };
    let signal = Operation::Signal {
        call: id.clone(),
        to: to.clone(),
        signal: Signal::Offer("private-connection-credential".into()),
    };
    assert!(
        call(&engine, &thread, &people[2], "forged", signal.clone())
            .await
            .is_err()
    );
    assert!(
        engine
            .call_for_session(
                thread.clone(),
                Some(people[1].clone()),
                Some(&people[0]),
                "a",
                signal.clone()
            )
            .await
            .is_err()
    );
    call(&engine, &thread, &people[0], "a", signal)
        .await
        .unwrap();
    let inbox = call(
        &engine,
        &thread,
        &people[1],
        "b",
        Operation::Poll { call: id.clone() },
    )
    .await
    .unwrap();
    assert_eq!(inbox.signals.len(), 1);
    assert_eq!(inbox.signals[0].from.person, people[0]);
    let found: i64 = store::sqlx::query_scalar(
        "SELECT count(*) FROM sync_op WHERE value LIKE '%private-connection-credential%'",
    )
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    assert_eq!(found, 0);
    store::people::deactivate(&engine.store.pool, &people[1], "2026-09-23T00:00:00Z", None)
        .await
        .unwrap();
    let snapshot = call(
        &engine,
        &thread,
        &people[0],
        "a",
        Operation::Poll { call: id.clone() },
    )
    .await
    .unwrap();
    assert_eq!(snapshot.participants.len(), 1);
    assert!(
        call(
            &engine,
            &thread,
            &people[0],
            "a",
            Operation::Signal {
                call: id.clone(),
                to,
                signal: Signal::Answer("revoked".into())
            }
        )
        .await
        .is_err()
    );
    call(
        &engine,
        &thread,
        &people[0],
        "a",
        Operation::End { call: id.clone() },
    )
    .await
    .unwrap();
    let summaries: i64 =
        store::sqlx::query_scalar("SELECT count(*) FROM record WHERE kind = 'call_session'")
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
    assert_eq!(summaries, 1);
    assert!(
        call(
            &engine,
            &thread,
            &people[0],
            "a",
            Operation::Join { call: id }
        )
        .await
        .is_err()
    );
    assert!(
        engine
            .coordinate_call(
                "forged-organ",
                Request {
                    thread,
                    person: Some(people[0].clone()),
                    name: "Impostor".into(),
                    device: "x".into(),
                    operation: Operation::Start
                }
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn group_person_admission_does_not_bypass_a_thread_read_filter() {
    let (engine, _wire, organ, thread, people) = fixture().await;
    engine
        .set_signer(engine::trust::Signer::generate(&organ, "call-group"))
        .await
        .unwrap();
    let peer = nucleus::new_uid("r");
    store::organs::add_contact(&engine.store.pool, &peer, None, "Peer", "", 1)
        .await
        .unwrap();
    store::organs::set_node_id(
        &engine.store.pool,
        &peer,
        Some(&iroh::SecretKey::from_bytes(&[94; 32]).public().to_string()),
    )
    .await
    .unwrap();
    let key = engine::trust::Signer::generate(&peer, "peer");
    engine::trust::adopt_key(&engine.store, &peer, &key.key_id, &key.public_key_b64())
        .await
        .unwrap();
    let group = engine
        .propose_group(&thread, "Fresh group", &[peer], None)
        .await
        .unwrap();
    let fresh = &group.membership.thread;
    assert!(
        call(&engine, fresh, &people[0], "a", Operation::Start)
            .await
            .is_err()
    );
    engine
        .set_group_person(&group.membership.root, &people[0], true, None)
        .await
        .unwrap();
    let snapshot = call(&engine, fresh, &people[0], "a", Operation::Start)
        .await
        .unwrap();
    assert!(!snapshot.participants[0].tracks.microphone);
    assert!(!snapshot.participants[0].tracks.camera);
    assert!(!snapshot.participants[0].tracks.screen);
    assert!(
        call(
            &engine,
            fresh,
            &people[1],
            "b",
            Operation::Join {
                call: snapshot.call.clone().unwrap()
            }
        )
        .await
        .is_err()
    );
    engine
        .set_read_filter(&people[0], Some(&protein::Predicate::Any(Vec::new())))
        .await
        .unwrap();
    assert!(
        call(
            &engine,
            fresh,
            &people[0],
            "a",
            Operation::Poll {
                call: snapshot.call.unwrap()
            }
        )
        .await
        .is_err()
    );
    engine.sweep_calls().await.unwrap();
    let local = engine
        .call_for_session(fresh.clone(), None, None, "observer", Operation::Inspect)
        .await
        .unwrap();
    assert!(local.call.is_none());
    engine.set_read_filter(&people[0], None).await.unwrap();
    engine
        .set_group_person(&group.membership.root, &people[0], false, None)
        .await
        .unwrap();
    assert!(
        call(&engine, fresh, &people[0], "a", Operation::Start)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn admission_expires_when_a_device_stops_renewing() {
    let (engine, _wire, _, thread, people) = fixture().await;
    let id = call(
        &engine,
        &thread,
        &people[0],
        "lost-device",
        Operation::Start,
    )
    .await
    .unwrap()
    .call
    .unwrap();
    tokio::time::sleep(engine::calls::LEASE + std::time::Duration::from_millis(50)).await;
    engine.sweep_calls().await.unwrap();
    let snapshot = engine
        .call_for_session(thread.clone(), None, None, "observer", Operation::Inspect)
        .await
        .unwrap();
    assert!(snapshot.call.is_none());
    assert!(
        call(
            &engine,
            &thread,
            &people[0],
            "lost-device",
            Operation::Tracks {
                call: id,
                tracks: Tracks {
                    microphone: true,
                    ..Default::default()
                }
            }
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn ordinary_record_threads_support_calls_without_a_replication_grant() {
    let (engine, _wire, _, _, people) = fixture().await;
    let thread = store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Thread,
            head: "Local thread",
            body: "",
            quantity: store::exact::one(),
        },
    )
    .await
    .unwrap();
    let started = call(&engine, &thread.uid, &people[0], "a", Operation::Start)
        .await
        .unwrap();
    call(
        &engine,
        &thread.uid,
        &people[0],
        "a",
        Operation::Leave {
            call: started.call.unwrap(),
        },
    )
    .await
    .unwrap();
    let predicate = store::concepts::resolve(&engine.store.pool, "call-in")
        .await
        .unwrap()
        .unwrap();
    let summaries =
        store::assertions::subjects_pointing_to(&engine.store.pool, &predicate, &thread.uid)
            .await
            .unwrap();
    assert_eq!(summaries.len(), 1);
    assert!(
        store::replica::root_of(&engine.store.pool, &summaries[0].uid)
            .await
            .unwrap()
            .is_none()
    );
}
