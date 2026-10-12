use engine::actions::Action;
use nucleus::simulation::{Predicate, ReplayStatus, Verdict};
use simulation::scenario::{Check, Scenario};

fn scenario() -> Scenario {
    simulation::fixtures::daily()
}

#[test]
fn refused_actions_require_an_explicit_expected_refusal() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let mut case = scenario();
        case.inputs.push(simulation::scenario::Input {
            id: "missing".into(),
            cell: "a".into(),
            at_ms: case.start_ms,
            event: simulation::scenario::Event::Action {
                invocation: simulation::scenario::Invocation {
                    id: "missing".into(),
                    actor: None,
                    action: Action::AddQuantity {
                        target: "does-not-exist".into(),
                        delta: 1.0,
                    },
                },
            },
        });
        let failed =
            simulation::artifacts::execute(case.clone(), &directory.path().join("failure"))
                .await
                .unwrap();
        assert_eq!(failed.result.verdict, Verdict::Failed);
        assert!(
            matches!(&failed.findings[0].witness, nucleus::simulation::Witness::RefusedAction { input, .. } if input == "missing")
        );
        case.checks.push(Check {
            options: Default::default(),
            id: "expected-missing".into(),
            predicate: Predicate::ExpectedRefusal {
                input: "missing".into(),
                refusal: nucleus::simulation::Refusal::UnknownRecord {
                    reference: "does-not-exist".into(),
                },
            },
        });
        let accepted = simulation::artifacts::execute(case, &directory.path().join("expected"))
            .await
            .unwrap();
        assert_eq!(
            accepted.result.verdict,
            Verdict::Passed,
            "{:?}",
            accepted.findings
        );
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
fn daily_karma_runs_through_real_cells_and_replays_every_identity_and_clock() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("original");
        let result = simulation::artifacts::execute(scenario(), &original)
            .await
            .unwrap();
        assert_eq!(
            result.result.verdict,
            Verdict::Passed,
            "{:?}",
            result.findings
        );
        let repeated = directory.path().join("repeated");
        let status = simulation::artifacts::replay(&original, &repeated)
            .await
            .unwrap();
        assert!(
            matches!(status, ReplayStatus::Verified { .. }),
            "{status:?}"
        );
        let bundle = simulation::artifacts::load(&original).unwrap();
        let stock = bundle
            .events
            .iter()
            .find_map(|event| match &event.observation {
                nucleus::simulation::Observation::ActionAccepted { input, created }
                    if input == "stock" =>
                {
                    created.as_ref()
                }
                _ => None,
            })
            .unwrap();
        let levels: Vec<_> =
            bundle
                .events
                .iter()
                .filter_map(|event| match &event.observation {
                    nucleus::simulation::Observation::CommittedQuantity {
                        record, after, ..
                    } if record.as_str() == stock => Some(after.value.to_string()),
                    _ => None,
                })
                .collect();
        assert_eq!(levels, ["10", "7", "4", "1", "-2"]);
    });
}

#[test]
fn witnessed_failure_survives_completion_and_replay_and_corruption_is_distinct() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("original");
        let mut case = scenario();
        case.checks.push(Check {
            options: Default::default(),
            id: "stock-never-negative".into(),
            predicate: Predicate::Nonnegative {
                cell: "a".into(),
                record: "stock".into(),
            },
        });
        let result = simulation::artifacts::execute(case, &original)
            .await
            .unwrap();
        assert_eq!(result.result.verdict, Verdict::Failed);
        assert_eq!(result.findings[0].check.id, "stock-never-negative");
        let status = simulation::artifacts::replay(&original, &directory.path().join("repeated"))
            .await
            .unwrap();
        assert!(
            matches!(status, ReplayStatus::Verified { reproduced_checks, .. } if reproduced_checks == ["stock-never-negative"])
        );
        std::fs::write(original.join("trace.jsonl"), "{\"unfinished\":").unwrap();
        assert!(matches!(
            simulation::artifacts::load(&original),
            Err(nucleus::simulation::ArtifactError::Corrupt { .. })
        ));
        std::fs::remove_file(original.join("result.json")).unwrap();
        assert!(matches!(
            simulation::artifacts::load(&original),
            Err(nucleus::simulation::ArtifactError::Incomplete { .. })
        ));
    });
}

