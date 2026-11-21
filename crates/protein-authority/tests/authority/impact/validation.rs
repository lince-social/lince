use protein_authority::{
    authority::{
        AssertionRole, AuthorityError, GraphSnapshot, Limits, selector_membership_changes,
    },
    query::Predicate,
};

use super::fixtures::{graph, record, under};
use crate::fixtures::{assertion, c, r};

#[test]
fn authority_impact_missing_not_dependency_refuses_even_on_empty_graph() {
    let graph = GraphSnapshot::default();
    assert_eq!(
        selector_membership_changes(
            &[Predicate::Not(Box::new(Predicate::UidEq(r(99))))],
            &graph,
            &graph,
            &Limits::default()
        ),
        Err(AuthorityError::MissingDependency)
    );
}

#[test]
fn authority_impact_missing_dependency_in_proposed_graph_refuses() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.records.remove(5);
    assert_eq!(
        selector_membership_changes(
            &[Predicate::Any(vec![
                Predicate::All(Vec::new()),
                Predicate::UidEq(r(6))
            ])],
            &current,
            &proposed,
            &Limits::default()
        ),
        Err(AuthorityError::MissingDependency)
    );
}

#[test]
fn authority_impact_later_invalid_selector_never_returns_partial_changes() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.records[2].deleted = true;
    assert_eq!(
        selector_membership_changes(
            &[Predicate::All(Vec::new()), Predicate::ConceptIn(c(99))],
            &current,
            &proposed,
            &Limits::default()
        ),
        Err(AuthorityError::MissingDependency)
    );
}

#[test]
fn authority_impact_unsupported_short_circuit_branch_refuses() {
    let graph = graph();
    for selector in [
        Predicate::Any(vec![Predicate::All(Vec::new()), Predicate::RevisionEq(1)]),
        Predicate::All(vec![
            Predicate::Any(Vec::new()),
            Predicate::StatusIn(vec!["active".into()]),
        ]),
        Predicate::Not(Box::new(Predicate::RecordEq(r(3)))),
    ] {
        assert_eq!(
            selector_membership_changes(&[selector], &graph, &graph, &Limits::default()),
            Err(AuthorityError::UnsupportedPredicate)
        );
    }
}

#[test]
fn authority_impact_invalid_identity_and_kind_refuse() {
    let graph = graph();
    assert_eq!(
        selector_membership_changes(
            &[Predicate::ConceptIn("task".into())],
            &graph,
            &graph,
            &Limits::default()
        ),
        Err(AuthorityError::InvalidIdentity)
    );
    assert_eq!(
        selector_membership_changes(
            &[Predicate::KindEq("company-task".into())],
            &graph,
            &graph,
            &Limits::default()
        ),
        Err(AuthorityError::InvalidPolicy)
    );
}

#[test]
fn authority_impact_malformed_graphs_refuse_before_empty_selector_result() {
    let current = graph();
    let mut duplicate = current.clone();
    duplicate.records.push(record(1));
    let mut missing = current.clone();
    missing.assertions.push(assertion(1, 3, 1, Some(99)));
    let mut identity = current.clone();
    let mut invalid = assertion(1, 3, 1, Some(4));
    invalid.role = AssertionRole::Identity;
    identity.assertions.push(invalid);
    for (proposed, error) in [
        (duplicate, AuthorityError::InvalidGraph),
        (missing, AuthorityError::MissingDependency),
        (identity, AuthorityError::InvalidGraph),
    ] {
        assert_eq!(
            selector_membership_changes(&[], &current, &proposed, &Limits::default()),
            Err(error)
        );
    }
}

#[test]
fn authority_impact_concept_cycle_refuses_without_selectors() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.concepts[0].parents.insert(c(2));
    assert_eq!(
        selector_membership_changes(&[], &current, &proposed, &Limits::default()),
        Err(AuthorityError::CyclicGraph)
    );
}

#[test]
fn authority_impact_under_retains_disconnected_cycle_refusal() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.assertions = vec![assertion(1, 5, 4, Some(6)), assertion(2, 6, 4, Some(5))];
    assert_eq!(
        selector_membership_changes(
            &[Predicate::Any(vec![
                Predicate::All(Vec::new()),
                under(false)
            ])],
            &current,
            &proposed,
            &Limits::default()
        ),
        Err(AuthorityError::CyclicGraph)
    );
}
