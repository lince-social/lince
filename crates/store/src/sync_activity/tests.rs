use super::*;
use crate::Store;
use sqlx::Executor;
use std::collections::HashSet;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Instant;

const LEGACY_QUERY: &str = "DELETE FROM sync_activity WHERE at <= ? - (SELECT seconds FROM sync_history_policy WHERE id = 1) OR seq NOT IN (SELECT seq FROM sync_activity ORDER BY seq DESC LIMIT (SELECT max_entries FROM sync_history_policy WHERE id = 1))";

type Entry = (i64, i64, String, String);

#[tokio::test]
async fn disabled_history_does_not_wait_for_the_writer_or_commit() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        directory.path().join("disabled.sqlite").display()
    );
    let store = Store::open(&url).await.unwrap();
    assert!(!retention(&store.pool).await.unwrap().enabled);
    let mut writer = crate::write_tx(&store.pool).await.unwrap();
    let activity = Activity {
        instance: Instance::interface("local", "records"),
        direction: Direction::Outgoing,
        update: nucleus::sync::Update::Full,
    };
    let summary = Summary::new(Outcome::Refreshed, 10);
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        append(&store.pool, &activity, &summary, 10_000_000)
            .await
            .unwrap();
        prune(&store.pool, 10_000_000).await.unwrap();
    })
    .await
    .expect("disabled diagnostic history must not acquire the writer");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM sync_activity")
        .fetch_one(&mut *writer)
        .await
        .unwrap();
    assert_eq!(count, 0);
    writer.rollback().await.unwrap();
    store.pool.close().await;
    let store = Store::open(&url).await.unwrap();
    assert!(!retention(&store.pool).await.unwrap().enabled);
    store.pool.close().await;
}

#[tokio::test]
async fn enabled_history_does_not_prune_when_nothing_has_expired() {
    let store = Store::open_memory().await.unwrap();
    set_retention(
        &store.pool,
        Retention {
            enabled: true,
            ..Default::default()
        },
        10_000_000,
    )
    .await
    .unwrap();
    let before: i64 = sqlx::query_scalar("SELECT value FROM commit_sequence WHERE id = 1")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    prune(&store.pool, 10_000_000).await.unwrap();
    let after: i64 = sqlx::query_scalar("SELECT value FROM commit_sequence WHERE id = 1")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    assert_eq!(after, before);
}

