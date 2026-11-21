use std::{collections::BTreeSet, fmt};

use nucleus::performance::{Span, add};

use super::{
    AuthorityError, GraphSnapshot, Limits, RolePolicy, VisibilityCeiling,
    budget::{Budget, within},
    graph::Graph,
    selector::{Evaluation, Selector, Selectors},
};
use crate::query::Predicate;

/// A validated snapshot and its initial budget charge, reusable across read policies.
pub struct ReadGraph<'snapshot, 'limits> {
    graph: Graph<'snapshot>,
    baseline: Budget<'limits>,
}

impl fmt::Debug for ReadGraph<'_, '_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("ReadGraph").finish_non_exhaustive()
    }
}

impl<'snapshot, 'limits> ReadGraph<'snapshot, 'limits> {
    /// Validates and indexes a snapshot for repeated reads.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid, cyclic, incomplete, or oversized graph data.
    pub fn new(
        snapshot: &'snapshot GraphSnapshot,
        limits: &'limits Limits,
    ) -> Result<Self, AuthorityError> {
        let mut baseline = Budget::new(limits);
        let graph = Graph::prepare(snapshot, &mut baseline)?;
        add("authority-read-graphs-prepared", 1);
        Ok(Self { graph, baseline })
    }

    /// Returns live Records that satisfy both the policy and the visibility ceiling.
    ///
    /// # Errors
    ///
    /// Returns an error if policy or ceiling validation or selector evaluation fails.
    pub fn readable_records(
        &self,
        policy: &RolePolicy,
        ceiling: &VisibilityCeiling,
    ) -> Result<BTreeSet<String>, AuthorityError> {
        let _read = Span::new("authority-prepared-read");
        add("authority-read-policy-evaluations", 1);
        let mut budget = self.baseline;
        let selectors = {
            let _preparation = Span::new("authority-read-policy-preparation");
            Selectors::prepare(policy, &self.graph, ceiling, &mut budget)?
        };
        collect(&self.graph, &selectors.read, ceiling, &mut budget)
    }
}

fn collect(
    graph: &Graph<'_>,
    read: &Selector<'_>,
    ceiling: &VisibilityCeiling,
    budget: &mut Budget<'_>,
) -> Result<BTreeSet<String>, AuthorityError> {
    let mut readable = BTreeSet::new();
    for record in graph.records.values() {
        budget.step()?;
        if !record.deleted
            && ceiling.records.contains(&record.uid)
            && read.matches(record, budget)?
        {
            readable.insert(record.uid.clone());
        }
    }
    Ok(readable)
}

/// Returns all live Record UIDs permitted by the policy and visibility ceiling.
///
/// # Errors
///
/// Returns an error if the policy is absent or the snapshot, ceiling, policy,
/// or selector evaluation cannot be validated within the limits.
pub fn readable_records(
    policy: Option<&RolePolicy>,
    graph: &GraphSnapshot,
    ceiling: &VisibilityCeiling,
    limits: &Limits,
) -> Result<BTreeSet<String>, AuthorityError> {
    let policy = policy.ok_or(AuthorityError::MissingPolicy)?;
    let mut budget = Budget::new(limits);
    let evaluation = Evaluation::prepare(policy, graph, ceiling, &mut budget)?;
    collect(&evaluation.graph, &evaluation.read, ceiling, &mut budget)
}

/// Checks whether a live Record may be read under the supplied policy and ceiling.
///
/// # Errors
///
/// Returns an error for an unresolved UID, missing policy, or invalid input
/// to the policy evaluator, including exceeded limits.
pub fn may_read(
    policy: Option<&RolePolicy>,
    graph: &GraphSnapshot,
    ceiling: &VisibilityCeiling,
    record_uid: &str,
    limits: &Limits,
) -> Result<bool, AuthorityError> {
    let policy = policy.ok_or(AuthorityError::MissingPolicy)?;
    let mut budget = Budget::new(limits);
    let evaluation = Evaluation::prepare(policy, graph, ceiling, &mut budget)?;
    evaluation.reference_readable(record_uid, &mut budget)
}

/// Returns live Record UIDs whose membership changes for any supplied selector.
///
/// # Errors
///
/// Returns an error if either snapshot or any selector cannot be evaluated within
/// the shared limits.
pub fn selector_membership_changes(
    selectors: &[Predicate],
    current: &GraphSnapshot,
    proposed: &GraphSnapshot,
    limits: &Limits,
) -> Result<BTreeSet<String>, AuthorityError> {
    let _membership = Span::new("authority-selector-membership");
    within(selectors.len(), limits.predicate_nodes)?;
    let mut budget = Budget::new(limits);
    let current = Graph::prepare(current, &mut budget)?;
    let proposed = Graph::prepare(proposed, &mut budget)?;
    let mut prepared = Vec::with_capacity(selectors.len());
    for selector in selectors {
        prepared.push((
            Selector::compile(selector, &current, &mut budget, 0)?,
            Selector::compile(selector, &proposed, &mut budget, 0)?,
        ));
    }
    let mut changed = BTreeSet::new();
    for &uid in current.records.keys().chain(
        proposed
            .records
            .keys()
            .filter(|uid| !current.records.contains_key(**uid)),
    ) {
        budget.step()?;
        let before = current.records.get(uid).copied();
        let after = proposed.records.get(uid).copied();
        let mut differs = false;
        // A later selector can still refuse the whole comparison.
        for (current_selector, proposed_selector) in &prepared {
            budget.step()?;
            let before = match before {
                Some(record) if !record.deleted => current_selector.matches(record, &mut budget)?,
                _ => false,
            };
            let after = match after {
                Some(record) if !record.deleted => {
                    proposed_selector.matches(record, &mut budget)?
                }
                _ => false,
            };
            differs |= before != after;
        }
        if differs {
            budget.text(uid)?;
            changed.insert(uid.to_owned());
        }
    }
    Ok(changed)
}
