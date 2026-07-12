//! Part VII completion: the fact/concept/transfer sources and the
//! extension/projection/link-depth includes.

use chrono::{DateTime, Utc};
use engine::Engine;
use engine::actions::Action;
use nucleus::{Cause, NewFact, PromiseState, RecordKind};
use protein::{
    Aggregate, AggregateOp, ExtensionInclude, GroupBy, Include, LinksInclude, Order, Predicate,
    ProjectionInclude, Protein, Source,
};
use store::records::NewRecord;

async fn engine() -> Engine {
    Engine::open_memory().await.expect("engine opens")
}

async fn plain(e: &Engine, slug: &str, quantity: f64) -> String {
    store::records::create(
        &e.store.pool,
        NewRecord {
            slug: Some(slug),
            kind: RecordKind::Plain,
            head: slug,
            body: "",
            quantity,
        },
    )
    .await
    .expect("record")
    .uid
}

fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

fn protein(source: Source) -> Protein {
    Protein {
        source,
        filter: vec![],
        include: Include::default(),
        aggregate: None,
        order: vec![],
        limit: None,
    }
}

async fn bump(e: &Engine, uid: &str, delta: f64, cause: Cause, now: DateTime<Utc>) {
    e.append(
        NewFact {
            actor_uid: None,
            ..NewFact::quantity(uid.to_string(), delta, cause)
        },
        now,
    )
    .await
    .expect("append");
}

#[tokio::test]
async fn fact_source_filters_and_aggregates_the_ledger() {
    let e = engine().await;
    let food = store::concepts::create(&e.store.pool, "food", &[])
        .await
        .unwrap();
    let apples = plain(&e, "apples.stock", 0.0).await;
    let hammer = plain(&e, "hammer", 0.0).await;
    store::records::set_concept(&e.store.pool, &apples, Some(&food))
        .await
        .unwrap();

    bump(&e, &apples, 10.0, Cause::user_edit(), at("2026-07-01T08:00:00Z")).await;
    bump(&e, &apples, -3.0, Cause::settlement("t_X"), at("2026-07-02T08:00:00Z")).await;
    bump(&e, &hammer, 1.0, Cause::user_edit(), at("2026-07-02T09:00:00Z")).await;

    // W-provenance: the facts of one record
    let mut q = protein(Source::Fact);
    q.filter = vec![Predicate::RecordEq("apples.stock".into())];
    let rows = protein::execute(&e.store, &q).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r["record"] == apples.as_str()));

    // at_since (absolute) narrows the window
    let mut q = protein(Source::Fact);
    q.filter = vec![Predicate::AtSince("2026-07-02T00:00:00Z".into())];
    assert_eq!(protein::execute(&e.store, &q).await.unwrap().len(), 2);

    // concept_in walks the Lingua DAG on the fact's record
    let mut q = protein(Source::Fact);
    q.filter = vec![Predicate::ConceptIn("food".into())];
    assert_eq!(protein::execute(&e.store, &q).await.unwrap().len(), 2);

    // W-finance: sum delta by cause_kind
    let mut q = protein(Source::Fact);
    q.aggregate = Some(Aggregate {
        op: AggregateOp::Sum,
        by: GroupBy::CauseKind,
    });
    let rows = protein::execute(&e.store, &q).await.unwrap();
    let get = |group: &str| {
        rows.iter()
            .find(|r| r["group"] == group)
            .map(|r| r["value"].as_f64().unwrap())
    };
    assert_eq!(get("user_edit"), Some(11.0));
    assert_eq!(get("settlement"), Some(-3.0));

    // ... and by calendar day
    let mut q = protein(Source::Fact);
    q.aggregate = Some(Aggregate {
        op: AggregateOp::Sum,
        by: GroupBy::Day,
    });
    let rows = protein::execute(&e.store, &q).await.unwrap();
    assert!(rows.iter().any(|r| r["group"] == "2026-07-01" && r["value"] == 10.0));
    assert!(rows.iter().any(|r| r["group"] == "2026-07-02" && r["value"] == -2.0));
}

#[tokio::test]
async fn concept_source_reads_the_lingua_dag() {
    let e = engine().await;
    let food = store::concepts::create(&e.store.pool, "food", &[])
        .await
        .unwrap();
    let fruit = store::concepts::create(&e.store.pool, "fruit", &[&food])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "apple", &[&fruit])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "tool", &[])
        .await
        .unwrap();

    let rows = protein::execute(&e.store, &protein(Source::Concept))
        .await
        .unwrap();
    assert_eq!(rows.len(), 4);
    let apple = rows.iter().find(|r| r["name"] == "apple").unwrap();
    assert_eq!(apple["parents"][0], fruit.as_str());

    // concept_in narrows to the family
    let mut q = protein(Source::Concept);
    q.filter = vec![Predicate::ConceptIn("food".into())];
    let names: Vec<String> = protein::execute(&e.store, &q)
        .await
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names.len(), 3);
    assert!(!names.contains(&"tool".to_string()));
}

