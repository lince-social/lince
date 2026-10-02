use super::*;
use std::{path::Path, time::Instant};

async fn command(engine: &Engine, request: Command) -> Value {
    engine
        .social_command(request, None, nucleus::execution::now())
        .await
        .unwrap()
        .data
        .unwrap()
}

fn timings(mut samples: Vec<f64>) -> Value {
    samples.sort_by(f64::total_cmp);
    let percentile =
        |percent: usize| samples[(samples.len() * percent).div_ceil(100).saturating_sub(1)];
    json!({"samples":samples.len(),"p50_ms":percentile(50),"p95_ms":percentile(95),"p99_ms":percentile(99),"maximum_ms":samples.last()})
}

async fn storage(engine: &Engine, path: &Path) -> Value {
    let pages: i64 = store::sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    let size: i64 = store::sqlx::query_scalar("PRAGMA page_size")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    let length = |path: &Path| std::fs::metadata(path).map_or(0, |metadata| metadata.len());
    let wal = path.with_file_name(format!(
        "{}-wal",
        path.file_name().unwrap().to_str().unwrap()
    ));
    json!({"allocated_main_bytes":pages*size,"main_file_bytes":length(path),"wal_bytes":length(&wal),"includes":"SQLite text, indexes and runtime metadata; excludes blob stores and backups"})
}

fn machine() -> Value {
    let cpu = std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                line.strip_prefix("model name")
                    .and_then(|tail| tail.split_once(':'))
                    .map(|(_, model)| model.trim().to_owned())
            })
        });
    let memory = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                line.strip_prefix("VmHWM:")
                    .map(|value| value.trim().to_owned())
            })
        });
    json!({"reference":"lince-social-search-baseline","cpu":cpu,"os":std::env::consts::OS,"architecture":std::env::consts::ARCH,"build":if cfg!(debug_assertions) {"unoptimized"} else {"optimized"},"available_parallelism":std::thread::available_parallelism().ok().map(std::num::NonZeroUsize::get),"process_peak_rss":memory})
}

async fn load(engine: &Engine, prototype: &Snippet, signer: &Signer, source: &str) {
    let started = Instant::now();
    let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
    store::social::anchor_posting_authority_on(&mut tx, prototype.anonymous.as_ref().unwrap())
        .await
        .unwrap();
    for index in 0..100_000u128 {
        let mut document = prototype.clone();
        document.nonce = B64.encode(index.to_be_bytes());
        document.id = post_id::post_id(
            &document.anonymous.as_ref().unwrap().owner_key,
            &document.nonce,
            document.mode,
            &document.alias,
            None,
            &document.destinations,
        )
        .unwrap();
        document.title = format!("Bicycle repair item {index}");
        document.signature = signer.sign_bytes(&signing_bytes("snippet", &document).unwrap());
        if index % 10_000 == 0 {
            validate_snippet(&document, nucleus::execution::now().timestamp()).unwrap();
        }
        let body = serde_json::to_string(&document).unwrap();
        let hash = document_hash("snippet", &document).unwrap();
        store::sqlx::query("INSERT INTO social_document(kind,id,authority,revision,hash,body,expires_at,state,title,text,direction,language,area,concept,unit,source,generation) VALUES('snippet',?,?,1,?,?,?,'active',?,?,'need','','','','',?,1)")
            .bind(&document.id).bind(&document.anonymous.as_ref().unwrap().owner_key).bind(&hash).bind(&body).bind(document.expires_at).bind(&document.title).bind(&document.text).bind(source).execute(&mut *tx).await.unwrap();
        store::sqlx::query(
            "INSERT INTO social_revision(kind,id,hash,body,expires_at) VALUES('snippet',?,?,?,?)",
        )
        .bind(&document.id)
        .bind(hash)
        .bind(body)
        .bind(document.expires_at)
        .execute(&mut *tx)
        .await
        .unwrap();
        store::sqlx::query("INSERT INTO social_search(id,title,text) VALUES(?,?,?)")
            .bind(&document.id)
            .bind(&document.title)
            .bind(&document.text)
            .execute(&mut *tx)
            .await
            .unwrap();
        if (index + 1) % 10_000 == 0 {
            eprintln!(
                "LINCE_SOCIAL_PROGRESS loaded={} elapsed_seconds={:.2}",
                index + 1,
                started.elapsed().as_secs_f64()
            );
        }
    }
    tx.commit().await.unwrap();
}

