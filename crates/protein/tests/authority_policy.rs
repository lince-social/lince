use std::collections::{BTreeMap, BTreeSet};

use nucleus::{DecimalValue, RecordKind};
use protein::authority::{
    AssertionGrant, AssertionProperty, AssertionRole, AssertionState, AssertionTarget,
    AuthorityError, ConceptState, ExtensionProperty, GraphSnapshot, Limits, MutationGrant,
    MutationTarget, Operation, Property, RecordContent, RecordDecision, RecordState, RolePolicy,
    VisibilityCeiling, authorize_record_change, authorize_record_changes, may_read,
    readable_records,
};
use protein::{LinkDirection, Predicate};
use serde_json::json;

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

fn quantity(amount: &str) -> DecimalValue {
    DecimalValue::parse_inferred(amount).unwrap()
}

fn record(number: u128, kind: RecordKind) -> RecordState {
    RecordState {
        uid: r(number),
        kind,
        organ_uid: (kind != RecordKind::Organ).then(|| r(1)),
        deleted: false,
        content: Some(RecordContent {
            slug: None,
            head: format!("Record {number}"),
            body: "Original content".into(),
            quantity: quantity("0"),
            unit_uid: None,
            place_uid: None,
            extensions: BTreeMap::new(),
        }),
    }
}

fn assertion(number: u128, subject: u128, predicate: u128, object: Option<u128>) -> AssertionState {
    AssertionState {
        uid: a(number),
        subject_uid: r(subject),
        predicate_uid: c(predicate),
        object_uid: object.map(r),
        role: AssertionRole::Ordinary,
        quantity: None,
        unit_uid: None,
    }
}

fn fixture() -> GraphSnapshot {
    GraphSnapshot {
        records: vec![
            record(1, RecordKind::Organ),
            record(2, RecordKind::Person),
            record(3, RecordKind::Plain),
            record(4, RecordKind::Plain),
            record(5, RecordKind::Plain),
        ],
        concepts: (1..=6)
            .map(|number| ConceptState {
                uid: c(number),
                name: format!("Vocabulary {number}"),
                parents: if number == 3 {
                    BTreeSet::from([c(1)])
                } else {
                    BTreeSet::new()
                },
            })
            .collect(),
        assertions: vec![assertion(1, 3, 1, None), assertion(2, 4, 2, None)],
        places: BTreeSet::new(),
    }
}

fn ordinary() -> Predicate {
    Predicate::Not(Box::new(Predicate::ConceptIn(c(2))))
}

fn grant(
    operation: Operation,
    selector: Predicate,
    properties: impl IntoIterator<Item = Property>,
) -> MutationGrant {
    MutationGrant {
        operation,
        selector,
        properties: properties.into_iter().collect(),
        assertions_add: Vec::new(),
        assertions_remove: Vec::new(),
    }
}

fn unary(predicate: u128) -> AssertionGrant {
    AssertionGrant {
        predicate_uid: c(predicate),
        target: AssertionTarget::Unary,
        role: AssertionRole::Ordinary,
        properties: BTreeSet::new(),
    }
}

fn policy() -> RolePolicy {
    RolePolicy {
        read: ordinary(),
        grants: vec![grant(Operation::Update, ordinary(), [Property::Body])],
    }
}

