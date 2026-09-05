use std::sync::Arc;

fn temp_db() -> (std::path::PathBuf, String) {
    let dir = std::env::temp_dir().join(nucleus::new_uid("store-concurrency"));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("lince.db");
    let url = format!("sqlite://{}", path.display());
    (dir, url)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_long_write_survives_readers_arriving_throughout() {
    let (dir, url) = temp_db();
    let store = Arc::new(store::Store::open(&url).await.expect("open"));

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

    let written = store::records::list_all(&store.pool)
        .await
        .unwrap()
        .into_iter()
        .filter(|record| record.head.starts_with('w'))
        .count();
    assert_eq!(written, 100, "every read-then-write transaction committed");
    let _ = std::fs::remove_dir_all(dir);
}
