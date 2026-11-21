use nucleus::RecordKind;
use sqlx::{SqlitePool, query, query_scalar};

use super::{
    compaction_due_on, crdt_ops_since, delete, doc_tail, has_crdt_history, put,
    records_needing_compaction,
};
use crate::{
    Store,
    exact::zero,
    records::{NewRecord, create},
};

async fn journal_op(pool: &SqlitePool, uid: &str, kind: &str, value: Option<&str>) -> i64 {
    query_scalar(
        "INSERT INTO sync_op (tbl, uid, kind, value, hlc, actor_cell, organ_uid)
         VALUES ('record', ?, ?, ?,
                 (SELECT COALESCE(MAX(hlc), 0) + 1 FROM sync_op WHERE actor_cell = 'test-cell'),
                 'test-cell', 'test-organ') RETURNING seq",
    )
    .bind(uid)
    .bind(kind)
    .bind(value)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn journal_tail_and_compaction_respect_the_stored_sequence() {
    let store = Store::open_memory().await.unwrap();
    let pool = &store.pool;
    let record = create(
        pool,
        NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head: "Journal",
            body: "",
            quantity: zero(),
        },
    )
    .await
    .unwrap();
    let uid = &record.uid;
    assert!(!has_crdt_history(pool, uid).await.unwrap());

    let through = journal_op(pool, uid, "crdt", Some("retired")).await;
    assert!(has_crdt_history(pool, uid).await.unwrap());
    put(pool, uid, b"snapshot", through).await.unwrap();
    let change = journal_op(pool, uid, "crdt", Some("change")).await;
    let snapshot = journal_op(pool, uid, "snapshot", Some("snapshot marker")).await;
    journal_op(pool, uid, "set", Some("other field")).await;
    journal_op(pool, uid, "crdt", None).await;

    assert_eq!(
        doc_tail(pool, uid, through).await.unwrap(),
        vec![
            (change, "change".to_owned()),
            (snapshot, "snapshot marker".to_owned())
        ]
    );
    assert_eq!(crdt_ops_since(pool, uid, through).await.unwrap(), 2);
    assert!(
        records_needing_compaction(pool, 2)
            .await
            .unwrap()
            .contains(uid)
    );
    assert!(
        !records_needing_compaction(pool, 3)
            .await
            .unwrap()
            .contains(uid)
    );
    let mut tx = pool.begin().await.unwrap();
    assert!(compaction_due_on(&mut tx, uid, 2).await.unwrap());
    assert!(!compaction_due_on(&mut tx, uid, 3).await.unwrap());
    tx.commit().await.unwrap();

    query("DELETE FROM sync_op WHERE uid = ? AND kind IN ('crdt', 'snapshot')")
        .bind(uid)
        .execute(pool)
        .await
        .unwrap();
    assert!(has_crdt_history(pool, uid).await.unwrap());
    delete(pool, uid).await.unwrap();
    assert!(!has_crdt_history(pool, uid).await.unwrap());
}