#[test]
fn event_budget_does_not_claim_a_finished_projection() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let mut case = scenario();
        case.limits.steps = 1;
        let run = simulation::artifacts::execute(case, &directory.path().join("run"))
            .await
            .unwrap();
        assert_eq!(run.result.verdict, Verdict::Inconclusive);
        assert_eq!(run.result.stop, nucleus::simulation::Stop::EventBudget {});
    });
}

#[test]
fn four_cells_converge_after_duplicates_partition_loss_and_restart() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let case = simulation::fixtures::network(false);
        let original = directory.path().join("original");
        let result = simulation::artifacts::execute(case, &original)
            .await
            .unwrap();
        assert_eq!(
            result.result.verdict,
            Verdict::Passed,
            "{:?}",
            result.findings
        );
        let bundle = simulation::artifacts::load(&original).unwrap();
        assert!(bundle.events.iter().any(|event| matches!(
            event.observation,
            nucleus::simulation::Observation::MessageDropped { .. }
        )));
        assert!(bundle.events.iter().any(|event| matches!(
            event.observation,
            nucleus::simulation::Observation::MessageDelivered { imported: 0, .. }
        )));
        for peer in ["b", "c", "d"] {
            assert!(bundle.events.iter().any(|event| event.cell == peer && matches!(
                event.observation,
                nucleus::simulation::Observation::MessageDelivered { imported, .. } if imported > 0
            )));
        }
        assert!(bundle.events.iter().any(|event| event.cell == "b"
            && event.virtual_ms > bundle.scenario.start_ms + 25
            && matches!(
                event.observation,
                nucleus::simulation::Observation::MessageDelivered { imported: 0, .. }
            )));
        let status = simulation::artifacts::replay(&original, &directory.path().join("repeated"))
            .await
            .unwrap();
        assert!(
            matches!(status, ReplayStatus::Verified { .. }),
            "{status:?}"
        );
    });
}

#[test]
fn unknown_actions_and_unprovided_external_effect_adapters_are_rejected_before_startup() {
    assert!(
        serde_json::from_str::<simulation::scenario::Event>(r#"{"kind":"restart","ignored":true}"#)
            .is_err()
    );
    assert!(
        serde_json::from_str::<Action>(
            r#"{"action":"set-quantity-exact","target":"x","amount":"3","unknown":true}"#
        )
        .is_err()
    );
    let mut case = scenario();
    case.inputs.push(simulation::scenario::Input {
        id: "late".into(),
        at_ms: case.end_ms + 1,
        cell: "a".into(),
        event: simulation::scenario::Event::Restart {},
    });
    assert!(case.validate().is_err());
}

#[test]
fn a_saved_sqlite_seed_is_copied_and_the_input_database_is_unchanged() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("original");
        simulation::artifacts::execute(scenario(), &original)
            .await
            .unwrap();
        let file = original.join("seed/a.sqlite");
        let hash = simulation::artifacts::file_hash(&file).unwrap();
        let mut case = scenario();
        case.cells[0].seed.clear();
        case.cells[0].database = Some(simulation::scenario::Database {
            file: "seed/a.sqlite".into(),
            hash: hash.clone(),
        });
        let run = simulation::artifacts::execute_with_sources(
            case,
            &directory.path().join("copied"),
            &original,
        )
        .await
        .unwrap();
        assert_eq!(run.result.verdict, Verdict::Passed, "{:?}", run.findings);
        assert_eq!(simulation::artifacts::file_hash(&file).unwrap(), hash);
    });
}

