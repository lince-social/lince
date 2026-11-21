use std::collections::BTreeSet;

use nucleus::RecordKind;
use protein_authority::{
    authority::{
        AuthorityError, ConceptState, ExtensionProperty, MutationTarget, Operation, Property,
    },
    query::Predicate,
};

use super::fixtures::{
    batch, content, fixture, grant, ordinary, policy, record, record_mut, unary,
};
use crate::fixtures::{a, assertion, c, r, target, uid};

#[test]
fn authority_policy_duplicate_and_canceled_footprints_still_require_the_property() {
    let graph = fixture();
    let mut policy = policy();
    let body = target(3, [Property::Body]);
    for _ in 0..2 {
        let decision = batch(&policy, &graph, &graph, std::slice::from_ref(&body)).unwrap();
        assert_eq!(decision[&r(3)].properties, BTreeSet::from([Property::Body]));
        assert_eq!(
            batch(&policy, &graph, &graph, &[target(3, [Property::Head])]),
            Err(AuthorityError::Denied)
        );
    }
    policy.grants.clear();
    assert_eq!(
        batch(&policy, &graph, &graph, &[body]),
        Err(AuthorityError::Denied)
    );
}

#[test]
fn authority_policy_actual_changes_and_footprints_use_matching_additive_grants() {
    let before = fixture();
    let mut after = before.clone();
    content(&mut after, 3).body = "Changed body".into();
    let mut policy = policy();
    policy
        .grants
        .push(grant(Operation::Update, ordinary(), [Property::Head]));
    let targets = [target(3, [Property::Head])];
    let decision = batch(&policy, &before, &after, &targets).unwrap();
    assert_eq!(
        decision[&r(3)].properties,
        BTreeSet::from([Property::Body, Property::Head])
    );
    assert!(
        batch(
            &policy,
            &before,
            &before,
            &[target(3, [Property::Head, Property::Body])]
        )
        .is_ok()
    );
    policy.grants[1].selector = Predicate::UidEq(r(5));
    assert_eq!(
        batch(&policy, &before, &after, &targets),
        Err(AuthorityError::Denied)
    );
}

#[test]
fn authority_policy_assertion_delta_and_footprint_use_matching_additive_grants() {
    let before = fixture();
    let mut after = before.clone();
    after.assertions.push(assertion(3, 3, 4, None));
    let mut policy = policy();
    let mut separate = grant(Operation::Update, ordinary(), []);
    separate.assertions_add.push(unary(4));
    policy.grants.push(separate);
    let targets = [target(3, [Property::Body])];
    let decision = batch(&policy, &before, &after, &targets).unwrap();
    assert_eq!(decision[&r(3)].assertions_added, BTreeSet::from([a(3)]));
    policy.grants[1].selector = Predicate::UidEq(r(5));
    assert_eq!(
        batch(&policy, &before, &after, &targets),
        Err(AuthorityError::Denied)
    );
}

#[test]
fn authority_policy_batch_allows_distinct_grants_for_distinct_records_only() {
    let before = fixture();
    let mut after = before.clone();
    content(&mut after, 3).body = "Body change".into();
    content(&mut after, 5).head = "Head change".into();
    let mut policy = policy();
    policy.grants[0].selector = Predicate::UidEq(r(3));
    policy.grants.push(grant(
        Operation::Update,
        Predicate::UidEq(r(5)),
        [Property::Head],
    ));
    let targets = [target(3, []), target(5, [])];
    let decisions = batch(&policy, &before, &after, &targets).unwrap();
    assert_eq!(decisions[&r(3)].grant_index, 0);
    assert_eq!(decisions[&r(5)].grant_index, 1);
    content(&mut after, 3).head = "Unauthorized head".into();
    assert_eq!(
        batch(&policy, &before, &after, &targets),
        Err(AuthorityError::Denied)
    );
}

#[test]
fn authority_policy_batch_footprint_does_not_bypass_current_or_proposed_selection() {
    let before = fixture();
    let mut after = before.clone();
    after.assertions.push(assertion(3, 3, 2, None));
    let mut policy = policy();
    policy.read = Predicate::All(Vec::new());
    policy.grants[0].assertions_add.push(unary(2));
    policy.grants.push(grant(
        Operation::Update,
        Predicate::ConceptIn(c(2)),
        [Property::Body],
    ));
    policy.grants[1].assertions_add.push(unary(2));
    assert_eq!(
        batch(&policy, &before, &after, &[target(3, [Property::Body])]),
        Err(AuthorityError::Denied)
    );
    assert_eq!(
        batch(
            &self::policy(),
            &before,
            &before,
            &[target(3, []), target(4, [Property::Body])]
        ),
        Err(AuthorityError::Denied)
    );
}

