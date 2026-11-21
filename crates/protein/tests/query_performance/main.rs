mod fixture;
mod linux_io;
#[path = "../support/process_cpu.rs"]
mod process_cpu;
mod sql_events;

use fixture::{Settings, seed};
use linux_io::process_reads;
use nucleus::performance::{Reading, install};
use process_cpu::process_cpu_us;
use protein::{Include, Protein, Source, execute};
use serde_json::json;
use sql_events::Queries;
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};
use store::{Store, records::get, sqlx::query_as};
use tracing_subscriber::{layer::SubscriberExt, registry};

static DECODED_BODY_BYTES: AtomicU64 = AtomicU64::new(0);

fn observe_reads(reading: Reading) {
    if let Reading::Count("record-read-body-bytes", count) = reading {
        DECODED_BODY_BYTES.fetch_add(count, Ordering::Relaxed);
    }
}

fn requested(name: &str, fields: Option<&[String]>) -> bool {
    matches!(name, "uid" | "kind")
        || fields.is_none_or(|fields| fields.iter().any(|field| field == name))
}

#[tokio::test]
#[ignore = "private file-backed query attribution; run without competing compilation"]
async fn file_backed_selected_query_probe() {
    let Settings {
        output,
        database,
        count,
        bytes,
        logs,
        traced,
    } = Settings::from_env();
    let queries = Queries::default();
    if traced {
        assert!(install(observe_reads));
        tracing::subscriber::set_global_default(registry().with(queries.clone())).unwrap();
    }
    let store = Store::open(&format!("sqlite://{}", database.display()))
        .await
        .unwrap();
    let (journal, synchronous, cache_size, page_size): (String, i64, i64, i64) = query_as(
        "SELECT journal_mode, synchronous, cache_size, page_size
         FROM pragma_journal_mode(), pragma_synchronous(), pragma_cache_size(), pragma_page_size()",
    )
    .fetch_one(&store.pool)
    .await
    .expect("file-backed SQLite settings");
    assert_eq!(journal, "wal");
    assert_eq!(synchronous, 2);
    let (body, ids) = seed(&store, count, bytes, logs).await;
    let full = Protein {
        source: Source::Record,
        filter: Vec::new(),
        fields: None,
        include: Include::default(),
        aggregate: None,
        order: Vec::new(),
        limit: None,
    };
    let original = execute(&store, &full).await.unwrap();
    assert!(original.len() >= count);
    for uid in &ids {
        let row = original.iter().find(|row| row["uid"] == *uid).unwrap();
        assert_eq!(row["body"], body);
        assert_eq!(row["spent_seconds"], logs * 900);
    }
    let mut reports = Vec::with_capacity(3);
    for (name, fields) in [
        (
            "native-metadata",
            Some(vec!["uid", "head", "slug", "kind", "quantity"]),
        ),
        ("body", Some(vec!["uid", "body", "head", "quantity"])),
        ("full", None),
    ] {
        let query = Protein {
            fields: fields.map(|fields| fields.into_iter().map(str::to_owned).collect()),
            ..full.clone()
        };
        let expected: Vec<_> = original
            .iter()
            .cloned()
            .map(|mut row| {
                row.as_object_mut()
                    .unwrap()
                    .retain(|key, _| requested(key, query.fields.as_deref()));
                row
            })
            .collect();
        for _ in 0..2 {
            assert_eq!(execute(&store, &query).await.unwrap(), expected);
        }
        let mut samples = Vec::with_capacity(3);
        for _ in 0..3 {
            DECODED_BODY_BYTES.store(0, Ordering::Relaxed);
            queries.start();
            let reads_started = traced.then(process_reads).flatten();
            let cpu_started = process_cpu_us();
            let started = Instant::now();
            let rows = execute(&store, &query).await.unwrap();
            let elapsed =
                u64::try_from(started.elapsed().as_micros()).expect("probe elapsed microseconds");
            let cpu_us = process_cpu_us()
                .zip(cpu_started)
                .and_then(|(end, start)| end.checked_sub(start));
            let reads = traced
                .then(process_reads)
                .flatten()
                .zip(reads_started)
                .map(|(end, start)| (end.0.saturating_sub(start.0), end.1.saturating_sub(start.1)));
            let statements = queries.finish();
            assert_eq!(rows, expected);
            assert!(!traced || !statements.is_empty());
            samples.push(json!({"elapsed_us":elapsed,"cpu_us":cpu_us,"sqlx_completions":statements.values().map(|entry|entry.0).sum::<usize>(),"statements":statements,"response_bytes":serde_json::to_vec(&rows).unwrap().len(),"decoded_body_bytes":traced.then(|| DECODED_BODY_BYTES.load(Ordering::Relaxed)),"logical_read_bytes":reads.map(|value|value.0),"read_calls":reads.map(|value|value.1)}));
        }
        reports.push(json!({"name":name,"samples":samples}));
    }
    store.pool.close().await;
    let reopened = Store::open_existing_durable(&format!("sqlite://{}", database.display()))
        .await
        .unwrap();
    for uid in &ids {
        let record = get(&reopened.pool, uid).await.unwrap().unwrap();
        assert_eq!(record.body, body);
    }
    reopened.pool.close().await;
    let report = json!({"case":"file-backed-selected-query","records":count,"response_rows":original.len(),"body_bytes_each":body.len(),"logs_each":logs,"traced":traced,"debug_assertions":cfg!(debug_assertions),"journal":journal,"synchronous":synchronous,"cache_size":cache_size,"page_size":page_size,"queries":reports,"cold_reopen_exact":true,"scope":"Direct Protein read/projection; excludes Session sync-history writes. CPU is process-wide across the query's threads. Diagnostic Linux logical-read counters include a small procfs-read overhead and are not physical disk reads. SQLx completions count logged invocations, not SQLite trigger/implicit internal statements. Logging overhead and unoptimized elapsed times are diagnostic; no physical presentation or desktop claim."});
    std::fs::write(output, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
}
