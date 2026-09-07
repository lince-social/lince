use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, FixedOffset, NaiveDate};
use nucleus::{DecimalValue, RecordKind};
use protein::authority::{
    AssertionRole, AssertionState, ConceptState, ExtensionProperty, GraphSnapshot, RecordContent,
    RecordState,
};
use protein::record_query::{
    CountGroup, QueryError, QueryLimits, QueryResult, QueryRows, ReadableIdentities, RecordField,
    RecordScalarRow, select_records,
};
use protein::{
    Aggregate, AggregateOp, DateComparison, ExtensionInclude, GroupBy, Include, LinkDirection,
    LinkEndpoint, LinkOrder, Order, Predicate, Protein, Source, WorkDateField,
};
use serde_json::json;

fn uid(prefix: &str, number: usize) -> String {
    format!("{prefix}_{number:026}")
}

fn r(number: usize) -> String {
    uid("r", number)
}

fn c(number: usize) -> String {
    uid("c", number)
}

fn a(number: usize) -> String {
    uid("a", number)
}

fn pl(number: usize) -> String {
    uid("pl", number)
}

fn decimal(value: &str) -> DecimalValue {
    DecimalValue::parse_inferred(value).unwrap()
}

fn date(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
}

fn instant(value: &str) -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339(value).unwrap()
}

fn scalar(
    number: usize,
    kind: RecordKind,
    slug: &str,
    head: &str,
    body: &str,
    quantity: &str,
    start_date: Option<&str>,
    due_date: Option<&str>,
    created_at: &str,
    updated_at: &str,
) -> RecordScalarRow {
    RecordScalarRow {
        uid: r(number),
        kind,
        slug: Some(slug.into()),
        head: head.into(),
        body: body.into(),
        quantity: decimal(quantity),
        start_date: start_date.map(date),
        due_date: due_date.map(date),
        created_at: instant(created_at),
        updated_at: instant(updated_at),
    }
}

fn record_from_scalar(row: &RecordScalarRow, organ_uid: Option<String>) -> RecordState {
    RecordState {
        uid: row.uid.clone(),
        kind: row.kind,
        organ_uid,
        deleted: false,
        content: Some(RecordContent {
            slug: row.slug.clone(),
            head: row.head.clone(),
            body: row.body.clone(),
            quantity: row.quantity,
            unit_uid: None,
            place_uid: None,
            extensions: BTreeMap::new(),
        }),
    }
}

struct Fixture {
    graph: GraphSnapshot,
    readable: ReadableIdentities,
    scalars: Vec<RecordScalarRow>,
}

