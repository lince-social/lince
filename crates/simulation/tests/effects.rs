use engine::actions::Action;
use nucleus::karma::Consequence;
use nucleus::projection::{Context, Incomplete, Window};
use nucleus::simulation::{Observation, ReplayStatus, Stop, Verdict};

fn run(work: impl Future<Output = ()> + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
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
fn queued_entry_captures_project_privately_and_replay_exactly() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let mut case = simulation::fixtures::daily();
        for invocation in &mut case.cells[0].seed {
            if let Action::CreateRecurrence {
                consequences,
                carry,
                ..
            } = &mut invocation.action
            {
                *consequences = vec![Consequence::CaptureEntry {
                    amount: nucleus::DecimalValue::parse_inferred("-3").unwrap(),
                    concept: None,
                }];
                *carry = Some("const:-3".into());
            }
        }
        let world = simulation::world::World::open(case.clone(), &directory.path().join("source"))
            .await
            .unwrap();
        let store = &world.nodes["a"].engine().store;
        let before = store.state_hash().await.unwrap();
        let projected = engine::projection::calculate(
            store,
            &Context {
                actor: None,
                window: Window {
                    from_ms: case.start_ms,
                    until_ms: case.end_ms + 1,
                    timezone: "UTC".into(),
                },
            },
            case.start_ms,
            None,
            &engine::projection::Metrics::default(),
        )
        .await
        .unwrap();
        assert_eq!(projected.incomplete, None);
        assert!(
            projected
                .spans
                .iter()
                .any(|span| matches!(span.cause, nucleus::simulation::Cause::Rule { .. }))
        );
        assert!(
            projected
                .spans
                .iter()
                .any(|span| span.quantity.value.to_string() == "-2")
        );
        assert_eq!(store.state_hash().await.unwrap(), before);
        store.pool.close().await;
        let original = directory.path().join("original");
        let completed = simulation::artifacts::execute(case, &original)
            .await
            .unwrap();
        assert_eq!(
            completed.result.verdict,
            Verdict::Passed,
            "{:?}",
            completed.findings
        );
        let loaded = simulation::artifacts::load(&original).unwrap();
        assert_eq!(
            loaded
                .events
                .iter()
                .filter(|event| matches!(
                    event.observation,
                    Observation::DatabaseEffect { ok: true, .. }
                ))
                .count(),
            4
        );
        let replay = simulation::artifacts::replay(&original, &directory.path().join("replay"))
            .await
            .unwrap();
        if !matches!(replay, ReplayStatus::Verified { .. }) {
            panic!(
                "{replay:?}; evidence retained in {}",
                directory.keep().display()
            );
        }
    });
}

#[test]
fn shell_effects_remain_unexecuted_and_report_incomplete() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("shell-must-not-run");
        let mut case = simulation::fixtures::daily();
        for invocation in &mut case.cells[0].seed {
            if let Action::CreateRecurrence { consequences, .. } = &mut invocation.action {
                *consequences = vec![Consequence::RunCommand {
                    command: format!("touch '{}'", marker.display()),
                }];
            }
        }
        let world = simulation::world::World::open(case.clone(), &directory.path().join("source"))
            .await
            .unwrap();
        let store = &world.nodes["a"].engine().store;
        let projected = engine::projection::calculate(
            store,
            &Context {
                actor: None,
                window: Window {
                    from_ms: case.start_ms,
                    until_ms: case.end_ms,
                    timezone: "UTC".into(),
                },
            },
            case.start_ms,
            None,
            &engine::projection::Metrics::default(),
        )
        .await
        .unwrap();
        assert_eq!(projected.incomplete, Some(Incomplete::ExternalEffects {}));
        store.pool.close().await;
        let result = simulation::artifacts::execute(case, &directory.path().join("run"))
            .await
            .unwrap();
        assert_eq!(result.result.verdict, Verdict::Inconclusive);
        assert!(matches!(result.result.stop, Stop::UnsupportedEffect { .. }));
        assert!(!marker.exists());
    });
}

#[test]
fn a_rule_gate_that_does_not_pass_is_a_complete_projection() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let mut case = simulation::fixtures::daily();
        for invocation in &mut case.cells[0].seed {
            if let Action::CreateRecurrence { gate, .. } = &mut invocation.action {
                *gate = Some(">1".into());
            }
        }
        let world = simulation::world::World::open(case.clone(), &directory.path().join("source"))
            .await
            .unwrap();
        let store = &world.nodes["a"].engine().store;
        let projected = engine::projection::calculate(
            store,
            &Context {
                actor: None,
                window: Window {
                    from_ms: case.start_ms,
                    until_ms: case.end_ms,
                    timezone: "UTC".into(),
                },
            },
            case.start_ms,
            None,
            &engine::projection::Metrics::default(),
        )
        .await
        .unwrap();
        assert_eq!(projected.incomplete, None);
        assert!(projected.spans.is_empty());
        store.pool.close().await;
    });
}

