use protein_authority::{
    authority::{
        AssertionRole, AssertionTarget, AuthorityError, ExtensionProperty, Limits, Operation,
        RolePolicy, authorize_record_change, may_read,
    },
    query::{LinkDirection, Predicate},
};
use serde_json::json;

use super::fixtures::{change, content, fixture, grant, policy, read, unary};
use crate::fixtures::{assertion, c, ceiling, r};

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
        Predicate::RevisionEq(1),
        Predicate::RecordEq(r(3)),
        Predicate::StateIn(vec![]),
        Predicate::StatusIn(vec!["active".into()]),
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
