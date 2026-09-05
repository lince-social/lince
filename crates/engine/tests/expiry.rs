use chrono::{DateTime, Utc};
use engine::Engine;
use engine::actions::{
    Action, TransferPromiseInput, TransferReservePoint, TransferSatiation, TransferVisibility,
};
use engine::trust::Signer;
use nucleus::transfer::{AgreementType, OpenPromiseReusePolicy};
use nucleus::{PromiseState, RecordKind};
use store::misc::NewPromise;
use store::records::NewRecord;

async fn engine() -> Engine {
    let engine = Engine::open_memory().await.expect("engine opens");
    store::organs::ensure_local(&engine.store.pool, "http://expiry.test")
        .await
        .expect("local Organ");
    engine
}

async fn plain(e: &Engine, slug: &str) -> String {
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
    .expect("record")
    .uid
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

fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

async fn promise(
    e: &Engine,
    record: &str,
    party: &str,
    state: PromiseState,
    window_end: Option<&str>,
) -> String {
    store::misc::insert_promise(
        &e.store.pool,
        NewPromise {
            record_uid: Some(record.to_string()),
            party_uid: Some(party.to_string()),
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
    let owner = person(&e, "expiry.owner").await;

    let agreed = promise(
        &e,
        &apples,
        &owner,
        PromiseState::Agreed,
        Some("2026-07-01T00:00:00Z"),
    )
    .await;
    let active = promise(
        &e,
        &apples,
        &owner,
        PromiseState::Active,
        Some("2026-07-01T00:00:00Z"),
    )
    .await;
    let open = promise(
        &e,
        &apples,
        &owner,
        PromiseState::Open,
        Some("2026-07-01T00:00:00Z"),
    )
    .await;
    let future = promise(
        &e,
        &apples,
        &owner,
        PromiseState::Agreed,
        Some("2027-01-01T00:00:00Z"),
    )
    .await;
    let windowless = promise(&e, &apples, &owner, PromiseState::Agreed, None).await;

    let now = at("2026-07-05T00:00:00Z");
    let facts = e.heartbeat(now).await.expect("heartbeat");

    let state = |uid: &str| {
        let pool = e.store.pool.clone();
        let uid = uid.to_string();
        async move {
            store::misc::promise_state(&pool, &uid)
                .await
                .unwrap()
                .unwrap()
        }
    };
    assert_eq!(state(&agreed).await, PromiseState::Broken);
    assert_eq!(state(&active).await, PromiseState::Broken);
    assert_eq!(state(&open).await, PromiseState::Withdrawn);
    assert_eq!(
        state(&future).await,
        PromiseState::Agreed,
        "future window untouched"
    );
    assert_eq!(
        state(&windowless).await,
        PromiseState::Agreed,
        "no deadline, no expiry"
    );

    assert_eq!(
        facts
            .iter()
            .filter(|f| f.delta == store::exact::from_f64(0.0))
            .count(),
        3
    );
    assert_eq!(
        store::records::quantity(&e.store.pool, &apples)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(0.0)
    );

    let decisions = store::misc::open_decisions(&e.store.pool).await.unwrap();
    let expiry: Vec<_> = decisions
        .iter()
        .filter(|(_, kind, _)| kind == "expiry")
        .collect();
    assert_eq!(expiry.len(), 2);

    let facts = e.expire_promises(at("2026-07-06T00:00:00Z")).await.unwrap();
    assert!(facts.is_empty());
}

#[tokio::test]
async fn reserve_from_inherits_the_transfer_default() {
    let e = engine().await;
    let apples = plain(&e, "ana.apples").await;
    let ana = person(&e, "ana").await;
    e.set_signer(Signer::generate(&ana, "test:expiry:ana"))
        .await
        .unwrap();

    let transfer = e
        .act(
            Action::CreateTransferDraft {
                request_id: "expiry:create".into(),
                creator: Some(ana.clone()),
                slug: Some("xfer.t".into()),
                head: "T".into(),
                agreement: AgreementType::Individual,
                agreement_pct: None,
                satiation: TransferSatiation::None,
                parent: None,
                source: None,
                visibility: TransferVisibility::Hidden,
                max_proximity: None,
                reserve_default: TransferReservePoint::Agreed,
                require_confirmation: false,
                default_place: None,
                invitees: Vec::new(),
                promises: vec![TransferPromiseInput {
                    uid: Some("expiry-bundled".into()),
                    record: apples.clone(),
                    party: Some(ana),
                    open: false,
                    delta: -5.0,
                    unit: None,
                    window_start: None,
                    window_end: None,
                    place: None,
                    condition: None,
                    reserve_from: Some(TransferReservePoint::Inherit),
                    reuse_policy: OpenPromiseReusePolicy::Duplicate,
                    withdrawn: false,
                }],
                dependencies: Vec::new(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();

    let row = store::misc::get_promise(&e.store.pool, "expiry-bundled")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.reserve_from, "agreed", "inherited from the transfer");

    store::config::set_transfer_reservation_default(&e.store.pool, "active")
        .await
        .unwrap();
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
