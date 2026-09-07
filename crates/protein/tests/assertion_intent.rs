use std::collections::{BTreeMap, BTreeSet};

use nucleus::{DecimalValue, RecordKind};
use protein::Predicate;
use protein::authority::{
    AssertionGrant, AssertionIntent, AssertionProperty, AssertionRole, AssertionState,
    AssertionTarget, AuthorityError, ConceptState, GraphSnapshot, Limits, MutationGrant,
    MutationTarget, Operation, Property, RecordContent, RecordDecision, RecordState, RolePolicy,
    VisibilityCeiling, authorize_record_changes, authorize_record_changes_with_assertion_intents,
};

fn uid(prefix: &str, number: u128) -> String {
    format!("{prefix}_{}", nucleus::id::ulid_from(0, number))
}

fn r(number: u128) -> String {
    uid("r", number)
}
fn c(number: u128) -> String {
    uid("c", number)
}
fn a(number: u128) -> String {
    uid("a", number)
}
fn q(value: &str) -> DecimalValue {
    DecimalValue::parse_inferred(value).unwrap()
}
fn all() -> Predicate {
    Predicate::All(Vec::new())
}

fn record(number: u128) -> RecordState {
    RecordState {
        uid: r(number),
        kind: if number == 1 {
            RecordKind::Organ
        } else {
            RecordKind::Plain
        },
        organ_uid: (number != 1).then(|| r(1)),
        deleted: false,
        content: Some(RecordContent {
            slug: None,
            head: "Head".into(),
            body: "Body".into(),
            quantity: q("0"),
            unit_uid: None,
            place_uid: None,
            extensions: BTreeMap::new(),
        }),
    }
}

fn assertion(number: u128) -> AssertionState {
    AssertionState {
        uid: a(number),
        subject_uid: r(3),
        predicate_uid: c(1),
        object_uid: None,
        role: AssertionRole::Ordinary,
        quantity: None,
        unit_uid: None,
    }
}

fn graph() -> GraphSnapshot {
    GraphSnapshot {
        records: (1..=5).map(record).collect(),
        concepts: (1..=3)
            .map(|number| ConceptState {
                uid: c(number),
                name: "Vocabulary".into(),
                parents: BTreeSet::new(),
            })
            .collect(),
        assertions: vec![assertion(1)],
        places: BTreeSet::new(),
    }
}

fn ceiling(graph: &GraphSnapshot) -> VisibilityCeiling {
    VisibilityCeiling {
        records: graph
            .records
            .iter()
            .map(|record| record.uid.clone())
            .collect(),
        concepts: graph
            .concepts
            .iter()
            .map(|concept| concept.uid.clone())
            .collect(),
        places: graph.places.clone(),
    }
}

fn rule(predicate: u128, role: AssertionRole, target: AssertionTarget) -> AssertionGrant {
    AssertionGrant {
        predicate_uid: c(predicate),
        role,
        target,
        properties: if role == AssertionRole::Identity {
            BTreeSet::new()
        } else {
            BTreeSet::from([AssertionProperty::Quantity, AssertionProperty::Unit])
        },
    }
}

fn grant(operation: Operation) -> MutationGrant {
    let rules: Vec<_> = (1..=3)
        .flat_map(|predicate| {
            [
                rule(predicate, AssertionRole::Ordinary, AssertionTarget::Unary),
                rule(
                    predicate,
                    AssertionRole::Ordinary,
                    AssertionTarget::AnyReadableRecord,
                ),
                rule(predicate, AssertionRole::Identity, AssertionTarget::Unary),
            ]
        })
        .collect();
    MutationGrant {
        operation,
        selector: all(),
        properties: BTreeSet::from([
            Property::Kind,
            Property::Head,
            Property::Body,
            Property::Slug,
            Property::Quantity,
            Property::Unit,
            Property::Place,
            Property::Organ,
        ]),
        assertions_add: rules.clone(),
        assertions_remove: rules,
    }
}

fn policy() -> RolePolicy {
    RolePolicy {
        read: all(),
        grants: vec![grant(Operation::Update), grant(Operation::Create)],
    }
}

fn target(number: u128, properties: &[Property]) -> MutationTarget {
    MutationTarget {
        record_uid: r(number),
        touched_properties: properties.iter().cloned().collect(),
    }
}

fn intent(
    before: Option<AssertionState>,
    after: Option<AssertionState>,
    properties: &[AssertionProperty],
) -> AssertionIntent {
    AssertionIntent {
        before,
        after,
        touched_properties: properties.iter().copied().collect(),
    }
}

fn check(
    policy: &RolePolicy,
    before: &GraphSnapshot,
    after: &GraphSnapshot,
    intents: &[AssertionIntent],
) -> Result<BTreeMap<String, RecordDecision>, AuthorityError> {
    authorize_record_changes_with_assertion_intents(
        Some(policy),
        before,
        after,
        &ceiling(before),
        &ceiling(after),
        &[target(3, &[])],
        intents,
        &Limits::default(),
    )
}

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
            &[target(3, &[])],
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
    before.assertions[0].quantity = Some(q("1.25"));
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
    before.assertions[0].quantity = Some(q("3"));
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
    forged.after.as_mut().unwrap().quantity = Some(q("1"));
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
    intermediate.quantity = Some(q("12.50"));
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