fn fixture() -> Fixture {
    let organ = scalar(
        0,
        RecordKind::Organ,
        "company",
        "Company",
        "",
        "0",
        None,
        None,
        "2026-09-01T09:00:00Z",
        "2026-09-01T09:00:00Z",
    );
    let task = scalar(
        1,
        RecordKind::Plain,
        "task.alpha",
        "Alpha",
        "Visible Needle",
        "1.000",
        Some("2026-09-03"),
        Some("2026-09-10"),
        "2026-09-05T10:00:00+01:00",
        "2026-09-06T12:00:00Z",
    );
    let second = scalar(
        2,
        RecordKind::Plain,
        "task.beta",
        "Beta",
        "ordinary",
        "2",
        None,
        Some("2026-09-08"),
        "2026-09-04T10:00:00Z",
        "2026-09-07T12:00:00Z",
    );
    let root = scalar(
        3,
        RecordKind::Plain,
        "project",
        "Project",
        "",
        "0",
        None,
        None,
        "2026-09-03T10:00:00Z",
        "2026-09-03T10:00:00Z",
    );
    let hidden = scalar(
        4,
        RecordKind::Plain,
        "hidden",
        "A Hidden",
        "Needle",
        "5",
        Some("2026-09-01"),
        None,
        "2026-09-01T10:00:00Z",
        "2026-09-01T10:00:00Z",
    );
    let intermediate = scalar(
        5,
        RecordKind::Plain,
        "hidden.intermediate",
        "Hidden intermediate",
        "",
        "0",
        None,
        None,
        "2026-09-02T10:00:00Z",
        "2026-09-02T10:00:00Z",
    );
    let leaf = scalar(
        6,
        RecordKind::Plain,
        "leaf",
        "Leaf",
        "",
        "0",
        None,
        None,
        "2026-09-06T10:00:00Z",
        "2026-09-06T10:00:00Z",
    );
    let person = scalar(
        7,
        RecordKind::Person,
        "person",
        "Person",
        "",
        "0",
        None,
        None,
        "2026-09-02T11:00:00Z",
        "2026-09-02T11:00:00Z",
    );
    let locked = scalar(
        8,
        RecordKind::Plain,
        "locked",
        "Locked",
        "  lince-vault.v1 malformed Needle",
        "0",
        None,
        None,
        "2026-09-07T10:00:00Z",
        "2026-09-07T10:00:00Z",
    );
    let uncategorized = scalar(
        9,
        RecordKind::Thread,
        "uncategorized",
        "Uncategorized",
        "",
        "0",
        None,
        None,
        "2026-09-08T10:00:00Z",
        "2026-09-08T10:00:00Z",
    );
    let hidden_organ = scalar(
        10,
        RecordKind::Organ,
        "hidden.company",
        "Hidden company",
        "",
        "0",
        None,
        None,
        "2026-09-01T08:00:00Z",
        "2026-09-01T08:00:00Z",
    );

    let mut records = vec![
        record_from_scalar(&organ, Some(r(0))),
        record_from_scalar(&task, Some(r(0))),
        record_from_scalar(&second, Some(r(0))),
        record_from_scalar(&root, Some(r(0))),
        record_from_scalar(&hidden, Some(r(10))),
        record_from_scalar(&intermediate, Some(r(0))),
        record_from_scalar(&leaf, Some(r(0))),
        record_from_scalar(&person, Some(r(0))),
        record_from_scalar(&locked, Some(r(0))),
        record_from_scalar(&uncategorized, Some(r(0))),
        record_from_scalar(&hidden_organ, Some(r(10))),
    ];
    let task_content = records[1].content.as_mut().unwrap();
    task_content.unit_uid = Some(c(1));
    task_content.place_uid = Some(pl(1));
    task_content.extensions.insert(
        ExtensionProperty {
            namespace: "custom".into(),
            field: "priority".into(),
        },
        json!(3),
    );

    let concepts = vec![
        ConceptState {
            uid: c(1),
            name: "work".into(),
            parents: BTreeSet::new(),
        },
        ConceptState {
            uid: c(2),
            name: "task".into(),
            parents: BTreeSet::from([c(1)]),
        },
        ConceptState {
            uid: c(3),
            name: "assigned".into(),
            parents: BTreeSet::new(),
        },
        ConceptState {
            uid: c(4),
            name: "contains".into(),
            parents: BTreeSet::new(),
        },
        ConceptState {
            uid: c(5),
            name: "secret".into(),
            parents: BTreeSet::new(),
        },
        ConceptState {
            uid: c(6),
            name: "assigned directly".into(),
            parents: BTreeSet::from([c(3)]),
        },
    ];
    let assertions = vec![
        AssertionState {
            uid: a(1),
            subject_uid: r(1),
            predicate_uid: c(2),
            object_uid: None,
            role: AssertionRole::Identity,
            quantity: None,
            unit_uid: None,
        },
        AssertionState {
            uid: a(2),
            subject_uid: r(2),
            predicate_uid: c(1),
            object_uid: None,
            role: AssertionRole::Identity,
            quantity: None,
            unit_uid: None,
        },
        AssertionState {
            uid: a(3),
            subject_uid: r(1),
            predicate_uid: c(6),
            object_uid: Some(r(7)),
            role: AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
        },
        AssertionState {
            uid: a(4),
            subject_uid: r(1),
            predicate_uid: c(4),
            object_uid: Some(r(3)),
            role: AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
        },
        AssertionState {
            uid: a(5),
            subject_uid: r(5),
            predicate_uid: c(4),
            object_uid: Some(r(3)),
            role: AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
        },
        AssertionState {
            uid: a(6),
            subject_uid: r(6),
            predicate_uid: c(4),
            object_uid: Some(r(5)),
            role: AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
        },
        AssertionState {
            uid: a(7),
            subject_uid: r(6),
            predicate_uid: c(5),
            object_uid: Some(r(3)),
            role: AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
        },
        AssertionState {
            uid: a(8),
            subject_uid: r(4),
            predicate_uid: c(2),
            object_uid: None,
            role: AssertionRole::Identity,
            quantity: None,
            unit_uid: None,
        },
        AssertionState {
            uid: a(9),
            subject_uid: r(6),
            predicate_uid: c(4),
            object_uid: Some(r(3)),
            role: AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
        },
        AssertionState {
            uid: a(11),
            subject_uid: r(9),
            predicate_uid: c(1),
            object_uid: None,
            role: AssertionRole::Identity,
            quantity: None,
            unit_uid: None,
        },
    ];
    let readable = ReadableIdentities {
        records: [0, 1, 2, 3, 6, 7, 8, 9].into_iter().map(r).collect(),
        concepts: [1, 2, 3, 4, 6].into_iter().map(c).collect(),
        places: BTreeSet::from([pl(1)]),
        assertions: [1, 2, 3, 4].into_iter().map(a).collect(),
    };
    Fixture {
        graph: GraphSnapshot {
            records,
            concepts,
            assertions,
            places: BTreeSet::from([pl(1)]),
        },
        readable,
        scalars: vec![
            organ,
            task,
            second,
            root,
            leaf,
            person,
            locked,
            uncategorized,
        ],
    }
}