#[tokio::test]
async fn transfer_source_derives_the_status_ladder() {
    let e = engine().await;
    plain(&e, "ana.apples", 10.0).await;
    plain(&e, "maria", 0.0).await;

    let transfer = e
        .act(
            Action::CreateTransfer {
                slug: Some("xfer.apples".into()),
                head: "Apples".into(),
                agreement: "full".into(),
                agreement_pct: None,
                satiation: None,
                source: None,
                reserve_default: None,
                require_confirmation: false,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();

    let status = |e: &Engine| {
        let store = e.store.clone();
        async move {
            let rows = protein::execute(&store, &protein(Source::Transfer))
                .await
                .unwrap();
            rows[0]["status"].as_str().unwrap().to_string()
        }
    };
    assert_eq!(status(&e).await, "draft", "a bundle with no promises");

    let party = e
        .act(
            Action::AddParty {
                transfer: transfer.clone(),
                actor: "maria".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let promise = e
        .act(
            Action::AddPromiseToTransfer {
                transfer: transfer.clone(),
                record: "ana.apples".into(),
                delta: -5.0,
                party: "maria".into(),
                window_end: None,
                condition: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    assert_eq!(status(&e).await, "proposed");

    // agreeing at level 2 also moves the party's bundled promises to agreed
    e.act(
        Action::AgreeTransfer {
            transfer: transfer.clone(),
            party: party.clone(),
            level: 2,
        },
        None,
    )
    .await
    .unwrap();
    let _ = &promise;
    assert_eq!(status(&e).await, "agreed", "policy satisfied, all agreed");

    e.act(
        Action::ActivateTransfer {
            transfer: transfer.clone(),
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(status(&e).await, "in_transfer");

    e.act(
        Action::SettleTransfer {
            transfer: transfer.clone(),
            actor: "maria".into(),
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(status(&e).await, "settled");

    e.act(
        Action::Deactivate {
            target: transfer.clone(),
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(status(&e).await, "inactive");
}

#[tokio::test]
async fn extension_and_projection_includes_attach() {
    let e = engine().await;
    let apples = plain(&e, "apples.stock", 5.0).await;
    e.act(
        Action::SetExtension {
            target: apples.clone(),
            namespace: "task.effort".into(),
            fds: serde_json::json!({ "points": 3 }),
        },
        None,
    )
    .await
    .unwrap();
    store::misc::insert_promise(
        &e.store.pool,
        store::misc::NewPromise {
            record_uid: Some(apples.clone()),
            delta: 3.0,
            window_end: Some("2026-08-01T00:00:00Z".into()),
            state: Some(PromiseState::Agreed),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let mut q = protein(Source::Record);
    q.filter = vec![Predicate::UidEq(apples.clone())];
    q.include = Include {
        extension: Some(ExtensionInclude {
            namespace: "task.effort".into(),
        }),
        projection: Some(ProjectionInclude {
            at: "2026-09-01T00:00:00Z".into(),
        }),
        ..Default::default()
    };
    let rows = protein::execute(&e.store, &q).await.unwrap();
    assert_eq!(rows[0]["extension"]["points"], 3);
    assert_eq!(
        rows[0]["projected"]["quantity"], 8.0,
        "the agreed +3 folds in by September"
    );

    // before the window closes, the promise does not count yet
    q.include.projection = Some(ProjectionInclude {
        at: "2026-07-15T00:00:00Z".into(),
    });
    let rows = protein::execute(&e.store, &q).await.unwrap();
    assert_eq!(rows[0]["projected"]["quantity"], 5.0);
}

#[tokio::test]
async fn link_depth_expands_the_tree() {
    let e = engine().await;
    plain(&e, "a", 0.0).await;
    plain(&e, "b", 0.0).await;
    plain(&e, "c", 0.0).await;
    store::concepts::create(&e.store.pool, "needs", &[])
        .await
        .unwrap();
    for (from, to) in [("a", "b"), ("b", "c")] {
        e.act(
            Action::AddLink {
                from: from.into(),
                kind: "needs".into(),
                to: to.into(),
                quantity: None,
            },
            None,
        )
        .await
        .unwrap();
    }

    let mut q = protein(Source::Record);
    q.filter = vec![Predicate::SlugEq("a".into())];
    q.include = Include {
        links: Some(LinksInclude {
            kind: None,
            kinds: vec!["needs".into()],
            direction: protein::LinkDirection::Out,
            depth: 0, // default: direct links only
        }),
        ..Default::default()
    };
    q.order = vec![Order::Asc("slug".into())];
    let rows = protein::execute(&e.store, &q).await.unwrap();
    assert_eq!(rows[0]["links"].as_array().unwrap().len(), 1);

    q.include.links.as_mut().unwrap().depth = 2;
    let rows = protein::execute(&e.store, &q).await.unwrap();
    let links = rows[0]["links"].as_array().unwrap();
    assert_eq!(links.len(), 2, "hop 2 reaches b -> c");
    assert!(links.iter().any(|l| l["hop"] == 1));
    assert!(links.iter().any(|l| l["hop"] == 2));
}
