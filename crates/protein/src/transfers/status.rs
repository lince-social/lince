//! Transfer status, agreement readiness, and settlement application.

use super::roles;
use crate::ProteinError;
use nucleus::{
    PromiseState,
    transfer::{AgreementType, policy_satisfied},
};
use std::collections::HashMap;
use store::{
    Store,
    misc::PromiseRow,
    transfer_accounting::{Binding, bound_record, effective},
    transfer_agreement::read,
    transfers::{
        OccurrenceSettlementProgress, TransferAgreementReadinessInput, TransferOccurrenceRow,
        TransferSourceGroupState, agreement_readiness_input,
    },
};

pub(super) fn all_concrete_match(
    states: &[PromiseState],
    when_empty: bool,
    matches: impl Fn(PromiseState) -> bool,
) -> bool {
    use PromiseState::{Open, Withdrawn};

    let mut concrete = states
        .iter()
        .copied()
        .filter(|state| !matches!(state, Open | Withdrawn));
    concrete
        .next()
        .map_or(when_empty, |first| matches(first) && concrete.all(matches))
}

pub(super) fn correction_status(
    system_disputed: bool,
    participant_disputed: bool,
    applications: usize,
    compensated: usize,
) -> &'static str {
    if system_disputed {
        "system_disputed"
    } else if participant_disputed {
        "participant_disputed"
    } else if applications > 0 && compensated == applications {
        "private_applications_compensated"
    } else if compensated > 0 {
        "private_applications_partially_compensated"
    } else {
        "none"
    }
}

pub(super) fn derive_transfer_status(
    active: bool,
    agreement_type: &str,
    agreement_pct: Option<i64>,
    party_levels: &[i64],
    promise_states: &[PromiseState],
) -> &'static str {
    use PromiseState::*;
    if !active {
        return "inactive";
    }
    if promise_states.is_empty() {
        return "draft";
    }
    if promise_states.iter().all(|s| matches!(s, Withdrawn)) {
        return "withdrawn";
    }
    if promise_states.iter().all(|s| matches!(s, Kept)) {
        return "settled";
    }
    if promise_states.contains(&Kept) {
        return "partially_settled";
    }
    if promise_states.contains(&Broken) {
        return "broken";
    }
    if promise_states.contains(&Active) {
        return "in_transfer";
    }
    let policy = AgreementType::parse(agreement_type).is_some_and(|kind| {
        policy_satisfied(kind, agreement_pct.map(|pct| pct as u8), party_levels)
    });
    if policy
        && all_concrete_match(promise_states, false, |state| {
            matches!(state, Agreed | Kept)
        })
    {
        return "agreed";
    }
    if promise_states
        .iter()
        .any(|s| matches!(s, Proposed | Agreed))
    {
        return "proposed";
    }
    "draft"
}

pub(super) struct PrimaryStatus<'a> {
    pub revision: u64,
    pub active: bool,
    pub promise_states: &'a [PromiseState],
    pub occurrences: &'a [TransferOccurrenceRow],
    pub progress: &'a HashMap<&'a str, OccurrenceSettlementProgress>,
    pub source_group: Option<&'a TransferSourceGroupState>,
    pub system_disputed: bool,
    pub disputed: bool,
    pub agreement_ready: bool,
    pub expired: bool,
    pub cancelled: bool,
    pub awaiting_me: bool,
    pub awaiting_others: bool,
}