fn query(filter: Vec<Predicate>) -> Protein {
    Protein {
        source: Source::Record,
        filter,
        fields: None,
        include: Include::default(),
        aggregate: None,
        order: Vec::new(),
        limit: None,
    }
}

fn run(fixture: &Fixture, query: &Protein) -> Result<QueryResult, QueryError> {
    select_records(
        query,
        &fixture.graph,
        &fixture.readable,
        &fixture.scalars,
        &QueryLimits::default(),
    )
}

fn records(result: QueryResult) -> Vec<String> {
    let QueryRows::Records(records) = result.rows else {
        panic!()
    };
    records
}

fn selected(fixture: &Fixture, filter: Vec<Predicate>) -> Vec<String> {
    records(run(fixture, &query(filter)).unwrap())
}

#[test]
fn every_supported_predicate_validates_inside_every_boolean_shape() {
    let fixture = fixture();
    let true_for_task = vec![
        Predicate::UidEq(r(1)),
        Predicate::KindEq("plain".into()),
        Predicate::SlugEq("task.alpha".into()),
        Predicate::ConceptIn(c(1)),
        Predicate::Relation {
            kind: c(3),
            direction: LinkDirection::Out,
            other: Some(r(7)),
        },
        Predicate::Under {
            record: r(3),
            kind: c(4),
            include_self: false,
        },
        Predicate::TextContains("needle".into()),
        Predicate::QuantityLt(decimal("2")),
        Predicate::QuantityLte(decimal("1")),
        Predicate::QuantityGt(decimal("0.999999999999999999")),
        Predicate::QuantityGte(decimal("1.0")),
        Predicate::QuantityEq(decimal("1")),
        Predicate::WorkDate {
            field: WorkDateField::Start,
            op: DateComparison::Eq,
            value: Some("2026-09-03".into()),
        },
        Predicate::OrganEq(r(0)),
        Predicate::OrganIn(vec![r(0)]),
    ];
    for predicate in true_for_task {
        for wrapped in [
            predicate.clone(),
            Predicate::All(vec![predicate.clone()]),
            Predicate::Any(vec![Predicate::SlugEq("absent".into()), predicate.clone()]),
            Predicate::Not(Box::new(Predicate::Not(Box::new(predicate)))),
        ] {
            assert_eq!(
                selected(&fixture, vec![Predicate::UidEq(r(1)), wrapped]),
                vec![r(1)]
            );
        }
    }
}

