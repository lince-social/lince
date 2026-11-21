use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, FixedOffset, NaiveDate};
use nucleus::{DecimalValue, RecordKind};
use protein::{
    Include, Predicate, Protein, Source,
    authority::{
        AssertionRole, AssertionState, ConceptState, ExtensionProperty, GraphSnapshot,
        RecordContent, RecordState,
    },
    record_query::{
        QueryError, QueryLimits, QueryResult, QueryRows, ReadableIdentities, RecordScalarRow,
        select_records,
    },
};
use serde_json::json;

fn uid(prefix: &str, number: usize) -> String {
    format!("{prefix}_{number:026}")
}

pub(super) fn r(number: usize) -> String {
    uid("r", number)
}

pub(super) fn c(number: usize) -> String {
    uid("c", number)
}

pub(super) fn a(number: usize) -> String {
    uid("a", number)
}

pub(super) fn pl(number: usize) -> String {
    uid("pl", number)
}

pub(super) fn decimal(value: &str) -> DecimalValue {
    DecimalValue::parse_inferred(value).expect("fixture quantity must be valid")
}

fn date(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").expect("fixture date must be valid")
}

fn instant(value: &str) -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339(value).expect("fixture timestamp must be valid")
}

pub(super) struct ScalarDetails<'a> {
    pub(super) body: &'a str,
    pub(super) quantity: DecimalValue,
    pub(super) start_date: Option<&'a str>,
    pub(super) due_date: Option<&'a str>,
    pub(super) updated_at: Option<&'a str>,
}

impl Default for ScalarDetails<'_> {
    fn default() -> Self {
        Self {
            body: "",
            quantity: DecimalValue::from_integer(0),
            start_date: None,
            due_date: None,
            updated_at: None,
        }
    }
}

pub(super) fn scalar(
    number: usize,
    kind: RecordKind,
    slug: &str,
    head: &str,
    created_at: &str,
    details: ScalarDetails<'_>,
) -> RecordScalarRow {
    let created_at = instant(created_at);
    RecordScalarRow {
        uid: r(number),
        kind,
        slug: Some(slug.into()),
        head: head.into(),
        body: details.body.into(),
        quantity: details.quantity,
        start_date: details.start_date.map(date),
        due_date: details.due_date.map(date),
        created_at,
        updated_at: details.updated_at.map_or(created_at, instant),
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

pub(super) struct Fixture {
    pub(super) graph: GraphSnapshot,
    pub(super) readable: ReadableIdentities,
    pub(super) scalars: Vec<RecordScalarRow>,
}

pub(super) fn fixture() -> Fixture {
    let organ = scalar(
        0,
        RecordKind::Organ,
        "company",
        "Company",
        "2026-09-01T09:00:00Z",
        Default::default(),
    );
    let task = scalar(
        1,
        RecordKind::Plain,
        "task.alpha",
        "Alpha",
        "2026-09-05T10:00:00+01:00",
        ScalarDetails {
            body: "Visible Needle",
            quantity: decimal("1.000"),
            start_date: Some("2026-09-03"),
            due_date: Some("2026-09-10"),
            updated_at: Some("2026-09-06T12:00:00Z"),
        },
    );
    let second = scalar(
        2,
        RecordKind::Plain,
        "task.beta",
        "Beta",
        "2026-09-04T10:00:00Z",
        ScalarDetails {
            body: "ordinary",
            quantity: decimal("2"),
            due_date: Some("2026-09-08"),
            updated_at: Some("2026-09-07T12:00:00Z"),
            ..Default::default()
        },
    );
    let root = scalar(
        3,
        RecordKind::Plain,
        "project",
        "Project",
        "2026-09-03T10:00:00Z",
        Default::default(),
    );
    let hidden = scalar(
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
    );
    let intermediate = scalar(
        5,
        RecordKind::Plain,
        "hidden.intermediate",
        "Hidden intermediate",
        "2026-09-02T10:00:00Z",
        Default::default(),
    );
    let leaf = scalar(
        6,
        RecordKind::Plain,
        "leaf",
        "Leaf",
        "2026-09-06T10:00:00Z",
        Default::default(),
    );
    let person = scalar(
        7,
        RecordKind::Person,
        "person",
        "Person",
        "2026-09-02T11:00:00Z",
        Default::default(),
    );
    let locked = scalar(
        8,
        RecordKind::Plain,
        "locked",
        "Locked",
        "2026-09-07T10:00:00Z",
        ScalarDetails {
            body: "  lince-vault.v1 malformed Needle",
            ..Default::default()
        },
    );
    let uncategorized = scalar(
        9,
        RecordKind::Thread,
        "uncategorized",
        "Uncategorized",
        "2026-09-08T10:00:00Z",
        Default::default(),
    );
    let hidden_organ = scalar(
        10,
        RecordKind::Organ,
        "hidden.company",
        "Hidden company",
        "2026-09-01T08:00:00Z",
        Default::default(),
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
    let task_content = records[1]
        .content
        .as_mut()
        .expect("task fixture has content");
    task_content.unit_uid = Some(c(1));
    task_content.place_uid = Some(pl(1));
    task_content.extensions.insert(
        ExtensionProperty {
            namespace: "custom".into(),
            field: "priority".into(),
        },
        json!(3),
    );

    let concepts = [
        (1, "work", None),
        (2, "task", Some(1)),
        (3, "assigned", None),
        (4, "contains", None),
        (5, "secret", None),
        (6, "assigned directly", Some(3)),
    ]
    .into_iter()
    .map(|(uid, name, parent)| ConceptState {
        uid: c(uid),
        name: name.into(),
        parents: parent.map(c).into_iter().collect(),
    })
    .collect();
    let assertions = [
        (1, 1, 2, None),
        (2, 2, 1, None),
        (3, 1, 6, Some(7)),
        (4, 1, 4, Some(3)),
        (5, 5, 4, Some(3)),
        (6, 6, 4, Some(5)),
        (7, 6, 5, Some(3)),
        (8, 4, 2, None),
        (9, 6, 4, Some(3)),
        (11, 9, 1, None),
    ]
    .into_iter()
    .map(|(uid, subject, predicate, object)| AssertionState {
        uid: a(uid),
        subject_uid: r(subject),
        predicate_uid: c(predicate),
        object_uid: object.map(r),
        object_uri: None,
        role: if object.is_some() {
            AssertionRole::Ordinary
        } else {
            AssertionRole::Identity
        },
        quantity: None,
        unit_uid: None,
    })
    .collect();
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
            resolved_links: Default::default(),
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

pub(super) fn query(filter: Vec<Predicate>) -> Protein {
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

impl Fixture {
    pub(super) fn run(&self, request: &Protein) -> Result<QueryResult, QueryError> {
        self.run_with_limits(request, &QueryLimits::default())
    }

    pub(super) fn run_with_limits(
        &self,
        request: &Protein,
        limits: &QueryLimits,
    ) -> Result<QueryResult, QueryError> {
        select_records(request, &self.graph, &self.readable, &self.scalars, limits)
    }

    pub(super) fn selected(&self, filter: Vec<Predicate>) -> Vec<String> {
        records(
            self.run(&query(filter))
                .expect("fixture query should succeed"),
        )
    }
}

pub(super) fn records(result: QueryResult) -> Vec<String> {
    let QueryRows::Records(records) = result.rows else {
        panic!("expected Record UIDs, got {:?}", result.rows)
    };
    records
}

pub(super) fn minimal(rows: Vec<RecordScalarRow>) -> Fixture {
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
