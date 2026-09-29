use std::sync::{Arc, atomic::Ordering};

use engine::actions::Action;
use nucleus::projection::{Context, Status, Window};

#[test]
fn concurrent_calendars_share_one_window_without_recomputing_each_other() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let mut case = simulation::fixtures::daily();
        let now = chrono::Utc::now();
        case.start_ms = now.timestamp_millis();
        case.end_ms = case.start_ms + 4 * 86_400_000;
        for invocation in &mut case.cells[0].seed {
            match &mut invocation.action {
                Action::CreateFrequency { anchor_at, .. } => {
                    *anchor_at = Some((now + chrono::TimeDelta::days(1)).to_rfc3339())
                }
                Action::CreateRecurrence { anchor_at, .. } => *anchor_at = Some(now.to_rfc3339()),
                _ => {}
            }
        }
        case.checks.retain(|check| {
            matches!(
                check.predicate,
                nucleus::simulation::Predicate::FactChain {}
            )
        });
        let world = simulation::world::World::open(case, &directory.path().join("world"))
            .await
            .unwrap();
        let engine = world.nodes["a"].cell.runtime().engine.clone();
        let first = Context {
            actor: None,
            window: Window {
                from_ms: now.timestamp_millis(),
                until_ms: now.timestamp_millis() + 3 * 86_400_000,
                timezone: "UTC".into(),
            },
        };
        let mut second = first.clone();
        second.window.until_ms += 2 * 86_400_000;
        second.window.timezone = "Asia/Tokyo".into();
        engine.request_projection(first.clone()).await.unwrap();
        engine.request_projection(second.clone()).await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(60), async {
            loop {
                let now = chrono::Utc::now().timestamp_millis();
                let a = store::projection::read(&engine.store.pool, &first, now)
                    .await
                    .unwrap();
                let b = store::projection::read(&engine.store.pool, &second, now)
                    .await
                    .unwrap();
                if let (Some(a), Some(b)) = (a, b) {
                    assert!(matches!(a.status, Status::Ready { .. }), "{:?}", a.status);
                    assert!(matches!(b.status, Status::Ready { .. }), "{:?}", b.status);
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let snapshots = engine.projection.metrics.snapshots.load(Ordering::Relaxed);
        let steps = engine.projection.metrics.steps.load(Ordering::Relaxed);
        assert!(snapshots <= 2, "{snapshots} snapshots for two windows");
        for _ in 0..20 {
            engine.request_projection(first.clone()).await.unwrap();
            engine.request_projection(second.clone()).await.unwrap();
        }
        assert_eq!(
            engine.projection.metrics.snapshots.load(Ordering::Relaxed),
            snapshots
        );
        assert_eq!(
            engine.projection.metrics.steps.load(Ordering::Relaxed),
            steps
        );
        engine.store.pool.close().await;
    });
}

