use protein::{DateComparison, LinkDirection, Predicate, WorkDateField};

use super::fixture::{c, decimal, fixture, r};

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
                fixture.selected(vec![Predicate::UidEq(r(1)), wrapped]),
                vec![r(1)]
            );
        }
    }
}

#[test]
fn relation_direction_and_work_date_operations_are_exact() {
    let fixture = fixture();
    assert_eq!(
        fixture.selected(vec![Predicate::Relation {
            kind: c(3),
            direction: LinkDirection::In,
            other: Some(r(1)),
        }]),
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
            fixture.selected(vec![
                Predicate::UidEq(r(1)),
                Predicate::WorkDate {
                    field: WorkDateField::Start,
                    op,
                    value: Some(value.into()),
                },
            ]),
            vec![r(1)]
        );
    }
    assert_eq!(
        fixture.selected(vec![
            Predicate::UidEq(r(1)),
            Predicate::WorkDate {
                field: WorkDateField::Due,
                op: DateComparison::Exists,
                value: None,
            },
        ]),
        vec![r(1)]
    );
    assert_eq!(
        fixture.selected(vec![Predicate::Under {
            record: r(3),
            kind: c(4),
            include_self: true,
        }]),
        vec![r(1), r(3)]
    );
}

#[test]
fn locked_or_malformed_vault_prefixes_are_never_plaintext_searchable() {
    let fixture = fixture();
    assert_eq!(
        fixture.selected(vec![Predicate::TextContains("needle".into())]),
        vec![r(1)]
    );
    assert_eq!(
        fixture.selected(vec![Predicate::TextContains("locked".into())]),
        vec![r(8)]
    );
}
