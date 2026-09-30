use engine::actions::Action;
use nucleus::transfer::application::{EffectMode, PrivateEffect};

use crate::{
    assumptions::{Direction, TransferAssumption, TransferSource},
    scenario::{Event, Input, Invocation, Scenario},
};

pub fn grouped_needs(replicated: bool, assumed: bool, ride_first: bool) -> Scenario {
    let mut case = super::independent_donation(replicated);
    case.name = format!("grouped-needs-{replicated}-{assumed}-{ride_first}");
    case.checks.retain(|check| {
        !matches!(
            check.predicate,
            nucleus::simulation::Predicate::Quantity { .. }
        )
    });
    let receiver = case.cells.iter_mut().find(|cell| cell.name == "b").unwrap();
    for seed in &mut receiver.seed {
        if let Action::CreateRecord { slug, quantity, .. } = &mut seed.action
            && slug.as_deref() == Some("stock-b")
        {
            *quantity = -1.0;
        }
    }
    receiver.seed.push(Invocation {
        id: "transport".into(),
        actor: None,
        action: Action::CreateRecord {
            slug: Some("transport".into()),
            kind: nucleus::RecordKind::Plain,
            head: "Transport Need".into(),
            body: String::new(),
            quantity: -1.0,
        },
    });
    let hide = case
        .inputs
        .iter()
        .find(|input| input.id == "hide-stock-b")
        .unwrap();
    case.inputs.push(Input {id:"hide-transport".into(),cell:"b".into(),at_ms:hide.at_ms+1,event:Event::Action {invocation:Invocation {
        id:"hide-transport".into(),actor:None,action:serde_json::from_value(serde_json::json!({"action":"hide-record-from-contact","target":"$cell:a:organ","record":"$transport","hidden":true})).unwrap(),
    }}});
    let receive = case
        .inputs
        .iter()
        .find(|input| input.id == "receive-apples")
        .unwrap()
        .clone();
    case.inputs.push(Input {
        id: "private-needs".into(),
        cell: "b".into(),
        at_ms: receive.at_ms - 2,
        event: Event::Action {
            invocation: Invocation {
                id: "private-needs".into(),
                actor: None,
                action: Action::SetTransferPrivateApplicationPolicy {
                    transfer: "$routes".into(),
                    exchange: "apples".into(),
                    person: "$beto".into(),
                    expected_version: 0,
                    request_id: "private-needs".into(),
                    effects: vec![
                        PrivateEffect {
                            record: "$stock-b".into(),
                            formula: "incoming() / 10".into(),
                            mode: EffectMode::Quantity,
                        },
                        PrivateEffect {
                            record: "$transport".into(),
                            formula: "incoming() / 10".into(),
                            mode: EffectMode::Fulfilment,
                        },
                    ],
                },
            },
        },
    });
    if ride_first {
        case.inputs.push(Input {
            id: "ride".into(),
            cell: "b".into(),
            at_ms: receive.at_ms - 1,
            event: Event::Action {
                invocation: Invocation {
                    id: "ride".into(),
                    actor: None,
                    action: Action::AddQuantityExact {
                        target: "$transport".into(),
                        delta: store::exact::integer(1),
                    },
                },
            },
        });
    }
    if assumed {
        let assumption = |amount| Event::AssumeTransfer {
            assumption: TransferAssumption {
                key: "bike".into(),
                title: "Bike outcome".into(),
                person: "$beto".into(),
                record: "$stock-b".into(),
                quantity: store::exact::integer(amount),
                unit: None,
                direction: Direction::Incoming,
                source: Some(TransferSource {
                    transfer: "$routes".into(),
                    revision: 2,
                    promise: "apples".into(),
                    exchange: "apples".into(),
                    occurrence: Some("$activate".into()),
                }),
            },
        };
        case.inputs
            .iter_mut()
            .find(|input| input.id == receive.id)
            .unwrap()
            .event = assumption(4);
        case.inputs.push(Input {
            id: "receive-rest".into(),
            at_ms: receive.at_ms + 5,
            event: assumption(6),
            ..receive
        });
        case.inputs.retain(|input| {
            !matches!(
                input.id.as_str(),
                "application-acknowledgement" | "application-confirmed"
            )
        });
    } else {
        let settlement = case
            .inputs
            .iter_mut()
            .find(|input| input.id == "settle")
            .unwrap();
        if let Event::SettleReviewed { quantity, .. } = &mut settlement.event {
            *quantity = store::exact::integer(4);
        }
        let at_ms = settlement.at_ms + 5;
        case.inputs.push(Input {
            id: "settle-rest".into(),
            cell: "a".into(),
            at_ms,
            event: Event::SettleReviewed {
                occurrence: "$activate".into(),
                person: "$ana".into(),
                quantity: store::exact::integer(6),
            },
        });
        case.inputs.push(Input {
            id: "receive-rest".into(),
            at_ms: receive.at_ms + 5,
            ..receive
        });
    }
    for record in ["stock-b", "transport"] {
        case.checks.push(nucleus::simulation::CheckDefinition {
            id: format!("need-{record}"),
            predicate: nucleus::simulation::Predicate::Quantity {
                cell: "b".into(),
                record: format!("${record}"),
                comparison: nucleus::simulation::Comparison::Equal,
                expected: nucleus::simulation::Quantity {
                    value: store::exact::zero(),
                    unit: None,
                },
            },
            options: Default::default(),
        });
    }
    case.inputs.sort_by_key(|input| input.at_ms);
    case
}
