#[tokio::test]
async fn evidence_cache_tracks_every_committed_connection_and_schema_change() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cache.sqlite");
    let database = store::Store::open(&format!("sqlite://{}", path.display()))
        .await
        .unwrap();
    let hasher = store::snapshot::StateHasher::new(database.clone())
        .await
        .unwrap();
    let initial = hasher.hash().await.unwrap();
    assert_eq!(hasher.hash().await.unwrap(), initial);
    assert_eq!(database.state_hash().await.unwrap(), initial);
    let mut held = Vec::new();
    for _ in 0..4 {
        held.push(database.pool.acquire().await.unwrap());
    }
    assert_eq!(
        tokio::time::timeout(std::time::Duration::from_secs(2), hasher.hash())
            .await
            .unwrap()
            .unwrap(),
        initial
    );
    drop(held);
    store::sqlx::query("CREATE TABLE evidence_probe (value TEXT NOT NULL) STRICT")
        .execute(&database.pool)
        .await
        .unwrap();
    let schema = hasher.hash().await.unwrap();
    assert_ne!(schema, initial);
    assert_eq!(schema, database.state_hash().await.unwrap());
    let mut first = database.pool.acquire().await.unwrap();
    let mut second = database.pool.acquire().await.unwrap();
    for (connection, value) in [(&mut first, "first"), (&mut second, "second")] {
        store::sqlx::query("INSERT INTO evidence_probe VALUES (?)")
            .bind(value)
            .execute(&mut **connection)
            .await
            .unwrap();
        assert_eq!(
            hasher.hash().await.unwrap(),
            database.state_hash().await.unwrap()
        );
    }
    let committed = hasher.hash().await.unwrap();
    let mut transaction = database.pool.begin().await.unwrap();
    store::sqlx::query("DELETE FROM evidence_probe")
        .execute(&mut *transaction)
        .await
        .unwrap();
    assert_eq!(hasher.hash().await.unwrap(), committed);
    transaction.rollback().await.unwrap();
    assert_eq!(hasher.hash().await.unwrap(), committed);
    drop((first, second));
    database.pool.close().await;
    drop(hasher);
    let reopened = store::Store::open_existing_durable(&format!("sqlite://{}", path.display()))
        .await
        .unwrap();
    let hasher = store::snapshot::StateHasher::new(reopened.clone())
        .await
        .unwrap();
    assert_eq!(hasher.hash().await.unwrap(), committed);
    assert_eq!(
        hasher.hash().await.unwrap(),
        reopened.state_hash().await.unwrap()
    );
    reopened.pool.close().await;
}

#[tokio::test]
async fn memory_evidence_uses_the_same_hash_without_holding_its_only_connection() {
    let database = store::Store::open_memory().await.unwrap();
    let hasher = store::snapshot::StateHasher::new(database.clone())
        .await
        .unwrap();
    let before = hasher.hash().await.unwrap();
    store::sqlx::query("CREATE TABLE evidence_probe (value INTEGER)")
        .execute(&database.pool)
        .await
        .unwrap();
    assert_ne!(hasher.hash().await.unwrap(), before);
    assert_eq!(
        hasher.hash().await.unwrap(),
        database.state_hash().await.unwrap()
    );
    database.pool.close().await;
}

#[tokio::test]
async fn streamed_state_hash_matches_canonical_snapshots_for_all_sqlite_values() {
    let database = store::Store::open_memory().await.unwrap();
    async fn assert_reference(database: &store::Store) -> nucleus::karma::CanonicalHash {
        let reference = nucleus::karma::canonical_hash(
            "lince.store.state.v1",
            &database.logical_snapshot().await.unwrap(),
        )
        .unwrap();
        assert_eq!(database.state_hash().await.unwrap(), reference);
        reference
    }
    assert_reference(&database).await;
    store::sqlx::query("CREATE TABLE \"hash\"\"probe\" (a, b, c, d, e)")
        .execute(&database.pool)
        .await
        .unwrap();
    for integer in [i64::MIN, -1, 0, 1, i64::MAX, i64::MIN] {
        for real in [-0.0, 0.30000000000000004, -1.25, f64::MIN, f64::MAX] {
            store::sqlx::query("INSERT INTO \"hash\"\"probe\" VALUES (NULL, ?, ?, ?, ?)")
                .bind(integer)
                .bind(real)
                .bind("a\"b\\c\n\r\t\0 🦀 \u{2028}")
                .bind(vec![0_u8, 1, 127, 128, 255])
                .execute(&database.pool)
                .await
                .unwrap();
        }
    }
    let committed = assert_reference(&database).await;
    store::sqlx::raw_sql("CREATE TEMP TABLE reversed AS SELECT * FROM \"hash\"\"probe\" ORDER BY rowid DESC; DELETE FROM \"hash\"\"probe\"; INSERT INTO \"hash\"\"probe\" SELECT * FROM reversed; DROP TABLE reversed;")
        .execute(&database.pool)
        .await
        .unwrap();
    assert_eq!(assert_reference(&database).await, committed);
    store::sqlx::raw_sql("CREATE TABLE projection_hash_probe (value); INSERT INTO projection_hash_probe VALUES ('ignored'); CREATE TABLE _sqlx_hash_probe (value); INSERT INTO _sqlx_hash_probe VALUES ('ignored');")
        .execute(&database.pool)
        .await
        .unwrap();
    assert_eq!(assert_reference(&database).await, committed);
    store::sqlx::query("UPDATE \"hash\"\"probe\" SET d = '', e = x'' WHERE rowid = 1")
        .execute(&database.pool)
        .await
        .unwrap();
    assert_ne!(assert_reference(&database).await, committed);
    database.pool.close().await;
}
