//! In-memory filtering and ordering of the heterogeneous Karma rows.

use std::cmp::Ordering;

use serde_json::Value;

use super::karma_query_error;
use crate::{Order, Predicate, Protein, ProteinError};

pub(super) fn finish(mut rows: Vec<Value>, query: &Protein) -> Result<Vec<Value>, ProteinError> {
    if !query.filter.is_empty() {
        let mut result = Ok(());
        rows.retain(|row| {
            result.is_ok()
                && matches_all(row, &query.filter).unwrap_or_else(|error| {
                    result = Err(error);
                    false
                })
        });
        result?;
    }
    let order = query
        .order
        .iter()
        .map(order_field)
        .collect::<Result<Vec<_>, _>>()?;
    if !order.is_empty() {
        rows.sort_by(|left, right| {
            for &(field, descending) in &order {
                let (a, b) = (sort_key(left, field), sort_key(right, field));
                let ordering = if descending { b.cmp(a) } else { a.cmp(b) };
                if !ordering.is_eq() {
                    return ordering;
                }
            }
            Ordering::Equal
        });
    }
    if let Some(limit) = query.limit {
        rows.truncate(limit);
    }
    Ok(rows)
}

fn matches_all(row: &Value, predicates: &[Predicate]) -> Result<bool, ProteinError> {
    for predicate in predicates {
        if !matches(row, predicate)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn matches(row: &Value, predicate: &Predicate) -> Result<bool, ProteinError> {
    Ok(match predicate {
        Predicate::All(children) => matches_all(row, children)?,
        Predicate::Any(children) => {
            for child in children {
                if matches(row, child)? {
                    return Ok(true);
                }
            }
            false
        }
        Predicate::Not(child) => !matches(row, child)?,
        Predicate::KindEq(value) => row["object_kind"].as_str() == Some(value),
        Predicate::UidEq(value) => row["uid"].as_str() == Some(value),
        Predicate::SlugEq(value) => row["slug"].as_str() == Some(value),
        Predicate::StatusIn(values) => row["status"]
            .as_str()
            .is_some_and(|status| values.iter().any(|value| value == status)),
        _ => {
            return Err(karma_query_error(
                "protein_karma_unsupported_predicate",
                "predicate is not defined for Karma objects",
            ));
        }
    })
}

fn order_field(order: &Order) -> Result<(&str, bool), ProteinError> {
    const CODE: &str = "protein_karma_unsupported_order";
    let (field, descending) = match order {
        Order::Asc(field) => (field.as_str(), false),
        Order::Desc(field) => (field.as_str(), true),
        Order::Link(_) => {
            return Err(karma_query_error(
                CODE,
                "link ordering is only defined for Record rows",
            ));
        }
    };
    if matches!(
        field,
        "object_kind" | "uid" | "slug" | "status" | "created_at" | "updated_at"
    ) {
        Ok((field, descending))
    } else {
        Err(karma_query_error(CODE, field))
    }
}

fn sort_key<'a>(row: &'a Value, field: &str) -> &'a str {
    row[field].as_str().unwrap_or_default()
}
