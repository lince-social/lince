use nucleus::simulation::{Observation, ReplayStatus, Verdict};

#[test]
fn loan_expiry_projects_unmet_needs_without_returning_physical_stock() {
    std::thread::Builder::new().stack_size(16*1024*1024).spawn(|| {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            for replicated in [false,true] {
                for assumed in [false,true] {
                    let case=simulation::fixtures::transfer::temporary_loan(replicated,assumed);
                    let from=case.start_ms+3_000;
                    let until=case.end_ms-1;
                    let directory=tempfile::tempdir().unwrap();
                    let path=directory.path().join("run");
                    let mut session=simulation::artifacts::Session::open(case,&path,std::path::Path::new(".")).await.unwrap();
                    while session.step().await.unwrap() {}
                    for record in ["stock-b","transport"] {
                        let (uid,quantity)=session.world.quantity_with_basis("b",record,nucleus::simulation::QuantityBasis::Available).await.unwrap().unwrap();
                        assert_eq!(quantity.value.exact_numeric_cmp(store::exact::integer(-1)),std::cmp::Ordering::Equal);
                        let physical=store::records::resolve(&session.world.nodes["b"].engine().store.pool,record).await.unwrap().unwrap();
                        assert!(physical.quantity.is_zero(),"{record}: {}",physical.quantity);
                        assert!(session.world.trace.iter().any(|event| matches!(&event.observation,Observation::LoanQuantity{record,at_ms,after,..} if record==&uid && *at_ms==from && after.value.is_zero())));
                        assert!(session.world.trace.iter().any(|event| matches!(&event.observation,Observation::LoanQuantity{record,at_ms,after,..} if record==&uid && *at_ms==until && after.value.is_negative())));
                    }
                    let run=session.finish().await.unwrap();
                    assert_eq!(run.result.verdict,Verdict::Passed,"{:?} {:?} {}",run.result,run.findings,directory.path().display());
                    let replay=simulation::artifacts::replay(&path,&directory.path().join("replay")).await.unwrap();
                    assert!(matches!(replay,ReplayStatus::Verified{..}),"{replay:?}");
                }
            }
        });
    }).unwrap().join().unwrap();
}

#[test]
fn optional_extension_and_early_return_change_only_projected_availability() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    let mut case = simulation::fixtures::transfer::temporary_loan(false, false);
                    let old_end = case.end_ms - 1;
                    let new_end = old_end + 86_400_000;
                    case.end_ms = new_end + 1;
                    case.checks
                        .retain(|check| !check.id.ends_with("during-loan"));
                    case.inputs.push(simulation::scenario::Input {
                        id: "assume-loan-change".into(),
                        cell: "b".into(),
                        at_ms: case.start_ms + 5_000,
                        event: simulation::scenario::Event::AssumeLoan {
                            timing: simulation::loans::Timing {
                                source: nucleus::transfer::loans::Reference {
                                    origin: "$cell:a:organ".into(),
                                    transfer: "$routes".into(),
                                    exchange: "apples".into(),
                                },
                                accepted_revision: 2,
                                person: "$beto".into(),
                                until: Some(
                                    chrono::DateTime::from_timestamp_millis(new_end)
                                        .unwrap()
                                        .to_rfc3339(),
                                ),
                                returned: Some(store::exact::integer(5)),
                            },
                        },
                    });
                    for record in ["stock-b", "transport"] {
                        case.checks.push(nucleus::simulation::CheckDefinition {
                            id: format!("{record}-extended"),
                            predicate: nucleus::simulation::Predicate::QuantityEquals {
                                cell: "b".into(),
                                record: record.into(),
                                at_ms: old_end,
                                expected: nucleus::simulation::Quantity {
                                    value: nucleus::DecimalValue::parse_inferred("-0.5").unwrap(),
                                    unit: None,
                                },
                            },
                            options: nucleus::simulation::CheckOptions {
                                quantity: nucleus::simulation::QuantityBasis::Available,
                                ..Default::default()
                            },
                        });
                    }
                    let directory = tempfile::tempdir().unwrap();
                    let path = directory.path().join("run");
                    let run = simulation::artifacts::execute_with_sources(
                        case,
                        &path,
                        std::path::Path::new("."),
                    )
                    .await
                    .unwrap();
                    assert_eq!(
                        run.result.verdict,
                        Verdict::Passed,
                        "{:?} {:?}",
                        run.result,
                        run.findings
                    );
                    let replay =
                        simulation::artifacts::replay(&path, &directory.path().join("replay"))
                            .await
                            .unwrap();
                    assert!(
                        matches!(replay, ReplayStatus::Verified { .. }),
                        "{replay:?}"
                    );
                });
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn simulating_a_linked_return_reopens_all_its_needs_once() {
    std::thread::Builder::new().stack_size(16*1024*1024).spawn(|| {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            let mut case=simulation::fixtures::transfer::temporary_loan(false,false);
            case.end_ms=case.start_ms+10_000;
            case.checks.retain(|check|!check.id.ends_with("during-loan") && !check.id.ends_with("-stored"));
            let actions=[
                serde_json::json!({"action":"create-transfer-draft","creator":"$beto","request_id":"return-draft","head":"Return bike","agreement":"full","visibility":"hidden","reserve_default":"none","invitees":["$ana"],"promises":[{"uid":"return-item","record":"$stock-b","party":"$beto","delta":-10,"item":{"title":"Return bike","exchange":{"uid":"return","giver":"$beto","receiver":"$ana"},"return_of":{"origin":"$cell:a:organ","transfer":"$routes","exchange":"apples"}}}]}),
                serde_json::json!({"action":"set-transfer-private-application-policy","transfer":"$return-draft","exchange":"return","person":"$beto","expected_version":0,"request_id":"return-policy","effects":[{"record":"$stock-b","mode":"quantity","formula":"-incoming() / 10"}]}),
            ];
            for (index,action) in actions.into_iter().enumerate() {
                let id=if index==0 {"return-draft"} else {"return-policy"};
                case.inputs.push(simulation::scenario::Input {id:id.into(),cell:"b".into(),at_ms:case.start_ms+4_000+index as i64,event:simulation::scenario::Event::Action {invocation:simulation::scenario::Invocation {id:id.into(),actor:None,action:serde_json::from_value(action).unwrap()}}});
            }
            case.inputs.push(simulation::scenario::Input {id:"assume-return".into(),cell:"b".into(),at_ms:case.start_ms+4_002,event:simulation::scenario::Event::AssumeTransfer {assumption:simulation::assumptions::TransferAssumption {key:"return".into(),title:"Return".into(),person:"$beto".into(),record:"$stock-b".into(),quantity:store::exact::integer(10),unit:None,direction:simulation::assumptions::Direction::Outgoing,source:Some(simulation::assumptions::TransferSource {transfer:"$return-draft".into(),revision:1,promise:"return-item".into(),exchange:"return".into(),occurrence:None})}}});
            case.inputs.sort_by_key(|input|input.at_ms);
            let directory=tempfile::tempdir().unwrap();
            let path=directory.path().join("run");
            let mut session=simulation::artifacts::Session::open(case,&path,std::path::Path::new(".")).await.unwrap();
            while session.step().await.unwrap() {}
            if let Ok(error)=std::fs::read_to_string(path.join("diagnostic.json")) {panic!("{error} at {}",directory.keep().display());}
            let pool=&session.world.nodes["b"].engine().store.pool;
            let count:i64=store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_occurrence o JOIN record r ON r.uid = o.transfer_uid WHERE r.head = 'Return bike'").fetch_one(pool).await.unwrap();
            assert_eq!(count,0);
            let actual=store::records::resolve(pool,"stock-b").await.unwrap().unwrap().quantity;
            assert_eq!(actual.exact_numeric_cmp(store::exact::integer(-1)),std::cmp::Ordering::Equal);
            let run=session.finish().await.unwrap();
            assert_eq!(run.result.verdict,Verdict::Passed,"{:?} {:?}",run.result,run.findings);
            let replay=simulation::artifacts::replay(&path,&directory.path().join("replay")).await.unwrap();
            assert!(matches!(replay,ReplayStatus::Verified{..}),"{replay:?}");
        });
    }).unwrap().join().unwrap();
}

