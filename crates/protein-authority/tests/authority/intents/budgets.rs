use protein_authority::{
    authority::{
        AssertionIntent, AssertionProperty, AuthorityError, Limits,
        authorize_record_changes_with_assertion_intents,
    },
    query::Predicate,
};

use super::fixtures::{all, assertion, check, graph, intent, policy};
use crate::fixtures::{c, ceiling, minimum_steps, r, target, uid};

#[test]
fn assertion_intent_limits_bound_sequence_seen_ids_and_transient_active_map() {
    let before = graph();
    let noop = intent(Some(assertion(1)), Some(assertion(1)), &[]);
    let limits = Limits {
        assertions: 1,
        ..Limits::default()
    };
    let policy = policy();
    let visible = ceiling(&before);
    let authorize = |intents: &[AssertionIntent]| {
        authorize_record_changes_with_assertion_intents(
            Some(&policy),
            &before,
            &before,
            &visible,
            &visible,
            &[target(3, [])],
            intents,
            &limits,
        )
    };
    assert!(authorize(std::slice::from_ref(&noop)).is_ok());
    assert_eq!(
        authorize(&[noop.clone(), noop]),
        Err(AuthorityError::LimitExceeded)
    );
    let mut empty = graph();
    empty.assertions.clear();
    let mut second = assertion(2);
    second.predicate_uid = c(2);
    let intents = [
        intent(None, Some(assertion(1)), &[]),
        intent(None, Some(second.clone()), &[]),
        intent(Some(assertion(1)), None, &[]),
        intent(Some(second), None, &[]),
    ];
    assert_eq!(
        authorize_record_changes_with_assertion_intents(
            Some(&policy),
            &empty,
            &empty,
            &ceiling(&empty),
            &ceiling(&empty),
            &[target(3, [])],
            &intents,
            &Limits {
                assertions: 3,
                ..Limits::default()
            }
        ),
        Err(AuthorityError::LimitExceeded)
    );
}

#[test]
fn assertion_intent_every_shared_limit_refuses_without_partial_decisions() {
    let before = graph();
    let intents = [intent(
        Some(assertion(1)),
        Some(assertion(1)),
        &[AssertionProperty::Quantity, AssertionProperty::Unit],
    )];
    for limits in [
        Limits {
            records: 0,
            ..Limits::default()
        },
        Limits {
            concepts: 0,
            ..Limits::default()
        },
        Limits {
            assertions: 0,
            ..Limits::default()
        },
        Limits {
            grants: 0,
            ..Limits::default()
        },
        Limits {
            predicate_nodes: 0,
            ..Limits::default()
        },
        Limits {
            graph_edges: 0,
            ..Limits::default()
        },
        Limits {
            steps: 0,
            ..Limits::default()
        },
        Limits {
            bytes: 0,
            ..Limits::default()
        },
    ] {
        assert_eq!(
            authorize_record_changes_with_assertion_intents(
                Some(&policy()),
                &before,
                &before,
                &ceiling(&before),
                &ceiling(&before),
                &[target(3, [])],
                &intents,
                &limits
            ),
            Err(AuthorityError::LimitExceeded)
        );
    }
    assert_eq!(
        authorize_record_changes_with_assertion_intents(
            None,
            &before,
            &before,
            &ceiling(&before),
            &ceiling(&before),
            &[target(3, [])],
            &intents,
            &Limits::default()
        ),
        Err(AuthorityError::MissingPolicy)
    );
}

#[test]
fn assertion_intent_budget_is_shared_across_all_transitions_and_targets() {
    let before = graph();
    let noop = intent(
        Some(assertion(1)),
        Some(assertion(1)),
        &[AssertionProperty::Quantity],
    );
    let intents = vec![noop.clone(); 32];
    let minimum = minimum_steps(10_000, |steps| {
        authorize_record_changes_with_assertion_intents(
            Some(&policy()),
            &before,
            &before,
            &ceiling(&before),
            &ceiling(&before),
            &[target(3, [])],
            std::slice::from_ref(&noop),
            &Limits {
                steps,
                ..Limits::default()
            },
        )
        .map(|_| ())
    });
    assert_eq!(
        authorize_record_changes_with_assertion_intents(
            Some(&policy()),
            &before,
            &before,
            &ceiling(&before),
            &ceiling(&before),
            &[target(3, []), target(4, [])],
            &intents,
            &Limits {
                steps: minimum,
                ..Limits::default()
            }
        ),
        Err(AuthorityError::LimitExceeded)
    );
    assert!(check(&policy(), &before, &before, &intents).is_ok());
}

#[test]
fn assertion_intent_distinct_uid_history_is_bounded_even_after_retraction() {
    let mut before = graph();
    for number in [2, 3] {
        let mut extra = assertion(number);
        extra.subject_uid = r(number + 2);
        before.assertions.push(extra);
    }
    let mut replacement = assertion(4);
    replacement.predicate_uid = c(2);
    let mut after = before.clone();
    after.assertions[0] = replacement.clone();
    let intents = [
        intent(Some(assertion(1)), None, &[]),
        intent(None, Some(replacement), &[]),
    ];
    assert_eq!(
        authorize_record_changes_with_assertion_intents(
            Some(&policy()),
            &before,
            &after,
            &ceiling(&before),
            &ceiling(&after),
            &[target(3, [])],
            &intents,
            &Limits {
                assertions: 3,
                ..Limits::default()
            }
        ),
        Err(AuthorityError::LimitExceeded)
    );
    assert!(check(&policy(), &before, &after, &intents).is_ok());
}

#[test]
fn assertion_intent_place_depth_and_cycle_limits_remain_fail_closed() {
    let mut before = graph();
    before.places.insert(uid("pl", 1));
    assert_eq!(
        authorize_record_changes_with_assertion_intents(
            Some(&policy()),
            &before,
            &before,
            &ceiling(&before),
            &ceiling(&before),
            &[target(3, [])],
            &[],
            &Limits {
                places: 0,
                ..Limits::default()
            }
        ),
        Err(AuthorityError::LimitExceeded)
    );
    let mut nested = policy();
    nested.read = Predicate::All(vec![all()]);
    assert_eq!(
        authorize_record_changes_with_assertion_intents(
            Some(&nested),
            &before,
            &before,
            &ceiling(&before),
            &ceiling(&before),
            &[target(3, [])],
            &[],
            &Limits {
                predicate_depth: 0,
                ..Limits::default()
            }
        ),
        Err(AuthorityError::LimitExceeded)
    );
    before.assertions[0].object_uid = Some(r(4));
    let mut reverse = assertion(2);
    reverse.subject_uid = r(4);
    reverse.object_uid = Some(r(3));
    before.assertions.push(reverse);
    nested = policy();
    nested.grants[0].selector = Predicate::Under {
        record: r(3),
        kind: c(1),
        include_self: true,
    };
    assert_eq!(
        check(&nested, &before, &before, &[]),
        Err(AuthorityError::CyclicGraph)
    );
}
