//! Transfer settlement preparation and authorized private applications.

mod handoff;
mod policies;

pub(super) use handoff::preview;
pub(super) use policies::policies;

use crate::ProteinError;
use nucleus::DecimalValue;
use serde_json::{Value, json};
use std::collections::HashSet;
use store::{
    Store,
    exact::difference,
    sqlx::{Error::Protocol, query_as},
    transfer_accounting::{Policy, allocated, amount, calculate, policy},
    transfer_cancellations::cancelled,
    transfer_effects::Group,
};

/// Returns the primary effect amounts, or evaluates the formula without an effects group.
pub(super) fn effect_amounts(
    group: Option<&Group>,
    formula: &str,
    canonical_after: DecimalValue,
    prior_local: DecimalValue,
) -> Result<(DecimalValue, DecimalValue), ProteinError> {
    match group {
        Some(group) => group
            .effects
            .first()
            .map(|effect| (effect.delta, effect.cumulative_after))
            .ok_or_else(|| Protocol("quoted settlement has no effects".into())),
        None => calculate(formula, canonical_after, prior_local),
    }
}

pub(super) async fn visible_policy(
    store: &Store,
    transfer: &str,
    exchange: &str,
    person: &str,
    visible: Option<&HashSet<String>>,
) -> Result<bool, ProteinError> {
    let Some(visible) = visible else {
        return Ok(true);
    };
    let policy = policy(&store.pool, transfer, exchange, person).await?;
    Ok(policy_visible(policy.as_ref(), Some(visible)))
}

fn policy_visible(policy: Option<&Policy>, visible: Option<&HashSet<String>>) -> bool {
    visible.is_none_or(|visible| {
        policy.is_none_or(|policy| {
            policy
                .effects
                .iter()
                .all(|effect| visible.contains(&effect.record))
        })
    })
}

pub(super) async fn prepare(
    store: &Store,
    transfer: &str,
    occurrence: &str,
    revision: u64,
    person: &str,
) -> Result<Value, ProteinError> {
    let (quantity, owner, state, delivered, received, disputed): (f64, String, String, bool, bool, bool) = query_as(
        "SELECT o.quantity, p.party_uid, p.state, o.delivery_claimed, o.receipt_claimed, o.disputed
         FROM transfer_occurrence o JOIN promise p ON p.uid = o.promise_uid WHERE o.uid = ? AND o.transfer_uid = ?"
    ).bind(occurrence).bind(transfer).fetch_one(&store.pool).await?;
    if owner != person {
        return Ok(Value::Null);
    }
    let allocated = allocated(&store.pool, occurrence, true).await?;
    let remaining = difference(
        difference(amount(quantity)?, cancelled(&store.pool, occurrence).await?)?,
        allocated,
    )?
    .to_f64()
    .max(0.0);
    let ready = remaining > 0.0 && state == "active" && delivered && received && !disputed;
    Ok(json!({
        "canonical_quantity":remaining,"expected_remaining_quantity":remaining,
        "capabilities":{"begin":ready},
        "blocking_reasons":{"begin":if ready { vec![] } else { vec!["occurrence_not_ready_or_already_prepared"] }},
        "action_payload":{"action":"begin-transfer-settlement","transfer":transfer,"occurrence":occurrence,
            "expected_revision":revision,"expected_remaining_quantity":remaining,"canonical_quantity":remaining,
            "request_id":Value::Null,"person":person}
    }))
}