fn create_grant() -> MutationGrant {
    let mut grant = grant(
        Operation::Create,
        ordinary(),
        [
            Property::Kind,
            Property::Head,
            Property::Body,
            Property::Quantity,
            Property::Organ,
        ],
    );
    grant.assertions_add = vec![unary(1), unary(2)];
    grant
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

fn change(
    policy: &RolePolicy,
    before: &GraphSnapshot,
    after: &GraphSnapshot,
    number: u128,
) -> Result<RecordDecision, AuthorityError> {
    authorize_record_change(
        Some(policy),
        before,
        after,
        &ceiling(before),
        &ceiling(after),
        &r(number),
        &Limits::default(),
    )
}

fn content(graph: &mut GraphSnapshot, number: u128) -> &mut RecordContent {
    graph
        .records
        .iter_mut()
        .find(|record| record.uid == r(number))
        .unwrap()
        .content
        .as_mut()
        .unwrap()
}

fn read(policy: &RolePolicy, graph: &GraphSnapshot, number: u128) -> Result<bool, AuthorityError> {
    may_read(
        Some(policy),
        graph,
        &ceiling(graph),
        &r(number),
        &Limits::default(),
    )
}

fn target(number: u128, touched: impl IntoIterator<Item = Property>) -> MutationTarget {
    MutationTarget {
        record_uid: r(number),
        touched_properties: touched.into_iter().collect(),
    }
}

fn batch(
    policy: &RolePolicy,
    before: &GraphSnapshot,
    after: &GraphSnapshot,
    targets: &[MutationTarget],
) -> Result<BTreeMap<String, RecordDecision>, AuthorityError> {
    authorize_record_changes(
        Some(policy),
        before,
        after,
        &ceiling(before),
        &ceiling(after),
        targets,
        &Limits::default(),
    )
}

fn minimum_steps(mut operation: impl FnMut(usize) -> Result<(), AuthorityError>) -> usize {
    let mut low = 0;
    let mut high = 100_000;
    assert_eq!(operation(high), Ok(()));
    while low < high {
        let middle = low + (high - low) / 2;
        match operation(middle) {
            Ok(()) => high = middle,
            Err(AuthorityError::LimitExceeded) => low = middle + 1,
            Err(error) => panic!("unexpected authority error: {error}"),
        }
    }
    low
}

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
fn authority_policy_property_grants_cannot_be_borrowed_across_scopes_or_grants() {
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
    assert_eq!(
        change(&policy, &before, &after, 3),
        Err(AuthorityError::Denied)
    );
    policy.grants[0].properties.insert(Property::Head);
    assert!(change(&policy, &before, &after, 3).is_ok());
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
fn authority_policy_missing_policy_never_inherits_visibility() {
    let graph = fixture();
    assert_eq!(
        may_read(None, &graph, &ceiling(&graph), &r(3), &Limits::default()),
        Err(AuthorityError::MissingPolicy)
    );
    assert_eq!(
        authorize_record_change(
            None,
            &graph,
            &graph,
            &ceiling(&graph),
            &ceiling(&graph),
            &r(3),
            &Limits::default()
        ),
        Err(AuthorityError::MissingPolicy)
    );
}

#[test]
fn authority_policy_missing_dependencies_fail_even_under_negation_and_true_branch() {
    let graph = fixture();
    for predicate in [
        Predicate::ConceptIn(c(99)),
        Predicate::UidEq(r(99)),
        Predicate::Relation {
            kind: c(4),
            direction: LinkDirection::Out,
            other: Some(r(99)),
        },
        Predicate::Under {
            record: r(99),
            kind: c(4),
            include_self: false,
        },
        Predicate::OrganEq(r(99)),
    ] {
        let policy = RolePolicy {
            read: Predicate::Any(vec![
                Predicate::All(vec![]),
                Predicate::Not(Box::new(predicate)),
            ]),
            grants: vec![],
        };
        assert_eq!(
            read(&policy, &graph, 3),
            Err(AuthorityError::MissingDependency)
        );
    }
}

#[test]
fn authority_policy_unused_grant_and_assertion_dependency_are_validated() {
    let graph = fixture();
    let mut policy = policy();
    policy.grants.push(grant(
        Operation::Delete,
        Predicate::Not(Box::new(Predicate::ConceptIn(c(99)))),
        [],
    ));
    assert_eq!(
        read(&policy, &graph, 3),
        Err(AuthorityError::MissingDependency)
    );
    policy.grants.pop();
    policy.grants[0].assertions_remove.push(unary(99));
    assert_eq!(
        read(&policy, &graph, 3),
        Err(AuthorityError::MissingDependency)
    );
}

#[test]
fn authority_policy_refuses_unsupported_predicates_and_magic_identities() {
    let graph = fixture();
    for predicate in [
        Predicate::TextContains("Original".into()),
        Predicate::QuantityEq(store::exact::zero()),
        Predicate::StateIn(vec![]),
        Predicate::SlugEq("anything".into()),
        Predicate::Near {
            of: r(3),
            meters: 1.0,
        },
    ] {
        let policy = RolePolicy {
            read: Predicate::Any(vec![Predicate::All(vec![]), predicate]),
            grants: vec![],
        };
        assert_eq!(
            read(&policy, &graph, 3),
            Err(AuthorityError::UnsupportedPredicate)
        );
    }
    for predicate in [
        Predicate::ConceptIn("admin".into()),
        Predicate::UidEq("$actor".into()),
        Predicate::OrganEq("$organ".into()),
        Predicate::Under {
            record: r(3),
            kind: "part-of".into(),
            include_self: true,
        },
    ] {
        let policy = RolePolicy {
            read: predicate,
            grants: vec![],
        };
        assert_eq!(
            read(&policy, &graph, 3),
            Err(AuthorityError::InvalidIdentity)
        );
    }
}

#[test]
fn authority_policy_concept_families_cover_identity_and_binary_assertions() {
    let mut graph = fixture();
    graph.assertions[0].predicate_uid = c(3);
    graph.assertions[0].role = AssertionRole::Identity;
    graph.assertions.push(assertion(3, 5, 3, Some(2)));
    let policy = RolePolicy {
        read: Predicate::ConceptIn(c(1)),
        grants: vec![],
    };
    assert_eq!(read(&policy, &graph, 3), Ok(true));
    assert_eq!(read(&policy, &graph, 5), Ok(true));
    assert_eq!(read(&policy, &graph, 2), Ok(false));
    graph.concepts[0].name = "renamed completely".into();
    assert_eq!(read(&policy, &graph, 3), Ok(true));
}

#[test]
fn authority_policy_relation_direction_and_under_are_explicit() {
    let mut graph = fixture();
    graph
        .assertions
        .extend([assertion(3, 3, 4, Some(5)), assertion(4, 5, 4, Some(2))]);
    let scoped = |predicate| RolePolicy {
        read: predicate,
        grants: vec![],
    };
    let out = scoped(Predicate::Relation {
        kind: c(4),
        direction: LinkDirection::Out,
        other: Some(r(5)),
    });
    assert_eq!(read(&out, &graph, 3), Ok(true));
    assert_eq!(read(&out, &graph, 5), Ok(false));
    let incoming = scoped(Predicate::Relation {
        kind: c(4),
        direction: LinkDirection::In,
        other: Some(r(3)),
    });
    assert_eq!(read(&incoming, &graph, 5), Ok(true));
    let under = scoped(Predicate::Under {
        record: r(2),
        kind: c(4),
        include_self: false,
    });
    assert_eq!(read(&under, &graph, 3), Ok(true));
    assert_eq!(read(&under, &graph, 5), Ok(true));
    assert_eq!(read(&under, &graph, 2), Ok(false));
    assert_eq!(read(&under, &graph, 4), Ok(false));
    let under = scoped(Predicate::Under {
        record: r(2),
        kind: c(4),
        include_self: true,
    });
    assert_eq!(read(&under, &graph, 2), Ok(true));
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
    deleted
        .records
        .iter_mut()
        .find(|record| record.uid == r(3))
        .unwrap()
        .deleted = true;
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
    after
        .records
        .iter_mut()
        .find(|record| record.uid == r(3))
        .unwrap()
        .organ_uid = None;
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
    before
        .records
        .iter_mut()
        .find(|record| record.uid == r(3))
        .unwrap()
        .content = None;
    assert_eq!(
        change(&policy(), &before, &after, 3),
        Err(AuthorityError::IncompleteRecord)
    );
}

#[test]
fn authority_policy_organ_and_kind_predicates_require_real_identities() {
    let graph = fixture();
    let mut policy = RolePolicy {
        read: Predicate::All(vec![
            Predicate::KindEq("plain".into()),
            Predicate::OrganIn(vec![r(1)]),
        ]),
        grants: vec![],
    };
    assert_eq!(read(&policy, &graph, 3), Ok(true));
    assert_eq!(read(&policy, &graph, 2), Ok(false));
    policy.read = Predicate::OrganEq(r(2));
    assert_eq!(read(&policy, &graph, 3), Err(AuthorityError::InvalidGraph));
    policy.read = Predicate::KindEq("project".into());
    assert_eq!(read(&policy, &graph, 3), Err(AuthorityError::InvalidPolicy));
}

#[test]
fn authority_policy_deep_and_wide_short_circuited_policies_are_refused() {
    let graph = fixture();
    let mut nested = Predicate::All(vec![]);
    for _ in 0..20 {
        nested = Predicate::Not(Box::new(nested));
    }
    let policy = RolePolicy {
        read: Predicate::Any(vec![Predicate::All(vec![]), nested]),
        grants: vec![],
    };
    assert_eq!(read(&policy, &graph, 3), Err(AuthorityError::LimitExceeded));
    let policy = RolePolicy {
        read: Predicate::Any(vec![Predicate::All(vec![]); 1025]),
        grants: vec![],
    };
    assert_eq!(read(&policy, &graph, 3), Err(AuthorityError::LimitExceeded));
}

#[test]
fn authority_policy_graph_and_traversal_budgets_fail_closed() {
    let graph = fixture();
    let policy = policy();
    for limits in [
        Limits {
            records: 4,
            ..Limits::default()
        },
        Limits {
            concepts: 5,
            ..Limits::default()
        },
        Limits {
            assertions: 1,
            ..Limits::default()
        },
        Limits {
            graph_edges: 1,
            ..Limits::default()
        },
        Limits {
            bytes: 10,
            ..Limits::default()
        },
        Limits {
            steps: 3,
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
    ] {
        assert_eq!(
            may_read(Some(&policy), &graph, &ceiling(&graph), &r(3), &limits),
            Err(AuthorityError::LimitExceeded)
        );
    }
}

#[test]
fn authority_policy_cycles_and_missing_graph_edges_are_refused() {
    let mut graph = fixture();
    graph.concepts[0].parents.insert(c(3));
    assert_eq!(read(&policy(), &graph, 3), Err(AuthorityError::CyclicGraph));
    graph = fixture();
    graph
        .assertions
        .extend([assertion(3, 3, 4, Some(5)), assertion(4, 5, 4, Some(3))]);
    let under = RolePolicy {
        read: Predicate::Not(Box::new(Predicate::Under {
            record: r(2),
            kind: c(4),
            include_self: false,
        })),
        grants: vec![],
    };
    assert_eq!(read(&under, &graph, 3), Err(AuthorityError::CyclicGraph));
    graph = fixture();
    graph.concepts[0].parents.insert(c(99));
    assert_eq!(
        read(&policy(), &graph, 3),
        Err(AuthorityError::MissingDependency)
    );
}

#[test]
fn authority_policy_invalid_snapshot_identity_and_duplicate_rows_are_refused() {
    let mut graph = fixture();
    graph.records.push(graph.records[0].clone());
    assert_eq!(
        read(&policy(), &graph, 3),
        Err(AuthorityError::InvalidGraph)
    );
    graph = fixture();
    graph.assertions.push(assertion(3, 99, 1, None));
    assert_eq!(
        read(&policy(), &graph, 3),
        Err(AuthorityError::MissingDependency)
    );
    graph = fixture();
    graph.records[2].uid = "r_invented".into();
    assert_eq!(
        read(&policy(), &graph, 3),
        Err(AuthorityError::InvalidIdentity)
    );
}

#[test]
fn authority_policy_invalid_unused_identity_grant_is_not_ignored() {
    let graph = fixture();
    let mut policy = policy();
    let mut invalid = unary(1);
    invalid.role = AssertionRole::Identity;
    invalid.target = AssertionTarget::Record(r(2));
    policy.grants[0].assertions_remove.push(invalid);
    assert_eq!(read(&policy, &graph, 3), Err(AuthorityError::InvalidPolicy));
}

#[test]
fn authority_policy_serialization_rejects_output_fields_and_unknown_masks() {
    let policy = policy();
    let mut value = serde_json::to_value(&policy).unwrap();
    value["fields"] = json!(["body"]);
    assert!(serde_json::from_value::<RolePolicy>(value).is_err());
    let mut value = serde_json::to_value(&policy).unwrap();
    value["grants"][0]["properties"] = json!(["anything"]);
    assert!(serde_json::from_value::<RolePolicy>(value).is_err());
    let roundtrip: RolePolicy =
        serde_json::from_value(serde_json::to_value(&policy).unwrap()).unwrap();
    assert_eq!(read(&roundtrip, &fixture(), 3), Ok(true));
}

#[test]
fn authority_policy_deep_extension_values_are_refused_before_diffing() {
    let mut graph = fixture();
    let mut value = json!(true);
    for _ in 0..40 {
        value = serde_json::Value::Array(vec![value]);
    }
    content(&mut graph, 3).extensions.insert(
        ExtensionProperty {
            namespace: "work".into(),
            field: "details".into(),
        },
        value,
    );
    assert_eq!(
        read(&policy(), &graph, 3),
        Err(AuthorityError::LimitExceeded)
    );
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
fn authority_policy_runtime_matching_obeys_the_shared_step_budget() {
    let before = fixture();
    let mut after = before.clone();
    after.assertions.push(assertion(3, 3, 6, None));
    let mut policy = policy();
    policy.grants[0].assertions_add = vec![unary(1); 300];
    policy.grants[0].assertions_add.push(unary(6));
    let limits = Limits {
        steps: 800,
        ..Limits::default()
    };
    assert!(may_read(Some(&policy), &before, &ceiling(&before), &r(3), &limits).unwrap());
    assert_eq!(
        authorize_record_change(
            Some(&policy),
            &before,
            &after,
            &ceiling(&before),
            &ceiling(&after),
            &r(3),
            &limits
        ),
        Err(AuthorityError::LimitExceeded)
    );
    assert!(change(&policy, &before, &after, 3).is_ok());
}

#[test]
fn authority_policy_hidden_server_resolved_operands_do_not_need_subject_visibility() {
    let mut graph = fixture();
    let mut restricted = ceiling(&graph);
    restricted.records.remove(&r(4));
    restricted.concepts.remove(&c(2));
    for predicate in [Predicate::Not(Box::new(Predicate::UidEq(r(4)))), ordinary()] {
        let policy = RolePolicy {
            read: predicate,
            grants: Vec::new(),
        };
        assert_eq!(
            may_read(
                Some(&policy),
                &graph,
                &restricted,
                &r(3),
                &Limits::default()
            ),
            Ok(true)
        );
        assert_eq!(
            may_read(
                Some(&policy),
                &graph,
                &restricted,
                &r(4),
                &Limits::default()
            ),
            Ok(false)
        );
    }
    graph.assertions.push(assertion(3, 3, 4, Some(2)));
    restricted.records.remove(&r(2));
    restricted.concepts.remove(&c(4));
    let policy = RolePolicy {
        read: Predicate::Under {
            record: r(2),
            kind: c(4),
            include_self: true,
        },
        grants: Vec::new(),
    };
    assert_eq!(
        may_read(
            Some(&policy),
            &graph,
            &restricted,
            &r(3),
            &Limits::default()
        ),
        Ok(true)
    );
    assert_eq!(
        may_read(
            Some(&policy),
            &graph,
            &restricted,
            &r(2),
            &Limits::default()
        ),
        Ok(false)
    );
    let policy = RolePolicy {
        read: Predicate::Not(Box::new(Predicate::UidEq(r(99)))),
        grants: Vec::new(),
    };
    assert_eq!(
        may_read(
            Some(&policy),
            &graph,
            &restricted,
            &r(3),
            &Limits::default()
        ),
        Err(AuthorityError::MissingDependency)
    );
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
fn authority_policy_readable_batch_matches_single_reads_and_intersects_the_ceiling() {
    let mut graph = fixture();
    graph.assertions.push(assertion(3, 3, 4, Some(2)));
    graph
        .records
        .iter_mut()
        .find(|row| row.uid == r(5))
        .unwrap()
        .deleted = true;
    let mut restricted = ceiling(&graph);
    restricted.records.remove(&r(2));
    restricted.concepts.remove(&c(4));
    for selector in [
        ordinary(),
        Predicate::All(Vec::new()),
        Predicate::Any(Vec::new()),
        Predicate::UidEq(r(3)),
        Predicate::Not(Box::new(Predicate::UidEq(r(2)))),
        Predicate::ConceptIn(c(1)),
        Predicate::Relation {
            kind: c(4),
            direction: LinkDirection::Out,
            other: Some(r(2)),
        },
        Predicate::Under {
            record: r(2),
            kind: c(4),
            include_self: true,
        },
        Predicate::OrganEq(r(1)),
    ] {
        let policy = RolePolicy {
            read: selector,
            grants: Vec::new(),
        };
        let expected = graph
            .records
            .iter()
            .filter_map(|record| {
                may_read(
                    Some(&policy),
                    &graph,
                    &restricted,
                    &record.uid,
                    &Limits::default(),
                )
                .unwrap()
                .then(|| record.uid.clone())
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(
            readable_records(Some(&policy), &graph, &restricted, &Limits::default()),
            Ok(expected)
        );
    }
}

#[test]
fn authority_policy_batches_validate_unused_policy_and_empty_input() {
    let graph = fixture();
    let mut invalid = policy();
    invalid.grants.push(grant(
        Operation::Update,
        Predicate::Not(Box::new(Predicate::UidEq(r(99)))),
        [],
    ));
    assert_eq!(
        readable_records(
            Some(&invalid),
            &graph,
            &VisibilityCeiling::default(),
            &Limits::default()
        ),
        Err(AuthorityError::MissingDependency)
    );
    assert_eq!(
        batch(&invalid, &graph, &graph, &[]),
        Err(AuthorityError::MissingDependency)
    );
    assert_eq!(
        readable_records(None, &graph, &ceiling(&graph), &Limits::default()),
        Err(AuthorityError::MissingPolicy)
    );
    assert_eq!(
        authorize_record_changes(
            None,
            &graph,
            &graph,
            &ceiling(&graph),
            &ceiling(&graph),
            &[],
            &Limits::default()
        ),
        Err(AuthorityError::MissingPolicy)
    );
    assert_eq!(batch(&policy(), &graph, &graph, &[]), Ok(BTreeMap::new()));
}

#[test]
fn authority_policy_batch_prepares_selectors_once_per_snapshot_not_per_record() {
    let mut before = fixture();
    before
        .records
        .extend((6..=25).map(|number| record(number, RecordKind::Plain)));
    let targets = before
        .records
        .iter()
        .filter(|record| record.uid != r(4))
        .map(|record| MutationTarget {
            record_uid: record.uid.clone(),
            touched_properties: BTreeSet::from([Property::Body]),
        })
        .collect::<Vec<_>>();
    let policy = policy();
    let readable = readable_records(
        Some(&policy),
        &before,
        &ceiling(&before),
        &Limits {
            predicate_nodes: 4,
            ..Limits::default()
        },
    )
    .unwrap();
    assert_eq!(readable.len(), 24);
    assert_eq!(
        authorize_record_changes(
            Some(&policy),
            &before,
            &before,
            &ceiling(&before),
            &ceiling(&before),
            &targets,
            &Limits {
                predicate_nodes: 8,
                ..Limits::default()
            }
        )
        .unwrap()
        .len(),
        targets.len()
    );
    assert_eq!(
        authorize_record_changes(
            Some(&policy),
            &before,
            &before,
            &ceiling(&before),
            &ceiling(&before),
            &targets,
            &Limits {
                predicate_nodes: 7,
                ..Limits::default()
            }
        ),
        Err(AuthorityError::LimitExceeded)
    );
}

#[test]
fn authority_policy_readable_batch_exhaustion_returns_no_partial_set() {
    let graph = fixture();
    let policy = policy();
    let ceiling = ceiling(&graph);
    let single_steps = minimum_steps(|steps| {
        may_read(
            Some(&policy),
            &graph,
            &ceiling,
            &r(3),
            &Limits {
                steps,
                ..Limits::default()
            },
        )
        .map(|_| ())
    });
    assert_eq!(
        readable_records(
            Some(&policy),
            &graph,
            &ceiling,
            &Limits {
                steps: single_steps,
                ..Limits::default()
            }
        ),
        Err(AuthorityError::LimitExceeded)
    );
    let batch_steps = minimum_steps(|steps| {
        readable_records(
            Some(&policy),
            &graph,
            &ceiling,
            &Limits {
                steps,
                ..Limits::default()
            },
        )
        .map(|_| ())
    });
    assert!(batch_steps > single_steps);
    assert_eq!(
        readable_records(
            Some(&policy),
            &graph,
            &ceiling,
            &Limits {
                steps: batch_steps - 1,
                ..Limits::default()
            }
        ),
        Err(AuthorityError::LimitExceeded)
    );
}

#[test]
fn authority_policy_mutation_batch_matches_individually_projected_changes() {
    let before = fixture();
    let policy = policy();
    let mut after = before.clone();
    content(&mut after, 3).body = "First change".into();
    content(&mut after, 5).body = "Second change".into();
    let decisions = batch(&policy, &before, &after, &[target(5, []), target(3, [])]).unwrap();
    for number in [3, 5] {
        let mut projected = before.clone();
        *content(&mut projected, number) = content(&mut after, number).clone();
        assert_eq!(
            decisions.get(&r(number)),
            Some(&change(&policy, &before, &projected, number).unwrap())
        );
    }
    assert_eq!(decisions.len(), 2);
    assert_eq!(
        change(&policy, &before, &after, 3),
        Err(AuthorityError::InvalidMutation)
    );
}

#[test]
fn authority_policy_batch_creation_deletion_and_restore_keep_separate_operations() {
    let mut before = fixture();
    before
        .records
        .iter_mut()
        .find(|row| row.uid == r(5))
        .unwrap()
        .deleted = true;
    let mut after = before.clone();
    after
        .records
        .iter_mut()
        .find(|row| row.uid == r(3))
        .unwrap()
        .deleted = true;
    after
        .records
        .iter_mut()
        .find(|row| row.uid == r(5))
        .unwrap()
        .deleted = false;
    after.records.push(record(6, RecordKind::Plain));
    after.assertions.push(assertion(3, 6, 1, None));
    let mut policy = policy();
    policy.grants.push(create_grant());
    policy.grants.push(grant(Operation::Delete, ordinary(), []));
    let mut restore = create_grant();
    restore.operation = Operation::Restore;
    policy.grants.push(restore);
    let targets = [target(3, []), target(5, []), target(6, [Property::Body])];
    let decisions = batch(&policy, &before, &after, &targets).unwrap();
    assert_eq!(decisions[&r(3)].operation, Operation::Delete);
    assert_eq!(decisions[&r(5)].operation, Operation::Restore);
    assert_eq!(decisions[&r(6)].operation, Operation::Create);
    assert_eq!(decisions[&r(6)].assertions_added, BTreeSet::from([a(3)]));
    policy.grants.pop();
    assert_eq!(
        batch(&policy, &before, &after, &targets),
        Err(AuthorityError::Denied)
    );
}

#[test]
fn authority_policy_mutation_batch_exhaustion_refuses_the_entire_operation() {
    let graph = fixture();
    let policy = policy();
    let visible = ceiling(&graph);
    let first = [target(3, [Property::Body])];
    let both = [target(3, [Property::Body]), target(5, [Property::Body])];
    let single_steps = minimum_steps(|steps| {
        authorize_record_changes(
            Some(&policy),
            &graph,
            &graph,
            &visible,
            &visible,
            &first,
            &Limits {
                steps,
                ..Limits::default()
            },
        )
        .map(|_| ())
    });
    assert_eq!(
        authorize_record_changes(
            Some(&policy),
            &graph,
            &graph,
            &visible,
            &visible,
            &both,
            &Limits {
                steps: single_steps,
                ..Limits::default()
            }
        ),
        Err(AuthorityError::LimitExceeded)
    );
    let batch_steps = minimum_steps(|steps| {
        authorize_record_changes(
            Some(&policy),
            &graph,
            &graph,
            &visible,
            &visible,
            &both,
            &Limits {
                steps,
                ..Limits::default()
            },
        )
        .map(|_| ())
    });
    assert!(batch_steps > single_steps);
    assert_eq!(
        authorize_record_changes(
            Some(&policy),
            &graph,
            &graph,
            &visible,
            &visible,
            &both,
            &Limits {
                steps: batch_steps - 1,
                ..Limits::default()
            }
        ),
        Err(AuthorityError::LimitExceeded)
    );
    assert_eq!(batch(&policy, &graph, &graph, &both).unwrap().len(), 2);
}

#[test]
fn authority_policy_batch_assertion_lookup_is_indexed_by_subject() {
    let mut graph = fixture();
    for number in 100..300 {
        graph.concepts.push(ConceptState {
            uid: c(number),
            name: format!("Predicate {number}"),
            parents: BTreeSet::new(),
        });
        graph.assertions.push(assertion(number, 2, number, None));
    }
    let policy = RolePolicy {
        read: Predicate::All(Vec::new()),
        grants: vec![grant(
            Operation::Update,
            Predicate::All(Vec::new()),
            [Property::Body],
        )],
    };
    let visible = ceiling(&graph);
    let steps = |targets: &[MutationTarget]| {
        minimum_steps(|steps| {
            authorize_record_changes(
                Some(&policy),
                &graph,
                &graph,
                &visible,
                &visible,
                targets,
                &Limits {
                    steps,
                    ..Limits::default()
                },
            )
            .map(|_| ())
        })
    };
    let single = steps(&[target(3, [Property::Body])]);
    let double = steps(&[target(3, [Property::Body]), target(5, [Property::Body])]);
    assert!(double > single);
    assert!(double - single < 30);
}

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
fn authority_policy_actual_changes_and_footprints_cannot_borrow_different_grants() {
    let before = fixture();
    let mut after = before.clone();
    content(&mut after, 3).body = "Changed body".into();
    let mut policy = policy();
    policy
        .grants
        .push(grant(Operation::Update, ordinary(), [Property::Head]));
    let targets = [target(3, [Property::Head])];
    assert_eq!(
        batch(&policy, &before, &after, &targets),
        Err(AuthorityError::Denied)
    );
    assert_eq!(
        batch(
            &policy,
            &before,
            &before,
            &[target(3, [Property::Head, Property::Body])]
        ),
        Err(AuthorityError::Denied)
    );
    policy.grants[0].properties.insert(Property::Head);
    let decision = batch(&policy, &before, &after, &targets).unwrap();
    assert_eq!(decision[&r(3)].grant_index, 0);
    assert_eq!(
        decision[&r(3)].properties,
        BTreeSet::from([Property::Body, Property::Head])
    );
}

#[test]
fn authority_policy_assertion_delta_and_footprint_must_share_one_grant() {
    let before = fixture();
    let mut after = before.clone();
    after.assertions.push(assertion(3, 3, 4, None));
    let mut policy = policy();
    let mut separate = grant(Operation::Update, ordinary(), []);
    separate.assertions_add.push(unary(4));
    policy.grants.push(separate);
    let targets = [target(3, [Property::Body])];
    assert_eq!(
        batch(&policy, &before, &after, &targets),
        Err(AuthorityError::Denied)
    );
    policy.grants[0].assertions_add.push(unary(4));
    let decision = batch(&policy, &before, &after, &targets).unwrap();
    assert_eq!(decision[&r(3)].grant_index, 0);
    assert_eq!(decision[&r(3)].assertions_added, BTreeSet::from([a(3)]));
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
    let mut variants = Vec::new();
    let mut changed = before.clone();
    content(&mut changed, 5).body = "Unlisted edit".into();
    variants.push(changed);
    let mut created = before.clone();
    created.records.push(record(6, RecordKind::Plain));
    variants.push(created);
    let mut deleted = before.clone();
    deleted
        .records
        .iter_mut()
        .find(|row| row.uid == r(5))
        .unwrap()
        .deleted = true;
    variants.push(deleted);
    let mut removed = before.clone();
    removed.records.retain(|row| row.uid != r(5));
    variants.push(removed);
    for after in variants {
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
    let mut variants = Vec::new();
    let mut added = before.clone();
    added.assertions.push(assertion(3, 5, 4, None));
    variants.push(added);
    let mut removed = before.clone();
    removed.assertions.retain(|row| row.uid != a(2));
    variants.push(removed);
    let mut replaced = before.clone();
    replaced.assertions[1].predicate_uid = c(4);
    variants.push(replaced);
    for after in variants {
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
    for targets in [vec![target(3, [])], vec![target(5, [])]] {
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
    let mut variants = Vec::new();
    let mut renamed = before.clone();
    renamed.concepts[0].name = "Renamed vocabulary".into();
    variants.push(renamed);
    let mut reparented = before.clone();
    reparented.concepts[2].parents = BTreeSet::from([c(2)]);
    variants.push(reparented);
    let mut concept_created = before.clone();
    concept_created.concepts.push(ConceptState {
        uid: c(7),
        name: "New vocabulary".into(),
        parents: BTreeSet::new(),
    });
    variants.push(concept_created);
    let mut place_created = before.clone();
    place_created.places.insert(uid("pl", 1));
    variants.push(place_created);
    for after in variants {
        assert_eq!(
            batch(&policy(), &before, &after, &[target(3, [])]),
            Err(AuthorityError::InvalidMutation)
        );
    }
}
