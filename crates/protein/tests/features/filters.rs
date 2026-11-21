use super::fixtures::{base, engine, make};
use engine::actions::Action;
use nucleus::RecordKind;
use protein::{DateComparison, LinkDirection, Predicate, Protein, Source, WorkDateField, execute};
use serde_json::{from_value, json};
use std::collections::HashSet;

#[tokio::test]
async fn uid_eq_targets_one_record_directly() {
    let e = engine().await;
    let target = make(&e, "target.record", RecordKind::Plain, 3.0).await;
    make(&e, "other.record", RecordKind::Plain, 3.0).await;

    let p = base(Source::Record, vec![Predicate::UidEq(target)]);
    let rows = execute(&e.store, &p).await.unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["slug"], "target.record");
}

#[tokio::test]
async fn organ_eq_and_organ_in_filter_by_record_origin() {
    let e = engine().await;

    let organ_a = store::organs::ensure_local(&e.store.pool, "http://cell-a")
        .await
        .unwrap()
        .uid;
    let organ_b = store::organs::add_contact(
        &e.store.pool,
        "organ_b_uid",
        Some("organ.b"),
        "Cell B",
        "http://cell-b",
        1,
    )
    .await
    .unwrap();

    let _apple = make(&e, "apple", RecordKind::Plain, 1.0).await;
    let mango = make(&e, "mango", RecordKind::Plain, 1.0).await;
    store::records::set_organ_origin(&e.store.pool, &mango, Some(&organ_b))
        .await
        .unwrap();

    let a_only = execute(
        &e.store,
        &base(
            Source::Record,
            vec![
                Predicate::OrganEq(organ_a.clone()),
                Predicate::KindEq("plain".into()),
            ],
        ),
    )
    .await
    .unwrap();
    assert_eq!(a_only.len(), 1);
    assert_eq!(a_only[0]["slug"], "apple");

    let a_or_b = execute(
        &e.store,
        &base(
            Source::Record,
            vec![
                Predicate::OrganIn(vec![organ_a, organ_b]),
                Predicate::KindEq("plain".into()),
            ],
        ),
    )
    .await
    .unwrap();
    let mut slugs: Vec<String> = a_or_b
        .iter()
        .map(|r| r["slug"].as_str().unwrap().to_string())
        .collect();
    slugs.sort();
    assert_eq!(slugs, vec!["apple", "mango"]);
}

#[tokio::test]
async fn quantity_lte_and_gte_include_the_boundary() {
    let e = engine().await;
    make(&e, "low", RecordKind::Plain, -1.0).await;
    make(&e, "edge", RecordKind::Plain, 0.0).await;
    make(&e, "high", RecordKind::Plain, 1.0).await;

    let lte = base(
        Source::Record,
        vec![
            Predicate::QuantityLte(store::exact::zero()),
            Predicate::KindEq("plain".into()),
        ],
    );
    let mut lte_slugs: Vec<String> = execute(&e.store, &lte)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r["slug"].as_str().unwrap().to_string())
        .collect();
    lte_slugs.sort();
    assert_eq!(lte_slugs, vec!["edge", "low"]);

    let gte_json = json!({
        "source": "record",
        "where": [ { "quantity_gte": "0.0" }, { "kind_eq": "plain" } ]
    });
    let gte: Protein = from_value(gte_json).unwrap();
    let gte_slugs: Vec<String> = execute(&e.store, &gte)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r["slug"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(gte_slugs, vec!["edge", "high"]);
}

#[tokio::test]
async fn near_predicate_uses_the_place_instinct() {
    let e = engine().await;
    let home = make(&e, "home", RecordKind::Plain, 1.0).await;
    let market = make(&e, "market.needs", RecordKind::Plain, -1.0).await;
    let far = make(&e, "far.needs", RecordKind::Plain, -1.0).await;

    e.act(
        Action::SetPlace {
            target: home,
            lat: 0.0,
            lon: 0.0,
            address: None,
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::SetPlace {
            target: market,
            lat: 0.0009,
            lon: 0.0,
            address: None,
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::SetPlace {
            target: far,
            lat: 0.09,
            lon: 0.0,
            address: None,
        },
        None,
    )
    .await
    .unwrap();

    let p = base(
        Source::Record,
        vec![
            Predicate::QuantityLt(store::exact::zero()),
            Predicate::Near {
                of: "home".into(),
                meters: 2000.0,
            },
        ],
    );
    let rows = execute(&e.store, &p).await.unwrap();
    let slugs: Vec<&str> = rows.iter().map(|r| r["slug"].as_str().unwrap()).collect();
    assert_eq!(slugs, vec!["market.needs"], "only the nearby need matches");
}

#[tokio::test]
async fn nested_record_filters_cover_text_dates_relations_and_assignee() {
    let e = engine().await;
    for kind in ["part-of", "assigned-to"] {
        e.act(
            Action::CreateConcept {
                lingua: "g_local".into(),
                name: kind.into(),
                parents: vec![],
            },
            None,
        )
        .await
        .unwrap();
    }
    let root = make(&e, "root", RecordKind::Plain, 0.0).await;
    let middle = make(&e, "middle", RecordKind::Plain, 0.0).await;
    let leaf = make(&e, "leaf", RecordKind::Plain, 0.0).await;
    let design = make(&e, "design-task", RecordKind::Plain, -1.0).await;
    let ana = make(&e, "ana", RecordKind::Person, 1.0).await;
    for (from, kind, to) in [
        (&middle, "part-of", &root),
        (&leaf, "part-of", &middle),
        (&design, "assigned-to", &ana),
    ] {
        e.act(
            Action::AssertRecord {
                subject: from.clone(),
                predicate: kind.into(),
                object: Some(to.clone()),
                quantity: None,
                unit: None,
            },
            None,
        )
        .await
        .unwrap();
    }
    store::records::set_extension(
        &e.store.pool,
        &middle,
        "work",
        &json!({"start":"2026-08-01", "due":"2026-08-10"}),
    )
    .await
    .unwrap();

    let query = base(
        Source::Record,
        vec![Predicate::Any(vec![
            Predicate::All(vec![
                Predicate::QuantityLt(store::exact::zero()),
                Predicate::TextContains("DESIGN".into()),
                Predicate::Relation {
                    kind: "assigned-to".into(),
                    direction: LinkDirection::Out,
                    other: Some("ana".into()),
                },
            ]),
            Predicate::All(vec![
                Predicate::Relation {
                    kind: "part-of".into(),
                    direction: LinkDirection::In,
                    other: None,
                },
                Predicate::Relation {
                    kind: "part-of".into(),
                    direction: LinkDirection::Out,
                    other: None,
                },
                Predicate::WorkDate {
                    field: WorkDateField::Due,
                    op: DateComparison::Lte,
                    value: Some("2026-08-10".into()),
                },
            ]),
        ])],
    );
    let rows = execute(&e.store, &query).await.unwrap();
    let ids: HashSet<&str> = rows.iter().filter_map(|row| row["uid"].as_str()).collect();
    assert_eq!(ids, HashSet::from([middle.as_str(), design.as_str()]));
}
