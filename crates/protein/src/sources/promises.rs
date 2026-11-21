//! Promise reads within the actor’s visibility scope.

use std::collections::HashSet;

use nucleus::{PromiseState, performance::Span};
use serde_json::{Value, json};
use store::{
    Store,
    misc::{PromiseRow, list_promises},
    sqlx::Error::Protocol,
};

use crate::{Predicate, Protein, ProteinError, transfers::protected_transfer_history};

pub(crate) async fn execute_promises(
    store: &Store,
    protein: &Protein,
    visible: Option<&HashSet<String>>,
    subject: Option<&str>,
) -> Result<Vec<Value>, ProteinError> {
    let protected_transfers = protected_transfer_history(store, subject).await?;
    let mut out = Vec::new();
    let promises = list_promises(&store.pool).await?;
    let _selection = Span::new("protein-promise-selection");
    'promises: for promise in promises {
        if !readable(&promise, visible, &protected_transfers) {
            continue;
        }
        for predicate in &protein.filter {
            if !matches(&promise, predicate)? {
                continue 'promises;
            }
        }
        let open = promise.state == PromiseState::Open;
        out.push(json!({
            "uid": promise.uid,
            "record": promise.record_uid,
            "delta": promise.delta,
            "direction": if promise.delta < 0.0 {
                "give"
            } else if promise.delta > 0.0 {
                "receive"
            } else {
                "invalid"
            },
            "state": promise.state.as_str(),
            "open": open,
            "proposer": promise.party_uid.as_deref().filter(|_| open),
            "reuse_policy": promise.open_reuse_policy.as_str(),
            "window_end": promise.window_end,
            "party": promise.party_uid,
            "transfer": promise.transfer_uid,
            "rule": promise.rule_uid,
        }));
        if protein.limit.is_some_and(|l| out.len() >= l) {
            break;
        }
    }
    Ok(out)
}

pub(super) fn readable(
    promise: &PromiseRow,
    visible: Option<&HashSet<String>>,
    protected: &HashSet<String>,
) -> bool {
    !promise
        .transfer_uid
        .as_ref()
        .is_some_and(|uid| protected.contains(uid))
        && visible.is_none_or(|visible| {
            promise
                .record_uid
                .as_deref()
                .is_some_and(|uid| visible.contains(uid))
        })
}

fn matches(promise: &PromiseRow, predicate: &Predicate) -> Result<bool, ProteinError> {
    Ok(match predicate {
        // Evaluate every child until an error so a determined result cannot hide one.
        Predicate::All(children) => children
            .iter()
            .try_fold(true, |all, child| -> Result<_, ProteinError> {
                Ok(matches(promise, child)? && all)
            })?,
        Predicate::Any(children) => children
            .iter()
            .try_fold(false, |any, child| -> Result<_, ProteinError> {
                Ok(matches(promise, child)? || any)
            })?,
        Predicate::Not(child) => !matches(promise, child)?,
        Predicate::StateIn(states) => states.iter().any(|state| state == promise.state.as_str()),
        Predicate::UidEq(uid) => uid == &promise.uid,
        _ => {
            return Err(Protocol(
                "protein_promise_unsupported_predicate:predicate is not defined for promises"
                    .into(),
            ));
        }
    })
}
