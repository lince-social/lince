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
        assert!(matches!(
            simulation::artifacts::replay(&original, &directory.path().join("replay"))
                .await
                .unwrap(),
            ReplayStatus::Verified { .. }
        ));
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
