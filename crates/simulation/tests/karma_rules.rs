use engine::actions::Action;
use nucleus::karma::rule_field::RuleFieldInput;
use nucleus::simulation::{
    CheckDefinition, CheckOptions, Comparison, CycleKind, Evaluation, Predicate, Quantity,
    ReplayStatus, Stop, Verdict,
};
use simulation::scenario::{Event, Input, Invocation, Scenario};

fn invocation(id: &str, action: Action) -> Invocation {
    Invocation {
        id: id.into(),
        actor: None,
        action,
    }
}

fn rule(id: &str, source: &str, target: &str) -> Invocation {
    invocation(
        id,
        Action::SaveKarmaRule {
            identity: None,
            rule: None,
            expected_revision: None,
            fields: [source, "always", target].map(|source| RuleFieldInput::Text {
                source: source.into(),
            }),
            request_id: id.into(),
        },
    )
}

fn feedback(settling: bool) -> Scenario {
    let mut case = simulation::fixtures::daily();
    case.name = "rule-feedback".into();
    case.cells[0].seed.clear();
    case.checks.clear();
    case.inputs.clear();
    case.end_ms = case.start_ms + 1000;
    for name in ["left", "right"] {
        case.cells[0].seed.push(invocation(
            name,
            Action::CreateRecord {
                slug: Some(name.into()),
                kind: nucleus::RecordKind::Plain,
                head: name.into(),
                body: String::new(),
                quantity: 0.0,
            },
        ));
    }
    case.cells[0].seed.extend([
        rule(
            "A",
            if settling {
                "(@left + 1) * (@left < 5) + 5 * (@left >= 5)"
            } else {
                "@left + 1"
            },
            "@right",
        ),
        rule("B", if settling { "@right" } else { "@right + 1" }, "@left"),
    ]);
    case.inputs.push(Input {
        id: "start".into(),
        cell: "a".into(),
        at_ms: case.start_ms + 1,
        event: Event::Action {
            invocation: invocation(
                "start",
                Action::AddQuantityExact {
                    target: "left".into(),
                    delta: nucleus::DecimalValue::parse_inferred("1").unwrap(),
                },
            ),
        },
    });
    case
}

#[test]
fn available_restrictions_use_the_actual_loan_adjustment_inside_feedback() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    let mut case = simulation::fixtures::transfer::temporary_loan(false, false);
                    case.checks.clear();
                    let at = case.end_ms + 100;
                    case.end_ms = at + 1_000;
                    case.inputs.extend([
                        Input {
                            id: "loan-rule".into(),
                            cell: "b".into(),
                            at_ms: at,
                            event: Event::Action {
                                invocation: rule("loan-rule", "@stock-b - 1", "@stock-b"),
                            },
                        },
                        Input {
                            id: "start-feedback".into(),
                            cell: "b".into(),
                            at_ms: at + 1,
                            event: Event::Action {
                                invocation: invocation(
                                    "start-feedback",
                                    Action::AddQuantityExact {
                                        target: "stock-b".into(),
                                        delta: store::exact::integer(-1),
                                    },
                                ),
                            },
                        },
                    ]);
                    case.inputs.sort_by_key(|input| input.at_ms);
                    case.checks.push(CheckDefinition {
                        id: "available-stock".into(),
                        options: CheckOptions {
                            quantity: nucleus::simulation::QuantityBasis::Available,
                            evaluation: Evaluation::EveryChange,
                            window: nucleus::simulation::checks::CheckWindow {
                                from_ms: Some(at),
                                until_ms: None,
                            },
                            ..Default::default()
                        },
                        predicate: Predicate::Quantity {
                            cell: "b".into(),
                            record: "stock-b".into(),
                            comparison: Comparison::AtLeast,
                            expected: Quantity {
                                value: store::exact::integer(-2),
                                unit: None,
                            },
                        },
                    });
                    let directory = tempfile::tempdir().unwrap();
                    let path = directory.path().join("run");
                    let mut session = simulation::artifacts::Session::open(
                        case,
                        &path,
                        std::path::Path::new("."),
                    )
                    .await
                    .unwrap();
                    while session.step().await.unwrap() {}
                    assert_eq!(session.world.control.position().0, 1);
                    let engine = session.world.nodes["b"].engine();
                    let stored = store::records::resolve(&engine.store.pool, "stock-b")
                        .await
                        .unwrap()
                        .unwrap();
                    assert_eq!(
                        stored.quantity.exact_numeric_cmp(store::exact::integer(-2)),
                        std::cmp::Ordering::Equal
                    );
                    let available = session
                        .world
                        .quantity_with_basis(
                            "b",
                            "stock-b",
                            nucleus::simulation::QuantityBasis::Available,
                        )
                        .await
                        .unwrap()
                        .unwrap()
                        .1;
                    assert_eq!(
                        available.value.exact_numeric_cmp(store::exact::integer(-3)),
                        std::cmp::Ordering::Equal
                    );
                    let run = session.finish().await.unwrap();
                    assert_eq!(
                        run.result.stop,
                        Stop::CheckFailed {
                            check: "available-stock".into()
                        }
                    );
                    assert!(matches!(
                        simulation::artifacts::replay(&path, &directory.path().join("replay"))
                            .await
                            .unwrap(),
                        ReplayStatus::Verified { .. }
                    ));
                });
        })
        .unwrap()
        .join()
        .unwrap();
}

