use nucleus::simulation::{ReplayStatus, Verdict};

#[test]
fn donation_updates_both_private_stocks_without_a_return_contribution() {
    check_donation(false, false);
}

#[test]
fn donation_recovers_from_lost_acknowledgements_and_duplicate_messages() {
    check_donation(true, false);
}

#[test]
fn source_free_donation_applies_only_after_the_owner_selects_a_record() {
    check_donation(false, true);
}

fn check_donation(recovery: bool, unbound: bool) {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    for replicated in [false, true] {
                        let directory = tempfile::tempdir().unwrap();
                        let original = directory.path().join("original");
                        let case = if unbound {
                            simulation::fixtures::transfer::donation_without_private_source(replicated)
                        } else if recovery {
                            simulation::fixtures::transfer::donation_with_lost_acknowledgements(replicated)
                        } else {
                            simulation::fixtures::transfer::independent_donation(replicated)
                        };
                        let review_at = case.inputs.iter().find(|input| input.id == "apply-own-stock").map(|input| input.at_ms);
                        let mut session = simulation::artifacts::Session::open(case, &original, std::path::Path::new(".")).await.unwrap();
                        if let Some(at) = review_at {
                            while session.world.next_ms().is_some_and(|time| time < at) { assert!(session.step().await.unwrap()); }
                            let node = &session.world.nodes["a"];
                            let probe = nucleus::execution::Execution::restore(node.execution.snapshot()).unwrap();
                            assert_eq!(store::records::resolve(&node.engine().store.pool, "stock-a").await.unwrap().unwrap().quantity.to_f64(),30.0);
                            let (binding,state):(Option<String>,String) = store::sqlx::query_as("SELECT record_uid,state FROM promise WHERE transfer_uid IS NOT NULL").fetch_one(&node.engine().store.pool).await.unwrap();
                            assert_eq!(binding,None);
                            assert_eq!(state,"active");
                            let before = node.engine().store.state_hash().await.unwrap();
                            let action = |quantity| engine::actions::Action::BeginTransferSettlement {
                                transfer:session.world.resolve_reference("$routes"),occurrence:session.world.resolve_reference("$activate"),
                                person:session.world.resolve_reference("$ana"),expected_revision:2,expected_remaining_quantity:10.0,canonical_quantity:quantity,request_id:"settle".into(),
                            };
                            let replay = probe.scope(node.engine().act(action(10.0),None)).await.unwrap();
                            assert_eq!(replay.created,Some(session.world.resolve_reference("$settle")));
                            let error = probe.scope(node.engine().act(action(1.0),None)).await.unwrap_err();
                            assert!(error.to_string().contains("reused with different"),"{error}");
                            let forbidden = engine::actions::Action::ApplyTransferApplication {
                                expected_effects_hash: None,
                                transfer:session.world.resolve_reference("$routes"),handoff:session.world.resolve_reference("$settle"),
                                person:session.world.resolve_reference("$ana"),local_record:session.world.resolve_reference("$beto"),
                                expected_formula_hash:nucleus::transfer::occurrence_application_formula_hash("-incoming()"),expected_formula_version:0,
                                expected_local_delta:-10.0,expected_local_cumulative_before:0.0,request_id:"foreign-record".into(),
                            };
                            let error=probe.scope(node.engine().act(forbidden,None)).await.unwrap_err();
                            assert!(error.to_string().contains("originating in this Cell"),"{error}");
                            assert_eq!(before,node.engine().store.state_hash().await.unwrap());
                        }
                        while session.step().await.unwrap() {}
                        let result = session.finish().await.unwrap();
                        if result.result.verdict != Verdict::Passed {
                            panic!(
                                "{:?} {:?}; evidence retained in {}",
                                result.result,
                                result.findings,
                                directory.keep().display()
                            );
                        }
                        let origin = store::Store::open_existing_durable(&format!(
                            "sqlite://{}",
                            original.join("working/a.sqlite").display()
                        ))
                        .await
                        .unwrap();
                        let occurrence: String =
                            store::sqlx::query_scalar("SELECT uid FROM transfer_occurrence")
                                .fetch_one(&origin.pool)
                                .await
                                .unwrap();
                        let progress = store::transfers::occurrence_settlement_progress(
                            &origin.pool,
                            &occurrence,
                        )
                        .await
                        .unwrap()
                        .unwrap();
                        assert_eq!(progress.settled_quantity, 10.0);
                        assert_eq!(progress.remaining_quantity, 0.0);
                        let promises: i64 = store::sqlx::query_scalar(
                            "SELECT COUNT(*) FROM promise WHERE transfer_uid IS NOT NULL",
                        )
                        .fetch_one(&origin.pool)
                        .await
                        .unwrap();
                        assert_eq!(promises, 1);
                        let handoff: String = store::sqlx::query_scalar(
                            "SELECT state FROM transfer_application_handoff",
                        )
                        .fetch_one(&origin.pool)
                        .await
                        .unwrap();
                        assert_eq!(handoff, "accepted");
                        assert!(
                            store::records::resolve(&origin.pool, "stock-b")
                                .await
                                .unwrap()
                                .is_none()
                        );
                        origin.pool.close().await;
                        let recipient = store::Store::open_existing_durable(&format!(
                            "sqlite://{}",
                            original.join("working/b.sqlite").display()
                        ))
                        .await
                        .unwrap();
                        let applications: i64 = store::sqlx::query_scalar(
                            "SELECT COUNT(*) FROM transfer_local_application",
                        )
                        .fetch_one(&recipient.pool)
                        .await
                        .unwrap();
                        assert_eq!(applications, 1);
                        assert!(
                            store::records::resolve(&recipient.pool, "stock-a")
                                .await
                                .unwrap()
                                .is_none()
                        );
                        recipient.pool.close().await;
                        let replay = simulation::artifacts::replay(
                            &original,
                            &directory.path().join("replay"),
                        )
                        .await
                        .unwrap();
                        if !matches!(replay, ReplayStatus::Verified { .. }) {
                            panic!(
                                "{replay:?}; evidence retained in {}",
                                directory.keep().display()
                            );
                        }
                    }
                });
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn three_organs_complete_only_the_named_routes() {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            let directory = tempfile::tempdir().unwrap();
            let original = directory.path().join("original");
            let result = match simulation::artifacts::execute(simulation::fixtures::transfer::three_parties(), &original).await {
                Ok(result) => result,
                Err(error) => panic!("{error}; evidence retained in {}", directory.keep().display()),
            };
            if result.result.verdict != Verdict::Passed {
                panic!("{:?} {:?}; evidence retained in {}", result.result, result.findings, directory.keep().display());
            }
            let origin = store::Store::open_existing_durable(&format!("sqlite://{}", original.join("working/a.sqlite").display())).await.unwrap();
            let routes: Vec<(String,String,String)> = store::sqlx::query_as(
                "SELECT x.public_exchange_uid, o.giver_person_uid, o.receiver_person_uid
                 FROM transfer_occurrence o JOIN transfer_exchange_path x ON x.uid = o.exchange_path_uid ORDER BY x.public_exchange_uid",
            ).fetch_all(&origin.pool).await.unwrap();
            assert_eq!(routes.len(), 3);
            assert_eq!(routes.iter().map(|row| row.0.as_str()).collect::<Vec<_>>(), ["ana-beto","carla-ana","carla-beto"]);
            assert_ne!(routes[1].2, routes[2].2);
            let states: Vec<String> = store::sqlx::query_scalar("SELECT state FROM promise WHERE transfer_uid IS NOT NULL ORDER BY uid")
                .fetch_all(&origin.pool).await.unwrap();
            assert_eq!(states, ["kept", "kept", "kept"]);
            let remote: Vec<(String,String)> = store::sqlx::query_as("SELECT actor_person_uid, sender_organ_uid FROM transfer_remote_command WHERE direction = 'incoming'")
                .fetch_all(&origin.pool).await.unwrap();
            assert!(!remote.is_empty());
            for (person, organ) in remote {
                let owner: String = store::sqlx::query_scalar("SELECT organ_uid FROM record WHERE uid = ?").bind(person).fetch_one(&origin.pool).await.unwrap();
                assert_eq!(owner, organ);
            }
            let stock_c: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record WHERE slug = 'stock-c'").fetch_one(&origin.pool).await.unwrap();
            assert_eq!(stock_c, 0);
            let parties: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_party").fetch_one(&origin.pool).await.unwrap();
            assert_eq!(parties, 3);
            origin.pool.close().await;
            let observer = store::Store::open_existing_durable(&format!("sqlite://{}", original.join("working/d.sqlite").display())).await.unwrap();
            let visibility: String = store::sqlx::query_scalar("SELECT visibility FROM transfer").fetch_one(&observer.pool).await.unwrap();
            assert_eq!(visibility, "hidden");
            let dependencies: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_dependency WHERE required_state = 'kept'").fetch_one(&observer.pool).await.unwrap();
            assert_eq!(dependencies, 1);
            observer.pool.close().await;
            let replay = simulation::artifacts::replay(&original, &directory.path().join("replay")).await.unwrap();
            if !matches!(replay, ReplayStatus::Verified { .. }) {
                panic!("{replay:?}; evidence retained in {}", directory.keep().display());
            }
        });
    }).unwrap().join().unwrap();
}
