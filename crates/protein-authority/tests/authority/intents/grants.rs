use std::collections::BTreeSet;

use protein_authority::authority::{
    AssertionProperty, AssertionRole, AssertionTarget, AuthorityError, Operation, RolePolicy,
};

use super::fixtures::{all, assertion, check, grant, graph, intent, policy, rule};
use crate::fixtures::{a, c, quantity, r};

#[test]
fn assertion_intent_noop_clear_at_none_requires_both_properties_on_both_sides() {
    let before = graph();
    let attempted = intent(
        Some(assertion(1)),
        Some(assertion(1)),
        &[AssertionProperty::Quantity, AssertionProperty::Unit],
    );
    let decision = check(
        &policy(),
        &before,
        &before,
        std::slice::from_ref(&attempted),
    )
    .unwrap();
    assert!(decision[&r(3)].assertions_added.is_empty());
    assert!(decision[&r(3)].assertions_removed.is_empty());
    for side in [false, true] {
        for property in [AssertionProperty::Quantity, AssertionProperty::Unit] {
            let mut restricted = policy();
            let rules = if side {
                &mut restricted.grants[0].assertions_add
            } else {
                &mut restricted.grants[0].assertions_remove
            };
            for rule in rules {
                rule.properties.remove(&property);
            }
            assert_eq!(
                check(
                    &restricted,
                    &before,
                    &before,
                    std::slice::from_ref(&attempted)
                ),
                Err(AuthorityError::Denied)
            );
        }
    }
}

#[test]
fn assertion_intent_unchanged_present_quantity_and_unit_remain_protected() {
    let mut before = graph();
    before.assertions[0].quantity = Some(quantity("1.25"));
    before.assertions[0].unit_uid = Some(c(2));
    let attempted = intent(
        Some(before.assertions[0].clone()),
        Some(before.assertions[0].clone()),
        &[AssertionProperty::Quantity, AssertionProperty::Unit],
    );
    assert!(
        check(
            &policy(),
            &before,
            &before,
            std::slice::from_ref(&attempted)
        )
        .is_ok()
    );
    let mut restricted = policy();
    restricted.grants[0].assertions_add.clear();
    assert_eq!(
        check(&restricted, &before, &before, &[attempted]),
        Err(AuthorityError::Denied)
    );
}

#[test]
fn assertion_intent_noop_identity_promotion_requires_identity_remove_and_add() {
    let mut before = graph();
    before.assertions[0].role = AssertionRole::Identity;
    let attempted = intent(
        Some(before.assertions[0].clone()),
        Some(before.assertions[0].clone()),
        &[],
    );
    assert!(
        check(
            &policy(),
            &before,
            &before,
            std::slice::from_ref(&attempted)
        )
        .is_ok()
    );
    for add in [false, true] {
        let mut restricted = policy();
        let rules = if add {
            &mut restricted.grants[0].assertions_add
        } else {
            &mut restricted.grants[0].assertions_remove
        };
        rules.retain(|rule| rule.role != AssertionRole::Identity);
        assert_eq!(
            check(
                &restricted,
                &before,
                &before,
                std::slice::from_ref(&attempted)
            ),
            Err(AuthorityError::Denied)
        );
    }
}

#[test]
fn assertion_intent_identity_promotion_reconciles_quantity_and_unit_clearing() {
    let mut before = graph();
    before.assertions[0].quantity = Some(quantity("3"));
    before.assertions[0].unit_uid = Some(c(2));
    let mut after = before.clone();
    after.assertions[0].role = AssertionRole::Identity;
    after.assertions[0].quantity = None;
    after.assertions[0].unit_uid = None;
    let attempted = intent(
        Some(before.assertions[0].clone()),
        Some(after.assertions[0].clone()),
        &[AssertionProperty::Quantity, AssertionProperty::Unit],
    );
    let decision = check(&policy(), &before, &after, std::slice::from_ref(&attempted)).unwrap();
    assert_eq!(decision[&r(3)].assertions_added, BTreeSet::from([a(1)]));
    assert_eq!(decision[&r(3)].assertions_removed, BTreeSet::from([a(1)]));
    let mut forged = attempted;
    forged.after.as_mut().unwrap().quantity = Some(quantity("1"));
    assert_eq!(
        check(&policy(), &before, &after, &[forged]),
        Err(AuthorityError::InvalidMutation)
    );
}

