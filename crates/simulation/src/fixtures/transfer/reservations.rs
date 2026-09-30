use engine::actions::{Action, TransferReservePoint};
use nucleus::simulation::{CheckDefinition, Predicate, Refusal};

use crate::scenario::{Event, Input, Invocation, Scenario};

pub fn competing_reservations(replicated: bool) -> Scenario {
    let mut case = super::independent_donation(replicated);
    case.name = format!("competing-reservations-{replicated}");
    let draft = case
        .inputs
        .iter_mut()
        .find(|input| input.id == "routes")
        .unwrap();
    let Event::Action { invocation } = &mut draft.event else {
        unreachable!()
    };
    let Action::CreateTransferDraft {
        reserve_default, ..
    } = &mut invocation.action
    else {
        unreachable!()
    };
    *reserve_default = TransferReservePoint::Proposed;
    let at = draft.at_ms;
    let mut competing = invocation.action.clone();
    let Action::CreateTransferDraft {
        request_id,
        slug,
        promises,
        ..
    } = &mut competing
    else {
        unreachable!()
    };
    *request_id = "competing-reservation".into();
    *slug = Some("competing-reservation".into());
    for promise in promises {
        promise.uid = Some("competing-apples".into());
        promise
            .item
            .as_mut()
            .unwrap()
            .exchange
            .as_mut()
            .unwrap()
            .uid = "competing-apples".into();
    }
    case.inputs.push(Input {
        id: "limit".into(),
        cell: "a".into(),
        at_ms: at - 1,
        event: Event::Action {
            invocation: Invocation {
                id: "limit".into(),
                actor: None,
                action: Action::SetRecordStockLimit {
                    record: "$stock-a".into(),
                    person: "$ana".into(),
                    minimum: Some(store::exact::integer(20)),
                    expected_version: 0,
                    request_id: "limit".into(),
                },
            },
        },
    });
    for (id, at_ms) in [
        ("competing-reservation", at + 1),
        ("competing-after-delivery", case.end_ms - 10),
    ] {
        let mut action = competing.clone();
        if let Action::CreateTransferDraft { request_id, .. } = &mut action {
            *request_id = id.into();
        }
        case.inputs.push(Input {
            id: id.into(),
            cell: "a".into(),
            at_ms,
            event: Event::Action {
                invocation: Invocation {
                    id: id.into(),
                    actor: None,
                    action,
                },
            },
        });
        case.checks.push(CheckDefinition {
            id: format!("{id}-refused"),
            predicate: Predicate::ExpectedRefusal {
                input: id.into(),
                refusal: Refusal::Conflict {
                    code: "hard_stock_limit".into(),
                },
            },
            options: Default::default(),
        });
    }
    case.inputs.sort_by_key(|input| input.at_ms);
    case
}
