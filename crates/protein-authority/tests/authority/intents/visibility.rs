use std::collections::BTreeSet;

use protein_authority::{
    authority::{
        AssertionRole, AssertionTarget, AuthorityError, Limits, Operation,
        authorize_record_changes_with_assertion_intents,
    },
    query::Predicate,
};

use super::fixtures::{all, assertion, check, graph, intent, policy, record, rule};
use crate::fixtures::{a, c, ceiling, quantity, r, target};

#[test]
fn assertion_intent_initial_removal_uses_current_reference_visibility() {
    let mut before = graph();
    before.assertions[0].object_uid = Some(r(4));
    let mut after = before.clone();
    after.assertions.clear();
    let removal = intent(Some(before.assertions[0].clone()), None, &[]);
    let mut hidden = ceiling(&before);
    hidden.records.remove(&r(4));
    let policy = policy();
    let limits = Limits::default();
    let authorize = |current, proposed| {
        authorize_record_changes_with_assertion_intents(
            Some(&policy),
            &before,
            &after,
            current,
            proposed,
            &[target(3, [])],
            std::slice::from_ref(&removal),
            &limits,
        )
    };
    let current = ceiling(&before);
    let proposed = ceiling(&after);
    assert!(authorize(&current, &hidden).is_ok());
    assert_eq!(authorize(&hidden, &proposed), Err(AuthorityError::Denied));
}

#[test]
fn assertion_intent_introduced_and_intermediate_states_use_proposed_visibility() {
    let mut before = graph();
    before.assertions.clear();
    let mut introduced = assertion(2);
    introduced.object_uid = Some(r(4));
    let intents = [
        intent(None, Some(introduced.clone()), &[]),
        intent(Some(introduced), None, &[]),
    ];
    let mut hidden = ceiling(&before);
    hidden.records.remove(&r(4));
    let policy = policy();
    let limits = Limits::default();
    let visible = ceiling(&before);
    let authorize = |current, proposed| {
        authorize_record_changes_with_assertion_intents(
            Some(&policy),
            &before,
            &before,
            current,
            proposed,
            &[target(3, [])],
            &intents,
            &limits,
        )
    };
    assert!(authorize(&hidden, &visible).is_ok());
    assert_eq!(authorize(&visible, &hidden), Err(AuthorityError::Denied));
}

#[test]
fn assertion_intent_new_cross_record_reference_needs_no_fake_current_record() {
    let mut before = graph();
    before.assertions.clear();
    let mut after = before.clone();
    after.records.push(record(6));
    let mut added = assertion(2);
    added.object_uid = Some(r(6));
    for retained in [false, true] {
        let mut intents = vec![intent(None, Some(added.clone()), &[])];
        if retained {
            after.assertions = vec![added.clone()];
        } else {
            after.assertions.clear();
            intents.push(intent(Some(added.clone()), None, &[]));
        }
        assert!(
            authorize_record_changes_with_assertion_intents(
                Some(&policy()),
                &before,
                &after,
                &ceiling(&before),
                &ceiling(&after),
                &[target(3, []), target(6, [])],
                &intents,
                &Limits::default()
            )
            .is_ok()
        );
    }
}

#[test]
fn assertion_intent_hidden_predicate_or_unit_deny_noops_and_transients() {
    let mut before = graph();
    before.assertions[0].quantity = Some(quantity("1"));
    before.assertions[0].unit_uid = Some(c(2));
    let attempted = intent(
        Some(before.assertions[0].clone()),
        Some(before.assertions[0].clone()),
        &[],
    );
    for concept in [c(1), c(2)] {
        for current_hidden in [false, true] {
            let mut hidden = ceiling(&before);
            hidden.concepts.remove(&concept);
            let visible = ceiling(&before);
            let (current, proposed) = if current_hidden {
                (&hidden, &visible)
            } else {
                (&visible, &hidden)
            };
            assert_eq!(
                authorize_record_changes_with_assertion_intents(
                    Some(&policy()),
                    &before,
                    &before,
                    current,
                    proposed,
                    &[target(3, [])],
                    std::slice::from_ref(&attempted),
                    &Limits::default()
                ),
                Err(AuthorityError::Denied)
            );
        }
    }
}

#[test]
fn assertion_intent_selectors_use_real_graphs_and_validate_every_dependency() {
    let before = graph();
    let mut transient = assertion(2);
    transient.predicate_uid = c(2);
    let intents = [
        intent(None, Some(transient.clone()), &[]),
        intent(Some(transient), None, &[]),
    ];
    let mut allowed = policy();
    allowed.grants[0].selector = Predicate::Not(Box::new(Predicate::ConceptIn(c(2))));
    assert!(check(&allowed, &before, &before, &intents).is_ok());
    allowed.grants[0].selector = Predicate::Any(vec![
        all(),
        Predicate::Not(Box::new(Predicate::UidEq(r(99)))),
    ]);
    assert_eq!(
        check(&allowed, &before, &before, &intents),
        Err(AuthorityError::MissingDependency)
    );
    allowed.grants[0].selector =
        Predicate::Any(vec![all(), Predicate::TextContains("unsupported".into())]);
    assert_eq!(
        check(&allowed, &before, &before, &intents),
        Err(AuthorityError::UnsupportedPredicate)
    );
}

#[test]
fn assertion_intent_new_subject_and_object_use_real_complete_creation_targets() {
    let before = graph();
    let mut after = before.clone();
    after.records.extend([record(6), record(7)]);
    let mut added = assertion(2);
    added.subject_uid = r(6);
    added.object_uid = Some(r(7));
    after.assertions.push(added.clone());
    let decisions = authorize_record_changes_with_assertion_intents(
        Some(&policy()),
        &before,
        &after,
        &ceiling(&before),
        &ceiling(&after),
        &[target(6, []), target(7, [])],
        &[intent(None, Some(added), &[])],
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(decisions[&r(6)].operation, Operation::Create);
    assert_eq!(decisions[&r(6)].assertions_added, BTreeSet::from([a(2)]));
    assert_eq!(decisions[&r(7)].operation, Operation::Create);
}

#[test]
fn uri_assertions_require_the_declared_uri_grant_and_remain_distinct_from_unary_assertions() {
    let before = graph();
    let uri = "https://example.org/Plato.lingua#r_00000000000000000000000009";
    let mut external = assertion(2);
    external.object_uri = Some(uri.into());
    let mut after = before.clone();
    after.assertions.push(external.clone());
    let attempted = intent(None, Some(external), &[]);
    let mut allowed = policy();
    assert_eq!(
        check(&allowed, &before, &after, std::slice::from_ref(&attempted)),
        Err(AuthorityError::Denied)
    );
    allowed.grants[0].assertions_add.push(rule(
        1,
        AssertionRole::Ordinary,
        AssertionTarget::Uri(uri.into()),
    ));
    assert!(check(&allowed, &before, &after, std::slice::from_ref(&attempted)).is_ok());
    allowed.grants[0].assertions_add.last_mut().unwrap().target =
        AssertionTarget::Uri("https://example.org/Another.lingua".into());
    assert_eq!(
        check(&allowed, &before, &after, &[attempted]),
        Err(AuthorityError::Denied)
    );
}
