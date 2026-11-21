use std::collections::BTreeSet;

use protein::{
    Aggregate, AggregateOp, DateComparison, ExtensionInclude, GroupBy, LinkEndpoint, LinkOrder,
    Order, Predicate, Source, WorkDateField,
    authority::{AssertionRole, AssertionState, ConceptState},
    record_query::{QueryError, QueryLimits},
};
use serde_json::json;

use super::fixture::{a, c, fixture, pl, query, r, records};

#[test]
fn unsupported_sources_predicates_orders_aggregates_and_includes_refuse_upfront() {
    let fixture = fixture();
    for unsupported in [
        Source::Promise,
        Source::Decision,
        Source::Fact,
        Source::Concept,
        Source::Lingua,
        Source::Assertion,
        Source::Transfer,
        Source::TransferSettlementPreview,
        Source::TransferBulkCompletionPreview,
        Source::Auth,
        Source::Karma,
        Source::Timeline,
        Source::Entry,
        Source::Frequency,
        Source::Recurrence,
        Source::Nearby,
    ] {
        let mut source = query(Vec::new());
        source.source = unsupported;
        assert_eq!(fixture.run(&source), Err(QueryError::Unsupported));
    }

    for predicate in [
        Predicate::OccurrenceIn(vec!["open".into()]),
        Predicate::StateIn(vec!["open".into()]),
        Predicate::RevisionEq(1),
        Predicate::RevisionLt(1),
        Predicate::RevisionLte(1),
        Predicate::RevisionGt(1),
        Predicate::RevisionGte(1),
        Predicate::StatusIn(vec!["open".into()]),
        Predicate::ViewerRoleIn(vec!["admin".into()]),
        Predicate::InvitationStateIn(vec!["open".into()]),
        Predicate::PersonEq(r(7)),
        Predicate::UnitEq(c(1)),
        Predicate::WindowEndBefore("2026-09-01T00:00:00Z".into()),
        Predicate::WindowEndAfter("2026-09-01T00:00:00Z".into()),
        Predicate::AtSince("2026-09-01T00:00:00Z".into()),
        Predicate::AtBefore("2026-09-01T00:00:00Z".into()),
        Predicate::ClassifiedIn(c(1)),
        Predicate::CauseKindEq("edit".into()),
        Predicate::RecordEq(r(1)),
        Predicate::Near {
            of: pl(1),
            meters: 1.0,
        },
    ] {
        let request = query(vec![Predicate::Any(vec![
            Predicate::UidEq(r(1)),
            predicate,
        ])]);
        assert_eq!(fixture.run(&request), Err(QueryError::Unsupported));
    }

    for order in [
        Order::Asc("uid".into()),
        Order::Asc("unknown".into()),
        Order::Link(LinkOrder {
            kind: c(4),
            higher: LinkEndpoint::From,
        }),
    ] {
        let mut request = query(Vec::new());
        request.order = vec![order];
        assert_eq!(fixture.run(&request), Err(QueryError::Unsupported));
    }

    let mut aggregates = vec![Aggregate {
        op: AggregateOp::Sum,
        by: GroupBy::Total,
    }];
    aggregates.extend(
        [
            GroupBy::Classification,
            GroupBy::CauseKind,
            GroupBy::Day,
            GroupBy::Month,
        ]
        .into_iter()
        .map(|by| Aggregate {
            op: AggregateOp::Count,
            by,
        }),
    );
    for aggregate in aggregates {
        let mut request = query(Vec::new());
        request.aggregate = Some(aggregate);
        assert_eq!(fixture.run(&request), Err(QueryError::Unsupported));
    }

    let include_values = [
        json!({"facts": {}}),
        json!({"promises": {}}),
        json!({"links": {}}),
        json!({"threads": {}}),
        json!({"availability": true}),
        json!({"projection": {"at": "2026-09-01T00:00:00Z"}}),
        json!({"contact": true}),
        json!({"conversations": true}),
        json!({"reference_reads": true}),
    ];
    for include in include_values {
        let mut request = query(Vec::new());
        request.include = serde_json::from_value(include).expect("valid include fixture");
        assert_eq!(fixture.run(&request), Err(QueryError::Unsupported));
    }

    let mut field = query(Vec::new());
    field.fields = Some(vec!["unknown".into()]);
    assert_eq!(fixture.run(&field), Err(QueryError::Unsupported));

    let mut extension = query(Vec::new());
    extension.include.extension = Some(ExtensionInclude {
        namespace: String::new(),
    });
    assert_eq!(fixture.run(&extension), Err(QueryError::InvalidInput));
}