#[test]
fn copied_enabled_integrations_and_external_work_never_attach_live_services() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let export = directory.path().join("export");
        std::fs::create_dir(&export).unwrap();
        std::fs::write(export.join("keep.md"), "original export").unwrap();
        let marker = directory.path().join("plugin-must-not-run");
        let executable = directory.path().join("sentinel");
        std::fs::write(
            &executable,
            format!("#!/bin/sh\nprintf launched > '{}'\n", marker.display()),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let mut case = simulation::fixtures::daily();
        case.cells[0].seed.truncate(1);
        case.end_ms = case.start_ms;
        case.checks.truncate(1);
        case.checks[0].predicate = nucleus::simulation::Predicate::QuantityEquals {
            cell: "a".into(),
            record: "stock".into(),
            at_ms: case.end_ms,
            expected: nucleus::simulation::Quantity {
                value: nucleus::DecimalValue::parse_inferred("10").unwrap(),
                unit: None,
            },
        };
        let source = simulation::world::World::open(case.clone(), &directory.path().join("source"))
            .await
            .unwrap();
        let store = &source.nodes["a"].engine().store;
        let organ = store::organs::local(&store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let record = store::records::resolve(&store.pool, "stock")
            .await
            .unwrap()
            .unwrap()
            .uid;
        store::records::set_extension(
            &store.pool,
            &organ,
            "lince.file_sync",
            &serde_json::json!({"enabled":true,"path":export,"format":"markdown"}),
        )
        .await
        .unwrap();
        store::sqlx::query(
            "INSERT INTO plugin_package(id,digest,enabled) VALUES('sentinel','approved-digest',1)",
        )
        .execute(&store.pool)
        .await
        .unwrap();
        let binding = nucleus::new_uid("p");
        store::sqlx::query("INSERT INTO plugin_binding(uid,provider,name,settings,incoming_filter,direction,interval_seconds,enabled) VALUES(?,'sentinel.export','export',?,'[]','both',30,1)").bind(&binding).bind(serde_json::json!({"executable":executable,"export":export}).to_string()).execute(&store.pool).await.unwrap();
        store::sqlx::query("INSERT INTO plugin_outbox(uid,binding_uid,record_uid,content_digest,input,state,created_at) VALUES(?,?,?,'queued-content','{}','queued',0)").bind(nucleus::new_uid("p")).bind(&binding).bind(&record).execute(&store.pool).await.unwrap();
        let effect = nucleus::new_uid("fx");
        store::sqlx::query("INSERT INTO effect_queue(uid,kind,payload,status,created_at) VALUES(?,'command',?,'queued','2020-01-01T00:00:00Z')").bind(&effect).bind(serde_json::json!({"command":format!("'{}'",executable.display())}).to_string()).execute(&store.pool).await.unwrap();
        let input = directory.path().join("input.sqlite");
        store.snapshot_into(&input).await.unwrap();
        let before = simulation::artifacts::file_hash(&input).unwrap();
        let state = store.state_hash().await.unwrap();
        case.cells[0].database = Some(simulation::scenario::Database {
            file: "input.sqlite".into(),
            hash: before.clone(),
        });
        case.cells[0].seed.clear();
        let output = directory.path().join("run");
        let mut session = simulation::artifacts::Session::open(case, &output, directory.path())
            .await
            .unwrap();
        {
            let node = &session.world.nodes["a"];
            let runtime = node.cell.runtime();
            assert!(runtime.engine.plugins.get().is_none());
            assert!(runtime.engine.blobs.get().is_none());
            assert!(runtime.wire.read().await.is_none());
            assert!(runtime.information.is_none());
            assert!(runtime.fiote.is_none());
            let error = node
                .execution
                .scope(runtime.engine.plugin_command(
                    nucleus::plugins::Command::Invoke {
                        contribution: "sentinel.run".into(),
                        arguments: serde_json::json!({}),
                    },
                    None,
                ))
                .await
                .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("Executable plugins are unavailable during Simulation"),
                "{error}"
            );
            let error = node
                .execution
                .scope(runtime.engine.run_plugin_binding(&binding))
                .await
                .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("Plugin Sync is unavailable during Simulation"),
                "{error}"
            );
            let pending: i64 = store::sqlx::query_scalar(
                "SELECT count(*) FROM plugin_outbox WHERE state='queued'",
            )
            .fetch_one(&runtime.store.pool)
            .await
            .unwrap();
            assert_eq!(pending, 1);
            let status: String =
                store::sqlx::query_scalar("SELECT status FROM effect_queue WHERE uid=?")
                    .bind(&effect)
                    .fetch_one(&runtime.store.pool)
                    .await
                    .unwrap();
            assert_eq!(status, "queued");
        }
        while session.step().await.unwrap() {}
        let result = session.finish().await.unwrap();
        assert_eq!(result.result.verdict, Verdict::Inconclusive);
        assert!(matches!(result.result.stop, Stop::UnsupportedEffect { .. }));
        let replay = simulation::artifacts::replay(&output, &directory.path().join("replay"))
            .await
            .unwrap();
        assert!(
            matches!(replay, ReplayStatus::Verified { .. }),
            "{replay:?}"
        );
        assert!(!marker.exists());
        assert_eq!(
            std::fs::read_to_string(export.join("keep.md")).unwrap(),
            "original export"
        );
        assert_eq!(std::fs::read_dir(&export).unwrap().count(), 1);
        assert_eq!(simulation::artifacts::file_hash(&input).unwrap(), before);
        assert_eq!(store.state_hash().await.unwrap(), state);
        store.pool.close().await;
    });
}