pub(super) fn derive_transfer_primary_status(input: &PrimaryStatus<'_>) -> &'static str {
    let &PrimaryStatus {
        revision,
        active,
        promise_states,
        occurrences,
        progress,
        source_group,
        system_disputed,
        disputed,
        agreement_ready,
        expired,
        cancelled,
        awaiting_me,
        awaiting_others,
    } = input;
    use PromiseState::*;
    if revision == 0 {
        return "legacy";
    }
    if system_disputed {
        return "system_disputed";
    }
    if disputed {
        return "disputed";
    }
    if promise_states.contains(&Broken) {
        return "broken";
    }
    if expired {
        return "expired";
    }
    if cancelled {
        return "cancelled";
    }
    let all_occurrences_settled = !occurrences.is_empty()
        && occurrences.iter().all(|occurrence| {
            progress
                .get(occurrence.uid.as_str())
                .is_some_and(|progress| progress.settled)
        });
    let completed =
        all_occurrences_settled || all_concrete_match(promise_states, false, |state| state == Kept);
    if !completed
        && (progress
            .values()
            .any(|progress| progress.settled_quantity > 0.0)
            || promise_states.contains(&Kept))
    {
        return "partially_settled";
    }
    if source_group.is_some_and(|group| group.satiated) {
        return "satiated";
    }
    if completed {
        return "completed";
    }
    if awaiting_me {
        return "awaiting_me";
    }
    if !occurrences.is_empty() || promise_states.contains(&Active) {
        return "active";
    }
    if awaiting_others {
        return "awaiting_others";
    }
    if agreement_ready {
        return "agreed";
    }
    if promise_states
        .iter()
        .any(|state| matches!(state, Proposed | Agreed))
    {
        return "proposed";
    }
    if promise_states.contains(&Open) {
        return "open";
    }
    if !active && promise_states.is_empty() {
        return "inactive";
    }
    "draft"
}

pub(super) fn agreement_required(
    agreement_type: &str,
    agreement_pct: Option<i64>,
    parties: usize,
) -> usize {
    match agreement_type {
        "full" => parties,
        "percentage" => {
            let pct = agreement_pct.unwrap_or(100).clamp(0, 100) as usize;
            parties.saturating_mul(pct).div_ceil(100)
        }
        _ => 0,
    }
}

pub(super) const PHASE_1_DRAFT_ACTIONS_BLOCKER: &str = "phase_1_draft_actions_not_available";

pub(super) struct EffectiveSettlementApplication {
    pub(super) record: Option<String>,
    pub(super) formula: String,
    pub(super) formula_hash: String,
    pub(super) version: u64,
    pub(super) source: &'static str,
    pub(super) error: Option<String>,
}

pub(super) async fn effective_settlement_application(
    store: &Store,
    occurrence: &TransferOccurrenceRow,
    source_promise: &PromiseRow,
    owner_person_uid: &str,
) -> Result<EffectiveSettlementApplication, ProteinError> {
    let transfer = &occurrence.transfer_uid;
    let exchange = occurrence
        .exchange_uid
        .as_deref()
        .unwrap_or(&occurrence.promise_uid);
    let record = bound_record(
        &store.pool,
        transfer,
        exchange,
        owner_person_uid,
        source_promise.record_uid.as_deref(),
    )
    .await?;
    match effective(
        &store.pool,
        &Binding {
            transfer,
            exchange,
            occurrence: Some(&occurrence.uid),
            person: owner_person_uid,
            record: record.as_deref(),
            unit: occurrence.unit_uid.as_deref(),
            outgoing: source_promise.delta < 0.0,
        },
    )
    .await
    {
        Ok(application) => Ok(EffectiveSettlementApplication {
            record: application.record,
            formula: application.formula,
            formula_hash: application.formula_hash,
            version: application.version,
            source: if application.policy.is_some() {
                "private_policy"
            } else {
                "default"
            },
            error: None,
        }),
        Err(error) => Ok(EffectiveSettlementApplication {
            record,
            formula: String::new(),
            formula_hash: String::new(),
            version: 0,
            source: "unavailable",
            error: Some(error.to_string()),
        }),
    }
}

/// Reports current agreement readiness, including required child Transfers.
///
/// # Errors
/// Returns an error if the Transfer or its agreement inputs cannot be read.
pub async fn transfer_agreement_ready(
    store: &Store,
    transfer_uid: &str,
) -> Result<bool, ProteinError> {
    Ok(read(&store.pool, transfer_uid).await?.0.ready)
}

/// Lists the Person's ready promises that may become occurrences.
///
/// # Errors
/// Returns an error if the agreement or its promises cannot be read.
pub async fn transfer_ready_promises_for_person(
    store: &Store,
    transfer_uid: &str,
    person_uid: &str,
) -> Result<Vec<String>, ProteinError> {
    let input = agreement_readiness_input(&store.pool, transfer_uid).await?;
    let readiness = read(&store.pool, transfer_uid).await?.0;
    Ok(input
        .promises
        .into_iter()
        .filter(|promise| promise.person_uid.as_deref() == Some(person_uid))
        .filter(|promise| {
            promise.state == "agreed"
                || input.agreement_type == "dependency" && promise.state == "proposed"
        })
        .filter(|promise| {
            readiness
                .promises
                .get(&promise.uid)
                .is_some_and(|state| state.ready)
        })
        .map(|promise| promise.uid)
        .collect())
}

/// Resolves the opposite promise, giver, and receiver for an occurrence.
///
/// # Errors
/// Returns the blocker code when the promise cannot form an occurrence.
pub fn transfer_occurrence_roles(
    input: &TransferAgreementReadinessInput,
    promise_uid: &str,
) -> Result<(Option<String>, String, String), &'static str> {
    roles::one(input, promise_uid)
}