#[test]
fn malformed_dates_and_duplicate_projection_or_order_fields_refuse() {
    let fixture = fixture();
    for value in ["2026-9-01", "2026-02-30", "not-a-date"] {
        let request = query(vec![Predicate::WorkDate {
            field: WorkDateField::Start,
            op: DateComparison::Eq,
            value: Some(value.into()),
        }]);
        assert_eq!(fixture.run(&request), Err(QueryError::InvalidInput));
    }
    let request = query(vec![Predicate::WorkDate {
        field: WorkDateField::Start,
        op: DateComparison::Exists,
        value: Some("2026-09-01".into()),
    }]);
    assert_eq!(fixture.run(&request), Err(QueryError::InvalidInput));
    assert_eq!(
        fixture.run(&query(vec![Predicate::KindEq("not-a-record-kind".into())])),
        Err(QueryError::InvalidInput)
    );

    let mut request = query(Vec::new());
    request.fields = Some(vec!["head".into(), "head".into()]);
    assert_eq!(fixture.run(&request), Err(QueryError::InvalidInput));
    let mut request = query(Vec::new());
    request.order = vec![Order::Asc("head".into()), Order::Desc("head".into())];
    assert_eq!(fixture.run(&request), Err(QueryError::InvalidInput));
}

#[test]
fn hard_limits_cover_limits_graph_predicates_strings_steps_and_results() {
    let base = fixture();
    let request = query(vec![Predicate::All(vec![Predicate::All(vec![
        Predicate::UidEq(r(1)),
    ])])]);
    for limits in [
        QueryLimits {
            records: 10,
            ..Default::default()
        },
        QueryLimits {
            assertions: 7,
            ..Default::default()
        },
        QueryLimits {
            predicate_nodes: 1,
            ..Default::default()
        },
        QueryLimits {
            predicate_depth: 1,
            ..Default::default()
        },
        QueryLimits {
            graph_edges: 1,
            ..Default::default()
        },
        QueryLimits {
            string_bytes: 27,
            ..Default::default()
        },
        QueryLimits {
            bytes: 100,
            ..Default::default()
        },
        QueryLimits {
            steps: 10,
            ..Default::default()
        },
    ] {
        assert_eq!(
            base.run_with_limits(&request, &limits),
            Err(QueryError::LimitExceeded)
        );
    }

    let mut order = query(Vec::new());
    order.order = vec![Order::Asc("head".into()), Order::Asc("slug".into())];
    let limits = QueryLimits {
        order_fields: 1,
        ..Default::default()
    };
    assert_eq!(
        base.run_with_limits(&order, &limits),
        Err(QueryError::LimitExceeded)
    );

    let mut projection = query(Vec::new());
    projection.fields = Some(vec!["head".into(), "body".into()]);
    let limits = QueryLimits {
        projection_fields: 1,
        ..Default::default()
    };
    assert_eq!(
        base.run_with_limits(&projection, &limits),
        Err(QueryError::LimitExceeded)
    );

    let mut long_operand = query(vec![Predicate::TextContains("x".repeat(33))]);
    long_operand.limit = Some(1);
    let limits = QueryLimits {
        string_bytes: 32,
        ..Default::default()
    };
    assert_eq!(
        base.run_with_limits(&long_operand, &limits),
        Err(QueryError::LimitExceeded)
    );

    let mut hidden_payload = fixture();
    hidden_payload.graph.records[4]
        .content
        .as_mut()
        .expect("hidden Record fixture has content")
        .body = "x".repeat(65);
    let limits = QueryLimits {
        string_bytes: 64,
        ..Default::default()
    };
    assert_eq!(
        hidden_payload.run_with_limits(&query(Vec::new()), &limits),
        Err(QueryError::LimitExceeded)
    );

    let limits = QueryLimits {
        results: 1,
        ..Default::default()
    };
    assert_eq!(
        base.run_with_limits(&query(Vec::new()), &limits),
        Err(QueryError::LimitExceeded)
    );
    let mut limited = query(Vec::new());
    limited.limit = Some(1);
    assert_eq!(
        records(base.run_with_limits(&limited, &limits).unwrap()).len(),
        1
    );
}

