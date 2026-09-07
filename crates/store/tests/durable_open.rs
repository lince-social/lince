use std::str::FromStr;
use store::Store;
use store::sqlx::{Connection, SqliteConnection};

fn temp_store(name: &str) -> (std::path::PathBuf, String) {
    let directory = std::env::temp_dir().join(nucleus::new_uid(name));
    std::fs::create_dir_all(&directory).unwrap();
    let url = format!("sqlite://{}", directory.join("lince.db").display());
    (directory, url)
}

async fn assert_durable_settings(connection: &mut store::sqlx::SqliteConnection) {
    let foreign_keys: i64 = store::sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    let journal_mode: String = store::sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    let synchronous: i64 = store::sqlx::query_scalar("PRAGMA synchronous")
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    let busy_timeout: i64 = store::sqlx::query_scalar("PRAGMA busy_timeout")
        .fetch_one(&mut *connection)
        .await
        .unwrap();

    assert_eq!(foreign_keys, 1);
    assert_eq!(journal_mode, "wal");
    assert_eq!(synchronous, 2);
    assert_eq!(busy_timeout, 10_000);
}

async fn acquire_all(store: &Store) -> Vec<store::sqlx::pool::PoolConnection<store::sqlx::Sqlite>> {
    let mut connections = Vec::new();
    for _ in 0..4 {
        connections.push(store.pool.acquire().await.unwrap());
    }
    connections
}

async fn assert_pool_is_durable(store: &Store) {
    let mut initial = acquire_all(store).await;
    for connection in &mut initial {
        assert_durable_settings(connection).await;
    }
    for connection in initial {
        connection.close().await.unwrap();
    }

    let mut replacements = acquire_all(store).await;
    for connection in &mut replacements {
        assert_durable_settings(connection).await;
    }
    drop(replacements);
}

