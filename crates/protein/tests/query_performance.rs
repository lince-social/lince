use protein::{Include, Protein, Source};
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;
use tracing::field::{Field, Visit};
use tracing_subscriber::{Layer, layer::Context, prelude::*};

#[derive(Default)]
struct Query {
    statement: String,
    summary: String,
    rows: u64,
    elapsed: f64,
}

impl Visit for Query {
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "db.statement" => {
                self.statement = value.split_whitespace().collect::<Vec<_>>().join(" ")
            }
            "summary" => self.summary = value.into(),
            _ => {}
        }
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        if field.name() == "rows_returned" {
            self.rows = value;
        }
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        if field.name() == "elapsed_secs" {
            self.elapsed = value;
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "summary" {
            self.summary = format!("{value:?}");
        }
    }
}

#[derive(Clone, Default)]
struct Queries {
    active: Arc<AtomicBool>,
    rows: Arc<Mutex<Vec<Query>>>,
}

impl<S: tracing::Subscriber> Layer<S> for Queries {
    fn enabled(&self, metadata: &tracing::Metadata<'_>, _: Context<'_, S>) -> bool {
        metadata.target() == "sqlx::query"
    }

    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        if self.active.load(Ordering::Acquire) {
            let mut query = Query::default();
            event.record(&mut query);
            let mut rows = self.rows.lock().unwrap();
            assert!(rows.len() < 512, "Private SQL probe exceeded its bound");
            rows.push(query);
        }
    }
}

fn requested(name: &str, fields: Option<&[String]>) -> bool {
    matches!(name, "uid" | "kind")
        || fields.is_none_or(|fields| fields.iter().any(|field| field == name))
}

#[tokio::test]
#[ignore = "private file-backed query attribution; run without competing compilation"]
async fn file_backed_selected_query_probe() {
    let output = std::path::PathBuf::from(std::env::var("LINCE_QUERY_PROBE_OUTPUT").unwrap());
    let database = std::path::PathBuf::from(std::env::var("LINCE_QUERY_PROBE_DB").unwrap());
    assert!(!database.exists());
    let count: usize = std::env::var("LINCE_QUERY_PROBE_RECORDS")
        .unwrap()
        .parse()
        .unwrap();
    let bytes: usize = std::env::var("LINCE_QUERY_PROBE_BYTES")
        .unwrap()
        .parse()
        .unwrap();
    let logs: usize = std::env::var("LINCE_QUERY_PROBE_LOGS")
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=64).contains(&count) && bytes <= 1_048_576 && logs <= 512);
    let traced = std::env::var("LINCE_QUERY_PROBE_TRACE").as_deref() == Ok("1");
    let queries = Queries::default();
    if traced {
        tracing::subscriber::set_global_default(
            tracing_subscriber::registry().with(queries.clone()),
        )
        .unwrap();
    }
    let store = store::Store::open(&format!("sqlite://{}", database.display()))
        .await
        .unwrap();
    let journal: String = store::sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    let synchronous: i64 = store::sqlx::query_scalar("PRAGMA synchronous")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    assert_eq!(journal, "wal");
    assert_eq!(synchronous, 2);
    let body = "é👩‍💻 paragraph\n".repeat(bytes / "é👩‍💻 paragraph\n".len());
    let mut ids = Vec::new();
    for index in 0..count {
        let record = store::records::create(
            &store.pool,
            store::records::NewRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: &format!("Private query {index}"),
                body: &body,
                quantity: nucleus::DecimalValue::parse_inferred("9007199254740993.125").unwrap(),
            },
        )
        .await
        .unwrap();
        store::records::set_extension(&store.pool, &record.uid, "work", &json!({
            "start":"2026-09-01", "due":"2026-09-30", "estimate_min":90,
            "logs":(0..logs).map(|_| json!({"start":"2026-09-01T10:00:00Z","end":"2026-09-01T10:15:00Z"})).collect::<Vec<_>>()
        })).await.unwrap();
        ids.push(record.uid);
    }
    let full = Protein {
        source: Source::Record,
        filter: Vec::new(),
        fields: None,
        include: Include::default(),
        aggregate: None,
        order: Vec::new(),
        limit: None,
    };
    let original = protein::execute(&store, &full).await.unwrap();
    assert!(original.len() >= count);
    for uid in &ids {
        let row = original.iter().find(|row| row["uid"] == *uid).unwrap();
        assert_eq!(row["body"], body);
        assert_eq!(row["spent_seconds"], logs * 900);
    }
    let mut reports = Vec::new();
    for (name, fields) in [
        (
            "native-metadata",
            Some(vec!["uid", "head", "slug", "kind", "quantity"]),
        ),
        ("body", Some(vec!["uid", "body", "head", "quantity"])),
        ("full", None),
    ] {
        let mut query = full.clone();
        query.fields = fields.map(|fields| fields.into_iter().map(str::to_owned).collect());
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
            assert_eq!(protein::execute(&store, &query).await.unwrap(), expected);
        }
        let mut samples = Vec::new();
        for _ in 0..3 {
            queries.rows.lock().unwrap().clear();
            queries.active.store(true, Ordering::Release);
            let started = Instant::now();
            let rows = protein::execute(&store, &query).await.unwrap();
            let elapsed = started.elapsed().as_micros() as u64;
            queries.active.store(false, Ordering::Release);
            assert_eq!(rows, expected);
            let mut statements = BTreeMap::<String, (usize, u64, f64)>::new();
            for query in queries.rows.lock().unwrap().drain(..) {
                let key = if query.statement.is_empty() {
                    query.summary
                } else {
                    query.statement
                };
                let entry = statements.entry(key).or_default();
                entry.0 += 1;
                entry.1 += query.rows;
                entry.2 += query.elapsed;
            }
            assert!(!traced || !statements.is_empty());
            samples.push(json!({"elapsed_us":elapsed,"sqlx_completions":statements.values().map(|entry|entry.0).sum::<usize>(),"statements":statements,"response_bytes":serde_json::to_vec(&rows).unwrap().len()}));
        }
        reports.push(json!({"name":name,"samples":samples}));
    }
    store.pool.close().await;
    let reopened = store::Store::open_existing_durable(&format!("sqlite://{}", database.display()))
        .await
        .unwrap();
    for uid in &ids {
        let record = store::records::get(&reopened.pool, uid)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(record.body, body);
    }
    reopened.pool.close().await;
    let report = json!({"case":"file-backed-selected-query","records":count,"response_rows":original.len(),"body_bytes_each":body.len(),"logs_each":logs,"traced":traced,"debug_assertions":cfg!(debug_assertions),"journal":journal,"synchronous":synchronous,"queries":reports,"cold_reopen_exact":true,"scope":"Direct Protein read/projection; excludes Session sync-history writes. SQLx completions count logged invocations, not SQLite trigger/implicit internal statements. Logging overhead and unoptimized elapsed times are diagnostic; no physical presentation or desktop claim."});
    std::fs::write(output, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
}
