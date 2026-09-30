use engine::actions::Action;
use nucleus::karma::Consequence;
use nucleus::karma::scheduled_change::{BoundaryInput, DateInput, Purpose};
use nucleus::simulation::{
    CheckDefinition, Observation, Predicate, Quantity, ReplayStatus, Verdict,
};
use simulation::scenario::{Event, Input, Invocation, Scenario};

fn invocation(id: &str, action: Action) -> Invocation {
    Invocation {
        id: id.into(),
        actor: None,
        action,
    }
}

fn boundary(start: i64, purpose: Purpose, offset: i64, value: i128) -> BoundaryInput {
    BoundaryInput {
        purpose,
        date: DateInput::Instant {
            at_ms: start + offset,
        },
        target: "room".into(),
        consequences: vec![Consequence::SetQuantity {
            value: Some(store::exact::integer(value)),
        }],
    }
}

fn case() -> Scenario {
    let mut case = simulation::fixtures::daily();
    case.name = "paired-schedule".into();
    case.cells[0].seed = vec![
        invocation(
            "room",
            Action::CreateRecord {
                slug: Some("room".into()),
                kind: nucleus::RecordKind::Plain,
                head: "Room".into(),
                body: String::new(),
                quantity: 0.0,
            },
        ),
        invocation(
            "schedule",
            Action::SaveKarmaSchedule {
                schedule: None,
                expected_revision: None,
                name: "Room dates".into(),
                boundaries: vec![
                    boundary(case.start_ms, Purpose::Start, 5_000, -1),
                    boundary(case.start_ms, Purpose::End, 10_000, 0),
                ],
                request_id: "schedule".into(),
            },
        ),
    ];
    case.inputs.clear();
    case.end_ms = case.start_ms + 20_000;
    case.checks = vec![
        CheckDefinition {
            id: "once".into(),
            options: Default::default(),
            predicate: Predicate::OncePerOccurrence {},
        },
        CheckDefinition {
            id: "final".into(),
            options: Default::default(),
            predicate: Predicate::QuantityEquals {
                cell: "a".into(),
                record: "room".into(),
                at_ms: case.end_ms,
                expected: Quantity {
                    value: store::exact::zero(),
                    unit: None,
                },
            },
        },
    ];
    case
}

fn input(case: &mut Scenario, id: &str, offset: i64, event: Event) {
    case.inputs.push(Input {
        id: id.into(),
        cell: "a".into(),
        at_ms: case.start_ms + offset,
        event,
    });
}

async fn execute(case: Scenario) -> (tempfile::TempDir, simulation::artifacts::Run) {
    let directory = tempfile::tempdir().unwrap();
    let run = simulation::artifacts::execute(case, &directory.path().join("run"))
        .await
        .unwrap();
    assert_eq!(
        run.result.verdict,
        Verdict::Passed,
        "{:?} {:?}",
        run.result,
        run.findings
    );
    assert!(matches!(
        simulation::artifacts::replay(&run.directory, &directory.path().join("replay"))
            .await
            .unwrap(),
        ReplayStatus::Verified { .. }
    ));
    (directory, run)
}

#[tokio::test]
async fn saved_ranges_survive_restarts_and_end_the_current_quantity() {
    let mut case = case();
    input(&mut case, "restart-before", 3_000, Event::Restart {});
    input(&mut case, "restart-after-start", 6_000, Event::Restart {});
    input(
        &mut case,
        "edit-quantity",
        7_000,
        Event::Action {
            invocation: invocation(
                "edit-quantity",
                Action::SetQuantityExact {
                    target: "room".into(),
                    amount: "7".into(),
                },
            ),
        },
    );
    case.checks.push(CheckDefinition {
        id: "need-started".into(),
        options: Default::default(),
        predicate: Predicate::QuantityEquals {
            cell: "a".into(),
            record: "room".into(),
            at_ms: case.start_ms + 6_000,
            expected: Quantity {
                value: store::exact::integer(-1),
                unit: None,
            },
        },
    });
    execute(case).await;
}

#[tokio::test]
async fn an_expired_range_never_briefly_creates_a_need_after_downtime() {
    let mut case = case();
    input(&mut case, "offline", 1_000, Event::Online { online: false });
    input(&mut case, "online", 15_000, Event::Online { online: true });
    let (directory, run) = execute(case).await;
    let bundle = simulation::artifacts::load(&run.directory).unwrap();
    let room = bundle.events.iter().find_map(|event| match &event.observation { Observation::ActionAccepted { input, created } if input == "room" => created.as_deref(), _ => None }).unwrap();
    assert!(bundle.events.iter().all(|event| !matches!(&event.observation, Observation::CommittedQuantity { record, delta, .. } if record.as_str() == room && delta.is_negative())));
    assert!(directory.path().exists());
}

#[tokio::test]
async fn replacing_dates_and_cancelling_ranges_use_the_same_runtime_and_replay() {
    for cancel in [false, true] {
        let mut case = case();
        if cancel {
            input(
                &mut case,
                "cancel",
                2_000,
                Event::Action {
                    invocation: invocation(
                        "cancel",
                        Action::CancelKarmaSchedule {
                            schedule: "$schedule".into(),
                            expected_revision: 1,
                            request_id: "cancel".into(),
                        },
                    ),
                },
            );
        } else {
            let start = case.start_ms;
            input(
                &mut case,
                "replace",
                2_000,
                Event::Action {
                    invocation: invocation(
                        "replace",
                        Action::SaveKarmaSchedule {
                            schedule: Some("$schedule".into()),
                            expected_revision: Some(1),
                            name: "Moved dates".into(),
                            boundaries: vec![
                                boundary(start, Purpose::Start, 8_000, -2),
                                boundary(start, Purpose::End, 15_000, 0),
                            ],
                            request_id: "replace".into(),
                        },
                    ),
                },
            );
        }
        input(&mut case, "restart", 3_000, Event::Restart {});
        case.checks.push(CheckDefinition {
            id: "old-start-does-not-run".into(),
            options: Default::default(),
            predicate: Predicate::QuantityEquals {
                cell: "a".into(),
                record: "room".into(),
                at_ms: case.start_ms + 6_000,
                expected: Quantity {
                    value: store::exact::zero(),
                    unit: None,
                },
            },
        });
        if !cancel {
            case.checks.push(CheckDefinition {
                id: "new-start-runs".into(),
                options: Default::default(),
                predicate: Predicate::QuantityEquals {
                    cell: "a".into(),
                    record: "room".into(),
                    at_ms: case.start_ms + 9_000,
                    expected: Quantity {
                        value: store::exact::integer(-2),
                        unit: None,
                    },
                },
            });
        }
        execute(case).await;
    }
}
