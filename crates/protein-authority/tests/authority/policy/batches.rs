use std::collections::{BTreeMap, BTreeSet};

use nucleus::RecordKind;
use protein_authority::{
    authority::{
        AuthorityError, ConceptState, Limits, MutationTarget, Operation, Property, RolePolicy,
        VisibilityCeiling, authorize_record_changes, readable_records,
    },
    query::Predicate,
};

use super::fixtures::{
    batch, change, content, create_grant, fixture, grant, ordinary, policy, record, record_mut,
};
use crate::fixtures::{a, assertion, c, ceiling, minimum_steps, r, target};

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
    let excluded = r(4);
    let targets = before
        .records
        .iter()
        .filter(|record| record.uid != excluded)
        .map(|record| MutationTarget {
            record_uid: record.uid.clone(),
            touched_properties: BTreeSet::from([Property::Body]),
        })
        .collect::<Vec<_>>();
    let policy = policy();
    let visible = ceiling(&before);
    let readable = readable_records(
        Some(&policy),
        &before,
        &visible,
        &Limits {
            predicate_nodes: 4,
            ..Limits::default()
        },
    )
    .unwrap();
    assert_eq!(readable.len(), 24);
    let evaluate = |predicate_nodes| {
        authorize_record_changes(
            Some(&policy),
            &before,
            &before,
            &visible,
            &visible,
            &targets,
            &Limits {
                predicate_nodes,
                ..Limits::default()
            },
        )
    };
    assert_eq!(
        evaluate(8).expect("shared selector preparation").len(),
        targets.len()
    );
    assert_eq!(evaluate(7), Err(AuthorityError::LimitExceeded));
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
    record_mut(&mut before, 5).deleted = true;
    let mut after = before.clone();
    record_mut(&mut after, 3).deleted = true;
    record_mut(&mut after, 5).deleted = false;
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
    let evaluate = |targets: &[MutationTarget], steps| {
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
    };
    let single_steps = minimum_steps(100_000, |steps| evaluate(&first, steps).map(drop));
    assert_eq!(
        evaluate(&both, single_steps),
        Err(AuthorityError::LimitExceeded)
    );
    let batch_steps = minimum_steps(100_000, |steps| evaluate(&both, steps).map(drop));
    assert!(batch_steps > single_steps);
    assert_eq!(
        evaluate(&both, batch_steps - 1),
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
        minimum_steps(100_000, |steps| {
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
    assert!(
        double > single && double - single < 30,
        "single {single}, double {double}"
    );
}
