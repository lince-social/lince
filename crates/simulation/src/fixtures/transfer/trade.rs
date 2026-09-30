use crate::scenario::{Check, Event, Input, Invocation, Scenario};
use nucleus::simulation::{Comparison, Predicate, Quantity};
use serde_json::{Value, json};

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

fn publish(case: &mut Scenario, id: &str) {
    command(
        case,
        "a",
        &format!("enqueue-{id}"),
        json!({"action":"enqueue-transfer-delivery","transfer":"$routes","delivery":"$policy-b","person":"$ana","request_id":format!("enqueue-{id}")}),
    );
    send(case, "a", "b", id);
}

pub fn trade(replicated: bool) -> Scenario {
    let mut case = crate::fixtures::transfer::independent_donation(replicated);
    case.name = format!("independent-trade-{replicated}");
    for cell in &mut case.cells {
        let quantity = if cell.name == "a" { 0 } else { 100 };
        cell.seed.push(invocation(&format!("money-{}",cell.name), json!({"action":"create-record","slug":format!("money-{}",cell.name),"kind":"plain","head":"Private money","body":"","quantity":quantity})));
        if cell.name == "a" {
            for seed in &mut cell.seed {
                if seed.id == "stock-a" {
                    if let engine::actions::Action::CreateRecord { quantity, .. } = &mut seed.action
                    {
                        *quantity = 1.0;
                    }
                }
            }
        }
    }
    let mut extra = Vec::new();
    for input in &mut case.inputs {
        if input.id.starts_with("hide-stock-") {
            let peer = if input.cell == "a" { "b" } else { "a" };
            let id = format!("hide-money-{}", input.cell);
            extra.push(Input { id:id.clone(),cell:input.cell.clone(),at_ms:input.at_ms+1,event:Event::Action { invocation:invocation(&id,json!({"action":"hide-record-from-contact","target":format!("$cell:{peer}:organ"),"record":format!("$money-{}",input.cell),"hidden":true})) } });
        }
        if input.id == "routes" {
            input.event = Event::Action {
                invocation: invocation(
                    "routes",
                    json!({
                        "action":"create-transfer-draft","request_id":"routes","creator":"$ana","slug":"trade","head":"Bike for ten money",
                        "agreement":"full","visibility":"hidden","reserve_default":"none","invitees":["$beto"],"promises":[
                            {"uid":"apples","record":"$stock-a","party":"$ana","delta":-1,"item":{"title":"Bike","exchange":{"uid":"bike","giver":"$ana","receiver":"$beto"}}},
                            {"uid":"payment","party":"$beto","delta":-10,"item":{"title":"Payment","exchange":{"uid":"payment","giver":"$beto","receiver":"$ana"}}}
                        ]
                    }),
                ),
            };
        }
        if let Event::SettleReviewed { quantity, .. } = &mut input.event {
            *quantity = nucleus::DecimalValue::parse_inferred("1").unwrap();
        }
    }
    case.inputs.extend(extra);
    case.inputs.sort_by_key(|input| input.at_ms);
    case.checks
        .retain(|check| !check.id.starts_with("balance-"));
    command(
        &mut case,
        "b",
        "activate-payment",
        json!({"action":"activate-transfer-occurrence","transfer":"$routes","promise":"payment","person":"$beto","expected_revision":2,"request_id":"activate-payment"}),
    );
    publish(&mut case, "payment-active");
    for (cell, person, role) in [("b", "beto", "delivery"), ("a", "ana", "receipt")] {
        command(
            &mut case,
            cell,
            &format!("payment-{role}"),
            json!({"action":"set-transfer-occurrence-claim","occurrence":"$activate-payment","person":format!("${person}"),"request_id":format!("payment-{role}"),"role":role,"claimed":true}),
        );
    }
    publish(&mut case, "payment-claims");
    for (part, amount, remaining) in [("first", 4, 10), ("last", 6, 6)] {
        let id = format!("prepare-{part}");
        command(
            &mut case,
            "b",
            &id,
            json!({"action":"begin-transfer-settlement","transfer":"$routes","occurrence":"$activate-payment","person":"$beto","expected_revision":2,"expected_remaining_quantity":remaining,"canonical_quantity":amount,"request_id":id}),
        );
        send(&mut case, "a", "b", &format!("prepared-{part}"));
        add(
            &mut case,
            "b",
            &format!("pay-{part}"),
            Event::ApplyReceivedTransfer {
                transfer: "$routes".into(),
                occurrence: "$activate-payment".into(),
                person: "$beto".into(),
                local_record: "$money-b".into(),
            },
        );
        send(&mut case, "b", "a", &format!("paid-{part}"));
        add(
            &mut case,
            "a",
            &format!("receive-{part}"),
            Event::ApplyReceivedTransfer {
                transfer: "$routes".into(),
                occurrence: "$activate-payment".into(),
                person: "$ana".into(),
                local_record: "$money-a".into(),
            },
        );
    }
    publish(&mut case, "completed");
    add(&mut case, "b", "restart-payment", Event::Restart {});
    for (cell, record, expected) in [
        ("a", "stock-a", "0"),
        ("b", "stock-b", "1"),
        ("a", "money-a", "10"),
        ("b", "money-b", "90"),
    ] {
        case.checks.push(Check {
            id: format!("final-{record}"),
            options: Default::default(),
            predicate: Predicate::Quantity {
                cell: cell.into(),
                record: format!("${record}"),
                comparison: Comparison::Equal,
                expected: Quantity {
                    value: nucleus::DecimalValue::parse_inferred(expected).unwrap(),
                    unit: None,
                },
            },
        });
    }
    case
}

