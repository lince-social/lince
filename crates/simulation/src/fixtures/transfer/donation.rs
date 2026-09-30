use nucleus::simulation::{Comparison, Evaluation, Predicate, Quantity};
use serde_json::json;

use super::routes::{action, add, decision, invocation, publish, send};
use crate::scenario::{Cell, Check, Event, Input, Scenario};

pub fn donation_without_private_source(replicated: bool) -> Scenario {
    let mut case = independent_donation(replicated);
    case.name.push_str("-unbound");
    for input in &mut case.inputs {
        if let Event::Action { invocation } = &mut input.event
            && let engine::actions::Action::CreateTransferDraft { promises, .. } =
                &mut invocation.action
        {
            promises[0].record.clear();
        }
    }
    let settle = case
        .inputs
        .iter_mut()
        .find(|input| input.id == "settle")
        .unwrap();
    settle.event = Event::Action {
        invocation: invocation(
            "settle",
            json!({
                "action":"begin-transfer-settlement","transfer":"$routes","occurrence":"$activate","person":"$ana",
                "expected_revision":2,"expected_remaining_quantity":10,"canonical_quantity":10,"request_id":"settle"
            }),
        ),
    };
    let at_ms = settle.at_ms + 5;
    case.inputs.push(Input {
        id: "apply-own-stock".into(),
        cell: "a".into(),
        at_ms,
        event: Event::ApplyReceivedTransfer {
            transfer: "$routes".into(),
            occurrence: "$activate".into(),
            person: "$ana".into(),
            local_record: "$stock-a".into(),
        },
    });
    case.inputs.sort_by_key(|input| input.at_ms);
    case
}

pub fn donation_with_lost_acknowledgements(replicated: bool) -> Scenario {
    let mut case = independent_donation(replicated);
    case.name.push_str("-lost-acknowledgements");
    for id in ["agreement-b-1", "application-acknowledgement"] {
        let input = case.inputs.iter_mut().find(|input| input.id == id).unwrap();
        let at = input.at_ms;
        if let Event::TransferCommand {
            copies,
            duplicate_spacing_ms,
            ..
        } = &mut input.event
        {
            *copies = 2;
            *duplicate_spacing_ms = 3;
        }
        for (suffix, offset, event) in [
            (
                "disconnect",
                2,
                Event::Link {
                    peer: "a".into(),
                    connected: false,
                },
            ),
            (
                "reconnect",
                6,
                Event::Link {
                    peer: "a".into(),
                    connected: true,
                },
            ),
            ("restart-before-acknowledgement", 4, Event::Restart {}),
            (
                "retry",
                7,
                Event::TransferDelivery {
                    peer: "a".into(),
                    delay_ms: 1,
                    copies: 2,
                    duplicate_spacing_ms: 2,
                    drop: false,
                },
            ),
        ] {
            case.inputs.push(Input {
                id: format!("{id}-{suffix}"),
                cell: "b".into(),
                at_ms: at + offset,
                event,
            });
        }
        case.checks.push(Check {
            id: format!("{id}-duplicate-refused"),
            options: Default::default(),
            predicate: Predicate::ExpectedMessageRefusal {
                input: format!("{id}-retry"),
                copy: 1,
                refusal: nucleus::simulation::Refusal::InvalidAction {},
            },
        });
    }
    case.inputs.sort_by_key(|input| input.at_ms);
    case
}

