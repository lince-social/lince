use nucleus::simulation::{ReplayStatus, Verdict};

#[test]
fn sibling_pairing_and_delayed_contact_bootstrap_recover_without_policy_conflicts() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    let directory = tempfile::tempdir().unwrap();
                    let path = directory.path().join("run");
                    let case = simulation::campaign::generate(1);
                    let result = simulation::artifacts::execute(case, &path).await.unwrap();
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
                });
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn competing_reservations_keep_the_winning_commitment_and_refuse_excess() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    for replicated in [false, true] {
                        let case =
                            simulation::fixtures::transfer::competing_reservations(replicated);
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
                        let node = &session.world.nodes["a"];
                        let balance = node
                            .execution
                            .scope(store::transfer_balances::read(
                                &node.engine().store.pool,
                                &session.world.resolve_reference("$stock-a"),
                            ))
                            .await
                            .unwrap();
                        assert!(balance.incomplete.is_empty());
                        assert!(balance.reserved.is_zero());
                        assert_eq!(balance.actual.to_f64(), 20.0);
                        let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer")
                            .fetch_one(&node.engine().store.pool)
                            .await
                            .unwrap();
                        assert_eq!(count, 1);
                        let run = session.finish().await.unwrap();
                        assert_eq!(
                            run.result.verdict,
                            Verdict::Passed,
                            "{:?} {:?}",
                            run.result,
                            run.findings
                        );
                        assert!(matches!(
                            simulation::artifacts::replay(&path, &directory.path().join("replay"))
                                .await
                                .unwrap(),
                            ReplayStatus::Verified { .. }
                        ));
                    }
                });
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn acceptance_workload_sizes_are_bounded_and_preserve_delivery_workflows() {
    for (cells, transfers) in [(4, 100), (32, 1000)] {
        let case = simulation::fixtures::transfer::volume(cells, transfers).unwrap();
        assert_eq!(case.cells.len(), cells);
        assert_eq!(case.inputs.iter().filter(|input| matches!(&input.event, simulation::scenario::Event::Action { invocation } if matches!(invocation.action, engine::actions::Action::CreateTransferDraft { .. }))).count(), transfers);
        case.validate().unwrap();
    }
    assert!(simulation::fixtures::transfer::volume(33, 1000).is_err());
    assert!(simulation::fixtures::transfer::volume(4, 1001).is_err());
    assert!(simulation::fixtures::transfer::volume(32, 1).is_err());
}

#[test]
fn declined_invitations_and_correction_retries_survive_real_restart_and_exact_replay() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    for replicated in [false, true] {
                        for (case, compensation_facts, private_record) in [
                            (
                                simulation::fixtures::transfer::declined_invitation(replicated),
                                0,
                                "stock-b",
                            ),
                            (
                                simulation::fixtures::transfer::private_correction_after_restart(
                                    replicated,
                                ),
                                1,
                                "money-b",
                            ),
                            (
                                simulation::fixtures::transfer::grouped_correction_after_restart(
                                    replicated,
                                ),
                                2,
                                "transport",
                            ),
                        ] {
                            let name = case.name.clone();
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
                            let count: i64 = store::sqlx::query_scalar(
                                "SELECT COUNT(*) FROM fact WHERE cause_kind = 'compensation'",
                            )
                            .fetch_one(pool)
                            .await
                            .unwrap();
                            assert_eq!(count, compensation_facts, "{name}");
                            let record = session
                                .world
                                .resolve_reference(&format!("${private_record}"));
                            let projections: Vec<String> = store::sqlx::query_scalar(
                                "SELECT projection FROM transfer_remote_reference",
                            )
                            .fetch_all(pool)
                            .await
                            .unwrap();
                            assert!(!projections.is_empty(), "{name}");
                            assert!(
                                projections
                                    .iter()
                                    .all(|projection| !projection.contains(&record)),
                                "{name}"
                            );
                            let origin = &session.world.nodes["a"].engine().store.pool;
                            assert!(
                                store::records::get(origin, &record)
                                    .await
                                    .unwrap()
                                    .is_none(),
                                "{name}"
                            );
                            if compensation_facts == 0 {
                                let invitation: String = store::sqlx::query_scalar(
                                    "SELECT status FROM transfer_invitation",
                                )
                                .fetch_one(origin)
                                .await
                                .unwrap();
                                assert_eq!(invitation, "rejected", "{name}");
                                let parties: i64 = store::sqlx::query_scalar(
                                    "SELECT COUNT(*) FROM transfer_party",
                                )
                                .fetch_one(origin)
                                .await
                                .unwrap();
                                assert_eq!(parties, 1, "{name}");
                                let applications: i64 = store::sqlx::query_scalar(
                                    "SELECT COUNT(*) FROM transfer_local_application",
                                )
                                .fetch_one(pool)
                                .await
                                .unwrap();
                                assert_eq!(applications, 0, "{name}");
                            }
                            let run = session.finish().await.unwrap();
                            if run.result.verdict != Verdict::Passed {
                                panic!(
                                    "{name}: {:?} {:?}; {}",
                                    run.result,
                                    run.findings,
                                    directory.keep().display()
                                );
                            }
                            assert!(
                                matches!(
                                    simulation::artifacts::replay(
                                        &path,
                                        &directory.path().join("replay")
                                    )
                                    .await
                                    .unwrap(),
                                    ReplayStatus::Verified { .. }
                                ),
                                "{name}"
                            );
                        }
                    }
                });
        })
        .unwrap()
        .join()
        .unwrap();
}
