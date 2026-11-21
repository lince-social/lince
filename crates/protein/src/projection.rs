//! Filtering and status rows shared by Calendar and Schedule forecasts.

use std::mem::take;

use nucleus::{DecimalValue, projection::Status};
use serde_json::{Value, json};

use crate::Predicate;

pub(super) fn status_row(status: &Status, timezone: &str) -> Value {
    json!({ "uid": "projection-status", "kind": "projection-status", "status": status, "timezone": timezone })
}

/// Separates the forecast quantity filters from the Record candidate filters.
pub(super) fn split(filters: &mut Vec<Predicate>) -> Option<Vec<Predicate>> {
    let mut quantities = Vec::new();
    for predicate in take(filters) {
        if !partition(predicate, filters, &mut quantities) {
            return None;
        }
    }
    Some(quantities)
}

fn partition(
    predicate: Predicate,
    filters: &mut Vec<Predicate>,
    quantities: &mut Vec<Predicate>,
) -> bool {
    match predicate {
        Predicate::All(children) => {
            return children
                .into_iter()
                .all(|child| partition(child, filters, quantities));
        }
        Predicate::QuantityLt(_)
        | Predicate::QuantityLte(_)
        | Predicate::QuantityGt(_)
        | Predicate::QuantityGte(_)
        | Predicate::QuantityEq(_) => quantities.push(predicate),
        Predicate::UidEq(_)
        | Predicate::RecordEq(_)
        | Predicate::KindEq(_)
        | Predicate::SlugEq(_)
        | Predicate::TextContains(_)
        | Predicate::OrganEq(_)
        | Predicate::OrganIn(_) => filters.push(predicate),
        _ => return false,
    }
    true
}

pub(super) fn matches_quantity(predicate: &Predicate, value: DecimalValue) -> bool {
    let expected = match *predicate {
        Predicate::QuantityLt(value)
        | Predicate::QuantityLte(value)
        | Predicate::QuantityGt(value)
        | Predicate::QuantityGte(value)
        | Predicate::QuantityEq(value) => value,
        _ => return false,
    };
    let Some(delta) = value.aligned_sub(expected) else {
        return false;
    };
    match predicate {
        Predicate::QuantityLt(_) => delta.is_negative(),
        Predicate::QuantityLte(_) => delta.is_negative() || delta.is_zero(),
        Predicate::QuantityGt(_) => !delta.is_negative() && !delta.is_zero(),
        Predicate::QuantityGte(_) => !delta.is_negative(),
        Predicate::QuantityEq(_) => delta.is_zero(),
        _ => false,
    }
}
