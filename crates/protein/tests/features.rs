//! Stage 3+ Protein features: aggregates, availability, the visibility gate,
//! saved Proteins, and the place `near` predicate.

use engine::Engine;
use engine::actions::Action;
use nucleus::RecordKind;
use protein::{
    Aggregate, AggregateOp, GroupBy, Include, LinkDirection, LinksInclude, Predicate, Protein,
    Source, ThreadsInclude,
};

async fn engine() -> Engine {
    Engine::open_memory().await.expect("engine")
}

async fn make(e: &Engine, slug: &str, kind: RecordKind, q: f64) -> String {
    e.act(
        Action::CreateRecord {
            slug: Some(slug.into()),
            kind,
            head: slug.into(),
            body: String::new(),
            quantity: q,
        },
        None,
    )
    .await
    .unwrap()
    .created
    .unwrap()
}

fn base(source: Source, filter: Vec<Predicate>) -> Protein {
    Protein {
        source,
        filter,
        include: Include::default(),
        aggregate: None,
        order: vec![],
        limit: None,
    }
}

#[tokio::test]
async fn aggregate_sums_by_group() {
    let e = engine().await;
    make(&e, "checking", RecordKind::Plain, 500.0).await;
    make(&e, "savings", RecordKind::Plain, 1500.0).await;
    make(&e, "rules.x", RecordKind::Rule, 1.0).await;

    let mut p = base(Source::Record, vec![]);
    p.aggregate = Some(Aggregate {
        op: AggregateOp::Sum,
        by: GroupBy::Kind,
    });
    let rows = protein::execute(&e.store, &p).await.unwrap();
    let plain = rows.iter().find(|r| r["group"] == "plain").unwrap();
    assert_eq!(plain["value"], 2000.0);
}

