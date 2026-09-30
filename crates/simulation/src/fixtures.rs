use crate::scenario::{Cell, Check, Invocation, Limits, Scenario};
use engine::actions::Action;
use nucleus::karma::{Cadence, CadenceStep, Consequence, DecimalValue};
use nucleus::simulation::{Predicate, Quantity, Version};

pub mod transfer;

pub fn daily() -> Scenario {
    let start_ms = 1_893_456_000_000;
    let day = 86_400_000;
    let start = chrono::DateTime::from_timestamp_millis(start_ms).unwrap();
    let invocation = |id: &str, action| Invocation {
        id: id.into(),
        actor: None,
        action,
    };
    Scenario {
        checking: Default::default(),
        version: Version::V1,
        name: "daily-negative".into(),
        seed: 42,
        start_ms,
        end_ms: start_ms + 4 * day,
        limits: Limits::default(),
        inputs: Vec::new(),
        cells: vec![Cell {
            name: "a".into(),
            database: None,
            lingua: Vec::new(),
            seed: vec![
                invocation(
                    "stock",
                    Action::CreateRecord {
                        slug: Some("stock".into()),
                        kind: nucleus::RecordKind::Plain,
                        head: "Stock".into(),
                        body: String::new(),
                        quantity: 10.0,
                    },
                ),
                invocation(
                    "daily",
                    Action::CreateFrequency {
                        slug: "daily".into(),
                        head: None,
                        every: CadenceStep {
                            days: 1,
                            ..Default::default()
                        },
                        anchor_at: Some((start + chrono::TimeDelta::days(1)).to_rfc3339()),
                        request_id: Some("daily".into()),
                    },
                ),
                invocation(
                    "rule",
                    Action::CreateRecurrence {
                        target: "stock".into(),
                        consequences: vec![Consequence::AddQuantity {
                            delta: Some(DecimalValue::parse_inferred("-3").unwrap()),
                        }],
                        condition: Some("freq(@daily)".into()),
                        gate: Some("!=0".into()),
                        carry: Some("value".into()),
                        note: None,
                        cadence: Cadence::every_days(1),
                        anchor_at: Some(start.to_rfc3339()),
                        request_id: Some("rule".into()),
                    },
                ),
            ],
        }],
        checks: vec![
            Check {
                options: Default::default(),
                id: "daily-result".into(),
                predicate: Predicate::QuantityEquals {
                    cell: "a".into(),
                    record: "stock".into(),
                    expected: Quantity {
                        value: DecimalValue::parse_inferred("-2").unwrap(),
                        unit: None,
                    },
                    at_ms: start_ms + 4 * day,
                },
            },
            Check {
                options: Default::default(),
                id: "fact-chain".into(),
                predicate: Predicate::FactChain {},
            },
            Check {
                options: Default::default(),
                id: "once".into(),
                predicate: Predicate::OncePerOccurrence {},
            },
            Check {
                options: Default::default(),
                id: "no-unexpected-refusals".into(),
                predicate: Predicate::NoUnexpectedRefusals {},
            },
        ],
    }
}

pub fn current_database(start_ms: i64) -> Scenario {
    Scenario {
        checking: Default::default(),
        version: Version::V1,
        name: "current-database".into(),
        seed: 42,
        start_ms,
        end_ms: start_ms + 30 * 86_400_000,
        limits: Limits::default(),
        cells: vec![Cell {
            name: "current".into(),
            database: None,
            lingua: Vec::new(),
            seed: Vec::new(),
        }],
        inputs: Vec::new(),
        checks: vec![
            Check {
                options: Default::default(),
                id: "fact-chain".into(),
                predicate: Predicate::FactChain {},
            },
            Check {
                options: Default::default(),
                id: "once-per-occurrence".into(),
                predicate: Predicate::OncePerOccurrence {},
            },
            Check {
                options: Default::default(),
                id: "no-unexpected-refusals".into(),
                predicate: Predicate::NoUnexpectedRefusals {},
            },
        ],
    }
}

pub fn network(two_organs: bool) -> Scenario {
    use crate::scenario::{Event, Input};
    let mut case = daily();
    case.name = if two_organs {
        "two-organs"
    } else {
        "four-organs"
    }
    .into();
    case.cells[0].seed.truncate(1);
    for name in ["b", "c", "d"] {
        case.cells.push(Cell {
            name: name.into(),
            database: None,
            lingua: Vec::new(),
            seed: Vec::new(),
        });
    }
    if two_organs {
        case.cells[2].seed.push(Invocation {
            id: "second-stock".into(),
            actor: None,
            action: Action::CreateRecord {
                slug: Some("second-stock".into()),
                kind: nucleus::RecordKind::Plain,
                head: "Second Organ stock".into(),
                body: String::new(),
                quantity: 5.0,
            },
        });
    }
    case.end_ms = case.start_ms + 1000;
    case.checks.retain(|check| {
        matches!(
            check.predicate,
            Predicate::FactChain {} | Predicate::NoUnexpectedRefusals {}
        )
    });
    case.checks.push(Check {
        options: Default::default(),
        id: "converged".into(),
        predicate: Predicate::Converged {
            cells: ["a", "b", "c", "d"].map(String::from).into(),
            record: "$stock".into(),
            at_ms: case.end_ms,
        },
    });
    if two_organs {
        case.checks.push(Check {
            options: Default::default(),
            id: "second-organ-converged".into(),
            predicate: Predicate::Converged {
                cells: vec!["c".into(), "d".into()],
                record: "$second-stock".into(),
                at_ms: case.end_ms,
            },
        });
    }
    let mut add = |cell: &str, at: i64, event| {
        case.inputs.push(Input {
            id: format!("network-{}", case.inputs.len()),
            cell: cell.into(),
            at_ms: case.start_ms + at,
            event,
        });
    };
    if two_organs {
        add("b", 0, Event::Enrol { peer: "a".into() });
        add("d", 0, Event::Enrol { peer: "c".into() });
    }
    add("a", 1, Event::Discover { peer: "c".into() });
    add("a", 2, Event::Pair { peer: "c".into() });
    if two_organs {
        add("a", 2, Event::Pair { peer: "d".into() });
    }
    if !two_organs {
        for peer in ["b", "d"] {
            add("a", 2, Event::Pair { peer: peer.into() });
        }
    }
    let sync = |peer: &str| Event::Sync {
        peer: peer.into(),
        delay_ms: 10,
        copies: 2,
        duplicate_spacing_ms: 30,
        drop: false,
    };
    add("a", 10, sync("b"));
    add("a", 10, sync("c"));
    add("a", 10, sync("d"));
    add(
        "a",
        15,
        Event::Link {
            peer: "d".into(),
            connected: false,
        },
    );
    add("b", 25, Event::Restart {});
    add(
        "a",
        50,
        Event::Link {
            peer: "d".into(),
            connected: true,
        },
    );
    if two_organs {
        add("c", 60, sync("d"));
    }
    add("a", 60, sync("d"));
    case
}
