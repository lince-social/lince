use std::collections::{BTreeMap, BTreeSet};

use super::{
    AssertionIntent, AuthorityError, GraphSnapshot, Limits, MutationTarget, Property,
    RecordDecision, RolePolicy, VisibilityCeiling,
    budget::{Budget, identity, within},
    graph::Graph,
    selector::Evaluation,
};

mod grants;
mod intents;
mod record;

use self::{intents::replay_assertion_intents, record::authorize_prepared_record_change};

/// Evaluates a single implied Record change without a declared footprint.
///
/// # Errors
///
/// Returns an error if the change lacks authority or the snapshots, ceilings,
/// or policy cannot be validated within the limits.
pub fn authorize_record_change(
    policy: Option<&RolePolicy>,
    current: &GraphSnapshot,
    proposed: &GraphSnapshot,
    current_ceiling: &VisibilityCeiling,
    proposed_ceiling: &VisibilityCeiling,
    record_uid: &str,
    limits: &Limits,
) -> Result<RecordDecision, AuthorityError> {
    authorize_record_changes(
        policy,
        current,
        proposed,
        current_ceiling,
        proposed_ceiling,
        &[MutationTarget {
            record_uid: record_uid.to_string(),
            touched_properties: BTreeSet::new(),
        }],
        limits,
    )?
    .remove(record_uid)
    .ok_or(AuthorityError::InvalidMutation)
}

/// Evaluates the entire batch against the current and proposed authorization views.
///
/// # Errors
///
/// Returns an error if any change falls outside its declared targets or lacks
/// authority, or if graph and policy validation or the evaluation fails.
pub fn authorize_record_changes(
    policy: Option<&RolePolicy>,
    current: &GraphSnapshot,
    proposed: &GraphSnapshot,
    current_ceiling: &VisibilityCeiling,
    proposed_ceiling: &VisibilityCeiling,
    targets: &[MutationTarget],
    limits: &Limits,
) -> Result<BTreeMap<String, RecordDecision>, AuthorityError> {
    authorize_changes(
        policy,
        (current, current_ceiling),
        (proposed, proposed_ceiling),
        targets,
        None,
        limits,
    )
}

/// Evaluates a batch and validates the declared sequence of assertion edits.
///
/// # Errors
///
/// Returns an error if the edits cannot be replayed to the proposed snapshot,
/// any change lacks authority, or validation or evaluation exceeds the limits.
#[expect(
    clippy::too_many_arguments,
    reason = "Keep declared assertions and authorization context separate at the public boundary"
)]
pub fn authorize_record_changes_with_assertion_intents(
    policy: Option<&RolePolicy>,
    current: &GraphSnapshot,
    proposed: &GraphSnapshot,
    current_ceiling: &VisibilityCeiling,
    proposed_ceiling: &VisibilityCeiling,
    targets: &[MutationTarget],
    intents: &[AssertionIntent],
    limits: &Limits,
) -> Result<BTreeMap<String, RecordDecision>, AuthorityError> {
    authorize_changes(
        policy,
        (current, current_ceiling),
        (proposed, proposed_ceiling),
        targets,
        Some(intents),
        limits,
    )
}

type Snapshot<'a> = (&'a GraphSnapshot, &'a VisibilityCeiling);

fn authorize_changes(
    policy: Option<&RolePolicy>,
    (current, current_ceiling): Snapshot<'_>,
    (proposed, proposed_ceiling): Snapshot<'_>,
    targets: &[MutationTarget],
    intents: Option<&[AssertionIntent]>,
    limits: &Limits,
) -> Result<BTreeMap<String, RecordDecision>, AuthorityError> {
    let policy = policy.ok_or(AuthorityError::MissingPolicy)?;
    let mut budget = Budget::new(limits);
    within(targets.len(), limits.records)?;
    let current = Evaluation::prepare(policy, current, current_ceiling, &mut budget)?;
    let proposed = Evaluation::prepare(policy, proposed, proposed_ceiling, &mut budget)?;
    let mut indexed = BTreeMap::new();
    for target in targets {
        budget.step()?;
        identity(&target.record_uid, "r")?;
        budget.text(&target.record_uid)?;
        if indexed
            .insert(target.record_uid.as_str(), &target.touched_properties)
            .is_some()
        {
            return Err(AuthorityError::InvalidMutation);
        }
        for property in &target.touched_properties {
            budget.step()?;
            if let Property::Extension(property) = property {
                budget.property(property)?;
            }
        }
    }
    validate_batch_changes(&current.graph, &proposed.graph, &indexed, &mut budget)?;
    let intents = intents
        .map(|intents| {
            replay_assertion_intents(intents, &current, &proposed, &indexed, &mut budget)
        })
        .transpose()?
        .unwrap_or_default();
    let mut decisions = BTreeMap::new();
    for (uid, footprint) in indexed {
        let decision = authorize_prepared_record_change(
            policy,
            &current,
            &proposed,
            uid,
            footprint,
            intents.get(uid).map_or(&[], Vec::as_slice),
            &mut budget,
        )?;
        decisions.insert(uid.to_string(), decision);
    }
    Ok(decisions)
}

fn validate_batch_changes(
    current: &Graph<'_>,
    proposed: &Graph<'_>,
    targets: &BTreeMap<&str, &BTreeSet<Property>>,
    budget: &mut Budget<'_>,
) -> Result<(), AuthorityError> {
    if current.concepts.len() != proposed.concepts.len()
        || current.source.places.len() != proposed.source.places.len()
    {
        return Err(AuthorityError::InvalidMutation);
    }
    for (uid, concept) in &current.concepts {
        budget.step()?;
        if proposed.concepts.get(uid) != Some(concept) {
            return Err(AuthorityError::InvalidMutation);
        }
    }
    for uid in &current.source.places {
        budget.step()?;
        if !proposed.source.places.contains(uid) {
            return Err(AuthorityError::InvalidMutation);
        }
    }
    for (uid, record) in &current.records {
        budget.step()?;
        if proposed.records.get(uid) != Some(record) && !targets.contains_key(uid) {
            return Err(AuthorityError::InvalidMutation);
        }
    }
    for uid in proposed.records.keys() {
        budget.step()?;
        if !current.records.contains_key(uid) && !targets.contains_key(uid) {
            return Err(AuthorityError::InvalidMutation);
        }
    }
    for (before, after) in [(current, proposed), (proposed, current)] {
        for (uid, assertion) in &before.assertions {
            budget.step()?;
            if after.assertions.get(uid) != Some(assertion)
                && !targets.contains_key(assertion.subject_uid.as_str())
            {
                return Err(AuthorityError::InvalidMutation);
            }
        }
    }
    Ok(())
}
