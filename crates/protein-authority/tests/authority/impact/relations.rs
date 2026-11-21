use protein_authority::{
    authority::{Limits, RolePolicy, VisibilityCeiling, readable_records},
    query::{LinkDirection, Predicate},
};

use super::fixtures::{changed, graph, hierarchy, ids, relation, under};
use crate::fixtures::{assertion, quantity, r};

#[test]
fn authority_impact_relation_out_without_other() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.assertions.push(assertion(1, 3, 2, Some(4)));
    assert_eq!(
        changed(&[relation(LinkDirection::Out, None)], &current, &proposed),
        ids(&[3])
    );
}

#[test]
fn authority_impact_relation_in_without_other_includes_unedited_object() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.assertions.push(assertion(1, 3, 2, Some(4)));
    assert_eq!(
        changed(&[relation(LinkDirection::In, None)], &current, &proposed),
        ids(&[4])
    );
}

#[test]
fn authority_impact_relation_both_without_other() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.assertions.push(assertion(1, 3, 2, Some(4)));
    assert_eq!(
        changed(&[relation(LinkDirection::Both, None)], &current, &proposed),
        ids(&[3, 4])
    );
}

#[test]
fn authority_impact_relation_out_with_other_retarget() {
    let mut current = graph();
    current.assertions.push(assertion(1, 3, 2, Some(4)));
    let mut proposed = current.clone();
    proposed.assertions[0].object_uid = Some(r(5));
    assert_eq!(
        changed(
            &[relation(LinkDirection::Out, Some(4))],
            &current,
            &proposed
        ),
        ids(&[3])
    );
    assert_eq!(
        changed(&[relation(LinkDirection::Out, None)], &current, &proposed),
        ids(&[])
    );
}

#[test]
fn authority_impact_relation_in_with_other_retarget() {
    let mut current = graph();
    current.assertions.push(assertion(1, 3, 2, Some(4)));
    let mut proposed = current.clone();
    proposed.assertions[0].object_uid = Some(r(5));
    assert_eq!(
        changed(&[relation(LinkDirection::In, Some(3))], &current, &proposed),
        ids(&[4, 5])
    );
    assert_eq!(
        changed(&[relation(LinkDirection::In, Some(6))], &current, &proposed),
        ids(&[])
    );
}

#[test]
fn authority_impact_relation_both_with_other_observes_both_ends() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.assertions = vec![assertion(1, 3, 2, Some(4)), assertion(2, 4, 2, Some(5))];
    assert_eq!(
        changed(
            &[relation(LinkDirection::Both, Some(4))],
            &current,
            &proposed
        ),
        ids(&[3, 5])
    );
}

#[test]
fn authority_impact_relation_ignores_unary_and_unrelated_predicates() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.assertions = vec![assertion(1, 3, 2, None), assertion(2, 4, 3, Some(5))];
    assert_eq!(
        changed(&[relation(LinkDirection::Both, None)], &current, &proposed),
        ids(&[])
    );
}

#[test]
fn authority_impact_relation_quantity_change_has_no_selection_effect() {
    let mut current = graph();
    current.assertions.push(assertion(1, 3, 2, Some(4)));
    let mut proposed = current.clone();
    proposed.assertions[0].quantity = Some(quantity("42"));
    assert_eq!(
        changed(&[relation(LinkDirection::Both, None)], &current, &proposed),
        ids(&[])
    );
}

#[test]
fn authority_impact_under_leaf_move_only_changes_leaf() {
    let current = hierarchy();
    let mut proposed = current.clone();
    proposed.assertions[2].object_uid = Some(r(2));
    assert_eq!(changed(&[under(false)], &current, &proposed), ids(&[5]));
}

#[test]
fn authority_impact_under_parent_move_includes_descendants() {
    let current = hierarchy();
    let mut proposed = current.clone();
    proposed.assertions[0].object_uid = Some(r(2));
    assert_eq!(
        changed(&[under(false)], &current, &proposed),
        ids(&[3, 4, 5])
    );
}

#[test]
fn authority_impact_under_move_within_scope_has_no_membership_change() {
    let current = hierarchy();
    let mut proposed = current.clone();
    proposed.assertions[2].object_uid = Some(r(3));
    assert_eq!(
        changed(&[under(false), under(true)], &current, &proposed),
        ids(&[])
    );
}

#[test]
fn authority_impact_under_include_self_respects_root_deletion() {
    let current = hierarchy();
    let mut proposed = current.clone();
    proposed.records[0].deleted = true;
    assert_eq!(changed(&[under(true)], &current, &proposed), ids(&[1]));
    assert_eq!(changed(&[under(false)], &current, &proposed), ids(&[]));
}

#[test]
fn authority_impact_hidden_operands_and_subjects_are_not_viewer_filtered() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.assertions.push(assertion(1, 3, 2, Some(4)));
    let selector = Predicate::Not(Box::new(relation(LinkDirection::In, Some(3))));
    assert_eq!(
        changed(std::slice::from_ref(&selector), &current, &proposed),
        ids(&[4])
    );
    let policy = RolePolicy {
        read: selector,
        grants: Vec::new(),
    };
    let ceiling = VisibilityCeiling {
        records: ids(&[1]),
        ..VisibilityCeiling::default()
    };
    assert_eq!(
        readable_records(Some(&policy), &current, &ceiling, &Limits::default()).unwrap(),
        ids(&[1])
    );
    assert_eq!(
        readable_records(Some(&policy), &proposed, &ceiling, &Limits::default()).unwrap(),
        ids(&[1])
    );
}

#[test]
fn authority_impact_deleted_operand_remains_a_resolved_dependency() {
    let mut current = graph();
    current.records[3].deleted = true;
    let mut proposed = current.clone();
    proposed.assertions.push(assertion(1, 3, 2, Some(4)));
    assert_eq!(
        changed(
            &[relation(LinkDirection::Out, Some(4))],
            &current,
            &proposed
        ),
        ids(&[3])
    );
}

#[test]
fn authority_impact_direct_relation_does_not_invent_general_cycle_ban() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.assertions = vec![assertion(1, 3, 2, Some(4)), assertion(2, 4, 2, Some(3))];
    assert_eq!(
        changed(&[relation(LinkDirection::Both, None)], &current, &proposed),
        ids(&[3, 4])
    );
}
