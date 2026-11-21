use std::collections::BTreeSet;

use protein_authority::{
    authority::{
        AssertionRole, AuthorityError, Limits, RolePolicy, authorize_record_change, may_read,
        readable_records, selector_membership_changes,
    },
    query::{LinkDirection, Predicate},
};

use super::fixtures::{content, fixture, ordinary, policy, read, record_mut};
use crate::fixtures::{assertion, c, ceiling, minimum_steps, quantity, r};

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
fn authority_policy_readable_batch_matches_single_reads_and_intersects_the_ceiling() {
    let mut graph = fixture();
    graph.assertions.push(assertion(3, 3, 4, Some(2)));
    record_mut(&mut graph, 5).deleted = true;
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
        let limits = Limits::default();
        let expected = graph
            .records
            .iter()
            .filter(|record| {
                may_read(Some(&policy), &graph, &restricted, &record.uid, &limits)
                    .expect("valid per-Record authority check")
            })
            .map(|record| record.uid.clone())
            .collect();
        assert_eq!(
            readable_records(Some(&policy), &graph, &restricted, &limits),
            Ok(expected)
        );
    }
}

#[test]
fn authority_policy_readable_batch_exhaustion_returns_no_partial_set() {
    let graph = fixture();
    let policy = policy();
    let ceiling = ceiling(&graph);
    let record = r(3);
    let single_steps = minimum_steps(100_000, |steps| {
        may_read(
            Some(&policy),
            &graph,
            &ceiling,
            &record,
            &Limits {
                steps,
                ..Limits::default()
            },
        )
        .map(drop)
    });
    let read_batch = |steps| {
        readable_records(
            Some(&policy),
            &graph,
            &ceiling,
            &Limits {
                steps,
                ..Limits::default()
            },
        )
    };
    assert_eq!(read_batch(single_steps), Err(AuthorityError::LimitExceeded));
    let batch_steps = minimum_steps(100_000, |steps| read_batch(steps).map(drop));
    assert!(batch_steps > single_steps);
    assert_eq!(
        read_batch(batch_steps - 1),
        Err(AuthorityError::LimitExceeded)
    );
}

#[test]
fn authority_policy_content_filters_track_changed_record_classification() {
    let before = fixture();
    let mut after = before.clone();
    let changed = content(&mut after, 3);
    changed.body = "Special token".into();
    changed.slug = Some("private".into());
    changed.unit_uid = Some(c(4));
    changed.quantity = quantity("3");
    for selector in [
        Predicate::SlugEq("private".into()),
        Predicate::UnitEq(c(4)),
        Predicate::TextContains("Special".into()),
        Predicate::QuantityGt(quantity("0")),
        Predicate::QuantityLte(quantity("0")),
        Predicate::QuantityEq(quantity("0")),
    ] {
        let policy = RolePolicy {
            read: selector.clone(),
            grants: vec![],
        };
        assert_ne!(
            read(&policy, &before, 3).unwrap(),
            read(&policy, &after, 3).unwrap()
        );
        assert_eq!(
            selector_membership_changes(&[selector], &before, &after, &Limits::default()).unwrap(),
            BTreeSet::from([r(3)])
        );
    }
}
