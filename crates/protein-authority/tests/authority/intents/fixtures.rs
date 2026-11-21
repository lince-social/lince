use std::collections::{BTreeMap, BTreeSet};

use nucleus::RecordKind;
use protein_authority::{
    authority::{
        AssertionGrant, AssertionIntent, AssertionProperty, AssertionRole, AssertionState,
        AssertionTarget, AuthorityError, ConceptState, GraphSnapshot, Limits, MutationGrant,
        Operation, Property, RecordDecision, RecordState, RolePolicy,
        authorize_record_changes_with_assertion_intents,
    },
    query::Predicate,
};

use crate::fixtures::{c, ceiling, document, r, target};

pub(super) fn all() -> Predicate {
    Predicate::All(Vec::new())
}

pub(super) fn record(number: u128) -> RecordState {
    RecordState {
        uid: r(number),
        kind: if number == 1 {
            RecordKind::Organ
        } else {
            RecordKind::Plain
        },
        organ_uid: (number != 1).then(|| r(1)),
        deleted: false,
        content: Some(document("Head".into(), "Body")),
    }
}

pub(super) fn assertion(number: u128) -> AssertionState {
    crate::fixtures::assertion(number, 3, 1, None)
}

pub(super) fn graph() -> GraphSnapshot {
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
        resolved_links: Default::default(),
        places: BTreeSet::new(),
    }
}

pub(super) fn rule(
    predicate: u128,
    role: AssertionRole,
    target: AssertionTarget,
) -> AssertionGrant {
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

pub(super) fn grant(operation: Operation) -> MutationGrant {
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

pub(super) fn policy() -> RolePolicy {
    RolePolicy {
        read: all(),
        grants: vec![grant(Operation::Update), grant(Operation::Create)],
    }
}

pub(super) fn intent(
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

pub(super) fn check(
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
        &[target(3, [])],
        intents,
        &Limits::default(),
    )
}