#[test]
fn relation_direction_and_work_date_operations_are_exact() {
    let fixture = fixture();
    assert_eq!(
        selected(
            &fixture,
            vec![Predicate::Relation {
                kind: c(3),
                direction: LinkDirection::In,
                other: Some(r(1)),
            }]
        ),
        vec![r(7)]
    );
    for (op, value) in [
        (DateComparison::Eq, "2026-09-03"),
        (DateComparison::Lt, "2026-09-04"),
        (DateComparison::Lte, "2026-09-03"),
        (DateComparison::Gt, "2026-09-02"),
        (DateComparison::Gte, "2026-09-03"),
    ] {
        assert_eq!(
            selected(
                &fixture,
                vec![
                    Predicate::UidEq(r(1)),
                    Predicate::WorkDate {
                        field: WorkDateField::Start,
                        op,
                        value: Some(value.into()),
                    },
                ]
            ),
            vec![r(1)]
        );
    }
    assert_eq!(
        selected(
            &fixture,
            vec![
                Predicate::UidEq(r(1)),
                Predicate::WorkDate {
                    field: WorkDateField::Due,
                    op: DateComparison::Exists,
                    value: None,
                },
            ]
        ),
        vec![r(1)]
    );
    assert_eq!(
        selected(
            &fixture,
            vec![Predicate::Under {
                record: r(3),
                kind: c(4),
                include_self: true,
            }]
        ),
        vec![r(1), r(3)]
    );
}

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
            run(&fixture, &query(vec![guarded])),
            Err(QueryError::Unavailable)
        );
    }
    assert_eq!(
        run(&fixture, &query(vec![Predicate::UidEq("r_bad".into())])),
        Err(QueryError::InvalidInput)
    );
}

#[test]
fn hidden_assertions_and_intermediate_records_never_make_a_visible_match() {
    let visible = fixture();
    assert!(
        selected(
            &visible,
            vec![
                Predicate::UidEq(r(6)),
                Predicate::Under {
                    record: r(3),
                    kind: c(4),
                    include_self: false,
                },
            ]
        )
        .is_empty()
    );
    assert!(
        selected(
            &visible,
            vec![
                Predicate::UidEq(r(6)),
                Predicate::Relation {
                    kind: c(4),
                    direction: LinkDirection::Both,
                    other: Some(r(3)),
                },
            ]
        )
        .is_empty()
    );
    let mut inconsistent = fixture();
    inconsistent.readable.assertions.insert(a(5));
    assert_eq!(
        run(&inconsistent, &query(Vec::new())),
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
        role: AssertionRole::Ordinary,
        quantity: None,
        unit_uid: None,
    });
    fixture.readable.assertions.insert(a(10));
    assert_eq!(
        run(
            &fixture,
            &query(vec![Predicate::Under {
                record: r(3),
                kind: c(4),
                include_self: false,
            }])
        ),
        Err(QueryError::CyclicGraph)
    );
    assert_eq!(selected(&fixture, vec![Predicate::UidEq(r(2))]), vec![r(2)]);
}

#[test]
fn concept_parent_cycles_refuse_the_supplied_visible_graph() {
    let mut fixture = fixture();
    fixture.graph.concepts[0].parents.insert(c(2));
    assert_eq!(
        run(&fixture, &query(Vec::new())),
        Err(QueryError::CyclicGraph)
    );
}

