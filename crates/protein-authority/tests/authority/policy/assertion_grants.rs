use std::collections::BTreeSet;

use nucleus::RecordKind;
use protein_authority::{
    authority::{
        AssertionGrant, AssertionProperty, AssertionRole, AssertionTarget, AuthorityError, Limits,
        Operation, Property, authorize_record_change,
    },
    query::Predicate,
};

use super::fixtures::{
    batch, change, content, create_grant, fixture, grant, ordinary, policy, read, record, unary,
};
use crate::fixtures::{a, assertion, c, ceiling, quantity, r, target, uid};

#[test]
fn authority_policy_protected_concept_create_add_remove_survive_renames() {
    for name in ["admin", "restricted-research", "green-frogs"] {
        let mut before = fixture();
        before.concepts[1].name = name.into();
        let mut policy = policy();
        policy.grants.push(create_grant());
        policy.grants[0].assertions_add.push(unary(2));
        policy.grants[0].assertions_remove.push(unary(2));
        let mut created = before.clone();
        created.records.push(record(6, RecordKind::Plain));
        created.assertions.push(assertion(3, 6, 2, None));
        assert_eq!(
            change(&policy, &before, &created, 6),
            Err(AuthorityError::Denied)
        );
        let mut added = before.clone();
        added.assertions.push(assertion(3, 3, 2, None));
        assert_eq!(
            change(&policy, &before, &added, 3),
            Err(AuthorityError::Denied)
        );
        let mut removed = before.clone();
        removed
            .assertions
            .retain(|assertion| assertion.subject_uid != r(4));
        assert_eq!(
            change(&policy, &before, &removed, 4),
            Err(AuthorityError::Denied)
        );
        assert_eq!(read(&policy, &before, 4), Ok(false));
    }
}

#[test]
fn authority_policy_assertion_grant_cannot_be_borrowed_from_another_record_set() {
    let before = fixture();
    let mut after = before.clone();
    after.assertions.push(assertion(3, 3, 6, None));
    content(&mut after, 3).body = "Changed".into();
    let mut policy = policy();
    let mut other = grant(Operation::Update, Predicate::UidEq(r(5)), [Property::Body]);
    other.assertions_add.push(unary(6));
    policy.grants.push(other);
    assert_eq!(
        change(&policy, &before, &after, 3),
        Err(AuthorityError::Denied)
    );
    policy.grants[0].assertions_add.push(unary(6));
    assert!(change(&policy, &before, &after, 3).is_ok());
}

#[test]
fn authority_policy_assertions_require_exact_target_role_and_quantity_permissions() {
    let before = fixture();
    let mut after = before.clone();
    after.assertions.push(assertion(3, 3, 4, Some(2)));
    let mut policy = policy();
    policy.grants[0].assertions_add.push(AssertionGrant {
        predicate_uid: c(4),
        target: AssertionTarget::Record(r(5)),
        role: AssertionRole::Ordinary,
        properties: BTreeSet::new(),
    });
    assert_eq!(
        change(&policy, &before, &after, 3),
        Err(AuthorityError::Denied)
    );
    policy.grants[0].assertions_add[0].target = AssertionTarget::Record(r(2));
    assert!(change(&policy, &before, &after, 3).is_ok());
    after.assertions.last_mut().unwrap().quantity = Some(quantity("4"));
    assert_eq!(
        change(&policy, &before, &after, 3),
        Err(AuthorityError::Denied)
    );
    policy.grants[0].assertions_add[0]
        .properties
        .insert(AssertionProperty::Quantity);
    assert!(change(&policy, &before, &after, 3).is_ok());
    after.assertions.last_mut().unwrap().unit_uid = Some(c(5));
    assert_eq!(
        change(&policy, &before, &after, 3),
        Err(AuthorityError::Denied)
    );
    policy.grants[0].assertions_add[0]
        .properties
        .insert(AssertionProperty::Unit);
    assert!(change(&policy, &before, &after, 3).is_ok());
}

#[test]
fn authority_policy_identity_assertion_is_not_ordinary_classification() {
    let before = fixture();
    let mut after = before.clone();
    let mut identity = assertion(3, 3, 6, None);
    identity.role = AssertionRole::Identity;
    after.assertions.push(identity);
    let mut policy = policy();
    policy.grants[0].assertions_add.push(unary(6));
    assert_eq!(
        change(&policy, &before, &after, 3),
        Err(AuthorityError::Denied)
    );
    policy.grants[0].assertions_add[0].role = AssertionRole::Identity;
    assert!(change(&policy, &before, &after, 3).is_ok());
}

#[test]
fn authority_policy_changing_existing_assertion_needs_remove_and_add() {
    let mut before = fixture();
    before.assertions.push(assertion(3, 3, 4, Some(2)));
    let mut after = before.clone();
    after.assertions.last_mut().unwrap().object_uid = Some(r(5));
    let mut policy = policy();
    let permission = AssertionGrant {
        predicate_uid: c(4),
        target: AssertionTarget::AnyReadableRecord,
        role: AssertionRole::Ordinary,
        properties: BTreeSet::new(),
    };
    policy.grants[0].assertions_add.push(permission.clone());
    assert_eq!(
        change(&policy, &before, &after, 3),
        Err(AuthorityError::Denied)
    );
    policy.grants[0].assertions_remove.push(permission);
    let decision = change(&policy, &before, &after, 3).unwrap();
    assert_eq!(decision.assertions_added, BTreeSet::from([a(3)]));
    assert_eq!(decision.assertions_removed, BTreeSet::from([a(3)]));
}

