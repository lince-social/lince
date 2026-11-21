use chrono::{TimeDelta, Utc};
use engine::{Engine, actions::Action};
use nucleus::{
    PromiseState, RecordKind,
    karma::{Cadence, Consequence},
    new_uid,
};

use protein::{
    FactsInclude, Include, Order, Predicate, Protein, Source, decision_queue, execute, focus_queue,
};
use serde_json::{from_value, json};
use store::{exact, records::NewRecord};

async fn engine() -> Engine {
    let engine = Engine::open_memory().await.expect("engine opens");
    store::records::create(
        &engine.store.pool,
        NewRecord {
            slug: Some("me"),
            kind: RecordKind::Person,
            head: "me",
            body: "",
            quantity: exact::one(),
        },
    )
    .await
    .expect("a local Person");
    engine
}

async fn create(e: &Engine, slug: &str, quantity: f64) -> String {
    e.act(
        Action::CreateRecord {
            slug: Some(slug.into()),
            kind: RecordKind::Plain,
            head: slug.into(),
            body: String::new(),
            quantity,
        },
        None,
    )
    .await
    .expect("create")
    .created
    .expect("uid")
}

#[tokio::test]
async fn focus_queue_is_a_protein() {
    let e = engine().await;
    for slug in ["exercise", "shower", "breakfast"] {
        create(&e, slug, -1.0).await;
    }
    create(&e, "someday", 0.0).await;

    e.act(
        Action::CreateConcept {
            lingua: "g_local".into(),
            name: "before".into(),
            parents: vec![],
        },
        None,
    )
    .await
    .unwrap();
    for (subject, object) in [("exercise", "shower"), ("shower", "breakfast")] {
        e.act(
            Action::AssertRecord {
                subject: subject.into(),
                predicate: "before".into(),
                object: Some(object.into()),
                quantity: None,
                unit: None,
            },
            None,
        )
        .await
        .unwrap();
    }

    let queue = execute(&e.store, &focus_queue("before")).await.unwrap();
    assert!(
        queue
            .iter()
            .map(|row| &row["slug"])
            .eq(["exercise", "shower", "breakfast"].iter())
    );

    e.act(
        Action::SetQuantity {
            target: "@exercise".into(),
            value: 0.0,
        },
        None,
    )
    .await
    .unwrap();
    let queue = execute(&e.store, &focus_queue("before")).await.unwrap();
    assert_eq!(queue[0]["slug"], "shower", "next task takes the focus");
    assert_eq!(queue.len(), 2);
}

#[tokio::test]
async fn concept_dag_filter_and_provenance_include() {
    let e = engine().await;
    let apples = create(&e, "apples.stock", -2.0).await;
    create(&e, "hammer", -1.0).await;

    for (name, parent) in [
        ("food", None),
        ("fruit", Some("food")),
        ("apple", Some("fruit")),
    ] {
        e.act(
            Action::CreateConcept {
                lingua: "g_local".into(),
                name: name.into(),
                parents: parent.into_iter().map(str::to_owned).collect(),
            },
            None,
        )
        .await
        .unwrap();
    }
    let apple_uid = store::concepts::resolve(&e.store.pool, "apple")
        .await
        .unwrap()
        .unwrap();
    store::assertions::set_identity(&e.store.pool, &apples, Some(&apple_uid), None)
        .await
        .unwrap();

    let p = Protein {
        source: Source::Record,
        filter: vec![
            Predicate::QuantityLt(exact::zero()),
            Predicate::ConceptIn("food".into()),
        ],
        fields: None,
        include: Include {
            facts: Some(FactsInclude { limit: 5 }),
            ..Default::default()
        },
        aggregate: None,
        order: vec![],
        limit: None,
    };
    let rows = execute(&e.store, &p).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["slug"], "apples.stock");
    let facts = rows[0]["facts"].as_array().unwrap();
    assert!(!facts.is_empty());
    assert_eq!(facts[0]["cause_kind"], "user_edit");
}

#[tokio::test]
async fn decision_queue_protein_and_decide_action() {
    let e = engine().await;
    let apples = create(&e, "apples.stock", 2.0).await;
    e.act(
        Action::CreateRecurrence {
            target: apples.clone(),
            consequences: vec![Consequence::Ask {
                question: Some("send reorder proposal?".into()),
                options: vec![],
            }],
            condition: Some("@apples.stock".into()),
            gate: Some("<3".into()),
            carry: Some("one".into()),
            note: None,
            cadence: Cadence::every_days(1),
            anchor_at: Some((Utc::now() - TimeDelta::minutes(1)).to_rfc3339()),
            request_id: Some(new_uid("req")),
        },
        None,
    )
    .await
    .unwrap();
    e.append_user(&apples, -1.0).await.unwrap();

    let queue = execute(&e.store, &decision_queue()).await.unwrap();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0]["question"], "send reorder proposal?");

    e.act(
        Action::Decide {
            decision: queue[0]["uid"].as_str().unwrap().into(),
            answer: "yes".into(),
        },
        None,
    )
    .await
    .unwrap();
    let queue = execute(&e.store, &decision_queue()).await.unwrap();
    assert!(queue.is_empty());
}

#[tokio::test]
async fn promise_lifecycle_through_actions() {
    let e = engine().await;
    create(&e, "apples.stock", 8.0).await;

    let promise = e
        .act(
            Action::CreatePromise {
                record: "@apples.stock".into(),
                delta: 5.0,
                window_end: Some("2026-07-10T18:00:00Z".into()),
                party: Some("me".into()),
                open: true,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();

    for state in [
        PromiseState::Proposed,
        PromiseState::Agreed,
        PromiseState::Active,
    ] {
        e.act(
            Action::PromiseTransition {
                promise: promise.clone(),
                to: state,
            },
            None,
        )
        .await
        .unwrap();
    }
    let err = e
        .act(
            Action::PromiseTransition {
                promise,
                to: PromiseState::Open,
            },
            None,
        )
        .await;
    assert!(err.is_err(), "active -> open is not a legal transition");

    let p = Protein {
        source: Source::Promise,
        filter: vec![Predicate::StateIn(vec!["active".into()])],
        fields: None,
        include: Include::default(),
        aggregate: None,
        order: vec![],
        limit: None,
    };
    let rows = execute(&e.store, &p).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["delta"], 5.0);
}

#[test]
fn the_wire_format_is_json_all_the_way() {
    let json = json!({
        "source": "record",
        "where": [ { "quantity_lt": "0.0" }, { "kind_eq": "plain" } ],
        "include": { "facts": { "limit": 3 } },
        "order": [ { "link": { "kind": "before", "higher": "from" } }, { "asc": "created_at" } ],
        "limit": 10
    });
    let parsed: Protein = from_value(json).unwrap();
    assert!(matches!(parsed.source, Source::Record));
    assert_eq!(parsed.filter.len(), 2);
    assert!(matches!(parsed.order[0], Order::Link(_)));
    assert_eq!(parsed.limit, Some(10));
}
