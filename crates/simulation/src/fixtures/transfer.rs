use engine::actions::{
    Action, TransferOccurrenceClaimRole, TransferPromiseInput, TransferReservePoint,
    TransferSatiation, TransferVisibility,
};
use nucleus::simulation::{Predicate, Quantity};
use nucleus::transfer::{AgreementType, OpenPromiseReusePolicy};

use crate::scenario::{Cell, Check, Event, Input, Invocation, Scenario};

mod reservations;
pub use reservations::competing_reservations;
mod recovery;
pub use recovery::{declined_invitation, grouped_correction_after_restart, private_correction_after_restart};
mod volume;
pub use volume::volume;
mod trade;
pub use trade::{trade, counteroffer, open_offer, private_trade};
mod parents;
mod needs;
mod loans;
pub use loans::{extended_loan, temporary_loan};
pub use needs::grouped_needs;
pub use parents::nested_parents;
mod dependencies;
pub use dependencies::observer_outcomes;
mod routes;
pub use routes::three_parties;
mod donation;
pub use donation::{donation_with_lost_acknowledgements, donation_without_private_source, independent_donation, partial_cancellation};

pub fn visibility() -> Scenario {
    let mut case = sale();
    case.name = "transfer-item-visibility".into();
    for input in &mut case.inputs {
        if let Event::Action { invocation } = &mut input.event
            && let Action::CreateTransferDraft { promises, .. } = &mut invocation.action
        {
            for promise in promises {
                promise.item = Some(nucleus::transfer::disclosure::TransferItem {
                    title: "City bike".into(),
                    description: "Blue frame".into(),
                    ..Default::default()
                });
            }
        }
    }
    case
}

