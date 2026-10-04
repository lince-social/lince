use super::*;
use std::time::Instant;

fn process() -> Value {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let fields: Vec<&str> = stat
        .rsplit_once(')')
        .map(|(_, tail)| tail.split_whitespace().collect())
        .unwrap_or_default();
    let ticks = fields
        .get(11)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0)
        + fields
            .get(12)
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
    let field = |name: &str| {
        status
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .map(str::trim)
            .unwrap_or("unavailable")
            .to_owned()
    };
    json!({"cpu_ticks":ticks,"rss":field("VmRSS:"),"peak_rss":field("VmHWM:"),"threads":field("Threads:"),"affinity":field("Cpus_allowed_list:")})
}

async fn run_tier(name: &str, owners: u32, concurrency: u32, settings: ServiceSettings) -> Value {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("tier.sqlite");
    let engine = Arc::new(
        Box::pin(Engine::open(path.to_str().unwrap()))
            .await
            .unwrap(),
    );
    Box::pin(engine.social_command(
        Command::ConfigureServices {
            settings: settings.clone(),
        },
        None,
        nucleus::execution::now(),
    ))
    .await
    .unwrap();
    let endpoint = iroh::SecretKey::from_bytes(&[221; 32]).public().to_string();
    let mut verification = Vec::new();
    for index in 0..32 {
        let (document, _) = publication(index, &endpoint);
        let begin = Instant::now();
        validate_snippet(&document, nucleus::execution::now().timestamp()).unwrap();
        verification.push(begin.elapsed().as_secs_f64() * 1000.0);
    }
    verification.sort_by(f64::total_cmp);
    let before = process();
    let begin = Instant::now();
    let mut admitted = Vec::new();
    let mut refused = 0;
    let mut incoming = 0;
    let mut outgoing = 0;
    let mut durations = Vec::new();
    for wave in (0..owners).step_by(concurrency as usize) {
        let mut workers = tokio::task::JoinSet::new();
        for index in wave..(wave + concurrency).min(owners) {
            let (document, editor) = publication(index, &endpoint);
            let mut source = [220; 32];
            source[..4].copy_from_slice(&index.to_le_bytes());
            let source = iroh::SecretKey::from_bytes(&source).public().to_string();
            let engine = engine.clone();
            let endpoint = endpoint.clone();
            workers.spawn(async move {
                let request = PublicRequest::PublishSnippet {
                    document: document.clone(),
                };
                let bytes = serde_json::to_vec(&request).unwrap().len();
                let begin = Instant::now();
                let result = engine
                    .social_public_request(
                        &source,
                        &endpoint,
                        request,
                        nucleus::execution::now().timestamp(),
                    )
                    .await;
                (
                    document,
                    editor,
                    result,
                    bytes,
                    begin.elapsed().as_secs_f64() * 1000.0,
                )
            });
        }
        while let Some(result) = workers.join_next().await {
            let (document, editor, result, bytes, duration) = result.unwrap();
            incoming += bytes;
            durations.push(duration);
            match result {
                Ok(value) => {
                    outgoing += serde_json::to_vec(&value).unwrap().len();
                    admitted.push((document, editor));
                }
                Err(error) => {
                    assert!(
                        error.to_string().contains("full") || error.to_string().contains("limited"),
                        "{error}"
                    );
                    refused += 1;
                }
            }
        }
    }
    assert!(!admitted.is_empty());
    let count = admitted.len();
    let (document, editor) = admitted.remove(0);
    let mut ending = document.clone();
    ending.state = PostState::Withdrawn;
    ending.revision = "2".into();
    ending.parent = Some(document_hash("snippet", &document).unwrap());
    ending.signature = editor.sign_bytes(&signing_bytes("snippet", &ending).unwrap());
    let control_start = Instant::now();
    engine
        .social_public_request(
            &endpoint,
            &endpoint,
            PublicRequest::PublishSnippet { document: ending },
            nucleus::execution::now().timestamp(),
        )
        .await
        .unwrap();
    let control_ms = control_start.elapsed().as_secs_f64() * 1000.0;
    let pages: i64 = store::sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    let size: i64 = store::sqlx::query_scalar("PRAGMA page_size")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    let indexes: i64 =
        store::sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type='index'")
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
    let index_bytes = store::sqlx::query_scalar::<_, i64>("SELECT COALESCE(SUM(pgsize),0) FROM dbstat WHERE name IN (SELECT name FROM sqlite_master WHERE type='index')").fetch_one(&engine.store.pool).await.ok();
    let wal =
        std::fs::metadata(path.with_file_name("tier.sqlite-wal")).map_or(0, |value| value.len());
    let after = process();
    durations.sort_by(f64::total_cmp);
    let result = json!({"name":name,"settings":settings,"owners":owners,"concurrent_handler_tasks":concurrency,"admitted":count,"refused":refused,"elapsed_seconds":begin.elapsed().as_secs_f64(),"handler_p95_ms":durations[(durations.len()*95).div_ceil(100)-1],"signature_verification_p95_ms":verification[30],"withdrawal_ms":control_ms,"incoming_serialized_bytes":incoming,"successful_reply_bytes":outgoing,"allocated_database_bytes":pages*size,"wal_bytes":wal,"index_count":indexes,"index_bytes":index_bytes,"before":before,"after":after,"measurement_scope":"real SQLite and signed many-owner handlers; transport churn measured separately; process memory includes shared schema/runtime"});
    engine.store.pool.close().await;
    let reopened = Engine::open(path.to_str().unwrap()).await.unwrap();
    let stale = reopened
        .social_public_request(
            &endpoint,
            &endpoint,
            PublicRequest::PublishSnippet {
                document: document.clone(),
            },
            nucleus::execution::now().timestamp(),
        )
        .await
        .unwrap();
    assert_eq!(stale["accepted"], false);
    assert_eq!(stale["state"], "withdrawn");
    reopened.store.pool.close().await;
    result
}

