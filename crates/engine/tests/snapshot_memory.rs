use store::{Store, sqlx};

#[tokio::test]
async fn memory_snapshots_preserve_data_and_isolate_later_changes() {
    let source = Store::open_memory().await.unwrap();
    sqlx::query("CREATE TABLE snapshot_probe(value TEXT NOT NULL) STRICT")
        .execute(&source.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO snapshot_probe VALUES ('original')")
        .execute(&source.pool)
        .await
        .unwrap();
    let expected = source.state_hash().await.unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("copied ?#é.sqlite");
    source.snapshot_into(&path).await.unwrap();
    assert!(path.metadata().unwrap().len() > 0);
    let copied = Store::open_existing_durable(&format!(
        "sqlite://{}",
        path.to_string_lossy()
            .replace('%', "%25")
            .replace('?', "%3F")
            .replace('#', "%23")
    ))
    .await
    .unwrap();
    assert_eq!(copied.state_hash().await.unwrap(), expected);
    sqlx::query("UPDATE snapshot_probe SET value = 'changed'")
        .execute(&source.pool)
        .await
        .unwrap();
    let value: String = sqlx::query_scalar("SELECT value FROM snapshot_probe")
        .fetch_one(&copied.pool)
        .await
        .unwrap();
    assert_eq!(value, "original");
    assert!(source.snapshot_into(&path).await.is_err());
    assert_eq!(copied.state_hash().await.unwrap(), expected);
}