#[test]
fn assertion_intent_insert_then_retract_retains_both_permissions_without_final_diff() {
    let mut before = graph();
    before.assertions.clear();
    let inserted = assertion(2);
    let intents = [
        intent(None, Some(inserted.clone()), &[]),
        intent(Some(inserted), None, &[]),
    ];
    let decision = check(&policy(), &before, &before, &intents).unwrap();
    assert!(decision[&r(3)].assertions_added.is_empty());
    assert!(decision[&r(3)].assertions_removed.is_empty());
    for add in [false, true] {
        let mut restricted = policy();
        if add {
            restricted.grants[0].assertions_add.clear();
        } else {
            restricted.grants[0].assertions_remove.clear();
        }
        assert_eq!(
            check(&restricted, &before, &before, &intents),
            Err(AuthorityError::Denied)
        );
    }
    let mut reversed = intents;
    reversed.reverse();
    assert_eq!(
        check(&policy(), &before, &before, &reversed),
        Err(AuthorityError::InvalidMutation)
    );
}

#[test]
fn assertion_intent_quantity_round_trip_keeps_intermediate_attempts() {
    let before = graph();
    let mut intermediate = assertion(1);
    intermediate.quantity = Some(quantity("12.50"));
    let intents = [
        intent(
            Some(assertion(1)),
            Some(intermediate.clone()),
            &[AssertionProperty::Quantity],
        ),
        intent(
            Some(intermediate),
            Some(assertion(1)),
            &[AssertionProperty::Quantity],
        ),
    ];
    assert!(check(&policy(), &before, &before, &intents).is_ok());
    let mut restricted = policy();
    for rule in &mut restricted.grants[0].assertions_add {
        rule.properties.clear();
    }
    assert_eq!(
        check(&restricted, &before, &before, &intents),
        Err(AuthorityError::Denied)
    );
    let mut forged = intents;
    forged[1].before = Some(assertion(1));
    assert_eq!(
        check(&policy(), &before, &before, &forged),
        Err(AuthorityError::InvalidMutation)
    );
}

#[test]
fn assertion_intent_same_record_grant_covers_scalar_actual_and_noop_assertion() {
    let before = graph();
    let mut after = before.clone();
    after.records[2].content.as_mut().unwrap().body = "Changed".into();
    let attempted = intent(
        Some(assertion(1)),
        Some(assertion(1)),
        &[AssertionProperty::Unit],
    );
    let mut scalar = grant(Operation::Update);
    scalar.assertions_add.clear();
    scalar.assertions_remove.clear();
    let mut assertion_only = grant(Operation::Update);
    assertion_only.properties.clear();
    let mut split = RolePolicy {
        read: all(),
        grants: vec![scalar, assertion_only],
    };
    assert_eq!(
        check(&split, &before, &after, std::slice::from_ref(&attempted)),
        Err(AuthorityError::Denied)
    );
    split.grants.push(grant(Operation::Update));
    assert_eq!(
        check(&split, &before, &after, &[attempted]).unwrap()[&r(3)].grant_index,
        2
    );
}

#[test]
fn assertion_intent_cannot_borrow_between_record_grants_or_assertion_rules() {
    let before = graph();
    let attempted = intent(
        Some(assertion(1)),
        Some(assertion(1)),
        &[AssertionProperty::Quantity, AssertionProperty::Unit],
    );
    let mut left = grant(Operation::Update);
    left.assertions_remove.clear();
    let mut right = grant(Operation::Update);
    right.assertions_add.clear();
    let split = RolePolicy {
        read: all(),
        grants: vec![left, right],
    };
    assert_eq!(
        check(&split, &before, &before, std::slice::from_ref(&attempted)),
        Err(AuthorityError::Denied)
    );
    let mut split = policy();
    let mut quantity = rule(1, AssertionRole::Ordinary, AssertionTarget::Unary);
    quantity.properties = BTreeSet::from([AssertionProperty::Quantity]);
    let mut unit = quantity.clone();
    unit.properties = BTreeSet::from([AssertionProperty::Unit]);
    split.grants[0].assertions_add = vec![quantity, unit];
    assert_eq!(
        check(&split, &before, &before, &[attempted]),
        Err(AuthorityError::Denied)
    );
}

#[test]
fn assertion_intent_all_transitions_require_one_grant_not_one_each() {
    let mut before = graph();
    let mut second = assertion(2);
    second.predicate_uid = c(2);
    before.assertions.push(second.clone());
    let intents = [
        intent(Some(assertion(1)), Some(assertion(1)), &[]),
        intent(Some(second.clone()), Some(second), &[]),
    ];
    let mut first = grant(Operation::Update);
    first
        .assertions_add
        .retain(|rule| rule.predicate_uid == c(1));
    first
        .assertions_remove
        .retain(|rule| rule.predicate_uid == c(1));
    let mut second = grant(Operation::Update);
    second
        .assertions_add
        .retain(|rule| rule.predicate_uid == c(2));
    second
        .assertions_remove
        .retain(|rule| rule.predicate_uid == c(2));
    assert_eq!(
        check(
            &RolePolicy {
                read: all(),
                grants: vec![first, second]
            },
            &before,
            &before,
            &intents
        ),
        Err(AuthorityError::Denied)
    );
}
