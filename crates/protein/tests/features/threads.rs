use super::fixtures::{engine, make, thread_query};
use engine::actions::Action;
use nucleus::RecordKind;
use protein::execute;
use serde_json::json;
use store::auth;

#[tokio::test]
async fn threads_include_keeps_an_empty_thread_without_a_message_predicate() {
    let e = engine().await;
    let subject = make(&e, "empty-conversation", RecordKind::Plain, 1.0).await;
    e.act(
        Action::CreateThread {
            target: subject.clone(),
            head: "Introductions".into(),
        },
        None,
    )
    .await
    .unwrap();

    assert!(
        store::concepts::resolve(&e.store.pool, "message-in")
            .await
            .unwrap()
            .is_none(),
        "the regression requires a Cell on which no message has existed"
    );
    let p = thread_query(subject, 20);
    let rows = execute(&e.store, &p).await.unwrap();
    let threads = rows[0]["threads"].as_array().unwrap();
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0]["head"], "Introductions");
    assert_eq!(threads[0]["messages"], json!([]));
}

#[tokio::test]
async fn threads_include_returns_nested_record_messages() {
    let e = engine().await;
    let subject = make(&e, "abstract-idea", RecordKind::Plain, 0.0).await;
    let receipt = make(&e, "receipt.july", RecordKind::Plain, 1.0).await;
    let thread = e
        .act(
            Action::CreateThread {
                target: subject.clone(),
                head: "Discussion A".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let first = e
        .act(
            Action::CreateMessage {
                content: Vec::new(),
                thread: thread.clone(),
                body: "First message".into(),
                author: None,
                state: nucleus::MessageState::Finished,
                parent: None,
                references: vec![receipt.clone()],
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let reply = e
        .act(
            Action::CreateMessage {
                content: Vec::new(),
                thread,
                body: "Reply message".into(),
                author: None,
                state: nucleus::MessageState::Finished,
                parent: Some(first.clone()),
                references: vec![],
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();

    let p = thread_query(subject, 20);
    let rows = execute(&e.store, &p).await.unwrap();
    let threads = rows[0]["threads"].as_array().unwrap();
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0]["head"], "Discussion A");
    let messages = threads[0]["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["body"], "First message");
    assert_eq!(messages[0]["references"][0]["uid"], receipt);
    assert_eq!(messages[0]["references"][0]["head"], "receipt.july");
    assert_eq!(messages[1]["body"], "Reply message");
    assert_eq!(
        messages[1]["parent_message_uid"].as_str(),
        Some(first.as_str())
    );
    assert_eq!(messages[1]["uid"].as_str(), Some(reply.as_str()));
    assert!(threads[0]["created_at"].as_str().is_some());
    assert!(messages[0]["created_at"].as_str().is_some());
    assert!(messages[1]["created_at"].as_str().is_some());
    assert!(threads[0]["sender"].is_null());
    assert!(messages[0]["sender"].is_null());
    assert!(threads[0]["created_by"].is_null());
    assert!(messages[0]["created_by"].is_null());
    assert!(messages[0]["author"].as_str().is_some());
    assert!(messages[0]["operator"].as_str().is_some());
    assert_eq!(messages[0]["message_state"], "finished");
}

#[tokio::test]
async fn threads_include_resolves_sender_name_from_the_actor() {
    let e = engine().await;
    let pool = &e.store.pool;
    let role = auth::ensure_role(pool, "lince").await.unwrap();
    for action in ["create", "read"] {
        let permission = auth::ensure_permission(pool, "record", action)
            .await
            .unwrap();
        auth::grant(pool, role, permission).await.unwrap();
    }
    let person = auth::create_person_login(pool, "Ana Diaz", "ana", "hash-not-checked-here", role)
        .await
        .unwrap();

    let subject = make(&e, "abstract-idea", RecordKind::Plain, 0.0).await;
    store::visibility::grant(pool, "actor", Some(&person), &subject)
        .await
        .unwrap();
    let thread = e
        .act(
            Action::CreateThread {
                target: subject.clone(),
                head: "Discussion B".into(),
            },
            Some(person.to_owned()),
        )
        .await
        .unwrap()
        .created
        .unwrap();
    e.act(
        Action::CreateMessage {
            content: Vec::new(),
            thread,
            body: "hi from ana".into(),
            author: None,
            state: nucleus::MessageState::Finished,
            parent: None,
            references: vec![],
        },
        Some(person.to_owned()),
    )
    .await
    .unwrap();

    let p = thread_query(subject, 20);
    let rows = execute(&e.store, &p).await.unwrap();
    let threads = rows[0]["threads"].as_array().unwrap();
    assert_eq!(threads[0]["sender"].as_str(), Some("Ana Diaz"));
    assert_eq!(threads[0]["created_by"].as_str(), Some(person.as_str()));
    let messages = threads[0]["messages"].as_array().unwrap();
    assert_eq!(messages[0]["sender"].as_str(), Some("Ana Diaz"));
    assert_eq!(messages[0]["created_by"].as_str(), Some(person.as_str()));
}