#[test]
fn filtering_precedes_count_order_and_limit() {
    let fixture = fixture();
    let mut request = query(vec![Predicate::TextContains("needle".into())]);
    request.order = vec![Order::Asc("head".into())];
    request.limit = Some(1);
    assert_eq!(records(run(&fixture, &request).unwrap()), vec![r(1)]);
    request.aggregate = Some(Aggregate {
        op: AggregateOp::Count,
        by: GroupBy::Total,
    });
    request.order.clear();
    request.limit = None;
    request.fields = None;
    let QueryRows::Counts(counts) = run(&fixture, &request).unwrap().rows else {
        panic!()
    };
    assert_eq!(
        counts,
        vec![protein::record_query::CountRow {
            group: CountGroup::Total,
            count: 1,
        }]
    );
}

#[test]
fn locked_or_malformed_vault_prefixes_are_never_plaintext_searchable() {
    let fixture = fixture();
    assert_eq!(
        selected(&fixture, vec![Predicate::TextContains("needle".into())]),
        vec![r(1)]
    );
    assert_eq!(
        selected(&fixture, vec![Predicate::TextContains("locked".into())]),
        vec![r(8)]
    );
}

fn minimal(rows: Vec<RecordScalarRow>) -> Fixture {
    let records = rows
        .iter()
        .map(|row| record_from_scalar(row, None))
        .collect();
    let readable = ReadableIdentities {
        records: rows.iter().map(|row| row.uid.clone()).collect(),
        ..ReadableIdentities::default()
    };
    Fixture {
        graph: GraphSnapshot {
            records,
            ..GraphSnapshot::default()
        },
        readable,
        scalars: rows,
    }
}

#[test]
fn every_scalar_order_is_typed_with_uid_as_the_final_tie() {
    let first = scalar(
        20,
        RecordKind::Thread,
        "zulu",
        "Same",
        "",
        "2",
        Some("2026-09-03"),
        Some("2026-09-10"),
        "2026-09-05T10:00:00+01:00",
        "2026-09-07T00:00:00Z",
    );
    let second = scalar(
        21,
        RecordKind::Plain,
        "alpha",
        "Same",
        "",
        "1",
        Some("2026-09-02"),
        Some("2026-09-11"),
        "2026-09-05T08:30:00Z",
        "2026-09-06T00:00:00Z",
    );
    let missing = scalar(
        22,
        RecordKind::Plain,
        "middle",
        "Same",
        "",
        "1",
        None,
        None,
        "2026-09-08T00:00:00Z",
        "2026-09-08T00:00:00Z",
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
        assert_eq!(records(run(&fixture, &request).unwrap()), expected);
    }
    for field in ["start_date", "due_date"] {
        let mut request = query(Vec::new());
        request.order = vec![Order::Desc(field.into())];
        let ordered = records(run(&fixture, &request).unwrap());
        assert_eq!(ordered.last(), Some(&r(22)));
    }
}

#[test]
fn quantity_filters_and_order_keep_full_decimal_precision() {
    let lower = scalar(
        20,
        RecordKind::Plain,
        "lower",
        "Lower",
        "",
        "9007199254740992",
        None,
        None,
        "2026-09-01T00:00:00Z",
        "2026-09-01T00:00:00Z",
    );
    let higher = scalar(
        21,
        RecordKind::Plain,
        "higher",
        "Higher",
        "",
        "9007199254740993",
        None,
        None,
        "2026-09-01T00:00:00Z",
        "2026-09-01T00:00:00Z",
    );
    let tiny = scalar(
        22,
        RecordKind::Plain,
        "tiny",
        "Tiny",
        "",
        "0.000000000000000001",
        None,
        None,
        "2026-09-01T00:00:00Z",
        "2026-09-01T00:00:00Z",
    );
    let fixture = minimal(vec![higher, tiny, lower]);
    assert_eq!(
        selected(
            &fixture,
            vec![Predicate::QuantityGt(decimal("9007199254740992"))]
        ),
        vec![r(21)]
    );
    let mut request = query(Vec::new());
    request.order = vec![Order::Asc("quantity".into())];
    assert_eq!(
        records(run(&fixture, &request).unwrap()),
        vec![r(22), r(20), r(21)]
    );

    let minimum = DecimalValue::from_mantissa(0, i128::MIN).unwrap();
    let next = DecimalValue::from_mantissa(0, i128::MIN + 1).unwrap();
    let minimum_row = scalar(
        30,
        RecordKind::Plain,
        "minimum",
        "Minimum",
        "",
        &minimum.to_string(),
        None,
        None,
        "2026-09-01T00:00:00Z",
        "2026-09-01T00:00:00Z",
    );
    let next_row = scalar(
        31,
        RecordKind::Plain,
        "next",
        "Next",
        "",
        &next.to_string(),
        None,
        None,
        "2026-09-01T00:00:00Z",
        "2026-09-01T00:00:00Z",
    );
    let extremes = minimal(vec![minimum_row, next_row]);
    assert_eq!(
        selected(&extremes, vec![Predicate::QuantityGt(minimum)]),
        vec![r(31)]
    );
}

