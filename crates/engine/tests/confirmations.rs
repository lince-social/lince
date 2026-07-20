//! Phase 4 occurrence claims and the Transfer's generic conversation surface.

pub mod support;

use engine::actions::{Action, TransferOccurrenceClaimRole, TransferReservePoint};
use nucleus::PromiseState;
use nucleus::transfer::{AgreementType, TransferRemainderPolicy};
use protein::{Include, Predicate, Protein, Source};
use support::{DraftOptions, activate, agree, claim, create_transfer, person, plain, promise};

#[test]
fn role_specific_claims_gate_an_idempotent_settlement() {
    support::run_async_test("transfer-confirmation-gate", || async {
        let engine = support::engine().await;
        let ana = person(&engine, "claims.ana").await;
        let bia = person(&engine, "claims.bia").await;
        let apples = plain(&engine, "claims.apples", 10.0).await;
        let fixture = create_transfer(
            &engine,
            &ana,
            std::slice::from_ref(&bia),
            vec![
                promise("claims-give", &apples, &ana, -5.0),
                promise("claims-receive", &apples, &bia, 5.0),
            ],
            DraftOptions {
                slug: "claims.transfer",
                agreement: AgreementType::Full,
                reserve_default: TransferReservePoint::Active,
                require_confirmation: true,
            },
        )
        .await;
        agree(&engine, &fixture, &ana, "claims:ana").await;
        agree(&engine, &fixture, &bia, "claims:bia").await;
        let occurrence = activate(&engine, &fixture, &ana, "claims-give", "claims:activate").await;

        engine.set_signer(ana.signer.clone()).await.unwrap();
        let settle_without_claims = engine
            .act(
                Action::SettleTransferOccurrence {
                    occurrence: occurrence.clone(),
                    request_id: "claims:settle:early".into(),
                    person: Some(ana.uid.clone()),
                    canonical_quantity: 5.0,
                    expected_remaining_quantity: 5.0,
                    expected_local_delta: -5.0,
                    expected_application_formula_hash:
                        nucleus::transfer::occurrence_application_formula_hash("-incoming()"),
                    expected_application_formula_version: 0,
                    expected_remainder_policy: TransferRemainderPolicy::Visible,
                },
                None,
            )
            .await;
        assert!(
            settle_without_claims.is_err(),
            "settlement requires both real-world claims"
        );

        engine.set_signer(bia.signer.clone()).await.unwrap();
        assert!(
            engine
                .act(
                    Action::SetTransferOccurrenceClaim {
                        occurrence: occurrence.clone(),
                        request_id: "claims:wrong-delivery-author".into(),
                        person: Some(bia.uid.clone()),
                        role: TransferOccurrenceClaimRole::Delivery,
                        claimed: true,
                    },
                    None,
                )
                .await
                .is_err(),
            "the receiver cannot claim the giver's delivery"
        );

        claim(
            &engine,
            &occurrence,
            &ana,
            TransferOccurrenceClaimRole::Delivery,
            "claims:delivery",
        )
        .await;
        assert!(
            protein::execute_for_with_signer(
                &engine.store,
                &Protein {
                    source: Source::TransferSettlementPreview,
                    filter: vec![
                        Predicate::UidEq(occurrence.clone()),
                        Predicate::QuantityEq(5.0),
                    ],
                    include: Include::default(),
                    aggregate: None,
                    order: Vec::new(),
                    limit: None,
                },
                None,
                Some(&ana.uid),
            )
            .await
            .unwrap()
            .is_empty(),
            "one claim is not enough to produce a settlement preview"
        );
        claim(
            &engine,
            &occurrence,
            &bia,
            TransferOccurrenceClaimRole::Receipt,
            "claims:receipt",
        )
        .await;

        let preview = support::settlement_preview(&engine, &occurrence, &ana, 5.0).await;
        let first =
            support::settle_from_preview(&engine, &occurrence, &ana, "claims:settle", &preview)
                .await;
        let replay =
            support::settle_from_preview(&engine, &occurrence, &ana, "claims:settle", &preview)
                .await;
        assert_eq!(first, replay, "settlement retry returns the original slice");
        assert_eq!(
            store::records::quantity(&engine.store.pool, &apples)
                .await
                .unwrap(),
            Some(5.0)
        );
    });
}

