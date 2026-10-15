#[tokio::test]
async fn existing_database_reads_are_consistent_and_cannot_write() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("lince.db");
    let writer = store::Store::open(&format!("sqlite://{}", path.display()))
        .await
        .unwrap();
    let reader = store::Store::open_readonly(&path).await.unwrap();
    let before: i64 = store::sqlx::query_scalar("SELECT value FROM commit_sequence WHERE id = 1")
        .fetch_one(&reader.pool)
        .await
        .unwrap();
    let mut transaction = store::write_tx(&writer.pool).await.unwrap();
    store::sqlx::query("UPDATE commit_sequence SET value = value + 1 WHERE id = 1")
        .execute(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    let after: i64 = store::sqlx::query_scalar("SELECT value FROM commit_sequence WHERE id = 1")
        .fetch_one(&reader.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    assert!(
        store::sqlx::query("DELETE FROM record")
            .execute(&reader.pool)
            .await
            .is_err()
    );
    reader.pool.close().await;
    writer.pool.close().await;
}

#[tokio::test]
async fn absent_or_unrelated_database_is_not_created_or_initialized() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("lince.db");
    assert!(store::Store::open_readonly(&path).await.is_err());
    assert!(!path.exists());
    std::fs::write(&path, b"this is not sqlite").unwrap();
    assert!(store::Store::open_readonly(&path).await.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"this is not sqlite");
}

#[tokio::test]
async fn incompatible_database_is_rejected_without_running_migrations() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("lince.db");
    let writer = store::Store::open(&format!("sqlite://{}", path.display()))
        .await
        .unwrap();
    store::sqlx::query(
        "DELETE FROM _sqlx_migrations WHERE version = (SELECT MAX(version) FROM _sqlx_migrations)",
    )
    .execute(&writer.pool)
    .await
    .unwrap();
    writer.pool.close().await;
    let before = std::fs::read(&path).unwrap();
    let error = store::Store::open_readonly(&path).await.err().unwrap();
    assert!(error.to_string().contains("current, valid Lince database"));
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[tokio::test]
async fn selected_extensions_exclude_unselected_deleted_and_other_namespaces() {
    let store = store::Store::open_memory().await.unwrap();
    let mut records = Vec::new();
    for slug in ["selected", "unselected", "deleted"] {
        let record = store::records::create(
            &store.pool,
            store::records::NewRecord {
                slug: Some(slug),
                kind: nucleus::RecordKind::Plain,
                head: slug,
                body: "",
                quantity: store::exact::zero(),
            },
        )
        .await
        .unwrap();
        store::records::set_extension(
            &store.pool,
            &record.uid,
            "sample",
            &serde_json::json!({"value": slug}),
        )
        .await
        .unwrap();
        records.push(record.uid);
    }
    store::records::mark_deleted(&store.pool, &records[2])
        .await
        .unwrap();
    let selected = store::records::extensions_for_records(
        &store.pool,
        "sample",
        &[records[0].clone(), records[2].clone()],
    )
    .await
    .unwrap();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[&records[0]]["value"], "selected");
    assert!(
        store::records::extensions_for_records(&store.pool, "other", &records)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        store::records::extensions_for_records(&store.pool, "sample", &[])
            .await
            .unwrap()
            .is_empty()
    );
}
