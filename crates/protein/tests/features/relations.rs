use super::fixtures::{base, engine, make};
use engine::actions::Action;
use nucleus::RecordKind;
use protein::{LinkDirection, LinksInclude, Predicate, Source, execute};

#[tokio::test]
async fn named_relation_paths_keep_terminal_records_and_isolate_shared_branches() {
    let e = engine().await;
    e.act(
        Action::CreateConcept {
            lingua: "g_local".into(),
            name: "philosophy".into(),
            parents: vec![],
        },
        None,
    )
    .await
    .unwrap();
    let family = store::concepts::resolve(&e.store.pool, "philosophy")
        .await
        .unwrap()
        .unwrap();
    for name in ["greek-study", "medieval-study"] {
        e.act(
            Action::CreateConcept {
                lingua: "g_local".into(),
                name: name.into(),
                parents: vec![family.clone()],
            },
            None,
        )
        .await
        .unwrap();
    }
    e.act(
        Action::CreateConcept {
            lingua: "g_local".into(),
            name: "geographical-context".into(),
            parents: vec![],
        },
        None,
    )
    .await
    .unwrap();
    let root = make(&e, "philosophy-root", RecordKind::Plain, 0.0).await;
    let greek = make(&e, "greek-reading", RecordKind::Plain, 0.0).await;
    let last = make(&e, "greek-terminal", RecordKind::Plain, 0.0).await;
    let medieval = make(&e, "medieval-reading", RecordKind::Plain, 0.0).await;
    let geography = make(&e, "geographical-context", RecordKind::Plain, 0.0).await;
    for (from, predicate, to) in [
        (&root, "greek-study", &greek),
        (&greek, "greek-study", &last),
        (&root, "medieval-study", &medieval),
        (&greek, "geographical-context", &geography),
    ] {
        e.act(
            Action::AssertRecord {
                subject: from.clone(),
                predicate: predicate.into(),
                object: Some(to.clone()),
                quantity: None,
                unit: None,
            },
            None,
        )
        .await
        .unwrap();
    }
    let mut query = base(
        Source::Record,
        vec![Predicate::Relation {
            kind: "greek-study".into(),
            direction: LinkDirection::Both,
            other: None,
        }],
    );
    query.include.links = Some(LinksInclude {
        kinds: vec!["greek-study".into()],
        direction: LinkDirection::Both,
        depth: 1,
    });
    let rows = execute(&e.store, &query).await.unwrap();
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().any(|row| row["uid"] == root));
    assert!(rows.iter().any(|row| row["uid"] == last));
    assert!(
        rows.iter()
            .all(|row| row["uid"] != medieval && row["uid"] != geography)
    );
    assert!(
        rows.iter()
            .flat_map(|row| row["links"].as_array().unwrap())
            .all(|link| link["kind"] == "greek-study")
    );
    assert!(
        rows.iter().find(|row| row["uid"] == last).unwrap()["links"]
            .as_array()
            .unwrap()
            .iter()
            .any(|link| link["from"] == greek && link["direction"] == "in")
    );
    query.filter = vec![Predicate::Relation {
        kind: "philosophy".into(),
        direction: LinkDirection::Both,
        other: None,
    }];
    query.include.links.as_mut().unwrap().kinds = vec!["philosophy".into()];
    let rows = execute(&e.store, &query).await.unwrap();
    assert_eq!(rows.len(), 4);
    assert!(rows.iter().any(|row| row["uid"] == medieval));
    assert!(
        rows.iter()
            .all(|row| !row["links"].as_array().unwrap().is_empty())
    );
    assert!(
        rows.iter()
            .flat_map(|row| row["links"].as_array().unwrap())
            .all(|link| link["kind"] != "geographical-context")
    );
}

