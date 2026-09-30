use engine::actions::Action;
use nucleus::simulation::{
    CheckDefinition, CheckOptions, CheckStatus, Comparison, CoverageKind, Evaluation, FailureMode,
    Predicate, Quantity, ReplayStatus, Stop, Verdict,
};
use simulation::scenario::{Event, Input, Invocation, Scenario};

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

fn limit(evaluation: Evaluation, comparison: Comparison, amount: &str) -> CheckDefinition {
    CheckDefinition {
        id: "stock-limit".into(),
        predicate: Predicate::Quantity {
            cell: "a".into(),
            record: "stock".into(),
            comparison,
            expected: Quantity {
                value: nucleus::DecimalValue::parse_inferred(amount).unwrap(),
                unit: None,
            },
        },
        options: CheckOptions {
            evaluation,
            ..Default::default()
        },
    }
}

fn case() -> Scenario {
    let mut case = simulation::fixtures::daily();
    case.cells[0].seed.truncate(1);
    if let Action::CreateRecord { quantity, .. } = &mut case.cells[0].seed[0].action {
        *quantity = 5.0;
    }
    case.end_ms = case.start_ms + 100;
    case.checks.clear();
    case.checking.on_failure = FailureMode::Continue;
    for (id, value) in [("drop", -1.0), ("restore", 5.0)] {
        case.inputs.push(Input {
            id: id.into(),
            cell: "a".into(),
            at_ms: case.start_ms + 50,
            event: Event::Action {
                invocation: Invocation {
                    id: id.into(),
                    actor: None,
                    action: Action::SetQuantity {
                        target: "stock".into(),
                        value,
                    },
                },
            },
        });
    }
    case
}

#[test]
fn check_schedules_leave_domain_events_and_final_state_unchanged() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let original = simulation::artifacts::execute(case(), &directory.path().join("none"))
            .await
            .unwrap();
        assert_eq!(original.result.verdict, Verdict::Unverified);
        let baseline = simulation::artifacts::load(&original.directory).unwrap();
        for (name, evaluation, verdict, kind) in [
            (
                "full",
                Evaluation::EveryChange,
                Verdict::Failed,
                CoverageKind::Continuous,
            ),
            (
                "continuous",
                Evaluation::EveryChange,
                Verdict::Failed,
                CoverageKind::Continuous,
            ),
            (
                "end",
                Evaluation::End,
                Verdict::Passed,
                CoverageKind::Instant,
            ),
            (
                "events",
                Evaluation::EveryEvents { every: 1 },
                Verdict::Failed,
                CoverageKind::Sampled,
            ),
            (
                "time",
                Evaluation::EveryDuration { millis: 7 },
                Verdict::Passed,
                CoverageKind::Sampled,
            ),
            (
                "quiet",
                Evaluation::At {
                    at_ms: case().start_ms + 75,
                },
                Verdict::Passed,
                CoverageKind::Instant,
            ),
        ] {
            let mut scenario = case();
            scenario
                .checks
                .push(limit(evaluation, Comparison::AtLeast, "0"));
            if name == "full" {
                for (id, predicate) in [
                    ("facts", Predicate::FactChain {}),
                    ("duplicates", Predicate::OncePerOccurrence {}),
                    ("refusals", Predicate::NoUnexpectedRefusals {}),
                ] {
                    scenario.checks.push(CheckDefinition {
                        id: id.into(),
                        predicate,
                        options: Default::default(),
                    });
                }
            }
            let result = simulation::artifacts::execute(scenario, &directory.path().join(name))
                .await
                .unwrap();
            assert_eq!(
                result.result.verdict, verdict,
                "{name}: {:?}",
                result.findings
            );
            assert_eq!(result.result.coverage[0].kind, kind);
            assert_eq!(
                result.result.final_state, baseline.result.final_state,
                "{name}"
            );
            assert_eq!(result.result.steps, baseline.result.steps, "{name}");
            let bundle = simulation::artifacts::load(&result.directory).unwrap();
            let evaluations = bundle
                .cost
                .checks
                .iter()
                .map(|check| check.evaluations)
                .sum::<u64>();
            assert_eq!(
                evaluations,
                bundle
                    .result
                    .coverage
                    .iter()
                    .map(|coverage| coverage.observations)
                    .sum::<u64>()
            );
            let total_micros = bundle.cost.execution_micros
                + bundle.cost.evidence_micros
                + bundle.cost.checking_micros;
            eprintln!(
                "{name}: {evaluations} evaluations, {} checking µs, {} execution µs, {} evidence µs, {:.1} events/s",
                bundle.cost.checking_micros,
                bundle.cost.execution_micros,
                bundle.cost.evidence_micros,
                bundle.result.steps as f64 * 1_000_000.0 / total_micros.max(1) as f64
            );
            assert_eq!(bundle.events, baseline.events, "{name}");
            if name == "continuous" {
                assert_eq!(result.findings.len(), 1);
                assert_eq!(result.findings[0].virtual_ms, case().start_ms + 50);
                assert!(matches!(
                    simulation::artifacts::replay(
                        &result.directory,
                        &directory.path().join("replay")
                    )
                    .await
                    .unwrap(),
                    ReplayStatus::Verified { .. }
                ));
            }
        }
        let mut disabled = case();
        let mut check = limit(Evaluation::EveryChange, Comparison::Greater, "100");
        check.options.enabled = false;
        disabled.checks.push(check);
        let result = simulation::artifacts::execute(disabled, &directory.path().join("disabled"))
            .await
            .unwrap();
        assert_eq!(result.result.verdict, Verdict::Unverified);
        assert_eq!(result.result.coverage[0].status, CheckStatus::Skipped);
        assert_eq!(result.result.final_state, baseline.result.final_state);
        simulation::artifacts::load(&result.directory).unwrap();
    });
}

