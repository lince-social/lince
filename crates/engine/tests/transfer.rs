//! Transfer (blueprint VIII) + Trust (XI) + Imagination (XII) acceptance.

pub mod support;

use chrono::{DateTime, TimeDelta, Utc};
use engine::actions::{
    Action, TransferDraftRevisionInput, TransferOccurrenceClaimRole, TransferReservePoint,
    TransferSatiation, TransferVisibility,
};
use engine::trust::{self, Signer};
use nucleus::PromiseState;
use nucleus::transfer::AgreementType;
use support::{DraftOptions, activate, agree, claim, create_transfer, person, plain, promise};

fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

#[test]
fn a_sale_applies_only_reviewed_occurrences_owned_by_the_signer() {
    support::run_async_test("transfer-sale", || async {
        let engine = support::engine().await;
        let ana = person(&engine, "sale.ana").await;
        let carlos = person(&engine, "sale.carlos").await;
        let bike = plain(&engine, "sale.ana.bike", 1.0).await;
        let ana_balance = plain(&engine, "sale.ana.balance", 0.0).await;
        let fixture = create_transfer(
            &engine,
            &ana,
            std::slice::from_ref(&carlos),
            vec![
                promise("sale-bike-give", &bike, &ana, -1.0),
                promise("sale-bike-receive", &bike, &carlos, 1.0),
                promise("sale-balance-give", &ana_balance, &carlos, -300.0),
                promise("sale-balance-receive", &ana_balance, &ana, 300.0),
            ],
            DraftOptions {
                slug: "sale.bike",
                agreement: AgreementType::Full,
                reserve_default: TransferReservePoint::Active,
                require_confirmation: true,
            },
        )
        .await;

        engine.set_signer(ana.signer.clone()).await.unwrap();
        assert!(
            engine
                .act(
                    Action::ActivateTransferOccurrence {
                        transfer: fixture.transfer.clone(),
                        promise: "sale-bike-give".into(),
                        expected_revision: fixture.revision,
                        request_id: "sale:activate:early".into(),
                        person: Some(ana.uid.clone()),
                    },
                    None,
                )
                .await
                .is_err(),
            "activation is refused before agreement"
        );

        agree(&engine, &fixture, &ana, "sale:ana").await;
        agree(&engine, &fixture, &carlos, "sale:carlos").await;
        let bike_occurrence = activate(
            &engine,
            &fixture,
            &ana,
            "sale-bike-give",
            "sale:activate:bike",
        )
        .await;
        let balance_occurrence = activate(
            &engine,
            &fixture,
            &ana,
            "sale-balance-receive",
            "sale:activate:balance",
        )
        .await;

        claim(
            &engine,
            &bike_occurrence,
            &ana,
            TransferOccurrenceClaimRole::Delivery,
            "sale:bike:delivery",
        )
        .await;
        claim(
            &engine,
            &bike_occurrence,
            &carlos,
            TransferOccurrenceClaimRole::Receipt,
            "sale:bike:receipt",
        )
        .await;
        claim(
            &engine,
            &balance_occurrence,
            &carlos,
            TransferOccurrenceClaimRole::Delivery,
            "sale:balance:delivery",
        )
        .await;
        claim(
            &engine,
            &balance_occurrence,
            &ana,
            TransferOccurrenceClaimRole::Receipt,
            "sale:balance:receipt",
        )
        .await;

        let bike_preview = support::settlement_preview(&engine, &bike_occurrence, &ana, 1.0).await;
        support::settle_from_preview(
            &engine,
            &bike_occurrence,
            &ana,
            "sale:settle:bike",
            &bike_preview,
        )
        .await;
        let balance_preview =
            support::settlement_preview(&engine, &balance_occurrence, &ana, 300.0).await;
        support::settle_from_preview(
            &engine,
            &balance_occurrence,
            &ana,
            "sale:settle:balance",
            &balance_preview,
        )
        .await;

        assert_eq!(
            store::records::quantity(&engine.store.pool, &bike)
                .await
                .unwrap().map(|q| q.to_f64()),
            Some(0.0)
        );
        assert_eq!(
            store::records::quantity(&engine.store.pool, &ana_balance)
                .await
                .unwrap().map(|q| q.to_f64()),
            Some(300.0)
        );
        assert_eq!(
            store::misc::promise_state(&engine.store.pool, "sale-bike-give")
                .await
                .unwrap(),
            Some(PromiseState::Kept)
        );
        assert_eq!(
            store::misc::promise_state(&engine.store.pool, "sale-balance-receive")
                .await
                .unwrap(),
            Some(PromiseState::Kept)
        );
    });
}