pub fn counteroffer(replicated: bool) -> Scenario {
    let mut case = crate::fixtures::transfer::independent_donation(replicated);
    case.name.push_str("-counteroffer");
    let split = case
        .inputs
        .iter()
        .position(|input| input.id == "activate")
        .unwrap();
    let suffix = case.inputs.split_off(split);
    command(
        &mut case,
        "b",
        "counteroffer",
        json!({
            "action":"counteroffer-transfer","transfer":"$routes","person":"$beto","expected_revision":2,"request_id":"counteroffer",
            "draft":{"creator":"$ana","head":"Five apples","slug":"donation","agreement":"full","visibility":"hidden","reserve_default":"none","invitees":[],"promises":[
                {"uid":"apples","party":"$ana","delta":-5,"item":{"title":"Apples","description":"Five apples","exchange":{"uid":"apples","giver":"$ana","receiver":"$beto"}}}
            ]}
        }),
    );
    command(
        &mut case,
        "a",
        "enqueue-revised",
        json!({
            "action":"enqueue-transfer-delivery", "transfer":"$routes", "delivery":"$policy-b",
            "person":"$ana", "request_id":"enqueue-revised"
        }),
    );
    add(
        &mut case,
        "b",
        "refresh-revised",
        Event::RefreshTransfer {
            transfer: "$routes".into(),
            person: "$beto".into(),
        },
    );
    send(&mut case, "b", "a", "pull-revised");
    command(
        &mut case,
        "a",
        "stale-activation",
        json!({"action":"activate-transfer-occurrence","transfer":"$routes","promise":"apples","person":"$ana","expected_revision":2,"request_id":"stale-activation"}),
    );
    case.checks.push(Check {
        id: "stale-agreement-refused".into(),
        options: Default::default(),
        predicate: Predicate::ExpectedRefusal {
            input: "stale-activation".into(),
            refusal: nucleus::simulation::Refusal::Conflict {
                code: "transfer_promise_not_ready".into(),
            },
        },
    });
    for (cell, person) in [("a", "ana"), ("b", "beto")] {
        for level in [1, 2] {
            let id = format!("new-agreement-{cell}-{level}");
            command(
                &mut case,
                cell,
                &id,
                json!({"action":"set-transfer-agreement-level","transfer":"$routes","person":format!("${person}"),"expected_revision":3,"request_id":id,"level":level}),
            );
        }
    }
    command(
        &mut case,
        "a",
        "stale-after-agreement",
        json!({
            "action":"activate-transfer-occurrence", "transfer":"$routes", "promise":"apples",
            "person":"$ana", "expected_revision":2, "request_id":"stale-after-agreement"
        }),
    );
    case.checks.push(Check {
        id: "stale-revision-refused".into(),
        options: Default::default(),
        predicate: Predicate::ExpectedRefusal {
            input: "stale-after-agreement".into(),
            refusal: nucleus::simulation::Refusal::Conflict {
                code: "transfer_revision_stale".into(),
            },
        },
    });
    for mut input in suffix {
        match &mut input.event {
            Event::Action { invocation } => {
                if let engine::actions::Action::ActivateTransferOccurrence {
                    expected_revision,
                    ..
                } = &mut invocation.action
                {
                    *expected_revision = 3;
                }
            }
            Event::SettleReviewed { quantity, .. } => {
                *quantity = nucleus::DecimalValue::parse_inferred("5").unwrap()
            }
            _ => {}
        }
        add(&mut case, &input.cell, &input.id, input.event);
    }
    for check in &mut case.checks {
        if let Predicate::Quantity { cell, expected, .. } = &mut check.predicate {
            expected.value =
                nucleus::DecimalValue::parse_inferred(if cell == "a" { "25" } else { "5" })
                    .unwrap();
        }
    }
    case
}

