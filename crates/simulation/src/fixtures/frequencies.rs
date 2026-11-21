// non-lean-yet
// Scope: const DAY, fn invocation, fn complete, fn shared.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 29, 163, 165,
//   167, 187, 195, 231, 260, 278.
// - [review: §5 Panics] Check expect invariants and diagnostic context. Sites expect (123, 217).
// - [Clippy probe: clippy::as_conversions] Use From or TryFrom when it preserves the conversion
//   contract. Sites 116, 138, 139, 149, 158, 203, 206, 208, 238, 250.
// - [Clippy probe: clippy::cast_possible_truncation] Prove the bounds or use a checked conversion.
//   Sites 138, 139.
// - [Clippy probe: clippy::cast_sign_loss] Prove the sign or preserve behavior with an explicit
//   conversion. Sites 138, 139.
// - [Clippy probe: clippy::expect_used] Check the stated invariant and handle reachable failures.
//   Sites 60, 70, 118, 121, 196, 210, 212, 215, 262, 279.
// - [Clippy probe: clippy::indexing_slicing] Verify bounds or use checked access on fallible
//   inputs. Sites 63, 124, 162, 172.
// - [Clippy probe: clippy::manual_is_multiple_of] manual implementation of `.is_multiple_of()`.
//   Sites 55.
// - [Clippy probe: clippy::missing_panics_doc] State the panic conditions for exported functions.
//   Sites 44.
// - [rustc probe: missing_docs] Document the externally visible contracts where applicable. Sites
//   9, 44.

use std::collections::BTreeMap;

use crate::scenario::{Check, Event, Input, Invocation, Scenario};
use chrono::{Months, TimeDelta};
use engine::actions::Action;
use nucleus::karma::{Cadence, CadenceStep, Slug, TimestampMs, rule_field::RuleFieldInput};
use nucleus::simulation::{Predicate, Quantity};

pub const DAY: i64 = 86_400_000;

fn invocation(id: String, action: Action) -> Invocation {
    Invocation {
        id,
        actor: None,
        action,
    }
}

fn complete(
    case: &mut Scenario,
    id: &str,
    at_ms: i64,
    changes: BTreeMap<String, nucleus::DecimalValue>,
) {
    let changes: Vec<_> = changes.into_iter().collect();
    for (index, chunk) in changes.chunks(32).enumerate() {
        let id = format!("{id}-{index}");
        case.inputs.push(Input {
            id: id.clone(),
            cell: "a".into(),
            at_ms,
            event: Event::Action {
                invocation: invocation(
                    id,
                    Action::AddQuantityGroupExact {
                        changes: chunk.iter().cloned().collect(),
                    },
                ),
            },
        });
    }
}

