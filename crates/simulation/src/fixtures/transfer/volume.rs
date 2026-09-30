use std::collections::BTreeMap;

use nucleus::simulation::{
    CheckDefinition, CheckOptions, Comparison, Evaluation, Predicate, Quantity,
};
use serde_json::Value;

use crate::scenario::{Cell, Event, Input, Scenario};

fn rewrite(
    value: &mut Value,
    names: &BTreeMap<String, String>,
    ids: &BTreeMap<String, String>,
    prefix: &str,
    key: &str,
) {
    match value {
        Value::String(text) => {
            if let Some(id) = text.strip_prefix('$') {
                if let Some(replacement) = names.get(id).or_else(|| ids.get(id)) {
                    *text = format!("${replacement}");
                }
            } else if matches!(key, "id" | "request_id") {
                if let Some(replacement) = ids.get(text).or_else(|| names.get(text)) {
                    *text = replacement.clone();
                }
            } else if matches!(key, "cell" | "peer" | "slug") {
                if let Some(replacement) = names.get(text) {
                    *text = replacement.clone();
                } else if key == "slug" {
                    *text = format!("{prefix}-{text}");
                }
            } else if text == "apples" && matches!(key, "uid" | "promise" | "exchange") {
                *text = format!("{prefix}-apples");
            }
        }
        Value::Array(values) => {
            for value in values {
                rewrite(value, names, ids, prefix, key);
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                rewrite(value, names, ids, prefix, key);
            }
        }
        _ => {}
    }
}

