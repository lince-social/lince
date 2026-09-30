use engine::actions::Action;
use nucleus::simulation::{
    CheckDefinition, CheckOptions, Comparison, Evaluation, FailureMode, Predicate, Quantity,
    ReplayStatus, Verdict,
};
use simulation::assumptions::{Direction, TransferAssumption};
use simulation::scenario::{Database, Event, Input, Invocation};

#[test]
fn optional_transfer_runs_use_each_owners_unapplied_remainder() {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            for replicated in [false, true] {
                let mut case = simulation::fixtures::transfer::independent_donation(replicated);
                let receive = case.inputs.iter().position(|input| input.id == "receive-apples").unwrap();
                case.inputs.truncate(receive);
                let settle = case.inputs.iter_mut().find(|input| input.id == "settle").unwrap();
                if let Event::SettleReviewed { quantity, .. } = &mut settle.event {
                    *quantity = nucleus::DecimalValue::parse_inferred("4").unwrap();
                }
                let shared = nucleus::simulation::sharing::Shared {
                    kind:nucleus::simulation::sharing::Kind::TransferSimulation,
                    id:"selected-assumption".into(), transfer:"$routes".into(),revision:2,
                    source_at_ms:case.start_ms, duration_ms:10_000,
                    assumptions:vec![nucleus::simulation::sharing::Assumption {
                        source:simulation::assumptions::TransferSource { transfer:"$routes".into(),revision:2,promise:"apples".into(),exchange:"apples".into(),occurrence:Some("$activate".into()) },
                        after_ms:2_000,quantity:nucleus::DecimalValue::parse_inferred("6").unwrap(),
                    }], result:None,
                };
                for (id, at, action) in [
                    ("shared-thread",1_000,Action::CreateTransferThread { transfer:"$routes".into(),head:"Selected simulation".into(),request_id:"shared-thread".into(),person:"$ana".into() }),
                    ("shared-message",1_020,Action::CreateTransferMessage { transfer:"$routes".into(),thread:"$shared-thread".into(),body:serde_json::to_string(&shared).unwrap(),parent:None,references:vec![],request_id:"shared-message".into(),person:"$ana".into() }),
                    ("share-publish",1_040,serde_json::from_value(serde_json::json!({"action":"enqueue-transfer-delivery","transfer":"$routes","delivery":"$policy-b","request_id":"share-publish","person":"$ana"})).unwrap()),
                ] {
                    case.inputs.push(Input { id:id.into(),cell:"a".into(),at_ms:case.start_ms + at,event:Event::Action { invocation:Invocation { id:id.into(),actor:None,action } } });
                }
                case.inputs.push(Input { id:"share-delivery".into(),cell:"a".into(),at_ms:case.start_ms + 1_060,event:Event::TransferDelivery { peer:"b".into(),delay_ms:1,copies:1,duplicate_spacing_ms:0,drop:false } });
                for (cell, person, direction, amount) in [
                    ("a", "$ana", Direction::Outgoing, "6"),
                    ("b", "$beto", Direction::Incoming, "10"),
                ] {
                    case.inputs.push(Input {
                        id:format!("optional-{cell}"), cell:cell.into(), at_ms:case.start_ms + 2_000,
                        event:Event::AssumeTransfer { assumption: TransferAssumption {
                            key:format!("apples-{cell}"), title:"Assumed remainder".into(), person:person.into(), record:format!("$stock-{cell}"),
                            quantity:nucleus::DecimalValue::parse_inferred(amount).unwrap(), unit:None, direction,
                            source:Some(simulation::assumptions::TransferSource { transfer:"$routes".into(), revision:2, promise:"apples".into(), exchange:"apples".into(), occurrence:Some("$activate".into()) }),
                        } },
                    });
                }
                let directory = tempfile::tempdir().unwrap();
                let mut session = simulation::artifacts::Session::open(case, &directory.path().join("run"), std::path::Path::new(".")).await.unwrap();
                while session.world.next_ms().is_some_and(|time| time < session.world.scenario.start_ms + 1_000) { assert!(session.step().await.unwrap()); }
                let public = |pool: store::sqlx::SqlitePool| async move {
                    let revision: i64 = store::sqlx::query_scalar("SELECT revision FROM transfer LIMIT 1").fetch_one(&pool).await.unwrap();
                    let levels:Vec<(String,i64)> = store::sqlx::query_as("SELECT party_uid, level FROM transfer_agreement ORDER BY party_uid").fetch_all(&pool).await.unwrap();
                    (revision,levels)
                };
                let before = public(session.world.nodes["a"].engine().store.pool.clone()).await;
                while session.step().await.unwrap() {}
                assert_eq!(before, public(session.world.nodes["a"].engine().store.pool.clone()).await);
                let projection:String = store::sqlx::query_scalar("SELECT projection FROM transfer_remote_reference LIMIT 1").fetch_one(&session.world.nodes["b"].engine().store.pool).await.unwrap();
                let projection:serde_json::Value = serde_json::from_str(&projection).unwrap();
                let messages:Vec<_> = projection["threads"].as_array().into_iter().flatten().flat_map(|thread| thread["messages"].as_array().into_iter().flatten()).collect();
                assert_eq!(messages.len(), 1, "{projection}");
                let shared = nucleus::simulation::sharing::Shared::parse(messages[0]["body"].as_str().unwrap()).unwrap();
                assert_eq!(shared.assumptions[0].quantity.to_f64(),6.0);
                let body = serde_json::to_string(&shared).unwrap();
                assert!(!body.contains(&session.world.resolve_reference("$stock-a")));
                assert!(!body.contains("formula"));
                let run = session.finish().await.unwrap();
                if run.result.verdict != Verdict::Passed {
                    panic!("{:?} {:?}; evidence retained in {}",run.result,run.findings,directory.keep().display());
                }
            }
        });
    }).unwrap().join().unwrap();
}

