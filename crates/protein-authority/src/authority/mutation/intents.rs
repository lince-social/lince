use std::collections::{BTreeMap, BTreeSet};

use crate::authority::{
    AssertionIntent, AssertionRole, AssertionState, AuthorityError, Property,
    budget::{Budget, identity, within},
    graph::valid_uri,
    selector::Evaluation,
};

pub(super) struct CheckedAssertionIntent<'a> {
    pub(super) intent: &'a AssertionIntent,
    pub(super) before_is_current: bool,
}

fn validate_intent_state(
    assertion: &AssertionState,
    evaluation: &Evaluation<'_>,
    budget: &mut Budget<'_>,
) -> Result<(), AuthorityError> {
    budget.step()?;
    identity(&assertion.uid, "a")?;
    budget.text(&assertion.uid)?;
    for uid in [&assertion.subject_uid, &assertion.predicate_uid] {
        budget.text(uid)?;
        budget.edge()?;
    }
    evaluation.graph.record(&assertion.subject_uid)?;
    evaluation.graph.concept(&assertion.predicate_uid)?;
    if let Some(uid) = &assertion.object_uid {
        budget.text(uid)?;
        budget.edge()?;
        evaluation.graph.record(uid)?;
    }
    if let Some(uri) = &assertion.object_uri {
        budget.text(uri)?;
        if assertion.object_uid.is_some() || !valid_uri(uri) {
            return Err(AuthorityError::InvalidGraph);
        }
    }
    if let Some(uid) = &assertion.unit_uid {
        budget.text(uid)?;
        budget.edge()?;
        evaluation.graph.concept(uid)?;
    }
    if assertion.invalid_identity_shape() {
        return Err(AuthorityError::InvalidMutation);
    }
    Ok(())
}

pub(super) fn replay_assertion_intents<'a>(
    intents: &'a [AssertionIntent],
    current: &'a Evaluation<'_>,
    proposed: &Evaluation<'_>,
    targets: &BTreeMap<&str, &BTreeSet<Property>>,
    budget: &mut Budget<'_>,
) -> Result<BTreeMap<&'a str, Vec<CheckedAssertionIntent<'a>>>, AuthorityError> {
    within(intents.len(), budget.limits.assertions)?;
    let mut active = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut tuples = BTreeSet::new();
    let mut identities = BTreeSet::new();
    for (&uid, &assertion) in &current.graph.assertions {
        budget.step()?;
        budget.text(uid)?;
        active.insert(uid, (assertion, true));
        seen.insert(uid);
        tuples.insert(assertion.relation_key());
        if assertion.role == AssertionRole::Identity {
            identities.insert(assertion.subject_uid.as_str());
        }
    }
    let mut checked: BTreeMap<&str, Vec<CheckedAssertionIntent<'_>>> = BTreeMap::new();
    for intent in intents {
        budget.step()?;
        let subject = intent
            .before
            .as_ref()
            .or(intent.after.as_ref())
            .ok_or(AuthorityError::InvalidMutation)?;
        if !targets.contains_key(subject.subject_uid.as_str()) {
            return Err(AuthorityError::InvalidMutation);
        }
        for _ in &intent.touched_properties {
            budget.step()?;
        }
        let mut before_is_current = false;
        if let Some(before) = &intent.before {
            let (actual, is_current) = active
                .get(before.uid.as_str())
                .copied()
                .ok_or(AuthorityError::InvalidMutation)?;
            if actual != before {
                return Err(AuthorityError::InvalidMutation);
            }
            before_is_current = is_current;
            validate_intent_state(before, if is_current { current } else { proposed }, budget)?;
            if let Some(after) = &intent.after
                && (before.uid != after.uid || before.relation_key() != after.relation_key())
            {
                return Err(AuthorityError::InvalidMutation);
            }
            active.remove(before.uid.as_str());
            tuples.remove(&before.relation_key());
            if before.role == AssertionRole::Identity {
                identities.remove(before.subject_uid.as_str());
            }
        }
        if let Some(after) = &intent.after {
            validate_intent_state(after, proposed, budget)?;
            if intent.before.is_none() && !seen.insert(after.uid.as_str()) {
                return Err(AuthorityError::InvalidMutation);
            }
            within(seen.len(), budget.limits.assertions)?;
            if !tuples.insert(after.relation_key())
                || after.role == AssertionRole::Identity
                    && !identities.insert(after.subject_uid.as_str())
                || active.insert(after.uid.as_str(), (after, false)).is_some()
            {
                return Err(AuthorityError::InvalidMutation);
            }
            within(active.len(), budget.limits.assertions)?;
        }
        checked
            .entry(&subject.subject_uid)
            .or_default()
            .push(CheckedAssertionIntent {
                intent,
                before_is_current,
            });
    }
    if active.len() != proposed.graph.assertions.len() {
        return Err(AuthorityError::InvalidMutation);
    }
    for (uid, (assertion, _)) in active {
        budget.step()?;
        if proposed.graph.assertions.get(uid).copied() != Some(assertion) {
            return Err(AuthorityError::InvalidMutation);
        }
    }
    Ok(checked)
}
