//! Transfer (blueprint VIII) + Trust (XI) + Imagination (XII) acceptance.

use chrono::{DateTime, TimeDelta, Utc};
use engine::actions::Action;
use engine::trust::{self, Signer};
use engine::Engine;
use nucleus::RecordKind;

async fn engine() -> Engine {
    Engine::open_memory().await.expect("engine")
}

async fn person(e: &Engine, slug: &str) -> String {
    e.act(
        Action::CreateRecord {
            slug: Some(slug.into()),
            kind: RecordKind::Person,
            head: slug.into(),
            body: String::new(),
            quantity: 1.0,
        },
        None,
    )
    .await
    .unwrap()
    .created
    .unwrap()
}

async fn plain(e: &Engine, slug: &str, q: f64) -> String {
    e.act(
        Action::CreateRecord {
            slug: Some(slug.into()),
            kind: RecordKind::Plain,
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

fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

#[tokio::test]
async fn a_sale_settles_only_through_agreement() {
    let e = engine().await;
    let ana = person(&e, "ana").await;
    let carlos = person(&e, "carlos").await;
    let bike = plain(&e, "ana.bike", 1.0).await;
    let ana_money = plain(&e, "ana.money", 0.0).await;

    // SALE bundle: Ana gives the bike, receives 300
    let transfer = e
        .act(
            Action::CreateTransfer {
                slug: Some("xfer.bike-sale".into()),
                head: "Bike sale".into(),
                agreement: "full".into(),
                agreement_pct: None,
                satiation: None,
                source: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let ana_party = e
        .act(Action::AddParty { transfer: transfer.clone(), actor: ana.clone() }, None)
        .await
        .unwrap()
        .created
        .unwrap();
    let _carlos_party = e
        .act(Action::AddParty { transfer: transfer.clone(), actor: carlos.clone() }, None)
        .await
        .unwrap()
        .created
        .unwrap();
    e.act(
        Action::AddPromiseToTransfer {
            transfer: transfer.clone(),
            record: bike.clone(),
            delta: -1.0,
            party: ana.clone(),
            window_end: None,
            condition: None,
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::AddPromiseToTransfer {
            transfer: transfer.clone(),
            record: ana_money.clone(),
            delta: 300.0,
            party: ana.clone(),
            window_end: None,
            condition: None,
        },
        None,
    )
    .await
    .unwrap();

    // settlement refused before agreement (full policy: nobody committed)
    assert!(e.settle_all_local(&transfer, &ana, Utc::now()).await.is_err());

    // both parties reach level 2; Ana's promises become agreed
    for party in [&ana_party] {
        e.act(
            Action::AgreeTransfer { transfer: transfer.clone(), party: party.clone(), level: 2 },
            None,
        )
        .await
        .unwrap();
    }
    // carlos party uid: fetch it
    let carlos_party = store::transfers::party_levels(&e.store.pool, &transfer)
        .await
        .unwrap()
        .into_iter()
        .find(|(_, actor, _)| actor == &carlos)
        .unwrap()
        .0;
    e.act(Action::AgreeTransfer { transfer: transfer.clone(), party: carlos_party, level: 2 }, None)
        .await
        .unwrap();

    assert!(e.transfer_agreed(&transfer).await.unwrap());
    e.act(Action::ActivateTransfer { transfer: transfer.clone() }, None).await.unwrap();

    // settle Ana's side: the bike leaves, money arrives — the ONLY record mutation
    let facts = e.settle_all_local(&transfer, &ana, Utc::now()).await.unwrap();
    assert_eq!(facts.len(), 2);
    assert_eq!(store::records::quantity(&e.store.pool, &bike).await.unwrap(), Some(0.0));
    assert_eq!(store::records::quantity(&e.store.pool, &ana_money).await.unwrap(), Some(300.0));

    // idempotent: promises are kept now, re-settling changes nothing
    let again = e.settle_all_local(&transfer, &ana, Utc::now()).await.unwrap();
    assert!(again.is_empty());
}

#[tokio::test]
async fn editing_a_bundled_promise_invalidates_agreement() {
    let e = engine().await;
    let ana = person(&e, "ana").await;
    let apples = plain(&e, "ana.apples", 10.0).await;
    let transfer = e
        .act(
            Action::CreateTransfer {
                slug: Some("xfer.t".into()),
                head: "T".into(),
                agreement: "full".into(),
                agreement_pct: None,
                satiation: None,
                source: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let party = e
        .act(Action::AddParty { transfer: transfer.clone(), actor: ana.clone() }, None)
        .await
        .unwrap()
        .created
        .unwrap();
    let promise = e
        .act(
            Action::AddPromiseToTransfer {
                transfer: transfer.clone(),
                record: apples,
                delta: -5.0,
                party: ana,
                window_end: None,
                condition: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    e.act(Action::AgreeTransfer { transfer: transfer.clone(), party, level: 2 }, None)
        .await
        .unwrap();
    assert!(e.transfer_agreed(&transfer).await.unwrap());

    // a counteroffer is an edit: agreement drops back to 0
    e.act(Action::EditPromiseDelta { promise, delta: -3.0 }, None).await.unwrap();
    assert!(!e.transfer_agreed(&transfer).await.unwrap(), "edit invalidated agreement");
}

#[tokio::test]
async fn every_fact_is_signed_and_verifiable() {
    let e = engine().await;
    e.set_signer(Signer::generate("ana", "ed25519:ana:2026-07")).await.unwrap();
    let apples = plain(&e, "apples", 8.0).await;

    let facts = e.append_user(&apples, -1.0).await.unwrap();
    let fact = &facts[0];
    assert!(fact.signature.is_some(), "facts are signed");
    assert_eq!(fact.actor_uid.as_deref(), Some("ana"));
    assert!(trust::verify_fact(&e.store, fact).await.unwrap(), "signature verifies");

    // two-layer tamper detection: the chain guards the content -> hash link,
    // the signature guards the hash -> author link.
    assert!(nucleus::fact::verify_chain_step(fact));
    let mut delta_tampered = fact.clone();
    delta_tampered.delta = -999.0;
    assert!(!nucleus::fact::verify_chain_step(&delta_tampered), "chain catches delta tampering");

    let mut hash_tampered = fact.clone();
    hash_tampered.hash = "deadbeef".into();
    assert!(!trust::verify_fact(&e.store, &hash_tampered).await.unwrap(), "signature catches hash tampering");
}

#[tokio::test]
async fn imagination_projects_the_scrubbable_future() {
    let e = engine().await;
    let apples = plain(&e, "apples.stock", 8.0).await;

    // a daily rule eats one apple
    store::freqs::create(
        &e.store.pool,
        store::freqs::NewFrequency {
            slug: "freq.daily",
            head: "Daily",
            seconds: 0,
            days: 1,
            months: 0,
            day_of_week: None,
            next_at: at("2026-07-05T06:00:00Z"),
            catch_up: false,
        },
    )
    .await
    .unwrap();
    store::rules::create(
        &e.store.pool,
        store::rules::NewRule {
            slug: "rules.eat",
            head: "Eat",
            condition: "-1 * freq(@freq.daily)",
            gate: "!=0",
            carry: "value",
            consequences: vec![(nucleus::ConsequenceKind::AddQuantity, Some("@apples.stock".into()), None)],
        },
    )
    .await
    .unwrap();
    e.reload_rules().await.unwrap();

    // project 5 days out: 8 - 5 = 3, and the run-out is foreseeable
    let now = at("2026-07-05T00:00:00Z");
    let timeline = e.project(now, now + TimeDelta::days(5)).await.unwrap();
    assert_eq!(timeline.projected(&apples), Some(3.0));
    assert!(timeline.crossing_below(&apples, 4.0).is_some());
}
