#![cfg(feature = "instinct")]

use engine::{Engine, actions::Action};

async fn count(engine: &Engine, table: &str) -> i64 {
    store::sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(&engine.store.pool).await.unwrap()
}

#[tokio::test]
async fn preview_cancel_repeat_and_conflicts_preserve_the_database() {
    let engine = Engine::open_memory().await.unwrap();
    let before = count(&engine, "record").await;
    let preview = engine.preview_instinct(None).await.unwrap();
    assert!(preview.conflicts.is_empty());
    assert_eq!(preview.created, preview.records.len());
    assert_eq!(count(&engine, "record").await, before);
    let imported = engine.import_instinct(&preview.fingerprint, None, chrono::Utc::now()).await.unwrap();
    assert_eq!(imported.data.unwrap()["created"], preview.records.len());
    let next = engine.preview_instinct(None).await.unwrap();
    assert!(next.conflicts.is_empty(), "{:?}", next.conflicts);
    assert_eq!(next.reused, preview.records.len());
    let facts = count(&engine, "fact").await;
    engine.import_instinct(&next.fingerprint, None, chrono::Utc::now()).await.unwrap();
    assert_eq!(count(&engine, "fact").await, facts);
    let uid = &preview.records[0].projection.uid;
    engine.act(Action::EditRecordText { target: uid.clone(), head: None, body: Some("My edit".into()) }, None).await.unwrap();
    let conflict = engine.preview_instinct(None).await.unwrap();
    assert!(!conflict.conflicts.is_empty());
    assert!(engine.import_instinct(&conflict.fingerprint, None, chrono::Utc::now()).await.is_err());
    assert_eq!(store::records::get(&engine.store.pool, uid).await.unwrap().unwrap().body, "My edit");
}

#[tokio::test]
async fn slug_races_and_permission_denial_leave_no_partial_import() {
    let engine = Engine::open_memory().await.unwrap();
    let preview = engine.preview_instinct(None).await.unwrap();
    let slug = preview.records.iter().find_map(|record| record.slug.clone()).unwrap();
    engine.act(Action::CreateRecord { slug: Some(slug), kind: nucleus::RecordKind::Plain, head: "Personal Record".into(), body: String::new(), quantity: 0.0 }, None).await.unwrap();
    let before = count(&engine, "record").await;
    assert!(engine.import_instinct(&preview.fingerprint, None, chrono::Utc::now()).await.is_err());
    let conflict = engine.preview_instinct(None).await.unwrap();
    assert!(conflict.conflicts.iter().any(|message| message.contains("slug")));
    assert!(engine.import_instinct(&conflict.fingerprint, None, chrono::Utc::now()).await.is_err());
    assert!(engine.preview_instinct(Some("untrusted")).await.is_err());
    assert!(engine.import_instinct(&preview.fingerprint, Some("untrusted"), chrono::Utc::now()).await.is_err());
    assert_eq!(count(&engine, "record").await, before);
}

#[tokio::test]
async fn failures_during_records_and_assertions_roll_back_every_write() {
    for table in ["record", "record_assertion"] {
        let engine = Engine::open_memory().await.unwrap();
        let preview = engine.preview_instinct(None).await.unwrap();
        let before: Vec<_> = {
            let mut values = Vec::new();
            for table in ["record", "record_assertion", "concept", "lingua", "fact", "sync_op"] { values.push((table, count(&engine, table).await)); }
            values
        };
        let statement = format!("CREATE TRIGGER refuse_instinct BEFORE INSERT ON {table} WHEN (SELECT COUNT(*) FROM {table}) > {} BEGIN SELECT RAISE(ABORT, 'injected import failure'); END", count(&engine, table).await + 2);
        store::sqlx::query(&statement).execute(&engine.store.pool).await.unwrap();
        assert!(engine.import_instinct(&preview.fingerprint, None, chrono::Utc::now()).await.is_err());
        for (table, count_before) in before { assert_eq!(count(&engine, table).await, count_before, "{table}"); }
    }
}