#[test]
fn legacy_confirmation_flag_cannot_bypass_occurrence_evidence() {
    support::run_async_test("transfer-confirmation-policy", || async {
        let engine = support::engine().await;
        let ana = person(&engine, "policy-claims.ana").await;
        let bia = person(&engine, "policy-claims.bia").await;
        let apples = plain(&engine, "policy-claims.apples", 10.0).await;
        let fixture = create_transfer(
            &engine,
            &ana,
            std::slice::from_ref(&bia),
            vec![
                promise("policy-claims-give", &apples, &ana, -2.0),
                promise("policy-claims-receive", &apples, &bia, 2.0),
            ],
            DraftOptions {
                slug: "policy-claims.transfer",
                agreement: AgreementType::Full,
                reserve_default: TransferReservePoint::Active,
                require_confirmation: false,
            },
        )
        .await;
        agree(&engine, &fixture, &ana, "policy-claims:ana").await;
        agree(&engine, &fixture, &bia, "policy-claims:bia").await;
        let occurrence = activate(
            &engine,
            &fixture,
            &ana,
            "policy-claims-give",
            "policy-claims:activate",
        )
        .await;
        engine.set_signer(ana.signer.clone()).await.unwrap();
        let result = engine
            .act(
                Action::SettleTransferOccurrence {
                    occurrence,
                    request_id: "policy-claims:settle".into(),
                    person: Some(ana.uid),
                    canonical_quantity: 2.0,
                    expected_remaining_quantity: 2.0,
                    expected_local_delta: -2.0,
                    expected_application_formula_hash:
                        nucleus::transfer::occurrence_application_formula_hash("-incoming()"),
                    expected_application_formula_version: 0,
                    expected_remainder_policy: TransferRemainderPolicy::Visible,
                },
                None,
            )
            .await;
        assert!(
            result.is_err(),
            "current settlement always requires delivery and receipt evidence"
        );
    });
}