pub fn sale() -> Scenario {
    let mut case = super::daily();
    case.name = "transfer-sale".into();
    case.end_ms = case.start_ms + 1000;
    case.cells[0].seed.clear();
    case.cells.push(Cell {
        name: "b".into(),
        database: None,
        lingua: Vec::new(),
        seed: Vec::new(),
    });
    for (id, kind, quantity) in [
        ("ana", nucleus::RecordKind::Person, 1.0),
        ("carlos", nucleus::RecordKind::Person, 1.0),
        ("bike", nucleus::RecordKind::Plain, 1.0),
    ] {
        case.cells[0].seed.push(Invocation {
            id: id.into(),
            actor: None,
            action: Action::CreateRecord {
                slug: Some(id.into()),
                kind,
                head: id.into(),
                body: String::new(),
                quantity,
            },
        });
    }
    case.checks = vec![
        Check {
            options: Default::default(),
            id: "no-unexpected-refusals".into(),
            predicate: Predicate::NoUnexpectedRefusals {},
        },
        Check {
            options: Default::default(),
            id: "fact-chain".into(),
            predicate: Predicate::FactChain {},
        },
        Check {
            options: Default::default(),
            id: "duplicate-request-refused".into(),
            predicate: Predicate::ExpectedMessageRefusal {
                input: "send".into(),
                copy: 1,
                refusal: nucleus::simulation::Refusal::InvalidAction {},
            },
        },
        Check {
            options: Default::default(),
            id: "bike-settled-once".into(),
            predicate: Predicate::QuantityEquals {
                cell: "a".into(),
                record: "$bike".into(),
                expected: Quantity {
                    value: store::exact::zero(),
                    unit: None,
                },
                at_ms: case.end_ms,
            },
        },
    ];
    let mut add = |id: &str, cell: &str, event| {
        case.inputs.push(Input {
            id: id.into(),
            cell: cell.into(),
            at_ms: case.start_ms + case.inputs.len() as i64 * 10,
            event,
        })
    };
    let action = |id: &str, action| Event::Action {
        invocation: Invocation {
            id: id.into(),
            actor: None,
            action,
        },
    };
    add("pair", "a", Event::Pair { peer: "b".into() });
    for (cell, peer) in [("a", "b"), ("b", "a")] {
        let id = format!("sync-{cell}");
        add(
            &id,
            cell,
            action(
                &id,
                Action::SetSyncPolicy {
                    target: format!("$cell:{peer}:organ"),
                    sync_out: true,
                    sync_in: true,
                },
            ),
        );
    }
    add(
        "ana-key",
        "a",
        Event::PersonKey {
            person: "$ana".into(),
        },
    );
    let promise = |uid: &str, person: &str, delta| TransferPromiseInput {
        item: None,
        uid: Some(uid.into()),
        record: "$bike".into(),
        party: Some(person.into()),
        open: false,
        delta,
        unit: None,
        window_start: None,
        window_end: None,
        place: None,
        condition: None,
        reserve_from: Some(TransferReservePoint::Inherit),
        reuse_policy: OpenPromiseReusePolicy::Duplicate,
        withdrawn: false,
    };
    add(
        "sale",
        "a",
        action(
            "sale",
            Action::CreateTransferDraft {
                request_id: "sale".into(),
                creator: Some("$ana".into()),
                slug: Some("sale".into()),
                head: "Bike sale".into(),
                agreement: AgreementType::Full,
                agreement_pct: None,
                satiation: TransferSatiation::None,
                parent: None,
                source: None,
                visibility: TransferVisibility::Hidden,
                max_proximity: None,
                reserve_default: TransferReservePoint::Active,
                require_confirmation: true,
                default_place: None,
                invitees: vec!["$carlos".into()],
                promises: vec![
                    promise("bike-give", "$ana", -1.0),
                    promise("bike-receive", "$carlos", 1.0),
                ],
                dependencies: Vec::new(),
            },
        ),
    );
    add(
        "carlos-key",
        "a",
        Event::PersonKey {
            person: "$carlos".into(),
        },
    );
    add(
        "accept",
        "a",
        Event::AcceptInvitation {
            transfer: "$sale".into(),
            person: "$carlos".into(),
            peer: None,
        },
    );
    for person in ["ana", "carlos"] {
        add(
            &format!("key-{person}"),
            "a",
            Event::PersonKey {
                person: format!("${person}"),
            },
        );
        for level in [1, 2] {
            let id = format!("agree-{person}-{level}");
            add(
                &id,
                "a",
                action(
                    &id,
                    Action::SetTransferAgreementLevel {
                        transfer: "$sale".into(),
                        expected_revision: 2,
                        request_id: id.clone(),
                        person: Some(format!("${person}")),
                        level,
                    },
                ),
            );
        }
    }
    add(
        "key-activate",
        "a",
        Event::PersonKey {
            person: "$ana".into(),
        },
    );
    add(
        "occurrence",
        "a",
        action(
            "occurrence",
            Action::ActivateTransferOccurrence {
                transfer: "$sale".into(),
                promise: "bike-give".into(),
                expected_revision: 2,
                request_id: "occurrence".into(),
                person: Some("$ana".into()),
            },
        ),
    );
    add(
        "delivery",
        "a",
        action(
            "delivery",
            Action::SetTransferOccurrenceClaim {
                occurrence: "$occurrence".into(),
                request_id: "delivery".into(),
                person: Some("$ana".into()),
                role: TransferOccurrenceClaimRole::Delivery,
                claimed: true,
            },
        ),
    );
    add(
        "key-receipt",
        "a",
        Event::PersonKey {
            person: "$carlos".into(),
        },
    );
    add(
        "receipt",
        "a",
        action(
            "receipt",
            Action::SetTransferOccurrenceClaim {
                occurrence: "$occurrence".into(),
                request_id: "receipt".into(),
                person: Some("$carlos".into()),
                role: TransferOccurrenceClaimRole::Receipt,
                claimed: true,
            },
        ),
    );
    add(
        "key-settle",
        "a",
        Event::PersonKey {
            person: "$ana".into(),
        },
    );
    add(
        "settle",
        "a",
        Event::SettleReviewed {
            occurrence: "$occurrence".into(),
            person: "$ana".into(),
            quantity: nucleus::DecimalValue::parse_inferred("1").unwrap(),
        },
    );
    add(
        "policy",
        "a",
        action(
            "policy",
            Action::ConfigureTransferDelivery {
                transfer: "$sale".into(),
                recipient_person: "$carlos".into(),
                recipient_organ: "$cell:b:organ".into(),
                person: Some("$ana".into()),
                request_id: "policy".into(),
                mode: nucleus::transfer_delivery::TransferDeliveryMode::Replicated,
            },
        ),
    );
    add(
        "enqueue",
        "a",
        action(
            "enqueue",
            Action::EnqueueTransferDelivery {
                transfer: "$sale".into(),
                delivery: "$policy".into(),
                person: Some("$ana".into()),
                request_id: "enqueue".into(),
            },
        ),
    );
    add(
        "send",
        "a",
        Event::TransferDelivery {
            peer: "b".into(),
            delay_ms: 10,
            copies: 2,
            duplicate_spacing_ms: 30,
            drop: false,
        },
    );
    add("restart", "b", Event::Restart {});
    case
}