async fn seed(store: &Store, entries: &[Entry]) {
    let mut tx = crate::write_tx(&store.pool).await.unwrap();
    for (seq, at, activity, summary) in entries {
        sqlx::query("INSERT INTO sync_activity(seq,at,activity,summary) VALUES(?,?,?,?)")
            .bind(seq)
            .bind(at)
            .bind(activity)
            .bind(summary)
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn retention_preserves_newest_entries_and_exact_age_boundary_with_sequence_gaps() {
    let store = Store::open_memory().await.unwrap();
    let now = 10_000_000i64;
    for seconds in [60u64, 604800, 7776000] {
        for max_entries in [1u32, 2, 10, 1000, 10000] {
            for count in [0i64, 1, 2, 7, 128] {
                clear(&store.pool).await.unwrap();
                set_retention(
                    &store.pool,
                    Retention {
                        enabled: true,
                        seconds,
                        max_entries,
                    },
                    now,
                )
                .await
                .unwrap();
                let entries: Vec<_> = (0..count)
                    .map(|index| {
                        let at = match index % 5 {
                            0 => now - seconds as i64 - 1,
                            1 => now - seconds as i64,
                            2 => now - seconds as i64 + 1,
                            3 => now,
                            _ => now + 10,
                        };
                        (
                            index * 2 + 1,
                            at,
                            format!("activity-{index}"),
                            format!("summary-{index}"),
                        )
                    })
                    .collect();
                seed(&store, &entries).await;
                let newest: HashSet<_> = entries
                    .iter()
                    .rev()
                    .take(max_entries as usize)
                    .map(|row| row.0)
                    .collect();
                let expected: Vec<_> = entries
                    .into_iter()
                    .filter(|row| newest.contains(&row.0) && row.1 > now - seconds as i64)
                    .collect();
                prune(&store.pool, now).await.unwrap();
                let actual: Vec<Entry> = sqlx::query_as(
                    "SELECT seq,at,activity,summary FROM sync_activity ORDER BY seq",
                )
                .fetch_all(&store.pool)
                .await
                .unwrap();
                assert_eq!(
                    actual, expected,
                    "seconds={seconds}, max_entries={max_entries}, count={count}"
                );
            }
        }
    }
}

#[tokio::test]
#[ignore = "private file-backed history retention comparison"]
async fn file_backed_retention_query_probe() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        directory.path().join("history.sqlite").display()
    );
    let store = Store::open(&url).await.unwrap();
    let version: String = sqlx::query_scalar("SELECT sqlite_version()")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    let now = 10_000_000i64;
    let activity = serde_json::to_string(&Activity {
        instance: Instance::interface("private-probe", "history"),
        direction: Direction::Outgoing,
        update: nucleus::sync::Update::Full,
    })
    .unwrap();
    let summary = serde_json::to_string(&Summary {
        subjects: (0..32)
            .map(|index| format!("private-{index:024}"))
            .collect(),
        ..Summary::new(Outcome::Refreshed, 1)
    })
    .unwrap();
    let mut cases = Vec::new();
    let mut expected_after_reopen = Vec::new();
    for limit in [8u32, 1000, 10000] {
        clear(&store.pool).await.unwrap();
        set_retention(
            &store.pool,
            Retention {
                enabled: true,
                seconds: 604800,
                max_entries: limit,
            },
            now,
        )
        .await
        .unwrap();
        let entries: Vec<_> = (0..=limit)
            .map(|index| (i64::from(index) + 1, now, activity.clone(), summary.clone()))
            .collect();
        seed(&store, &entries).await;
        let mut connection = store.pool.acquire().await.unwrap();
        let mut samples = Vec::new();
        let mut work = Vec::new();
        for (variant, query) in [("legacy", LEGACY_QUERY), ("candidate", PRUNE_QUERY)] {
            let plan: Vec<(i64, i64, i64, String)> =
                sqlx::query_as(&format!("EXPLAIN QUERY PLAN {query}"))
                    .bind(now)
                    .fetch_all(&mut *connection)
                    .await
                    .unwrap();
            connection
                .execute("SAVEPOINT retention_probe")
                .await
                .unwrap();
            let steps = Arc::new(AtomicU64::new(0));
            connection
                .lock_handle()
                .await
                .unwrap()
                .set_progress_handler(1, {
                    let steps = steps.clone();
                    move || {
                        steps.fetch_add(1, Ordering::Relaxed);
                        true
                    }
                });
            let result = sqlx::query(query)
                .bind(now)
                .execute(&mut *connection)
                .await
                .unwrap();
            connection
                .lock_handle()
                .await
                .unwrap()
                .remove_progress_handler();
            let count = steps.load(Ordering::Relaxed);
            assert_eq!(result.rows_affected(), 1);
            connection
                .execute("ROLLBACK TO retention_probe")
                .await
                .unwrap();
            connection.execute("RELEASE retention_probe").await.unwrap();
            work.push(
                serde_json::json!({"variant": variant, "progress_callbacks": count, "plan": plan}),
            );
        }
        assert!(
            work[1]["progress_callbacks"].as_u64().unwrap() * 2
                < work[0]["progress_callbacks"].as_u64().unwrap()
        );
        for (sample, variant) in [0usize, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0]
            .into_iter()
            .enumerate()
        {
            connection
                .execute("SAVEPOINT retention_probe")
                .await
                .unwrap();
            let started = Instant::now();
            let result = sqlx::query(if variant == 0 {
                LEGACY_QUERY
            } else {
                PRUNE_QUERY
            })
            .bind(now)
            .execute(&mut *connection)
            .await
            .unwrap();
            let elapsed_us = started.elapsed().as_micros();
            assert_eq!(result.rows_affected(), 1);
            connection
                .execute("ROLLBACK TO retention_probe")
                .await
                .unwrap();
            connection.execute("RELEASE retention_probe").await.unwrap();
            if sample >= 4 {
                samples.push(serde_json::json!({"variant": if variant == 0 { "legacy" } else { "candidate" }, "elapsed_us": elapsed_us}));
            }
        }
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sync_activity")
            .fetch_one(&mut *connection)
            .await
            .unwrap();
        assert_eq!(count, i64::from(limit) + 1);
        let actual: Vec<Entry> =
            sqlx::query_as("SELECT seq,at,activity,summary FROM sync_activity ORDER BY seq")
                .fetch_all(&mut *connection)
                .await
                .unwrap();
        assert_eq!(actual, entries);
        expected_after_reopen = entries;
        cases.push(serde_json::json!({"limit": limit, "work": work, "samples": samples}));
    }
    store.pool.close().await;
    let reopened = Store::open(&url).await.unwrap();
    let actual: Vec<Entry> =
        sqlx::query_as("SELECT seq,at,activity,summary FROM sync_activity ORDER BY seq")
            .fetch_all(&reopened.pool)
            .await
            .unwrap();
    assert_eq!(actual, expected_after_reopen);
    reopened.pool.close().await;
    let report = serde_json::json!({"sqlite_version": version, "progress_interval": 1, "scope": "Progress callbacks approximate VM instruction work; query-only elapsed excludes savepoint, rollback, setup, commit and fsync, and is not desktop latency.", "cases": cases, "reopened_exactly": true});
    let output = std::path::PathBuf::from(std::env::var("LINCE_HISTORY_PROBE_OUTPUT").unwrap());
    assert!(!output.exists());
    std::fs::write(output, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
}
