use simulation::fixtures::transfer::{trade, counteroffer, open_offer, private_trade};
use nucleus::simulation::{Comparison, Predicate, Quantity, ReplayStatus, Verdict};
use serde_json::{Value, json};
use simulation::scenario::{Check, Event, Input, Invocation, Scenario};

fn invocation(id: &str, payload: Value) -> Invocation {
    Invocation {
        id: id.into(),
        actor: None,
        action: serde_json::from_value(payload).unwrap(),
    }
}

fn add(case: &mut Scenario, cell: &str, id: &str, event: Event) {
    let at_ms = case.inputs.iter().map(|input| input.at_ms).max().unwrap() + 20;
    case.inputs.push(Input {
        id: id.into(),
        cell: cell.into(),
        at_ms,
        event,
    });
}

fn command(case: &mut Scenario, cell: &str, id: &str, payload: Value) {
    let invocation = invocation(id, payload);
    let event = if cell == "a" {
        Event::Action { invocation }
    } else {
        Event::TransferCommand {
            peer: "a".into(),
            transfer: "$routes".into(),
            invocation,
            delay_ms: 1,
            copies: 1,
            duplicate_spacing_ms: 0,
            drop: false,
        }
    };
    add(case, cell, id, event);
}

fn send(case: &mut Scenario, from: &str, to: &str, id: &str) {
    add(
        case,
        from,
        id,
        Event::TransferDelivery {
            peer: to.into(),
            delay_ms: 1,
            copies: 1,
            duplicate_spacing_ms: 0,
            drop: false,
        },
    );
}


#[test]
fn separate_organs_trade_and_keep_payment_outstanding_until_applied() {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            for replicated in [false,true] {
                let directory = tempfile::tempdir().unwrap();
                let run = directory.path().join("run");
                let case = trade(replicated);
                let payment_at = case.inputs.iter().find(|input|input.id == "activate-payment").unwrap().at_ms;
                let mut session = simulation::artifacts::Session::open(case,&run,std::path::Path::new(".")).await.unwrap();
                while session.world.next_ms().is_some_and(|time|time < payment_at) { assert!(session.step().await.unwrap()); }
                let pool = &session.world.nodes["a"].engine().store.pool;
                let states:Vec<(String,String)> = store::sqlx::query_as("SELECT uid,state FROM promise WHERE transfer_uid IS NOT NULL ORDER BY uid").fetch_all(pool).await.unwrap();
                assert_eq!(states[0],("apples".into(),"kept".into()));
                assert_ne!(states[1].1,"kept");
                for (cell,record,quantity) in [("a","stock-a",0.0),("b","stock-b",1.0),("a","money-a",0.0),("b","money-b",100.0)] {
                    let record=store::records::resolve(&session.world.nodes[cell].engine().store.pool,record).await.unwrap().unwrap();
                    assert_eq!(record.quantity.to_f64(),quantity);
                }
                while session.step().await.unwrap() {}
                let pool = &session.world.nodes["a"].engine().store.pool;
                let unfinished:i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM promise WHERE transfer_uid IS NOT NULL AND state != 'kept'").fetch_one(pool).await.unwrap();
                assert_eq!(unfinished,0);
                let slices:Vec<f64> = store::sqlx::query_scalar("SELECT d.canonical_quantity FROM transfer_application_handoff_detail d JOIN transfer_application_handoff h ON h.uid=d.handoff_uid JOIN promise p ON p.uid=d.source_promise_uid WHERE p.uid='payment' AND p.party_uid=h.participant_person_uid ORDER BY d.canonical_cumulative_before").fetch_all(pool).await.unwrap();
                assert_eq!(slices,vec![4.0,6.0]);
                let result=session.finish().await.unwrap();
                if result.result.verdict != Verdict::Passed { panic!("{:?} {:?}; {}",result.result,result.findings,directory.keep().display()); }
                assert!(matches!(simulation::artifacts::replay(&run,&directory.path().join("replay")).await.unwrap(),ReplayStatus::Verified { .. }));
            }
        });
    }).unwrap().join().unwrap();
}


