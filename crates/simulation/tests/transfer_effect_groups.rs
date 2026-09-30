use nucleus::simulation::{ReplayStatus, Verdict};

#[test]
fn received_group_corrections_preserve_unrelated_edits_and_replay_once() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    for replicated in [false, true] {
                        let mut case =
                            simulation::fixtures::transfer::grouped_needs(replicated, false, false);
                        for check in &mut case.checks {
                            if let nucleus::simulation::Predicate::Quantity {
                                record,
                                expected,
                                ..
                            } = &mut check.predicate
                            {
                                expected.value = nucleus::DecimalValue::parse_inferred(
                                    if record == "$transport" {
                                        "1.6"
                                    } else {
                                        "-0.4"
                                    },
                                )
                                .unwrap();
                            }
                        }
                        let correction = engine::actions::Action::CompensateTransferApplication {
                            application: "$receive-apples".into(),
                            person: "$beto".into(),
                            request_id: "correct-group".into(),
                        };
                        for (index, action) in [
                            engine::actions::Action::AddQuantityExact {
                                target: "$transport".into(),
                                delta: store::exact::integer(2),
                            },
                            correction.clone(),
                            correction,
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            let id = format!("correction-{index}");
                            case.inputs.push(simulation::scenario::Input {
                                id: id.clone(),
                                cell: "b".into(),
                                at_ms: case.start_ms + 2_000 + index as i64 * 10,
                                event: simulation::scenario::Event::Action {
                                    invocation: simulation::scenario::Invocation {
                                        id,
                                        actor: None,
                                        action,
                                    },
                                },
                            });
                        }
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
                        let pool = &session.world.nodes["b"].engine().store.pool;
                        for (record, expected) in [("stock-b", "-0.4"), ("transport", "1.6")] {
                            assert_eq!(
                                store::records::resolve(pool, record)
                                    .await
                                    .unwrap()
                                    .unwrap()
                                    .quantity
                                    .exact_numeric_cmp(
                                        nucleus::DecimalValue::parse_inferred(expected).unwrap()
                                    ),
                                std::cmp::Ordering::Equal
                            );
                        }
                        let count: i64 = store::sqlx::query_scalar(
                            "SELECT COUNT(*) FROM fact WHERE cause_kind = 'compensation'",
                        )
                        .fetch_one(pool)
                        .await
                        .unwrap();
                        assert_eq!(count, 2);
                        let run = session.finish().await.unwrap();
                        if run.result.verdict != Verdict::Passed {
                            panic!(
                                "{:?} {:?} at {}",
                                run.result,
                                run.findings,
                                directory.keep().display()
                            );
                        }
                        let replay =
                            simulation::artifacts::replay(&path, &directory.path().join("replay"))
                                .await
                                .unwrap();
                        assert!(
                            matches!(replay, ReplayStatus::Verified { .. }),
                            "{replay:?}"
                        );
                    }
                });
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn independent_private_groups_match_optional_scenarios_and_replay() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    for replicated in [false, true] {
                        for assumed in [false, true] {
                            for ride_first in [false, true] {
                                let case = simulation::fixtures::transfer::grouped_needs(
                                    replicated, assumed, ride_first,
                                );
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
                                let pool = &session.world.nodes["b"].engine().store.pool;
                                for name in ["stock-b", "transport"] {
                                    assert_eq!(
                                        store::records::resolve(pool, name)
                                            .await
                                            .unwrap()
                                            .unwrap()
                                            .quantity
                                            .is_zero(),
                                        true
                                    );
                                }
                                let count: i64 = store::sqlx::query_scalar(
                                    "SELECT COUNT(*) FROM transfer_private_effect",
                                )
                                .fetch_one(pool)
                                .await
                                .unwrap();
                                assert_eq!(count, if assumed { 0 } else { 4 });
                                let applications: i64 = store::sqlx::query_scalar(
                                    "SELECT COUNT(*) FROM transfer_local_application",
                                )
                                .fetch_one(pool)
                                .await
                                .unwrap();
                                assert_eq!(applications, if assumed { 0 } else { 2 });
                                let public: Vec<String> = store::sqlx::query_scalar(
                                    "SELECT projection FROM transfer_remote_reference",
                                )
                                .fetch_all(pool)
                                .await
                                .unwrap();
                                let transport = session.world.resolve_reference("$transport");
                                for projection in public {
                                    assert!(!projection.contains(&transport));
                                }
                                assert!(
                                    store::records::get(
                                        &session.world.nodes["a"].engine().store.pool,
                                        &transport
                                    )
                                    .await
                                    .unwrap()
                                    .is_none()
                                );
                                let run = session.finish().await.unwrap();
                                if run.result.verdict != Verdict::Passed {
                                    panic!(
                                        "{:?} {:?} at {}",
                                        run.result,
                                        run.findings,
                                        directory.keep().display()
                                    );
                                }
                                let replay = simulation::artifacts::replay(
                                    &path,
                                    &directory.path().join("replay"),
                                )
                                .await
                                .unwrap();
                                assert!(
                                    matches!(replay, ReplayStatus::Verified { .. }),
                                    "{replay:?}"
                                );
                            }
                        }
                    }
                });
        })
        .unwrap()
        .join()
        .unwrap();
}
