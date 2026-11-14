use engine::Engine;
use protein::Protein;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Once,
        atomic::{AtomicU64, Ordering},
    },
};
use transport::{ClientMessage, LaneHub, ServerMessage, Session, SyncEvent};

static OBSERVER: Once = Once::new();
static MEASUREMENT: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static QUERIES: AtomicU64 = AtomicU64::new(0);
static REUSES: AtomicU64 = AtomicU64::new(0);
static READ_HELD_US: AtomicU64 = AtomicU64::new(0);

fn observe() {
    OBSERVER.call_once(|| {
        assert!(nucleus::performance::install(|reading| match reading {
            nucleus::performance::Reading::Count("cell-queries", count) => {
                QUERIES.fetch_add(count, Ordering::Relaxed);
            }
            nucleus::performance::Reading::Count("cell-query-batch-reuses", count) => {
                REUSES.fetch_add(count, Ordering::Relaxed);
            }
            nucleus::performance::Reading::Elapsed("record-access-read-hold-await", elapsed) => {
                READ_HELD_US.fetch_add(elapsed.as_micros() as u64, Ordering::Relaxed);
            }
            _ => {}
        }));
    });
}

fn query() -> Protein {
    serde_json::from_value(json!({
        "source": "record",
        "where": [{"kind_eq": "plain"}],
        "fields": ["uid", "head", "body", "quantity"],
        "order": [{"asc": "head"}],
        "limit": 300
    }))
    .unwrap()
}

async fn records(engine: &Engine, count: usize, body_bytes: usize) -> Vec<String> {
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let mut transaction = store::write_tx(&engine.store.pool).await.unwrap();
    let body = "x".repeat(body_bytes);
    let mut ids = Vec::new();
    for i in 0..count {
        let uid = nucleus::new_uid("r");
        store::records::create_with_uid_on(
            &mut transaction,
            &uid,
            store::records::NewRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: &format!("Private query fixture {i}"),
                body: &body,
                quantity: store::exact::zero(),
            },
            &organ,
            None,
        )
        .await
        .unwrap();
        ids.push(uid);
    }
    transaction.commit().await.unwrap();
    ids
}

fn rows(messages: Vec<ServerMessage>, snapshot: bool) -> BTreeMap<String, Vec<Value>> {
    messages
        .into_iter()
        .map(|message| match message {
            ServerMessage::Snapshot { id, rows } if snapshot => (id, rows),
            ServerMessage::Update { id, rows } if !snapshot => (id, rows),
            other => panic!("Unexpected subscription response: {other:?}"),
        })
        .collect()
}