#[tokio::test]
async fn resource_tiers_measure_many_owner_intake_control_progress_and_disk_cost() {
    let selected = std::env::var("LINCE_SOCIAL_RESOURCE_TIER").unwrap_or_else(|_| "both".into());
    assert!(matches!(selected.as_str(), "both" | "small" | "server"));
    let mut profiles = Vec::new();
    if selected != "server" {
        let small = Box::pin(run_tier(
            "small-cell-quota-reference",
            256,
            4,
            ServiceSettings {
                directory: true,
                cache_entries: 100,
                storage_bytes: 1024 * 1024,
                ..Default::default()
            },
        ))
        .await;
        profiles.push(small);
    }
    if selected != "small" {
        let server = Box::pin(run_tier(
            "modest-host-quota-reference",
            1024,
            16,
            ServiceSettings {
                directory: true,
                cache_entries: 10_000,
                storage_bytes: 1024 * 1024 * 1024,
                incoming_bytes_per_minute: 16 * 1024 * 1024,
                outgoing_bytes_per_minute: 16 * 1024 * 1024,
                ..Default::default()
            },
        ))
        .await;
        profiles.push(server);
    }
    let cpu = std::fs::read_to_string("/proc/cpuinfo")
        .unwrap_or_default()
        .lines()
        .find_map(|line| {
            line.strip_prefix("model name")
                .and_then(|tail| tail.split_once(':'))
                .map(|(_, value)| value.trim().to_owned())
        });
    let cgroup = std::fs::read_to_string("/proc/self/cgroup").unwrap_or_default();
    let group = cgroup
        .lines()
        .find_map(|line| line.strip_prefix("0::"))
        .unwrap_or("");
    let root = std::path::Path::new("/sys/fs/cgroup").join(group.trim_start_matches('/'));
    let limits = json!({"memory_max":std::fs::read_to_string(root.join("memory.max")).ok().map(|value|value.trim().to_owned()),"cpu_max":std::fs::read_to_string(root.join("cpu.max")).ok().map(|value|value.trim().to_owned())});
    println!(
        "LINCE_SOCIAL_RESOURCE_REPORT {}",
        json!({"os":std::env::consts::OS,"architecture":std::env::consts::ARCH,"cpu":cpu,"build":if cfg!(debug_assertions) {"unoptimized"} else {"optimized"},"limits":limits,"profiles":profiles,"search_optimization":"deferred by owner","hardware_scope":"current Linux reference machine, with actual affinity/limits reported; not a physical phone qualification"})
    );
}