fn run(work: impl Future<Output = ()> + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(work)
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn calendar_combines_work_and_future_needs_and_reuses_disk_without_execution() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let case = simulation::fixtures::daily();
        let base = case.start_ms;
        let world = simulation::world::World::open(case, &directory.path().join("world"))
            .await
            .unwrap();
        let node = &world.nodes["a"];
        let engine = node.cell.runtime().engine.clone();
        let manual = node
            .execution
            .scope(engine.act(
                Action::CreateRecord {
                    slug: Some("manual".into()),
                    kind: nucleus::RecordKind::Plain,
                    head: "Manual work".into(),
                    body: String::new(),
                    quantity: -1.0,
                },
                None,
            ))
            .await
            .unwrap()
            .created
            .unwrap();
        node.execution
            .scope(store::records::set_extension(
                &engine.store.pool,
                &manual,
                "work",
                &serde_json::json!({"due":"2030-01-02"}),
            ))
            .await
            .unwrap();
        let context = Context {
            actor: None,
            window: Window {
                from_ms: base,
                until_ms: base + 5 * 86_400_000,
                timezone: "UTC".into(),
            },
        };
        let config =
            engine::karma_runtime::KarmaDeadlineDirectorConfig::for_host("projection-test".into())
                .unwrap();
        engine.install_karma_runtime_config(config.clone()).unwrap();
        store::projection::set_runtime(
            &engine.store.pool,
            config.projection_identity().unwrap().as_str(),
        )
        .await
        .unwrap();
        let before = engine.store.state_hash().await.unwrap();
        let metrics = engine::projection::Metrics::default();
        let computed = engine::projection::calculate(
            &engine.store,
            &context,
            base,
            Some(config.clone()),
            &metrics,
        )
        .await
        .unwrap();
        assert!(computed.incomplete.is_none(), "{:?}", computed.incomplete);
        assert_eq!(engine.store.state_hash().await.unwrap(), before);
        assert!(
            store::projection::publish(
                &engine.store.pool,
                &context,
                computed.source_revision,
                base,
                computed.expires_ms,
                None,
                &computed.spans
            )
            .await
            .unwrap()
        );
        let query = protein::calendar::query(
            context.window.clone(),
            vec![protein::Predicate::QuantityLt(store::exact::zero())],
        );
        let rows = node
            .execution
            .scope(protein::execute(&engine.store, &query))
            .await
            .unwrap();
        assert!(rows.iter().any(|row| row["uid"] == manual
            && row["origin"]["kind"] == "manual"
            && row["due_date"] == "2030-01-02"));
        assert!(
            rows.iter()
                .any(|row| row["record_uid"] == world.captured["stock"]
                    && row["origin"]["kind"] == "projection"
                    && row["quantity"] == "-2"
                    && row["start_date"] == "2030-01-05")
        );
        node.execution
            .scope(engine.request_projection(context.clone()))
            .await
            .unwrap();
        assert_eq!(
            engine.projection.metrics.snapshots.load(Ordering::Relaxed),
            0
        );
        assert_eq!(engine.projection.metrics.steps.load(Ordering::Relaxed), 0);
        assert_eq!(engine.projection.metrics.hits.load(Ordering::Relaxed), 1);
        let mut subset = context.clone();
        subset.window.from_ms += 86_400_000;
        subset.window.until_ms -= 86_400_000;
        assert!(
            store::projection::read(&engine.store.pool, &subset, base)
                .await
                .unwrap()
                .is_some()
        );
        engine.store.pool.close().await;
        let url = format!(
            "sqlite://{}",
            directory.path().join("world/working/a.sqlite").display()
        );
        let reopened = node
            .execution
            .scope(store::Store::open(&url))
            .await
            .unwrap();
        let engine = Arc::new(
            node.execution
                .scope(engine::Engine::new(reopened))
                .await
                .unwrap(),
        );
        engine.install_karma_runtime_config(config).unwrap();
        node.execution
            .scope(engine.request_projection(context.clone()))
            .await
            .unwrap();
        assert_eq!(engine.projection.metrics.hits.load(Ordering::Relaxed), 1);
        assert_eq!(
            engine.projection.metrics.snapshots.load(Ordering::Relaxed),
            0
        );
        assert!(matches!(
            store::projection::read(&engine.store.pool, &context, base)
                .await
                .unwrap()
                .unwrap()
                .status,
            Status::Ready { .. }
        ));
        let started = std::time::Instant::now();
        for _ in 0..100 {
            let cached = store::projection::read(&engine.store.pool, &context, base)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(cached.spans, computed.spans);
        }
        eprintln!(
            "100 warm SQLite reads: {:?}; zero snapshots and rule steps",
            started.elapsed()
        );
        let started = std::time::Instant::now();
        for _ in 0..100 {
            let cached = node
                .execution
                .scope(protein::execute(&engine.store, &query))
                .await
                .unwrap();
            assert_eq!(cached, rows);
        }
        eprintln!(
            "100 warm Protein Calendar reads: {:?}; zero snapshots and rule steps",
            started.elapsed()
        );
        assert_eq!(
            engine.projection.metrics.snapshots.load(Ordering::Relaxed),
            0
        );
        assert_eq!(engine.projection.metrics.steps.load(Ordering::Relaxed), 0);
        node.execution
            .scope(engine.act(
                Action::SetQuantity {
                    target: "stock".into(),
                    value: 20.0,
                },
                None,
            ))
            .await
            .unwrap();
        assert!(
            store::projection::read(&engine.store.pool, &context, base)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            !store::projection::publish(
                &engine.store.pool,
                &context,
                computed.source_revision,
                base,
                computed.expires_ms,
                None,
                &computed.spans
            )
            .await
            .unwrap()
        );
        engine.store.pool.close().await;
    });
}

#[test]
fn restricted_projection_does_not_reveal_private_rules_and_expiry_invalidates_cache() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let case = simulation::fixtures::daily();
        let base = case.start_ms;
        let world = simulation::world::World::open(case, &directory.path().join("world"))
            .await
            .unwrap();
        let store = &world.nodes["a"].engine().store;
        let context = Context {
            actor: Some("untrusted".into()),
            window: Window {
                from_ms: base,
                until_ms: base + 86_400_000,
                timezone: "UTC".into(),
            },
        };
        let metrics = engine::projection::Metrics::default();
        let result = engine::projection::calculate(store, &context, base, None, &metrics)
            .await
            .unwrap();
        assert!(result.spans.is_empty());
        assert_eq!(metrics.snapshots.load(Ordering::Relaxed), 0);
        assert!(result.incomplete.is_some());
        store::projection::publish(
            &store.pool,
            &context,
            result.source_revision,
            base,
            result.expires_ms,
            result.incomplete.as_ref(),
            &result.spans,
        )
        .await
        .unwrap();
        assert!(
            store::projection::read(&store.pool, &context, result.expires_ms)
                .await
                .unwrap()
                .is_none()
        );
        let mut another = context;
        another.actor = None;
        assert!(
            store::projection::read(&store.pool, &another, base)
                .await
                .unwrap()
                .is_none()
        );
        store.pool.close().await;
    });
}
