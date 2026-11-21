use engine::{Engine, actions::Action};
use nucleus::{
    MessageState::Finished,
    RecordKind,
    performance::{Reading::Count, install},
};
use protein::{Predicate, Protein, execute};
use serde_json::{Value, from_value, json};
use std::{
    slice::from_ref,
    sync::atomic::{AtomicU64, Ordering::Relaxed},
};
use store::{
    exact::one,
    records::{NewRecord, create},
};

static SELECTIONS: AtomicU64 = AtomicU64::new(0);
static BATCHES: AtomicU64 = AtomicU64::new(0);

fn reset_work() {
    SELECTIONS.store(0, Relaxed);
    BATCHES.store(0, Relaxed);
}

fn row<'a>(rows: &'a [Value], uid: &str) -> &'a Value {
    rows.iter()
        .find(|row| row["uid"] == uid)
        .expect("projected UID")
}

fn threads(row: &Value) -> &[Value] {
    row["threads"].as_array().expect("projected threads")
}

async fn conversation(engine: &Engine, target: &str, index: usize) -> String {
    let uid = engine
        .act(
            Action::CreateThread {
                target: target.into(),
                head: format!("Discussion {index}"),
            },
            None,
        )
        .await
        .expect("create fixture Thread")
        .created
        .expect("Thread UID");
    for message in 0..3 {
        engine
            .act(
                Action::CreateMessage {
                    content: Vec::new(),
                    thread: uid.clone(),
                    body: format!("Thread {index}, message {message}"),
                    author: None,
                    state: Finished,
                    parent: None,
                    references: Vec::new(),
                },
                None,
            )
            .await
            .expect("create fixture Message");
    }
    uid
}

#[tokio::test]
async fn many_records_share_thread_selection_without_sharing_message_pages_or_stale_results() {
    assert!(install(|reading| {
        let (counter, count) = match reading {
            Count("protein-thread-selections", count) => (&SELECTIONS, count),
            Count("record-related-subject-batches", count) => (&BATCHES, count),
            _ => return,
        };
        counter.fetch_add(count, Relaxed);
    }));
    let engine = Engine::open_memory().await.unwrap();
    let store = &engine.store;
    let mut ids = Vec::with_capacity(64);
    for index in 0..64 {
        let record = create(
            &store.pool,
            NewRecord {
                slug: None,
                kind: RecordKind::Plain,
                head: &format!("Thread root {index}"),
                body: "The root's own description is not requested.",
                quantity: one(),
            },
        )
        .await
        .unwrap();
        ids.push(record.uid);
    }
    let [first_uid, second_uid, ..] = ids.as_slice() else {
        panic!("missing thread roots")
    };
    conversation(&engine, first_uid, 0).await;
    let second_thread = conversation(&engine, second_uid, 1).await;
    conversation(&engine, first_uid, 2).await;

    let mut query: Protein = from_value(json!({
        "source": "record", "where": [{"any": ids.iter().map(|uid| json!({"uid_eq": uid})).collect::<Vec<_>>()}],
        "fields": ["uid", "threads"], "include": {"threads": {"messages_limit": 2}},
    })).unwrap();
    query
        .include
        .threads
        .as_mut()
        .unwrap()
        .message_limits
        .insert(second_thread, 3);
    reset_work();
    let rows = execute(store, &query).await.unwrap();
    assert_eq!(rows.len(), ids.len());
    assert_eq!(SELECTIONS.load(Relaxed), 1);
    assert_eq!(BATCHES.load(Relaxed), 1);
    for row in &rows {
        assert!(row.get("body").is_none());
        let single = Protein {
            filter: vec![Predicate::UidEq(row["uid"].as_str().unwrap().into())],
            ..query.clone()
        };
        assert_eq!(execute(store, &single).await.unwrap(), from_ref(row));
    }

    let first = threads(row(&rows, first_uid));
    assert_eq!(first.len(), 2);
    for thread in first {
        assert_eq!(thread["messages"].as_array().unwrap().len(), 2);
        assert_eq!(thread["messages_has_more"], true);
    }
    let second = &threads(row(&rows, second_uid))[0];
    assert_eq!(second["messages"].as_array().unwrap().len(), 3);
    assert_eq!(second["messages_has_more"], false);

    let cursor = from_value(first[0]["messages_before"].clone()).unwrap();
    let thread = first[0]["uid"].as_str().unwrap();
    query
        .include
        .threads
        .as_mut()
        .unwrap()
        .message_before
        .insert(thread.into(), cursor);
    let older = execute(store, &query).await.unwrap();
    let page = row(threads(row(&older, first_uid)), thread);
    assert_eq!(page["messages"].as_array().unwrap().len(), 1);
    assert_eq!(page["messages_has_more"], false);

    engine
        .act(
            Action::DeleteRecord {
                target: thread.into(),
            },
            None,
        )
        .await
        .unwrap();
    let changed = execute(store, &query).await.unwrap();
    assert!(
        changed
            .iter()
            .flat_map(threads)
            .all(|row| row["uid"] != thread)
    );

    query.filter = vec![Predicate::UidEq("missing-record".into())];
    reset_work();
    assert!(execute(store, &query).await.unwrap().is_empty());
    assert_eq!(SELECTIONS.load(Relaxed), 0);
    assert_eq!(BATCHES.load(Relaxed), 0);
}