#[test]
fn authority_policy_batch_rejects_every_kind_of_unlisted_record_write() {
    let before = fixture();
    let mut changed = before.clone();
    content(&mut changed, 5).body = "Unlisted edit".into();
    let mut created = before.clone();
    created.records.push(record(6, RecordKind::Plain));
    let mut deleted = before.clone();
    record_mut(&mut deleted, 5).deleted = true;
    let mut removed = before.clone();
    removed.records.retain(|row| row.uid != r(5));
    for after in [changed, created, deleted, removed] {
        assert_eq!(
            batch(&policy(), &before, &after, &[target(3, [])]),
            Err(AuthorityError::InvalidMutation)
        );
        assert_eq!(
            batch(&policy(), &before, &after, &[]),
            Err(AuthorityError::InvalidMutation)
        );
    }
}

#[test]
fn authority_policy_batch_rejects_unlisted_assertion_add_remove_and_replace() {
    let before = fixture();
    let mut added = before.clone();
    added.assertions.push(assertion(3, 5, 4, None));
    let mut removed = before.clone();
    removed.assertions.retain(|row| row.uid != a(2));
    let mut replaced = before.clone();
    replaced.assertions[1].predicate_uid = c(4);
    for after in [added, removed, replaced] {
        assert_eq!(
            batch(&policy(), &before, &after, &[target(3, [])]),
            Err(AuthorityError::InvalidMutation)
        );
    }
}

#[test]
fn authority_policy_moving_an_assertion_requires_both_subjects_and_both_permissions() {
    let before = fixture();
    let mut after = before.clone();
    after.assertions[0].subject_uid = r(5);
    let mut policy = policy();
    policy.grants[0].assertions_add.push(unary(1));
    policy.grants[0].assertions_remove.push(unary(1));
    for targets in [[target(3, [])], [target(5, [])]] {
        assert_eq!(
            batch(&policy, &before, &after, &targets),
            Err(AuthorityError::InvalidMutation)
        );
    }
    let targets = [target(3, []), target(5, [])];
    let decisions = batch(&policy, &before, &after, &targets).unwrap();
    assert_eq!(decisions[&r(3)].assertions_removed, BTreeSet::from([a(1)]));
    assert_eq!(decisions[&r(5)].assertions_added, BTreeSet::from([a(1)]));
    policy.grants[0].assertions_remove.clear();
    assert_eq!(
        batch(&policy, &before, &after, &targets),
        Err(AuthorityError::Denied)
    );
}

#[test]
fn authority_policy_batch_validates_targets_and_trusted_footprint_shape() {
    let graph = fixture();
    let policy = policy();
    assert_eq!(
        batch(
            &policy,
            &graph,
            &graph,
            &[target(3, []), target(3, [Property::Head])]
        ),
        Err(AuthorityError::InvalidMutation)
    );
    assert_eq!(
        batch(&policy, &graph, &graph, &[target(99, [])]),
        Err(AuthorityError::InvalidMutation)
    );
    assert_eq!(
        batch(
            &policy,
            &graph,
            &graph,
            &[MutationTarget {
                record_uid: "task".into(),
                touched_properties: BTreeSet::new()
            }]
        ),
        Err(AuthorityError::InvalidIdentity)
    );
    assert_eq!(
        batch(
            &policy,
            &graph,
            &graph,
            &[target(
                3,
                [Property::Extension(ExtensionProperty {
                    namespace: String::new(),
                    field: "value".into()
                })]
            )]
        ),
        Err(AuthorityError::InvalidPolicy)
    );
}

#[test]
fn authority_policy_record_batch_does_not_invent_concept_or_place_control_grants() {
    let before = fixture();
    let mut renamed = before.clone();
    renamed.concepts[0].name = "Renamed vocabulary".into();
    let mut reparented = before.clone();
    reparented.concepts[2].parents = BTreeSet::from([c(2)]);
    let mut concept_created = before.clone();
    concept_created.concepts.push(ConceptState {
        uid: c(7),
        name: "New vocabulary".into(),
        parents: BTreeSet::new(),
    });
    let mut place_created = before.clone();
    place_created.places.insert(uid("pl", 1));
    for after in [renamed, reparented, concept_created, place_created] {
        assert_eq!(
            batch(&policy(), &before, &after, &[target(3, [])]),
            Err(AuthorityError::InvalidMutation)
        );
    }
}