#[test]
fn assertion_intent_missing_extra_and_forged_before_or_final_states_refuse() {
    let before = graph();
    let mut after = before.clone();
    after.assertions[0].quantity = Some(q("2"));
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
    forged.before.as_mut().unwrap().quantity = Some(q("3"));
    assert_eq!(
        check(&policy(), &before, &after, &[forged]),
        Err(AuthorityError::InvalidMutation)
    );
}

#[test]
fn assertion_intent_retained_uid_subject_predicate_and_object_are_immutable() {
    let before = graph();
    for field in 0..4 {
        let mut changed = assertion(1);
        match field {
            0 => changed.uid = a(2),
            1 => changed.subject_uid = r(4),
            2 => changed.predicate_uid = c(2),
            _ => changed.object_uid = Some(r(4)),
        }
        let mut after = before.clone();
        after.assertions = vec![changed.clone()];
        let result = authorize_record_changes_with_assertion_intents(
            Some(&policy()),
            &before,
            &after,
            &ceiling(&before),
            &ceiling(&after),
            &[target(3, &[]), target(4, &[])],
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
                inserted.quantity = Some(q("1"));
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
            &[target(4, &[])],
            &[intent(Some(assertion(1)), Some(assertion(1)), &[])],
            &Limits::default()
        ),
        Err(AuthorityError::InvalidMutation)
    );
}

#[test]
fn assertion_intent_initial_removal_uses_current_reference_visibility() {
    let mut before = graph();
    before.assertions[0].object_uid = Some(r(4));
    let mut after = before.clone();
    after.assertions.clear();
    let removal = intent(Some(before.assertions[0].clone()), None, &[]);
    let mut hidden = ceiling(&before);
    hidden.records.remove(&r(4));
    assert!(
        authorize_record_changes_with_assertion_intents(
            Some(&policy()),
            &before,
            &after,
            &ceiling(&before),
            &hidden,
            &[target(3, &[])],
            std::slice::from_ref(&removal),
            &Limits::default()
        )
        .is_ok()
    );
    assert_eq!(
        authorize_record_changes_with_assertion_intents(
            Some(&policy()),
            &before,
            &after,
            &hidden,
            &ceiling(&after),
            &[target(3, &[])],
            &[removal],
            &Limits::default()
        ),
        Err(AuthorityError::Denied)
    );
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
    assert!(
        authorize_record_changes_with_assertion_intents(
            Some(&policy()),
            &before,
            &before,
            &hidden,
            &ceiling(&before),
            &[target(3, &[])],
            &intents,
            &Limits::default()
        )
        .is_ok()
    );
    assert_eq!(
        authorize_record_changes_with_assertion_intents(
            Some(&policy()),
            &before,
            &before,
            &ceiling(&before),
            &hidden,
            &[target(3, &[])],
            &intents,
            &Limits::default()
        ),
        Err(AuthorityError::Denied)
    );
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
                &[target(3, &[]), target(6, &[])],
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
    before.assertions[0].quantity = Some(q("1"));
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
                    &[target(3, &[])],
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
fn assertion_intent_limits_bound_sequence_seen_ids_and_transient_active_map() {
    let before = graph();
    let noop = intent(Some(assertion(1)), Some(assertion(1)), &[]);
    let limits = Limits {
        assertions: 1,
        ..Limits::default()
    };
    assert!(
        authorize_record_changes_with_assertion_intents(
            Some(&policy()),
            &before,
            &before,
            &ceiling(&before),
            &ceiling(&before),
            &[target(3, &[])],
            std::slice::from_ref(&noop),
            &limits
        )
        .is_ok()
    );
    assert_eq!(
        authorize_record_changes_with_assertion_intents(
            Some(&policy()),
            &before,
            &before,
            &ceiling(&before),
            &ceiling(&before),
            &[target(3, &[])],
            &[noop.clone(), noop],
            &limits
        ),
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
            Some(&policy()),
            &empty,
            &empty,
            &ceiling(&empty),
            &ceiling(&empty),
            &[target(3, &[])],
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
                &[target(3, &[])],
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
            &[target(3, &[])],
            &intents,
            &Limits::default()
        ),
        Err(AuthorityError::MissingPolicy)
    );
}

#[test]
fn assertion_intent_budget_is_shared_across_all_transitions_and_targets() {
    let before = graph();
    let intents = vec![
        intent(
            Some(assertion(1)),
            Some(assertion(1)),
            &[AssertionProperty::Quantity]
        );
        32
    ];
    let succeeds = |steps| {
        authorize_record_changes_with_assertion_intents(
            Some(&policy()),
            &before,
            &before,
            &ceiling(&before),
            &ceiling(&before),
            &[target(3, &[])],
            &intents[..1],
            &Limits {
                steps,
                ..Limits::default()
            },
        )
        .is_ok()
    };
    let mut low = 0;
    let mut high = 10_000;
    assert!(succeeds(high));
    while low < high {
        let middle = low + (high - low) / 2;
        if succeeds(middle) {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    let minimum = low;
    assert_eq!(
        authorize_record_changes_with_assertion_intents(
            Some(&policy()),
            &before,
            &before,
            &ceiling(&before),
            &ceiling(&before),
            &[target(3, &[]), target(4, &[])],
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
            &[target(3, &[])],
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
        &[target(6, &[]), target(7, &[])],
        &[intent(None, Some(added), &[])],
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(decisions[&r(6)].operation, Operation::Create);
    assert_eq!(decisions[&r(6)].assertions_added, BTreeSet::from([a(2)]));
    assert_eq!(decisions[&r(7)].operation, Operation::Create);
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
            &[target(3, &[])],
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
            &[target(3, &[])],
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
