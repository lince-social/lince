use std::collections::{BTreeMap, BTreeSet};

use nucleus::{DecimalValue, RecordKind};
use protein::authority::{
    AssertionRole, AssertionState, AuthorityError, ConceptState, GraphSnapshot, Limits,
    RecordContent, RecordState, RolePolicy, VisibilityCeiling, readable_records,
    selector_membership_changes,
};
use protein::{LinkDirection, Predicate};

fn uid(prefix: &str, number: u128) -> String {
    format!("{prefix}_{}", nucleus::id::ulid_from(0, number))
}

fn r(number: u128) -> String {
    uid("r", number)
}

fn c(number: u128) -> String {
    uid("c", number)
}

fn record(number: u128) -> RecordState {
    RecordState {
        uid: r(number),
        kind: RecordKind::Plain,
        organ_uid: None,
        deleted: false,
        content: None,
    }
}

fn assertion(number: u128, subject: u128, predicate: u128, object: Option<u128>) -> AssertionState {
    AssertionState {
        uid: uid("a", number),
        subject_uid: r(subject),
        predicate_uid: c(predicate),
        object_uid: object.map(r),
        role: AssertionRole::Ordinary,
        quantity: None,
        unit_uid: None,
    }
}

fn graph() -> GraphSnapshot {
    GraphSnapshot {
        records: (1..=6).map(record).collect(),
        concepts: (1..=4)
            .map(|number| ConceptState {
                uid: c(number),
                name: String::new(),
                parents: if number == 2 {
                    BTreeSet::from([c(1)])
                } else {
                    BTreeSet::new()
                },
            })
            .collect(),
        assertions: Vec::new(),
        places: BTreeSet::new(),
    }
}

fn under(include_self: bool) -> Predicate {
    Predicate::Under {
        record: r(1),
        kind: c(4),
        include_self,
    }
}

fn hierarchy() -> GraphSnapshot {
    let mut graph = graph();
    graph.assertions = vec![
        assertion(1, 3, 4, Some(1)),
        assertion(2, 4, 4, Some(3)),
        assertion(3, 5, 4, Some(4)),
    ];
    graph
}

fn relation(direction: LinkDirection, other: Option<u128>) -> Predicate {
    Predicate::Relation {
        kind: c(1),
        direction,
        other: other.map(r),
    }
}

fn changed(
    selectors: &[Predicate],
    current: &GraphSnapshot,
    proposed: &GraphSnapshot,
) -> BTreeSet<String> {
    selector_membership_changes(selectors, current, proposed, &Limits::default()).unwrap()
}

fn ids(numbers: &[u128]) -> BTreeSet<String> {
    numbers.iter().copied().map(r).collect()
}

fn content(head: &str) -> RecordContent {
    RecordContent {
        slug: None,
        head: head.into(),
        body: String::new(),
        quantity: DecimalValue::parse_inferred("0").unwrap(),
        unit_uid: None,
        place_uid: None,
        extensions: BTreeMap::new(),
    }
}

#[test]
fn authority_impact_concept_descendants_change_without_subject_write() {
    let mut current = graph();
    current.concepts[2].parents.insert(c(2));
    current.assertions = vec![assertion(1, 3, 2, None), assertion(2, 4, 3, None)];
    let mut proposed = current.clone();
    proposed.concepts[1].parents.clear();
    assert_eq!(
        changed(&[Predicate::ConceptIn(c(1))], &current, &proposed),
        ids(&[3, 4])
    );
    assert_eq!(
        changed(&[Predicate::ConceptIn(c(2))], &current, &proposed),
        ids(&[])
    );
}

#[test]
fn authority_impact_direct_classification_only_changes_subject() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.assertions.push(assertion(1, 3, 2, None));
    assert_eq!(
        changed(&[Predicate::ConceptIn(c(1))], &current, &proposed),
        ids(&[3])
    );
}

#[test]
fn authority_impact_relation_out_without_other() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.assertions.push(assertion(1, 3, 2, Some(4)));
    assert_eq!(
        changed(&[relation(LinkDirection::Out, None)], &current, &proposed),
        ids(&[3])
    );
}

