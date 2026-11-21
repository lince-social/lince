use super::fixtures::{base, engine, make, thread_query};
use engine::actions::Action;
use nucleus::RecordKind;
use protein::{MessageCursor, Predicate, Source, ThreadsInclude, execute};
use serde_json::from_value;
use std::collections::HashSet;
use store::sqlx::query as sql;

fn thread_row<'a>(rows: &'a [serde_json::Value], uid: &str) -> &'a serde_json::Value {
    rows[0]["threads"]
        .as_array()
        .expect("projected threads")
        .iter()
        .find(|thread| thread["uid"] == uid)
        .expect("requested thread")
}

#[tokio::test]
async fn thread_history_loads_newest_then_older_and_excludes_deleted_messages() {
    let e = engine().await;
    let record = make(&e, "thread-history", RecordKind::Plain, 1.0).await;
    let mut threads = Vec::with_capacity(2);
    for title in ["A", "B"] {
        let thread = e
            .act(
                Action::CreateThread {
                    target: record.clone(),
                    head: title.into(),
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let mut ids = Vec::new();
        for index in 0..5 {
            let uid = e
                .act(
                    Action::CreateMessage {
                        content: Vec::new(),
                        thread: thread.clone(),
                        body: format!("{title}-{index}"),
                        author: None,
                        state: nucleus::MessageState::Finished,
                        parent: None,
                        references: Vec::new(),
                    },
                    None,
                )
                .await
                .unwrap()
                .created
                .unwrap();
            sql("UPDATE record SET created_at = ? WHERE uid = ?")
                .bind(format!("2026-01-01T00:00:0{index}Z"))
                .bind(&uid)
                .execute(&e.store.pool)
                .await
                .unwrap();
            ids.push(uid);
        }
        threads.push((thread, ids));
    }
    let [(first_uid, first_messages), (second_uid, _)] = threads.as_slice() else {
        panic!("expected two fixture threads")
    };
    let mut query = thread_query(record, 2);
    let rows = execute(&e.store, &query).await.unwrap();
    let first = thread_row(&rows, first_uid);
    assert_eq!(first["messages_has_more"], true);
    assert_eq!(first["messages"][0]["body"], "A-3");
    assert_eq!(first["messages"][1]["body"], "A-4");
    let cursor: MessageCursor = from_value(first["messages_before"].clone()).unwrap();
    query
        .include
        .threads
        .as_mut()
        .unwrap()
        .message_before
        .insert(first_uid.to_owned(), cursor);
    let older = execute(&e.store, &query).await.unwrap();
    let first = thread_row(&older, first_uid);
    assert_eq!(first["messages"][0]["body"], "A-1");
    assert_eq!(first["messages"][1]["body"], "A-2");
    assert_eq!(first["messages_has_more"], true);
    let cursor = from_value(first["messages_before"].clone()).unwrap();
    query
        .include
        .threads
        .as_mut()
        .unwrap()
        .message_before
        .insert(first_uid.to_owned(), cursor);
    let oldest = execute(&e.store, &query).await.unwrap();
    let first = thread_row(&oldest, first_uid);
    assert_eq!(first["messages"].as_array().unwrap().len(), 1);
    assert_eq!(first["messages"][0]["body"], "A-0");
    assert_eq!(first["messages_has_more"], false);
    query
        .include
        .threads
        .as_mut()
        .unwrap()
        .message_before
        .clear();
    query
        .include
        .threads
        .as_mut()
        .unwrap()
        .message_limits
        .insert(first_uid.to_owned(), 10);
    let rows = execute(&e.store, &query).await.unwrap();
    let first = thread_row(&rows, first_uid);
    let second = thread_row(&rows, second_uid);
    assert_eq!(first["messages"].as_array().unwrap().len(), 5);
    assert_eq!(first["messages_has_more"], false);
    assert_eq!(second["messages"].as_array().unwrap().len(), 2);
    e.act(
        Action::DeleteRecord {
            target: first_messages[3].clone(),
        },
        None,
    )
    .await
    .unwrap();
    let rows = execute(&e.store, &query).await.unwrap();
    let first = thread_row(&rows, first_uid);
    assert_eq!(first["messages"].as_array().unwrap().len(), 4);
    assert!(
        !first["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| message["uid"] == first_messages[3])
    );
}

#[tokio::test]
async fn message_cursor_survives_deleted_boundary_and_equal_timestamps() {
    let e = engine().await;
    let record = make(&e, "history-cursor", RecordKind::Plain, 1.0).await;
    let thread = e
        .act(
            Action::CreateThread {
                target: record.clone(),
                head: "History".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let mut ids = Vec::new();
    for index in 0..4 {
        let uid = e
            .act(
                Action::CreateMessage {
                    content: Vec::new(),
                    thread: thread.clone(),
                    body: index.to_string(),
                    author: None,
                    state: nucleus::MessageState::Finished,
                    parent: None,
                    references: Vec::new(),
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        sql("UPDATE record SET created_at = '2026-01-01T00:00:00Z' WHERE uid = ?")
            .bind(&uid)
            .execute(&e.store.pool)
            .await
            .unwrap();
        ids.push(uid);
    }
    ids.sort_unstable();
    let mut query = thread_query(record, 2);
    let rows = execute(&e.store, &query).await.unwrap();
    let page = &rows[0]["threads"][0];
    assert_eq!(page["messages"][0]["uid"], ids[2]);
    let cursor = from_value(page["messages_before"].clone()).unwrap();
    e.act(
        Action::DeleteRecord {
            target: ids[2].clone(),
        },
        None,
    )
    .await
    .unwrap();
    query
        .include
        .threads
        .as_mut()
        .unwrap()
        .message_before
        .insert(thread, cursor);
    let rows = execute(&e.store, &query).await.unwrap();
    let page = &rows[0]["threads"][0];
    assert_eq!(page["messages"][0]["uid"], ids[0]);
    assert_eq!(page["messages"][1]["uid"], ids[1]);
    assert_eq!(page["messages_has_more"], false);
}

#[tokio::test]
async fn message_pages_reach_past_two_thousand_with_bounded_results() {
    let e = engine().await;
    let record = make(&e, "long-history", RecordKind::Plain, 1.0).await;
    let thread = e
        .act(
            Action::CreateThread {
                target: record.clone(),
                head: "History".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let predicate = store::concepts::ensure(&e.store.pool, "message-in")
        .await
        .unwrap();
    for index in 0..2003 {
        let message = store::records::create(
            &e.store.pool,
            store::records::NewRecord {
                slug: None,
                kind: RecordKind::Message,
                head: "",
                body: &index.to_string(),
                quantity: store::exact::one(),
            },
        )
        .await
        .unwrap();
        store::assertions::assert(
            &e.store.pool,
            store::assertions::NewAssertion {
                subject_uid: &message.uid,
                predicate_uid: &predicate,
                object_uid: Some(&thread),
                role: store::assertions::AssertionRole::Ordinary,
                quantity: None,
                unit_uid: None,
                asserted_by: None,
            },
        )
        .await
        .unwrap();
    }
    let mut query = base(Source::Record, vec![Predicate::UidEq(record)]);
    query.include.threads = Some(ThreadsInclude::default());
    let mut seen = HashSet::new();
    loop {
        let rows = execute(&e.store, &query).await.unwrap();
        let page = &rows[0]["threads"][0];
        let messages = page["messages"].as_array().unwrap();
        assert!(messages.len() <= 50);
        for message in messages {
            assert!(seen.insert(message["uid"].as_str().unwrap().to_owned()));
        }
        if page["messages_has_more"] != true {
            break;
        }
        let cursor = from_value(page["messages_before"].clone()).unwrap();
        query
            .include
            .threads
            .as_mut()
            .unwrap()
            .message_before
            .insert(thread.clone(), cursor);
    }
    assert_eq!(seen.len(), 2003);
}