pub fn independent_donation(replicated: bool) -> Scenario {
    let mut case = crate::fixtures::daily();
    case.name = if replicated {
        "donation-replicated"
    } else {
        "donation-hosted"
    }
    .into();
    case.cells.clear();
    case.inputs.clear();
    case.end_ms = case.start_ms + 10_000;
    case.checks.retain(|check| {
        matches!(
            check.predicate,
            Predicate::FactChain {} | Predicate::NoUnexpectedRefusals {}
        )
    });
    for (cell, person, stock) in [("a", "ana", 30), ("b", "beto", 0)] {
        case.cells.push(Cell { name: cell.into(), database: None, lingua: Vec::new(), seed: vec![
            invocation(person, json!({"action":"create-record", "slug":person, "kind":"person", "head":person, "body":"", "quantity":1})),
            invocation(&format!("stock-{cell}"), json!({"action":"create-record", "slug":format!("stock-{cell}"), "kind":"plain", "head":"Private apples", "body":"", "quantity":stock})),
            invocation(&format!("public-{cell}"), json!({"action":"grant-visibility", "subject_kind":"public", "target":format!("${person}")})),
        ] });
        add(
            &mut case,
            cell,
            &format!("key-{cell}"),
            Event::PersonKey {
                person: format!("${person}"),
            },
        );
    }
    add(&mut case, "a", "pair", Event::Pair { peer: "b".into() });
    for (from, to) in [("a", "b"), ("b", "a")] {
        action(
            &mut case,
            from,
            &format!("hide-stock-{from}"),
            json!({"action":"hide-record-from-contact", "target":format!("$cell:{to}:organ"), "record":format!("$stock-{from}"), "hidden":true}),
        );
        action(
            &mut case,
            from,
            &format!("sync-policy-{from}"),
            json!({"action":"set-sync-policy", "target":format!("$cell:{to}:organ"), "sync_in":true, "sync_out":true}),
        );
        add(
            &mut case,
            from,
            &format!("identities-{from}"),
            Event::Sync {
                peer: to.into(),
                delay_ms: 1,
                copies: 1,
                duplicate_spacing_ms: 0,
                drop: false,
            },
        );
    }
    action(
        &mut case,
        "a",
        "routes",
        json!({
            "action":"create-transfer-draft", "request_id":"routes", "creator":"$ana", "slug":"donation", "head":"Ten apples for Beto",
            "agreement":"full", "visibility":"hidden", "reserve_default":"none", "invitees":["$beto"],
            "promises":[{"uid":"apples", "record":"$stock-a", "party":"$ana", "delta":-10,
                "item":{"title":"Apples", "description":"Ten apples", "exchange":{"uid":"apples", "giver":"$ana", "receiver":"$beto"}}}]
        }),
    );
    action(
        &mut case,
        "a",
        "policy-b",
        json!({"action":"configure-transfer-delivery", "transfer":"$routes", "recipient_person":"$beto", "recipient_organ":"$cell:b:organ", "person":"$ana", "request_id":"policy-b", "mode": if replicated {"replicated"} else {"hosted"}}),
    );
    publish(&mut case, "b", "invitation");
    add(
        &mut case,
        "b",
        "accept",
        Event::AcceptInvitation {
            transfer: "$routes".into(),
            person: "$beto".into(),
            peer: Some("a".into()),
        },
    );
    publish(&mut case, "b", "accepted");
    for (cell, person) in [("a", "ana"), ("b", "beto")] {
        for level in [1, 2] {
            let id = format!("agreement-{cell}-{level}");
            decision(
                &mut case,
                cell,
                &id,
                json!({"action":"set-transfer-agreement-level", "transfer":"$routes", "person":format!("${person}"), "expected_revision":2, "request_id":id, "level":level}),
            );
        }
    }
    action(
        &mut case,
        "a",
        "activate",
        json!({"action":"activate-transfer-occurrence", "transfer":"$routes", "promise":"apples", "person":"$ana", "expected_revision":2, "request_id":"activate"}),
    );
    publish(&mut case, "b", "active");
    for (cell, person, role) in [("a", "ana", "delivery"), ("b", "beto", "receipt")] {
        decision(
            &mut case,
            cell,
            role,
            json!({"action":"set-transfer-occurrence-claim", "occurrence":"$activate", "person":format!("${person}"), "request_id":role, "role":role, "claimed":true}),
        );
    }
    add(
        &mut case,
        "a",
        "settle",
        Event::SettleReviewed {
            occurrence: "$activate".into(),
            person: "$ana".into(),
            quantity: nucleus::DecimalValue::parse_inferred("10").unwrap(),
        },
    );
    publish(&mut case, "b", "settled");
    add(
        &mut case,
        "b",
        "receive-apples",
        Event::ApplyReceivedTransfer {
            transfer: "$routes".into(),
            occurrence: "$activate".into(),
            person: "$beto".into(),
            local_record: "$stock-b".into(),
        },
    );
    send(&mut case, "b", "a", "application-acknowledgement");
    send(&mut case, "a", "b", "application-confirmed");
    add(&mut case, "b", "restart", Event::Restart {});
    for (cell, amount) in [("a", "20"), ("b", "10")] {
        case.checks.push(Check {
            id: format!("balance-{cell}"),
            predicate: Predicate::Quantity {
                cell: cell.into(),
                record: format!("$stock-{cell}"),
                comparison: Comparison::Equal,
                expected: Quantity {
                    value: nucleus::DecimalValue::parse_inferred(amount).unwrap(),
                    unit: None,
                },
            },
            options: nucleus::simulation::CheckOptions {
                evaluation: Evaluation::End,
                ..Default::default()
            },
        });
    }
    case
}

