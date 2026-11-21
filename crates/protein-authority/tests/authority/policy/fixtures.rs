use std::collections::{BTreeMap, BTreeSet};

use nucleus::RecordKind;
use protein_authority::{
    authority::{
        AssertionGrant, AssertionRole, AssertionTarget, AuthorityError, ConceptState,
        GraphSnapshot, Limits, MutationGrant, MutationTarget, Operation, Property, RecordContent,
        RecordDecision, RecordState, RolePolicy, authorize_record_change, authorize_record_changes,
        may_read,
    },
    query::Predicate,
};

use crate::fixtures::{assertion, c, ceiling, document, r};

pub(super) fn record(number: u128, kind: RecordKind) -> RecordState {
    RecordState {
        uid: r(number),
        kind,
        organ_uid: (kind != RecordKind::Organ).then(|| r(1)),
        deleted: false,
        content: Some(document(format!("Record {number}"), "Original content")),
    }
}

pub(super) fn fixture() -> GraphSnapshot {
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
        resolved_links: Default::default(),
        places: BTreeSet::new(),
    }
}

pub(super) fn ordinary() -> Predicate {
    Predicate::Not(Box::new(Predicate::ConceptIn(c(2))))
}

pub(super) fn grant(
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

pub(super) fn unary(predicate: u128) -> AssertionGrant {
    AssertionGrant {
        predicate_uid: c(predicate),
        target: AssertionTarget::Unary,
        role: AssertionRole::Ordinary,
        properties: BTreeSet::new(),
    }
}

pub(super) fn policy() -> RolePolicy {
    RolePolicy {
        read: ordinary(),
        grants: vec![grant(Operation::Update, ordinary(), [Property::Body])],
    }
}

pub(super) fn create_grant() -> MutationGrant {
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

pub(super) fn change(
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

pub(super) fn content(graph: &mut GraphSnapshot, number: u128) -> &mut RecordContent {
    record_mut(graph, number)
        .content
        .as_mut()
        .expect("fixture Record content")
}

pub(super) fn read(
    policy: &RolePolicy,
    graph: &GraphSnapshot,
    number: u128,
) -> Result<bool, AuthorityError> {
    may_read(
        Some(policy),
        graph,
        &ceiling(graph),
        &r(number),
        &Limits::default(),
    )
}

pub(super) fn batch(
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

pub(super) fn record_mut(graph: &mut GraphSnapshot, number: u128) -> &mut RecordState {
    let uid = r(number);
    graph
        .records
        .iter_mut()
        .find(|record| record.uid == uid)
        .expect("fixture Record")
}