#[test]
fn stop_budget_initial_bounds_and_missing_records_are_explicit() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let mut stopped = case();
        stopped.checking.on_failure = FailureMode::Stop;
        stopped
            .checks
            .push(limit(Evaluation::EveryChange, Comparison::AtLeast, "0"));
        let result = simulation::artifacts::execute(stopped, &directory.path().join("stop"))
            .await
            .unwrap();
        assert!(matches!(result.result.stop, Stop::CheckFailed { .. }));
        assert_eq!(result.result.inputs, 1);
        assert_eq!(result.result.coverage[0].status, CheckStatus::Failed);
        assert!(!result.result.coverage[0].complete);
        for (name, comparison, verdict) in [
            ("inclusive", Comparison::AtLeast, Verdict::Passed),
            ("strict", Comparison::Greater, Verdict::Failed),
        ] {
            let mut scenario = case();
            scenario.inputs.clear();
            scenario
                .checks
                .push(limit(Evaluation::EveryChange, comparison, "5"));
            let result = simulation::artifacts::execute(scenario, &directory.path().join(name))
                .await
                .unwrap();
            assert_eq!(result.result.verdict, verdict);
            if verdict == Verdict::Failed {
                assert_eq!(result.findings[0].virtual_ms, case().start_ms);
            }
        }
        let mut budget = case();
        budget.checking.evaluations = 1;
        budget
            .checks
            .push(limit(Evaluation::EveryChange, Comparison::AtLeast, "0"));
        let result = simulation::artifacts::execute(budget, &directory.path().join("budget"))
            .await
            .unwrap();
        assert_eq!(result.result.stop, Stop::CheckBudget {});
        assert_eq!(result.result.verdict, Verdict::Inconclusive);
        let mut deleted = case();
        deleted.inputs.truncate(1);
        deleted.inputs[0].event = Event::Action {
            invocation: Invocation {
                id: "drop".into(),
                actor: None,
                action: Action::DeleteRecord {
                    target: "stock".into(),
                },
            },
        };
        deleted
            .checks
            .push(limit(Evaluation::EveryChange, Comparison::AtLeast, "0"));
        let result = simulation::artifacts::execute(deleted, &directory.path().join("deleted"))
            .await
            .unwrap();
        assert_eq!(result.result.verdict, Verdict::Failed);
        assert!(matches!(
            result.findings[0].witness,
            nucleus::simulation::Witness::MissingRecord { .. }
        ));
        let mut scoped = case();
        let mut check = limit(Evaluation::EveryChange, Comparison::AtLeast, "0");
        check.options.window.from_ms = Some(scoped.start_ms + 75);
        check.options.window.until_ms = Some(scoped.start_ms + 80);
        scoped.checks.push(check);
        let result = simulation::artifacts::execute(scoped, &directory.path().join("window"))
            .await
            .unwrap();
        assert_eq!(result.result.verdict, Verdict::Passed);
        assert_eq!(
            result.result.coverage[0].last_evaluated_ms,
            Some(case().start_ms + 80)
        );
    });
}

#[test]
fn generic_quantity_checks_follow_existing_karma_and_saved_sets_are_versioned() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let mut scenario = simulation::fixtures::daily();
        scenario.checks = vec![limit(Evaluation::EveryChange, Comparison::AtLeast, "0")];
        let result =
            simulation::artifacts::execute(scenario.clone(), &directory.path().join("karma"))
                .await
                .unwrap();
        assert_eq!(result.result.verdict, Verdict::Failed);
        assert!(
            matches!(&result.findings[0].witness, nucleus::simulation::Witness::Quantity { observed, .. } if observed.value.to_string() == "-2")
        );
        let store = store::Store::open_memory().await.unwrap();
        let saved = store::simulation_checks::save(
            &store.pool,
            "stock",
            "Stock limits",
            0,
            &scenario.checks,
        )
        .await
        .unwrap();
        scenario.checks[0].options.enabled = false;
        let revised = store::simulation_checks::save(
            &store.pool,
            &saved.uid,
            &saved.name,
            saved.revision,
            &scenario.checks,
        )
        .await
        .unwrap();
        assert_eq!(revised.revision, 2);
        assert!(
            store::simulation_checks::save(
                &store.pool,
                &saved.uid,
                &saved.name,
                saved.revision,
                &saved.checks
            )
            .await
            .is_err()
        );
        assert!(saved.checks[0].options.enabled);
        assert_eq!(
            store::simulation_checks::list(&store.pool).await.unwrap(),
            [revised]
        );
        let bundle = simulation::artifacts::load(&result.directory).unwrap();
        assert!(bundle.scenario.checks[0].options.enabled);
    });
}