pub fn volume(cells: usize, transfers: usize) -> crate::Result<Scenario> {
    if !(4..=32).contains(&cells)
        || !cells.is_multiple_of(4)
        || transfers < cells / 4
        || transfers > 1000
    {
        return Err(
            "Transfer workloads need 4–32 Cells in groups of four, at least one Transfer per group and at most 1,000 Transfers".into(),
        );
    }
    let pairs = cells / 4;
    let mut case = super::independent_donation(false);
    case.name = format!("transfers-{cells}-cells-{transfers}-transfers");
    case.cells.clear();
    case.inputs.clear();
    case.checks.retain(|check| {
        matches!(
            check.predicate,
            Predicate::FactChain {} | Predicate::NoUnexpectedRefusals {}
        )
    });
    for check in &mut case.checks {
        check.options.evaluation = Evaluation::End;
    }
    case.limits.steps = 1_000_000;
    case.limits.evidence_bytes = 1024 * 1024 * 1024;
    case.limits.pending_messages = 100_000;
    let waves = transfers.div_ceil(pairs);
    let recovery_start = case.start_ms + (waves as i64 + 1) * 2000;
    let pages = waves / 2 + 12;
    case.end_ms = recovery_start + pages as i64 * 30 + 100;
    for pair in 0..pairs {
        let count = transfers / pairs + usize::from(pair < transfers % pairs);
        let base = super::independent_donation(pair.is_multiple_of(2));
        let names: BTreeMap<_, _> = [
            "a", "b", "ana", "beto", "stock-a", "stock-b", "public-a", "public-b",
        ]
        .into_iter()
        .map(|name| (name.to_owned(), format!("p{pair}-{name}")))
        .chain(["a", "b"].into_iter().map(|name| {
            (
                format!("cell:{name}:organ"),
                format!("cell:p{pair}-{name}:organ"),
            )
        }))
        .collect();
        for source in &base.cells {
            let mut cell = source.clone();
            cell.name = names[&source.name].clone();
            for seed in &mut cell.seed {
                let mut value = serde_json::to_value(&*seed)?;
                rewrite(
                    &mut value,
                    &names,
                    &BTreeMap::new(),
                    &format!("p{pair}"),
                    "",
                );
                *seed = serde_json::from_value(value)?;
                if let engine::actions::Action::CreateRecord { quantity, .. } = &mut seed.action {
                    if seed.id == names["stock-a"] {
                        *quantity = (count * 10 + 20) as f64;
                    }
                }
            }
            case.cells.push(cell);
            let replica = format!("p{pair}-{}-replica", source.name);
            case.cells.push(Cell {
                name: replica.clone(),
                database: None,
                lingua: Vec::new(),
                seed: Vec::new(),
            });
            case.inputs.push(Input {
                id: format!("enrol-{replica}"),
                cell: replica,
                at_ms: case.start_ms,
                event: Event::Enrol {
                    peer: names[&source.name].clone(),
                },
            });
        }
        let start = base
            .inputs
            .iter()
            .position(|input| input.id == "routes")
            .ok_or("donation draft unavailable")?;
        for wave in 0..count {
            let prefix = format!("p{pair}-t{wave}");
            let ids = base
                .inputs
                .iter()
                .map(|input| (input.id.clone(), format!("{prefix}-{}", input.id)))
                .collect();
            for (index, input) in base.inputs.iter().enumerate() {
                if wave > 0 && index < start && !matches!(input.event, Event::PersonKey { .. }) {
                    continue;
                }
                let mut value = serde_json::to_value(input)?;
                rewrite(&mut value, &names, &ids, &prefix, "");
                let mut input: Input = serde_json::from_value(value)?;
                input.at_ms += wave as i64 * 2000 + 20;
                case.inputs.push(input);
            }
        }
        for side in ["a", "b"] {
            let source = names[side].clone();
            let replica = format!("{source}-replica");
            for (suffix, offset, event) in [
                ("offline", 0, Event::Online { online: false }),
                ("clock", 1, Event::Clock { offset_ms: 150 }),
                ("online", 10, Event::Online { online: true }),
                ("clock-restored", 11, Event::Clock { offset_ms: 0 }),
            ] {
                case.inputs.push(Input {
                    id: format!("{replica}-{suffix}"),
                    cell: replica.clone(),
                    at_ms: recovery_start + offset,
                    event,
                });
            }
            case.inputs.push(Input {
                id: format!("{source}-dropped"),
                cell: source.clone(),
                at_ms: recovery_start + 5,
                event: Event::Sync {
                    peer: replica.clone(),
                    delay_ms: 1,
                    copies: 1,
                    duplicate_spacing_ms: 0,
                    drop: true,
                },
            });
            for page in 0..pages {
                case.inputs.push(Input {
                    id: format!("{source}-recovery-{page}"),
                    cell: source.clone(),
                    at_ms: recovery_start + 20 + page as i64 * 30,
                    event: Event::Sync {
                        peer: replica.clone(),
                        delay_ms: 1,
                        copies: 2,
                        duplicate_spacing_ms: 1,
                        drop: false,
                    },
                });
            }
            case.inputs.push(Input {
                id: format!("{replica}-restart"),
                cell: replica.clone(),
                at_ms: case.end_ms - 10,
                event: Event::Restart {},
            });
            for cell in [&source, &replica] {
                case.checks.push(CheckDefinition {
                    id: format!("balance-{cell}"),
                    predicate: Predicate::Quantity {
                        cell: cell.clone(),
                        record: format!("${}", names[&format!("stock-{side}")]),
                        comparison: Comparison::Equal,
                        expected: Quantity {
                            value: store::exact::integer(if side == "a" {
                                20
                            } else {
                                (count * 10) as i128
                            }),
                            unit: None,
                        },
                    },
                    options: CheckOptions {
                        evaluation: Evaluation::End,
                        ..Default::default()
                    },
                });
            }
            case.checks.push(CheckDefinition {
                id: format!("nonnegative-{source}"),
                predicate: Predicate::Nonnegative {
                    cell: source,
                    record: format!("${}", names[&format!("stock-{side}")]),
                },
                options: CheckOptions {
                    evaluation: Evaluation::EveryChange,
                    ..Default::default()
                },
            });
        }
    }
    case.inputs.sort_by_key(|input| input.at_ms);
    case.validate()?;
    Ok(case)
}