#[tokio::test]
async fn links_include_is_explicit_and_supports_multiple_kinds() {
    let e = engine().await;
    let a = make(&e, "a", RecordKind::Plain, 0.0).await;
    let b = make(&e, "b", RecordKind::Plain, 0.0).await;
    let c = make(&e, "c", RecordKind::Plain, 0.0).await;
    for kind in ["before", "contributes"] {
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
    e.act(
        Action::AssertRecord {
            subject: a.clone(),
            predicate: "before".into(),
            object: Some(b.clone()),
            quantity: Some("1".into()),
            unit: None,
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::AssertRecord {
            subject: c,
            predicate: "contributes".into(),
            object: Some(a.clone()),
            quantity: None,
            unit: None,
        },
        None,
    )
    .await
    .unwrap();

    let mut none = base(Source::Record, vec![Predicate::UidEq(a.clone())]);
    none.include.links = Some(LinksInclude {
        kinds: vec![],
        direction: LinkDirection::Both,
        depth: 0,
    });
    assert!(
        execute(&e.store, &none).await.unwrap()[0]["links"]
            .as_array()
            .unwrap()
            .is_empty(),
        "empty kinds means no links"
    );

    let mut both = base(Source::Record, vec![Predicate::UidEq(a.clone())]);
    both.include.links = Some(LinksInclude {
        kinds: vec!["before".into(), "contributes".into()],
        direction: LinkDirection::Both,
        depth: 0,
    });
    let rows = execute(&e.store, &both).await.unwrap();
    let links = rows[0]["links"].as_array().unwrap();
    assert_eq!(links.len(), 2);
    assert!(
        links
            .iter()
            .any(|link| link["kind"] == "before" && link["direction"] == "out")
    );
    assert!(
        links
            .iter()
            .any(|link| link["kind"] == "contributes" && link["direction"] == "in")
    );

    let mut outgoing = both;
    outgoing.include.links.as_mut().unwrap().direction = LinkDirection::Out;
    let rows = execute(&e.store, &outgoing).await.unwrap();
    let links = rows[0]["links"].as_array().unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0]["kind"], "before");

    let mut all = base(Source::Record, vec![Predicate::UidEq(a.clone())]);
    all.include.links = Some(LinksInclude {
        kinds: vec!["*".into()],
        direction: LinkDirection::Both,
        depth: 0,
    });
    let rows = execute(&e.store, &all).await.unwrap();
    let links = rows[0]["links"].as_array().unwrap();
    assert_eq!(links.len(), 2);
    assert!(links.iter().any(|link| link["kind"] == "before"));
    assert!(links.iter().any(|link| link["kind"] == "contributes"));
    all.include.links.as_mut().unwrap().direction = LinkDirection::In;
    let rows = execute(&e.store, &all).await.unwrap();
    let links = rows[0]["links"].as_array().unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0]["kind"], "contributes");
}

#[tokio::test]
async fn relation_filters_by_tag_cluster_with_include_and_exclude() {
    let e = engine().await;
    e.act(
        Action::CreateConcept {
            lingua: "g_local".into(),
            name: "tag".into(),
            parents: vec![],
        },
        None,
    )
    .await
    .unwrap();
    make(&e, "tasks", RecordKind::Plain, 0.0).await;
    make(&e, "project-a", RecordKind::Plain, 0.0).await;
    make(&e, "project-b", RecordKind::Plain, 0.0).await;
    let t1 = make(&e, "t1", RecordKind::Plain, 0.0).await;
    let t2 = make(&e, "t2", RecordKind::Plain, 0.0).await;
    let t3 = make(&e, "t3", RecordKind::Plain, 0.0).await;

    let tag = |from: &str, to: &str| {
        let e = &e;
        let from = from.to_string();
        let to = to.to_string();
        async move {
            e.act(
                Action::AssertRecord {
                    subject: from,
                    predicate: "tag".into(),
                    object: Some(to),
                    quantity: None,
                    unit: None,
                },
                None,
            )
            .await
            .unwrap();
        }
    };
    tag(&t1, "tasks").await;
    tag(&t1, "project-a").await;
    tag(&t2, "project-a").await;
    tag(&t2, "project-b").await;
    tag(&t3, "tasks").await;

    let filter = vec![Predicate::All(vec![
        Predicate::Any(vec![
            Predicate::Relation {
                kind: "tag".into(),
                direction: LinkDirection::Out,
                other: Some("tasks".into()),
            },
            Predicate::Relation {
                kind: "tag".into(),
                direction: LinkDirection::Out,
                other: Some("project-a".into()),
            },
        ]),
        Predicate::Not(Box::new(Predicate::Relation {
            kind: "tag".into(),
            direction: LinkDirection::Out,
            other: Some("project-b".into()),
        })),
    ])];
    let rows = execute(&e.store, &base(Source::Record, filter))
        .await
        .unwrap();
    let uids: Vec<&str> = rows.iter().map(|r| r["uid"].as_str().unwrap()).collect();
    assert!(
        uids.contains(&t1.as_str()),
        "t1 (Tasks+ProjectA, no ProjectB) should match"
    );
    assert!(uids.contains(&t3.as_str()), "t3 (Tasks) should match");
    assert!(
        !uids.contains(&t2.as_str()),
        "t2 (has ProjectB) should be excluded"
    );
    assert_eq!(uids.len(), 2, "only t1 and t3 match: {uids:?}");
}
