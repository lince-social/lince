use nucleus::RecordKind;
use serde_json::json;
use store::Store;

async fn local_organ(store: &Store) -> String {
    store::organs::local(&store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid
}

async fn record(store: &Store, slug: Option<&str>, head: &str) -> String {
    store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug,
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

async fn op_count(store: &Store, uid: &str) -> i64 {
    store::sqlx::query_scalar("SELECT COUNT(*) FROM sync_op WHERE uid = ?")
        .bind(uid)
        .fetch_one(&store.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn preselected_creation_and_scalar_authoring_share_one_transaction() {
    let store = Store::open_memory().await.unwrap();
    let organ = local_organ(&store).await;
    let unit = store::concepts::ensure(&store.pool, "transaction-unit")
        .await
        .unwrap();
    let place = store::places::create(&store.pool, 1.0, 2.0, None)
        .await
        .unwrap();
    let uid = nucleus::new_uid("r");
    let mut tx = store::write_tx(&store.pool).await.unwrap();

    let created = store::records::create_with_uid_on(
        &mut tx,
        &uid,
        store::records::NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head: "Initial",
            body: "",
            quantity: store::exact::zero(),
        },
        &organ,
        Some(&uid),
    )
    .await
    .unwrap();
    assert_eq!(created.uid, uid);
    store::records::set_authoring_text_on(&mut tx, &uid, Some("Changed"), Some("Body"))
        .await
        .unwrap();
    store::records::set_slug_on(&mut tx, &uid, Some("transaction-record"))
        .await
        .unwrap();
    store::records::set_unit_on(&mut tx, &uid, Some(&unit))
        .await
        .unwrap();
    store::records::set_place_on(&mut tx, &uid, Some(&place))
        .await
        .unwrap();
    assert_eq!(
        store::records::set_extension_on(
            &mut tx,
            &uid,
            "work.tracking",
            &json!({"estimate": 3, "owner": "ana"}),
        )
        .await
        .unwrap(),
        Some(1)
    );

    let row = store::sqlx::query_as::<_, (String, String, String, String, String, String)>(
        "SELECT head, body, slug, unit_uid, place_uid, replica_root
           FROM record WHERE uid = ?",
    )
    .bind(&uid)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        row,
        (
            "Changed".into(),
            "Body".into(),
            "transaction-record".into(),
            unit,
            place,
            uid.clone(),
        )
    );
    let fields = store::sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT tbl, field, actor_cell, organ_uid
           FROM sync_op WHERE uid = ? ORDER BY seq",
    )
    .bind(&uid)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert!(
        fields
            .iter()
            .any(|row| row.0 == "record" && row.1 == "head")
    );
    assert!(
        fields
            .iter()
            .any(|row| row.0 == "record" && row.1 == "body")
    );
    assert!(
        fields
            .iter()
            .any(|row| row.0 == "record" && row.1 == "slug")
    );
    assert!(
        fields
            .iter()
            .any(|row| row.0 == "record" && row.1 == "unit_uid")
    );
    assert!(
        fields
            .iter()
            .any(|row| row.0 == "record" && row.1 == "place_uid")
    );
    assert!(
        fields
            .iter()
            .any(|row| { row.0 == "record_extension" && row.1 == "work.tracking.estimate" })
    );
    assert!(fields.iter().all(|row| !row.2.is_empty() && row.3 == organ));
    tx.commit().await.unwrap();

    assert!(
        store::records::get(&store.pool, &uid)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn rollback_removes_scalar_lifecycle_extension_and_sync_changes() {
    let store = Store::open_memory().await.unwrap();
    let uid = record(&store, None, "Before").await;
    let before_ops = op_count(&store, &uid).await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();

    store::records::set_authoring_text_on(&mut tx, &uid, Some("After"), None)
        .await
        .unwrap();
    store::records::set_slug_on(&mut tx, &uid, Some("rolled-back"))
        .await
        .unwrap();
    store::records::set_extension_on(&mut tx, &uid, "test.rollback", &json!({"value": 1}))
        .await
        .unwrap();
    store::records::mark_deleted_on(&mut tx, &uid)
        .await
        .unwrap();
    assert!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT deleted_at IS NOT NULL FROM record WHERE uid = ?",
        )
        .bind(&uid)
        .fetch_one(&mut *tx)
        .await
        .unwrap()
            != 0
    );
    assert!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sync_op WHERE uid = ?")
            .bind(&uid)
            .fetch_one(&mut *tx)
            .await
            .unwrap()
            > before_ops
    );
    tx.rollback().await.unwrap();

    let row = store::records::get(&store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.head, "Before");
    assert_eq!(row.slug, None);
    assert_eq!(
        store::records::get_extension(&store.pool, &uid, "test.rollback")
            .await
            .unwrap(),
        None
    );
    assert_eq!(op_count(&store, &uid).await, before_ops);
}

