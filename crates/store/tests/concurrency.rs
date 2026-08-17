//! Can this store take a long write while something else is reading?
//!
//! Every other test in this workspace opens `Store::open_memory()`, which runs
//! on ONE connection — so no test can produce contention, and for a long time
//! none did. The first time an Action wrote a few hundred rows in a row while
//! the board held live Protein subscriptions open, the real app answered
//! `database is locked` (SQLITE_BUSY, code 5) and no test had ever been in a
//! position to notice.
//!
//! So this one opens a real FILE, which is what `Store::open` configures and
//! what the app actually runs on.

use std::sync::Arc;

fn temp_db() -> (std::path::PathBuf, String) {
    let dir = std::env::temp_dir().join(nucleus::new_uid("store-concurrency"));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("lince.db");
    let url = format!("sqlite://{}", path.display());
    (dir, url)
}

/// The shape that broke: many writes in sequence while readers keep arriving.
///
/// Fails with `database is locked` when the pool is left on SQLite's default
/// rollback journal, because a writer holds an exclusive lock and every other
/// connection in the pool is refused outright rather than made to wait.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_long_write_survives_readers_arriving_throughout() {
    let (dir, url) = temp_db();
    let store = Arc::new(store::Store::open(&url).await.expect("open"));

    // Readers, of the kind a board's live subscriptions produce.
    let mut readers = Vec::new();
    for _ in 0..4 {
        let store = Arc::clone(&store);
        readers.push(tokio::spawn(async move {
            for _ in 0..200 {
                store::records::list_all(&store.pool)
                    .await
                    .expect("a read must not be refused while a write is in flight");
                tokio::task::yield_now().await;
            }
        }));
    }

    // The write: one Action's worth of Records, made one at a time, which is
    // what importing the documentation bundle does.
    for index in 0..150 {
        store::records::create(
            &store.pool,
            store::records::NewRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: &format!("Record {index}"),
                body: "",
                quantity: store::exact::zero(),
            },
        )
        .await
        .unwrap_or_else(|err| panic!("write {index} was refused: {err}"));
    }

    for reader in readers {
        reader.await.expect("reader task");
    }

    let all = store::records::list_all(&store.pool).await.unwrap();
    assert!(all.len() >= 150, "every write landed: {}", all.len());
    let _ = std::fs::remove_dir_all(dir);
}

/// Two writers at once must WAIT for each other, not fail. WAL still allows
/// only one writer at a time; what keeps that from being an error is the busy
/// timeout, and a pool with several connections has no other way to survive
/// two Actions arriving together.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_writers_at_once_queue_rather_than_fail() {
    let (dir, url) = temp_db();
    let store = Arc::new(store::Store::open(&url).await.expect("open"));

    let mut writers = Vec::new();
    for writer in 0..3 {
        let store = Arc::clone(&store);
        writers.push(tokio::spawn(async move {
            for index in 0..40 {
                store::records::create(
                    &store.pool,
                    store::records::NewRecord {
                        slug: None,
                        kind: nucleus::RecordKind::Plain,
                        head: &format!("w{writer} r{index}"),
                        body: "",
                        quantity: store::exact::zero(),
                    },
                )
                .await
                .unwrap_or_else(|err| panic!("writer {writer} was refused at {index}: {err}"));
            }
        }));
    }
    for handle in writers {
        handle.await.expect("writer task");
    }

    let all = store::records::list_all(&store.pool).await.unwrap();
    assert!(all.len() >= 120, "nothing was dropped: {}", all.len());
    let _ = std::fs::remove_dir_all(dir);
}

/// The shape that produced the owner's `database is locked`: a transaction
/// that READS and only later WRITES, while another CONNECTION writes in
/// between. SQLite refuses the upgrade immediately (code 517,
/// SQLITE_BUSY_SNAPSHOT) because waiting would deadlock, so no busy timeout
/// and no journal mode avoids it.
///
/// This is the acceptance gate for `BEGIN IMMEDIATE`: it fails against a
/// plain `pool.begin()` and passes through `store::write_tx`, which is the
/// whole difference between the two.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn read_then_write_transactions_running_at_once_all_commit() {
    let (dir, url) = temp_db();
    let store = Arc::new(store::Store::open(&url).await.expect("open"));
    let organ = store::organs::local(&store.pool)
        .await
        .expect("local organ")
        .expect("every Cell has one")
        .uid;

    let mut workers = Vec::new();
    for worker in 0..4 {
        let store = Arc::clone(&store);
        let organ = organ.clone();
        workers.push(tokio::spawn(async move {
            for round in 0..25 {
                // `write_tx` is `BEGIN IMMEDIATE`: it says up front that it
                // will write and takes the write lock NOW. Swap it for
                // `store.pool.begin()` and this loop dies on code 517, because
                // the read below would leave it holding a snapshot it then has
                // to upgrade — and whoever commits first wins.
                let mut tx = store::write_tx(&store.pool).await.expect("begin");

                let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM record")
                    .fetch_one(&mut *tx)
                    .await
                    .expect("read inside the transaction");

                sqlx::query(
                    "INSERT INTO record (uid, kind, head, body, quantity_mantissa,
                                         quantity_scale, organ_uid, created_at, updated_at)
                     VALUES (?, 'plain', ?, '', '0', 0, ?, '2026-01-01', '2026-01-01')",
                )
                .bind(nucleus::new_uid("r"))
                .bind(format!("w{worker} r{round} saw {before}"))
                .bind(&organ)
                .execute(&mut *tx)
                .await
                .unwrap_or_else(|err| {
                    panic!("worker {worker} could not write after reading, round {round}: {err}")
                });

                tx.commit().await.expect("commit");
            }
        }));
    }
    for handle in workers {
        handle.await.expect("worker task");
    }

    // Counted by head rather than by total: `Store::open` seeds the Cell's own
    // Organ and Lingua Records, which are not this test's business.
    let written = store::records::list_all(&store.pool)
        .await
        .unwrap()
        .into_iter()
        .filter(|record| record.head.starts_with('w'))
        .count();
    assert_eq!(written, 100, "every read-then-write transaction committed");
    let _ = std::fs::remove_dir_all(dir);
}