#[test]
fn concept_membership_charges_every_assertion_in_eager_branches() {
    let mut fixture = fixture();
    for number in 100..228 {
        let predicate_uid = c(number);
        fixture.graph.concepts.push(ConceptState {
            uid: predicate_uid.clone(),
            name: format!("scan predicate {number}"),
            parents: BTreeSet::new(),
        });
        fixture.readable.concepts.insert(predicate_uid.clone());
        let assertion = AssertionState {
            uid: a(number),
            subject_uid: r(2),
            predicate_uid,
            object_uid: Some(r(3)),
            object_uri: None,
            role: AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
        };
        fixture.readable.assertions.insert(assertion.uid.clone());
        fixture.graph.assertions.push(assertion);
    }

    let control = query(vec![Predicate::Any(vec![
        Predicate::UidEq(r(1)),
        Predicate::SlugEq("never-first".into()),
        Predicate::SlugEq("never-second".into()),
    ])]);
    let mut lower = 1;
    let mut upper = QueryLimits::default().steps;
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        let limits = QueryLimits {
            steps: middle,
            ..Default::default()
        };
        match fixture.run_with_limits(&control, &limits) {
            Ok(_) => upper = middle,
            Err(QueryError::LimitExceeded) => lower = middle + 1,
            result => panic!("unexpected control result: {result:?}"),
        }
    }

    let limits = QueryLimits {
        steps: upper + 32,
        ..Default::default()
    };
    assert!(fixture.run_with_limits(&query(Vec::new()), &limits).is_ok());
    assert!(fixture.run_with_limits(&control, &limits).is_ok());

    let scanned = query(vec![Predicate::Any(vec![
        Predicate::UidEq(r(1)),
        Predicate::ConceptIn(c(1)),
        Predicate::ConceptIn(c(1)),
    ])]);
    assert_eq!(
        fixture.run_with_limits(&scanned, &limits),
        Err(QueryError::LimitExceeded)
    );
}

#[test]
fn caller_limits_cannot_disable_or_expand_the_hard_ceiling() {
    let fixture = fixture();
    let zero = QueryLimits {
        steps: 0,
        ..Default::default()
    };
    assert_eq!(
        fixture.run_with_limits(&query(Vec::new()), &zero),
        Err(QueryError::InvalidLimits)
    );
    let expanded = QueryLimits {
        bytes: QueryLimits::default().bytes + 1,
        ..Default::default()
    };
    assert_eq!(
        fixture.run_with_limits(&query(Vec::new()), &expanded),
        Err(QueryError::InvalidLimits)
    );
}

#[test]
fn errors_are_content_free() {
    for error in [
        QueryError::InvalidLimits,
        QueryError::LimitExceeded,
        QueryError::InvalidInput,
        QueryError::Unavailable,
        QueryError::Unsupported,
        QueryError::CyclicGraph,
    ] {
        let text = error.to_string();
        assert!(!text.contains(&r(4)));
        assert!(!text.contains("Needle"));
        assert!(!text.contains("secret"));
    }
}