#[tokio::test]
async fn evaluation_budget_stops_rising_feedback_inside_one_action_and_replays() {
    let mut case = feedback(false);
    case.limits.rule_evaluations = 300;
    let directory = tempfile::tempdir().unwrap();
    let run = simulation::artifacts::execute(case, &directory.path().join("run"))
        .await
        .unwrap();
    assert_eq!(run.result.stop, Stop::RuleEvaluationBudget {});
    assert_eq!(run.result.rule_evaluations, 300);
    assert_eq!(run.result.stopped_at_ms, 1_893_456_000_001);
    assert_eq!(run.result.cycles[0].kind, CycleKind::Feedback);
    assert_eq!(run.result.cycles[0].rules.len(), 2);
    let bundle = simulation::artifacts::load(&run.directory).unwrap();
    assert!(bundle.events.iter().all(|event| !matches!(
        event.observation,
        nucleus::simulation::Observation::RuleApplication {
            status: nucleus::simulation::ApplicationStatus::Failed,
            ..
        }
    )));
    assert!(matches!(
        simulation::artifacts::replay(&run.directory, &directory.path().join("replay"))
            .await
            .unwrap(),
        ReplayStatus::Verified { .. }
    ));
}

#[tokio::test]
async fn selected_cycle_restriction_stops_at_the_first_closed_causal_chain() {
    let mut case = feedback(false);
    case.checks.push(CheckDefinition {
        id: "no-feedback".into(),
        options: Default::default(),
        predicate: Predicate::NoRuleCycles {
            include_timed_recurrence: false,
        },
    });
    let directory = tempfile::tempdir().unwrap();
    let run = simulation::artifacts::execute(case, &directory.path().join("run"))
        .await
        .unwrap();
    assert_eq!(
        run.result.stop,
        Stop::CheckFailed {
            check: "no-feedback".into()
        }
    );
    assert_eq!(run.result.rule_evaluations, 3);
    assert_eq!(run.result.verdict, Verdict::Failed);
    assert!(matches!(
        run.findings[0].witness,
        nucleus::simulation::Witness::RuleCycle { .. }
    ));
}

#[tokio::test]
async fn quantity_restrictions_stop_after_the_violating_committed_transaction() {
    let mut case = feedback(false);
    case.checks.push(CheckDefinition {
        id: "right-cap".into(),
        options: CheckOptions {
            evaluation: Evaluation::EveryChange,
            ..Default::default()
        },
        predicate: Predicate::Quantity {
            cell: "a".into(),
            record: "right".into(),
            comparison: Comparison::AtMost,
            expected: Quantity {
                value: nucleus::DecimalValue::parse_inferred("4").unwrap(),
                unit: None,
            },
        },
    });
    let directory = tempfile::tempdir().unwrap();
    let run = simulation::artifacts::execute(case, &directory.path().join("run"))
        .await
        .unwrap();
    assert_eq!(
        run.result.stop,
        Stop::CheckFailed {
            check: "right-cap".into()
        }
    );
    assert_eq!(run.result.rule_evaluations, 5);
    assert_eq!(run.result.verdict, Verdict::Failed);
    assert!(
        matches!(&run.findings[0].witness, nucleus::simulation::Witness::Quantity { observed, .. } if observed.value.to_string() == "6")
    );
}

#[tokio::test]
async fn feedback_that_finishes_is_reported_as_settled() {
    let directory = tempfile::tempdir().unwrap();
    let run = simulation::artifacts::execute(feedback(true), &directory.path().join("run"))
        .await
        .unwrap();
    assert_eq!(run.result.stop, Stop::HorizonReached {});
    assert_eq!(run.result.cycles[0].kind, CycleKind::SettledFeedback);
}