#[test]
fn two_organs_enrol_discover_and_sync_through_four_cells() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("original");
        let mut session = simulation::artifacts::Session::open(
            simulation::fixtures::network(true),
            &original,
            std::path::Path::new("."),
        )
        .await
        .unwrap();
        while session.step().await.unwrap() {
            for cell in ["a", "b"] {
                assert!(
                    session
                        .world
                        .quantity(cell, "$second-stock")
                        .await
                        .unwrap()
                        .is_none()
                );
            }
        }
        for (host, member) in [("a", "b"), ("c", "d")] {
            let organ = session
                .world
                .resolve_reference(&format!("$cell:{host}:organ"));
            let node = &session.world.nodes[member];
            assert_eq!(
                store::organs::local(&node.engine().store.pool)
                    .await
                    .unwrap()
                    .unwrap()
                    .uid,
                organ
            );
            let cell = store::cells::local(&node.engine().store.pool)
                .await
                .unwrap()
                .unwrap();
            for name in [host, member] {
                let roster = session.world.nodes[name]
                    .engine()
                    .roster_of(&organ)
                    .await
                    .unwrap()
                    .unwrap();
                assert!(
                    roster
                        .roster
                        .cells
                        .iter()
                        .any(|entry| entry.cell_uid == cell.uid)
                );
            }
        }
        let result = session.finish().await.unwrap();
        assert_eq!(
            result.result.verdict,
            Verdict::Passed,
            "{:?}",
            result.findings
        );
        let status = simulation::artifacts::replay(&original, &directory.path().join("repeated"))
            .await
            .unwrap();
        assert!(
            matches!(status, ReplayStatus::Verified { .. }),
            "{status:?}"
        );
    });
}

#[test]
fn organ_sharing_requires_outbound_and_inbound_permission() {
    use nucleus::simulation::Refusal;
    use simulation::scenario::{Event, Input};

    run(async {
        for (sync_out, sync_in) in [(false, false), (true, false), (false, true), (true, true)] {
            let directory = tempfile::tempdir().unwrap();
            let mut case = simulation::fixtures::network(true);
            case.checks.retain(|check| {
                matches!(
                    check.predicate,
                    Predicate::FactChain {} | Predicate::NoUnexpectedRefusals {}
                )
            });
            case.inputs.retain(|input| {
                matches!(
                    input.event,
                    Event::Enrol { .. }
                        | Event::Discover { .. }
                        | Event::Pair { .. }
                        | Event::Action { .. }
                )
            });
            for input in &mut case.inputs {
                if let Event::Action { invocation } = &mut input.event {
                    if let Action::SetSyncPolicy {
                        sync_out: outgoing,
                        sync_in: incoming,
                        ..
                    } = &mut invocation.action
                    {
                        if input.cell == "a" {
                            *outgoing = sync_out;
                        } else {
                            *outgoing = true;
                            *incoming = sync_in;
                        }
                    }
                }
            }
            for (from, to) in [("a", "b"), ("a", "c"), ("a", "d"), ("c", "d"), ("c", "a")] {
                let id = format!("sync-{from}-{to}");
                case.inputs.push(Input {
                    id: id.clone(),
                    cell: from.into(),
                    at_ms: case.start_ms + 10,
                    event: Event::Sync {
                        peer: to.into(),
                        delay_ms: 1,
                        copies: 2,
                        duplicate_spacing_ms: 2,
                        drop: false,
                    },
                });
                if (from == "a" && matches!(to, "c" | "d") && !sync_in)
                    || (from == "c" && to == "a")
                {
                    for copy in 0..2 {
                        case.checks.push(Check {
                            id: format!("refused-{id}-{copy}"),
                            options: Default::default(),
                            predicate: Predicate::ExpectedMessageRefusal {
                                input: id.clone(),
                                copy,
                                refusal: Refusal::Forbidden {},
                            },
                        });
                    }
                }
            }
            let mut session = simulation::artifacts::Session::open(
                case,
                &directory.path().join("run"),
                std::path::Path::new("."),
            )
            .await
            .unwrap();
            while session.step().await.unwrap() {}
            for cell in ["a", "b"] {
                assert!(
                    session
                        .world
                        .quantity(cell, "$second-stock")
                        .await
                        .unwrap()
                        .is_none()
                );
                assert_eq!(
                    session
                        .world
                        .quantity(cell, "$stock")
                        .await
                        .unwrap()
                        .unwrap()
                        .1
                        .value
                        .to_string(),
                    "10"
                );
            }
            for cell in ["c", "d"] {
                let stock = session.world.quantity(cell, "$stock").await.unwrap();
                assert_eq!(
                    stock.is_some(),
                    sync_out && sync_in,
                    "{cell}: outbound={sync_out}, inbound={sync_in}"
                );
                if let Some((_, quantity)) = stock {
                    assert_eq!(quantity.value.to_string(), "10");
                }
                assert_eq!(
                    session
                        .world
                        .quantity(cell, "$second-stock")
                        .await
                        .unwrap()
                        .unwrap()
                        .1
                        .value
                        .to_string(),
                    "5"
                );
            }
            let result = session.finish().await.unwrap();
            assert_eq!(
                result.result.verdict,
                Verdict::Passed,
                "{:?}",
                result.findings
            );
        }
    });
}

