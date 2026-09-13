use engine::{Engine, actions::Action};
use nucleus::{
    RecordKind,
    sync::{Destination, Outcome, Retention},
};
use std::{sync::Arc, time::Duration};
use transport::{ClientMessage, LaneHub, ServerMessage, Session, SyncEvents};

async fn guest(engine: &Engine) -> String {
    let role = store::auth::ensure_role(&engine.store.pool, "guest")
        .await
        .unwrap();
    let permission = store::auth::ensure_permission(&engine.store.pool, "record", "read")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, permission)
        .await
        .unwrap();
    store::auth::create_person_login(&engine.store.pool, "Guest", "guest", "unused", role)
        .await
        .unwrap()
        .to_string()
}

fn query(id: &str) -> ClientMessage {
    ClientMessage::Subscribe {
        id: id.into(),
        protein: protein::Protein {
            source: protein::Source::Record,
            filter: Vec::new(),
            fields: Some(vec!["uid".into(), "head".into()]),
            include: Default::default(),
            aggregate: None,
            order: Vec::new(),
            limit: None,
        },
    }
}

#[tokio::test]
async fn local_status_uses_the_common_history_and_remote_users_cannot_read_or_clear_it() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let hub = Arc::new(LaneHub::new());
    let mut local = Session::local(engine.clone(), hub.clone(), "local");
    local.handle(query("records")).await;
    let messages = local
        .handle(ClientMessage::SyncInspect {
            id: "status".into(),
            before: None,
        })
        .await;
    let ServerMessage::SyncStatus { overview, .. } = &messages[0] else {
        panic!("missing sync status")
    };
    assert!(overview.history.iter().any(|change| matches!(
        change.activity.instance.destination,
        Destination::Interface { .. }
    ) && change.summary.outcome
        == Outcome::Refreshed));
    let history = overview.history.clone();
    let person = guest(&engine).await;
    let mut remote = Session::new(engine.clone(), hub, "remote", Some(person));
    for message in [
        ClientMessage::SyncInspect {
            id: "status".into(),
            before: None,
        },
        ClientMessage::SyncForgetHistory { id: "clear".into() },
        ClientMessage::SyncHistoryPolicy {
            id: "limits".into(),
            retention: Retention {
                seconds: 60,
                max_entries: 1,
            },
        },
    ] {
        assert!(
            matches!(&remote.handle(message).await[0], ServerMessage::Error { code: Some(code), .. } if code == "sync_local_only")
        );
    }
    assert_eq!(engine.sync_overview(None).await.unwrap().history, history);
    let mut anonymous = Session::new(engine.clone(), Arc::new(LaneHub::new()), "anonymous", None);
    assert!(
        matches!(&anonymous.handle(ClientMessage::SyncInspect { id: "anonymous".into(), before: None }).await[0], ServerMessage::Error { code: Some(code), .. } if code == "sync_local_only")
    );
    assert!(
        matches!(&local.handle(ClientMessage::SyncForgetHistory { id: "clear".into() }).await[0], ServerMessage::SyncStatus { overview, .. } if overview.history.is_empty())
    );
}

#[tokio::test]
async fn shared_events_refresh_changes_without_facts_and_stop_when_the_engine_closes() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let mut events = SyncEvents::new(&engine);
    let mut session = Session::new(engine.clone(), Arc::new(LaneHub::new()), "local", None);
    let mut subscription = match query("concepts") {
        ClientMessage::Subscribe { protein, .. } => protein,
        _ => unreachable!(),
    };
    subscription.source = protein::Source::Concept;
    session
        .handle(ClientMessage::Subscribe {
            id: "concepts".into(),
            protein: subscription,
        })
        .await;
    let created = engine
        .act(
            Action::CreateConcept {
                lingua: "g_local".into(),
                name: "shared-event".into(),
                parents: Vec::new(),
            },
            None,
        )
        .await
        .unwrap();
    assert!(created.facts.is_empty());
    let event = tokio::time::timeout(Duration::from_secs(2), events.next(false))
        .await
        .unwrap()
        .unwrap();
    let messages = session.on_sync_event(event).await;
    assert!(
        matches!(&messages[0], ServerMessage::Snapshot { rows, .. } if rows.iter().any(|row| row["uid"] == created.created.as_ref().unwrap().as_str()))
    );
    drop(session);
    drop(engine);
    assert!(
        tokio::time::timeout(Duration::from_secs(2), events.next(false))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn a_joined_document_is_rechecked_before_broadcasting_new_text() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let uid = store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head: "Shared",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid;
    let person = guest(&engine).await;
    let grant = store::visibility::grant(&engine.store.pool, "actor", Some(&person), &uid)
        .await
        .unwrap();
    let mut session = Session::new(
        engine.clone(),
        Arc::new(LaneHub::new()),
        "remote",
        Some(person),
    );
    assert!(matches!(
        &session
            .handle(ClientMessage::CollabJoin {
                id: "join".into(),
                record_uid: uid.clone()
            })
            .await[0],
        ServerMessage::CollabState { .. }
    ));
    store::sqlx::query("DELETE FROM visibility_rule WHERE uid = ?")
        .bind(grant)
        .execute(&engine.store.pool)
        .await
        .unwrap();
    let facts = engine.append_user(&uid, 1.0).await.unwrap();
    assert!(
        !session
            .on_fact(&facts[0])
            .await
            .iter()
            .any(|message| matches!(message, ServerMessage::CollabChange { .. }))
    );
    assert!(
        !session
            .refresh()
            .await
            .iter()
            .any(|message| matches!(message, ServerMessage::CollabChange { .. }))
    );
}