#[test]
fn remote_extension_waits_for_both_parties_and_replays_in_both_delivery_modes() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    for replicated in [false, true] {
                        let case = simulation::fixtures::transfer::extended_loan(replicated);
                        let until = case.end_ms - 1;
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
                        if let Ok(error) = std::fs::read_to_string(path.join("diagnostic.json")) {
                            panic!("{error} at {}", directory.keep().display());
                        }
                        for cell in ["a", "b"] {
                            let terms = store::transfer_loans::terms(
                                &session.world.nodes[cell].engine().store.pool,
                                false,
                            )
                            .await
                            .unwrap();
                            let loan = terms
                                .iter()
                                .find_map(|terms| terms.accepted.as_ref())
                                .unwrap();
                            assert_eq!(loan.until_ms, until);
                            assert_eq!(loan.revision, 3);
                        }
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
fn changed_units_leave_loan_checks_incomplete_without_breaking_other_records() {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            for assumed in [false, true] {
                let mut case = simulation::fixtures::transfer::temporary_loan(false, assumed);
                case.checks.retain(|check| matches!(check.id.as_str(), "stock-b-after-loan" | "transport-after-loan"));
                for (index, action) in [
                    serde_json::json!({"action":"create-lingua","name":"Test units","visibility":"private"}),
                    serde_json::json!({"action":"create-concept","lingua":"$unit-0","name":"changed-unit","parents":[]}),
                    serde_json::json!({"action":"set-unit","target":"$stock-b","unit":"$unit-1"}),
                ].into_iter().enumerate() {
                    let id = format!("unit-{index}");
                    case.inputs.push(simulation::scenario::Input { id: id.clone(), cell: "b".into(), at_ms: case.start_ms + 5_000 + index as i64 * 20, event: simulation::scenario::Event::Action { invocation: simulation::scenario::Invocation { id, actor: None, action: serde_json::from_value(action).unwrap() } } });
                }
                let directory = tempfile::tempdir().unwrap();
                let path = directory.path().join("run");
                let run = simulation::artifacts::execute(case, &path).await.unwrap();
                assert_eq!(run.result.verdict, Verdict::Inconclusive, "{:?} {:?}", run.result, run.findings);
                assert!(run.findings.is_empty());
                let coverage = &run.result.coverage;
                assert!(coverage.iter().any(|coverage| coverage.reason == Some(nucleus::simulation::CoverageReason::UnsupportedUnit)));
                assert!(matches!(simulation::artifacts::replay(&path, &directory.path().join("replay")).await.unwrap(), ReplayStatus::Verified { .. }));
            }
        });
    }).unwrap().join().unwrap();
}
