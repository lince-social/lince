use engine::{Engine, actions::Action};
use nucleus::{MessageState, RecordKind};
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};

static SELECTIONS: AtomicU64 = AtomicU64::new(0);
static BATCHES: AtomicU64 = AtomicU64::new(0);

#[tokio::test]
async fn many_records_share_thread_selection_without_sharing_message_pages_or_stale_results() {
    assert!(nucleus::performance::install(|reading| match reading {
        nucleus::performance::Reading::Count("protein-thread-selections", count) => {
            SELECTIONS.fetch_add(count, Ordering::Relaxed);
        }
        nucleus::performance::Reading::Count("record-related-subject-batches", count) => {
            BATCHES.fetch_add(count, Ordering::Relaxed);
        }
        _ => {}
    }));
    let engine = Engine::open_memory().await.unwrap();
    let mut ids = Vec::new();
    for index in 0..64 {
        ids.push(
            store::records::create(
                &engine.store.pool,
                store::records::NewRecord {
                    slug: None,
                    kind: RecordKind::Plain,
                    head: &format!("Thread root {index}"),
                    body: "The root's own description is not requested.",
                    quantity: store::exact::one(),
                },
            )
            .await
            .unwrap()
            .uid,
        );
    }
    let mut threads = Vec::new();
    for index in 0..3 {
        let uid = engine
            .act(
                Action::CreateThread {
                    target: ids[index % 2].clone(),
                    head: format!("Discussion {index}"),
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        for message in 0..3 {
            engine
                .act(
                    Action::CreateMessage {
                        content: Vec::new(),
                        thread: uid.clone(),
                        body: format!("Thread {index}, message {message}"),
                        author: None,
                        state: MessageState::Finished,
                        parent: None,
                        references: Vec::new(),
                    },
                    None,
                )
                .await
                .unwrap();
        }
        threads.push(uid);
    }
    let mut query: protein::Protein = serde_json::from_value(json!({
        "source": "record", "where": [{"any": ids.iter().map(|uid| json!({"uid_eq": uid})).collect::<Vec<_>>() }],
        "fields": ["uid", "threads"], "include": {"threads": {"messages_limit": 2}}
    })).unwrap();
    query
        .include
        .threads
        .as_mut()
        .unwrap()
        .message_limits
        .insert(threads[1].clone(), 3);
    SELECTIONS.store(0, Ordering::Relaxed);
    BATCHES.store(0, Ordering::Relaxed);
    let rows = protein::execute(&engine.store, &query).await.unwrap();
    assert_eq!(rows.len(), ids.len());
    assert_eq!(SELECTIONS.load(Ordering::Relaxed), 1);
    assert_eq!(BATCHES.load(Ordering::Relaxed), 1);
    for row in &rows {
        assert!(row.get("body").is_none());
        let mut single = query.clone();
        single.filter = vec![protein::Predicate::UidEq(
            row["uid"].as_str().unwrap().into(),
        )];
        assert_eq!(
            protein::execute(&engine.store, &single).await.unwrap(),
            vec![row.clone()]
        );
    }
    let first = rows.iter().find(|row| row["uid"] == ids[0]).unwrap();
    assert_eq!(first["threads"].as_array().unwrap().len(), 2);
    for thread in first["threads"].as_array().unwrap() {
        assert_eq!(thread["messages"].as_array().unwrap().len(), 2);
        assert_eq!(thread["messages_has_more"], true);
    }
    let second = rows.iter().find(|row| row["uid"] == ids[1]).unwrap();
    assert_eq!(
        second["threads"][0]["messages"].as_array().unwrap().len(),
        3
    );
    assert_eq!(second["threads"][0]["messages_has_more"], false);
    let cursor = serde_json::from_value(first["threads"][0]["messages_before"].clone()).unwrap();
    let thread = first["threads"][0]["uid"].as_str().unwrap().to_owned();
    query
        .include
        .threads
        .as_mut()
        .unwrap()
        .message_before
        .insert(thread.clone(), cursor);
    let older = protein::execute(&engine.store, &query).await.unwrap();
    let first = older.iter().find(|row| row["uid"] == ids[0]).unwrap();
    let page = first["threads"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["uid"] == thread)
        .unwrap();
    assert_eq!(page["messages"].as_array().unwrap().len(), 1);
    assert_eq!(page["messages_has_more"], false);
    engine
        .act(
            Action::DeleteRecord {
                target: thread.clone(),
            },
            None,
        )
        .await
        .unwrap();
    let changed = protein::execute(&engine.store, &query).await.unwrap();
    assert!(changed.iter().all(|row| {
        row["threads"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["uid"] != thread)
    }));
    query.filter = vec![protein::Predicate::UidEq("missing-record".into())];
    SELECTIONS.store(0, Ordering::Relaxed);
    BATCHES.store(0, Ordering::Relaxed);
    assert!(
        protein::execute(&engine.store, &query)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(SELECTIONS.load(Ordering::Relaxed), 0);
    assert_eq!(BATCHES.load(Ordering::Relaxed), 0);
}
