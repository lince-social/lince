use protein_authority::{
    authority::{AuthorityError, GraphSnapshot, Limits, selector_membership_changes},
    query::Predicate,
};

use super::fixtures::{graph, ids};
use crate::fixtures::{document, r};

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
    graph.records[0].content = Some(document("abcd".into(), ""));
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
