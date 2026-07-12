//! Part V completion: the promise expiry sweep (heartbeat arm) and the
//! reserve_from default inheritance from the bundle's transfer.

use chrono::{DateTime, Utc};
use engine::Engine;
use engine::actions::Action;
use nucleus::{PromiseState, RecordKind};
use store::misc::NewPromise;
use store::records::NewRecord;

async fn engine() -> Engine {
    Engine::open_memory().await.expect("engine opens")
}

async fn plain(e: &Engine, slug: &str) -> String {
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
    .expect("record")
    .uid
}

fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

async fn promise(e: &Engine, record: &str, state: PromiseState, window_end: Option<&str>) -> String {
    store::misc::insert_promise(
        &e.store.pool,
        NewPromise {
            record_uid: Some(record.to_string()),
            delta: 5.0,
            window_end: window_end.map(String::from),
            state: Some(state),
            ..Default::default()
        },
    )
    .await
    .expect("promise")
}

#[tokio::test]
async fn expiry_breaks_commitments_and_withdraws_lapsed_offers() {
    let e = engine().await;
    let apples = plain(&e, "apples.stock").await;

    let agreed = promise(&e, &apples, PromiseState::Agreed, Some("2026-07-01T00:00:00Z")).await;
    let active = promise(&e, &apples, PromiseState::Active, Some("2026-07-01T00:00:00Z")).await;
    let open = promise(&e, &apples, PromiseState::Open, Some("2026-07-01T00:00:00Z")).await;
    let future = promise(&e, &apples, PromiseState::Agreed, Some("2027-01-01T00:00:00Z")).await;
    let windowless = promise(&e, &apples, PromiseState::Agreed, None).await;

    let now = at("2026-07-05T00:00:00Z");
    let facts = e.heartbeat(now).await.expect("heartbeat");

    let state = |uid: &str| {
        let pool = e.store.pool.clone();
        let uid = uid.to_string();
        async move { store::misc::promise_state(&pool, &uid).await.unwrap().unwrap() }
    };
    assert_eq!(state(&agreed).await, PromiseState::Broken);
    assert_eq!(state(&active).await, PromiseState::Broken);
    assert_eq!(state(&open).await, PromiseState::Withdrawn);
    assert_eq!(state(&future).await, PromiseState::Agreed, "future window untouched");
    assert_eq!(state(&windowless).await, PromiseState::Agreed, "no deadline, no expiry");

    // One zero-delta annotation fact per transition, quantity cache untouched.
    assert_eq!(facts.iter().filter(|f| f.delta == 0.0).count(), 3);
    assert_eq!(
        store::records::quantity(&e.store.pool, &apples).await.unwrap(),
        Some(0.0)
    );

    // Broken commitments enqueue decisions; the lapsed open offer does not.
    let decisions = store::misc::open_decisions(&e.store.pool).await.unwrap();
    let expiry: Vec<_> = decisions.iter().filter(|(_, kind, _)| kind == "expiry").collect();
    assert_eq!(expiry.len(), 2);

    // Idempotent: the next beat finds nothing left to expire.
    let facts = e.expire_promises(at("2026-07-06T00:00:00Z")).await.unwrap();
    assert!(facts.is_empty());
}

#[tokio::test]
async fn reserve_from_inherits_the_transfer_default() {
    let e = engine().await;
    plain(&e, "ana.apples").await;
    plain(&e, "ana").await;

    let transfer = e
        .act(
            Action::CreateTransfer {
                slug: Some("xfer.t".into()),
                head: "T".into(),
                agreement: "individual".into(),
                agreement_pct: None,
                satiation: None,
                source: None,
                reserve_default: Some("agreed".into()),
                require_confirmation: false,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();

    let bundled = e
        .act(
            Action::AddPromiseToTransfer {
                transfer: transfer.clone(),
                record: "ana.apples".into(),
                delta: -5.0,
                party: "ana".into(),
                window_end: None,
                condition: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let row = store::misc::get_promise(&e.store.pool, &bundled)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.reserve_from, "agreed", "inherited from the transfer");

    // A loose promise falls back to the global default.
    let loose = store::misc::insert_promise(
        &e.store.pool,
        NewPromise {
            record_uid: store::records::resolve(&e.store.pool, "ana.apples")
                .await
                .unwrap()
                .map(|r| r.uid),
            delta: 1.0,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let row = store::misc::get_promise(&e.store.pool, &loose)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.reserve_from, "active");

    // An explicit reserve_from beats the transfer default.
    let explicit = store::misc::insert_promise(
        &e.store.pool,
        NewPromise {
            record_uid: store::records::resolve(&e.store.pool, "ana.apples")
                .await
                .unwrap()
                .map(|r| r.uid),
            delta: 1.0,
            transfer_uid: Some(transfer),
            reserve_from: Some("proposed".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let row = store::misc::get_promise(&e.store.pool, &explicit)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.reserve_from, "proposed");
}