#[test]
fn authority_impact_relation_in_without_other_includes_unedited_object() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.assertions.push(assertion(1, 3, 2, Some(4)));
    assert_eq!(
        changed(&[relation(LinkDirection::In, None)], &current, &proposed),
        ids(&[4])
    );
}

#[test]
fn authority_impact_relation_both_without_other() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.assertions.push(assertion(1, 3, 2, Some(4)));
    assert_eq!(
        changed(&[relation(LinkDirection::Both, None)], &current, &proposed),
        ids(&[3, 4])
    );
}

#[test]
fn authority_impact_relation_out_with_other_retarget() {
    let mut current = graph();
    current.assertions.push(assertion(1, 3, 2, Some(4)));
    let mut proposed = current.clone();
    proposed.assertions[0].object_uid = Some(r(5));
    assert_eq!(
        changed(
            &[relation(LinkDirection::Out, Some(4))],
            &current,
            &proposed
        ),
        ids(&[3])
    );
    assert_eq!(
        changed(&[relation(LinkDirection::Out, None)], &current, &proposed),
        ids(&[])
    );
}

#[test]
fn authority_impact_relation_in_with_other_retarget() {
    let mut current = graph();
    current.assertions.push(assertion(1, 3, 2, Some(4)));
    let mut proposed = current.clone();
    proposed.assertions[0].object_uid = Some(r(5));
    assert_eq!(
        changed(&[relation(LinkDirection::In, Some(3))], &current, &proposed),
        ids(&[4, 5])
    );
    assert_eq!(
        changed(&[relation(LinkDirection::In, Some(6))], &current, &proposed),
        ids(&[])
    );
}

#[test]
fn authority_impact_relation_both_with_other_observes_both_ends() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.assertions = vec![assertion(1, 3, 2, Some(4)), assertion(2, 4, 2, Some(5))];
    assert_eq!(
        changed(
            &[relation(LinkDirection::Both, Some(4))],
            &current,
            &proposed
        ),
        ids(&[3, 5])
    );
}

#[test]
fn authority_impact_relation_ignores_unary_and_unrelated_predicates() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.assertions = vec![assertion(1, 3, 2, None), assertion(2, 4, 3, Some(5))];
    assert_eq!(
        changed(&[relation(LinkDirection::Both, None)], &current, &proposed),
        ids(&[])
    );
}

#[test]
fn authority_impact_relation_quantity_change_has_no_selection_effect() {
    let mut current = graph();
    current.assertions.push(assertion(1, 3, 2, Some(4)));
    let mut proposed = current.clone();
    proposed.assertions[0].quantity = Some(DecimalValue::parse_inferred("42").unwrap());
    assert_eq!(
        changed(&[relation(LinkDirection::Both, None)], &current, &proposed),
        ids(&[])
    );
}

#[test]
fn authority_impact_under_leaf_move_only_changes_leaf() {
    let current = hierarchy();
    let mut proposed = current.clone();
    proposed.assertions[2].object_uid = Some(r(2));
    assert_eq!(changed(&[under(false)], &current, &proposed), ids(&[5]));
}

#[test]
fn authority_impact_under_parent_move_includes_descendants() {
    let current = hierarchy();
    let mut proposed = current.clone();
    proposed.assertions[0].object_uid = Some(r(2));
    assert_eq!(
        changed(&[under(false)], &current, &proposed),
        ids(&[3, 4, 5])
    );
}

#[test]
fn authority_impact_under_move_within_scope_has_no_membership_change() {
    let current = hierarchy();
    let mut proposed = current.clone();
    proposed.assertions[2].object_uid = Some(r(3));
    assert_eq!(
        changed(&[under(false), under(true)], &current, &proposed),
        ids(&[])
    );
}

