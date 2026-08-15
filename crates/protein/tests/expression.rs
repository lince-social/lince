//! Protein + Actions end-to-end (blueprint Stage 3 acceptance): writes go
//! through typed Actions, reads come back through Proteins, and the two never
//! trade places.

use engine::Engine;
use engine::actions::Action;
use nucleus::{PromiseState, RecordKind};

use protein::{FactsInclude, Include, Order, Predicate, Protein, Source};

async fn engine() -> Engine {
    let engine = Engine::open_memory().await.expect("engine opens");
    // An open promise is an offer somebody makes, so publishing one needs a
    // Person to make it.
    store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: Some("me"),
            kind: RecordKind::Person,
            head: "me",
            body: "",
            quantity: store::exact::one(),
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
    // three active Needs; order lives on the records, arrival on quantity
    create(&e, "exercise", -1.0).await;
    create(&e, "shower", -1.0).await;
    create(&e, "breakfast", -1.0).await;
    create(&e, "someday", 0.0).await; // not a Need: never appears

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
    e.act(
        Action::AssertRecord {
            subject: "exercise".into(),
            predicate: "before".into(),
            object: Some("shower".into()),
            quantity: None,
            unit: None,
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::AssertRecord {
            subject: "shower".into(),
            predicate: "before".into(),
            object: Some("breakfast".into()),
            quantity: None,
            unit: None,
        },
        None,
    )
    .await
    .unwrap();

    let queue = protein::execute(&e.store, &protein::focus_queue("before"))
        .await
        .unwrap();
    let slugs: Vec<&str> = queue.iter().map(|r| r["slug"].as_str().unwrap()).collect();
    assert_eq!(slugs, vec!["exercise", "shower", "breakfast"]);

    // completing the focus (a SetQuantity Action -> fact) promotes the next
    e.act(
        Action::SetQuantity {
            target: "@exercise".into(),
            value: 0.0,
        },
        None,
    )
    .await
    .unwrap();
    let queue = protein::execute(&e.store, &protein::focus_queue("before"))
        .await
        .unwrap();
    assert_eq!(queue[0]["slug"], "shower", "next task takes the focus");
    assert_eq!(queue.len(), 2);
}

#[tokio::test]
async fn concept_dag_filter_and_provenance_include() {
    let e = engine().await;
    let apples = create(&e, "apples.stock", -2.0).await;
    create(&e, "hammer", -1.0).await;

    // Lingua: apple -> fruit -> food
    e.act(
        Action::CreateConcept {
            lingua: "g_local".into(),
            name: "food".into(),
            parents: vec![],
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::CreateConcept {
            lingua: "g_local".into(),
            name: "fruit".into(),
            parents: vec!["food".into()],
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::CreateConcept {
            lingua: "g_local".into(),
            name: "apple".into(),
            parents: vec!["fruit".into()],
        },
        None,
    )
    .await
    .unwrap();
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
            Predicate::QuantityLt(0.0),
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
    let rows = protein::execute(&e.store, &p).await.unwrap();
    // `concept_in @food` matches @apple through the DAG; the hammer does not
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["slug"], "apples.stock");
    // provenance is one include away: the creation fact with its cause
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
            consequences: vec![nucleus::karma::Consequence::Ask {
                question: Some("send reorder proposal?".into()),
                options: Vec::new(),
            }],
            condition: Some("@apples.stock".into()),
            gate: Some("<3".into()),
            carry: Some("one".into()),
            note: None,
            cadence: nucleus::karma::Cadence::every_days(1),
            anchor_at: Some((chrono::Utc::now() - chrono::TimeDelta::minutes(1)).to_rfc3339()),
            request_id: Some(nucleus::new_uid("req")),
        },
        None,
    )
    .await
    .unwrap();
    e.append_user(&apples, -1.0).await.unwrap(); // fires the ask

    let queue = protein::execute(&e.store, &protein::decision_queue())
        .await
        .unwrap();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0]["question"], "send reorder proposal?");

    // answering is an Action; the queue empties; the answer is Ledger-visible
    let decision_uid = queue[0]["uid"].as_str().unwrap().to_string();
    e.act(
        Action::Decide {
            decision: decision_uid,
            answer: "yes".into(),
        },
        None,
    )
    .await
    .unwrap();
    let queue = protein::execute(&e.store, &protein::decision_queue())
        .await
        .unwrap();
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
                party: Some("me".to_string()),
                open: true, // a published Need: unfilled party slot
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();

    // open -> proposed -> agreed -> active: each transition validated
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
    // kept is settlement-only: the state machine refuses it here
    let err = e
        .act(
            Action::PromiseTransition {
                promise: promise.clone(),
                to: PromiseState::Open,
            },
            None,
        )
        .await;
    assert!(err.is_err(), "active -> open is not a legal transition");

    // the promise is visible through the promise-source Protein
    let p = Protein {
        source: Source::Promise,
        filter: vec![Predicate::StateIn(vec!["active".into()])],
        fields: None,
        include: Include::default(),
        aggregate: None,
        order: vec![],
        limit: None,
    };
    let rows = protein::execute(&e.store, &p).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["delta"], 5.0);
}

#[tokio::test]
async fn the_wire_format_is_json_all_the_way() {
    // a sand ships this JSON; the host parses it into a Protein — the contract
    let json = serde_json::json!({
        "source": "record",
        "where": [ { "quantity_lt": 0.0 }, { "kind_eq": "plain" } ],
        "include": { "facts": { "limit": 3 } },
        "order": [ { "link": { "kind": "before", "higher": "from" } }, { "asc": "created_at" } ],
        "limit": 10
    });
    let parsed: Protein = serde_json::from_value(json).unwrap();
    assert!(matches!(parsed.source, Source::Record));
    assert_eq!(parsed.filter.len(), 2);
    assert!(matches!(parsed.order[0], Order::Link(_)));
    assert_eq!(parsed.limit, Some(10));
}