pub fn open_offer(replicated: bool) -> Scenario {
    let mut case = crate::fixtures::transfer::donation_without_private_source(replicated);
    case.name.push_str("-public-open");
    for input in &mut case.inputs {
        if let Event::Action { invocation } = &mut input.event {
            if let engine::actions::Action::CreateTransferDraft {
                promises,
                invitees,
                visibility,
                ..
            } = &mut invocation.action
            {
                invitees.clear();
                *visibility = engine::actions::TransferVisibility::Public;
                promises[0].open = true;
                promises[0].reuse_policy = nucleus::transfer::OpenPromiseReusePolicy::Consume;
                promises[0].item.as_mut().unwrap().exchange = None;
            }
        }
    }
    let agreement = case
        .inputs
        .iter()
        .position(|input| input.id == "agreement-a-1")
        .unwrap();
    let suffix = case.inputs.split_off(agreement);
    let accept = case
        .inputs
        .iter()
        .position(|input| input.id == "accept")
        .unwrap();
    case.inputs.truncate(accept);
    command(
        &mut case,
        "b",
        "claim",
        json!({"action":"claim-open-transfer-promise","transfer":"$routes","promise":"apples","person":"$beto","expected_revision":1,"request_id":"claim","terms":{"record":"","party":"$beto","delta":10,"reuse_policy":"consume"}}),
    );
    publish(&mut case, "claimed");
    for input in suffix {
        if input.id == "receive-apples" {
            command(
                &mut case,
                "b",
                "prepare-open-receipt",
                json!({"action":"begin-transfer-settlement","transfer":"$routes","occurrence":"$activate-claim","person":"$beto","expected_revision":2,"expected_remaining_quantity":10,"canonical_quantity":10,"request_id":"prepare-open-receipt"}),
            );
            send(&mut case, "a", "b", "prepared-open-receipt");
            add(
                &mut case,
                "b",
                "receive-apples",
                Event::ApplyReceivedTransfer {
                    transfer: "$routes".into(),
                    occurrence: "$activate-claim".into(),
                    person: "$beto".into(),
                    local_record: "$stock-b".into(),
                },
            );
        } else {
            let active = input.id == "send-active";
            add(&mut case, &input.cell, &input.id, input.event);
            if active {
                command(
                    &mut case,
                    "b",
                    "activate-claim",
                    json!({"action":"activate-transfer-occurrence","transfer":"$routes","promise":"$claim","person":"$beto","expected_revision":2,"request_id":"activate-claim"}),
                );
                publish(&mut case, "claim-active");
                for (cell, person, role) in [("a", "ana", "delivery"), ("b", "beto", "receipt")] {
                    let id = format!("claim-{role}");
                    command(
                        &mut case,
                        cell,
                        &id,
                        json!({"action":"set-transfer-occurrence-claim","occurrence":"$activate-claim","person":format!("${person}"),"role":role,"claimed":true,"request_id":id}),
                    );
                }
                publish(&mut case, "claim-confirmed");
            }
        }
    }
    case
}

pub fn private_trade(replicated: bool) -> Scenario {
    let mut case = trade(replicated);
    case.name.push_str("-private");
    let at_ms = case
        .inputs
        .iter()
        .find(|input| input.id == "prepare-first")
        .unwrap()
        .at_ms
        - 5;
    case.inputs.push(Input {
            id: "private-half-payment".into(), cell: "b".into(), at_ms,
            event: Event::Action { invocation: invocation("private-half-payment", json!({
                "action":"set-transfer-private-application-policy", "transfer":"$routes", "exchange":"payment",
                "effects":[{"record":"$money-b","formula":"-incoming() / 2","mode":"quantity"}], "person":"$beto", "expected_version":0, "request_id":"private-half-payment"
            })) },
        });
    case.inputs.sort_by_key(|input| input.at_ms);
    for check in &mut case.checks {
        if check.id == "final-money-b" {
            if let Predicate::Quantity { expected, .. } = &mut check.predicate {
                expected.value = nucleus::DecimalValue::parse_inferred("95").unwrap();
            }
        }
    }
    case
}
