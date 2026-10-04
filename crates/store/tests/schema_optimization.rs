use store::Store;

#[tokio::test]
async fn fresh_and_reopened_schema_has_no_superseded_storage() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cell.sqlite");
    let url = format!("sqlite://{}", path.display());
    for _ in 0..2 {
        let store = Store::open_durable(&url).await.unwrap();
        let obsolete: i64 = store::sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_schema WHERE name IN ('frequency', 'frequency_revision', 'anicca_rule_firing', 'record_move')",
        )
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_eq!(obsolete, 0);
        for table in ["recurrence", "recurrence_revision"] {
            let legacy_column: i64 = store::sqlx::query_scalar(
                "SELECT COUNT(*) FROM pragma_table_info(?) WHERE name = 'frequency_uid'",
            )
            .bind(table)
            .fetch_one(&store.pool)
            .await
            .unwrap();
            assert_eq!(legacy_column, 0);
        }
        assert!(
            store::sqlx::query("PRAGMA foreign_key_check")
                .fetch_all(&store.pool)
                .await
                .unwrap()
                .is_empty()
        );
        let integrity: String = store::sqlx::query_scalar("PRAGMA integrity_check")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert_eq!(integrity, "ok");
        store.pool.close().await;
    }
    assert!(
        store::record_move::offers::TABLES
            .iter()
            .all(|(table, _)| *table != "frequency")
    );
}

#[tokio::test]
async fn concept_lookups_keep_diamond_descendants_and_alias_ambiguity() {
    let store = Store::open_memory().await.unwrap();
    let root = store::concepts::create(&store.pool, "root", &[])
        .await
        .unwrap();
    let left = store::concepts::create(&store.pool, "left", &[&root])
        .await
        .unwrap();
    let right = store::concepts::create(&store.pool, "right", &[&root])
        .await
        .unwrap();
    let leaf = store::concepts::create(&store.pool, "leaf", &[&left, &right])
        .await
        .unwrap();
    let mut descendants = store::concepts::descendants_including(&store.pool, &root)
        .await
        .unwrap();
    descendants.sort();
    let mut expected = vec![root, left.clone(), right.clone(), leaf];
    expected.sort();
    assert_eq!(descendants, expected);
    for language in ["en", "pt"] {
        store::concepts::add_name(&store.pool, &left, language, "alias")
            .await
            .unwrap();
    }
    assert_eq!(
        store::concepts::resolve(&store.pool, "alias")
            .await
            .unwrap(),
        Some(left.clone())
    );
    assert!(
        store::concepts::add_name(&store.pool, &left, "en", "alias")
            .await
            .is_err()
    );
    store::concepts::add_name(&store.pool, &right, "en", "alias")
        .await
        .unwrap();
    assert!(
        store::concepts::resolve(&store.pool, "alias")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn namespace_lookup_excludes_deleted_records_and_other_extensions() {
    let store = Store::open_memory().await.unwrap();
    let mut records = Vec::new();
    for name in ["visible", "deleted", "other"] {
        let record = store::records::create(
            &store.pool,
            store::records::NewRecord {
                slug: Some(name),
                kind: nucleus::RecordKind::Plain,
                head: name,
                body: "",
                quantity: nucleus::DecimalValue::parse_inferred("0").unwrap(),
            },
        )
        .await
        .unwrap();
        records.push(record.uid);
    }
    for uid in &records[..2] {
        store::records::set_extension(&store.pool, uid, "target", &serde_json::json!({"x": 1}))
            .await
            .unwrap();
    }
    store::records::set_extension(
        &store.pool,
        &records[2],
        "other",
        &serde_json::json!({"x": 2}),
    )
    .await
    .unwrap();
    store::sqlx::query("UPDATE record SET deleted_at = '2026-10-03T00:00:00Z' WHERE uid = ?")
        .bind(&records[1])
        .execute(&store.pool)
        .await
        .unwrap();
    let extensions = store::records::all_extensions(&store.pool, "target")
        .await
        .unwrap();
    assert_eq!(extensions.len(), 1);
    assert_eq!(extensions[&records[0]], serde_json::json!({"x": 1}));
}
