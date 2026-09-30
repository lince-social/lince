use nucleus::simulation::{ReplayStatus, Verdict};

#[test]
fn received_outcomes_are_signed_fresh_revocable_and_independent_of_observer_agreement() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    for replicated in [false, true] {
                        let case = simulation::fixtures::transfer::observer_outcomes(replicated);
                        let milestones = [
                            ("agreement-observer", false, false),
                            ("observed-agreed", true, false),
                            ("observed-revised", false, false),
                            ("observed-settled", true, true),
                            ("observer-restart", true, true),
                            ("observed-corrected", true, true),
                            ("observed-disputed", false, false),
                            ("observed-restored", true, true),
                            ("observed-fresh", true, true),
                            ("observed-revoked", false, false),
                            ("revoked-restart", false, false),
                        ]
                        .map(|(id, agreed, settled)| {
                            (
                                case.inputs
                                    .iter()
                                    .find(|input| input.id == id)
                                    .unwrap()
                                    .at_ms
                                    + 10,
                                id,
                                agreed,
                                settled,
                            )
                        });
                        let stale_at = case
                            .inputs
                            .iter()
                            .find(|input| input.id == "refresh-fresh")
                            .unwrap()
                            .at_ms
                            - 1;
                        let directory = tempfile::tempdir().unwrap();
                        let path = directory.path().join("run");
                        let mut session = simulation::artifacts::Session::open(
                            case,
                            &path,
                            std::path::Path::new("."),
                        )
                        .await
                        .unwrap();
                        let mut latest = String::new();
                        for (at, stage, agreed, settled) in milestones {
                            if stage == "observed-fresh" {
                                let observer = session.world.resolve_reference("$observer");
                                let (result, statuses) = store::transfer_agreement::read_at(
                                    &session.world.nodes["d"].engine().store.pool,
                                    &observer,
                                    chrono::DateTime::from_timestamp_millis(stale_at).unwrap(),
                                )
                                .await
                                .unwrap();
                                assert!(!result.ready);
                                assert_eq!(
                                    statuses[0]["blocking_reason"],
                                    "upstream_evidence_stale"
                                );
                                let source = session.world.resolve_reference("$routes");
                                let dora = session.world.resolve_reference("$dora");
                                let clock =
                                    nucleus::execution::Execution::new([0; 32], stale_at).unwrap();
                                let rows = clock
                                    .scope(protein::execute_for_with_signer(
                                        &session.world.nodes["d"].engine().store,
                                        &protein::Protein {
                                            source: protein::Source::Transfer,
                                            filter: vec![protein::Predicate::UidEq(source.clone())],
                                            fields: None,
                                            include: Default::default(),
                                            aggregate: None,
                                            order: vec![],
                                            limit: None,
                                        },
                                        None,
                                        Some(&dora),
                                    ))
                                    .await
                                    .unwrap();
                                let row = rows.iter().find(|row| row["uid"] == source).unwrap();
                                assert_eq!(
                                    row["social_delivery"]["local_view"]["freshness"]["state"],
                                    "stale"
                                );
                            }
                            while session.world.next_ms().is_some_and(|next| next <= at) {
                                assert!(session.step().await.unwrap());
                            }
                            let time = chrono::DateTime::from_timestamp_millis(at).unwrap();
                            let pool = &session.world.nodes["d"].engine().store.pool;
                            for (reference, expected) in
                                [("$observer", settled), ("$agreement-observer", agreed)]
                            {
                                let transfer = session.world.resolve_reference(reference);
                                let (result, statuses) =
                                    store::transfer_agreement::read_at(pool, &transfer, time)
                                        .await
                                        .unwrap();
                                assert_eq!(
                                    result.ready, expected,
                                    "{stage} {reference}: {result:?}; {statuses:?}"
                                );
                                assert_eq!(result.settled, expected);
                                assert!(result.observed);
                                assert!(
                                    store::transfers::party_levels(pool, &transfer)
                                        .await
                                        .unwrap()
                                        .iter()
                                        .all(|(_, _, level)| *level == 0)
                                );
                                assert!(
                                    store::transfers::occurrences_of(pool, &transfer)
                                        .await
                                        .unwrap()
                                        .is_empty()
                                );
                                if reference == "$observer" && expected {
                                    assert_eq!(statuses[0]["evidence"]["kind"], "received_outcome");
                                    let envelope = statuses[0]["evidence"]["envelope"]
                                        .as_str()
                                        .unwrap()
                                        .to_owned();
                                    if stage != "observer-restart" {
                                        assert_ne!(latest, envelope);
                                    }
                                    latest = envelope;
                                }
                            }
                            let own = session.world.resolve_reference("$stock-d");
                            assert_eq!(
                                store::records::get(pool, &own)
                                    .await
                                    .unwrap()
                                    .unwrap()
                                    .quantity_f64(),
                                0.0
                            );
                            let source = session.world.resolve_reference("$routes");
                            assert!(store::records::get(pool, &source).await.unwrap().is_none());
                            let dora = session.world.resolve_reference("$dora");
                            assert!(
                                store::transfers::party_for_actor(
                                    &session.world.nodes["a"].engine().store.pool,
                                    &source,
                                    &dora
                                )
                                .await
                                .unwrap()
                                .is_none()
                            );
                        }
                        let count: i64 = store::sqlx::query_scalar(
                            "SELECT count(*) FROM transfer_outcome_evidence",
                        )
                        .fetch_one(&session.world.nodes["d"].engine().store.pool)
                        .await
                        .unwrap();
                        assert!(count >= 6);
                        while session.step().await.unwrap() {}
                        let run = session.finish().await.unwrap();
                        if run.result.verdict != Verdict::Passed {
                            panic!("{:?}; {}", run.findings, directory.keep().display());
                        }
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