#[test]
fn partial_settlement_rejects_stale_review_and_compensates_only_private_application() {
    support::run_async_test("transfer-partial-compensation", || async {
        let engine = support::engine().await;
        let ana = person(&engine, "partial.ana").await;
        let bia = person(&engine, "partial.bia").await;
        let apples = plain(&engine, "partial.apples", 10.0).await;
        let fixture = create_transfer(
            &engine,
            &ana,
            std::slice::from_ref(&bia),
            vec![
                promise("partial-give", &apples, &ana, -5.0),
                promise("partial-receive", &apples, &bia, 5.0),
            ],
            DraftOptions {
                slug: "partial.transfer",
                agreement: AgreementType::Full,
                reserve_default: TransferReservePoint::Active,
                require_confirmation: true,
            },
        )
        .await;
        agree(&engine, &fixture, &ana, "partial:ana").await;
        agree(&engine, &fixture, &bia, "partial:bia").await;
        let occurrence =
            activate(&engine, &fixture, &ana, "partial-give", "partial:activate").await;
        claim(
            &engine,
            &occurrence,
            &ana,
            TransferOccurrenceClaimRole::Delivery,
            "partial:delivery",
        )
        .await;
        claim(
            &engine,
            &occurrence,
            &bia,
            TransferOccurrenceClaimRole::Receipt,
            "partial:receipt",
        )
        .await;

        let first_preview = support::settlement_preview(&engine, &occurrence, &ana, 2.0).await;
        let first_slice = support::settle_from_preview(
            &engine,
            &occurrence,
            &ana,
            "partial:settle:first",
            &first_preview,
        )
        .await;
        assert_eq!(
            store::records::quantity(&engine.store.pool, &apples)
                .await
                .unwrap(),
            Some(8.0)
        );
        let progress =
            store::transfers::occurrence_settlement_progress(&engine.store.pool, &occurrence)
                .await
                .unwrap()
                .unwrap();
        assert_eq!(progress.settled_quantity, 2.0);
        assert_eq!(progress.remaining_quantity, 3.0);
        assert_eq!(
            store::misc::promise_state(&engine.store.pool, "partial-give")
                .await
                .unwrap(),
            Some(PromiseState::Active),
            "a partial slice leaves the promise active"
        );

        engine.set_signer(ana.signer.clone()).await.unwrap();
        let stale = engine
            .act(
                Action::SettleTransferOccurrence {
                    occurrence: occurrence.clone(),
                    request_id: "partial:settle:stale-preview".into(),
                    person: Some(ana.uid.clone()),
                    canonical_quantity: first_preview["canonical_quantity"].as_f64().unwrap(),
                    expected_remaining_quantity: first_preview["expected_remaining_quantity"]
                        .as_f64()
                        .unwrap(),
                    expected_local_delta: first_preview["expected_local_delta"].as_f64().unwrap(),
                    expected_application_formula_hash:
                        first_preview["expected_application_formula_hash"]
                            .as_str()
                            .unwrap()
                            .into(),
                    expected_application_formula_version:
                        first_preview["expected_application_formula_version"]
                            .as_u64()
                            .unwrap(),
                    expected_remainder_policy: TransferRemainderPolicy::Visible,
                },
                None,
            )
            .await;
        assert!(
            stale.is_err(),
            "a consumed preview cannot authorize another slice"
        );

        let compensation = engine
            .act(
                Action::CompensateTransferOccurrenceSettlement {
                    settlement: first_slice.clone(),
                    request_id: "partial:compensate".into(),
                    person: Some(ana.uid.clone()),
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let replay = engine
            .act(
                Action::CompensateTransferOccurrenceSettlement {
                    settlement: first_slice.clone(),
                    request_id: "partial:compensate".into(),
                    person: Some(ana.uid.clone()),
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        assert_eq!(compensation, replay);
        assert_eq!(
            store::records::quantity(&engine.store.pool, &apples)
                .await
                .unwrap(),
            Some(10.0),
            "compensation reverses only the private Record application"
        );
        let progress =
            store::transfers::occurrence_settlement_progress(&engine.store.pool, &occurrence)
                .await
                .unwrap()
                .unwrap();
        assert_eq!(progress.settled_quantity, 2.0);
        assert_eq!(progress.remaining_quantity, 3.0);
        assert!(
            store::transfers::occurrence_settlement_compensation_for_settlement(
                &engine.store.pool,
                &first_slice,
            )
            .await
            .unwrap()
            .is_some(),
            "public fulfillment remains and links to its private correction"
        );

        let remainder_preview = support::settlement_preview(&engine, &occurrence, &ana, 3.0).await;
        support::settle_from_preview(
            &engine,
            &occurrence,
            &ana,
            "partial:settle:remainder",
            &remainder_preview,
        )
        .await;
        let progress =
            store::transfers::occurrence_settlement_progress(&engine.store.pool, &occurrence)
                .await
                .unwrap()
                .unwrap();
        assert_eq!(progress.settled_quantity, 5.0);
        assert_eq!(progress.remaining_quantity, 0.0);
        assert_eq!(
            store::misc::promise_state(&engine.store.pool, "partial-give")
                .await
                .unwrap(),
            Some(PromiseState::Kept)
        );
        assert_eq!(
            store::records::quantity(&engine.store.pool, &apples)
                .await
                .unwrap(),
            Some(7.0),
            "the later slice applies only its own reviewed local delta"
        );
    });
}

#[test]
fn balance_is_advisory_and_conversation_uses_generic_threads() {
    support::run_async_test("transfer-balance-chat", || async {
        let engine = support::engine().await;
        let ana = person(&engine, "chat.ana").await;
        let carlos = person(&engine, "chat.carlos").await;
        let money = store::concepts::create(&engine.store.pool, "chat.money", &[])
            .await
            .unwrap();
        let ana_money = plain(&engine, "chat.ana.money", 0.0).await;
        let carlos_money = plain(&engine, "chat.carlos.money", 300.0).await;
        store::records::set_concept(&engine.store.pool, &ana_money, Some(&money))
            .await
            .unwrap();
        store::records::set_concept(&engine.store.pool, &carlos_money, Some(&money))
            .await
            .unwrap();
        let fixture = create_transfer(
            &engine,
            &ana,
            std::slice::from_ref(&carlos),
            vec![
                promise("chat-carlos-money", &carlos_money, &carlos, -300.0),
                promise("chat-ana-money", &ana_money, &ana, 300.0),
            ],
            DraftOptions {
                slug: "chat.sale",
                agreement: AgreementType::Full,
                reserve_default: TransferReservePoint::Active,
                require_confirmation: true,
            },
        )
        .await;

        let rows = protein::execute_for_with_signer(
            &engine.store,
            &Protein {
                source: Source::Transfer,
                filter: vec![Predicate::UidEq(fixture.transfer.clone())],
                include: Include::default(),
                aggregate: None,
                order: Vec::new(),
                limit: None,
            },
            None,
            Some(&ana.uid),
        )
        .await
        .unwrap();
        let transfer = rows
            .iter()
            .find(|row| row["uid"] == fixture.transfer)
            .expect("Transfer projection");
        assert_eq!(transfer["balanced"], true);
        assert_eq!(transfer["balance"][&money], 0.0);

        let thread = engine
            .act(
                Action::CreateThread {
                    target: fixture.transfer.clone(),
                    head: "Negotiation".into(),
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        engine
            .act(
                Action::CreateMessage {
                    thread,
                    body: "can we do saturday?".into(),
                    parent: None,
                    references: Vec::new(),
                },
                None,
            )
            .await
            .unwrap();
        let rows = protein::execute(
            &engine.store,
            &Protein {
                source: Source::Record,
                filter: vec![Predicate::UidEq(fixture.transfer)],
                include: Include {
                    threads: Some(protein::ThreadsInclude { messages_limit: 10 }),
                    ..Default::default()
                },
                aggregate: None,
                order: Vec::new(),
                limit: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            rows[0]["threads"][0]["messages"][0]["body"],
            "can we do saturday?"
        );
    });
}