#[tokio::test]
async fn fact_quantity_updates_still_follow_a_soft_delete_in_one_transaction() {
    let store = Store::open_memory().await.unwrap();
    let uid = record(&store, None, "Deleted Fact target").await;
    let before = store::records::quantity(&store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();

    store::records::mark_deleted_on(&mut tx, &uid)
        .await
        .unwrap();
    store::records::bump_quantity(&mut tx, &uid, store::exact::zero(), "2026-09-06T00:00:00Z")
        .await
        .unwrap();
    assert_eq!(
        store::sqlx::query_scalar::<_, String>(
            "SELECT quantity_mantissa FROM record WHERE uid = ? AND deleted_at IS NOT NULL",
        )
        .bind(&uid)
        .fetch_one(&mut *tx)
        .await
        .unwrap(),
        before.mantissa().to_string()
    );
    tx.rollback().await.unwrap();

    assert!(
        store::records::get(&store.pool, &uid)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn creation_and_its_sync_genesis_disappear_on_rollback() {
    let store = Store::open_memory().await.unwrap();
    let organ = local_organ(&store).await;
    let uid = nucleus::new_uid("r");
    let mut tx = store::write_tx(&store.pool).await.unwrap();

    store::records::create_with_uid_on(
        &mut tx,
        &uid,
        store::records::NewRecord {
            slug: Some("rolled-back-create"),
            kind: RecordKind::Plain,
            head: "Pending",
            body: "",
            quantity: store::exact::zero(),
        },
        &organ,
        None,
    )
    .await
    .unwrap();
    store::records::set_extension_on(&mut tx, &uid, "test.pending", &json!({"x": 1}))
        .await
        .unwrap();
    tx.rollback().await.unwrap();

    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM record WHERE uid = ?")
            .bind(&uid)
            .fetch_one(&store.pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(op_count(&store, &uid).await, 0);
}

#[tokio::test]
async fn a_second_connection_never_observes_partial_creation() {
    let directory = std::env::temp_dir().join(nucleus::new_uid("record-transaction"));
    std::fs::create_dir_all(&directory).unwrap();
    let url = format!("sqlite://{}", directory.join("lince.db").display());
    let store = Store::open(&url).await.unwrap();
    let organ = local_organ(&store).await;
    let uid = nucleus::new_uid("r");
    let mut observer = store.pool.acquire().await.unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();

    store::records::create_with_uid_on(
        &mut tx,
        &uid,
        store::records::NewRecord {
            slug: Some("atomic-create"),
            kind: RecordKind::Plain,
            head: "Atomic",
            body: "Body",
            quantity: store::exact::zero(),
        },
        &organ,
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM record WHERE uid = ?")
            .bind(&uid)
            .fetch_one(&mut *observer)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sync_op WHERE uid = ?")
            .bind(&uid)
            .fetch_one(&mut *observer)
            .await
            .unwrap(),
        0
    );
    tx.commit().await.unwrap();
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM record WHERE uid = ?")
            .bind(&uid)
            .fetch_one(&mut *observer)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sync_op WHERE uid = ?")
            .bind(&uid)
            .fetch_one(&mut *observer)
            .await
            .unwrap(),
        6
    );
    drop(observer);
    store.pool.close().await;
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn missing_deleted_and_invalid_authoring_targets_refuse() {
    let store = Store::open_memory().await.unwrap();
    let organ = local_organ(&store).await;
    let target = record(&store, None, "Target").await;
    let plain_origin = record(&store, None, "Not Organ").await;
    let missing = nucleus::new_uid("r");
    let missing_root = nucleus::new_uid("r");
    let invalid_uid = "not-a-record";
    let mut tx = store::write_tx(&store.pool).await.unwrap();

    for (uid, origin, root) in [
        (invalid_uid, organ.as_str(), None),
        (target.as_str(), organ.as_str(), None),
        (missing.as_str(), plain_origin.as_str(), None),
        (
            missing.as_str(),
            organ.as_str(),
            Some(missing_root.as_str()),
        ),
    ] {
        assert!(
            store::records::create_with_uid_on(
                &mut tx,
                uid,
                store::records::NewRecord {
                    slug: None,
                    kind: RecordKind::Plain,
                    head: "Invalid",
                    body: "",
                    quantity: store::exact::zero(),
                },
                origin,
                root,
            )
            .await
            .is_err()
        );
    }
    assert!(
        store::records::set_authoring_text_on(&mut tx, &missing, Some("x"), None)
            .await
            .is_err()
    );
    assert!(
        store::records::set_slug_on(&mut tx, invalid_uid, None)
            .await
            .is_err()
    );
    assert!(
        store::records::set_unit_on(&mut tx, &target, Some(&missing))
            .await
            .is_err()
    );
    assert!(
        store::records::set_place_on(&mut tx, &target, Some(&missing))
            .await
            .is_err()
    );
    store::records::mark_deleted_on(&mut tx, &target)
        .await
        .unwrap();
    assert!(
        store::records::create_with_uid_on(
            &mut tx,
            &target,
            store::records::NewRecord {
                slug: None,
                kind: RecordKind::Plain,
                head: "Replacement",
                body: "",
                quantity: store::exact::zero(),
            },
            &organ,
            None,
        )
        .await
        .is_err()
    );
    assert!(
        store::records::set_extension_on(&mut tx, &target, "test", &json!({"x": 1}))
            .await
            .is_err()
    );
    assert!(
        store::records::mark_deleted_on(&mut tx, &target)
            .await
            .is_err()
    );
    assert!(
        store::records::restore_on(&mut tx, &missing, None)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn pool_deletion_preserves_false_for_missing_and_already_deleted_records() {
    let store = Store::open_memory().await.unwrap();
    let uid = record(&store, None, "Delete once").await;
    let missing = nucleus::new_uid("r");

    assert!(
        store::records::mark_deleted(&store.pool, &uid)
            .await
            .unwrap()
    );
    assert!(
        !store::records::mark_deleted(&store.pool, &uid)
            .await
            .unwrap()
    );
    assert!(
        !store::records::mark_deleted(&store.pool, &missing)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn creation_refuses_deleted_and_nested_existing_roots_without_partial_rows_or_sync() {
    let store = Store::open_memory().await.unwrap();
    let organ = local_organ(&store).await;
    let root = record(&store, None, "Root").await;
    let nested = store::records::create_in_root(
        &store.pool,
        store::records::NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head: "Nested",
            body: "",
            quantity: store::exact::zero(),
        },
        Some(&root),
    )
    .await
    .unwrap()
    .uid;
    let deleted = record(&store, None, "Deleted root").await;
    assert!(
        store::records::mark_deleted(&store.pool, &deleted)
            .await
            .unwrap()
    );
    let nested_target = nucleus::new_uid("r");
    let deleted_target = nucleus::new_uid("r");
    let mut tx = store::write_tx(&store.pool).await.unwrap();

    for (uid, invalid_root) in [(&nested_target, &nested), (&deleted_target, &deleted)] {
        assert!(
            store::records::create_with_uid_on(
                &mut tx,
                uid,
                store::records::NewRecord {
                    slug: None,
                    kind: RecordKind::Plain,
                    head: "Refused",
                    body: "",
                    quantity: store::exact::zero(),
                },
                &organ,
                Some(invalid_root),
            )
            .await
            .is_err()
        );
        assert_eq!(
            store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM record WHERE uid = ?")
                .bind(uid)
                .fetch_one(&mut *tx)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sync_op WHERE uid = ?")
                .bind(uid)
                .fetch_one(&mut *tx)
                .await
                .unwrap(),
            0
        );
    }
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn extensions_diff_keys_keep_server_versions_and_normalize_new_empty_objects() {
    let store = Store::open_memory().await.unwrap();
    let uid = record(&store, None, "Extensions").await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();

    assert_eq!(
        store::records::set_extension_on(&mut tx, &uid, "work.state", &json!({}))
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM record_extension WHERE record_uid = ?",
        )
        .bind(&uid)
        .fetch_one(&mut *tx)
        .await
        .unwrap(),
        0
    );
    store::sqlx::query(
        "INSERT INTO record_extension (record_uid, namespace, version, fds)
         VALUES (?, 'legacy.empty', 7, '{}')",
    )
    .bind(&uid)
    .execute(&mut *tx)
    .await
    .unwrap();
    let before_legacy_empty = store::sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sync_op
          WHERE uid = ? AND tbl = 'record_extension'",
    )
    .bind(&uid)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        store::records::set_extension_on(&mut tx, &uid, "legacy.empty", &json!({}))
            .await
            .unwrap(),
        Some(7)
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT version FROM record_extension
              WHERE record_uid = ? AND namespace = 'legacy.empty'",
        )
        .bind(&uid)
        .fetch_one(&mut *tx)
        .await
        .unwrap(),
        7
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sync_op
              WHERE uid = ? AND tbl = 'record_extension'",
        )
        .bind(&uid)
        .fetch_one(&mut *tx)
        .await
        .unwrap(),
        before_legacy_empty
    );
    assert_eq!(
        store::records::set_extension_on(
            &mut tx,
            &uid,
            "work.state",
            &json!({"estimate": 3, "owner": "ana"}),
        )
        .await
        .unwrap(),
        Some(1)
    );
    assert_eq!(
        store::records::set_extension_on(
            &mut tx,
            &uid,
            "work.state",
            &json!({"estimate": 5, "status": "ready"}),
        )
        .await
        .unwrap(),
        Some(2)
    );
    let before_noop = store::sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sync_op
          WHERE uid = ? AND tbl = 'record_extension'",
    )
    .bind(&uid)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        store::records::set_extension_on(
            &mut tx,
            &uid,
            "work.state",
            &json!({"estimate": 5, "status": "ready"}),
        )
        .await
        .unwrap(),
        Some(2)
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sync_op
              WHERE uid = ? AND tbl = 'record_extension'",
        )
        .bind(&uid)
        .fetch_one(&mut *tx)
        .await
        .unwrap(),
        before_noop
    );
    assert_eq!(
        store::records::set_extension_on(&mut tx, &uid, "work.state", &json!({}))
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM record_extension
              WHERE record_uid = ? AND namespace = 'work.state'",
        )
        .bind(&uid)
        .fetch_one(&mut *tx)
        .await
        .unwrap(),
        0
    );
    store::records::set_extension_on(&mut tx, &uid, "work.remove", &json!({"x": 1}))
        .await
        .unwrap();
    assert!(
        store::records::delete_extension_on(&mut tx, &uid, "work.remove")
            .await
            .unwrap()
    );
    assert!(
        !store::records::delete_extension_on(&mut tx, &uid, "work.remove")
            .await
            .unwrap()
    );
    let operations = store::sqlx::query_as::<_, (String, String, Option<String>)>(
        "SELECT field, kind, value FROM sync_op
          WHERE uid = ? AND tbl = 'record_extension' ORDER BY seq",
    )
    .bind(&uid)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert!(operations.contains(&("work.state.owner".into(), "tombstone".into(), None,)));
    assert!(operations.contains(&(
        "work.state.status".into(),
        "set".into(),
        Some("\"ready\"".into()),
    )));
    assert_eq!(
        operations
            .iter()
            .filter(|row| row.0 == "work.state.estimate" && row.1 == "set")
            .count(),
        2
    );
    assert_eq!(
        operations.iter().filter(|row| row.1 == "tombstone").count(),
        4
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn corrupt_oversized_and_overflowing_extensions_refuse_without_replacement() {
    let store = Store::open_memory().await.unwrap();
    let uid = record(&store, None, "Corruption").await;
    for namespace in ["malformed", "nonobject", "oversized", "version", "overflow"] {
        store::records::set_extension(&store.pool, &uid, namespace, &json!({"old": true}))
            .await
            .unwrap();
    }
    store::sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query(
        "UPDATE record_extension SET fds = '{' WHERE record_uid = ? AND namespace = 'malformed'",
    )
    .bind(&uid)
    .execute(&store.pool)
    .await
    .unwrap();
    store::sqlx::query(
        "UPDATE record_extension SET fds = '[]' WHERE record_uid = ? AND namespace = 'nonobject'",
    )
    .bind(&uid)
    .execute(&store.pool)
    .await
    .unwrap();
    store::sqlx::query(
        "UPDATE record_extension SET fds = zeroblob(?)
          WHERE record_uid = ? AND namespace = 'oversized'",
    )
    .bind(i64::try_from(store::records::MAX_EXTENSION_BYTES + 1).unwrap())
    .bind(&uid)
    .execute(&store.pool)
    .await
    .unwrap();
    store::sqlx::query(
        "UPDATE record_extension SET version = 1.5
          WHERE record_uid = ? AND namespace = 'version'",
    )
    .bind(&uid)
    .execute(&store.pool)
    .await
    .unwrap();
    store::sqlx::query(
        "UPDATE record_extension SET version = ?
          WHERE record_uid = ? AND namespace = 'overflow'",
    )
    .bind(i64::MAX)
    .bind(&uid)
    .execute(&store.pool)
    .await
    .unwrap();
    store::sqlx::query("PRAGMA ignore_check_constraints = OFF")
        .execute(&store.pool)
        .await
        .unwrap();
    let oversized = json!({"value": "x".repeat(store::records::MAX_EXTENSION_BYTES)});
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        store::records::set_extension_on(&mut tx, &uid, "new-oversized", &oversized)
            .await
            .is_err()
    );
    for namespace in ["malformed", "nonobject", "oversized", "version", "overflow"] {
        let error = store::records::set_extension_on(
            &mut tx,
            &uid,
            namespace,
            &json!({"replacement": true}),
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("extension") || error.to_string().contains("JSON"));
    }
    assert_eq!(
        store::sqlx::query_scalar::<_, String>(
            "SELECT fds FROM record_extension
              WHERE record_uid = ? AND namespace = 'overflow'",
        )
        .bind(&uid)
        .fetch_one(&mut *tx)
        .await
        .unwrap(),
        "{\"old\":true}"
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn restore_keeps_identity_and_never_steals_a_reused_slug() {
    let store = Store::open_memory().await.unwrap();
    let original = record(&store, Some("reused-slug"), "Original").await;
    store::records::mark_deleted(&store.pool, &original)
        .await
        .unwrap();
    let replacement = record(&store, Some("reused-slug"), "Replacement").await;

    assert!(
        store::records::restore(&store.pool, &original, Some("reused-slug"))
            .await
            .is_err()
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT deleted_at IS NOT NULL FROM record WHERE uid = ?",
        )
        .bind(&original)
        .fetch_one(&store.pool)
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, String>("SELECT uid FROM record WHERE slug = ?")
            .bind("reused-slug")
            .fetch_one(&store.pool)
            .await
            .unwrap(),
        replacement
    );

    store::records::restore(&store.pool, &original, None)
        .await
        .unwrap();
    let restored = store::records::get(&store.pool, &original)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(restored.uid, original);
    assert_eq!(restored.slug, None);
    assert!(
        store::records::restore(&store.pool, &original, None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn legacy_collaborative_text_stays_unlogged_and_authoring_text_logs() {
    let store = Store::open_memory().await.unwrap();
    let uid = record(&store, None, "Original").await;
    let before = store::sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sync_op WHERE uid = ? AND tbl = 'record' AND field = 'head'",
    )
    .bind(&uid)
    .fetch_one(&store.pool)
    .await
    .unwrap();

    store::records::set_text(&store.pool, &uid, Some("Collaborative"), None)
        .await
        .unwrap();
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sync_op WHERE uid = ? AND tbl = 'record' AND field = 'head'",
        )
        .bind(&uid)
        .fetch_one(&store.pool)
        .await
        .unwrap(),
        before
    );

    store::records::set_authoring_text(&store.pool, &uid, Some("Authored"), None)
        .await
        .unwrap();
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sync_op WHERE uid = ? AND tbl = 'record' AND field = 'head'",
        )
        .bind(&uid)
        .fetch_one(&store.pool)
        .await
        .unwrap(),
        before + 1
    );
}