pub fn partial_cancellation(replicated: bool) -> Scenario {
    let mut case = independent_donation(replicated);
    case.name = format!("partial-cancellation-{replicated}");
    let settle = case
        .inputs
        .iter()
        .position(|input| input.id == "settle")
        .unwrap();
    case.inputs.truncate(settle + 1);
    for input in &mut case.inputs {
        match &mut input.event {
            Event::SettleReviewed { quantity, .. } => {
                *quantity = nucleus::DecimalValue::parse_inferred("4").unwrap()
            }
            Event::Action { invocation } => {
                if let engine::actions::Action::CreateTransferDraft {
                    reserve_default, ..
                } = &mut invocation.action
                {
                    *reserve_default = engine::actions::TransferReservePoint::Agreed;
                }
            }
            _ => {}
        }
    }
    publish(&mut case, "b", "partial");
    decision(
        &mut case,
        "b",
        "cancel-proposal",
        json!({
            "action":"propose-transfer-cancellation", "transfer":"$routes", "occurrence":"$activate", "person":"$beto",
            "expected_revision":2,"expected_remaining_quantity":{"scale":0,"value":"6"},"request_id":"cancel-proposal"
        }),
    );
    publish(&mut case, "b", "cancellation-proposed");
    for (cell, person) in [("a", "ana"), ("b", "beto")] {
        for level in [1, 2] {
            let id = format!("cancel-agreement-{cell}-{level}");
            decision(
                &mut case,
                cell,
                &id,
                json!({
                    "action":"set-transfer-agreement-level","transfer":"$routes","person":format!("${person}"),
                    "expected_revision":3,"request_id":id,"level":level
                }),
            );
        }
    }
    publish(&mut case, "b", "cancellation-agreed");
    for id in ["cancel-apply"] {
        decision(
            &mut case,
            "b",
            id,
            json!({
                "action":"apply-transfer-cancellation","transfer":"$routes","cancellation":"$cancel-proposal","person":"$beto",
                "expected_revision":3,"request_id":"cancel-apply"
            }),
        );
    }
    publish(&mut case, "b", "cancelled");
    add(
        &mut case,
        "b",
        "receive-four",
        Event::ApplyReceivedTransfer {
            transfer: "$routes".into(),
            occurrence: "$activate".into(),
            person: "$beto".into(),
            local_record: "$stock-b".into(),
        },
    );
    send(&mut case, "b", "a", "application-acknowledgement");
    publish(&mut case, "b", "cancelled-and-applied");
    add(&mut case, "b", "restart", Event::Restart {});
    for check in &mut case.checks {
        if let Predicate::Quantity { cell, expected, .. } = &mut check.predicate {
            expected.value =
                nucleus::DecimalValue::parse_inferred(if cell == "a" { "26" } else { "4" })
                    .unwrap();
        }
    }
    let input = case
        .inputs
        .iter_mut()
        .find(|input| input.id == "cancel-apply")
        .unwrap();
    let at = input.at_ms;
    if let Event::TransferCommand {
        copies,
        duplicate_spacing_ms,
        ..
    } = &mut input.event
    {
        *copies = 2;
        *duplicate_spacing_ms = 3;
    }
    for (suffix, offset, event) in [
        (
            "disconnect",
            2,
            Event::Link {
                peer: "a".into(),
                connected: false,
            },
        ),
        ("restart", 4, Event::Restart {}),
        (
            "reconnect",
            6,
            Event::Link {
                peer: "a".into(),
                connected: true,
            },
        ),
        (
            "retry",
            7,
            Event::TransferDelivery {
                peer: "a".into(),
                delay_ms: 1,
                copies: 2,
                duplicate_spacing_ms: 2,
                drop: false,
            },
        ),
    ] {
        case.inputs.push(Input {
            id: format!("cancel-apply-{suffix}"),
            cell: "b".into(),
            at_ms: at + offset,
            event,
        });
    }
    case.checks.push(Check {
        id: "cancel-retry-duplicate-refused".into(),
        options: Default::default(),
        predicate: Predicate::ExpectedMessageRefusal {
            input: "cancel-apply-retry".into(),
            copy: 1,
            refusal: nucleus::simulation::Refusal::InvalidAction {},
        },
    });
    case.inputs.sort_by_key(|input| input.at_ms);
    case
}
