use engine::actions::Action;
use simulation::scenario::{Event, Input};

#[test]
fn partial_receipts_require_order_and_replays_preserve_the_original_change() {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            for replicated in [false, true] {
            let mut case = simulation::fixtures::transfer::independent_donation(replicated);
            let settlement = case.inputs.iter_mut().find(|input| input.id == "settle").unwrap();
            if let Event::SettleReviewed { quantity, .. } = &mut settlement.event {
                *quantity = nucleus::DecimalValue::parse_inferred("4").unwrap();
            }
            let settlement_at = settlement.at_ms;
            case.inputs.push(Input { id: "settle-rest".into(), cell: "a".into(), at_ms: settlement_at + 5,
                event: Event::SettleReviewed { occurrence: "$activate".into(), person: "$ana".into(), quantity: nucleus::DecimalValue::parse_inferred("6").unwrap() } });
            let receive = case.inputs.iter().find(|input| input.id == "receive-apples").unwrap().clone();
            case.inputs.push(Input { id: "receive-rest".into(), at_ms: receive.at_ms + 5, ..receive.clone() });
            case.inputs.sort_by_key(|input| input.at_ms);
            let directory = tempfile::tempdir().unwrap();
            let mut session = simulation::artifacts::Session::open(case, &directory.path().join("run"), std::path::Path::new(".")).await.unwrap();
            while session.world.next_ms().is_some_and(|time| time < receive.at_ms) {
                assert!(session.step().await.unwrap());
            }
            let node = &session.world.nodes["b"];
            let stock = store::records::resolve(&node.engine().store.pool, "stock-b").await.unwrap().unwrap();
            let handoffs: Vec<(String,String,String)> = store::sqlx::query_as("SELECT uid, transfer_uid, participant_person_uid FROM transfer_remote_application_handoff ORDER BY canonical_cumulative_before")
                .fetch_all(&node.engine().store.pool).await.unwrap();
            assert_eq!(handoffs.len(), 2);
            let formula = store::config::transfer_application_formula(&node.engine().store.pool).await.unwrap();
            let action = |index: usize, request: &str, delta: f64, before: f64| -> Action {
                serde_json::from_value(serde_json::json!({
                    "action":"apply-transfer-application", "transfer":handoffs[index].1,
                    "handoff":handoffs[index].0, "person":handoffs[index].2, "local_record":stock.uid,
                    "expected_formula_hash":nucleus::transfer::occurrence_application_formula_hash(&formula),
                    "expected_formula_version":0, "expected_local_delta":delta,
                    "expected_local_cumulative_before":before, "request_id":request,
                })).unwrap()
            };
            let second = action(1, "too-early", 10.0, 0.0);
            let error = node.execution.scope(node.engine().act(second, None)).await.unwrap_err();
            assert!(error.to_string().contains("earlier slice"), "{error}");
            let error = node.execution.scope(node.engine().act(action(0, "stale", 400.0, 0.0), None)).await.unwrap_err();
            assert!(error.to_string().contains("changed after review"), "{error}");
            let replay_first = action(0, "receive-apples", 4.0, 0.0);
            let replay_second = action(1, "receive-rest", 6.0, 4.0);
            while session.step().await.unwrap() {}
            let node = &session.world.nodes["b"];
            let before = node.engine().store.state_hash().await.unwrap();
            for action in [replay_first, replay_second] {
                let result = node.execution.scope(node.engine().act(action, None)).await.unwrap();
                assert!(result.facts.is_empty());
            }
            assert_eq!(before, node.engine().store.state_hash().await.unwrap());
            let record = store::records::get(&node.engine().store.pool, &stock.uid).await.unwrap().unwrap();
            assert_eq!(record.quantity.to_f64(), 10.0);
            let result = session.finish().await.unwrap();
            if result.result.verdict != nucleus::simulation::Verdict::Passed {
                panic!("{:?} {:?}; evidence retained in {}", result.result, result.findings, directory.keep().display());
            }
            }
        });
    }).unwrap().join().unwrap();
}