pub fn shared(seed: u64) -> Scenario {
    let mut entropy = seed.wrapping_add(0x9e3779b97f4a7c15);
    let mut next = || {
        entropy ^= entropy << 13;
        entropy ^= entropy >> 7;
        entropy ^= entropy << 17;
        entropy
    };
    let mut case = super::daily();
    case.name = format!("shared-frequencies-{seed}");
    case.seed = seed;
    case.start_ms = chrono::DateTime::parse_from_rfc3339(if seed % 2 == 0 {
        "2024-01-28T00:00:00Z"
    } else {
        "2025-12-28T00:00:00Z"
    })
    .expect("both frequency scenario dates are RFC 3339 literals")
    .timestamp_millis();
    case.end_ms = case.start_ms + 100 * DAY - 1;
    case.cells[0].seed.clear();
    case.checks.clear();
    case.inputs.clear();
    case.limits.steps = 50_000;
    case.limits.rule_evaluations = 500_000;
    case.limits.evidence_bytes = 512 * 1024 * 1024;
    let start = chrono::DateTime::from_timestamp_millis(case.start_ms)
        .expect("the seed selects one of the two fixed start dates");
    let rewind_day = 30 + next() % 30;
    let mut tasks = Vec::new();
    for (family, slug, step, count) in [
        (
            0,
            "daily",
            CadenceStep {
                days: 1,
                ..Default::default()
            },
            108,
        ),
        (
            1,
            "weekly",
            CadenceStep {
                days: 7,
                ..Default::default()
            },
            8,
        ),
        (
            2,
            "monthly",
            CadenceStep {
                months: 1,
                ..Default::default()
            },
            8,
        ),
        (
            3,
            "yearly",
            CadenceStep {
                years: 1,
                ..Default::default()
            },
            8,
        ),
    ] {
        let offset = if family == 0 {
            0
        } else {
            next() % if family == 3 { 20 } else { 5 }
        };
        let anchor = start + TimeDelta::days(offset as i64) + TimeDelta::minutes(1);
        let frequency = nucleus::karma::simple_frequency::frequency_from_cadence(
            Slug::new(slug).expect("the frequency families use valid literal slugs"),
            slug.into(),
            &Cadence::every(step),
            TimestampMs::from_millis(anchor.timestamp_millis()).expect("the calendar anchor stays near the fixed start date"),
        )
        .expect("the fixture uses a supported cadence for each frequency family");
        case.cells[0].seed.push(invocation(
            slug.into(),
            Action::SaveKarmaFrequency {
                request_id: slug.into(),
                frequency_uid: None,
                expected_handle_revision: None,
                frequency,
                restart: false,
            },
        ));
        let dates: Vec<i64> = (0..=100)
            .filter_map(|index| match family {
                0 => Some(anchor + TimeDelta::days(index)),
                1 => Some(anchor + TimeDelta::days(index * 7)),
                2 => anchor.checked_add_months(Months::new(index as u32)),
                _ => anchor.checked_add_months(Months::new(index as u32 * 12)),
            })
            .map(|date| date.timestamp_millis())
            .take_while(|date| *date <= case.end_ms)
            .collect();
        for index in 0..count {
            let target = format!("{slug}-task-{index}");
            let amount = if family == 0 && index < 100 {
                -1.0
            } else {
                let magnitude = (1 + next() % 31) as f64 / 4.0;
                if next() % 2 == 0 {
                    magnitude
                } else {
                    -magnitude
                }
            };
            let additive = family != 0 && index % 3 == 0;
            let initial = if additive {
                (10 + next() % 100) as f64
            } else {
                0.0
            };
            case.cells[0].seed.push(invocation(
                target.clone(),
                Action::CreateRecord {
                    slug: Some(target.clone()),
                    kind: nucleus::RecordKind::Plain,
                    head: target.clone(),
                    body: String::new(),
                    quantity: initial,
                },
            ));
            case.cells[0].seed.push(invocation(
                format!("rule-{target}"),
                Action::SaveKarmaRule {
                    identity: None,
                    rule: None,
                    expected_revision: None,
                    fields: [
                        format!("{amount} * freq(@{slug})"),
                        "!=0".into(),
                        format!("@{target}{}", if additive { " += result" } else { "" }),
                    ]
                    .map(|source| RuleFieldInput::Text { source }),
                    request_id: format!("rule-{target}"),
                },
            ));
            tasks.push((target, amount, initial, additive, dates.clone()));
        }
    }
    let changes = tasks
        .iter()
        .filter(|(target, ..)| target.starts_with("daily-"))
        .map(|(target, amount, ..)| {
            (
                target.clone(),
                nucleus::DecimalValue::parse_inferred(&(-amount).to_string()).expect("the generated work amount is bounded"),
            )
        })
        .collect();
    complete(
        &mut case,
        "complete-before-rewind",
        start.timestamp_millis() + rewind_day as i64 * DAY + DAY / 2 - 1,
        changes,
    );
    let rewind_at = case.start_ms + rewind_day as i64 * DAY + DAY / 2;
    let anchor = chrono::DateTime::from_timestamp_millis(
        case.start_ms + (rewind_day as i64 - 1) * DAY + 60_000,
    )
    .expect("the rewind stays within 60 days of the fixed start date");
    let frequency = nucleus::karma::simple_frequency::frequency_from_cadence(
        Slug::new("daily").expect("daily is a valid literal slug"),
        "daily".into(),
        &Cadence::every_days(1),
        TimestampMs::from_millis(anchor.timestamp_millis()).expect("the rewind anchor stays near the fixed start date"),
    )
    .expect("a daily cadence is supported");
    let action = Action::SaveKarmaFrequency {
        request_id: "rewind-daily".into(),
        frequency_uid: Some("$daily".into()),
        expected_handle_revision: Some(2),
        frequency,
        restart: true,
    };
    for (id, at_ms) in [("rewind-daily", rewind_at), ("retry-rewind", rewind_at + 1)] {
        case.inputs.push(Input {
            id: id.into(),
            at_ms,
            cell: "a".into(),
            event: Event::Action {
                invocation: invocation(id.into(), action.clone()),
            },
        });
    }
    for (index, day) in [12 + next() % 10, 65 + next() % 10].into_iter().enumerate() {
        case.inputs.push(Input {
            id: format!("restart-{index}"),
            at_ms: case.start_ms + day as i64 * DAY + 7 * 3_600_000,
            cell: "a".into(),
            event: Event::Restart {},
        });
    }
    for day in 1..=100 {
        let at_ms = case.start_ms + day * DAY - 1;
        let records: BTreeMap<_, _> = tasks
            .iter()
            .map(|(target, amount, initial, additive, dates)| {
                let occurrences = dates.iter().filter(|date| **date <= at_ms).count();
                let value = if *additive {
                    initial + amount * occurrences as f64
                } else if dates
                    .iter()
                    .any(|date| *date >= at_ms - DAY + 1 && *date <= at_ms)
                {
                    *amount
                } else {
                    0.0
                };
                (
                    target.clone(),
                    Quantity {
                        value: nucleus::DecimalValue::parse_inferred(&value.to_string()).expect("the 100-day fixture bounds each generated quantity"),
                        unit: None,
                    },
                )
            })
            .collect();
        let changes = tasks
            .iter()
            .filter(|(_, _, _, additive, dates)| {
                !additive
                    && dates
                        .iter()
                        .any(|date| *date >= at_ms - DAY + 1 && *date <= at_ms)
            })
            .map(|(target, amount, ..)| {
                (
                    target.clone(),
                    nucleus::DecimalValue::parse_inferred(&(-amount).to_string()).expect("the generated work amount is bounded"),
                )
            })
            .collect();
        if day < 100 {
            complete(
                &mut case,
                &format!("complete-day-{day}"),
                at_ms + 1,
                changes,
            );
        }
        case.checks.push(Check {
            id: format!("day-{day}"),
            options: Default::default(),
            predicate: Predicate::QuantitiesEqual {
                cell: "a".into(),
                records,
                at_ms,
            },
        });
    }
    for (id, predicate) in [
        ("fact-chain", Predicate::FactChain {}),
        ("once-per-occurrence", Predicate::OncePerOccurrence {}),
        ("refusals", Predicate::NoUnexpectedRefusals {}),
        (
            "cycles",
            Predicate::NoRuleCycles {
                include_timed_recurrence: false,
            },
        ),
    ] {
        case.checks.push(Check {
            id: id.into(),
            options: nucleus::simulation::checks::CheckOptions {
                evaluation: nucleus::simulation::checks::Evaluation::End,
                ..Default::default()
            },
            predicate,
        });
    }
    case
}
