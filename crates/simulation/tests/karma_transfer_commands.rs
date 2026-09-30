use engine::actions::Action;
use nucleus::karma::{Cadence, Consequence};
use nucleus::simulation::{Predicate, ReplayStatus, Verdict};
use simulation::scenario::{Check, Event, Input, Invocation, Scenario};

fn run(test: impl std::future::Future<Output = ()> + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(test);
        })
        .unwrap()
        .join()
        .unwrap();
}

fn add(case: &mut Scenario, id: &str, event: Event) {
    let at_ms = case.inputs.iter().map(|input| input.at_ms).max().unwrap() + 20;
    case.inputs.push(Input {
        id: id.into(),
        cell: "b".into(),
        at_ms,
        event,
    });
}

fn action(id: &str, action: Action) -> Event {
    Event::Action {
        invocation: Invocation {
            id: id.into(),
            actor: None,
            action,
        },
    }
}

fn send(drop: bool, copies: u8) -> Event {
    Event::TransferDelivery {
        peer: "a".into(),
        delay_ms: 1,
        copies,
        duplicate_spacing_ms: 3,
        drop,
    }
}

fn case(replicated: bool) -> Scenario {
    let mut case = simulation::fixtures::transfer::independent_donation(replicated);
    case.name = format!("karma-transfer-command-{replicated}");
    let split = case
        .inputs
        .iter()
        .position(|input| input.id == "agreement-a-1")
        .unwrap();
    case.inputs.truncate(split);
    case.checks.retain(|check| {
        matches!(
            check.predicate,
            Predicate::FactChain {} | Predicate::NoUnexpectedRefusals {}
        )
    });
    add(
        &mut case,
        "rule",
        action(
            "rule",
            Action::CreateRecurrence {
                target: "$routes".into(),
                consequences: vec![Consequence::SetTransferAgreement {
                    transfer: "$routes".into(),
                    person: "$beto".into(),
                    level: Some(store::exact::integer(2)),
                    after_ms: None,
                }],
                condition: Some("@stock-b".into()),
                gate: Some("always".into()),
                carry: None,
                note: None,
                cadence: Cadence::once(),
                anchor_at: None,
                request_id: Some("rule".into()),
            },
        ),
    );
    let due_at = chrono::DateTime::from_timestamp_millis(case.inputs.last().unwrap().at_ms)
        .unwrap()
        .to_rfc3339();
    add(
        &mut case,
        "fire",
        action(
            "fire",
            Action::ApplyRecurrenceOccurrence {
                recurrence: "$rule".into(),
                due_at,
                amount: None,
                note: None,
            },
        ),
    );
    case
}

async fn execute(case: Scenario, expected: u8, cancelled: bool) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("run");
    let mut session = simulation::artifacts::Session::open(case, &path, std::path::Path::new("."))
        .await
        .unwrap();
    while session.step().await.unwrap() {}
    let transfer = session.world.resolve_reference("$routes");
    let person = session.world.resolve_reference("$beto");
    let level: i64 = store::sqlx::query_scalar("SELECT a.level FROM transfer_agreement a JOIN transfer_party p ON p.uid = a.party_uid WHERE a.transfer_uid = ? AND p.actor_uid = ?")
        .bind(&transfer).bind(&person).fetch_one(&session.world.nodes["a"].engine().store.pool).await.unwrap();
    let trace: Vec<_> = session
        .world
        .trace
        .iter()
        .filter(|event| {
            matches!(
                event.observation,
                nucleus::simulation::Observation::ActionRefused { .. }
                    | nucleus::simulation::Observation::RuleApplication { .. }
                    | nucleus::simulation::Observation::DatabaseEffect { .. }
            )
        })
        .collect();
    assert_eq!(level, i64::from(expected), "{trace:?}");
    let pool = &session.world.nodes["b"].engine().store.pool;
    let row: (i64, Option<String>, String) = store::sqlx::query_as("SELECT k.cancelled, k.dispatched_at, c.status FROM karma_transfer_command k JOIN transfer_remote_command c ON c.command_uid = k.command_uid")
        .fetch_one(pool).await.unwrap_or_else(|error| panic!("{error}; {trace:?}"));
    assert_eq!(row.0 != 0, cancelled);
    if cancelled {
        assert_eq!(row.1, None);
    } else {
        assert!(row.1.is_some());
    }
    let result = session.finish().await.unwrap();
    assert_eq!(
        result.result.verdict,
        Verdict::Passed,
        "{:?} {:?}; {}",
        result.result,
        result.findings,
        directory.path().display()
    );
    let replay = simulation::artifacts::replay(&path, &directory.path().join("replay"))
        .await
        .unwrap();
    if !matches!(replay, ReplayStatus::Verified { .. }) {
        panic!("{replay:?}; {}", directory.keep().display());
    }
}

