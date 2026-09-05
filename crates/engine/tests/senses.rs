use engine::Engine;
use engine::actions::Action;
use engine::senses::{MatchRule, RemoteOpen};
use nucleus::RecordKind;

async fn engine() -> Engine {
    let engine = Engine::open_memory().await.unwrap();
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

async fn concept(e: &Engine, name: &str, parents: Vec<String>) -> String {
    e.act(
        Action::CreateConcept {
            lingua: "g_local".into(),
            name: name.into(),
            parents,
        },
        None,
    )
    .await
    .unwrap()
    .created
    .unwrap()
}

async fn open_need(e: &Engine, slug: &str, concept_name: &str, delta: f64) -> String {
    let rec = e
        .act(
            Action::CreateRecord {
                slug: Some(slug.into()),
                kind: RecordKind::Plain,
                head: slug.into(),
                body: String::new(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let cuid = store::concepts::resolve(&e.store.pool, concept_name)
        .await
        .unwrap()
        .unwrap();
    store::assertions::set_identity(&e.store.pool, &rec, Some(&cuid), None)
        .await
        .unwrap();
    e.act(
        Action::CreatePromise {
            record: rec,
            delta,
            window_end: Some("2026-07-10T00:00:00Z".into()),
            party: Some("me".to_string()),
            open: true,
        },
        None,
    )
    .await
    .unwrap()
    .created
    .unwrap()
}

#[tokio::test]
async fn matches_complementary_promises_within_proximity() {
    let e = engine().await;
    concept(&e, "food", vec![]).await;
    let apple = concept(&e, "apple", vec!["food".into()]).await;
    let need = open_need(&e, "my.apples", "apple", -3.0).await;

    let cache = vec![
        RemoteOpen {
            promise_uid: "p_neighbor".into(),
            organ: "organ.neighbor".into(),
            proximity: 1,
            concept: Some(apple.clone()),
            unit: None,
            delta: 5.0,
            window_start: None,
            window_end: Some("2026-07-09T00:00:00Z".into()),
            confidence: 0.9,
        },
        RemoteOpen {
            promise_uid: "p_faraway".into(),
            organ: "organ.faraway".into(),
            proximity: 5,
            concept: Some(apple.clone()),
            unit: None,
            delta: 5.0,
            window_start: None,
            window_end: None,
            confidence: 0.9,
        },
    ];

    let rule = MatchRule {
        watch_concept: Some("food".into()),
        max_proximity: 2,
        min_confidence: 0.5,
    };
    let drafts = e.senses_match(&rule, &cache).await.unwrap();

    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].local_promise, need);
    assert_eq!(drafts[0].remote_promise, "p_neighbor");
    assert_eq!(drafts[0].organ, "organ.neighbor");
}

#[tokio::test]
async fn same_sign_promises_never_match() {
    let e = engine().await;
    concept(&e, "apple", vec![]).await;
    open_need(&e, "my.apples", "apple", -3.0).await;
    let apple = store::concepts::resolve(&e.store.pool, "apple")
        .await
        .unwrap()
        .unwrap();
    let cache = vec![RemoteOpen {
        promise_uid: "p_also_wants".into(),
        organ: "organ.x".into(),
        proximity: 1,
        concept: Some(apple),
        unit: None,
        delta: -2.0,
        window_start: None,
        window_end: None,
        confidence: 1.0,
    }];
    let rule = MatchRule {
        max_proximity: 1,
        ..Default::default()
    };
    assert!(e.senses_match(&rule, &cache).await.unwrap().is_empty());
}

#[tokio::test]
async fn concept_dag_lets_a_specific_offer_meet_a_general_need() {
    let e = engine().await;
    concept(&e, "fruit", vec![]).await;
    let apple = concept(&e, "apple", vec!["fruit".into()]).await;
    open_need(&e, "my.fruit", "fruit", -1.0).await;
    let cache = vec![RemoteOpen {
        promise_uid: "p_apples".into(),
        organ: "organ.n".into(),
        proximity: 1,
        concept: Some(apple),
        unit: None,
        delta: 4.0,
        window_start: None,
        window_end: None,
        confidence: 0.7,
    }];
    let rule = MatchRule {
        max_proximity: 1,
        min_confidence: 0.5,
        ..Default::default()
    };
    let drafts = e.senses_match(&rule, &cache).await.unwrap();
    assert_eq!(
        drafts.len(),
        1,
        "apple (specific) meets the fruit (general) Need via the DAG"
    );
}
