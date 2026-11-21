use std::collections::BTreeSet;

use super::{
    grants::{MatchingGrants, assertion_intents_allowed, assertions_allowed},
    intents::CheckedAssertionIntent,
};
use crate::authority::{
    AuthorityError, Operation, Property, RecordContent, RecordDecision, RecordState, RolePolicy,
    budget::Budget, selector::Evaluation,
};

pub(super) fn authorize_prepared_record_change(
    policy: &RolePolicy,
    current: &Evaluation<'_>,
    proposed: &Evaluation<'_>,
    record_uid: &str,
    footprint: &BTreeSet<Property>,
    intents: &[CheckedAssertionIntent<'_>],
    budget: &mut Budget<'_>,
) -> Result<RecordDecision, AuthorityError> {
    budget.step()?;
    let before = current.graph.records.get(record_uid).copied();
    let after = proposed
        .graph
        .records
        .get(record_uid)
        .copied()
        .ok_or(AuthorityError::InvalidMutation)?;
    if before.is_some_and(|before| before.organ_uid != after.organ_uid) {
        return Err(AuthorityError::InvalidMutation);
    }
    let before_content = before
        .map(|record| {
            record
                .content
                .as_ref()
                .ok_or(AuthorityError::IncompleteRecord)
        })
        .transpose()?;
    let after_content = after
        .content
        .as_ref()
        .ok_or(AuthorityError::IncompleteRecord)?;
    let operation = match before {
        None if !after.deleted => Operation::Create,
        Some(before) if !before.deleted && after.deleted => Operation::Delete,
        Some(before) if before.deleted && !after.deleted => Operation::Restore,
        Some(before) if !before.deleted && !after.deleted => Operation::Update,
        _ => return Err(AuthorityError::InvalidMutation),
    };
    if let Some(before) = before
        && !current.readable(before, budget)?
    {
        return Err(AuthorityError::Denied);
    }
    if operation != Operation::Delete && !proposed.readable(after, budget)? {
        return Err(AuthorityError::Denied);
    }
    let complete = matches!(operation, Operation::Create | Operation::Restore);
    let mut properties = changed_properties(
        if complete { None } else { before },
        if complete { None } else { before_content },
        after,
        after_content,
        budget,
    )?;
    properties.extend(footprint.iter().cloned());
    let old_assertions = current.graph.assertions_by_subject.get(record_uid);
    let new_assertions = proposed.graph.assertions_by_subject.get(record_uid);
    let mut added = Vec::new();
    let mut removed = Vec::new();
    for (uid, assertion) in new_assertions
        .into_iter()
        .flat_map(|assertions| assertions.iter())
    {
        budget.step()?;
        if complete || old_assertions.and_then(|assertions| assertions.get(uid)) != Some(assertion)
        {
            added.push(*assertion);
        }
    }
    for (uid, assertion) in old_assertions
        .into_iter()
        .flat_map(|assertions| assertions.iter())
    {
        budget.step()?;
        if new_assertions.and_then(|assertions| assertions.get(uid)) != Some(assertion) {
            removed.push(*assertion);
        }
    }
    for property in &properties {
        budget.step()?;
        let permitted = match property {
            Property::Unit => after_content
                .unit_uid
                .as_ref()
                .is_none_or(|uid| proposed.ceiling.concepts.contains(uid)),
            Property::Place => after_content
                .place_uid
                .as_ref()
                .is_none_or(|uid| proposed.ceiling.places.contains(uid)),
            Property::Organ => match &after.organ_uid {
                Some(uid) => proposed.reference_readable(uid, budget)?,
                None => true,
            },
            _ => true,
        };
        if !permitted {
            return Err(AuthorityError::Denied);
        }
    }
    let mut combined = MatchingGrants::default();
    // Both snapshots prepare exactly one selector per policy grant, in policy order.
    let selectors = current.grants.iter().zip(&proposed.grants);
    for (index, (grant, (before_selector, after_selector))) in
        policy.grants.iter().zip(selectors).enumerate()
    {
        budget.step()?;
        if grant.operation != operation
            || operation != Operation::Delete && !after_selector.matches(after, budget)?
        {
            continue;
        }
        if let Some(before) = before
            && !before_selector.matches(before, budget)?
        {
            continue;
        }
        combined.first.get_or_insert(index);
        combined.properties.extend(&grant.properties);
        combined.assertions_add.extend(&grant.assertions_add);
        combined.assertions_remove.extend(&grant.assertions_remove);
    }
    if let Some(index) = combined.first
        && properties
            .iter()
            .all(|property| combined.properties.contains(property))
        && assertions_allowed(&added, &combined.assertions_add, proposed, budget)?
        && assertions_allowed(&removed, &combined.assertions_remove, current, budget)?
        && assertion_intents_allowed(intents, &combined, current, proposed, budget)?
    {
        return Ok(RecordDecision {
            operation,
            grant_index: index,
            properties,
            assertions_added: added
                .into_iter()
                .map(|assertion| assertion.uid.clone())
                .collect(),
            assertions_removed: removed
                .into_iter()
                .map(|assertion| assertion.uid.clone())
                .collect(),
        });
    }
    Err(AuthorityError::Denied)
}

fn changed_properties(
    before: Option<&RecordState>,
    before_content: Option<&RecordContent>,
    after: &RecordState,
    after_content: &RecordContent,
    budget: &mut Budget<'_>,
) -> Result<BTreeSet<Property>, AuthorityError> {
    use Property::{Body, Head, Kind, Organ, Place, Quantity, Slug, Unit};

    let mut properties = BTreeSet::new();
    for (property, changed) in [
        (Kind, before.is_none_or(|before| before.kind != after.kind)),
        (
            Head,
            before_content.is_none_or(|before| before.head != after_content.head),
        ),
        (
            Body,
            before_content.is_none_or(|before| before.body != after_content.body),
        ),
        (
            Quantity,
            before_content.is_none_or(|before| before.quantity != after_content.quantity),
        ),
        (
            Slug,
            before_content.and_then(|before| before.slug.as_deref())
                != after_content.slug.as_deref(),
        ),
        (
            Unit,
            before_content.and_then(|before| before.unit_uid.as_deref())
                != after_content.unit_uid.as_deref(),
        ),
        (
            Place,
            before_content.and_then(|before| before.place_uid.as_deref())
                != after_content.place_uid.as_deref(),
        ),
        (Organ, before.is_none() && after.organ_uid.is_some()),
    ] {
        if changed {
            properties.insert(property);
        }
    }
    for (property, value) in &after_content.extensions {
        budget.step()?;
        if before_content.and_then(|before| before.extensions.get(property)) != Some(value) {
            properties.insert(Property::Extension(property.clone()));
        }
    }
    if let Some(before) = before_content {
        for property in before.extensions.keys() {
            budget.step()?;
            if !after_content.extensions.contains_key(property) {
                properties.insert(Property::Extension(property.clone()));
            }
        }
    }
    Ok(properties)
}