#[tokio::test]
async fn ordinary_identity_flood_cannot_reset_global_counters_or_spend_control_reserve() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("budget.sqlite");
    let engine = host(&path).await;
    let now = nucleus::execution::now().timestamp();
    let endpoint = iroh::SecretKey::from_bytes(&[229; 32]).public().to_string();
    let (document, editor) = publication(9_000, &endpoint);
    engine
        .social_public_request(
            &endpoint,
            &endpoint,
            PublicRequest::PublishSnippet {
                document: document.clone(),
            },
            now,
        )
        .await
        .unwrap();
    let mut admitted = 0;
    for index in 0..1600u32 {
        let mut key = [228; 32];
        key[..4].copy_from_slice(&index.to_le_bytes());
        let source = iroh::SecretKey::from_bytes(&key).public().to_string();
        let result = engine
            .social_public_request(&source, &endpoint, PublicRequest::DescribeService, now)
            .await;
        if result.is_ok() {
            admitted += 1;
        } else {
            break;
        }
    }
    assert!(admitted > 1000 && admitted < 1536);
    let spent: (i64, i64) = store::sqlx::query_as(
        "SELECT bytes,work FROM social_service_budget WHERE source='*' AND direction='in'",
    )
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    engine.store.pool.close().await;
    let engine = Engine::open(path.to_str().unwrap()).await.unwrap();
    let fresh = iroh::SecretKey::from_bytes(&[227; 32]).public().to_string();
    assert!(
        engine
            .social_public_request(&fresh, &endpoint, PublicRequest::DescribeService, now)
            .await
            .unwrap_err()
            .to_string()
            .contains("limited")
    );
    assert_eq!(
        store::sqlx::query_as::<_, (i64, i64)>(
            "SELECT bytes,work FROM social_service_budget WHERE source='*' AND direction='in'"
        )
        .fetch_one(&engine.store.pool)
        .await
        .unwrap(),
        spent
    );
    let mut ending = document.clone();
    ending.state = PostState::Withdrawn;
    ending.revision = "2".into();
    ending.parent = Some(document_hash("snippet", &document).unwrap());
    ending.signature = editor.sign_bytes(&signing_bytes("snippet", &ending).unwrap());
    let mut forged = ending.clone();
    forged.signature = "forged-control".into();
    let forged_source = iroh::SecretKey::from_bytes(&[226; 32]).public().to_string();
    assert!(
        engine
            .social_public_request(
                &forged_source,
                &endpoint,
                PublicRequest::PublishSnippet { document: forged },
                now
            )
            .await
            .is_err()
    );
    engine
        .social_public_request(
            &endpoint,
            &endpoint,
            PublicRequest::PublishSnippet { document: ending },
            now,
        )
        .await
        .unwrap();
    assert_eq!(
        store::sqlx::query_scalar::<_, String>("SELECT state FROM social_document WHERE id=?")
            .bind(&document.id)
            .fetch_one(&engine.store.pool)
            .await
            .unwrap(),
        "withdrawn"
    );
    engine.store.pool.close().await;
}

#[tokio::test]
async fn frame_and_traffic_limits_refuse_large_payloads_before_full_document_decoding() {
    let engine = Engine::open_memory().await.unwrap();
    let endpoint = iroh::SecretKey::from_bytes(&[225; 32]).public().to_string();
    let now = nucleus::execution::now().timestamp();
    engine
        .social_command(
            Command::ConfigureServices {
                settings: ServiceSettings {
                    directory: true,
                    mailbox: true,
                    incoming_bytes_per_minute: 1024 * 1024 * 1024,
                    ..Default::default()
                },
            },
            None,
            nucleus::execution::now(),
        )
        .await
        .unwrap();
    let elements = std::iter::repeat_n("\"\"", 120_000)
        .collect::<Vec<_>>()
        .join(",");
    let public = format!(
        "{{\"request\":\"publish-profile\",\"document\":{{\"fields\":{{\"links\":[{elements}]}}}}}}"
    );
    assert!(public.len() > MAX_FRAME_BYTES && public.len() < MAX_PRIVATE_FRAME_BYTES);
    let error = engine
        .social_public_frame(&endpoint, &endpoint, public.as_bytes(), now)
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("operation size limit"),
        "{error}"
    );
    engine
        .social_command(
            Command::ConfigureServices {
                settings: ServiceSettings {
                    directory: true,
                    mailbox: true,
                    ..Default::default()
                },
            },
            None,
            nucleus::execution::now(),
        )
        .await
        .unwrap();
    let elements = std::iter::repeat_n("\"\"", 400_000)
        .collect::<Vec<_>>()
        .join(",");
    let private =
        format!("{{\"request\":\"collect-private\",\"access\":{{\"envelopes\":[{elements}]}}}}");
    assert!(private.len() > 1024 * 1024 && private.len() < MAX_PRIVATE_FRAME_BYTES);
    let spent: (i64, i64) = store::sqlx::query_as(
        "SELECT bytes,work FROM social_service_budget WHERE source='*' AND direction='in'",
    )
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    let error = engine
        .social_public_frame(&endpoint, &endpoint, private.as_bytes(), now)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("limited"), "{error}");
    assert_eq!(
        store::sqlx::query_as::<_, (i64, i64)>(
            "SELECT bytes,work FROM social_service_budget WHERE source='*' AND direction='in'"
        )
        .fetch_one(&engine.store.pool)
        .await
        .unwrap(),
        spent
    );
    for table in [
        "social_document",
        "social_owner_control",
        "social_device_state",
        "social_message_event",
    ] {
        assert_eq!(
            store::sqlx::query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(&engine.store.pool)
                .await
                .unwrap(),
            0,
            "{table}"
        );
    }
}