#[test]
fn authority_impact_under_include_self_respects_root_deletion() {
    let current = hierarchy();
    let mut proposed = current.clone();
    proposed.records[0].deleted = true;
    assert_eq!(changed(&[under(true)], &current, &proposed), ids(&[1]));
    assert_eq!(changed(&[under(false)], &current, &proposed), ids(&[]));
}

#[test]
fn authority_impact_boolean_branches_can_change_without_effective_change() {
    let mut current = graph();
    current.assertions.push(assertion(1, 3, 1, None));
    let mut proposed = current.clone();
    proposed.assertions[0].predicate_uid = c(3);
    let selector = Predicate::Any(vec![Predicate::ConceptIn(c(1)), Predicate::ConceptIn(c(3))]);
    assert_eq!(changed(&[selector], &current, &proposed), ids(&[]));
}

#[test]
fn authority_impact_any_selector_change_is_not_union_membership_change() {
    let mut current = graph();
    current.assertions.push(assertion(1, 3, 1, None));
    let mut proposed = current.clone();
    proposed.assertions[0].predicate_uid = c(3);
    assert_eq!(
        changed(
            &[Predicate::ConceptIn(c(1)), Predicate::ConceptIn(c(3))],
            &current,
            &proposed
        ),
        ids(&[3])
    );
}

#[test]
fn authority_impact_nested_boolean_not_changes_effective_membership() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.assertions.push(assertion(1, 3, 2, None));
    let selector = Predicate::All(vec![
        Predicate::KindEq(RecordKind::Plain.as_str().into()),
        Predicate::Not(Box::new(Predicate::Any(vec![
            Predicate::ConceptIn(c(1)),
            Predicate::UidEq(r(6)),
        ]))),
    ]);
    assert_eq!(changed(&[selector], &current, &proposed), ids(&[3]));
}

#[test]
fn authority_impact_deletion_and_restoration_change_live_membership() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.records[2].deleted = true;
    assert_eq!(
        changed(&[Predicate::All(Vec::new())], &current, &proposed),
        ids(&[3])
    );
    assert_eq!(
        changed(&[Predicate::All(Vec::new())], &proposed, &current),
        ids(&[3])
    );
    assert_eq!(
        changed(&[Predicate::Any(Vec::new())], &current, &proposed),
        ids(&[])
    );
}

#[test]
fn authority_impact_creation_and_removal_use_complete_union_of_records() {
    let mut current = graph();
    let mut proposed = current.clone();
    current.records.remove(5);
    proposed.records.remove(0);
    assert_eq!(
        changed(&[Predicate::All(Vec::new())], &current, &proposed),
        ids(&[1, 6])
    );
}

#[test]
fn authority_impact_deleted_records_do_not_gain_membership() {
    let mut current = graph();
    current.records[2].deleted = true;
    let mut proposed = current.clone();
    proposed.assertions.push(assertion(1, 3, 1, None));
    assert_eq!(
        changed(&[Predicate::ConceptIn(c(1))], &current, &proposed),
        ids(&[])
    );
}

#[test]
fn authority_impact_kind_and_organ_changes_use_existing_selectors() {
    let mut current = graph();
    current.records[0].kind = RecordKind::Organ;
    current.records[1].kind = RecordKind::Organ;
    current.records[2].organ_uid = Some(r(1));
    let mut proposed = current.clone();
    proposed.records[2].organ_uid = Some(r(2));
    proposed.records[3].kind = RecordKind::Person;
    assert_eq!(
        changed(
            &[
                Predicate::OrganEq(r(1)),
                Predicate::OrganIn(vec![r(2)]),
                Predicate::KindEq(RecordKind::Person.as_str().into()),
            ],
            &current,
            &proposed
        ),
        ids(&[3, 4])
    );
    assert_eq!(
        changed(&[Predicate::OrganIn(vec![r(1), r(2)])], &current, &proposed),
        ids(&[])
    );
}

