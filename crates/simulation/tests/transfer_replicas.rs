use nucleus::simulation::{
    CheckDefinition, Comparison, Predicate, Quantity, QuantityBasis, ReplayStatus, Verdict,
};
use simulation::scenario::{Cell, Event, Input};

#[test]
fn sibling_cells_receive_loan_terms_and_private_effects_after_offline_recovery() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    for replicated in [false, true] {
                    for corrected in [false, true] {
                    let mut scenario = simulation::fixtures::transfer::temporary_loan(replicated, false);
                    if corrected {
                        for check in &mut scenario.checks {
                            if check.id.ends_with("-stored") {
                                if let Predicate::Quantity {expected, ..} = &mut check.predicate { expected.value = nucleus::DecimalValue::parse_inferred("-0.4").unwrap(); }
                            }
                            if check.id.ends_with("-during-loan") { check.options.window.until_ms = Some(scenario.start_ms + 4_499); }
                        }
                        for index in 0..2 {
                            let id = format!("correct-once-{index}");
                            scenario.inputs.push(Input { id:id.clone(), cell:"b".into(), at_ms:scenario.start_ms + 4_500 + index, event:Event::Action {invocation:simulation::scenario::Invocation {id,actor:None,action:engine::actions::Action::CompensateTransferApplication {application:"$receive-apples".into(),person:"$beto".into(),request_id:"correct-group".into()}}}});
                        }
                    }
                    scenario.name = "loan-sibling-recovery".into();
                    for (cell, host) in [("c", "a"), ("d", "b")] {
                        scenario.cells.push(Cell {
                            name: cell.into(),
                            seed: Vec::new(),
                            database: None,
                            lingua: Vec::new(),
                        });
                        scenario.inputs.insert(
                            0,
                            Input {
                                id: format!("enrol-{cell}"),
                                cell: cell.into(),
                                at_ms: scenario.start_ms,
                                event: Event::Enrol { peer: host.into() },
                            },
                        );
                        for (index, at) in [4_000, 5_000, 6_000].into_iter().enumerate() {
                            scenario.inputs.push(Input {
                                id: format!("recover-{cell}-{index}"),
                                cell: host.into(),
                                at_ms: scenario.start_ms + at,
                                event: Event::Sync {
                                    peer: cell.into(),
                                    delay_ms: 1,
                                    copies: 2,
                                    duplicate_spacing_ms: 2,
                                    drop: index == 0,
                                },
                            });
                        }
                        scenario.inputs.push(Input {
                            id: format!("restart-{cell}"),
                            cell: cell.into(),
                            at_ms: scenario.start_ms + 7_000,
                            event: Event::Restart {},
                        });
                    }
                    for record in ["stock-b", "transport"] {
                        scenario.checks.push(CheckDefinition {
                            id: format!("replica-{record}-expiry"),
                            predicate: Predicate::Quantity {
                                cell: "d".into(),
                                record: format!("${record}"),
                                comparison: Comparison::Equal,
                                expected: Quantity {
                                    value: store::exact::integer(-1),
                                    unit: None,
                                },
                            },
                            options: nucleus::simulation::CheckOptions {
                                quantity: QuantityBasis::Available,
                                ..Default::default()
                            },
                        });
                    }
                    scenario.inputs.sort_by_key(|input| input.at_ms);
                    let directory = tempfile::tempdir().unwrap();
                    let path = directory.path().join("run");
                    let mut session = simulation::artifacts::Session::open(
                        scenario,
                        &path,
                        std::path::Path::new("."),
                    )
                    .await
                    .unwrap();
                    while match session.step().await {
                        Ok(more) => more,
                        Err(error) => panic!("{error} at {}", directory.keep().display()),
                    } {}
                    let transfer = session.world.resolve_reference("$routes");
                    let canonical = store::transfers::get(
                        &session.world.nodes["a"].engine().store.pool,
                        &transfer,
                    )
                    .await
                    .unwrap();
                    let replica = store::transfers::get(
                        &session.world.nodes["c"].engine().store.pool,
                        &transfer,
                    )
                    .await
                    .unwrap();
                    assert!(canonical.is_some());
                    assert!(
                        replica.is_some(),
                        "same-Organ sync must include Transfer metadata at {}", directory.keep().display()
                    );
                    let source = &session.world.nodes["b"].engine().store.pool;
                    let target = &session.world.nodes["d"].engine().store.pool;
                    async fn count(pool: &store::sqlx::SqlitePool) -> i64 {
                        store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_private_effect")
                            .fetch_one(pool)
                            .await
                            .unwrap()
                    }
                    assert!(count(source).await > 0);
                    assert_eq!(count(source).await, count(target).await);
                    for record in ["$stock-b", "$transport"] {
                        let record = session.world.resolve_reference(record);
                        let expected = if corrected {nucleus::DecimalValue::parse_inferred("-0.4").unwrap()} else {store::exact::zero()};
                        assert_eq!(store::records::quantity(target, &record).await.unwrap().unwrap().exact_numeric_cmp(expected), std::cmp::Ordering::Equal);
                    }
                    if corrected {
                        let corrections: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact_origin WHERE json_extract(payload,'$.cause.kind') = 'compensation'").fetch_one(target).await.unwrap();
                        assert_eq!(corrections,2);
                        let repeated: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_private_application_correction").fetch_one(target).await.unwrap();
                        assert_eq!(repeated,1);
                    }
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
                    }
                });
        })
        .unwrap()
        .join()
        .unwrap();
}
