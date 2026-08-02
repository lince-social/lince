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
            quantity: store::exact::zero(),
        },
    )
    .await
    .map(|r| r.uid)
    .expect("record")
    .pipe_bump(e, quantity)
    .await
}

async fn person(e: &Engine, slug: &str) -> String {
    store::records::create(
        &e.store.pool,
        NewRecord {
            slug: Some(slug),
            kind: RecordKind::Person,
            head: slug,
            body: "",
            quantity: store::exact::one(),
        },
    )
    .await
    .expect("person")
    .uid
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
    from.enqueue_sync_to(to_organ_in_from)
        .await
        .expect("enqueue");
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
    store::assertions::set_identity(&a.store.pool, &apples, Some(&apple), None)
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
        store::records::quantity(&b.store.pool, &apples)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(10.0)
    );
    assert_eq!(
        store::organs::quarantine_count(&b.store.pool)
            .await
            .unwrap(),
        0,
        "adopted keys verify the origin signatures"
    );

    // idempotent: re-sending changes nothing
    assert_eq!(wire_sync(&a, &b, &b_organ).await, 1);
    assert_eq!(
        store::records::quantity(&b.store.pool, &apples)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(10.0)
    );

    // ---- the discovery loop: A's open offer meets B's Need as a decision
    let ana = person(&a, "ana").await;
    store::misc::insert_promise(
        &a.store.pool,
        store::misc::NewPromise {
            record_uid: Some(apples.clone()),
            delta: 5.0, // an open Contribution
            party_uid: Some(ana),
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
    let bia = person(&b, "bia").await;
    store::assertions::set_identity(&b.store.pool, &my_apples, Some(&apple), None)
        .await
        .unwrap();
    store::misc::insert_promise(
        &b.store.pool,
        store::misc::NewPromise {
            record_uid: Some(my_apples),
            delta: -3.0,
            party_uid: Some(bia),
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
    package.facts[0].delta = store::exact::from_f64(500.0); // the tamper

    let applied = b.import_package(&package).await.unwrap();
    assert!(applied.is_empty(), "the tampered fact never lands");
    assert_eq!(
        store::organs::quarantine_count(&b.store.pool)
            .await
            .unwrap(),
        1,
        "…and is remembered in the quarantine list"
    );
    assert_eq!(
        store::records::quantity(&b.store.pool, &apples)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(0.0)
    );
}

/// E0.0: a Fact's declared precision must survive the sync wire, not just its
/// value. `1.50` at scale 2 and `1.5` at scale 1 are the same number and
/// different Facts, because the preimage carries the scale — so if any hop
/// round-tripped the delta through `f64` the imported chain would fail
/// verification and land in quarantine. Scale-0 test data (10, 3, 0) cannot
/// catch that; a trailing zero can.
#[tokio::test]
async fn declared_precision_survives_the_sync_wire() {
    let (a, a_organ) = cell("http://cell-precise-a").await;
    let (b, b_organ) = cell("http://cell-precise-b").await;

    let a_intro = a.introduction().await.unwrap();
    let b_intro = b.introduction().await.unwrap();
    b.adopt_introduction(&a_intro, 1).await.unwrap();
    a.adopt_introduction(&b_intro, 1).await.unwrap();
    store::organs::set_sync_policy(&a.store.pool, &b_organ, true, false)
        .await
        .unwrap();

    let grams = plain(&a, "flour.grams", 0.0).await;
    store::visibility::grant(&a.store.pool, "organ", Some(&b_organ), &grams)
        .await
        .unwrap();
    let _ = a_organ;

    // A delta whose canonical form keeps a trailing zero.
    let precise = nucleus::DecimalValue::parse_canonical(2, "1.50").unwrap();
    a.append(
        nucleus::NewFact::quantity(grams.clone(), precise, nucleus::Cause::user_edit()),
        chrono::Utc::now(),
    )
    .await
    .expect("append exact fact");

    assert_eq!(wire_sync(&a, &b, &b_organ).await, 1);

    assert_eq!(
        store::organs::quarantine_count(&b.store.pool)
            .await
            .unwrap(),
        0,
        "an exact delta's scale survives the wire, so signatures still verify"
    );

    let landed = store::records::quantity(&b.store.pool, &grams)
        .await
        .unwrap()
        .expect("record replicated");
    assert_eq!(landed.canonical(), "1.50", "trailing zero is not dropped");
    assert_eq!(landed.scale(), 2);
}
