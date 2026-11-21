use protein_authority::authority::{
    AssertionProperty, AssertionRole, AssertionState, AuthorityError, Limits,
    authorize_record_changes, authorize_record_changes_with_assertion_intents,
};

use super::fixtures::{assertion, check, graph, intent, policy};
use crate::fixtures::{a, c, ceiling, quantity, r, target};

#[test]
fn assertion_intent_empty_checked_sequence_requires_exact_actual_coverage() {
    let before = graph();
    assert!(check(&policy(), &before, &before, &[]).is_ok());
    let mut after = before.clone();
    after.assertions.clear();
    assert_eq!(
        check(&policy(), &before, &after, &[]),
        Err(AuthorityError::InvalidMutation)
    );
    assert!(
        authorize_record_changes(
            Some(&policy()),
            &before,
            &after,
            &ceiling(&before),
            &ceiling(&after),
            &[target(3, [])],
            &Limits::default()
        )
        .is_ok()
    );
    after.assertions = vec![assertion(2)];
    assert_eq!(
        check(&policy(), &before, &after, &[]),
        Err(AuthorityError::InvalidMutation)
    );
}

#[test]
fn assertion_intent_missing_extra_and_forged_before_or_final_states_refuse() {
    let before = graph();
    let mut after = before.clone();
    after.assertions[0].quantity = Some(quantity("2"));
    let correct = intent(
        Some(assertion(1)),
        Some(after.assertions[0].clone()),
        &[AssertionProperty::Quantity],
    );
    assert!(check(&policy(), &before, &after, std::slice::from_ref(&correct)).is_ok());
    for attempted in [
        intent(None, None, &[]),
        intent(None, Some(after.assertions[0].clone()), &[]),
        intent(Some(assertion(2)), Some(after.assertions[0].clone()), &[]),
        intent(Some(assertion(1)), None, &[]),
        intent(Some(assertion(1)), Some(assertion(1)), &[]),
    ] {
        assert_eq!(
            check(&policy(), &before, &after, &[attempted]),
            Err(AuthorityError::InvalidMutation)
        );
    }
    let mut forged = correct;
    forged.before.as_mut().unwrap().quantity = Some(quantity("3"));
    assert_eq!(
        check(&policy(), &before, &after, &[forged]),
        Err(AuthorityError::InvalidMutation)
    );
}

#[test]
fn assertion_intent_retained_uid_subject_predicate_and_object_are_immutable() {
    let before = graph();
    let original = assertion(1);
    for changed in [
        AssertionState {
            uid: a(2),
            ..original.clone()
        },
        AssertionState {
            subject_uid: r(4),
            ..original.clone()
        },
        AssertionState {
            predicate_uid: c(2),
            ..original.clone()
        },
        AssertionState {
            object_uid: Some(r(4)),
            ..original
        },
    ] {
        let mut after = before.clone();
        after.assertions = vec![changed.clone()];
        let result = authorize_record_changes_with_assertion_intents(
            Some(&policy()),
            &before,
            &after,
            &ceiling(&before),
            &ceiling(&after),
            &[target(3, []), target(4, [])],
            &[intent(Some(assertion(1)), Some(changed), &[])],
            &Limits::default(),
        );
        assert_eq!(result, Err(AuthorityError::InvalidMutation));
    }
}

#[test]
fn assertion_intent_retracted_uid_cannot_be_reinserted_even_with_same_identity() {
    let before = graph();
    for predicate in [c(1), c(2)] {
        let mut replacement = assertion(1);
        replacement.predicate_uid = predicate;
        let mut after = before.clone();
        after.assertions = vec![replacement.clone()];
        assert_eq!(
            check(
                &policy(),
                &before,
                &after,
                &[
                    intent(Some(assertion(1)), None, &[]),
                    intent(None, Some(replacement), &[])
                ]
            ),
            Err(AuthorityError::InvalidMutation)
        );
    }
}

#[test]
fn assertion_intent_ordered_retract_then_new_uid_replacement_is_supported() {
    let before = graph();
    let mut after = before.clone();
    after.assertions = vec![assertion(2)];
    let intents = [
        intent(Some(assertion(1)), None, &[]),
        intent(None, Some(assertion(2)), &[]),
    ];
    assert!(check(&policy(), &before, &after, &intents).is_ok());
    let mut reverse = intents;
    reverse.reverse();
    assert_eq!(
        check(&policy(), &before, &after, &reverse),
        Err(AuthorityError::InvalidMutation)
    );
}

#[test]
fn assertion_intent_transient_duplicate_tuple_and_identity_are_refused() {
    let before = graph();
    assert_eq!(
        check(
            &policy(),
            &before,
            &before,
            &[
                intent(None, Some(assertion(2)), &[]),
                intent(Some(assertion(2)), None, &[])
            ]
        ),
        Err(AuthorityError::InvalidMutation)
    );
    let mut before = graph();
    before.assertions[0].role = AssertionRole::Identity;
    let mut second = assertion(2);
    second.predicate_uid = c(2);
    second.role = AssertionRole::Identity;
    assert_eq!(
        check(
            &policy(),
            &before,
            &before,
            &[
                intent(None, Some(second.clone()), &[]),
                intent(Some(second), None, &[])
            ]
        ),
        Err(AuthorityError::InvalidMutation)
    );
}

#[test]
fn assertion_intent_malformed_and_missing_transient_dependencies_fail_before_allow() {
    let mut before = graph();
    before.assertions.clear();
    for field in 0..7 {
        let mut inserted = assertion(2);
        match field {
            0 => inserted.uid = "a_bad".into(),
            1 => inserted.predicate_uid = c(99),
            2 => inserted.object_uid = Some(r(99)),
            3 => inserted.unit_uid = Some(c(99)),
            4 => {
                inserted.role = AssertionRole::Identity;
                inserted.quantity = Some(quantity("1"));
            }
            5 => {
                inserted.role = AssertionRole::Identity;
                inserted.object_uid = Some(r(4));
            }
            _ => {
                inserted.role = AssertionRole::Identity;
                inserted.unit_uid = Some(c(2));
            }
        }
        assert!(
            check(
                &policy(),
                &before,
                &before,
                &[
                    intent(None, Some(inserted.clone()), &[]),
                    intent(Some(inserted), None, &[])
                ]
            )
            .is_err()
        );
    }
}

#[test]
fn assertion_intent_noop_subjects_cannot_be_omitted_from_declared_targets() {
    let before = graph();
    assert_eq!(
        authorize_record_changes_with_assertion_intents(
            Some(&policy()),
            &before,
            &before,
            &ceiling(&before),
            &ceiling(&before),
            &[target(4, [])],
            &[intent(Some(assertion(1)), Some(assertion(1)), &[])],
            &Limits::default()
        ),
        Err(AuthorityError::InvalidMutation)
    );
}
