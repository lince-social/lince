use std::collections::BTreeSet;

use super::intents::CheckedAssertionIntent;
use crate::authority::{
    AssertionGrant, AssertionProperty, AssertionRole, AssertionState, AssertionTarget,
    AuthorityError, Property, budget::Budget, selector::Evaluation,
};

#[derive(Default)]
pub(super) struct MatchingGrants<'a> {
    pub(super) first: Option<usize>,
    pub(super) properties: BTreeSet<&'a Property>,
    pub(super) assertions_add: Vec<&'a AssertionGrant>,
    pub(super) assertions_remove: Vec<&'a AssertionGrant>,
}

pub(super) fn assertion_intents_allowed(
    intents: &[CheckedAssertionIntent<'_>],
    grant: &MatchingGrants<'_>,
    current: &Evaluation<'_>,
    proposed: &Evaluation<'_>,
    budget: &mut Budget<'_>,
) -> Result<bool, AuthorityError> {
    for checked in intents {
        budget.step()?;
        for (state, rules, evaluation) in [
            (
                checked.intent.before.as_ref(),
                &grant.assertions_remove,
                if checked.before_is_current {
                    current
                } else {
                    proposed
                },
            ),
            (
                checked.intent.after.as_ref(),
                &grant.assertions_add,
                proposed,
            ),
        ] {
            if let Some(state) = state {
                let Some(properties) = assertion_properties(state, rules, evaluation, budget)?
                else {
                    return Ok(false);
                };
                if state.role != AssertionRole::Identity
                    && !checked.intent.touched_properties.is_subset(&properties)
                {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}

pub(super) fn assertions_allowed(
    assertions: &[&AssertionState],
    grants: &[&AssertionGrant],
    evaluation: &Evaluation<'_>,
    budget: &mut Budget<'_>,
) -> Result<bool, AuthorityError> {
    for assertion in assertions {
        budget.step()?;
        if assertion_properties(assertion, grants, evaluation, budget)?.is_none() {
            return Ok(false);
        }
    }
    Ok(true)
}

fn assertion_properties(
    assertion: &AssertionState,
    grants: &[&AssertionGrant],
    evaluation: &Evaluation<'_>,
    budget: &mut Budget<'_>,
) -> Result<Option<BTreeSet<AssertionProperty>>, AuthorityError> {
    if !evaluation
        .ceiling
        .concepts
        .contains(&assertion.predicate_uid)
        || assertion
            .unit_uid
            .as_ref()
            .is_some_and(|uid| !evaluation.ceiling.concepts.contains(uid))
    {
        return Ok(None);
    }
    if let Some(uid) = evaluation.graph.object(assertion)
        && !evaluation.reference_readable(uid, budget)?
    {
        return Ok(None);
    }
    let mut matched = false;
    let mut properties = BTreeSet::new();
    for grant in grants {
        budget.step()?;
        if grant.predicate_uid == assertion.predicate_uid
            && grant.role == assertion.role
            && match (&grant.target, &assertion.object_uid) {
                (AssertionTarget::Unary, None) => assertion.object_uri.is_none(),
                (AssertionTarget::Uri(expected), None) => {
                    assertion.object_uri.as_ref() == Some(expected)
                }
                (AssertionTarget::Record(expected), Some(actual)) => expected == actual,
                (AssertionTarget::AnyReadableRecord, Some(_)) => true,
                _ => false,
            }
        {
            matched = true;
            properties.extend(grant.properties.iter().copied());
        }
    }
    Ok((matched
        && (assertion.quantity.is_none() || properties.contains(&AssertionProperty::Quantity))
        && (assertion.unit_uid.is_none() || properties.contains(&AssertionProperty::Unit)))
    .then_some(properties))
}