#[test]
fn authority_impact_hidden_operands_and_subjects_are_not_viewer_filtered() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.assertions.push(assertion(1, 3, 2, Some(4)));
    let selector = Predicate::Not(Box::new(relation(LinkDirection::In, Some(3))));
    assert_eq!(changed(&[selector.clone()], &current, &proposed), ids(&[4]));
    let policy = RolePolicy {
        read: selector,
        grants: Vec::new(),
    };
    let ceiling = VisibilityCeiling {
        records: ids(&[1]),
        ..VisibilityCeiling::default()
    };
    assert_eq!(
        readable_records(Some(&policy), &current, &ceiling, &Limits::default()).unwrap(),
        ids(&[1])
    );
    assert_eq!(
        readable_records(Some(&policy), &proposed, &ceiling, &Limits::default()).unwrap(),
        ids(&[1])
    );
}

#[test]
fn authority_impact_deleted_operand_remains_a_resolved_dependency() {
    let mut current = graph();
    current.records[3].deleted = true;
    let mut proposed = current.clone();
    proposed.assertions.push(assertion(1, 3, 2, Some(4)));
    assert_eq!(
        changed(
            &[relation(LinkDirection::Out, Some(4))],
            &current,
            &proposed
        ),
        ids(&[3])
    );
}

#[test]
fn authority_impact_missing_not_dependency_refuses_even_on_empty_graph() {
    let graph = GraphSnapshot::default();
    assert_eq!(
        selector_membership_changes(
            &[Predicate::Not(Box::new(Predicate::UidEq(r(99))))],
            &graph,
            &graph,
            &Limits::default()
        ),
        Err(AuthorityError::MissingDependency)
    );
}

#[test]
fn authority_impact_missing_dependency_in_proposed_graph_refuses() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.records.remove(5);
    assert_eq!(
        selector_membership_changes(
            &[Predicate::Any(vec![
                Predicate::All(Vec::new()),
                Predicate::UidEq(r(6))
            ])],
            &current,
            &proposed,
            &Limits::default()
        ),
        Err(AuthorityError::MissingDependency)
    );
}

#[test]
fn authority_impact_later_invalid_selector_never_returns_partial_changes() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.records[2].deleted = true;
    assert_eq!(
        selector_membership_changes(
            &[Predicate::All(Vec::new()), Predicate::ConceptIn(c(99))],
            &current,
            &proposed,
            &Limits::default()
        ),
        Err(AuthorityError::MissingDependency)
    );
}

#[test]
fn authority_impact_unsupported_short_circuit_branch_refuses() {
    let graph = graph();
    for selector in [
        Predicate::Any(vec![
            Predicate::All(Vec::new()),
            Predicate::TextContains("secret".into()),
        ]),
        Predicate::All(vec![
            Predicate::Any(Vec::new()),
            Predicate::QuantityEq(store::exact::zero()),
        ]),
        Predicate::Not(Box::new(Predicate::SlugEq("task".into()))),
    ] {
        assert_eq!(
            selector_membership_changes(&[selector], &graph, &graph, &Limits::default()),
            Err(AuthorityError::UnsupportedPredicate)
        );
    }
}

#[test]
fn authority_impact_invalid_identity_and_kind_refuse() {
    let graph = graph();
    assert_eq!(
        selector_membership_changes(
            &[Predicate::ConceptIn("task".into())],
            &graph,
            &graph,
            &Limits::default()
        ),
        Err(AuthorityError::InvalidIdentity)
    );
    assert_eq!(
        selector_membership_changes(
            &[Predicate::KindEq("company-task".into())],
            &graph,
            &graph,
            &Limits::default()
        ),
        Err(AuthorityError::InvalidPolicy)
    );
}

#[test]
fn authority_impact_malformed_graphs_refuse_before_empty_selector_result() {
    let current = graph();
    let mut duplicate = current.clone();
    duplicate.records.push(record(1));
    let mut missing = current.clone();
    missing.assertions.push(assertion(1, 3, 1, Some(99)));
    let mut identity = current.clone();
    let mut invalid = assertion(1, 3, 1, Some(4));
    invalid.role = AssertionRole::Identity;
    identity.assertions.push(invalid);
    for (proposed, error) in [
        (duplicate, AuthorityError::InvalidGraph),
        (missing, AuthorityError::MissingDependency),
        (identity, AuthorityError::InvalidGraph),
    ] {
        assert_eq!(
            selector_membership_changes(&[], &current, &proposed, &Limits::default()),
            Err(error)
        );
    }
}