#[test]
fn hosted_and_replicated_rules_deliver_signed_targets_after_loss_and_restart() {
    run(async {
        for replicated in [false, true] {
            let mut case = case(replicated);
            add(&mut case, "lost", send(true, 1));
            add(&mut case, "restart-before-retry", Event::Restart {});
            add(&mut case, "dispatch", send(false, 2));
            case.checks.push(Check {
                id: "duplicate-wire-refused".into(),
                options: Default::default(),
                predicate: Predicate::ExpectedMessageRefusal {
                    input: "dispatch".into(),
                    copy: 1,
                    refusal: nucleus::simulation::Refusal::InvalidAction {},
                },
            });
            execute(case, 2, false).await;
        }
    });
}

#[test]
fn pausing_cancels_an_unsent_remote_assignment() {
    run(async {
        for replicated in [false, true] {
            let mut case = case(replicated);
            add(
                &mut case,
                "pause",
                action(
                    "pause",
                    Action::SetRecurrencePaused {
                        recurrence: "$rule".into(),
                        expected_revision: 1,
                        request_id: "pause".into(),
                        paused: true,
                    },
                ),
            );
            add(&mut case, "restart", Event::Restart {});
            add(&mut case, "dispatch", send(false, 1));
            execute(case, 0, true).await;
        }
    });
}

#[test]
fn pausing_after_dispatch_preserves_the_issued_command_and_origin_receipt() {
    run(async {
        let mut case = case(false);
        add(&mut case, "issued-but-lost", send(true, 1));
        add(
            &mut case,
            "pause",
            action(
                "pause",
                Action::SetRecurrencePaused {
                    recurrence: "$rule".into(),
                    expected_revision: 1,
                    request_id: "pause".into(),
                    paused: true,
                },
            ),
        );
        add(&mut case, "retry-issued", send(false, 1));
        execute(case, 2, false).await;
    });
}

fn projection(case: &mut Scenario, id: &str) {
    let at_ms = case.inputs.iter().map(|input| input.at_ms).max().unwrap() + 20;
    case.inputs.push(Input {
        id: format!("enqueue-{id}"),
        cell: "a".into(),
        at_ms,
        event: action(
            &format!("enqueue-{id}"),
            Action::EnqueueTransferDelivery {
                transfer: "$routes".into(),
                delivery: "$policy-b".into(),
                person: Some("$ana".into()),
                request_id: format!("enqueue-{id}"),
            },
        ),
    });
    case.inputs.push(Input {
        id: format!("send-{id}"),
        cell: "a".into(),
        at_ms: at_ms + 20,
        event: Event::TransferDelivery {
            peer: "b".into(),
            delay_ms: 1,
            copies: 1,
            duplicate_spacing_ms: 0,
            drop: false,
        },
    });
}