async fn search_samples(engine: &Engine, endpoint: &str, public: bool) -> Value {
    let mut samples = Vec::new();
    let mut first = Vec::new();
    let query = Search {
        text: "bicycle repair".into(),
        ..Default::default()
    };
    for iteration in 0..60 {
        let start = Instant::now();
        let response = if public {
            engine
                .social_public_request(
                    "measurement-reader",
                    endpoint,
                    PublicRequest::Search {
                        query: query.clone(),
                        known: Vec::new(),
                    },
                    nucleus::execution::now().timestamp(),
                )
                .await
                .unwrap()
        } else {
            command(
                engine,
                Command::Search {
                    query: query.clone(),
                    services: Vec::new(),
                },
            )
            .await
        };
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        if iteration % 10 == 0 {
            eprintln!(
                "LINCE_SOCIAL_PROGRESS public={public} iteration={iteration} elapsed_ms={elapsed:.2}"
            );
        }
        let rows = response["results"].as_array().unwrap();
        assert_eq!(rows.len(), 50);
        assert!(serde_json::to_vec(&response).unwrap().len() <= MAX_FRAME_BYTES);
        if iteration == 0 {
            first = rows
                .iter()
                .map(|row| row["document"]["id"].as_str().unwrap().to_owned())
                .collect();
            let mut unique = std::collections::BTreeSet::new();
            for row in rows {
                let document: Snippet = serde_json::from_value(row["document"].clone()).unwrap();
                validate_snippet(&document, nucleus::execution::now().timestamp()).unwrap();
                assert_eq!(document_hash("snippet", &document).unwrap(), row["hash"]);
                assert!(unique.insert(document.id));
            }
            if !public {
                let started = Instant::now();
                let mut rows = store::social::search(
                    &engine.store.pool,
                    &query,
                    nucleus::execution::now().timestamp(),
                )
                .await
                .unwrap();
                let search_ms = started.elapsed().as_secs_f64() * 1000.0;
                let started = Instant::now();
                engine.social_filter_local_results(&mut rows).await.unwrap();
                let filter_ms = started.elapsed().as_secs_f64() * 1000.0;
                let started = Instant::now();
                engine.social_discovery_details(&mut rows).await.unwrap();
                let details_ms = started.elapsed().as_secs_f64() * 1000.0;
                let started = Instant::now();
                engine.social_discovery_conflicts(&query).await.unwrap();
                let conflicts_ms = started.elapsed().as_secs_f64() * 1000.0;
                eprintln!(
                    "LINCE_SOCIAL_COMPONENTS {}",
                    json!({"search_ms":search_ms,"filter_ms":filter_ms,"details_ms":details_ms,"conflicts_ms":conflicts_ms})
                );
            }
        }
        if iteration >= 10 {
            samples.push(elapsed);
        }
    }
    let mut next = query;
    next.after = first.last().cloned();
    let page = store::social::search(
        &engine.store.pool,
        &next,
        nucleus::execution::now().timestamp(),
    )
    .await
    .unwrap();
    assert_eq!(page.len(), 50);
    assert!(
        page.iter()
            .all(|row| !first.iter().any(|id| row["document"]["id"] == *id))
    );
    timings(samples)
}

#[tokio::test]
#[ignore = "Explicit disk-backed 100,000-listing and 10,000-cache resource baseline"]
async fn disk_backed_directory_and_local_cache_search_baseline() {
    tokio::time::timeout(std::time::Duration::from_secs(900),Box::pin(async {
        let directory = tempfile::Builder::new().prefix(".social-search-baseline-").tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let path = directory.path().join("search-baseline.db");
        let engine = Engine::open(&format!("sqlite://{}?mode=rwc",path.display())).await.unwrap();
        command(&engine,Command::ConfigureServices {settings:ServiceSettings {directory:true,cache_entries:100_000,incoming_bytes_per_minute:1024*1024*1024,outgoing_bytes_per_minute:1024*1024*1024,..Default::default()}}).await;
        let author = Engine::open_memory().await.unwrap();
        let endpoint = iroh::SecretKey::from_bytes(&[213;32]).public().to_string();
        let saved = command(&author,Command::SaveDraft {record:None,source:None,draft:PostDraft {title:"Bicycle repair".into(),text:"A benchmark fixture needs a spare bicycle chain".into(),alias:"Benchmark author".into(),destinations:vec![endpoint.clone()],..Default::default()},}).await;
        let context = saved["record"].as_str().unwrap();
        let preview = command(&author,Command::Preview {record:context.into(),state:PostState::Active}).await;
        let prototype:Snippet = serde_json::from_value(preview["document"].clone()).unwrap();
        let state = store::records::get_extension(&author.store.pool,context,PUBLICATION_NAMESPACE).await.unwrap().unwrap();
        let signer = session::secret_signer(state["secret"].as_str().unwrap()).unwrap();
        let loading = Instant::now();
        load(&engine,&prototype,&signer,&endpoint).await;
        let loading_seconds = loading.elapsed().as_secs_f64();
        let listings:i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM social_document WHERE kind='snippet'").fetch_one(&engine.store.pool).await.unwrap();
        assert_eq!(listings,100_000);
        let directory_search = search_samples(&engine,&endpoint,true).await;
        let directory_storage = storage(&engine,&path).await;
        let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
        store::sqlx::query("DELETE FROM social_document WHERE id NOT IN (SELECT id FROM social_document ORDER BY id LIMIT 10000)").execute(&mut *tx).await.unwrap();
        store::sqlx::query("DELETE FROM social_search WHERE id NOT IN (SELECT id FROM social_document)").execute(&mut *tx).await.unwrap();
        store::sqlx::query("DELETE FROM social_revision WHERE id NOT IN (SELECT id FROM social_document)").execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        command(&engine,Command::ConfigureServices {settings:ServiceSettings {cache_entries:10_000,..Default::default()}}).await;
        let cached:i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM social_document WHERE kind='snippet'").fetch_one(&engine.store.pool).await.unwrap();
        assert_eq!(cached,10_000);
        let local_search = search_samples(&engine,&endpoint,false).await;
        let report = json!({"measured_at":nucleus::execution::now().to_rfc3339(),"machine":machine(),"storage_medium":"Temporary directory on the shared workspace filesystem","loading_seconds":loading_seconds,"directory":{"entries":listings,"search":directory_search,"storage":directory_storage},"local":{"entries":cached,"search":local_search,"storage":storage(&engine,&path).await},"scope":"Single process, text-only, deliberately shared fixture author; bulk loading excludes admission/verification throughput; deleting rows does not compact SQLite allocation; no connection churn, power loss or media load"});
        eprintln!("LINCE_SOCIAL_MEASUREMENT {report}");
        assert!(report["local"]["search"]["p95_ms"].as_f64().unwrap()<250.0,"{report}");
    })).await.unwrap();
}
