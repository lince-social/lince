use std::collections::BTreeSet;

use nucleus::{DecimalValue, RecordKind};
use protein::{
    Aggregate, AggregateOp, ExtensionInclude, GroupBy, Order, Predicate,
    record_query::{CountGroup, CountRow, QueryRows, RecordField},
};

use super::fixture::{ScalarDetails, c, decimal, fixture, minimal, query, r, records, scalar};

#[test]
fn filtering_precedes_count_order_and_limit() {
    let fixture = fixture();
    let mut request = query(vec![Predicate::TextContains("needle".into())]);
    request.order = vec![Order::Asc("head".into())];
    request.limit = Some(1);
    assert_eq!(records(fixture.run(&request).unwrap()), vec![r(1)]);
    request.aggregate = Some(Aggregate {
        op: AggregateOp::Count,
        by: GroupBy::Total,
    });
    request.order.clear();
    request.limit = None;
    request.fields = None;
    assert_eq!(
        fixture.run(&request).expect("valid count request").rows,
        QueryRows::Counts(vec![CountRow {
            group: CountGroup::Total,
            count: 1,
        }])
    );
}

#[test]
fn every_scalar_order_is_typed_with_uid_as_the_final_tie() {
    let first = scalar(
        20,
        RecordKind::Thread,
        "zulu",
        "Same",
        "2026-09-05T10:00:00+01:00",
        ScalarDetails {
            quantity: decimal("2"),
            start_date: Some("2026-09-03"),
            due_date: Some("2026-09-10"),
            updated_at: Some("2026-09-07T00:00:00Z"),
            ..Default::default()
        },
    );
    let second = scalar(
        21,
        RecordKind::Plain,
        "alpha",
        "Same",
        "2026-09-05T08:30:00Z",
        ScalarDetails {
            quantity: decimal("1"),
            start_date: Some("2026-09-02"),
            due_date: Some("2026-09-11"),
            updated_at: Some("2026-09-06T00:00:00Z"),
            ..Default::default()
        },
    );
    let missing = scalar(
        22,
        RecordKind::Plain,
        "middle",
        "Same",
        "2026-09-08T00:00:00Z",
        ScalarDetails {
            quantity: decimal("1"),
            ..Default::default()
        },
    );
    let fixture = minimal(vec![first, second, missing]);
    for (field, expected) in [
        ("head", vec![r(20), r(21), r(22)]),
        ("slug", vec![r(21), r(22), r(20)]),
        ("kind", vec![r(21), r(22), r(20)]),
        ("quantity", vec![r(21), r(22), r(20)]),
        ("start_date", vec![r(21), r(20), r(22)]),
        ("due_date", vec![r(20), r(21), r(22)]),
        ("created_at", vec![r(21), r(20), r(22)]),
        ("updated_at", vec![r(21), r(20), r(22)]),
    ] {
        let mut request = query(Vec::new());
        request.order = vec![Order::Asc(field.into())];
        assert_eq!(records(fixture.run(&request).unwrap()), expected);
    }
    for field in ["start_date", "due_date"] {
        let mut request = query(Vec::new());
        request.order = vec![Order::Desc(field.into())];
        let ordered = records(fixture.run(&request).unwrap());
        assert_eq!(ordered.last(), Some(&r(22)));
    }
}

#[test]
fn quantity_filters_and_order_keep_full_decimal_precision() {
    let quantity_row = |number, slug, head, quantity| {
        scalar(
            number,
            RecordKind::Plain,
            slug,
            head,
            "2026-09-01T00:00:00Z",
            ScalarDetails {
                quantity,
                ..Default::default()
            },
        )
    };
    let lower = quantity_row(20, "lower", "Lower", decimal("9007199254740992"));
    let higher = quantity_row(21, "higher", "Higher", decimal("9007199254740993"));
    let tiny = quantity_row(22, "tiny", "Tiny", decimal("0.000000000000000001"));
    let fixture = minimal(vec![higher, tiny, lower]);
    assert_eq!(
        fixture.selected(vec![Predicate::QuantityGt(decimal("9007199254740992"))]),
        vec![r(21)]
    );
    let mut request = query(Vec::new());
    request.order = vec![Order::Asc("quantity".into())];
    assert_eq!(
        records(fixture.run(&request).unwrap()),
        vec![r(22), r(20), r(21)]
    );

    let minimum = DecimalValue::from_mantissa(0, i128::MIN).unwrap();
    let next = DecimalValue::from_mantissa(0, i128::MIN + 1).unwrap();
    let minimum_row = quantity_row(30, "minimum", "Minimum", minimum);
    let next_row = quantity_row(31, "next", "Next", next);
    let extremes = minimal(vec![minimum_row, next_row]);
    assert_eq!(
        extremes.selected(vec![Predicate::QuantityGt(minimum)]),
        vec![r(31)]
    );
}