#[test]
fn count_aggregates_are_typed_and_use_only_visible_identity_assertions() {
    let fixture = fixture();
    for (by, expected) in [
        (
            GroupBy::Total,
            vec![protein::record_query::CountRow {
                group: CountGroup::Total,
                count: 8,
            }],
        ),
        (
            GroupBy::Kind,
            vec![
                protein::record_query::CountRow {
                    group: CountGroup::Kind(RecordKind::Organ),
                    count: 1,
                },
                protein::record_query::CountRow {
                    group: CountGroup::Kind(RecordKind::Person),
                    count: 1,
                },
                protein::record_query::CountRow {
                    group: CountGroup::Kind(RecordKind::Plain),
                    count: 5,
                },
                protein::record_query::CountRow {
                    group: CountGroup::Kind(RecordKind::Thread),
                    count: 1,
                },
            ],
        ),
        (
            GroupBy::Concept,
            vec![
                protein::record_query::CountRow {
                    group: CountGroup::Concept(Some(c(1))),
                    count: 1,
                },
                protein::record_query::CountRow {
                    group: CountGroup::Concept(Some(c(2))),
                    count: 1,
                },
                protein::record_query::CountRow {
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
        let QueryRows::Counts(counts) = run(&fixture, &request).unwrap().rows else {
            panic!()
        };
        assert_eq!(counts, expected);
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
    let result = run(&fixture, &request).unwrap();
    assert_eq!(records(result.clone()), vec![r(1)]);
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
    let default = run(&fixture, &query(vec![Predicate::UidEq(r(1))])).unwrap();
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
        run(&fixture, &complete).unwrap().projection.fields,
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
        assert_eq!(run(&fixture, &source), Err(QueryError::Unsupported));
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
        assert_eq!(run(&fixture, &request), Err(QueryError::Unsupported));
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
        assert_eq!(run(&fixture, &request), Err(QueryError::Unsupported));
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
        assert_eq!(run(&fixture, &request), Err(QueryError::Unsupported));
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
        request.include = serde_json::from_value(include).unwrap();
        assert_eq!(run(&fixture, &request), Err(QueryError::Unsupported));
    }

    let mut field = query(Vec::new());
    field.fields = Some(vec!["unknown".into()]);
    assert_eq!(run(&fixture, &field), Err(QueryError::Unsupported));

    let mut extension = query(Vec::new());
    extension.include.extension = Some(ExtensionInclude {
        namespace: String::new(),
    });
    assert_eq!(run(&fixture, &extension), Err(QueryError::InvalidInput));
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
        assert_eq!(run(&fixture, &request), Err(QueryError::InvalidInput));
    }
    let request = query(vec![Predicate::WorkDate {
        field: WorkDateField::Start,
        op: DateComparison::Exists,
        value: Some("2026-09-01".into()),
    }]);
    assert_eq!(run(&fixture, &request), Err(QueryError::InvalidInput));
    assert_eq!(
        run(
            &fixture,
            &query(vec![Predicate::KindEq("not-a-record-kind".into())])
        ),
        Err(QueryError::InvalidInput)
    );

    let mut request = query(Vec::new());
    request.fields = Some(vec!["head".into(), "head".into()]);
    assert_eq!(run(&fixture, &request), Err(QueryError::InvalidInput));
    let mut request = query(Vec::new());
    request.order = vec![Order::Asc("head".into()), Order::Desc("head".into())];
    assert_eq!(run(&fixture, &request), Err(QueryError::InvalidInput));
}

#[test]
fn scalar_materialization_must_be_complete_unique_and_consistent() {
    let mut missing = fixture();
    missing.scalars.pop();
    assert_eq!(
        run(&missing, &query(Vec::new())),
        Err(QueryError::InvalidInput)
    );

    let mut duplicate = fixture();
    duplicate.scalars.push(duplicate.scalars[0].clone());
    assert_eq!(
        run(&duplicate, &query(Vec::new())),
        Err(QueryError::InvalidInput)
    );

    let mut wrong_kind = fixture();
    wrong_kind.scalars[1].kind = RecordKind::Thread;
    assert_eq!(
        run(&wrong_kind, &query(Vec::new())),
        Err(QueryError::InvalidInput)
    );

    let mut wrong_content = fixture();
    wrong_content.scalars[1].body = "changed".into();
    assert_eq!(
        run(&wrong_content, &query(Vec::new())),
        Err(QueryError::InvalidInput)
    );

    let mut hidden_row = fixture();
    hidden_row.scalars.push(scalar(
        4,
        RecordKind::Plain,
        "hidden",
        "A Hidden",
        "Needle",
        "5",
        Some("2026-09-01"),
        None,
        "2026-09-01T10:00:00Z",
        "2026-09-01T10:00:00Z",
    ));
    assert_eq!(
        run(&hidden_row, &query(Vec::new())),
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
        run(&duplicate, &query(Vec::new())),
        Err(QueryError::InvalidInput)
    );

    let mut missing = fixture();
    missing.graph.records[1].organ_uid = Some(r(99));
    assert_eq!(
        run(&missing, &query(Vec::new())),
        Err(QueryError::InvalidInput)
    );

    let mut wrong = fixture();
    wrong.graph.assertions[0].object_uid = Some(r(2));
    assert_eq!(
        run(&wrong, &query(Vec::new())),
        Err(QueryError::InvalidInput)
    );

    let mut deleted = fixture();
    deleted.graph.records[1].deleted = true;
    assert_eq!(
        run(&deleted, &query(Vec::new())),
        Err(QueryError::InvalidInput)
    );
}

#[test]
fn hard_limits_cover_limits_graph_predicates_strings_steps_and_results() {
    let base = fixture();
    let request = query(vec![Predicate::All(vec![Predicate::All(vec![
        Predicate::UidEq(r(1)),
    ])])]);
    let mut cases = Vec::new();

    let mut limits = QueryLimits::default();
    limits.records = 10;
    cases.push(limits);
    let mut limits = QueryLimits::default();
    limits.assertions = 7;
    cases.push(limits);
    let mut limits = QueryLimits::default();
    limits.predicate_nodes = 1;
    cases.push(limits);
    let mut limits = QueryLimits::default();
    limits.predicate_depth = 1;
    cases.push(limits);
    let mut limits = QueryLimits::default();
    limits.graph_edges = 1;
    cases.push(limits);
    let mut limits = QueryLimits::default();
    limits.string_bytes = 27;
    cases.push(limits);
    let mut limits = QueryLimits::default();
    limits.bytes = 100;
    cases.push(limits);
    let mut limits = QueryLimits::default();
    limits.steps = 10;
    cases.push(limits);

    for limits in cases {
        assert_eq!(
            select_records(
                &request,
                &base.graph,
                &base.readable,
                &base.scalars,
                &limits,
            ),
            Err(QueryError::LimitExceeded)
        );
    }

    let mut order = query(Vec::new());
    order.order = vec![Order::Asc("head".into()), Order::Asc("slug".into())];
    let mut limits = QueryLimits::default();
    limits.order_fields = 1;
    assert_eq!(
        select_records(&order, &base.graph, &base.readable, &base.scalars, &limits,),
        Err(QueryError::LimitExceeded)
    );

    let mut projection = query(Vec::new());
    projection.fields = Some(vec!["head".into(), "body".into()]);
    let mut limits = QueryLimits::default();
    limits.projection_fields = 1;
    assert_eq!(
        select_records(
            &projection,
            &base.graph,
            &base.readable,
            &base.scalars,
            &limits,
        ),
        Err(QueryError::LimitExceeded)
    );

    let mut long_operand = query(vec![Predicate::TextContains("x".repeat(33))]);
    long_operand.limit = Some(1);
    let mut limits = QueryLimits::default();
    limits.string_bytes = 32;
    assert_eq!(
        select_records(
            &long_operand,
            &base.graph,
            &base.readable,
            &base.scalars,
            &limits,
        ),
        Err(QueryError::LimitExceeded)
    );

    let mut hidden_payload = fixture();
    hidden_payload.graph.records[4]
        .content
        .as_mut()
        .unwrap()
        .body = "x".repeat(65);
    let mut limits = QueryLimits::default();
    limits.string_bytes = 64;
    assert_eq!(
        select_records(
            &query(Vec::new()),
            &hidden_payload.graph,
            &hidden_payload.readable,
            &hidden_payload.scalars,
            &limits,
        ),
        Err(QueryError::LimitExceeded)
    );

    let mut limits = QueryLimits::default();
    limits.results = 1;
    assert_eq!(
        select_records(
            &query(Vec::new()),
            &base.graph,
            &base.readable,
            &base.scalars,
            &limits,
        ),
        Err(QueryError::LimitExceeded)
    );
    let mut limited = query(Vec::new());
    limited.limit = Some(1);
    assert_eq!(
        records(
            select_records(
                &limited,
                &base.graph,
                &base.readable,
                &base.scalars,
                &limits,
            )
            .unwrap()
        )
        .len(),
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
        let mut limits = QueryLimits::default();
        limits.steps = middle;
        match select_records(
            &control,
            &fixture.graph,
            &fixture.readable,
            &fixture.scalars,
            &limits,
        ) {
            Ok(_) => upper = middle,
            Err(QueryError::LimitExceeded) => lower = middle + 1,
            result => panic!("unexpected control result: {result:?}"),
        }
    }

    let mut limits = QueryLimits::default();
    limits.steps = upper + 32;
    assert!(
        select_records(
            &query(Vec::new()),
            &fixture.graph,
            &fixture.readable,
            &fixture.scalars,
            &limits,
        )
        .is_ok()
    );
    assert!(
        select_records(
            &control,
            &fixture.graph,
            &fixture.readable,
            &fixture.scalars,
            &limits,
        )
        .is_ok()
    );

    let scanned = query(vec![Predicate::Any(vec![
        Predicate::UidEq(r(1)),
        Predicate::ConceptIn(c(1)),
        Predicate::ConceptIn(c(1)),
    ])]);
    assert_eq!(
        select_records(
            &scanned,
            &fixture.graph,
            &fixture.readable,
            &fixture.scalars,
            &limits,
        ),
        Err(QueryError::LimitExceeded)
    );
}

#[test]
fn caller_limits_cannot_disable_or_expand_the_hard_ceiling() {
    let fixture = fixture();
    let mut zero = QueryLimits::default();
    zero.steps = 0;
    assert_eq!(
        select_records(
            &query(Vec::new()),
            &fixture.graph,
            &fixture.readable,
            &fixture.scalars,
            &zero,
        ),
        Err(QueryError::InvalidLimits)
    );
    let mut expanded = QueryLimits::default();
    expanded.bytes += 1;
    assert_eq!(
        select_records(
            &query(Vec::new()),
            &fixture.graph,
            &fixture.readable,
            &fixture.scalars,
            &expanded,
        ),
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
