use engine::actions::{Action, TransferReservePoint};
use nucleus::simulation::{Predicate, ReplayStatus, Verdict};
use simulation::assumptions::{Direction, TransferAssumption, TransferSource};
use simulation::scenario::{Event, Input, Invocation};

#[test]
fn assumed_delivery_releases_copied_reservations_without_public_settlement() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    let mut case = simulation::fixtures::transfer::independent_donation(false);
                    case.name = "remaining-stock-scenario".into();
                    let settle = case
                        .inputs
                        .iter()
                        .position(|input| input.id == "settle")
                        .unwrap();
                    case.inputs.truncate(settle + 1);
                    case.checks
                        .retain(|check| !matches!(check.predicate, Predicate::Quantity { .. }));
                    for input in &mut case.inputs {
                        if let Event::Action { invocation } = &mut input.event
                            && let Action::CreateTransferDraft {
                                reserve_default, ..
                            } = &mut invocation.action
                        {
                            *reserve_default = TransferReservePoint::Agreed;
                        }
                    }
                    let last = case.inputs.last_mut().unwrap();
                    let at = last.at_ms;
                    last.event = Event::AssumeTransfer {
                        assumption: TransferAssumption {
                            key: "expected-delivery".into(),
                            title: "If ten apples are delivered".into(),
                            person: "$ana".into(),
                            record: "$stock-a".into(),
                            quantity: nucleus::DecimalValue::parse_inferred("10").unwrap(),
                            unit: None,
                            direction: Direction::Outgoing,
                            source: Some(TransferSource {
                                transfer: "$routes".into(),
                                revision: 2,
                                promise: "apples".into(),
                                exchange: "apples".into(),
                                occurrence: Some("$activate".into()),
                            }),
                        },
                    };
                    case.inputs.push(Input {
                        id: "stock-limit".into(),
                        cell: "a".into(),
                        at_ms: at - 1,
                        event: Event::Action {
                            invocation: Invocation {
                                id: "stock-limit".into(),
                                actor: None,
                                action: Action::SetRecordStockLimit {
                                    record: "$stock-a".into(),
                                    person: "$ana".into(),
                                    minimum: Some(
                                        nucleus::DecimalValue::parse_inferred("20").unwrap(),
                                    ),
                                    expected_version: 0,
                                    request_id: "stock-limit".into(),
                                },
                            },
                        },
                    });
                    case.inputs.sort_by_key(|input| input.at_ms);
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
                    let record = session.world.resolve_reference("$stock-a");
                    let occurrence = session.world.resolve_reference("$activate");
                    let node = &session.world.nodes["a"];
                    let balance = node
                        .execution
                        .scope(store::transfer_balances::read(
                            &node.engine().store.pool,
                            &record,
                        ))
                        .await
                        .unwrap();
                    assert!(balance.incomplete.is_empty(), "{:?}", balance.incomplete);
                    assert_eq!(balance.actual.to_f64(), 20.0);
                    assert!(balance.reserved.is_zero());
                    assert_eq!(balance.surplus.to_f64(), 20.0);
                    let public = store::transfers::occurrence_settlement_progress(
                        &node.engine().store.pool,
                        &occurrence,
                    )
                    .await
                    .unwrap()
                    .unwrap();
                    assert_eq!(public.remaining_quantity, 10.0);
                    assert_eq!(public.settled_quantity, 0.0);
                    let run = session.finish().await.unwrap();
                    assert_eq!(run.result.verdict, Verdict::Passed, "{:?}", run.findings);
                    let replay =
                        simulation::artifacts::replay(&path, &directory.path().join("replay"))
                            .await
                            .unwrap();
                    assert!(matches!(replay, ReplayStatus::Verified { .. }), "{replay:?}");
                });
        })
        .unwrap()
        .join()
        .unwrap();
}
