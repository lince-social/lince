use nucleus::RecordKind;
use protein::{
    LinkDirection, Predicate,
    authority::{AssertionRole, AssertionState},
    record_query::QueryError,
};

use super::fixture::{ScalarDetails, a, c, decimal, fixture, query, r, scalar};

#[test]
fn inaccessible_operands_refuse_even_in_unreached_branches() {
    let fixture = fixture();
    for predicate in [
        Predicate::UidEq(r(4)),
        Predicate::ConceptIn(c(5)),
        Predicate::Relation {
            kind: c(3),
            direction: LinkDirection::Both,
            other: Some(r(4)),
        },
        Predicate::Under {
            record: r(5),
            kind: c(4),
            include_self: false,
        },
        Predicate::Under {
            record: r(3),
            kind: c(5),
            include_self: false,
        },
        Predicate::OrganEq(r(10)),
        Predicate::OrganEq(r(1)),
        Predicate::OrganEq(r(99)),
        Predicate::OrganIn(vec![r(0), r(10)]),
        Predicate::UidEq(r(99)),
        Predicate::ConceptIn(c(99)),
    ] {
        let guarded = Predicate::Any(vec![Predicate::UidEq(r(1)), predicate]);
        assert_eq!(
            fixture.run(&query(vec![guarded])),
            Err(QueryError::Unavailable)
        );
    }
    assert_eq!(
        fixture.run(&query(vec![Predicate::UidEq("r_bad".into())])),
        Err(QueryError::InvalidInput)
    );
}

#[test]
fn hidden_assertions_and_intermediate_records_never_make_a_visible_match() {
    let visible = fixture();
    assert!(
        visible
            .selected(vec![
                Predicate::UidEq(r(6)),
                Predicate::Under {
                    record: r(3),
                    kind: c(4),
                    include_self: false,
                },
            ])
            .is_empty()
    );
    assert!(
        visible
            .selected(vec![
                Predicate::UidEq(r(6)),
                Predicate::Relation {
                    kind: c(4),
                    direction: LinkDirection::Both,
                    other: Some(r(3)),
                },
            ])
            .is_empty()
    );
    let mut inconsistent = fixture();
    inconsistent.readable.assertions.insert(a(5));
    assert_eq!(
        inconsistent.run(&query(Vec::new())),
        Err(QueryError::InvalidInput)
    );
}

#[test]
fn visible_relation_cycles_refuse_under_without_banning_other_queries() {
    let mut fixture = fixture();
    fixture.graph.assertions.push(AssertionState {
        uid: a(10),
        subject_uid: r(3),
        predicate_uid: c(4),
        object_uid: Some(r(1)),
        object_uri: None,
        role: AssertionRole::Ordinary,
        quantity: None,
        unit_uid: None,
    });
    fixture.readable.assertions.insert(a(10));
    assert_eq!(
        fixture.run(&query(vec![Predicate::Under {
            record: r(3),
            kind: c(4),
            include_self: false,
        }])),
        Err(QueryError::CyclicGraph)
    );
    assert_eq!(fixture.selected(vec![Predicate::UidEq(r(2))]), vec![r(2)]);
}

#[test]
fn concept_parent_cycles_refuse_the_supplied_visible_graph() {
    let mut fixture = fixture();
    fixture.graph.concepts[0].parents.insert(c(2));
    assert_eq!(
        fixture.run(&query(Vec::new())),
        Err(QueryError::CyclicGraph)
    );
}

#[test]
fn scalar_materialization_must_be_complete_unique_and_consistent() {
    let mut missing = fixture();
    missing.scalars.pop();
    assert_eq!(
        missing.run(&query(Vec::new())),
        Err(QueryError::InvalidInput)
    );

    let mut duplicate = fixture();
    duplicate.scalars.push(duplicate.scalars[0].clone());
    assert_eq!(
        duplicate.run(&query(Vec::new())),
        Err(QueryError::InvalidInput)
    );

    let mut wrong_kind = fixture();
    wrong_kind.scalars[1].kind = RecordKind::Thread;
    assert_eq!(
        wrong_kind.run(&query(Vec::new())),
        Err(QueryError::InvalidInput)
    );

    let mut wrong_content = fixture();
    wrong_content.scalars[1].body = "changed".into();
    assert_eq!(
        wrong_content.run(&query(Vec::new())),
        Err(QueryError::InvalidInput)
    );

    let mut hidden_row = fixture();
    hidden_row.scalars.push(scalar(
        4,
        RecordKind::Plain,
        "hidden",
        "A Hidden",
        "2026-09-01T10:00:00Z",
        ScalarDetails {
            body: "Needle",
            quantity: decimal("5"),
            start_date: Some("2026-09-01"),
            ..Default::default()
        },
    ));
    assert_eq!(
        hidden_row.run(&query(Vec::new())),
        Err(QueryError::InvalidInput)
    );
}

#[test]
fn graph_identity_reference_and_shape_corruption_refuses() {
    let mut duplicate = fixture();
    duplicate
        .graph
        .records
        .push(duplicate.graph.records[0].clone());
    assert_eq!(
        duplicate.run(&query(Vec::new())),
        Err(QueryError::InvalidInput)
    );

    let mut missing = fixture();
    missing.graph.records[1].organ_uid = Some(r(99));
    assert_eq!(
        missing.run(&query(Vec::new())),
        Err(QueryError::InvalidInput)
    );

    let mut wrong = fixture();
    wrong.graph.assertions[0].object_uid = Some(r(2));
    assert_eq!(wrong.run(&query(Vec::new())), Err(QueryError::InvalidInput));

    let mut deleted = fixture();
    deleted.graph.records[1].deleted = true;
    assert_eq!(
        deleted.run(&query(Vec::new())),
        Err(QueryError::InvalidInput)
    );
}