#[test]
fn signed_remote_state_wakes_calculated_agreements_without_a_backing_record() {
    run(async {
        for replicated in [false, true] {
            let mut case = case(replicated);
            for input in &mut case.inputs {
                if let Event::Action { invocation } = &mut input.event
                    && let Action::CreateRecurrence {
                        consequences,
                        condition,
                        ..
                    } = &mut invocation.action
                {
                    *condition = Some("1 * (agreement_level(@{routes}, @{beto}) < 1) + 2 * (agreement_level(@{routes}, @{beto}) >= 1)".into());
                    if let Consequence::SetTransferAgreement { level, .. } = &mut consequences[0] {
                        *level = None;
                    }
                }
            }
            add(&mut case, "checked", send(false, 1));
            projection(&mut case, "checked");
            add(&mut case, "restart-after-calculation", Event::Restart {});
            add(&mut case, "agreed", send(false, 1));
            projection(&mut case, "agreed");
            add(&mut case, "no-op-at-maximum", send(false, 1));
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("run");
            let mut session =
                simulation::artifacts::Session::open(case, &path, std::path::Path::new("."))
                    .await
                    .unwrap();
            while session.step().await.unwrap() {}
            let transfer = session.world.resolve_reference("$routes");
            let person = session.world.resolve_reference("$beto");
            let state = session.world.nodes["b"]
                .engine()
                .read_karma_transfer_state(&transfer, None)
                .await
                .unwrap();
            assert_eq!(state.participants[&person].guard.level, 2);
            assert!(
                store::records::get(&session.world.nodes["b"].engine().store.pool, &transfer)
                    .await
                    .unwrap()
                    .is_none()
            );
            let count: i64 =
                store::sqlx::query_scalar("SELECT COUNT(*) FROM karma_transfer_command")
                    .fetch_one(&session.world.nodes["b"].engine().store.pool)
                    .await
                    .unwrap();
            assert_eq!(count, 2);
            let rule = session.world.resolve_reference("$rule");
            let cycles = session.world.control.cycles();
            assert!(
                cycles.iter().any(|cycle| cycle.rules.contains(&rule)),
                "{cycles:?}"
            );
            assert!(
                cycles
                    .iter()
                    .flat_map(|cycle| &cycle.steps)
                    .flat_map(|step| &step.transfer_changes)
                    .any(|change| change.cell == "a"
                        && change
                            .after
                            .participants
                            .get(&person)
                            .is_some_and(|state| state.guard.level == 2)),
                "{cycles:?}"
            );
            let result = session.finish().await.unwrap();
            assert_eq!(
                result.result.verdict,
                Verdict::Passed,
                "{:?} {:?}",
                result.result,
                result.findings
            );
            assert!(matches!(
                simulation::artifacts::replay(&path, &directory.path().join("replay"))
                    .await
                    .unwrap(),
                ReplayStatus::Verified { .. }
            ));
        }
    });
}

#[test]
fn a_single_cell_remote_proposal_reports_pending_origin_work_without_live_changes() {
    run(async {
        let mut case = case(false);
        case.inputs.retain(|input| !matches!(&input.event, Event::Action { invocation } if matches!(invocation.action, Action::CreateRecurrence { .. } | Action::ApplyRecurrenceOccurrence { .. })));
        let directory = tempfile::tempdir().unwrap();
        let mut session = simulation::artifacts::Session::open(
            case,
            &directory.path().join("source"),
            std::path::Path::new("."),
        )
        .await
        .unwrap();
        while session.step().await.unwrap() {}
        let transfer = session.world.resolve_reference("$routes");
        let person = session.world.resolve_reference("$beto");
        let engine = session.world.nodes["b"].engine();
        simulation::karma_preview::install(engine).unwrap();
        let before = engine.store.state_hash().await.unwrap();
        let request = engine::karma_preview::Request {
            proposals: vec![engine::karma_preview::ProposedRule {
                identity: None,
                rule: None,
                expected_revision: None,
                fields: [
                    format!("agreement_level(@{transfer}, @{person})"),
                    "always".into(),
                    format!("@{transfer}: agreement(@{person}, 2)"),
                ]
                .map(|source| nucleus::karma::rule_field::RuleFieldInput::Text { source }),
            }],
            limits: engine::karma_preview::Limits {
                horizon_ms: 1000,
                ..Default::default()
            },
            inputs: vec![engine::karma_preview::Input::Occurrence {
                after_ms: 0,
                proposal: 0,
            }],
            records: vec![transfer],
            quantity_basis: Default::default(),
            checks: Vec::new(),
            saved_checks: None,
            checks_start_ms: None,
            checking: Default::default(),
        };
        let report: engine::karma_preview::Report = serde_json::from_value(
            engine
                .act(Action::PreviewKarmaProposal { request }, None)
                .await
                .unwrap()
                .data
                .unwrap(),
        )
        .unwrap();
        assert!(report.incomplete, "{report:?}");
        assert!(
            report
                .unsupported
                .iter()
                .any(|reason| reason.contains("origin delivery adapter")),
            "{report:?}"
        );
        assert_eq!(
            report.final_values[0]
                .transfer
                .as_ref()
                .unwrap()
                .participants[&person]
                .guard
                .level,
            0
        );
        assert!(report.final_values[0].quantity.is_none());
        assert_eq!(engine.store.state_hash().await.unwrap(), before);
    });
}