async fn expected(
    engine: &Engine,
    subscriptions: &BTreeMap<String, Protein>,
) -> BTreeMap<String, Vec<Value>> {
    let mut result = BTreeMap::new();
    for (id, query) in subscriptions {
        result.insert(
            id.clone(),
            protein::execute(&engine.store, query).await.unwrap(),
        );
    }
    result
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refresh_and_fact_batches_preserve_aliases_query_shapes_history_and_freshness() {
    let _measurement = MEASUREMENT.lock().await;
    observe();
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let records = records(&engine, 6, 64).await;
    let mut session = Session::local(engine.clone(), Arc::new(LaneHub::new()), "batch");
    session
        .handle(ClientMessage::SyncHistoryPolicy {
            id: "history".into(),
            retention: nucleus::sync::Retention {
                enabled: true,
                ..Default::default()
            },
        })
        .await;
    let base = query();
    let mut subscriptions = BTreeMap::new();
    for i in 0..8 {
        subscriptions.insert(format!("alias-{i}"), base.clone());
    }
    let mut different = base.clone();
    different.filter = vec![protein::Predicate::UidEq(records[0].clone())];
    subscriptions.insert("filter".into(), different);
    let mut different = base.clone();
    different.fields = Some(vec!["uid".into()]);
    subscriptions.insert("fields".into(), different);
    let mut different = base.clone();
    different.limit = Some(1);
    subscriptions.insert("limit".into(), different);
    let mut different = base.clone();
    different.order = vec![protein::Order::Desc("head".into())];
    subscriptions.insert("order".into(), different);
    let mut different = base.clone();
    different.include.facts = Some(protein::FactsInclude { limit: 1 });
    subscriptions.insert("include".into(), different);
    subscriptions.insert(
        "aggregate".into(),
        serde_json::from_value(json!({
            "source": "record", "where": [{"kind_eq": "plain"}],
            "aggregate": {"op": "count", "by": "total"}
        }))
        .unwrap(),
    );
    subscriptions.insert(
        "source".into(),
        serde_json::from_value(json!({"source": "concept"})).unwrap(),
    );
    let original = expected(&engine, &subscriptions).await;
    for (id, protein) in &subscriptions {
        assert_eq!(
            rows(
                session
                    .handle(ClientMessage::Subscribe {
                        id: id.clone(),
                        protein: protein.clone(),
                    })
                    .await,
                true
            )[id],
            original[id]
        );
    }
    session
        .handle(ClientMessage::SyncForgetHistory { id: "clear".into() })
        .await;
    QUERIES.store(0, Ordering::Relaxed);
    REUSES.store(0, Ordering::Relaxed);
    assert_eq!(
        rows(session.on_sync_event(SyncEvent::Refresh).await, true),
        original
    );
    assert_eq!(QUERIES.load(Ordering::Relaxed), 8);
    assert_eq!(REUSES.load(Ordering::Relaxed), 7);
    let history = engine.sync_overview(None).await.unwrap().history;
    let mut aliases = std::collections::BTreeSet::new();
    assert_eq!(history.len(), subscriptions.len());
    for item in history {
        let nucleus::sync::Content::Query { subscription } = item.activity.instance.content else {
            panic!("Expected an interface subscription activity")
        };
        assert_eq!(item.summary.outcome, nucleus::sync::Outcome::Refreshed);
        assert_eq!(item.summary.count as usize, original[&subscription].len());
        aliases.insert(subscription);
    }
    assert_eq!(aliases, subscriptions.keys().cloned().collect());
    let edit = engine
        .act(
            engine::actions::Action::SetQuantityExact {
                target: records[0].clone(),
                amount: "7".into(),
            },
            None,
        )
        .await
        .unwrap();
    assert!(!edit.facts.is_empty());
    let changed = expected(&engine, &subscriptions).await;
    assert_ne!(changed["alias-0"], original["alias-0"]);
    QUERIES.store(0, Ordering::Relaxed);
    REUSES.store(0, Ordering::Relaxed);
    assert_eq!(
        rows(
            session
                .on_sync_event(SyncEvent::Fact(edit.facts[0].clone()))
                .await,
            false
        ),
        changed
    );
    assert_eq!(QUERIES.load(Ordering::Relaxed), 8);
    assert_eq!(REUSES.load(Ordering::Relaxed), 7);
    session
        .handle(ClientMessage::Unsubscribe {
            id: "alias-7".into(),
        })
        .await;
    subscriptions.remove("alias-7");
    let replacement = subscriptions["filter"].clone();
    session
        .handle(ClientMessage::Subscribe {
            id: "alias-0".into(),
            protein: replacement.clone(),
        })
        .await;
    subscriptions.insert("alias-0".into(), replacement);
    assert_eq!(
        rows(session.on_sync_event(SyncEvent::Refresh).await, true),
        expected(&engine, &subscriptions).await
    );
    engine.store.pool.close().await;
    QUERIES.store(0, Ordering::Relaxed);
    REUSES.store(0, Ordering::Relaxed);
    let failed = session.on_sync_event(SyncEvent::Refresh).await;
    assert_eq!(failed.len(), subscriptions.len());
    let ids: std::collections::BTreeSet<_> = failed
        .into_iter()
        .map(|message| match message {
            ServerMessage::Error { id, .. } => id,
            _ => panic!("A failed refresh must not publish a retained snapshot"),
        })
        .collect();
    assert_eq!(ids, subscriptions.keys().cloned().collect());
    assert_eq!(
        QUERIES.load(Ordering::Relaxed) as usize,
        subscriptions.len()
    );
    assert_eq!(REUSES.load(Ordering::Relaxed), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shared_results_remain_inside_the_session_and_current_visibility() {
    let _measurement = MEASUREMENT.lock().await;
    observe();
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let records = records(&engine, 2, 64).await;
    let owner = store::auth::ensure_role(&engine.store.pool, "owner")
        .await
        .unwrap()
        .to_string();
    for record in &records {
        store::visibility::grant(&engine.store.pool, "role", Some(&owner), record)
            .await
            .unwrap();
    }
    let role = store::auth::ensure_role(&engine.store.pool, "reader")
        .await
        .unwrap();
    let read = store::auth::ensure_permission(&engine.store.pool, "record", "read")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, read)
        .await
        .unwrap();
    let person =
        store::auth::create_person_login(&engine.store.pool, "Reader", "reader", "unused", role)
            .await
            .unwrap()
            .to_string();
    let grant = store::visibility::grant(&engine.store.pool, "actor", Some(&person), &records[0])
        .await
        .unwrap();
    let hub = Arc::new(LaneHub::new());
    let mut local = Session::local(engine.clone(), hub.clone(), "owner");
    let mut remote = Session::new(engine.clone(), hub, "reader", Some(person));
    for session in [&mut local, &mut remote] {
        for id in ["a", "b"] {
            rows(
                session
                    .handle(ClientMessage::Subscribe {
                        id: id.into(),
                        protein: query(),
                    })
                    .await,
                true,
            );
        }
    }
    for (session, count) in [(&mut local, 2), (&mut remote, 1)] {
        QUERIES.store(0, Ordering::Relaxed);
        let actual = rows(session.on_sync_event(SyncEvent::Refresh).await, true);
        assert_eq!(actual.len(), 2);
        assert_eq!(actual["a"].len(), count);
        assert_eq!(actual["a"], actual["b"]);
        assert_eq!(QUERIES.load(Ordering::Relaxed), 1);
    }
    engine
        .access_scope(true, async {
            store::sqlx::query("DELETE FROM visibility_rule WHERE uid = ?")
                .bind(grant)
                .execute(&engine.store.pool)
                .await?;
            Ok(())
        })
        .await
        .unwrap();
    let actual = rows(remote.on_sync_event(SyncEvent::Refresh).await, true);
    assert!(actual.values().all(Vec::is_empty));
    assert_eq!(actual.len(), 2);
    let actual = rows(local.on_sync_event(SyncEvent::Refresh).await, true);
    assert!(actual.values().all(|rows| rows.len() == 2));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refreshing_ephemeral_aliases_keeps_the_next_tick_quiet_and_replacement_current() {
    let _measurement = MEASUREMENT.lock().await;
    observe();
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let nearby = engine::wire::Nearby::default();
    nearby.observe("a".into(), "A".into(), "Laptop".into());
    engine.attach_nearby(nearby.clone());
    let mut session = Session::local(engine, Arc::new(LaneHub::new()), "nearby");
    for id in ["a", "b"] {
        rows(
            session
                .handle(ClientMessage::Subscribe {
                    id: id.into(),
                    protein: serde_json::from_value(json!({"source": "nearby"})).unwrap(),
                })
                .await,
            true,
        );
    }
    nearby.observe("b".into(), "B".into(), "Phone".into());
    QUERIES.store(0, Ordering::Relaxed);
    let refreshed = rows(session.on_sync_event(SyncEvent::Refresh).await, true);
    assert_eq!(refreshed.len(), 2);
    assert_eq!(refreshed["a"].len(), 2);
    assert_eq!(refreshed["a"], refreshed["b"]);
    assert_eq!(QUERIES.load(Ordering::Relaxed), 1);
    assert!(session.on_sync_event(SyncEvent::Ephemeral).await.is_empty());
    nearby.forget("b");
    let changed = rows(session.on_sync_event(SyncEvent::Ephemeral).await, false);
    assert_eq!(changed.len(), 2);
    assert_eq!(changed["a"].len(), 1);
    session
        .handle(ClientMessage::Subscribe {
            id: "b".into(),
            protein: query(),
        })
        .await;
    let refreshed = rows(session.on_sync_event(SyncEvent::Refresh).await, true);
    assert_eq!(refreshed["a"].len(), 1);
    assert!(refreshed["b"].is_empty());
    assert!(session.on_sync_event(SyncEvent::Ephemeral).await.is_empty());
}

#[cfg(target_os = "linux")]
fn resources() -> (u64, u64, u64) {
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap();
    let fields: Vec<_> = stat
        .rsplit_once(')')
        .unwrap()
        .1
        .split_whitespace()
        .collect();
    let cpu = fields[11].parse::<u64>().unwrap() + fields[12].parse::<u64>().unwrap();
    let io = std::fs::read_to_string("/proc/self/io").unwrap();
    let value = |name| {
        io.lines()
            .find_map(|line| line.strip_prefix(name))
            .unwrap()
            .trim()
            .parse::<u64>()
            .unwrap()
    };
    (cpu, value("rchar:"), value("syscr:"))
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "private file-backed query benchmark; run without concurrent compilation"]
async fn measure_subscription_refreshes() {
    let _measurement = MEASUREMENT.lock().await;
    observe();
    let directory = tempfile::tempdir().unwrap();
    let engine = Arc::new(
        Engine::open(&format!(
            "sqlite://{}",
            directory.path().join("cell.sqlite").display()
        ))
        .await
        .unwrap(),
    );
    records(&engine, 300, 4096).await;
    for (aliases, distinct) in [(8, false), (32, false), (32, true)] {
        let mut session = Session::local(engine.clone(), Arc::new(LaneHub::new()), "measure");
        for i in 0..aliases {
            let mut protein = query();
            if distinct {
                protein.limit = Some(300 - i);
            }
            let result = session
                .handle(ClientMessage::Subscribe {
                    id: i.to_string(),
                    protein,
                })
                .await;
            assert_eq!(
                rows(result, true)[&i.to_string()].len(),
                if distinct { 300 - i } else { 300 }
            );
        }
        session.on_sync_event(SyncEvent::Refresh).await;
        QUERIES.store(0, Ordering::Relaxed);
        REUSES.store(0, Ordering::Relaxed);
        READ_HELD_US.store(0, Ordering::Relaxed);
        let before = resources();
        let mut milliseconds = Vec::new();
        for _ in 0..12 {
            let start = std::time::Instant::now();
            let messages = session.on_sync_event(SyncEvent::Refresh).await;
            milliseconds.push(start.elapsed().as_secs_f64() * 1000.0);
            let result = rows(messages, true);
            assert_eq!(result.len(), aliases);
            for i in 0..aliases {
                assert_eq!(
                    result[&i.to_string()].len(),
                    if distinct { 300 - i } else { 300 }
                );
            }
        }
        let after = resources();
        println!(
            "{}",
            json!({
                "benchmark": "subscription-refresh", "optimized": !cfg!(debug_assertions),
                "aliases": aliases, "distinct": distinct, "records": 300, "body_bytes": 4096,
                "samples_ms": milliseconds, "queries": QUERIES.load(Ordering::Relaxed),
                "reuses": REUSES.load(Ordering::Relaxed),
                "read_held_us": READ_HELD_US.load(Ordering::Relaxed),
                "cpu_ticks": after.0 - before.0, "rchar": after.1 - before.1,
                "read_calls": after.2 - before.2
            })
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "manual isolated fixture; needs PARK_QUERY_FIXTURE and no concurrent compilation"]
async fn measure_frozen_query_definitions() {
    use sha2::{Digest, Sha256};
    let _measurement = MEASUREMENT.lock().await;
    observe();
    let root = std::path::PathBuf::from(std::env::var_os("PARK_QUERY_FIXTURE").unwrap());
    assert_eq!(
        std::fs::read_to_string(root.join("PRIVATE-PERFORMANCE-FIXTURE")).unwrap(),
        "isolated; no services or credentials\n"
    );
    let mut snapshots: Vec<_> = std::fs::read_dir(root.join("interface.snapshots"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "json"))
        .collect();
    snapshots.sort();
    let snapshot: Value =
        serde_json::from_slice(&std::fs::read(snapshots.last().unwrap()).unwrap()).unwrap();
    fn collect(value: &Value, queries: &mut Vec<Protein>) {
        match value {
            Value::Object(fields) => {
                if fields.contains_key("source") && fields.contains_key("where") {
                    let query: Protein = serde_json::from_value(value.clone()).unwrap();
                    if !queries.contains(&query) {
                        queries.push(query);
                    }
                }
                for child in fields.values() {
                    collect(child, queries);
                }
            }
            Value::Array(values) => {
                for child in values {
                    collect(child, queries);
                }
            }
            _ => {}
        }
    }
    let mut queries = Vec::new();
    collect(&snapshot, &mut queries);
    assert!(!queries.is_empty() && queries.len() <= 32);
    let saved_queries = queries.len();
    for source in [
        protein::Source::Record,
        protein::Source::Promise,
        protein::Source::Decision,
        protein::Source::Fact,
        protein::Source::Concept,
        protein::Source::Lingua,
        protein::Source::Assertion,
        protein::Source::Transfer,
        protein::Source::Auth,
        protein::Source::Karma,
        protein::Source::KarmaRule,
        protein::Source::Timeline,
        protein::Source::Entry,
        protein::Source::Frequency,
        protein::Source::Recurrence,
        protein::Source::Nearby,
    ] {
        queries.push(Protein {
            source,
            filter: Vec::new(),
            fields: None,
            include: Default::default(),
            aggregate: None,
            order: Vec::new(),
            limit: Some(300),
        });
    }
    let temporary = tempfile::tempdir().unwrap();
    let database = temporary.path().join("cell.sqlite");
    std::fs::copy(root.join("lince.db"), &database).unwrap();
    let engine = Arc::new(
        Engine::open(&format!("sqlite://{}", database.display()))
            .await
            .unwrap(),
    );
    let mut session = Session::local(engine, Arc::new(LaneHub::new()), "query-profile");
    for round in 0..2 {
        for (index, query) in queries.iter().enumerate() {
            let hash: String = Sha256::digest(serde_json::to_vec(query).unwrap())
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            let started = std::time::Instant::now();
            let result = session
                .handle(ClientMessage::Subscribe {
                    id: format!("query-{index}"),
                    protein: query.clone(),
                })
                .await;
            let elapsed = started.elapsed().as_secs_f64() * 1000.0;
            let (count, error) = match result.as_slice() {
                [ServerMessage::Snapshot { rows, .. }] => (Some(rows.len()), None),
                [ServerMessage::Error { code, .. }] => (None, code.clone()),
                _ => panic!("Missing query completion"),
            };
            println!(
                "{}",
                json!({"profile": "frozen-query", "round": round, "index": index, "hash": hash, "source": query.source, "fields": query.fields, "limit": query.limit, "rows": count, "error_code": error, "elapsed_ms": elapsed})
            );
            if index < saved_queries {
                assert!(count.is_some());
            }
        }
    }
}
