// non-lean-yet
// Scope: fn url, fn file_snapshot_preserves_sparse_rowids_schema_and_independent_contents, fn
//   concurrent_wal_writes_leave_a_single_consistent_snapshot, fn
//   cancelling_a_queued_copy_releases_its_source_connection.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::indexing_slicing] Verify bounds or use checked access on fallible inputs.
//   Sites 85, 86.

use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
use store::Store;

fn url(path: &std::path::Path) -> String {
    format!(
        "sqlite://{}",
        path.to_string_lossy()
            .replace('%', "%25")
            .replace('?', "%3F")
            .replace('#', "%23")
    )
}

#[tokio::test]
async fn file_snapshot_preserves_sparse_rowids_schema_and_independent_contents() {
    let directory = tempfile::tempdir().unwrap();
    let source = Store::open(&url(&directory.path().join("source.sqlite")))
        .await
        .unwrap();
    sqlx::raw_sql("CREATE TABLE snapshot_probe(value TEXT NOT NULL) STRICT; INSERT INTO snapshot_probe(rowid, value) VALUES (-5, 'first'), (7, 'middle'), (99, 'last'); CREATE INDEX snapshot_probe_value ON snapshot_probe(value);")
        .execute(&source.pool).await.unwrap();
    let expected = source.state_hash().await.unwrap();
    let path = directory.path().join("copied ?#é.sqlite");
    source.snapshot_into(&path).await.unwrap();
    let copy = Store::open_existing_durable(&url(&path)).await.unwrap();
    assert_eq!(copy.state_hash().await.unwrap(), expected);
    let rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT rowid, value FROM snapshot_probe ORDER BY rowid")
            .fetch_all(&copy.pool)
            .await
            .unwrap();
    assert_eq!(
        rows,
        vec![
            (-5, "first".into()),
            (7, "middle".into()),
            (99, "last".into())
        ]
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("PRAGMA integrity_check")
            .fetch_one(&copy.pool)
            .await
            .unwrap(),
        "ok"
    );
    sqlx::query("UPDATE snapshot_probe SET value = 'changed'")
        .execute(&source.pool)
        .await
        .unwrap();
    assert_eq!(copy.state_hash().await.unwrap(), expected);
    assert!(source.snapshot_into(&path).await.is_err());
    assert_eq!(copy.state_hash().await.unwrap(), expected);
    let empty = directory.path().join("existing-empty.sqlite");
    std::fs::write(&empty, []).unwrap();
    assert!(source.snapshot_into(&empty).await.is_err());
    assert_eq!(empty.metadata().unwrap().len(), 0);
    copy.pool.close().await;
    source.pool.close().await;
}

#[tokio::test]
async fn concurrent_wal_writes_leave_a_single_consistent_snapshot() {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        let directory = tempfile::tempdir().unwrap();
        let source = Store::open(&url(&directory.path().join("source.sqlite"))).await.unwrap();
        sqlx::raw_sql("CREATE TABLE snapshot_generation (side INTEGER PRIMARY KEY, value INTEGER); INSERT INTO snapshot_generation VALUES (0, 0), (1, 0); CREATE TABLE snapshot_padding (payload BLOB); WITH RECURSIVE n(i) AS (VALUES(1) UNION ALL SELECT i+1 FROM n WHERE i<2048) INSERT INTO snapshot_padding SELECT zeroblob(8192) FROM n;")
            .execute(&source.pool).await.unwrap();
        assert_eq!(sqlx::query_scalar::<_, String>("PRAGMA journal_mode").fetch_one(&source.pool).await.unwrap(), "wal");
        let path = directory.path().join("snapshot.sqlite");
        let writing = async {
            for generation in 1..=20 {
                let mut tx = source.pool.begin().await.unwrap();
                sqlx::query("UPDATE snapshot_generation SET value=? WHERE side=0").bind(generation).execute(&mut *tx).await.unwrap();
                tokio::task::yield_now().await;
                sqlx::query("UPDATE snapshot_generation SET value=? WHERE side=1").bind(generation).execute(&mut *tx).await.unwrap();
                tx.commit().await.unwrap();
            }
        };
        let (copied, ()) = tokio::join!(source.snapshot_into(&path), writing);
        copied.unwrap();
        let copy = Store::open_existing_durable(&url(&path)).await.unwrap();
        let values: Vec<i64> = sqlx::query_scalar("SELECT value FROM snapshot_generation ORDER BY side").fetch_all(&copy.pool).await.unwrap();
        assert_eq!(values.len(), 2);
        assert_eq!(values[0], values[1]);
        assert!((0..=20).contains(&values[0]));
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM snapshot_padding WHERE length(payload)=8192").fetch_one(&copy.pool).await.unwrap(), 2048);
        assert_eq!(sqlx::query_scalar::<_, String>("PRAGMA integrity_check").fetch_one(&copy.pool).await.unwrap(), "ok");
        copy.pool.close().await;
        source.pool.close().await;
    }).await.expect("snapshot copying and concurrent local writes must finish");
}

#[test]
fn cancelling_a_queued_copy_releases_its_source_connection() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    runtime.block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let source = Store::open_memory().await.unwrap();
        let (release, blocked) = std::sync::mpsc::channel();
        let blocker = tokio::task::spawn_blocking(move || blocked.recv().unwrap());
        let path = directory.path().join("cancelled.sqlite");
        let copying = {
            let source = source.clone();
            let path = path.clone();
            tokio::spawn(async move { source.snapshot_into(&path).await })
        };
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while !path.exists() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        copying.abort();
        assert!(copying.await.unwrap_err().is_cancelled());
        release.send(()).unwrap();
        blocker.await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), source.pool.close())
            .await
            .expect("cancelled worker must release its connection");
        let mut file = SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(&path)
                .create_if_missing(false),
        )
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM sqlite_schema")
                .fetch_one(&mut file)
                .await
                .unwrap(),
            0
        );
        file.close().await.unwrap();
    });
}