#[test]
fn an_observed_level_change_cancels_the_original_unsent_dispatch() {
    run(async {
        for replicated in [false, true] {
            let mut case = case(replicated);
            case.checks
                .retain(|check| !matches!(check.predicate, Predicate::NoUnexpectedRefusals {}));
            add(
                &mut case,
                "manual-check",
                Event::TransferCommand {
                    peer: "a".into(),
                    transfer: "$routes".into(),
                    invocation: Invocation {
                        id: "manual-check".into(),
                        actor: None,
                        action: Action::AssignTransferAgreementLevel {
                            transfer: "$routes".into(),
                            expected_revision: 2,
                            request_id: "manual-check".into(),
                            person: Some("$beto".into()),
                            level: 1,
                            expected: None,
                            expected_state: None,
                        },
                    },
                    delay_ms: 1,
                    copies: 1,
                    duplicate_spacing_ms: 0,
                    drop: false,
                },
            );
            projection(&mut case, "manual-check");
            add(&mut case, "stale-dispatch", send(false, 1));
            execute(case, 1, true).await;
        }
    });
}

#[test]
fn retreat_can_keep_automation_enabled_or_pause_it_explicitly() {
    run(async {
        for replicated in [false, true] {
            for pause in [false, true] {
                let mut case = case(replicated);
                for input in &mut case.inputs {
                    if let Event::Action { invocation } = &mut input.event
                        && let Action::CreateRecurrence { condition, .. } = &mut invocation.action
                    {
                        *condition = Some("agreement_level(@{routes}, @{beto})".into());
                    }
                }
                add(&mut case, "initial-agreement", send(false, 1));
                projection(&mut case, "initial-agreement");
                if pause {
                    add(
                        &mut case,
                        "chosen-pause",
                        action(
                            "chosen-pause",
                            Action::SetRecurrencePaused {
                                recurrence: "$rule".into(),
                                expected_revision: 1,
                                request_id: "chosen-pause".into(),
                                paused: true,
                            },
                        ),
                    );
                }
                add(
                    &mut case,
                    "retreat",
                    Event::TransferCommand {
                        peer: "a".into(),
                        transfer: "$routes".into(),
                        invocation: Invocation {
                            id: "retreat".into(),
                            actor: None,
                            action: Action::AssignTransferAgreementLevel {
                                transfer: "$routes".into(),
                                expected_revision: 2,
                                request_id: "retreat".into(),
                                person: Some("$beto".into()),
                                level: 0,
                                expected: None,
                                expected_state: None,
                            },
                        },
                        delay_ms: 1,
                        copies: 1,
                        duplicate_spacing_ms: 0,
                        drop: false,
                    },
                );
                projection(&mut case, "retreat");
                add(&mut case, "automation-after-retreat", send(false, 1));
                projection(&mut case, "final");
                execute(case, if pause { 0 } else { 2 }, false).await;
            }
        }
    });
}