#[tokio::test]
async fn daily_changes_are_recurrence_and_the_default_cycle_restriction_allows_them() {
    let mut case = simulation::fixtures::daily();
    case.checks.push(CheckDefinition {
        id: "no-feedback".into(),
        options: Default::default(),
        predicate: Predicate::NoRuleCycles {
            include_timed_recurrence: false,
        },
    });
    let directory = tempfile::tempdir().unwrap();
    let run = simulation::artifacts::execute(case, &directory.path().join("run"))
        .await
        .unwrap();
    assert_eq!(run.result.verdict, Verdict::Passed, "{:?}", run.findings);
    assert!(
        run.result
            .cycles
            .iter()
            .all(|cycle| cycle.kind == CycleKind::TimedRecurrence)
    );
    assert!(!run.result.cycles.is_empty());
}

#[tokio::test]
async fn wall_time_replay_uses_the_saved_execution_checkpoint() {
    let mut case = feedback(false);
    case.limits.wall_time_ms = Some(1);
    let directory = tempfile::tempdir().unwrap();
    let run = simulation::artifacts::execute(case, &directory.path().join("run"))
        .await
        .unwrap();
    assert_eq!(run.result.stop, Stop::WallTimeBudget {});
    assert!(run.result.execution_checkpoints > 0);
    assert!(matches!(
        simulation::artifacts::replay(&run.directory, &directory.path().join("replay"))
            .await
            .unwrap(),
        ReplayStatus::Verified { .. }
    ));
}

#[tokio::test]
async fn restart_keeps_the_shared_rule_evaluation_total() {
    let mut case = simulation::fixtures::daily();
    let start = case.start_ms;
    case.limits.rule_evaluations = 2;
    case.checks.clear();
    case.inputs.push(Input {
        id: "restart".into(),
        cell: "a".into(),
        at_ms: start + 86_400_001,
        event: Event::Restart {},
    });
    let directory = tempfile::tempdir().unwrap();
    let run = simulation::artifacts::execute(case, &directory.path().join("run"))
        .await
        .unwrap();
    assert_eq!(run.result.stop, Stop::RuleEvaluationBudget {});
    assert_eq!(run.result.rule_evaluations, 2);
}

#[tokio::test]
async fn intermediate_writes_in_one_rule_transaction_do_not_break_a_quantity_check() {
    let mut case = simulation::fixtures::daily();
    let Action::CreateRecurrence { consequences, .. } = &mut case.cells[0].seed[2].action else {
        unreachable!()
    };
    *consequences = [-11, 11]
        .map(|delta| nucleus::karma::Consequence::AddQuantity {
            delta: Some(nucleus::DecimalValue::from_mantissa(0, delta).unwrap()),
        })
        .into();
    case.checks = vec![CheckDefinition {
        id: "never-negative".into(),
        options: Default::default(),
        predicate: Predicate::Nonnegative {
            cell: "a".into(),
            record: "stock".into(),
        },
    }];
    let directory = tempfile::tempdir().unwrap();
    let run = simulation::artifacts::execute(case, &directory.path().join("run"))
        .await
        .unwrap();
    assert_eq!(run.result.stop, Stop::HorizonReached {});
    assert_eq!(run.result.verdict, Verdict::Passed, "{:?}", run.findings);
    assert!(run.findings.is_empty());
}

#[tokio::test]
async fn oscillating_values_expose_the_same_causal_feedback() {
    let mut case = feedback(false);
    case.cells[0].seed.truncate(2);
    case.cells[0]
        .seed
        .push(rule("oscillate", "1 - @left", "@left"));
    case.limits.rule_evaluations = 12;
    let directory = tempfile::tempdir().unwrap();
    let run = simulation::artifacts::execute(case, &directory.path().join("run"))
        .await
        .unwrap();
    assert_eq!(run.result.stop, Stop::RuleEvaluationBudget {});
    assert_eq!(run.result.rule_evaluations, 12);
    assert_eq!(run.result.cycles[0].kind, CycleKind::Feedback);
    assert_eq!(run.result.cycles[0].rules.len(), 1);
    let values: Vec<_> = run.result.cycles[0]
        .steps
        .iter()
        .flat_map(|step| &step.changes)
        .map(|change| change.after.value.to_string())
        .collect();
    assert_eq!(values, ["0", "1"]);
}