#[tokio::test]
async fn availability_reflects_active_outgoing_promises() {
    let e = engine().await;
    let apples = make(&e, "apples.stock", RecordKind::Plain, 10.0).await;
    // an active outgoing promise reserves 3
    let promise = e
        .act(
            Action::CreatePromise {
                record: apples.clone(),
                delta: -3.0,
                window_end: None,
                party: Some("someone".into()),
                open: false,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    for to in [nucleus::PromiseState::Agreed, nucleus::PromiseState::Active] {
        e.act(
            Action::PromiseTransition {
                promise: promise.clone(),
                to,
            },
            None,
        )
        .await
        .unwrap();
    }

    let mut p = base(
        Source::Record,
        vec![Predicate::SlugEq("apples.stock".into())],
    );
    p.include.availability = true;
    let rows = protein::execute(&e.store, &p).await.unwrap();
    assert_eq!(rows[0]["quantity"], 10.0);
    assert_eq!(rows[0]["available"], 7.0, "10 - 3 reserved");
    assert_eq!(rows[0]["planned"], 7.0, "10 + (-3) agreed/active");
}

#[tokio::test]
async fn uid_eq_targets_one_record_directly() {
    let e = engine().await;
    let target = make(&e, "target.record", RecordKind::Plain, 3.0).await;
    make(&e, "other.record", RecordKind::Plain, 3.0).await;

    let p = base(Source::Record, vec![Predicate::UidEq(target)]);
    let rows = protein::execute(&e.store, &p).await.unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["slug"], "target.record");
}

#[tokio::test]
async fn quantity_lte_and_gte_include_the_boundary() {
    let e = engine().await;
    make(&e, "low", RecordKind::Plain, -1.0).await;
    make(&e, "edge", RecordKind::Plain, 0.0).await;
    make(&e, "high", RecordKind::Plain, 1.0).await;

    let lte = base(Source::Record, vec![Predicate::QuantityLte(0.0)]);
    let lte_slugs: Vec<String> = protein::execute(&e.store, &lte)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r["slug"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(lte_slugs, vec!["low", "edge"]);

    let gte_json = serde_json::json!({
        "source": "record",
        "where": [ { "quantity_gte": 0.0 } ]
    });
    let gte: Protein = serde_json::from_value(gte_json).unwrap();
    let gte_slugs: Vec<String> = protein::execute(&e.store, &gte)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r["slug"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(gte_slugs, vec!["edge", "high"]);
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
                name: kind.into(),
                parents: vec![],
            },
            None,
        )
        .await
        .unwrap();
    }
    e.act(
        Action::AddLink {
            from: a.clone(),
            kind: "before".into(),
            to: b.clone(),
            quantity: Some(1.0),
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::AddLink {
            from: c,
            kind: "contributes".into(),
            to: a.clone(),
            quantity: None,
        },
        None,
    )
    .await
    .unwrap();

    let mut none = base(Source::Record, vec![Predicate::UidEq(a.clone())]);
    none.include.links = Some(LinksInclude {
        kind: None,
        kinds: vec![],
        direction: LinkDirection::Both,
        depth: 0,
    });
    assert!(
        protein::execute(&e.store, &none).await.unwrap()[0]["links"]
            .as_array()
            .unwrap()
            .is_empty(),
        "empty kinds means no links"
    );

    let mut both = base(Source::Record, vec![Predicate::UidEq(a.clone())]);
    both.include.links = Some(LinksInclude {
        kind: None,
        kinds: vec!["before".into(), "contributes".into()],
        direction: LinkDirection::Both,
        depth: 0,
    });
    let rows = protein::execute(&e.store, &both).await.unwrap();
    let links = rows[0]["links"].as_array().unwrap();
    assert_eq!(links.len(), 2);
    assert!(links.iter().any(|link| link["kind"] == "before" && link["direction"] == "out"));
    assert!(links.iter().any(|link| link["kind"] == "contributes" && link["direction"] == "in"));

    let mut outgoing = both;
    outgoing.include.links.as_mut().unwrap().direction = LinkDirection::Out;
    let rows = protein::execute(&e.store, &outgoing).await.unwrap();
    let links = rows[0]["links"].as_array().unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0]["kind"], "before");

    let legacy: Protein = serde_json::from_value(serde_json::json!({
        "source": "record",
        "where": [{ "uid_eq": a }],
        "include": { "links": { "kind": "before" } }
    }))
    .unwrap();
    let rows = protein::execute(&e.store, &legacy).await.unwrap();
    assert_eq!(rows[0]["links"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn linked_to_filters_by_tag_cluster_with_include_and_exclude() {
    // Multi-valued cluster tags (Stage 8b, Phase 5): a record is tagged into
    // clusters by `tag`-kind links to cluster records. `linked_to` filters on
    // them; `any`/`not`/`all` compose "Tasks OR ProjectA but NOT ProjectB".
    let e = engine().await;
    e.act(
        Action::CreateConcept { name: "tag".into(), parents: vec![] },
        None,
    )
    .await
    .unwrap();
    // Cluster records (the tag targets).
    make(&e, "tasks", RecordKind::Plain, 0.0).await;
    make(&e, "project-a", RecordKind::Plain, 0.0).await;
    make(&e, "project-b", RecordKind::Plain, 0.0).await;
    // Task records.
    let t1 = make(&e, "t1", RecordKind::Plain, 0.0).await;
    let t2 = make(&e, "t2", RecordKind::Plain, 0.0).await;
    let t3 = make(&e, "t3", RecordKind::Plain, 0.0).await;

    let tag = |from: &str, to: &str| {
        let e = &e;
        let from = from.to_string();
        let to = to.to_string();
        async move {
            e.act(
                Action::AddLink { from, kind: "tag".into(), to, quantity: None },
                None,
            )
            .await
            .unwrap();
        }
    };
    tag(&t1, "tasks").await; // t1 carries TWO tags
    tag(&t1, "project-a").await;
    tag(&t2, "project-a").await;
    tag(&t2, "project-b").await;
    tag(&t3, "tasks").await;

    // (Tasks OR ProjectA) AND NOT ProjectB.
    let filter = vec![Predicate::All(vec![
        Predicate::Any(vec![
            Predicate::LinkedTo { kind: "tag".into(), to: "tasks".into() },
            Predicate::LinkedTo { kind: "tag".into(), to: "project-a".into() },
        ]),
        Predicate::Not(Box::new(Predicate::LinkedTo {
            kind: "tag".into(),
            to: "project-b".into(),
        })),
    ])];
    let rows = protein::execute(&e.store, &base(Source::Record, filter))
        .await
        .unwrap();
    let uids: Vec<&str> = rows.iter().map(|r| r["uid"].as_str().unwrap()).collect();
    assert!(uids.contains(&t1.as_str()), "t1 (Tasks+ProjectA, no ProjectB) should match");
    assert!(uids.contains(&t3.as_str()), "t3 (Tasks) should match");
    assert!(!uids.contains(&t2.as_str()), "t2 (has ProjectB) should be excluded");
    // Cluster records themselves carry no tag link → excluded.
    assert_eq!(uids.len(), 2, "only t1 and t3 match: {uids:?}");
}

#[tokio::test]
async fn threads_include_returns_nested_record_messages() {
    let e = engine().await;
    let subject = make(&e, "abstract-idea", RecordKind::Plain, 0.0).await;
    let thread = e
        .act(
            Action::CreateThread {
                target: subject.clone(),
                head: "Discussion A".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let first = e
        .act(
            Action::CreateMessage {
                thread: thread.clone(),
                body: "First message".into(),
                parent: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let reply = e
        .act(
            Action::CreateMessage {
                thread,
                body: "Reply message".into(),
                parent: Some(first.clone()),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();

    let p = Protein {
        source: Source::Record,
        filter: vec![Predicate::UidEq(subject)],
        include: Include {
            threads: Some(ThreadsInclude { messages_limit: 20 }),
            ..Default::default()
        },
        aggregate: None,
        order: vec![],
        limit: None,
    };
    let rows = protein::execute(&e.store, &p).await.unwrap();
    let threads = rows[0]["threads"].as_array().unwrap();
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0]["head"], "Discussion A");
    let messages = threads[0]["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["body"], "First message");
    assert_eq!(messages[1]["body"], "Reply message");
    assert_eq!(
        messages[1]["parent_message_uid"].as_str(),
        Some(first.as_str())
    );
    assert_eq!(messages[1]["uid"].as_str(), Some(reply.as_str()));
}

#[tokio::test]
async fn visibility_gate_is_the_one_read_boundary() {
    let e = engine().await;
    let public_need = make(&e, "public.apples", RecordKind::Plain, -1.0).await;
    make(&e, "private.diary", RecordKind::Plain, -1.0).await;

    // grant only the public need to an outside organ
    e.act(
        Action::GrantVisibility {
            subject_kind: "organ".into(),
            subject: Some("organ.neighbors".into()),
            target: public_need.clone(),
        },
        None,
    )
    .await
    .unwrap();

    let p = base(Source::Record, vec![Predicate::QuantityLt(0.0)]);
    // local Cell sees both
    assert_eq!(protein::execute(&e.store, &p).await.unwrap().len(), 2);
    // the neighbor organ sees only what was granted
    let seen = protein::execute_for(&e.store, &p, Some("organ.neighbors"))
        .await
        .unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0]["slug"], "public.apples");
    // a stranger sees nothing
    assert!(
        protein::execute_for(&e.store, &p, Some("organ.unknown"))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn saved_protein_is_a_record() {
    let e = engine().await;
    make(&e, "a", RecordKind::Plain, -1.0).await;
    make(&e, "b", RecordKind::Plain, 5.0).await;

    let ast = serde_json::json!({
        "source": "record",
        "where": [ { "quantity_lt": 0.0 } ]
    });
    e.act(
        Action::SaveProtein {
            slug: "views.my-needs".into(),
            head: "My Needs".into(),
            ast,
        },
        None,
    )
    .await
    .unwrap();

    let rows = protein::execute_saved(&e.store, "views.my-needs", None)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["slug"], "a");
}

#[tokio::test]
async fn near_predicate_uses_the_place_instinct() {
    let e = engine().await;
    let home = make(&e, "home", RecordKind::Plain, 1.0).await;
    let market = make(&e, "market.needs", RecordKind::Plain, -1.0).await;
    let far = make(&e, "far.needs", RecordKind::Plain, -1.0).await;

    // home at origin; market ~100m north; far ~10km north
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
            Predicate::QuantityLt(0.0),
            Predicate::Near {
                of: "home".into(),
                meters: 2000.0,
            },
        ],
    );
    let rows = protein::execute(&e.store, &p).await.unwrap();
    let slugs: Vec<&str> = rows.iter().map(|r| r["slug"].as_str().unwrap()).collect();
    assert_eq!(slugs, vec!["market.needs"], "only the nearby need matches");
}