#[test]
fn reviewed_transfer_settles_and_delivers_through_duplicates_and_restart() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("original");
        let result =
            simulation::artifacts::execute(simulation::fixtures::transfer::sale(), &original)
                .await
                .unwrap();
        assert_eq!(
            result.result.verdict,
            Verdict::Passed,
            "{:?} {:?}",
            result.result,
            result.findings
        );
        let bundle = simulation::artifacts::load(&original).unwrap();
        assert!(
            !bundle.events.iter().any(|event| matches!(
                event.observation,
                nucleus::simulation::Observation::ActionRefused { .. }
            )),
            "{:?}",
            bundle.events
        );
        assert_eq!(
            bundle
                .events
                .iter()
                .filter(|event| matches!(
                    event.observation,
                    nucleus::simulation::Observation::MessageRefused {
                        refusal: nucleus::simulation::Refusal::InvalidAction {},
                        ..
                    }
                ))
                .count(),
            1
        );
        for receipt in [false, true] {
            assert_eq!(bundle.events.iter().filter(|event| matches!(event.observation, nucleus::simulation::Observation::TransferReceived { receipt: observed, .. } if receipt == observed)).count(), 1);
        }
        let status = simulation::artifacts::replay(&original, &directory.path().join("repeated"))
            .await
            .unwrap();
        assert!(
            matches!(status, ReplayStatus::Verified { .. }),
            "{status:?}"
        );
        let mut unexpected = simulation::fixtures::transfer::sale();
        unexpected.checks.retain(|check| {
            !matches!(
                check.predicate,
                nucleus::simulation::Predicate::ExpectedMessageRefusal { .. }
            )
        });
        let rejected =
            simulation::artifacts::execute(unexpected, &directory.path().join("unexpected"))
                .await
                .unwrap();
        assert_eq!(rejected.result.verdict, Verdict::Failed);
        assert!(rejected.findings.iter().any(|finding| matches!(
            finding.witness,
            nucleus::simulation::Witness::RefusedMessage { .. }
        )));
    });
}

#[test]
fn transfer_item_visibility_survives_delivery_and_replay() {
    run(async {
        for mode in [
            nucleus::transfer_delivery::TransferDeliveryMode::Replicated,
            nucleus::transfer_delivery::TransferDeliveryMode::Hosted,
        ] {
            let directory = tempfile::tempdir().unwrap();
            let original = directory.path().join("visibility");
            let mut case = simulation::fixtures::transfer::visibility();
            for input in &mut case.inputs {
                if let simulation::scenario::Event::Action { invocation } = &mut input.event
                    && let Action::ConfigureTransferDelivery {
                        mode: configured, ..
                    } = &mut invocation.action
                {
                    *configured = mode;
                }
            }
            let mut session =
                simulation::artifacts::Session::open(case, &original, std::path::Path::new("."))
                    .await
                    .unwrap();
            while session.step().await.unwrap() {}
            let source = session.world.resolve_reference("$bike");
            let pool = &session.world.nodes["a"].engine().store.pool;
            let payloads: Vec<String> =
                store::sqlx::query_scalar("SELECT payload FROM transfer_delivery_outbox")
                    .fetch_all(pool)
                    .await
                    .unwrap();
            assert!(!payloads.is_empty());
            for payload in payloads {
                let envelope: nucleus::transfer_delivery::TransferEnvelopeV1 =
                    serde_json::from_str(&payload).unwrap();
                assert!(!payload.contains(&source), "{payload}");
                assert_eq!(envelope.projection["promises"][0]["title"], "City bike");
                assert!(envelope.facts.is_empty());
                assert!(envelope.action_intents.is_empty());
            }
            let result = session.finish().await.unwrap();
            assert_eq!(
                result.result.verdict,
                Verdict::Passed,
                "{:?}",
                result.findings
            );
            assert!(matches!(
                simulation::artifacts::replay(&original, &directory.path().join("replay"))
                    .await
                    .unwrap(),
                ReplayStatus::Verified { .. }
            ));
        }
    });
}