async fn record_snapshot(
    store: &Store,
) -> Vec<(String, Option<String>, String, String, Option<String>, i64)> {
    store::sqlx::query_as(
        "SELECT r.uid, r.slug, r.kind, r.updated_at, r.deleted_at, rr.revision
           FROM record r JOIN record_revision rr ON rr.record_uid = r.uid
          ORDER BY r.uid",
    )
    .fetch_all(&store.pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn every_initial_and_replacement_connection_is_durable_for_both_openers() {
    let (directory, url) = temp_store("durable-connections");
    let created = Store::open_durable(&url).await.unwrap();
    assert_pool_is_durable(&created).await;
    created.pool.close().await;

    let existing = Store::open_existing_durable(&url).await.unwrap();
    assert_pool_is_durable(&existing).await;
    existing.pool.close().await;

    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn durable_openers_refuse_in_memory_and_temporary_urls() {
    for url in [
        "sqlite::memory:",
        "sqlite://:memory:",
        "sqlite://?mode=memory",
        "sqlite://named?mode=memory",
        "sqlite://",
    ] {
        let error = Store::open_durable(url).await.unwrap_err();
        assert!(
            error
                .to_string()
                .contains("requires a file-backed SQLite database"),
            "{url}: {error}"
        );

        let error = Store::open_existing_durable(url).await.unwrap_err();
        assert!(
            error
                .to_string()
                .contains("requires a file-backed SQLite database"),
            "{url}: {error}"
        );
    }
}

#[tokio::test]
async fn a_new_durable_store_has_schema_without_domain_defaults() {
    let (directory, url) = temp_store("durable-empty");
    let store = Store::open_durable(&url).await.unwrap();
    let migrations: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    assert!(migrations > 0);
    for table in ["record", "lingua", "configuration", "role"] {
        let count: i64 = store::sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "{table}");
    }
    assert!(store::organs::local(&store.pool).await.unwrap().is_none());
    assert!(store::cells::local(&store.pool).await.unwrap().is_none());
    store.pool.close().await;

    let reopened = Store::open_existing_durable(&url).await.unwrap();
    assert!(record_snapshot(&reopened).await.is_empty());
    let local_lingua: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM lingua WHERE uid = ?")
        .bind(store::linguas::LOCAL_UID)
        .fetch_one(&reopened.pool)
        .await
        .unwrap();
    assert_eq!(local_lingua, 0);
    reopened.pool.close().await;
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn existing_only_refuses_a_missing_path_without_creating_it() {
    let (directory, url) = temp_store("durable-missing");
    let path = directory.join("lince.db");
    assert!(!path.exists());
    assert!(Store::open_existing_durable(&url).await.is_err());
    assert!(!path.exists());
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn existing_only_does_not_migrate_a_raw_sqlite_database() {
    let (directory, url) = temp_store("durable-unmigrated");
    let options = store::sqlx::sqlite::SqliteConnectOptions::from_str(&url)
        .unwrap()
        .create_if_missing(true);
    let raw = SqliteConnection::connect_with(&options).await.unwrap();
    raw.close().await.unwrap();

    let store = Store::open_existing_durable(&url).await.unwrap();
    let tables: i64 = store::sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_schema
          WHERE type = 'table' AND name IN ('_sqlx_migrations', 'record')",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(tables, 0);
    store.pool.close().await;
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn existing_only_preserves_a_valid_identity_and_rollback() {
    let (directory, url) = temp_store("durable-valid-identity");
    let store = Store::open_durable(&url).await.unwrap();
    let organ = store::organs::ensure_local(&store.pool, "https://company.test")
        .await
        .unwrap();
    store::linguas::ensure_local(&store.pool).await.unwrap();
    let before = record_snapshot(&store).await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::sqlx::query("UPDATE record SET head = ? WHERE uid = ?")
        .bind("Uncommitted identity")
        .bind(&organ.uid)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(record_snapshot(&store).await, before);
    store.pool.close().await;

    let reopened = Store::open_existing_durable(&url).await.unwrap();
    assert_eq!(record_snapshot(&reopened).await, before);
    assert_eq!(
        store::organs::local(&reopened.pool)
            .await
            .unwrap()
            .unwrap()
            .uid,
        organ.uid
    );

    reopened.pool.close().await;
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn existing_only_does_not_repair_deleted_or_wrong_kind_identity_rows() {
    let (directory, url) = temp_store("durable-damaged-identity");
    let store = Store::open_durable(&url).await.unwrap();
    let organ_uid = nucleus::new_uid("r");
    let cell_uid = nucleus::new_uid("r");
    store::sqlx::query(
        "INSERT INTO record
             (uid, slug, kind, head, body, quantity_mantissa, quantity_scale,
              organ_uid, created_at, updated_at)
         VALUES (?, ?, 'plain', 'wrong kind', '', '1', 0, ?, ?, ?)",
    )
    .bind(&organ_uid)
    .bind(store::organs::LOCAL_ORGAN_SLUG)
    .bind(&organ_uid)
    .bind("2026-09-07T00:00:00Z")
    .bind("2026-09-07T00:00:00Z")
    .execute(&store.pool)
    .await
    .unwrap();
    store::sqlx::query(
        "INSERT INTO record
             (uid, slug, kind, head, body, quantity_mantissa, quantity_scale,
              organ_uid, created_at, updated_at, deleted_at)
         VALUES (?, ?, 'device', 'deleted cell', '', '1', 0, ?, ?, ?, ?)",
    )
    .bind(&cell_uid)
    .bind(store::cells::LOCAL_CELL_SLUG)
    .bind(&organ_uid)
    .bind("2026-09-07T00:00:00Z")
    .bind("2026-09-07T00:00:00Z")
    .bind("2026-09-07T00:00:01Z")
    .execute(&store.pool)
    .await
    .unwrap();
    let before = record_snapshot(&store).await;
    store.pool.close().await;

    let reopened = Store::open_existing_durable(&url).await.unwrap();
    assert_eq!(record_snapshot(&reopened).await, before);
    assert!(
        store::organs::local(&reopened.pool)
            .await
            .unwrap()
            .is_none()
    );
    let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&reopened.pool)
        .await
        .unwrap();
    assert_eq!(count, 2);
    reopened.pool.close().await;
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn ordinary_open_keeps_its_existing_normal_profile() {
    let (directory, url) = temp_store("ordinary-open");
    let store = Store::open(&url).await.unwrap();
    let mut connection = store.pool.acquire().await.unwrap();
    let synchronous: i64 = store::sqlx::query_scalar("PRAGMA synchronous")
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    assert_eq!(synchronous, 1);
    drop(connection);
    store.pool.close().await;
    std::fs::remove_dir_all(directory).unwrap();

    let memory = Store::open_memory().await.unwrap();
    assert!(store::organs::local(&memory.pool).await.unwrap().is_some());
}