#[test]
fn revising_a_signed_promise_invalidates_current_agreement() {
    support::run_async_test("transfer-revision-invalidation", || async {
        let engine = support::engine().await;
        let ana = person(&engine, "revision.ana").await;
        let bia = person(&engine, "revision.bia").await;
        let apples = plain(&engine, "revision.apples", 10.0).await;
        let fixture = create_transfer(
            &engine,
            &ana,
            std::slice::from_ref(&bia),
            vec![
                promise("revision-give", &apples, &ana, -5.0),
                promise("revision-receive", &apples, &bia, 5.0),
            ],
            DraftOptions {
                slug: "revision.transfer",
                agreement: AgreementType::Full,
                reserve_default: TransferReservePoint::Active,
                require_confirmation: true,
            },
        )
        .await;
        agree(&engine, &fixture, &ana, "revision:ana").await;
        agree(&engine, &fixture, &bia, "revision:bia").await;
        assert!(
            protein::transfer_agreement_ready(&engine.store, &fixture.transfer)
                .await
                .unwrap()
        );

        engine.set_signer(ana.signer.clone()).await.unwrap();
        engine
            .act(
                Action::ReviseTransferDraft {
                    transfer: fixture.transfer.clone(),
                    expected_revision: fixture.revision,
                    request_id: "revision:change-quantity".into(),
                    draft: TransferDraftRevisionInput {
                        creator: ana.uid.clone(),
                        slug: Some("revision.transfer".into()),
                        head: "revision.transfer".into(),
                        agreement: AgreementType::Full,
                        agreement_pct: None,
                        satiation: TransferSatiation::None,
                        parent: None,
                        source: None,
                        visibility: TransferVisibility::Hidden,
                        max_proximity: None,
                        reserve_default: TransferReservePoint::Active,
                        require_confirmation: true,
                        default_place: None,
                        invitees: Vec::new(),
                        promises: vec![
                            promise("revision-give", &apples, &ana, -3.0),
                            promise("revision-receive", &apples, &bia, 3.0),
                        ],
                        dependencies: Vec::new(),
                    },
                },
                None,
            )
            .await
            .expect("signed promise revision");
        assert_eq!(
            support::current_revision(&engine, &fixture.transfer).await,
            fixture.revision + 1
        );
        assert!(
            store::transfers::party_levels(&engine.store.pool, &fixture.transfer)
                .await
                .unwrap()
                .iter()
                .all(|(_, _, level)| *level == 0),
            "a public quantity revision resets every agreement level"
        );
    });
}

#[tokio::test]
async fn every_fact_is_signed_and_verifiable() {
    let e = support::engine().await;
    e.set_signer(Signer::generate("ana", "ed25519:ana:2026-07"))
        .await
        .unwrap();
    let apples = plain(&e, "apples", 8.0).await;

    let facts = e.append_user(&apples, -1.0).await.unwrap();
    let fact = &facts[0];
    assert!(fact.signature.is_some(), "facts are signed");
    assert_eq!(fact.actor_uid.as_deref(), Some("ana"));
    assert!(
        trust::verify_fact(&e.store, fact).await.unwrap(),
        "signature verifies"
    );

    // two-layer tamper detection: the chain guards the content -> hash link,
    // the signature guards the hash -> author link.
    assert!(nucleus::fact::verify_chain_step(fact));
    let mut delta_tampered = fact.clone();
    delta_tampered.delta = store::exact::from_f64(-999.0);
    assert!(
        !nucleus::fact::verify_chain_step(&delta_tampered),
        "chain catches delta tampering"
    );

    let mut hash_tampered = fact.clone();
    hash_tampered.hash = "deadbeef".into();
    assert!(
        !trust::verify_fact(&e.store, &hash_tampered).await.unwrap(),
        "signature catches hash tampering"
    );
}

#[tokio::test]
async fn imagination_projects_the_scrubbable_future() {
    let e = support::engine().await;
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
            consequences: vec![(
                nucleus::ConsequenceKind::AddQuantity,
                Some("@apples.stock".into()),
                None,
            )],
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
