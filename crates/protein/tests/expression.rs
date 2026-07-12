//! Protein + Actions end-to-end (blueprint Stage 3 acceptance): writes go
//! through typed Actions, reads come back through Proteins, and the two never
//! trade places.

use engine::Engine;
use engine::actions::Action;
use nucleus::{ConsequenceKind, PromiseState, RecordKind};
use protein::{FactsInclude, Include, Order, Predicate, Protein, Source};

async fn engine() -> Engine {
    Engine::open_memory().await.expect("engine opens")
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
            name: "before".into(),
            parents: vec![],
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::AddLink {
            from: "exercise".into(),
            kind: "before".into(),
            to: "shower".into(),
            quantity: None,
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::AddLink {
            from: "shower".into(),
            kind: "before".into(),
            to: "breakfast".into(),
            quantity: None,
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
            name: "food".into(),
            parents: vec![],
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::CreateConcept {
            name: "fruit".into(),
            parents: vec!["food".into()],
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::CreateConcept {
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
    store::sqlx::query("UPDATE record SET concept_uid = ? WHERE uid = ?")
        .bind(&apple_uid)
        .bind(&apples)
        .execute(&e.store.pool)
        .await
        .unwrap();

    let p = Protein {
        source: Source::Record,
        filter: vec![
            Predicate::QuantityLt(0.0),
            Predicate::ConceptIn("food".into()),
        ],
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
    store::rules::create(
        &e.store.pool,
        store::rules::NewRule {
            slug: "rules.reorder-ask",
            head: "Ask before reorder",
            condition: "@apples.stock",
            gate: "<3",
            carry: "one",
            consequences: vec![(
                ConsequenceKind::Ask,
                None,
                Some(serde_json::json!({ "question": "send reorder proposal?" })),
            )],
        },
    )
    .await
    .unwrap();
    e.reload_rules().await.unwrap();
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
                party: None,
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
        "order": [ { "topo": "before" }, { "asc": "created_at" } ],
        "limit": 10
    });
    let parsed: Protein = serde_json::from_value(json).unwrap();
    assert!(matches!(parsed.source, Source::Record));
    assert_eq!(parsed.filter.len(), 2);
    assert!(matches!(parsed.order[0], Order::Topo(_)));
    assert_eq!(parsed.limit, Some(10));
}