#[test]
fn remote_counteroffer_resets_agreement_and_old_terms_cannot_activate() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    for replicated in [false, true] {
                        let case = counteroffer(replicated);
                        let agreed_at = case
                            .inputs
                            .iter()
                            .find(|input| input.id == "new-agreement-a-1")
                            .unwrap()
                            .at_ms;
                        let directory = tempfile::tempdir().unwrap();
                        let run = directory.path().join("run");
                        let mut session = simulation::artifacts::Session::open(
                            case,
                            &run,
                            std::path::Path::new("."),
                        )
                        .await
                        .unwrap();
                        while session.world.next_ms().is_some_and(|time| time < agreed_at) {
                            if !session.step().await.unwrap() {
                                let result = session.finish().await.unwrap();
                                panic!(
                                    "{:?} {:?}; {}",
                                    result.result,
                                    result.findings,
                                    directory.keep().display()
                                );
                            }
                        }
                        let pool = &session.world.nodes["a"].engine().store.pool;
                        let revision: i64 =
                            store::sqlx::query_scalar("SELECT revision FROM transfer")
                                .fetch_one(pool)
                                .await
                                .unwrap();
                        assert_eq!(revision, 3);
                        let levels: Vec<i64> =
                            store::sqlx::query_scalar("SELECT level FROM transfer_agreement")
                                .fetch_all(pool)
                                .await
                                .unwrap();
                        assert_eq!(levels, vec![0, 0]);
                        let binding: Option<String> = store::sqlx::query_scalar(
                            "SELECT record_uid FROM promise WHERE uid='apples'",
                        )
                        .fetch_one(pool)
                        .await
                        .unwrap();
                        assert_eq!(binding, Some(session.world.resolve_reference("$stock-a")));
                        while session.step().await.unwrap() {}
                        let result = session.finish().await.unwrap();
                        if result.result.verdict != Verdict::Passed {
                            panic!(
                                "{:?} {:?}; {}",
                                result.result,
                                result.findings,
                                directory.keep().display()
                            );
                        }
                        assert!(matches!(
                            simulation::artifacts::replay(&run, &directory.path().join("replay"))
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
fn public_open_item_is_claimed_without_exposing_either_private_record() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    for replicated in [false, true] {
                        let directory = tempfile::tempdir().unwrap();
                        let run = directory.path().join("run");
                        let mut session = simulation::artifacts::Session::open(
                            open_offer(replicated),
                            &run,
                            std::path::Path::new("."),
                        )
                        .await
                        .unwrap();
                        while session.step().await.unwrap() {}
                        let pool = &session.world.nodes["a"].engine().store.pool;
                        let promises: Vec<(String, Option<String>)> = store::sqlx::query_as(
                            "SELECT state,record_uid FROM promise WHERE transfer_uid IS NOT NULL",
                        )
                        .fetch_all(pool)
                        .await
                        .unwrap();
                        assert_eq!(promises, vec![("kept".into(), None), ("kept".into(), None)]);
                        let routes: i64 = store::sqlx::query_scalar(
                            "SELECT COUNT(*) FROM transfer_exchange_path",
                        )
                        .fetch_one(pool)
                        .await
                        .unwrap();
                        assert_eq!(routes, 1);
                        let source: Option<String> = store::sqlx::query_scalar(
                            "SELECT source_record_uid FROM transfer_open_claim_pair",
                        )
                        .fetch_one(pool)
                        .await
                        .unwrap();
                        assert_eq!(source, None);
                        let result = session.finish().await.unwrap();
                        if result.result.verdict != Verdict::Passed {
                            panic!(
                                "{:?} {:?}; {}",
                                result.result,
                                result.findings,
                                directory.keep().display()
                            );
                        }
                        assert!(matches!(
                            simulation::artifacts::replay(&run, &directory.path().join("replay"))
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
fn declining_a_remote_invitation_never_joins_or_changes_private_stock() {
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
                            simulation::fixtures::transfer::independent_donation(replicated);
                        let accept = case
                            .inputs
                            .iter()
                            .position(|input| input.id == "accept")
                            .unwrap();
                        case.inputs.truncate(accept);
                        add(
                            &mut case,
                            "b",
                            "decline",
                            Event::RejectInvitation {
                                transfer: "$routes".into(),
                                person: "$beto".into(),
                                peer: Some("a".into()),
                            },
                        );
                        case.checks
                            .retain(|check| !check.id.starts_with("balance-"));
                        for (cell, amount) in [("a", "30"), ("b", "0")] {
                            case.checks.push(Check {
                                id: format!("unchanged-{cell}"),
                                options: Default::default(),
                                predicate: Predicate::Quantity {
                                    cell: cell.into(),
                                    record: format!("$stock-{cell}"),
                                    comparison: Comparison::Equal,
                                    expected: Quantity {
                                        value: nucleus::DecimalValue::parse_inferred(amount)
                                            .unwrap(),
                                        unit: None,
                                    },
                                },
                            });
                        }
                        let directory = tempfile::tempdir().unwrap();
                        let run = directory.path().join("run");
                        let mut session = simulation::artifacts::Session::open(
                            case,
                            &run,
                            std::path::Path::new("."),
                        )
                        .await
                        .unwrap();
                        while session.step().await.unwrap() {}
                        let pool = &session.world.nodes["a"].engine().store.pool;
                        let state: String =
                            store::sqlx::query_scalar("SELECT status FROM transfer_invitation")
                                .fetch_one(pool)
                                .await
                                .unwrap();
                        assert_eq!(state, "rejected");
                        let parties: i64 =
                            store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_party")
                                .fetch_one(pool)
                                .await
                                .unwrap();
                        assert_eq!(parties, 1);
                        let result = session.finish().await.unwrap();
                        if result.result.verdict != Verdict::Passed {
                            panic!(
                                "{:?} {:?}; {}",
                                result.result,
                                result.findings,
                                directory.keep().display()
                            );
                        }
                        assert!(matches!(
                            simulation::artifacts::replay(&run, &directory.path().join("replay"))
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
fn revoked_delivery_cannot_apply_an_earlier_pending_receipt() {
    std::thread::Builder::new().stack_size(16*1024*1024).spawn(|| {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            for replicated in [false,true] {
                let mut case=simulation::fixtures::transfer::independent_donation(replicated);
                let receive=case.inputs.iter().position(|input|input.id=="receive-apples").unwrap();
                case.inputs.truncate(receive);
                command(&mut case,"a","revoke",json!({"action":"revoke-transfer-delivery","transfer":"$routes","delivery":"$policy-b","person":"$ana","expected_revision":1,"request_id":"revoke"}));
                add(&mut case,"b","refresh-revocation",Event::RefreshTransfer { transfer:"$routes".into(),person:"$beto".into() });
                send(&mut case,"b","a","revoked");
                for check in &mut case.checks {
                    if let Predicate::Quantity { cell,expected,.. }=&mut check.predicate {
                        if cell=="b" { expected.value=nucleus::DecimalValue::parse_inferred("0").unwrap(); }
                    }
                }
                let directory=tempfile::tempdir().unwrap();
                let mut session=simulation::artifacts::Session::open(case,&directory.path().join("run"),std::path::Path::new(".")).await.unwrap();
                while session.step().await.unwrap() {}
                let node=&session.world.nodes["b"];
                let state:String=store::sqlx::query_scalar("SELECT state FROM transfer_remote_reference").fetch_one(&node.engine().store.pool).await.unwrap();
                if state != "revoked" {
                    let result=session.finish().await.unwrap();
                    panic!("Reference is {state}; {:?} {:?}; {}",result.result,result.findings,directory.keep().display());
                }
                let handoff:String=store::sqlx::query_scalar("SELECT uid FROM transfer_remote_application_handoff").fetch_one(&node.engine().store.pool).await.unwrap();
                let before=node.engine().store.state_hash().await.unwrap();
                let action=serde_json::from_value(json!({"action":"apply-transfer-application","transfer":session.world.resolve_reference("$routes"),"handoff":handoff,"person":session.world.resolve_reference("$beto"),"local_record":session.world.resolve_reference("$stock-b"),"expected_formula_hash":nucleus::transfer::occurrence_application_formula_hash("incoming()"),"expected_formula_version":0,"expected_local_delta":10,"expected_local_cumulative_before":0,"request_id":"revoked-application"})).unwrap();
                let probe=nucleus::execution::Execution::restore(node.execution.snapshot()).unwrap();
                let error=probe.scope(node.engine().act(action,None)).await.unwrap_err();
                assert_eq!(error.code(), Some("transfer_delivery_inactive"), "{error}");
                assert_eq!(before,node.engine().store.state_hash().await.unwrap());
                let result=session.finish().await.unwrap();
                if result.result.verdict!=Verdict::Passed { panic!("{:?} {:?}; {}",result.result,result.findings,directory.keep().display()); }
                assert!(matches!(simulation::artifacts::replay(&directory.path().join("run"), &directory.path().join("replay")).await.unwrap(), ReplayStatus::Verified { .. }));
            }
        });
    }).unwrap().join().unwrap();
}

#[test]
fn remote_owner_private_half_payment_keeps_the_public_payment_at_ten() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    for replicated in [false, true] {
                        let case = private_trade(replicated);
                        let directory = tempfile::tempdir().unwrap();
                        let run = directory.path().join("run");
                        let mut session = simulation::artifacts::Session::open(
                            case,
                            &run,
                            std::path::Path::new("."),
                        )
                        .await
                        .unwrap();
                        while session.step().await.unwrap() {}
                        let pool = &session.world.nodes["a"].engine().store.pool;
                        let public: f64 = store::sqlx::query_scalar(
                            "SELECT delta FROM promise WHERE uid = 'payment'",
                        )
                        .fetch_one(pool)
                        .await
                        .unwrap();
                        assert_eq!(public, -10.0);
                        let leaked: i64 = store::sqlx::query_scalar(
                            "SELECT COUNT(*) FROM transfer_private_policy_event",
                        )
                        .fetch_one(pool)
                        .await
                        .unwrap();
                        assert_eq!(leaked, 0);
                        let result = session.finish().await.unwrap();
                        assert_eq!(
                            result.result.verdict,
                            Verdict::Passed,
                            "{:?}",
                            result.findings
                        );
                        assert!(matches!(
                            simulation::artifacts::replay(&run, &directory.path().join("replay"))
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
fn revoked_delivery_keeps_exact_completed_receipt_retries_in_its_history() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    for replicated in [false, true] {
                        let mut case = simulation::fixtures::transfer::independent_donation(replicated);
                        command(&mut case, "a", "revoke-after-receipt", json!({
                            "action":"revoke-transfer-delivery","transfer":"$routes","delivery":"$policy-b",
                            "person":"$ana","expected_revision":1,"request_id":"revoke-after-receipt"
                        }));
                        add(&mut case, "b", "restore-person", Event::PersonKey { person:"$beto".into() });
                        add(&mut case, "b", "refresh-revocation", Event::RefreshTransfer { transfer:"$routes".into(), person:"$beto".into() });
                        send(&mut case, "b", "a", "revoked-after-receipt");
                        let directory = tempfile::tempdir().unwrap();
                        let path = directory.path().join("run");
                        let mut session = simulation::artifacts::Session::open(case, &path, std::path::Path::new(".")).await.unwrap();
                        while session.step().await.unwrap() {}
                        let node = &session.world.nodes["b"];
                        let state: String = store::sqlx::query_scalar("SELECT state FROM transfer_remote_reference")
                            .fetch_one(&node.engine().store.pool).await.unwrap();
                        assert_eq!(state, "revoked");
                        let application = store::transfer_delivery::local_application_for_request(&node.engine().store.pool, "receive-apples")
                            .await.unwrap().unwrap();
                        let before = node.engine().store.state_hash().await.unwrap();
                        let mut payload = json!({
                            "action":"apply-transfer-application","transfer":session.world.resolve_reference("$routes"),
                            "handoff":application.handoff_uid,"person":session.world.resolve_reference("$beto"),
                            "local_record":session.world.resolve_reference("$stock-b"),
                            "expected_formula_hash":application.application_formula_hash,
                            "expected_formula_version":application.application_formula_version,
                            "expected_local_delta":application.local_delta,
                            "expected_local_cumulative_before":application.local_cumulative_before,
                            "request_id":"receive-apples"
                        });
                        let action = serde_json::from_value(payload.clone()).unwrap();
                        let probe = nucleus::execution::Execution::restore(node.execution.snapshot()).unwrap();
                        let outcome = probe.scope(node.engine().act(action, None)).await.unwrap();
                        assert_eq!(outcome.created.as_deref(), Some(application.uid.as_str()));
                        assert!(outcome.facts.is_empty());
                        assert_eq!(before, node.engine().store.state_hash().await.unwrap());
                        payload["expected_local_delta"] = json!(application.local_delta + 1.0);
                        let changed = serde_json::from_value(payload).unwrap();
                        let error = probe.scope(node.engine().act(changed, None)).await.unwrap_err();
                        assert_eq!(error.code(), Some("transfer_remote_application_preview_stale"));
                        assert_eq!(before, node.engine().store.state_hash().await.unwrap());
                        let run = session.finish().await.unwrap();
                        assert_eq!(run.result.verdict, Verdict::Passed, "{:?}", run.findings);
                        assert!(matches!(
                            simulation::artifacts::replay(&path, &directory.path().join("replay")).await.unwrap(),
                            ReplayStatus::Verified { .. }
                        ));
                    }
                });
        }).unwrap().join().unwrap();
}


#[test]
fn private_policy_simulation_matches_live_payment_without_settling_it() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    let mut case = private_trade(false);
                    let first = case
                        .inputs
                        .iter()
                        .position(|input| input.id == "prepare-first")
                        .unwrap();
                    case.inputs.truncate(first);
                    add(
                        &mut case,
                        "b",
                        "assume-payment",
                        Event::AssumeTransfer {
                            assumption: simulation::assumptions::TransferAssumption {
                                key: "payment".into(),
                                title: "If I pay the agreed ten".into(),
                                person: "$beto".into(),
                                record: "$money-b".into(),
                                quantity: nucleus::DecimalValue::parse_inferred("10").unwrap(),
                                unit: None,
                                direction: simulation::assumptions::Direction::Outgoing,
                                source: Some(simulation::assumptions::TransferSource {
                                    transfer: "$routes".into(),
                                    revision: 2,
                                    promise: "payment".into(),
                                    exchange: "payment".into(),
                                    occurrence: Some("$activate-payment".into()),
                                }),
                            },
                        },
                    );
                    for check in &mut case.checks {
                        if check.id == "final-money-a" {
                            if let Predicate::Quantity { expected, .. } = &mut check.predicate {
                                expected.value = store::exact::zero();
                            }
                        }
                    }
                    let directory = tempfile::tempdir().unwrap();
                    let mut session = simulation::artifacts::Session::open(
                        case,
                        &directory.path().join("run"),
                        std::path::Path::new("."),
                    )
                    .await
                    .unwrap();
                    while session.step().await.unwrap() {}
                    let state: String = store::sqlx::query_scalar(
                        "SELECT state FROM promise WHERE uid = 'payment'",
                    )
                    .fetch_one(&session.world.nodes["a"].engine().store.pool)
                    .await
                    .unwrap();
                    assert_eq!(state, "active");
                    let result = session.finish().await.unwrap();
                    assert_eq!(
                        result.result.verdict,
                        Verdict::Passed,
                        "{:?}",
                        result.findings
                    );
                });
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn private_receipt_correction_reverses_the_recorded_delta_after_policy_changes() {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            let mut case = private_trade(false);
            add(&mut case,"b","change-policy", Event::Action {invocation:invocation("change-policy",json!({
                "action":"set-transfer-private-application-policy","transfer":"$routes","exchange":"payment",
                "effects":[{"record":"$money-b","formula":"-incoming() * 100","mode":"quantity"}],"person":"$beto","expected_version":1,"request_id":"change-policy"
            }))});
            for id in ["correct-last-payment","retry-last-correction"] {
                add(&mut case,"b",id,Event::Action {invocation:invocation(id,json!({
                    "action":"compensate-transfer-application","application":"$pay-last","person":"$beto","request_id":"correct-last-payment"
                }))});
            }
            for check in &mut case.checks {
                if check.id == "final-money-b" {
                    if let Predicate::Quantity {expected,..} = &mut check.predicate { expected.value = nucleus::DecimalValue::parse_inferred("98").unwrap(); }
                }
            }
            let directory = tempfile::tempdir().unwrap();
            let run = directory.path().join("run");
            let result = simulation::artifacts::execute(case,&run).await.unwrap();
            assert_eq!(result.result.verdict,Verdict::Passed,"{:?}",result.findings);
            assert!(matches!(simulation::artifacts::replay(&run,&directory.path().join("replay")).await.unwrap(),ReplayStatus::Verified {..}));
        });
    }).unwrap().join().unwrap();
}