#[test]
fn five_second_frequency_runs_twelve_beats_and_keeps_its_renamed_binding() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let mut case = scenario();
        case.end_ms = case.start_ms + 60_000;
        if let Action::CreateFrequency {
            every, anchor_at, ..
        } = &mut case.cells[0].seed[1].action
        {
            *every = nucleus::karma::CadenceStep {
                seconds: 5,
                ..Default::default()
            };
            *anchor_at = Some(
                chrono::DateTime::from_timestamp_millis(case.start_ms + 5000)
                    .unwrap()
                    .to_rfc3339(),
            );
        }
        if let Predicate::QuantityEquals {
            expected, at_ms, ..
        } = &mut case.checks[0].predicate
        {
            expected.value = nucleus::DecimalValue::parse_inferred("-26").unwrap();
            *at_ms = case.end_ms;
        }
        case.inputs.push(simulation::scenario::Input {
            id: "rename-frequency".into(),
            cell: "a".into(),
            at_ms: case.start_ms + 15_000,
            event: simulation::scenario::Event::Action {
                invocation: simulation::scenario::Invocation {
                    id: "rename-frequency".into(),
                    actor: None,
                    action: Action::SetSlug {
                        target: "$daily".into(),
                        slug: Some("renamed-frequency".into()),
                    },
                },
            },
        });
        let original = directory.path().join("original");
        let result = simulation::artifacts::execute(case, &original)
            .await
            .unwrap();
        assert_eq!(
            result.result.verdict,
            Verdict::Passed,
            "{:?}",
            result.findings
        );
        let bundle = simulation::artifacts::load(&original).unwrap();
        let beats = bundle
            .events
            .iter()
            .filter(|event| {
                matches!(
                    event.observation,
                    nucleus::simulation::Observation::RuleApplication { .. }
                )
            })
            .count();
        assert_eq!(beats, 12);
    });
}

#[test]
fn replay_reports_the_first_changed_typed_field() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("original");
        simulation::artifacts::execute(scenario(), &original)
            .await
            .unwrap();
        let mut bundle = simulation::artifacts::load(&original).unwrap();
        let event = bundle
            .events
            .iter_mut()
            .find(|event| {
                matches!(
                    event.observation,
                    nucleus::simulation::Observation::State { .. }
                )
            })
            .unwrap();
        let sequence = event.sequence;
        event.observation = nucleus::simulation::Observation::State {
            hash: simulation::world::digest(&"altered state").unwrap(),
        };
        let trace: String = bundle
            .events
            .iter()
            .map(|event| format!("{}\n", serde_json::to_string(event).unwrap()))
            .collect();
        std::fs::write(original.join("trace.jsonl"), trace).unwrap();
        bundle.manifest.files.insert(
            "trace.jsonl".into(),
            simulation::artifacts::file_hash(&original.join("trace.jsonl")).unwrap(),
        );
        simulation::artifacts::atomic(&original.join("manifest.json"), &bundle.manifest).unwrap();
        let result = simulation::artifacts::replay(&original, &directory.path().join("repeated"))
            .await
            .unwrap();
        assert!(
            matches!(result, ReplayStatus::Diverged { sequence: at, field, .. } if at == sequence && field.ends_with("hash")),
            "first changed state must be identified"
        );
    });
}