#[test]
fn authority_policy_cross_link_does_not_grant_access_to_hidden_target() {
    let before = fixture();
    let mut after = before.clone();
    after.assertions.push(assertion(3, 3, 4, Some(4)));
    let mut policy = policy();
    policy.grants[0].assertions_add.push(AssertionGrant {
        predicate_uid: c(4),
        target: AssertionTarget::AnyReadableRecord,
        role: AssertionRole::Ordinary,
        properties: BTreeSet::new(),
    });
    assert_eq!(
        change(&policy, &before, &after, 3),
        Err(AuthorityError::Denied)
    );
    assert_eq!(read(&policy, &after, 4), Ok(false));
}

#[test]
fn authority_policy_units_and_places_use_their_real_identity_and_reference_ceiling() {
    let mut before = fixture();
    let place_uid = uid("pl", 1);
    before.places.insert(place_uid.clone());
    let mut after = before.clone();
    content(&mut after, 3).unit_uid = Some(c(5));
    content(&mut after, 3).place_uid = Some(place_uid.clone());
    let mut policy = policy();
    assert_eq!(
        change(&policy, &before, &after, 3),
        Err(AuthorityError::Denied)
    );
    policy.grants[0]
        .properties
        .extend([Property::Unit, Property::Place]);
    assert!(change(&policy, &before, &after, 3).is_ok());
    let mut denied_unit = ceiling(&after);
    denied_unit.concepts.remove(&c(5));
    let mut denied_place = ceiling(&after);
    denied_place.places.remove(&place_uid);
    for proposed_ceiling in [denied_unit, denied_place] {
        assert_eq!(
            authorize_record_change(
                Some(&policy),
                &before,
                &after,
                &ceiling(&before),
                &proposed_ceiling,
                &r(3),
                &Limits::default()
            ),
            Err(AuthorityError::Denied)
        );
    }
    content(&mut after, 3).unit_uid = Some(r(5));
    assert_eq!(
        change(&policy, &before, &after, 3),
        Err(AuthorityError::InvalidIdentity)
    );
}

#[test]
fn authority_policy_assertion_predicate_reference_ceiling_is_required() {
    let before = fixture();
    let mut after = before.clone();
    after.assertions.push(assertion(3, 3, 6, None));
    let mut policy = policy();
    policy.grants[0].assertions_add.push(unary(6));
    let mut proposed_ceiling = ceiling(&after);
    proposed_ceiling.concepts.remove(&c(6));
    assert_eq!(
        authorize_record_change(
            Some(&policy),
            &before,
            &after,
            &ceiling(&before),
            &proposed_ceiling,
            &r(3),
            &Limits::default()
        ),
        Err(AuthorityError::Denied)
    );
    assert!(change(&policy, &before, &after, 3).is_ok());
}

#[test]
fn authority_policy_hidden_dependency_metadata_does_not_authorize_a_write_reference() {
    let before = fixture();
    let mut after = before.clone();
    after.assertions.push(assertion(3, 3, 4, Some(2)));
    let mut policy = policy();
    policy.grants[0].assertions_add.push(AssertionGrant {
        predicate_uid: c(4),
        target: AssertionTarget::Record(r(2)),
        role: AssertionRole::Ordinary,
        properties: BTreeSet::new(),
    });
    let mut proposed_ceiling = ceiling(&after);
    proposed_ceiling.records.remove(&r(2));
    assert_eq!(
        authorize_record_change(
            Some(&policy),
            &before,
            &after,
            &ceiling(&before),
            &proposed_ceiling,
            &r(3),
            &Limits::default()
        ),
        Err(AuthorityError::Denied)
    );
}

#[test]
fn matching_assertion_grants_union_properties_without_borrowing_other_targets() {
    let before = fixture();
    let mut after = before.clone();
    let mut added = assertion(3, 3, 3, None);
    added.quantity = Some(quantity("2"));
    added.unit_uid = Some(c(4));
    after.assertions.push(added);
    let mut policy = policy();
    let mut quantity_grant = unary(3);
    quantity_grant
        .properties
        .insert(AssertionProperty::Quantity);
    let mut unit_grant = unary(3);
    unit_grant.properties.insert(AssertionProperty::Unit);
    policy.grants[0].assertions_add.push(quantity_grant);
    let mut second = grant(Operation::Update, ordinary(), []);
    second.assertions_add.push(unit_grant);
    policy.grants.push(second);
    assert!(batch(&policy, &before, &after, &[target(3, [])]).is_ok());
    policy.grants[1].assertions_add[0].target = AssertionTarget::Record(r(5));
    assert_eq!(
        batch(&policy, &before, &after, &[target(3, [])]),
        Err(AuthorityError::Denied)
    );
    policy.grants[1].assertions_add[0].target = AssertionTarget::Unary;
    policy.grants[1].selector = Predicate::UidEq(r(5));
    assert_eq!(
        batch(&policy, &before, &after, &[target(3, [])]),
        Err(AuthorityError::Denied)
    );
}
