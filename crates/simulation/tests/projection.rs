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
        authorize_projection(&world.nodes["a"]).await;
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

async fn authorize_projection(node: &simulation::world::Node) {
    let engine = node.cell.runtime().engine.clone();
    let directory = tempfile::tempdir().unwrap();
    let node_id = engine::wire::node_secret(&directory.path().join("node.key"))
        .unwrap()
        .public()
        .to_string();
    node.execution
        .scope(async {
            let organ = store::organs::local(&engine.store.pool)
                .await
                .unwrap()
                .unwrap();
            let cell = store::cells::local(&engine.store.pool)
                .await
                .unwrap()
                .unwrap();
            let root = engine::trust::Signer::from_bytes(
                &organ.uid,
                engine::roster::ROOT_KEY_ID,
                [17; 32],
            );
            engine.publish_root_key(&root).await.unwrap();
            engine
                .publish_roster(
                    &root,
                    vec![engine::roster::CellEntry {
                        cell_uid: cell.uid,
                        node_id,
                        label: "Projection fixture".into(),
                        operational_key: engine.local_organ_public_key().await.unwrap().unwrap(),
                        sealing_key: None,
                        front_door: false,
                        capabilities: engine::roster::full_capabilities(),
                    }],
                )
                .await
                .unwrap();
            assert!(engine.karma_device_execution().await.unwrap().executing);
        })
        .await;
}

#[test]
fn unavailable_rule_execution_preserves_manual_schedule_and_reports_incomplete() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let case = simulation::fixtures::daily();
        let base = case.start_ms;
        let world = simulation::world::World::open(case, &directory.path().join("world"))
            .await
            .unwrap();
        let node = &world.nodes["a"];
        let engine = node.cell.runtime().engine.clone();
        let at = chrono::DateTime::from_timestamp_millis(base + 600_000)
            .unwrap()
            .to_rfc3339();
        node.execution
            .scope(store::records::set_extension(
                &engine.store.pool,
                &world.captured["stock"],
                "work",
                &serde_json::json!({"start":at}),
            ))
            .await
            .unwrap();
        let context = Context {
            actor: None,
            window: Window {
                from_ms: base,
                until_ms: base + 3_600_000,
                timezone: "UTC".into(),
            },
        };
        let metrics = engine::projection::Metrics::default();
        let calculated =
            engine::projection::calculate(&engine.store, &context, base, None, &metrics)
                .await
                .unwrap();
        assert!(matches!(
            calculated.incomplete,
            Some(nucleus::projection::Incomplete::UnavailableRuntime {})
        ));
        assert_eq!(calculated.schedule.len(), 1);
        assert!(matches!(
            calculated.schedule[0].cause,
            nucleus::simulation::Cause::Seed {}
        ));
        assert_eq!(calculated.schedule[0].time.from_ms, base + 600_000);
        assert_eq!(metrics.steps.load(Ordering::Relaxed), 0);
        assert!(calculated.spans.is_empty());
        engine.store.pool.close().await;
    });
}

#[test]
fn recurring_schedule_uses_cost_even_when_the_occurrence_changes_no_quantity() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let mut case = simulation::fixtures::daily();
        let base = case.start_ms;
        for invocation in &mut case.cells[0].seed {
            if let Action::CreateRecurrence { consequences, .. } = &mut invocation.action {
                *consequences = vec![nucleus::karma::Consequence::AddQuantity {
                    delta: Some(nucleus::DecimalValue::parse_inferred("0").unwrap()),
                }];
            }
        }
        let world = simulation::world::World::open(case, &directory.path().join("world"))
            .await
            .unwrap();
        let node = &world.nodes["a"];
        authorize_projection(node).await;
        let engine = node.cell.runtime().engine.clone();
        let stock = store::records::list_all(&engine.store.pool)
            .await
            .unwrap()
            .into_iter()
            .find(|record| record.slug.as_deref() == Some("stock"))
            .unwrap();
        node.execution
            .scope(store::records::set_extension(
                &engine.store.pool,
                &stock.uid,
                "work",
                &serde_json::json!({"estimate_min":10}),
            ))
            .await
            .unwrap();
        let context = Context {
            actor: None,
            window: Window {
                from_ms: base,
                until_ms: base + 4 * 86_400_000,
                timezone: "UTC".into(),
            },
        };
        let config =
            engine::karma_runtime::KarmaDeadlineDirectorConfig::for_host("schedule-test".into())
                .unwrap();
        let metrics = engine::projection::Metrics::default();
        let calculated =
            engine::projection::calculate(&engine.store, &context, base, Some(config), &metrics)
                .await
                .unwrap();
        assert!(
            calculated.incomplete.is_none(),
            "{:?}",
            calculated.incomplete
        );
        assert!(calculated.schedule.len() >= 3, "{:?}", calculated.schedule);
        assert!(
            calculated
                .schedule
                .iter()
                .all(|entry| entry.time.until_ms == Some(entry.time.from_ms + 600_000))
        );
        assert!(
            calculated
                .schedule
                .iter()
                .all(|entry| matches!(entry.cause, nucleus::simulation::Cause::Rule { .. }))
        );
        let ids: std::collections::HashSet<_> =
            calculated.schedule.iter().map(|entry| &entry.id).collect();
        assert_eq!(ids.len(), calculated.schedule.len());
        assert!(
            calculated
                .spans
                .iter()
                .all(|span| span.record.as_str() != stock.uid)
        );
        assert_eq!(
            store::records::get(&engine.store.pool, &stock.uid)
                .await
                .unwrap()
                .unwrap()
                .quantity
                .to_string(),
            "10"
        );
        engine.store.pool.close().await;
    });
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
        authorize_projection(node).await;
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
            store::projection::publish_schedule(
                &engine.store.pool,
                &context,
                computed.source_revision,
                base,
                computed.expires_ms,
                None,
                &computed.spans,
                &computed.schedule
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
            assert_eq!(cached.schedule, computed.schedule);
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
