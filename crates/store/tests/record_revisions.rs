use std::borrow::Cow;
use std::str::FromStr;

use nucleus::RecordKind;
use store::Store;

async fn record(store: &Store, kind: RecordKind, head: &str) -> String {
    store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: None,
            kind,
            head,
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid
}

async fn revision(store: &Store, uid: &str) -> i64 {
    let mut connection = store.pool.acquire().await.unwrap();
    store::record_revisions::get_on(&mut connection, uid)
        .await
        .unwrap()
        .revision
}

#[tokio::test]
async fn the_real_migration_backfills_existing_records_and_installs_every_trigger() {
    let all = store::sqlx::migrate!("./migrations");
    let before_revisions = store::sqlx::migrate::Migrator {
        migrations: Cow::Owned(
            all.iter()
                .filter(|migration| migration.version <= 78)
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
    before_revisions.run(&pool).await.unwrap();
    let store = Store { pool };
    store::organs::ensure_local(&store.pool, "").await.unwrap();
    let existing = record(&store, RecordKind::Plain, "Before 0079").await;

    all.run(&store.pool).await.unwrap();

    assert_eq!(revision(&store, &existing).await, 1);
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT strict FROM pragma_table_list WHERE name = 'record_revision'",
        )
        .fetch_one(&store.pool)
        .await
        .unwrap(),
        1
    );
    let triggers = store::sqlx::query_scalar::<_, String>(
        "SELECT name FROM sqlite_schema
          WHERE type = 'trigger' AND name LIKE 'record_revision_%'
          ORDER BY name",
    )
    .fetch_all(&store.pool)
    .await
    .unwrap();
    for expected in [
        "record_revision_assertion_delete",
        "record_revision_assertion_insert",
        "record_revision_assertion_update",
        "record_revision_extension_delete",
        "record_revision_extension_insert",
        "record_revision_extension_update",
        "record_revision_record_insert",
        "record_revision_record_uid_immutable",
        "record_revision_record_update",
    ] {
        assert!(triggers.iter().any(|name| name == expected), "{expected}");
    }
}

#[tokio::test]
async fn new_records_start_positive_and_every_record_update_advances() {
    let store = Store::open_memory().await.unwrap();
    let uid = record(&store, RecordKind::Plain, "New").await;

    assert_eq!(revision(&store, &uid).await, 1);
    store::records::set_text(&store.pool, &uid, Some("Changed"), None)
        .await
        .unwrap();
    assert_eq!(revision(&store, &uid).await, 2);

    let replacement = nucleus::new_uid("r");
    assert!(
        store::sqlx::query("UPDATE record SET uid = ? WHERE uid = ?")
            .bind(&replacement)
            .bind(&uid)
            .execute(&store.pool)
            .await
            .is_err()
    );
    assert_eq!(revision(&store, &uid).await, 2);
}

