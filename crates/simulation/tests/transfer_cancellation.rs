use nucleus::simulation::{ReplayStatus, Verdict};

#[test]
fn independent_organs_cancel_remaining_work_and_keep_delivered_receipts() {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            for replicated in [false, true] {
                let case = simulation::fixtures::transfer::partial_cancellation(replicated);
                let directory = tempfile::tempdir().unwrap();
                let path = directory.path().join("run");
                let agreement_at=case.inputs.iter().find(|input|input.id == "cancel-apply").unwrap().at_ms;
                let mut session = simulation::artifacts::Session::open(case,&path,std::path::Path::new(".")).await.unwrap();
                while session.world.next_ms().is_some_and(|at|at<agreement_at) { assert!(session.step().await.unwrap()); }
                let person=session.world.resolve_reference("$beto");
                let transfer=session.world.resolve_reference("$routes");
                let query=protein::Protein {source:protein::Source::Transfer,filter:vec![protein::Predicate::UidEq(transfer.clone())],fields:None,include:Default::default(),aggregate:None,order:vec![],limit:None};
                let rows=protein::execute_for_with_signer(&session.world.nodes["b"].engine().store,&query,None,Some(&person)).await.unwrap();
                let row=rows.iter().find(|row|row["uid"]==transfer).unwrap();
                assert_eq!(row["occurrences"][0]["cancellations"][0]["capabilities"]["apply_cancellation"],true);
                let locked=protein::execute_for_with_signer(&session.world.nodes["b"].engine().store,&query,None,None).await.unwrap();
                let row=locked.iter().find(|row|row["uid"]==transfer).unwrap();
                assert_eq!(row["occurrences"][0]["cancellations"][0]["capabilities"]["apply_cancellation"],false);
                while session.step().await.unwrap() {}
                let record = session.world.resolve_reference("$stock-a");
                let occurrence = session.world.resolve_reference("$activate");
                let transfer = session.world.resolve_reference("$routes");
                let pool = &session.world.nodes["a"].engine().store.pool;
                let balance = store::transfer_balances::read(pool, &record).await.unwrap();
                assert!(balance.incomplete.is_empty(), "{:?}",balance.incomplete);
                assert_eq!([balance.actual.to_f64(),balance.reserved.to_f64(),balance.surplus.to_f64()],[26.0,0.0,26.0]);
                let progress = store::transfers::occurrence_settlement_progress(pool,&occurrence).await.unwrap().unwrap();
                assert_eq!([progress.settled_quantity,progress.cancelled_quantity,progress.remaining_quantity],[4.0,6.0,0.0]);
                assert!(!progress.settled);
                let applied: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_cancellation_application").fetch_one(pool).await.unwrap();
                assert_eq!(applied,1);
                let projections:Vec<String> = store::sqlx::query_scalar("SELECT projection FROM transfer_remote_reference WHERE transfer_uid = ?").bind(&transfer).fetch_all(&session.world.nodes["b"].engine().store.pool).await.unwrap();
                assert!(!projections.is_empty());
                for projection in projections {
                    let value:serde_json::Value = serde_json::from_str(&projection).unwrap();
                    if value["primary_status"] != "cancelled" {
                        let promises=store::transfers::promises_of(pool,&transfer).await.unwrap();
                        let states=promises.iter().map(|p|(&p.uid,p.state)).collect::<Vec<_>>();
                        let run=session.finish().await.unwrap();
                        panic!("remote status: {value}; states: {states:?}; findings: {:?}; {}",run.findings,directory.keep().display());
                    }
                    assert_eq!(value["occurrences"][0]["settlement_progress"]["cancelled_quantity"],6.0);
                    assert!(!projection.contains(&record));
                }
                let run = session.finish().await.unwrap();
                if run.result.verdict != Verdict::Passed { panic!("{:?}; {}",run.findings,directory.keep().display()); }
                assert!(matches!(simulation::artifacts::replay(&path,&directory.path().join("replay")).await.unwrap(),ReplayStatus::Verified{..}));
            }
        });
    }).unwrap().join().unwrap();
}
