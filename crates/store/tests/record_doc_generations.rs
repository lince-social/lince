use std::borrow::Cow;
use std::str::FromStr;

use nucleus::RecordKind;
use store::Store;
use store::record_docs::Qualification;

async fn record(store: &Store, head: &str) -> String {
    store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head,
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid
}

async fn metadata(store: &Store, record_uid: &str) -> store::record_docs::RecordDocMetadata {
    let mut tx = store.pool.begin().await.unwrap();
    let metadata = store::record_docs::metadata_on(&mut tx, record_uid)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    metadata
}

async fn snapshot(store: &Store, record_uid: &str) -> store::record_docs::RecordDocSnapshot {
    let mut tx = store.pool.begin().await.unwrap();
    let snapshot = store::record_docs::snapshot_on(&mut tx, record_uid)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    snapshot
}

#[tokio::test]
async fn qualified_snapshots_are_bounded_and_use_exact_generation_and_revision() {
    let store = Store::open_memory().await.unwrap();
    let uid = record(&store, "Qualified").await;
    let initial = metadata(&store, &uid).await;
    assert_eq!(initial.record_revision, 1);
    assert_eq!(initial.retained_generation, 1);
    assert_eq!(initial.qualification, Qualification::Absent);

    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        store::record_docs::put_qualified_on(&mut tx, &uid, b"wrong", 4, 2, 1)
            .await
            .unwrap_err()
            .to_string()
            .contains("generation conflict")
    );
    assert!(
        store::record_docs::put_qualified_on(&mut tx, &uid, b"wrong", 4, 1, 2)
            .await
            .unwrap_err()
            .to_string()
            .contains("revision conflict")
    );
    let written = store::record_docs::put_qualified_on(&mut tx, &uid, b"snapshot", 4, 1, 1)
        .await
        .unwrap();
    assert_eq!(
        written.qualification,
        Qualification::Qualified {
            generation: 1,
            base_revision: 1,
        }
    );
    assert_eq!(written.snapshot_bytes, Some(8));
    tx.commit().await.unwrap();

    let stored = snapshot(&store, &uid).await;
    assert_eq!(stored.snapshot.as_deref(), Some(b"snapshot".as_slice()));
    assert_eq!(stored.metadata.through_seq, Some(4));

    store::records::set_text(&store.pool, &uid, Some("Metadata changed"), None)
        .await
        .unwrap();
    let stale_base = metadata(&store, &uid).await;
    assert_eq!(stale_base.record_revision, 2);
    assert_eq!(stale_base.retained_generation, 1);
    assert_eq!(
        stale_base.qualification,
        Qualification::Qualified {
            generation: 1,
            base_revision: 1,
        }
    );
    let mut refresh = store::write_tx(&store.pool).await.unwrap();
    let refreshed = store::record_docs::put_qualified_on(&mut refresh, &uid, b"snapshot", 4, 1, 2)
        .await
        .unwrap();
    assert_eq!(
        refreshed.qualification,
        Qualification::Qualified {
            generation: 1,
            base_revision: 2,
        }
    );
    refresh.commit().await.unwrap();

    let too_large = vec![0; store::record_docs::MAX_SNAPSHOT_BYTES + 1];
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        store::record_docs::put_qualified_on(&mut tx, &uid, &too_large, 5, 1, 1)
            .await
            .unwrap_err()
            .to_string()
            .contains("byte limit")
    );
    assert!(
        store::record_docs::put_qualified_on(&mut tx, &uid, b"negative", -1, 1, 1)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn the_real_migration_keeps_old_snapshots_explicitly_unqualified() {
    let all = store::sqlx::migrate!("./migrations");
    let before_generations = store::sqlx::migrate::Migrator {
        migrations: Cow::Owned(
            all.iter()
                .filter(|migration| migration.version <= 80)
                .cloned()
                .collect(),
        ),
        ..store::sqlx::migrate::Migrator::DEFAULT
    };
    let options = store::sqlx::sqlite::SqliteConnectOptions::from_str("sqlite::memory:")
        .unwrap()
        .foreign_keys(true);
    let pool = store::sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    before_generations.run(&pool).await.unwrap();
    let store = Store { pool };
    store::organs::ensure_local(&store.pool, "").await.unwrap();
    let uid = record(&store, "Before 0081").await;
    store::sqlx::query(
        "INSERT INTO record_doc (record_uid, snapshot, through_seq, updated_at)
         VALUES (?, ?, 9, ?)",
    )
    .bind(&uid)
    .bind(b"old snapshot".as_slice())
    .bind("2026-09-06T00:00:00Z")
    .execute(&store.pool)
    .await
    .unwrap();

    all.run(&store.pool).await.unwrap();

    let stored = snapshot(&store, &uid).await;
    assert_eq!(stored.metadata.retained_generation, 1);
    assert_eq!(
        stored.metadata.qualification,
        Qualification::Unqualified {
            generation: None,
            base_revision: None,
        }
    );
    assert_eq!(stored.snapshot.as_deref(), Some(b"old snapshot".as_slice()));
}

#[tokio::test]
async fn reset_and_recreation_never_revive_an_old_generation() {
    let store = Store::open_memory().await.unwrap();
    let uid = record(&store, "ABA").await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::record_docs::put_qualified_on(&mut tx, &uid, b"first", 1, 1, 1)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert_eq!(
        store::record_docs::reset_on(&mut tx, &uid, 1)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        store::record_docs::metadata_on(&mut tx, &uid)
            .await
            .unwrap()
            .qualification,
        Qualification::Absent
    );
    assert!(
        store::record_docs::put_qualified_on(&mut tx, &uid, b"stale", 2, 1, 1)
            .await
            .unwrap_err()
            .to_string()
            .contains("generation conflict")
    );
    store::record_docs::put_qualified_on(&mut tx, &uid, b"second", 2, 2, 1)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let mut rolled_back = store::write_tx(&store.pool).await.unwrap();
    assert_eq!(
        store::record_docs::reset_on(&mut rolled_back, &uid, 2)
            .await
            .unwrap(),
        3
    );
    assert_eq!(
        store::record_docs::metadata_on(&mut rolled_back, &uid)
            .await
            .unwrap()
            .qualification,
        Qualification::Absent
    );
    rolled_back.rollback().await.unwrap();

    let stored = snapshot(&store, &uid).await;
    assert_eq!(stored.metadata.retained_generation, 2);
    assert_eq!(stored.snapshot.as_deref(), Some(b"second".as_slice()));
    assert_eq!(
        stored.metadata.qualification,
        Qualification::Qualified {
            generation: 2,
            base_revision: 1,
        }
    );
}

#[tokio::test]
async fn text_revision_and_snapshot_changes_roll_back_together() {
    let store = Store::open_memory().await.unwrap();
    let uid = record(&store, "Before").await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::sqlx::query("UPDATE record SET head = 'After' WHERE uid = ?")
        .bind(&uid)
        .execute(&mut *tx)
        .await
        .unwrap();
    let pending = store::record_revisions::get_on(&mut tx, &uid)
        .await
        .unwrap()
        .revision;
    assert_eq!(pending, 2);
    store::record_docs::put_qualified_on(&mut tx, &uid, b"pending", 8, 1, pending)
        .await
        .unwrap();
    assert_eq!(
        store::record_docs::snapshot_on(&mut tx, &uid)
            .await
            .unwrap()
            .snapshot
            .as_deref(),
        Some(b"pending".as_slice())
    );
    tx.rollback().await.unwrap();

    assert_eq!(
        store::records::get(&store.pool, &uid)
            .await
            .unwrap()
            .unwrap()
            .head,
        "Before"
    );
    let after = metadata(&store, &uid).await;
    assert_eq!(after.record_revision, 1);
    assert_eq!(after.retained_generation, 1);
    assert_eq!(after.qualification, Qualification::Absent);
}

#[tokio::test]
async fn missing_deleted_and_invalid_document_targets_refuse() {
    let store = Store::open_memory().await.unwrap();
    let deleted = record(&store, "Deleted").await;
    store::records::mark_deleted(&store.pool, &deleted)
        .await
        .unwrap();
    let missing = nucleus::new_uid("r");
    let mut tx = store::write_tx(&store.pool).await.unwrap();

    for uid in ["invalid", missing.as_str(), deleted.as_str()] {
        assert!(store::record_docs::metadata_on(&mut tx, uid).await.is_err());
        assert!(
            store::record_docs::put_qualified_on(&mut tx, uid, b"snapshot", 0, 1, 1)
                .await
                .is_err()
        );
        assert!(store::record_docs::reset_on(&mut tx, uid, 1).await.is_err());
    }
    tx.rollback().await.unwrap();
    assert!(
        store::record_docs::put(&store.pool, &missing, b"legacy", 0)
            .await
            .is_err()
    );
    assert!(
        store::record_docs::delete(&store.pool, &deleted)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn malformed_numeric_types_and_oversized_blobs_refuse_before_payload_reads() {
    let store = Store::open_memory().await.unwrap();
    let through = record(&store, "Through type").await;
    let generation = record(&store, "Generation type").await;
    let base = record(&store, "Base type").await;
    let future_base = record(&store, "Future base").await;
    let incomplete = record(&store, "Incomplete qualification").await;
    let snapshot_type = record(&store, "Snapshot type").await;
    let oversized = record(&store, "Oversized").await;
    let identity = record(&store, "Identity").await;
    let updated = record(&store, "Update time").await;
    let retained = record(&store, "Retained").await;
    let revision = record(&store, "Revision").await;
    for uid in [
        &through,
        &generation,
        &base,
        &future_base,
        &incomplete,
        &snapshot_type,
        &oversized,
        &identity,
        &updated,
    ] {
        let mut tx = store::write_tx(&store.pool).await.unwrap();
        store::record_docs::put_qualified_on(&mut tx, uid, b"valid", 1, 1, 1)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }
    store::sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("UPDATE record_doc SET through_seq = 1.5 WHERE record_uid = ?")
        .bind(&through)
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("UPDATE record_doc SET generation = 1.5 WHERE record_uid = ?")
        .bind(&generation)
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("UPDATE record_doc SET base_revision = zeroblob(8) WHERE record_uid = ?")
        .bind(&base)
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("UPDATE record_doc SET base_revision = 2 WHERE record_uid = ?")
        .bind(&future_base)
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("UPDATE record_doc SET base_revision = NULL WHERE record_uid = ?")
        .bind(&incomplete)
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("UPDATE record_doc SET snapshot = 'text' WHERE record_uid = ?")
        .bind(&snapshot_type)
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("UPDATE record_doc SET snapshot = zeroblob(?) WHERE record_uid = ?")
        .bind(i64::try_from(store::record_docs::MAX_SNAPSHOT_BYTES + 1).unwrap())
        .bind(&oversized)
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("UPDATE record_doc SET updated_at = zeroblob(65) WHERE record_uid = ?")
        .bind(&updated)
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("UPDATE record_doc SET record_uid = CAST(? AS BLOB) WHERE record_uid = ?")
        .bind(&identity)
        .bind(&identity)
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("UPDATE record_revision SET document_generation = -1 WHERE record_uid = ?")
        .bind(&retained)
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("UPDATE record_revision SET revision = -1 WHERE record_uid = ?")
        .bind(&revision)
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("PRAGMA ignore_check_constraints = OFF")
        .execute(&store.pool)
        .await
        .unwrap();

    let mut tx = store.pool.begin().await.unwrap();
    for uid in [
        &through,
        &generation,
        &base,
        &future_base,
        &incomplete,
        &snapshot_type,
        &identity,
        &updated,
        &retained,
        &revision,
    ] {
        assert!(store::record_docs::metadata_on(&mut tx, uid).await.is_err());
    }
    let error = store::record_docs::metadata_on(&mut tx, &oversized)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("snapshot exceeds its byte limit")
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn exhausted_generations_refuse_reset_and_legacy_invalidation() {
    let store = Store::open_memory().await.unwrap();
    let uid = record(&store, "Exhausted").await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::record_docs::put_qualified_on(&mut tx, &uid, b"retained", 1, 1, 1)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    store::sqlx::query("UPDATE record_revision SET document_generation = ? WHERE record_uid = ?")
        .bind(i64::MAX)
        .bind(&uid)
        .execute(&store.pool)
        .await
        .unwrap();

    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        store::record_docs::reset_on(&mut tx, &uid, i64::MAX)
            .await
            .unwrap_err()
            .to_string()
            .contains("exhausted")
    );
    tx.rollback().await.unwrap();
    assert!(
        store::record_docs::put(&store.pool, &uid, b"legacy", 2)
            .await
            .is_err()
    );
    assert!(store::record_docs::delete(&store.pool, &uid).await.is_err());
    assert_eq!(
        snapshot(&store, &uid).await.snapshot.as_deref(),
        Some(b"retained".as_slice())
    );
}

#[tokio::test]
async fn legacy_put_and_delete_advance_generation_and_remove_qualification() {
    let store = Store::open_memory().await.unwrap();
    let uid = record(&store, "Legacy").await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::record_docs::put_qualified_on(&mut tx, &uid, b"qualified", 1, 1, 1)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    store::record_docs::put(&store.pool, &uid, b"legacy", 2)
        .await
        .unwrap();
    let legacy = snapshot(&store, &uid).await;
    assert_eq!(legacy.metadata.retained_generation, 2);
    assert_eq!(
        legacy.metadata.qualification,
        Qualification::Unqualified {
            generation: None,
            base_revision: None,
        }
    );
    assert_eq!(legacy.snapshot.as_deref(), Some(b"legacy".as_slice()));

    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::record_docs::put_qualified_on(&mut tx, &uid, b"requalified", 3, 2, 1)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    store::record_docs::delete(&store.pool, &uid).await.unwrap();
    let deleted = metadata(&store, &uid).await;
    assert_eq!(deleted.retained_generation, 3);
    assert_eq!(deleted.qualification, Qualification::Absent);
}

#[tokio::test]
async fn separate_writers_serialize_and_a_stale_generation_refuses_after_commit() {
    let directory = std::env::temp_dir().join(nucleus::new_uid("record-doc-generation"));
    std::fs::create_dir_all(&directory).unwrap();
    let url = format!("sqlite://{}", directory.join("lince.db").display());
    let store = Store::open(&url).await.unwrap();
    let uid = record(&store, "Serialized").await;
    let mut first = store::write_tx(&store.pool).await.unwrap();
    assert_eq!(
        store::record_docs::reset_on(&mut first, &uid, 1)
            .await
            .unwrap(),
        2
    );
    let mut second = store.pool.acquire().await.unwrap();
    store::sqlx::query("PRAGMA busy_timeout = 0")
        .execute(&mut *second)
        .await
        .unwrap();
    let busy = store::sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *second)
        .await
        .unwrap_err();
    assert!(busy.to_string().contains("locked"));
    first.commit().await.unwrap();

    let mut stale = store::write_tx(&store.pool).await.unwrap();
    assert!(
        store::record_docs::reset_on(&mut stale, &uid, 1)
            .await
            .unwrap_err()
            .to_string()
            .contains("generation conflict")
    );
    stale.rollback().await.unwrap();
    assert_eq!(metadata(&store, &uid).await.retained_generation, 2);
    drop(second);
    store.pool.close().await;
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn qualified_generation_and_snapshot_survive_restart() {
    let directory = std::env::temp_dir().join(nucleus::new_uid("record-doc-restart"));
    std::fs::create_dir_all(&directory).unwrap();
    let url = format!("sqlite://{}", directory.join("lince.db").display());
    let store = Store::open(&url).await.unwrap();
    let uid = record(&store, "Restart").await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::record_docs::put_qualified_on(&mut tx, &uid, b"durable", 11, 1, 1)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    store.pool.close().await;

    let reopened = Store::open(&url).await.unwrap();
    let stored = snapshot(&reopened, &uid).await;
    assert_eq!(stored.snapshot.as_deref(), Some(b"durable".as_slice()));
    assert_eq!(
        stored.metadata.qualification,
        Qualification::Qualified {
            generation: 1,
            base_revision: 1,
        }
    );
    reopened.pool.close().await;
    std::fs::remove_dir_all(directory).unwrap();
}
