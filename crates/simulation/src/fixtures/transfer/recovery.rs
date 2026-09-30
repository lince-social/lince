use engine::actions::Action;
use nucleus::simulation::{Comparison, Evaluation, Predicate, Quantity};
use serde_json::json;

use crate::scenario::{Check, Event, Input, Invocation, Scenario};

fn append(case: &mut Scenario, cell: &str, id: &str, event: Event) {
    let at_ms = case.inputs.iter().map(|input| input.at_ms).max().unwrap() + 20;
    case.inputs.push(Input {
        id: id.into(),
        cell: cell.into(),
        at_ms,
        event,
    });
    case.end_ms = case.end_ms.max(at_ms + 1_000);
}

fn action(case: &mut Scenario, id: &str, action: Action) {
    append(
        case,
        "b",
        id,
        Event::Action {
            invocation: Invocation {
                id: id.into(),
                actor: None,
                action,
            },
        },
    );
}

fn quantity(case: &mut Scenario, cell: &str, record: &str, amount: &str) {
    case.checks.push(Check {
        id: format!("recovery-{record}"),
        options: nucleus::simulation::CheckOptions {
            evaluation: Evaluation::End,
            ..Default::default()
        },
        predicate: Predicate::Quantity {
            cell: cell.into(),
            record: format!("${record}"),
            comparison: Comparison::Equal,
            expected: Quantity {
                value: nucleus::DecimalValue::parse_inferred(amount).unwrap(),
                unit: None,
            },
        },
    });
}

fn restore_person(case: &mut Scenario, id: &str) {
    append(
        case,
        "b",
        id,
        Event::PersonKey {
            person: "$beto".into(),
        },
    );
}

pub fn declined_invitation(replicated: bool) -> Scenario {
    let mut case = super::independent_donation(replicated);
    case.name = format!("declined-invitation-{replicated}");
    let accept = case
        .inputs
        .iter()
        .position(|input| input.id == "accept")
        .unwrap();
    case.inputs.truncate(accept);
    append(
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
    quantity(&mut case, "a", "stock-a", "30");
    quantity(&mut case, "b", "stock-b", "0");
    case
}

pub fn private_correction_after_restart(replicated: bool) -> Scenario {
    let mut case = super::private_trade(replicated);
    case.name = format!("private-correction-restart-{replicated}");
    restore_person(&mut case, "restore-policy-person");
    action(&mut case, "change-policy", serde_json::from_value(json!({
        "action":"set-transfer-private-application-policy","transfer":"$routes","exchange":"payment",
        "effects":[{"record":"$money-b","formula":"-incoming() * 100","mode":"quantity"}],
        "person":"$beto","expected_version":1,"request_id":"change-policy"
    })).unwrap());
    let correction = Action::CompensateTransferApplication {
        application: "$pay-last".into(),
        person: "$beto".into(),
        request_id: "correct-last-payment".into(),
    };
    action(&mut case, "correct-last-payment", correction.clone());
    append(
        &mut case,
        "b",
        "restart-after-correction",
        Event::Restart {},
    );
    restore_person(&mut case, "restore-correction-person");
    action(&mut case, "retry-last-correction", correction);
    case.checks.retain(|check| check.id != "final-money-b");
    quantity(&mut case, "b", "money-b", "98");
    case
}

pub fn grouped_correction_after_restart(replicated: bool) -> Scenario {
    let mut case = super::grouped_needs(replicated, false, false);
    case.checks.retain(|check| !check.id.starts_with("need-"));
    case.name = format!("grouped-correction-restart-{replicated}");
    restore_person(&mut case, "restore-group-person");
    action(
        &mut case,
        "unrelated-transport-change",
        Action::AddQuantityExact {
            target: "$transport".into(),
            delta: store::exact::integer(2),
        },
    );
    let correction = Action::CompensateTransferApplication {
        application: "$receive-apples".into(),
        person: "$beto".into(),
        request_id: "correct-group".into(),
    };
    action(&mut case, "correct-group", correction.clone());
    append(
        &mut case,
        "b",
        "restart-after-correction",
        Event::Restart {},
    );
    restore_person(&mut case, "restore-correction-person");
    action(&mut case, "retry-group-correction", correction);
    quantity(&mut case, "b", "stock-b", "-0.4");
    quantity(&mut case, "b", "transport", "1.6");
    case
}
