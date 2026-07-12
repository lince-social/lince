//! Part XV acceptance: two Cells over the (in-memory) wire — introduction,
//! visibility-gated export through the outbox, hardened import, and the
//! discovery feed closing the loop into the Decision Queue.

use engine::Engine;
use engine::actions::{Action, ConceptSeed};
use engine::sync::Package;
use engine::trust::Signer;
use nucleus::{PromiseState, RecordKind};
use store::records::NewRecord;

async fn cell(base_url: &str) -> (Engine, String) {
    let e = Engine::open_memory().await.expect("engine opens");
    let organ = store::organs::ensure_local(&e.store.pool, base_url)
        .await
        .expect("local organ")
        .uid;
    e.set_signer(Signer::generate(&organ, "k1"))
        .await
        .expect("signer");
    (e, organ)
}

async fn plain(e: &Engine, slug: &str, quantity: f64) -> String {
    store::records::create(
        &e.store.pool,
        NewRecord {
            slug: Some(slug),
            kind: RecordKind::Plain,
            head: slug,
            body: "",
            quantity: 0.0,
        },
    )
    .await
    .map(|r| r.uid)
    .expect("record")
    .pipe_bump(e, quantity)
    .await
}

trait PipeBump {
    async fn pipe_bump(self, e: &Engine, quantity: f64) -> String;
}
impl PipeBump for String {
    async fn pipe_bump(self, e: &Engine, quantity: f64) -> String {
        if quantity != 0.0 {
            e.append_user(&self, quantity).await.expect("bump");
        }
        self
    }
}

/// The in-memory wire: what the HTTP boundary does in production.
async fn wire_sync(from: &Engine, to: &Engine, to_organ_in_from: &str) -> usize {
    from.enqueue_sync_to(to_organ_in_from).await.expect("enqueue");
    from.drain_outbox(|_contact, package| async move {
        to.import_package(&package)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .expect("drain")
}

#[tokio::test]
async fn donation_flows_between_two_cells_and_feeds_the_decision_queue() {
    let (a, a_organ) = cell("http://cell-a").await;
    let (b, b_organ) = cell("http://cell-b").await;

    // introduction: exchange identity + keys, register contacts by REMOTE uid
    let a_intro = a.introduction().await.unwrap();
    let b_intro = b.introduction().await.unwrap();
    b.adopt_introduction(&a_intro, 1).await.unwrap();
    a.adopt_introduction(&b_intro, 1).await.unwrap();
    store::organs::set_sync_policy(&a.store.pool, &b_organ, true, false)
        .await
        .unwrap();

    // Ana's Cell: 10 apples, tagged @apple, visible to B's organ
    let apple = store::concepts::create(&a.store.pool, "apple", &[])
        .await
        .unwrap();
    let apples = plain(&a, "apples.stock", 10.0).await;
    store::records::set_concept(&a.store.pool, &apples, Some(&apple))
        .await
        .unwrap();
    store::visibility::grant(&a.store.pool, "organ", Some(&b_organ), &apples)
        .await
        .unwrap();

    // DONATION over the wire: the outbox drains through the boundary
    assert_eq!(wire_sync(&a, &b, &b_organ).await, 1);

    // B has the record by the SAME uid, deltas applied, signatures verified
    let imported = store::records::get(&b.store.pool, &apples).await.unwrap();
    assert!(imported.is_some(), "identity replicates by uid");
    assert_eq!(
        store::records::quantity(&b.store.pool, &apples).await.unwrap(),
        Some(10.0)
    );
    assert_eq!(
        store::organs::quarantine_count(&b.store.pool).await.unwrap(),
        0,
        "adopted keys verify the origin signatures"
    );

    // idempotent: re-sending changes nothing
    assert_eq!(wire_sync(&a, &b, &b_organ).await, 1);
    assert_eq!(
        store::records::quantity(&b.store.pool, &apples).await.unwrap(),
        Some(10.0)
    );

    // ---- the discovery loop: A's open offer meets B's Need as a decision
    store::misc::insert_promise(
        &a.store.pool,
        store::misc::NewPromise {
            record_uid: Some(apples.clone()),
            delta: 5.0, // an open Contribution
            state: Some(PromiseState::Open),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // B adopts A's concept (uid + lineage) and has a complementary Need
    b.act(
        Action::AdoptConcepts {
            concepts: vec![ConceptSeed {
                uid: apple.clone(),
                name: "apple".into(),
                origin: Some(a_organ.clone()),
                parents: vec![],
            }],
        },
        None,
    )
    .await
    .unwrap();
    let my_apples = plain(&b, "my.apples", -3.0).await;
    store::records::set_concept(&b.store.pool, &my_apples, Some(&apple))
        .await
        .unwrap();
    store::misc::insert_promise(
        &b.store.pool,
        store::misc::NewPromise {
            record_uid: Some(my_apples),
            delta: -3.0,
            state: Some(PromiseState::Open),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    b.act(
        Action::CreateMatchRule {
            slug: "senses.nearby".into(),
            head: "Nearby".into(),
            watch_concept: None,
            max_proximity: 1,
            min_confidence: 0.0,
            auto: "draft_only".into(),
        },
        None,
    )
    .await
    .unwrap();

    // B pulls A's visible open promises into its discovery cache (the wire's
    // pull side; proximity stamped from B's own contact row)
    let fetched = a.open_promise_export(&b_organ).await.unwrap();
    assert_eq!(fetched.len(), 1, "only what visibility allows travels");
    b.refresh_discovery(&a_organ, fetched).await.unwrap();

    let drafts = b.senses_pass().await.unwrap();
    assert_eq!(drafts.len(), 1, "the offer meets the Need in the queue");

    // ---- blocked rejects everything everywhere
    store::organs::set_trust(&b.store.pool, &a_organ, "blocked")
        .await
        .unwrap();
    let package = a.export_package(&b_organ, &a_organ).await.unwrap();
    assert!(b.import_package(&package).await.is_err());
    assert!(b.refresh_discovery(&a_organ, vec![]).await.is_err());
}

#[tokio::test]
async fn tampered_facts_are_quarantined_on_import() {
    let (a, a_organ) = cell("http://cell-a").await;
    let (b, b_organ) = cell("http://cell-b").await;
    b.adopt_introduction(&a.introduction().await.unwrap(), 1)
        .await
        .unwrap();

    let apples = plain(&a, "apples.stock", 10.0).await;
    store::visibility::grant(&a.store.pool, "organ", Some(&b_organ), &apples)
        .await
        .unwrap();
    let mut package: Package = a.export_package(&b_organ, &a_organ).await.unwrap();
    assert!(!package.facts.is_empty());
    package.facts[0].delta = 500.0; // the tamper

    let applied = b.import_package(&package).await.unwrap();
    assert!(applied.is_empty(), "the tampered fact never lands");
    assert_eq!(
        store::organs::quarantine_count(&b.store.pool).await.unwrap(),
        1,
        "…and is remembered in the quarantine list"
    );
    assert_eq!(
        store::records::quantity(&b.store.pool, &apples).await.unwrap(),
        Some(0.0)
    );
}