#[test]
fn delayed_receipt_does_not_hide_a_temporary_shortage() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    let mut case = simulation::fixtures::daily();
                    case.cells[0].seed.truncate(1);
                    if let Action::CreateRecord { quantity, .. } = &mut case.cells[0].seed[0].action
                    {
                        *quantity = 5.0;
                    }
                    case.cells[0].seed.push(Invocation {
                        id: "ana".into(),
                        actor: None,
                        action: Action::CreateRecord {
                            slug: Some("ana".into()),
                            kind: nucleus::RecordKind::Person,
                            head: "Ana".into(),
                            body: String::new(),
                            quantity: 1.0,
                        },
                    });
                    case.checking.on_failure = FailureMode::Continue;
                    case.inputs.clear();
                    case.end_ms = case.start_ms + 1_000;
                    case.checks = vec![
                        CheckDefinition {
                            id: "never-negative".into(),
                            options: CheckOptions {
                                evaluation: Evaluation::EveryChange,
                                ..Default::default()
                            },
                            predicate: Predicate::Nonnegative {
                                cell: "a".into(),
                                record: "stock".into(),
                            },
                        },
                        CheckDefinition {
                            id: "ends-positive".into(),
                            options: CheckOptions {
                                evaluation: Evaluation::End,
                                ..Default::default()
                            },
                            predicate: Predicate::Nonnegative {
                                cell: "a".into(),
                                record: "stock".into(),
                            },
                        },
                    ];
                    for (id, at, direction) in [
                        ("give", 100, Direction::Outgoing),
                        ("receive", 200, Direction::Incoming),
                    ] {
                        case.inputs.push(Input {
                            id: id.into(),
                            cell: "a".into(),
                            at_ms: case.start_ms + at,
                            event: Event::AssumeTransfer {
                                assumption: TransferAssumption {
                                    key: id.into(),
                                    title: id.into(),
                                    person: "ana".into(),
                                    record: "stock".into(),
                                    quantity: nucleus::DecimalValue::parse_inferred("6").unwrap(),
                                    unit: None,
                                    direction,
                                    source: None,
                                },
                            },
                        });
                    }
                    let directory = tempfile::tempdir().unwrap();
                    let run = simulation::artifacts::execute(case, &directory.path().join("run"))
                        .await
                        .unwrap();
                    assert_eq!(run.result.verdict, Verdict::Failed);
                    assert_eq!(
                        run.result.coverage[0].status,
                        nucleus::simulation::CheckStatus::Failed
                    );
                    assert_eq!(
                        run.result.coverage[1].status,
                        nucleus::simulation::CheckStatus::Passed
                    );
                    assert!(run.result.coverage.iter().all(|coverage| coverage.complete));
                });
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn optional_comparisons_keep_the_same_snapshot_and_existing_karma() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    let directory = tempfile::tempdir().unwrap();
                    let mut seed = simulation::fixtures::daily();
                    seed.name = "existing-karma".into();
                    seed.end_ms = seed.start_ms;
                    seed.checks.clear();
                    seed.cells[0].seed.push(Invocation {
                        id: "ana".into(),
                        actor: None,
                        action: Action::CreateRecord {
                            slug: Some("ana".into()),
                            kind: nucleus::RecordKind::Person,
                            head: "Ana".into(),
                            body: String::new(),
                            quantity: 1.0,
                        },
                    });
                    let mut source = simulation::artifacts::Session::open(
                        seed.clone(),
                        &directory.path().join("source-run"),
                        directory.path(),
                    )
                    .await
                    .unwrap();
                    let path = directory.path().join("source.sqlite");
                    source.world.nodes["a"]
                        .engine()
                        .store
                        .snapshot_into(&path)
                        .await
                        .unwrap();
                    let before = source.world.nodes["a"]
                        .engine()
                        .store
                        .state_hash()
                        .await
                        .unwrap();
                    let hash = simulation::artifacts::file_hash(&path).unwrap();
                    for (amount, expected, verdict) in
                        [("6", 1.0, Verdict::Failed), ("5", 2.0, Verdict::Passed)]
                    {
                        let mut case = simulation::fixtures::current_database(seed.start_ms);
                        case.end_ms = case.start_ms + 86_400_000;
                        case.cells[0].database = Some(Database {
                            file: "source.sqlite".into(),
                            hash: hash.clone(),
                        });
                        case.checking.on_failure = FailureMode::Continue;
                        case.inputs.push(Input {
                            id: "assumed-donation".into(),
                            cell: "current".into(),
                            at_ms: case.start_ms + 100,
                            event: Event::AssumeTransfer {
                                assumption: TransferAssumption {
                                    key: "draft-donation".into(),
                                    title: "Apples for Beto".into(),
                                    person: "ana".into(),
                                    record: "stock".into(),
                                    quantity: nucleus::DecimalValue::parse_inferred(amount)
                                        .unwrap(),
                                    unit: None,
                                    direction: Direction::Outgoing,
                                    source: None,
                                },
                            },
                        });
                        case.checks.push(CheckDefinition {
                            id: "keep-two-apples".into(),
                            options: CheckOptions {
                                evaluation: Evaluation::EveryChange,
                                ..Default::default()
                            },
                            predicate: Predicate::Quantity {
                                cell: "current".into(),
                                record: "stock".into(),
                                comparison: Comparison::AtLeast,
                                expected: Quantity {
                                    value: nucleus::DecimalValue::parse_inferred("2").unwrap(),
                                    unit: None,
                                },
                            },
                        });
                        let comparison = simulation::comparison::run(
                            case,
                            &directory.path().join(format!("compare-{amount}")),
                            directory.path(),
                        )
                        .await
                        .unwrap();
                        assert_eq!(
                            comparison.proposed.verdict, verdict,
                            "{:?}",
                            comparison.proposed
                        );
                        assert_eq!(
                            comparison.baseline.verdict,
                            Verdict::Passed,
                            "{:?}",
                            comparison.baseline
                        );
                        for (run, expected) in [
                            (&comparison.with_transfers, expected),
                            (&comparison.without_transfers, 7.0),
                        ] {
                            let store = store::Store::open_existing_durable(&format!(
                                "sqlite://{run}/working/current.sqlite"
                            ))
                            .await
                            .unwrap();
                            assert_eq!(
                                store::records::resolve(&store.pool, "stock")
                                    .await
                                    .unwrap()
                                    .unwrap()
                                    .quantity
                                    .to_f64(),
                                expected
                            );
                            store.pool.close().await;
                            let bundle =
                                simulation::artifacts::load(std::path::Path::new(run)).unwrap();
                            assert_eq!(
                                bundle.scenario.cells[0].database.as_ref().unwrap().hash,
                                hash
                            );
                            assert_eq!(
                                simulation::artifacts::source_status(
                                    std::path::Path::new(run),
                                    "current",
                                    &source.world.nodes["a"].engine().store
                                )
                                .await
                                .unwrap(),
                                simulation::artifacts::SourceStatus::Matching
                            );
                        }
                        let replay = simulation::artifacts::replay(
                            std::path::Path::new(&comparison.with_transfers),
                            &directory.path().join(format!("replay-{amount}")),
                        )
                        .await
                        .unwrap();
                        assert!(
                            matches!(replay, ReplayStatus::Verified { .. }),
                            "{replay:?}"
                        );
                    }
                    assert_eq!(
                        before,
                        source.world.nodes["a"]
                            .engine()
                            .store
                            .state_hash()
                            .await
                            .unwrap()
                    );
                    assert_eq!(
                        store::records::resolve(
                            &source.world.nodes["a"].engine().store.pool,
                            "stock"
                        )
                        .await
                        .unwrap()
                        .unwrap()
                        .quantity
                        .to_f64(),
                        10.0
                    );
                    let node = &source.world.nodes["a"];
                    node.execution
                        .scope(node.engine().act(
                            Action::AddQuantity {
                                target: "stock".into(),
                                delta: 1.0,
                            },
                            None,
                        ))
                        .await
                        .unwrap();
                    assert_eq!(
                        simulation::artifacts::source_status(
                            &directory.path().join("compare-5/with-transfers"),
                            "current",
                            &node.engine().store
                        )
                        .await
                        .unwrap(),
                        simulation::artifacts::SourceStatus::Changed
                    );
                    source.cancel();
                    source.finish().await.unwrap();
                });
        })
        .unwrap()
        .join()
        .unwrap();
}