#[tokio::test]
async fn a_wall_limit_during_seed_import_saves_interrupted_evidence_and_replays() {
    let sources = tempfile::tempdir().unwrap();
    std::fs::write(
        sources.path().join("seed.lingua"),
        "Stock (@stock: 10) {\n}\n",
    )
    .unwrap();
    let mut case = simulation::fixtures::daily();
    case.cells[0].seed.clear();
    case.cells[0].lingua = simulation::lingua::package(sources.path()).unwrap();
    case.checks.clear();
    case.limits.wall_time_ms = Some(1);
    let output = tempfile::tempdir().unwrap();
    let run = simulation::artifacts::execute_with_sources(
        case,
        &output.path().join("run"),
        sources.path(),
    )
    .await
    .unwrap();
    assert_eq!(run.result.stop, Stop::WallTimeBudget {});
    let bundle = simulation::artifacts::load(&run.directory).unwrap();
    assert!(bundle.events.iter().any(|event| matches!(
        event.observation,
        nucleus::simulation::Observation::LinguaInterrupted { .. }
    )));
    assert!(matches!(
        simulation::artifacts::replay(&run.directory, &output.path().join("replay"))
            .await
            .unwrap(),
        ReplayStatus::Verified { .. }
    ));
}

#[tokio::test]
async fn pausing_and_stopping_inside_feedback_preserves_commits_and_replays() {
    let directory = tempfile::tempdir().unwrap();
    let case = feedback(false);
    let control =
        nucleus::execution::control::Control::new(case.limits.rule_evaluations, None, None);
    let mut session = simulation::artifacts::Session::open_interruptible(
        case,
        &directory.path().join("run"),
        directory.path(),
        control.clone(),
    )
    .await
    .unwrap();
    let (step, evaluations) = tokio::join!(session.step(), async {
        loop {
            if control.position().0 >= 10 {
                break;
            }
            tokio::task::yield_now().await;
        }
        control.pause();
        let evaluations = control.position().0;
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert_eq!(control.position().0, evaluations);
        control.request_cancel();
        evaluations
    });
    step.unwrap();
    let run = session.finish().await.unwrap();
    assert_eq!(run.result.stop, Stop::Cancelled {});
    assert!(run.result.execution_interrupted);
    assert_eq!(run.result.rule_evaluations, evaluations);
    assert!(evaluations >= 10);
    let bundle = simulation::artifacts::load(&run.directory).unwrap();
    assert!(bundle.events.iter().any(|event| matches!(
        event.observation,
        nucleus::simulation::Observation::ActionInterrupted { .. }
    )));
    assert!(matches!(
        simulation::artifacts::replay(&run.directory, &directory.path().join("replay"))
            .await
            .unwrap(),
        ReplayStatus::Verified { .. }
    ));
}

#[tokio::test]
async fn available_quantity_limits_stop_at_the_first_violating_commit() {
    let mut case = feedback(false);
    case.checks = vec![CheckDefinition {
        id: "available-cap".into(),
        options: CheckOptions {
            quantity: nucleus::simulation::QuantityBasis::Available,
            evaluation: Evaluation::EveryChange,
            ..Default::default()
        },
        predicate: Predicate::Quantity {
            cell: "a".into(),
            record: "right".into(),
            comparison: Comparison::AtMost,
            expected: Quantity {
                value: nucleus::DecimalValue::parse_inferred("4").unwrap(),
                unit: None,
            },
        },
    }];
    let directory = tempfile::tempdir().unwrap();
    let run = simulation::artifacts::execute(case, &directory.path().join("run"))
        .await
        .unwrap();
    assert_eq!(
        run.result.stop,
        Stop::CheckFailed {
            check: "available-cap".into()
        }
    );
    assert_eq!(run.result.rule_evaluations, 5);
}

#[tokio::test]
async fn a_cycle_restriction_is_independent_of_the_quantity_basis_control() {
    let mut case = feedback(false);
    case.checks = vec![CheckDefinition {
        id: "no-feedback".into(),
        options: CheckOptions {
            quantity: nucleus::simulation::QuantityBasis::Available,
            ..Default::default()
        },
        predicate: Predicate::NoRuleCycles {
            include_timed_recurrence: false,
        },
    }];
    let directory = tempfile::tempdir().unwrap();
    let run = simulation::artifacts::execute(case, &directory.path().join("run"))
        .await
        .unwrap();
    assert_eq!(
        run.result.stop,
        Stop::CheckFailed {
            check: "no-feedback".into()
        }
    );
    assert_eq!(run.result.rule_evaluations, 3);
}