#[test]
fn authority_impact_concept_cycle_refuses_without_selectors() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.concepts[0].parents.insert(c(2));
    assert_eq!(
        selector_membership_changes(&[], &current, &proposed, &Limits::default()),
        Err(AuthorityError::CyclicGraph)
    );
}

#[test]
fn authority_impact_under_retains_disconnected_cycle_refusal() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.assertions = vec![assertion(1, 5, 4, Some(6)), assertion(2, 6, 4, Some(5))];
    assert_eq!(
        selector_membership_changes(
            &[Predicate::Any(vec![
                Predicate::All(Vec::new()),
                under(false)
            ])],
            &current,
            &proposed,
            &Limits::default()
        ),
        Err(AuthorityError::CyclicGraph)
    );
}

#[test]
fn authority_impact_direct_relation_does_not_invent_general_cycle_ban() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.assertions = vec![assertion(1, 3, 2, Some(4)), assertion(2, 4, 2, Some(3))];
    assert_eq!(
        changed(&[relation(LinkDirection::Both, None)], &current, &proposed),
        ids(&[3, 4])
    );
}

#[test]
fn authority_impact_many_selectors_share_predicate_node_budget() {
    let graph = graph();
    let selectors = vec![Predicate::All(Vec::new()); 8];
    let mut limits = Limits {
        predicate_nodes: 15,
        ..Limits::default()
    };
    assert_eq!(
        selector_membership_changes(&selectors, &graph, &graph, &limits),
        Err(AuthorityError::LimitExceeded)
    );
    limits.predicate_nodes = 16;
    assert_eq!(
        selector_membership_changes(&selectors, &graph, &graph, &limits),
        Ok(ids(&[]))
    );
    limits.predicate_nodes = 7;
    assert_eq!(
        selector_membership_changes(
            &selectors,
            &GraphSnapshot::default(),
            &GraphSnapshot::default(),
            &limits
        ),
        Err(AuthorityError::LimitExceeded)
    );
}

#[test]
fn authority_impact_graphs_share_content_byte_budget() {
    let mut graph = graph();
    graph.records[0].content = Some(content("abcd"));
    let mut limits = Limits {
        bytes: 7,
        ..Limits::default()
    };
    assert_eq!(
        selector_membership_changes(&[Predicate::All(Vec::new())], &graph, &graph, &limits),
        Err(AuthorityError::LimitExceeded)
    );
    limits.bytes = 8;
    assert_eq!(
        selector_membership_changes(&[Predicate::All(Vec::new())], &graph, &graph, &limits),
        Ok(ids(&[]))
    );
}

#[test]
fn authority_impact_selector_literals_share_byte_budget() {
    let graph = graph();
    let selectors = vec![Predicate::UidEq(r(1)); 3];
    let mut limits = Limits {
        bytes: r(1).len() * 6 - 1,
        ..Limits::default()
    };
    assert_eq!(
        selector_membership_changes(&selectors, &graph, &graph, &limits),
        Err(AuthorityError::LimitExceeded)
    );
    limits.bytes += 1;
    assert_eq!(
        selector_membership_changes(&selectors, &graph, &graph, &limits),
        Ok(ids(&[]))
    );
}

#[test]
fn authority_impact_output_bytes_refuse_instead_of_returning_partial_set() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.records[0].deleted = true;
    proposed.records[1].deleted = true;
    let mut limits = Limits {
        bytes: r(1).len() * 2 - 1,
        ..Limits::default()
    };
    assert_eq!(
        selector_membership_changes(&[Predicate::All(Vec::new())], &current, &proposed, &limits),
        Err(AuthorityError::LimitExceeded)
    );
    limits.bytes += 1;
    assert_eq!(
        selector_membership_changes(&[Predicate::All(Vec::new())], &current, &proposed, &limits),
        Ok(ids(&[1, 2]))
    );
}

