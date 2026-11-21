// non-lean-yet
// Scope: fn temporary_loan, fn extended_loan.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 25.
// - [review: §4 API] Check public bool parameters for a named mode or distinct operations. Sites
//   extended_loan:replicated (81), temporary_loan:assumed (5), temporary_loan:replicated (5).
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   109.
// - [Clippy probe: clippy::as_conversions] Use From or TryFrom when it preserves the conversion
//   contract. Sites 110.
// - [Clippy probe: clippy::expect_used] Check the stated invariant and handle reachable failures.
//   Sites 13, 16.
// - [Clippy probe: clippy::indexing_slicing] Verify bounds or use checked access on fallible
//   inputs. Sites 109.
// - [Clippy probe: clippy::missing_panics_doc] State the panic conditions for exported functions.
//   Sites 5, 81.
// - [rustc probe: missing_docs] Document the externally visible contracts where applicable. Sites
//   5, 81.

use crate::scenario::{Event, Scenario};
use engine::actions::Action;
use nucleus::transfer::loans::Interval;

pub fn temporary_loan(replicated: bool, assumed: bool) -> Scenario {
    let mut case = super::grouped_needs(replicated, assumed, false);
    case.checks.retain(|check| !check.id.starts_with("need-"));
    case.name = format!("temporary-loan-{replicated}-{assumed}");
    let from = case.start_ms + 3_000;
    let end = from + 3 * 86_400_000;
    let interval = Interval {
        from: chrono::DateTime::from_timestamp_millis(from)
            .expect("the loan starts near the fixed scenario date")
            .to_rfc3339(),
        until: chrono::DateTime::from_timestamp_millis(end)
            .expect("the loan ends near the fixed scenario date")
            .to_rfc3339(),
    };
    for input in &mut case.inputs {
        if let Event::Action { invocation } = &mut input.event
            && let Action::CreateTransferDraft { promises, .. } = &mut invocation.action
        {
            for promise in promises {
                if let Some(item) = &mut promise.item {
                    item.loan = Some(interval.clone());
                }
            }
        }
    }
    case.end_ms = end + 1;
    for record in ["stock-b", "transport"] {
        case.checks.push(nucleus::simulation::CheckDefinition {
            id: format!("{record}-stored"),
            predicate: nucleus::simulation::Predicate::Quantity {
                cell: "b".into(),
                record: record.into(),
                comparison: nucleus::simulation::Comparison::Equal,
                expected: nucleus::simulation::Quantity {
                    value: store::exact::zero(),
                    unit: None,
                },
            },
            options: Default::default(),
        });
        case.checks.push(nucleus::simulation::CheckDefinition {
            id: format!("{record}-during-loan"),
            predicate: nucleus::simulation::Predicate::Nonnegative {
                cell: "b".into(),
                record: record.into(),
            },
            options: nucleus::simulation::CheckOptions {
                quantity: nucleus::simulation::QuantityBasis::Available,
                evaluation: nucleus::simulation::Evaluation::EveryChange,
                window: nucleus::simulation::checks::CheckWindow {
                    from_ms: Some(from),
                    until_ms: Some(end - 1),
                },
                ..Default::default()
            },
        });
        case.checks.push(nucleus::simulation::CheckDefinition {
            id: format!("{record}-after-loan"),
            predicate: nucleus::simulation::Predicate::Quantity {
                cell: "b".into(),
                record: record.into(),
                comparison: nucleus::simulation::Comparison::Equal,
                expected: nucleus::simulation::Quantity {
                    value: store::exact::integer(-1),
                    unit: None,
                },
            },
            options: nucleus::simulation::CheckOptions {
                quantity: nucleus::simulation::QuantityBasis::Available,
                ..Default::default()
            },
        });
    }
    case
}

pub fn extended_loan(replicated: bool) -> Scenario {
    use super::routes::{decision, publish};
    use serde_json::json;
    let mut case = temporary_loan(replicated, false);
    case.name = format!("extended-loan-{replicated}");
    let original_end = case.end_ms - 1;
    let until = original_end + 86_400_000;
    case.end_ms = until + 1;
    let first = case.inputs.len();
    decision(
        &mut case,
        "b",
        "propose-extension",
        json!({"action":"propose-transfer-loan-extension","transfer":"$routes","exchange":"apples","expected_revision":2,"request_id":"propose-extension","person":"$beto","until":chrono::DateTime::from_timestamp_millis(until).expect("the extension ends near the fixed scenario date").to_rfc3339()}),
    );
    publish(&mut case, "b", "pending-extension");
    for (cell, person) in [("a", "ana"), ("b", "beto")] {
        for level in [1, 2] {
            let id = format!("extension-agree-{cell}-{level}");
            decision(
                &mut case,
                cell,
                &id,
                json!({"action":"set-transfer-agreement-level","transfer":"$routes","expected_revision":3,"request_id":id,"person":format!("${person}"),"level":level}),
            );
        }
        publish(&mut case, "b", &format!("extension-agreed-{cell}"));
    }
    for (index, input) in case.inputs[first..].iter_mut().enumerate() {
        input.at_ms = case.start_ms + 5_000 + index as i64 * 20;
    }
    for record in ["stock-b", "transport"] {
        case.checks.push(nucleus::simulation::CheckDefinition {
            id: format!("{record}-extended-deadline"),
            predicate: nucleus::simulation::Predicate::QuantityEquals {
                cell: "b".into(),
                record: record.into(),
                at_ms: original_end,
                expected: nucleus::simulation::Quantity {
                    value: store::exact::zero(),
                    unit: None,
                },
            },
            options: nucleus::simulation::CheckOptions {
                quantity: nucleus::simulation::QuantityBasis::Available,
                ..Default::default()
            },
        });
    }
    case
}