#[tokio::test]
async fn assertion_lifecycle_advances_old_and_new_subjects_only() {
    let store = Store::open_memory().await.unwrap();
    let first = record(&store, RecordKind::Plain, "First").await;
    let second = record(&store, RecordKind::Plain, "Second").await;
    let unrelated = record(&store, RecordKind::Plain, "Unrelated").await;
    let predicate = store::concepts::ensure(&store.pool, "revision-test")
        .await
        .unwrap();
    let assertion = store::assertions::assert(
        &store.pool,
        store::assertions::NewAssertion {
            subject_uid: &first,
            predicate_uid: &predicate,
            object_uid: None,
            role: store::assertions::AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
            asserted_by: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(revision(&store, &first).await, 2);

    store::sqlx::query("UPDATE record_assertion SET asserted_by = ? WHERE uid = ?")
        .bind(&first)
        .bind(&assertion)
        .execute(&store.pool)
        .await
        .unwrap();
    assert_eq!(revision(&store, &first).await, 3);

    store::sqlx::query("UPDATE record_assertion SET subject_uid = ? WHERE uid = ?")
        .bind(&second)
        .bind(&assertion)
        .execute(&store.pool)
        .await
        .unwrap();
    assert_eq!(revision(&store, &first).await, 4);
    assert_eq!(revision(&store, &second).await, 2);

    assert!(
        store::assertions::retract(&store.pool, &assertion, None)
            .await
            .unwrap()
    );
    assert_eq!(revision(&store, &second).await, 3);

    store::sqlx::query("DELETE FROM record_assertion WHERE uid = ?")
        .bind(&assertion)
        .execute(&store.pool)
        .await
        .unwrap();
    assert_eq!(revision(&store, &second).await, 4);
    assert_eq!(revision(&store, &unrelated).await, 1);
}

#[tokio::test]
async fn extension_lifecycle_advances_old_and_new_records_only() {
    let store = Store::open_memory().await.unwrap();
    let first = record(&store, RecordKind::Plain, "First").await;
    let second = record(&store, RecordKind::Plain, "Second").await;
    let unrelated = record(&store, RecordKind::Plain, "Unrelated").await;

    store::records::set_extension_raw(&store.pool, &first, "test.revision", &serde_json::json!({}))
        .await
        .unwrap();
    assert_eq!(revision(&store, &first).await, 2);

    store::records::set_extension_raw(
        &store.pool,
        &first,
        "test.revision",
        &serde_json::json!({"changed": true}),
    )
    .await
    .unwrap();
    assert_eq!(revision(&store, &first).await, 3);

    store::sqlx::query(
        "UPDATE record_extension SET record_uid = ? WHERE record_uid = ? AND namespace = ?",
    )
    .bind(&second)
    .bind(&first)
    .bind("test.revision")
    .execute(&store.pool)
    .await
    .unwrap();
    assert_eq!(revision(&store, &first).await, 4);
    assert_eq!(revision(&store, &second).await, 2);

    store::records::delete_extension(&store.pool, &second, "test.revision")
        .await
        .unwrap();
    assert_eq!(revision(&store, &second).await, 3);
    assert_eq!(revision(&store, &unrelated).await, 1);
}

#[tokio::test]
async fn revision_updates_and_checks_share_the_caller_transaction() {
    let store = Store::open_memory().await.unwrap();
    let uid = record(&store, RecordKind::Plain, "Rollback").await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();

    store::record_revisions::check_expected_on(&mut tx, &uid, 1)
        .await
        .unwrap();
    store::sqlx::query("UPDATE record SET body = ? WHERE uid = ?")
        .bind("pending")
        .bind(&uid)
        .execute(&mut *tx)
        .await
        .unwrap();
    store::record_revisions::check_expected_on(&mut tx, &uid, 2)
        .await
        .unwrap();
    assert!(
        store::record_revisions::check_expected_on(&mut tx, &uid, 1)
            .await
            .unwrap_err()
            .to_string()
            .contains("revision conflict")
    );
    tx.rollback().await.unwrap();

    assert_eq!(revision(&store, &uid).await, 1);
    assert_eq!(
        store::sqlx::query_scalar::<_, String>("SELECT body FROM record WHERE uid = ?")
            .bind(&uid)
            .fetch_one(&store.pool)
            .await
            .unwrap(),
        ""
    );
}

#[tokio::test]
async fn overflow_missing_rows_and_corrupt_revisions_refuse_without_writes() {
    let store = Store::open_memory().await.unwrap();
    let overflow = record(&store, RecordKind::Plain, "Overflow").await;
    store::sqlx::query("UPDATE record_revision SET revision = ? WHERE record_uid = ?")
        .bind(i64::MAX)
        .bind(&overflow)
        .execute(&store.pool)
        .await
        .unwrap();
    assert!(
        store::sqlx::query("UPDATE record SET head = 'lost' WHERE uid = ?")
            .bind(&overflow)
            .execute(&store.pool)
            .await
            .is_err()
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, String>("SELECT head FROM record WHERE uid = ?")
            .bind(&overflow)
            .fetch_one(&store.pool)
            .await
            .unwrap(),
        "Overflow"
    );

    let missing = record(&store, RecordKind::Plain, "Missing").await;
    store::sqlx::query("DELETE FROM record_revision WHERE record_uid = ?")
        .bind(&missing)
        .execute(&store.pool)
        .await
        .unwrap();
    assert!(
        store::sqlx::query("UPDATE record SET head = 'lost' WHERE uid = ?")
            .bind(&missing)
            .execute(&store.pool)
            .await
            .is_err()
    );
    {
        let mut connection = store.pool.acquire().await.unwrap();
        assert!(
            store::record_revisions::get_on(&mut connection, &missing)
                .await
                .unwrap_err()
                .to_string()
                .contains("missing its revision")
        );
    }

    let corrupt = record(&store, RecordKind::Plain, "Corrupt").await;
    store::sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("UPDATE record_revision SET revision = 0 WHERE record_uid = ?")
        .bind(&corrupt)
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("PRAGMA ignore_check_constraints = OFF")
        .execute(&store.pool)
        .await
        .unwrap();
    let mut connection = store.pool.acquire().await.unwrap();
    assert!(
        store::record_revisions::get_on(&mut connection, &corrupt)
            .await
            .is_err()
    );
    assert!(
        store::record_revisions::get_on(&mut connection, "not-a-record")
            .await
            .is_err()
    );
    assert!(
        store::record_revisions::check_expected_on(&mut connection, &overflow, 0)
            .await
            .is_err()
    );
}
