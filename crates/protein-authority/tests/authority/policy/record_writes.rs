use std::collections::BTreeSet;

use nucleus::RecordKind;
use protein_authority::{
    authority::{
        AuthorityError, ExtensionProperty, Limits, Operation, Property, RolePolicy,
        authorize_record_change, may_read,
    },
    query::Predicate,
};
use serde_json::json;

use super::fixtures::{
    change, content, create_grant, fixture, grant, ordinary, policy, read, record, record_mut,
    unary,
};
use crate::fixtures::{a, assertion, c, ceiling, quantity, r};

#[test]
fn authority_policy_body_edit_uses_one_grant_and_read_is_separate() {
    let before = fixture();
    let mut after = before.clone();
    content(&mut after, 3).body = "New body".into();
    let policy = policy();
    assert_eq!(read(&policy, &before, 3), Ok(true));
    assert_eq!(read(&policy, &before, 4), Ok(false));
    let decision = change(&policy, &before, &after, 3).unwrap();
    assert_eq!(decision.operation, Operation::Update);
    assert_eq!(decision.properties, BTreeSet::from([Property::Body]));
    assert_eq!(decision.grant_index, 0);
    let readonly = RolePolicy {
        read: policy.read,
        grants: Vec::new(),
    };
    assert_eq!(
        change(&readonly, &before, &after, 3),
        Err(AuthorityError::Denied)
    );
}

#[test]
fn authority_policy_refuses_head_quantity_and_undeclared_assertions() {
    let before = fixture();
    let mut head = before.clone();
    content(&mut head, 3).head = "Changed head".into();
    let mut quantity_change = before.clone();
    content(&mut quantity_change, 3).quantity = quantity("2.50");
    let mut assertion_change = before.clone();
    assertion_change.assertions.push(assertion(3, 3, 6, None));
    for after in [head, quantity_change, assertion_change] {
        assert_eq!(
            change(&policy(), &before, &after, 3),
            Err(AuthorityError::Denied)
        );
    }
}

#[test]
fn authority_policy_creation_requires_complete_candidate_and_explicit_properties() {
    let before = fixture();
    let mut after = before.clone();
    after.records.push(record(6, RecordKind::Plain));
    after.assertions.push(assertion(3, 6, 1, None));
    let mut policy = policy();
    policy.grants = vec![create_grant()];
    let decision = change(&policy, &before, &after, 6).unwrap();
    assert_eq!(decision.operation, Operation::Create);
    assert_eq!(decision.assertions_added, BTreeSet::from([a(3)]));
    policy.grants[0].properties.remove(&Property::Quantity);
    assert_eq!(
        change(&policy, &before, &after, 6),
        Err(AuthorityError::Denied)
    );
    policy.grants[0].properties.insert(Property::Quantity);
    policy.grants[0].assertions_add.clear();
    assert_eq!(
        change(&policy, &before, &after, 6),
        Err(AuthorityError::Denied)
    );
}

#[test]
fn authority_policy_property_grants_add_only_across_matching_scopes() {
    let before = fixture();
    let mut after = before.clone();
    content(&mut after, 3).body = "Body".into();
    content(&mut after, 3).head = "Head".into();
    let mut policy = policy();
    policy.grants.push(grant(
        Operation::Update,
        Predicate::UidEq(r(5)),
        [Property::Head],
    ));
    assert_eq!(
        change(&policy, &before, &after, 3),
        Err(AuthorityError::Denied)
    );
    policy.grants[1].selector = ordinary();
    assert!(change(&policy, &before, &after, 3).is_ok());
    policy.grants[1].operation = Operation::Create;
    assert_eq!(
        change(&policy, &before, &after, 3),
        Err(AuthorityError::Denied)
    );
}

#[test]
fn authority_policy_update_selector_must_match_both_states_in_one_grant() {
    let before = fixture();
    let mut after = before.clone();
    after
        .assertions
        .retain(|assertion| assertion.subject_uid != r(3));
    let mut policy = policy();
    policy.grants[0].selector = Predicate::ConceptIn(c(1));
    policy.grants[0].assertions_remove.push(unary(1));
    let mut after_only = grant(
        Operation::Update,
        Predicate::Not(Box::new(Predicate::ConceptIn(c(1)))),
        [],
    );
    after_only.assertions_remove.push(unary(1));
    policy.grants.push(after_only);
    assert_eq!(
        change(&policy, &before, &after, 3),
        Err(AuthorityError::Denied)
    );
    policy.grants[0].selector = ordinary();
    assert!(change(&policy, &before, &after, 3).is_ok());
}

#[test]
fn authority_policy_display_filter_is_not_part_of_authority() {
    let before = fixture();
    let mut after = before.clone();
    after
        .assertions
        .retain(|assertion| assertion.subject_uid != r(3));
    let mut policy = policy();
    policy.grants[0].assertions_remove.push(unary(1));
    assert!(change(&policy, &before, &after, 3).is_ok());
}