#[test]
fn authority_impact_graphs_share_edge_budget() {
    let graph = graph();
    let mut limits = Limits {
        graph_edges: 1,
        ..Limits::default()
    };
    assert_eq!(
        selector_membership_changes(&[], &graph, &graph, &limits),
        Err(AuthorityError::LimitExceeded)
    );
    limits.graph_edges = 2;
    assert_eq!(
        selector_membership_changes(&[], &graph, &graph, &limits),
        Ok(ids(&[]))
    );
}

#[test]
fn authority_impact_record_count_and_predicate_depth_are_bounded() {
    let graph = graph();
    let limits = Limits {
        records: 5,
        ..Limits::default()
    };
    assert_eq!(
        selector_membership_changes(&[], &graph, &graph, &limits),
        Err(AuthorityError::LimitExceeded)
    );
    let limits = Limits {
        predicate_depth: 1,
        ..Limits::default()
    };
    assert_eq!(
        selector_membership_changes(
            &[Predicate::Not(Box::new(Predicate::Not(Box::new(
                Predicate::All(Vec::new())
            ))))],
            &graph,
            &graph,
            &limits
        ),
        Err(AuthorityError::LimitExceeded)
    );
}

#[test]
fn authority_impact_all_comparisons_share_one_step_budget() {
    let current = graph();
    let mut proposed = current.clone();
    proposed.records[0].deleted = true;
    let one = vec![Predicate::All(Vec::new())];
    let many = vec![Predicate::All(Vec::new()); 8];
    let limits = Limits {
        steps: 90,
        ..Limits::default()
    };
    assert_eq!(
        selector_membership_changes(&one, &current, &proposed, &limits),
        Ok(ids(&[1]))
    );
    assert_eq!(
        selector_membership_changes(&many, &current, &proposed, &limits),
        Err(AuthorityError::LimitExceeded)
    );
    for selector in many {
        assert_eq!(
            selector_membership_changes(&[selector], &current, &proposed, &limits),
            Ok(ids(&[1]))
        );
    }
}

#[test]
fn authority_impact_empty_selectors_and_unchanged_graphs_have_empty_effects() {
    let current = hierarchy();
    let mut proposed = current.clone();
    proposed.records[0].deleted = true;
    assert_eq!(changed(&[], &current, &proposed), ids(&[]));
    assert_eq!(
        changed(
            &[
                under(false),
                Predicate::ConceptIn(c(4)),
                Predicate::UidEq(r(1))
            ],
            &current,
            &current
        ),
        ids(&[])
    );
}

#[test]
fn authority_impact_matches_independent_read_sets_without_graph_visibility_loss() {
    let current = hierarchy();
    let mut proposed = current.clone();
    proposed.assertions[0].object_uid = Some(r(2));
    proposed.assertions.push(assertion(4, 6, 1, None));
    proposed.records[4].deleted = true;
    let selectors = vec![
        under(false),
        Predicate::ConceptIn(c(1)),
        Predicate::Not(Box::new(Predicate::UidEq(r(6)))),
    ];
    let ceiling = VisibilityCeiling {
        records: current
            .records
            .iter()
            .map(|record| record.uid.clone())
            .collect(),
        concepts: current
            .concepts
            .iter()
            .map(|concept| concept.uid.clone())
            .collect(),
        places: BTreeSet::new(),
    };
    let mut expected = BTreeSet::new();
    for selector in &selectors {
        let policy = RolePolicy {
            read: selector.clone(),
            grants: Vec::new(),
        };
        let before =
            readable_records(Some(&policy), &current, &ceiling, &Limits::default()).unwrap();
        let after =
            readable_records(Some(&policy), &proposed, &ceiling, &Limits::default()).unwrap();
        expected.extend(before.symmetric_difference(&after).cloned());
    }
    assert_eq!(changed(&selectors, &current, &proposed), expected);
    assert_eq!(expected, ids(&[3, 4, 5, 6]));
}
