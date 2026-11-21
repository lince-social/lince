use std::collections::BTreeSet;

use nucleus::RecordKind;
use protein_authority::{
    authority::{Limits, RolePolicy, readable_records},
    query::Predicate,
};

use super::fixtures::{changed, graph, hierarchy, ids, under};
use crate::fixtures::{assertion, c, ceiling, r};

#[test]
fn authority_impact_concept_descendants_change_without_subject_write() {
    let mut current = graph();
    current.concepts[2].parents.insert(c(2));
    current.assertions = vec![assertion(1, 3, 2, None), assertion(2, 4, 3, None)];
    let mut proposed = current.clone();
    proposed.concepts[1].parents.clear();
    assert_eq!(
        changed(&[Predicate::ConceptIn(c(1))], &current, &proposed),
        ids(&[3, 4])
    );
    assert_eq!(
        changed(&[Predicate::ConceptIn(c(2))], &current, &proposed),
        ids(&[])
    );
}

#[test]
fn authority_impact_direct_classification_only_changes_subject() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.assertions.push(assertion(1, 3, 2, None));
    assert_eq!(
        changed(&[Predicate::ConceptIn(c(1))], &current, &proposed),
        ids(&[3])
    );
}

#[test]
fn authority_impact_boolean_branches_can_change_without_effective_change() {
    let mut current = graph();
    current.assertions.push(assertion(1, 3, 1, None));
    let mut proposed = current.clone();
    proposed.assertions[0].predicate_uid = c(3);
    let selector = Predicate::Any(vec![Predicate::ConceptIn(c(1)), Predicate::ConceptIn(c(3))]);
    assert_eq!(changed(&[selector], &current, &proposed), ids(&[]));
}

#[test]
fn authority_impact_any_selector_change_is_not_union_membership_change() {
    let mut current = graph();
    current.assertions.push(assertion(1, 3, 1, None));
    let mut proposed = current.clone();
    proposed.assertions[0].predicate_uid = c(3);
    assert_eq!(
        changed(
            &[Predicate::ConceptIn(c(1)), Predicate::ConceptIn(c(3))],
            &current,
            &proposed
        ),
        ids(&[3])
    );
}

#[test]
fn authority_impact_nested_boolean_not_changes_effective_membership() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.assertions.push(assertion(1, 3, 2, None));
    let selector = Predicate::All(vec![
        Predicate::KindEq(RecordKind::Plain.as_str().into()),
        Predicate::Not(Box::new(Predicate::Any(vec![
            Predicate::ConceptIn(c(1)),
            Predicate::UidEq(r(6)),
        ]))),
    ]);
    assert_eq!(changed(&[selector], &current, &proposed), ids(&[3]));
}

#[test]
fn authority_impact_deletion_and_restoration_change_live_membership() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.records[2].deleted = true;
    assert_eq!(
        changed(&[Predicate::All(Vec::new())], &current, &proposed),
        ids(&[3])
    );
    assert_eq!(
        changed(&[Predicate::All(Vec::new())], &proposed, &current),
        ids(&[3])
    );
    assert_eq!(
        changed(&[Predicate::Any(Vec::new())], &current, &proposed),
        ids(&[])
    );
}

#[test]
fn authority_impact_creation_and_removal_use_complete_union_of_records() {
    let mut current = graph();
    let mut proposed = current.clone();
    current.records.remove(5);
    proposed.records.remove(0);
    assert_eq!(
        changed(&[Predicate::All(Vec::new())], &current, &proposed),
        ids(&[1, 6])
    );
}

#[test]
fn authority_impact_deleted_records_do_not_gain_membership() {
    let mut current = graph();
    current.records[2].deleted = true;
    let mut proposed = current.clone();
    proposed.assertions.push(assertion(1, 3, 1, None));
    assert_eq!(
        changed(&[Predicate::ConceptIn(c(1))], &current, &proposed),
        ids(&[])
    );
}

#[test]
fn authority_impact_kind_and_organ_changes_use_existing_selectors() {
    let mut current = graph();
    current.records[0].kind = RecordKind::Organ;
    current.records[1].kind = RecordKind::Organ;
    current.records[2].organ_uid = Some(r(1));
    let mut proposed = current.clone();
    proposed.records[2].organ_uid = Some(r(2));
    proposed.records[3].kind = RecordKind::Person;
    assert_eq!(
        changed(
            &[
                Predicate::OrganEq(r(1)),
                Predicate::OrganIn(vec![r(2)]),
                Predicate::KindEq(RecordKind::Person.as_str().into()),
            ],
            &current,
            &proposed
        ),
        ids(&[3, 4])
    );
    assert_eq!(
        changed(&[Predicate::OrganIn(vec![r(1), r(2)])], &current, &proposed),
        ids(&[])
    );
}

#[test]
fn authority_impact_empty_selectors_and_unchanged_graphs_have_empty_effects() {
    let current = hierarchy();
    let mut proposed = current.clone();
    proposed.records[0].deleted = true;
    assert_eq!(changed(&[], &current, &proposed), ids(&[]));
    assert_eq!(
        changed(
            &[
                under(false),
                Predicate::ConceptIn(c(4)),
                Predicate::UidEq(r(1))
            ],
            &current,
            &current
        ),
        ids(&[])
    );
}

#[test]
fn authority_impact_matches_independent_read_sets_without_graph_visibility_loss() {
    let current = hierarchy();
    let mut proposed = current.clone();
    proposed.assertions[0].object_uid = Some(r(2));
    proposed.assertions.push(assertion(4, 6, 1, None));
    proposed.records[4].deleted = true;
    let selectors = vec![
        under(false),
        Predicate::ConceptIn(c(1)),
        Predicate::Not(Box::new(Predicate::UidEq(r(6)))),
    ];
    let ceiling = ceiling(&current);
    let mut expected = BTreeSet::new();
    for selector in &selectors {
        let policy = RolePolicy {
            read: selector.clone(),
            grants: Vec::new(),
        };
        let before =
            readable_records(Some(&policy), &current, &ceiling, &Limits::default()).unwrap();
        let after =
            readable_records(Some(&policy), &proposed, &ceiling, &Limits::default()).unwrap();
        expected.extend(before.symmetric_difference(&after).cloned());
    }
    assert_eq!(changed(&selectors, &current, &proposed), expected);
    assert_eq!(expected, ids(&[3, 4, 5, 6]));
}