#[test]
fn authority_policy_ceiling_and_read_selector_only_narrow_mutation() {
    let before = fixture();
    let mut after = before.clone();
    content(&mut after, 3).body = "Updated".into();
    let policy = policy();
    let mut denied = ceiling(&before);
    denied.records.remove(&r(3));
    assert_eq!(
        may_read(Some(&policy), &before, &denied, &r(3), &Limits::default()),
        Ok(false)
    );
    for (current, proposed) in [(&denied, &ceiling(&after)), (&ceiling(&before), &denied)] {
        assert_eq!(
            authorize_record_change(
                Some(&policy),
                &before,
                &after,
                current,
                proposed,
                &r(3),
                &Limits::default()
            ),
            Err(AuthorityError::Denied)
        );
    }
    let mut unreadable = policy;
    unreadable.read = Predicate::Any(Vec::new());
    assert_eq!(
        change(&unreadable, &before, &after, 3),
        Err(AuthorityError::Denied)
    );
}

#[test]
fn authority_policy_extensions_have_exact_namespace_and_field_masks() {
    let before = fixture();
    let mut after = before.clone();
    let assignees = ExtensionProperty {
        namespace: "work".into(),
        field: "assignees".into(),
    };
    content(&mut after, 3)
        .extensions
        .insert(assignees.clone(), json!([r(2)]));
    assert_eq!(
        change(&policy(), &before, &after, 3),
        Err(AuthorityError::Denied)
    );
    let mut policy = policy();
    policy.grants[0]
        .properties
        .insert(Property::Extension(assignees.clone()));
    assert_eq!(
        change(&policy, &before, &after, 3).unwrap().properties,
        BTreeSet::from([Property::Extension(assignees)])
    );
    content(&mut after, 3).extensions.insert(
        ExtensionProperty {
            namespace: "lince.person".into(),
            field: "standing".into(),
        },
        json!({"active": true}),
    );
    assert_eq!(
        change(&policy, &before, &after, 3),
        Err(AuthorityError::Denied)
    );
}

#[test]
fn authority_policy_extension_removal_and_nested_changes_are_detected() {
    let mut before = fixture();
    let key = ExtensionProperty {
        namespace: "work".into(),
        field: "estimate".into(),
    };
    content(&mut before, 3)
        .extensions
        .insert(key.clone(), json!({"amount": 3, "unit": "h"}));
    let mut after = before.clone();
    content(&mut after, 3)
        .extensions
        .insert(key.clone(), json!({"amount": 4, "unit": "h"}));
    assert_eq!(
        change(&policy(), &before, &after, 3),
        Err(AuthorityError::Denied)
    );
    content(&mut after, 3).extensions.clear();
    assert_eq!(
        change(&policy(), &before, &after, 3),
        Err(AuthorityError::Denied)
    );
    let mut policy = policy();
    policy.grants[0].properties.insert(Property::Extension(key));
    assert!(change(&policy, &before, &after, 3).is_ok());
}

#[test]
fn authority_policy_delete_and_restore_use_separate_current_authority() {
    let before = fixture();
    let mut deleted = before.clone();
    record_mut(&mut deleted, 3).deleted = true;
    let mut policy = policy();
    assert_eq!(
        change(&policy, &before, &deleted, 3),
        Err(AuthorityError::Denied)
    );
    policy.grants.push(grant(Operation::Delete, ordinary(), []));
    assert_eq!(
        change(&policy, &before, &deleted, 3).unwrap().operation,
        Operation::Delete
    );
    assert_eq!(read(&policy, &deleted, 3), Ok(false));
    assert_eq!(
        change(&policy, &deleted, &before, 3),
        Err(AuthorityError::Denied)
    );
    let mut restore = create_grant();
    restore.operation = Operation::Restore;
    policy.grants.push(restore);
    assert_eq!(
        change(&policy, &deleted, &before, 3).unwrap().operation,
        Operation::Restore
    );
    let mut protected = before.clone();
    protected.assertions.push(assertion(3, 3, 2, None));
    assert_eq!(
        change(&policy, &deleted, &protected, 3),
        Err(AuthorityError::Denied)
    );
    policy
        .grants
        .retain(|grant| grant.operation != Operation::Restore);
    assert_eq!(
        change(&policy, &deleted, &before, 3),
        Err(AuthorityError::Denied)
    );
}

#[test]
fn authority_policy_immutable_origin_and_missing_candidate_are_refused() {
    let before = fixture();
    let mut after = before.clone();
    record_mut(&mut after, 3).organ_uid = None;
    let mut policy = policy();
    policy.grants[0].properties.insert(Property::Organ);
    assert_eq!(
        change(&policy, &before, &after, 3),
        Err(AuthorityError::InvalidMutation)
    );
    let mut after = before.clone();
    after.records.retain(|record| record.uid != r(3));
    after
        .assertions
        .retain(|assertion| assertion.subject_uid != r(3));
    assert_eq!(
        change(&policy, &before, &after, 3),
        Err(AuthorityError::InvalidMutation)
    );
}

#[test]
fn authority_policy_metadata_graph_does_not_require_every_document() {
    let mut before = fixture();
    for record in &mut before.records {
        if record.uid != r(3) {
            record.content = None;
        }
    }
    let mut after = before.clone();
    content(&mut after, 3).body = "Changed".into();
    assert!(change(&policy(), &before, &after, 3).is_ok());
    record_mut(&mut before, 3).content = None;
    assert_eq!(
        change(&policy(), &before, &after, 3),
        Err(AuthorityError::IncompleteRecord)
    );
}