#[test]
fn count_aggregates_are_typed_and_use_only_visible_identity_assertions() {
    let fixture = fixture();
    for (by, expected) in [
        (
            GroupBy::Total,
            vec![CountRow {
                group: CountGroup::Total,
                count: 8,
            }],
        ),
        (
            GroupBy::Kind,
            vec![
                CountRow {
                    group: CountGroup::Kind(RecordKind::Organ),
                    count: 1,
                },
                CountRow {
                    group: CountGroup::Kind(RecordKind::Person),
                    count: 1,
                },
                CountRow {
                    group: CountGroup::Kind(RecordKind::Plain),
                    count: 5,
                },
                CountRow {
                    group: CountGroup::Kind(RecordKind::Thread),
                    count: 1,
                },
            ],
        ),
        (
            GroupBy::Concept,
            vec![
                CountRow {
                    group: CountGroup::Concept(Some(c(1))),
                    count: 1,
                },
                CountRow {
                    group: CountGroup::Concept(Some(c(2))),
                    count: 1,
                },
                CountRow {
                    group: CountGroup::Concept(None),
                    count: 6,
                },
            ],
        ),
    ] {
        let mut request = query(Vec::new());
        request.aggregate = Some(Aggregate {
            op: AggregateOp::Count,
            by,
        });
        assert_eq!(
            fixture.run(&request).expect("valid count request").rows,
            QueryRows::Counts(expected)
        );
    }
}

#[test]
fn projection_is_explicit_and_never_returns_raw_content() {
    let fixture = fixture();
    let mut request = query(vec![Predicate::UidEq(r(1))]);
    request.fields = Some(vec!["head".into(), "assertions".into()]);
    request.include.extension = Some(ExtensionInclude {
        namespace: "custom".into(),
    });
    let result = fixture.run(&request).unwrap();
    assert_eq!(result.rows, QueryRows::Records(vec![r(1)]));
    assert_eq!(
        result.projection.fields,
        BTreeSet::from([
            RecordField::Uid,
            RecordField::Kind,
            RecordField::Head,
            RecordField::Assertions,
        ])
    );
    assert_eq!(
        result.projection.extension_namespace.as_deref(),
        Some("custom")
    );
    let default = fixture.run(&query(vec![Predicate::UidEq(r(1))])).unwrap();
    assert!(!default.projection.fields.contains(&RecordField::Assertions));
    assert!(!default.projection.fields.contains(&RecordField::Body));
    assert!(default.projection.fields.contains(&RecordField::Concept));
    assert!(default.projection.fields.contains(&RecordField::Organ));
    assert!(default.projection.fields.contains(&RecordField::StartDate));
    assert!(default.projection.fields.contains(&RecordField::Revision));

    let mut complete = query(vec![Predicate::UidEq(r(1))]);
    complete.fields = Some(
        [
            "uid",
            "kind",
            "slug",
            "head",
            "body",
            "quantity",
            "concept",
            "unit",
            "place",
            "organ",
            "start_date",
            "due_date",
            "created_at",
            "updated_at",
            "revision",
            "assertions",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
    );
    assert_eq!(
        fixture.run(&complete).unwrap().projection.fields,
        BTreeSet::from([
            RecordField::Uid,
            RecordField::Kind,
            RecordField::Slug,
            RecordField::Head,
            RecordField::Body,
            RecordField::Quantity,
            RecordField::Concept,
            RecordField::Unit,
            RecordField::Place,
            RecordField::Organ,
            RecordField::StartDate,
            RecordField::DueDate,
            RecordField::CreatedAt,
            RecordField::UpdatedAt,
            RecordField::Revision,
            RecordField::Assertions,
        ])
    );
}
