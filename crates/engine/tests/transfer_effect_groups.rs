mod support;

use engine::{
    Engine,
    actions::{Action, TransferOccurrenceClaimRole, TransferReservePoint},
};
use nucleus::transfer::{
    AgreementType,
    application::{EffectMode, PrivateEffect},
    disclosure::TransferItem,
    exchange::ExchangeRoute,
};
use serde_json::json;
use support::{DraftOptions, Person, TransferFixture};

async fn ready(
    engine: &Engine,
    giver: &Person,
    receiver: &Person,
    source: &str,
    name: &str,
) -> (TransferFixture, String) {
    let mut promise = support::promise(name, source, receiver, 1.0);
    promise.item = Some(TransferItem {
        title: name.into(),
        exchange: Some(ExchangeRoute {
            uid: name.into(),
            giver: giver.uid.clone(),
            receiver: receiver.uid.clone(),
        }),
        ..Default::default()
    });
    let fixture = support::create_transfer(
        engine,
        receiver,
        std::slice::from_ref(giver),
        vec![promise],
        DraftOptions {
            slug: name,
            agreement: AgreementType::Full,
            reserve_default: TransferReservePoint::None,
            require_confirmation: true,
        },
    )
    .await;
    for person in [giver, receiver] {
        support::agree(
            engine,
            &fixture,
            person,
            &format!("{name}:agree:{}", person.uid),
        )
        .await;
    }
    let occurrence = support::activate(
        engine,
        &fixture,
        receiver,
        name,
        &format!("{name}:activate"),
    )
    .await;
    support::claim(
        engine,
        &occurrence,
        giver,
        TransferOccurrenceClaimRole::Delivery,
        &format!("{name}:delivery"),
    )
    .await;
    support::claim(
        engine,
        &occurrence,
        receiver,
        TransferOccurrenceClaimRole::Receipt,
        &format!("{name}:receipt"),
    )
    .await;
    (fixture, occurrence)
}

async fn policy(
    engine: &Engine,
    owner: &Person,
    fixture: &TransferFixture,
    exchange: &str,
    effects: Vec<PrivateEffect>,
) {
    engine.set_signer(owner.signer.clone()).await.unwrap();
    engine
        .act(
            Action::SetTransferPrivateApplicationPolicy {
                transfer: fixture.transfer.clone(),
                exchange: exchange.into(),
                person: owner.uid.clone(),
                effects,
                expected_version: 0,
                request_id: format!("{exchange}:policy"),
            },
            None,
        )
        .await
        .unwrap();
}

fn effect(record: &str, mode: EffectMode) -> PrivateEffect {
    PrivateEffect {
        record: record.into(),
        formula: "incoming()".into(),
        mode,
    }
}

async fn quantities(engine: &Engine, bike: &str, transport: &str, expected: [&str; 2]) {
    for (record, expected) in [(bike, expected[0]), (transport, expected[1])] {
        assert_eq!(
            store::records::quantity(&engine.store.pool, record)
                .await
                .unwrap()
                .unwrap()
                .exact_numeric_cmp(nucleus::DecimalValue::parse_inferred(expected).unwrap()),
            std::cmp::Ordering::Equal
        );
    }
}

fn settle(preview: &serde_json::Value, request: &str) -> Action {
    serde_json::from_value(json!({"action":"settle-transfer-occurrence","occurrence":preview["occurrence"],"person":preview["person"],"request_id":request,
        "canonical_quantity":preview["canonical_quantity"],"expected_remaining_quantity":preview["expected_remaining_quantity"],
        "expected_local_delta":preview["expected_local_delta"],"expected_application_formula_hash":preview["expected_application_formula_hash"],
        "expected_application_formula_version":preview["expected_application_formula_version"],"expected_remainder_policy":preview["expected_remainder_policy"],"expected_effects_hash":preview["expected_effects_hash"]})).unwrap()
}

#[test]
fn bike_and_ride_meet_each_need_once_and_corrections_reverse_only_recorded_effects() {
    support::run_async_test("grouped-needs", || async {
        for ride_first in [false, true] {
            let engine = support::engine().await;
            let ana = support::person(&engine, "ana").await;
            let beto = support::person(&engine, "beto").await;
            let bike = support::plain(&engine, "bike", -1.0).await;
            let transport = support::plain(&engine, "transport", -1.0).await;
            let source = support::plain(&engine, "public-source", 0.0).await;
            let journey = store::concepts::create(&engine.store.pool, "journey", &[])
                .await
                .unwrap();
            engine
                .act(
                    Action::SetUnit {
                        target: transport.clone(),
                        unit: Some(journey),
                    },
                    None,
                )
                .await
                .unwrap();
            let (bike_transfer, bike_occurrence) =
                ready(&engine, &ana, &beto, &source, "bike-outcome").await;
            policy(
                &engine,
                &beto,
                &bike_transfer,
                "bike-outcome",
                vec![
                    effect(&bike, EffectMode::Quantity),
                    effect(&transport, EffectMode::Fulfilment),
                ],
            )
            .await;
            if ride_first {
                let (ride_transfer, ride_occurrence) =
                    ready(&engine, &ana, &beto, &source, "ride-outcome").await;
                policy(
                    &engine,
                    &beto,
                    &ride_transfer,
                    "ride-outcome",
                    vec![effect(&transport, EffectMode::Fulfilment)],
                )
                .await;
                let review =
                    support::settlement_preview(&engine, &ride_occurrence, &beto, 1.0).await;
                support::settle_from_preview(
                    &engine,
                    &ride_occurrence,
                    &beto,
                    "ride:apply",
                    &review,
                )
                .await;
                quantities(&engine, &bike, &transport, ["-1", "0"]).await;
            }
            let review = support::settlement_preview(&engine, &bike_occurrence, &beto, 0.4).await;
            assert_eq!(review["effects"].as_array().unwrap().len(), 2);
            assert_eq!(
                review["effects"][1]["delta"]["value"],
                if ride_first { "0" } else { "0.4" }
            );
            let first = support::settle_from_preview(
                &engine,
                &bike_occurrence,
                &beto,
                "bike:first",
                &review,
            )
            .await;
            assert_eq!(
                first,
                support::settle_from_preview(
                    &engine,
                    &bike_occurrence,
                    &beto,
                    "bike:first",
                    &review
                )
                .await
            );
            quantities(
                &engine,
                &bike,
                &transport,
                ["-0.6", if ride_first { "0" } else { "-0.6" }],
            )
            .await;
            let review = support::settlement_preview(&engine, &bike_occurrence, &beto, 0.6).await;
            support::settle_from_preview(&engine, &bike_occurrence, &beto, "bike:last", &review)
                .await;
            quantities(&engine, &bike, &transport, ["0", "0"]).await;
            let mut public = protein::execute_for_with_signer(
                &engine.store,
                &protein::Protein {
                    source: protein::Source::Transfer,
                    filter: vec![protein::Predicate::UidEq(bike_transfer.transfer.clone())],
                    fields: None,
                    include: Default::default(),
                    aggregate: None,
                    order: vec![],
                    limit: None,
                },
                None,
                Some(&beto.uid),
            )
            .await
            .unwrap();
            for row in &mut public {
                nucleus::transfer::disclosure::project_transfer(row, Some(&ana.uid), false);
            }
            let public = serde_json::to_string(&public).unwrap();
            assert!(!public.contains(&transport));
            assert!(!public.contains(&bike));
            let slice = store::transfers::occurrence_settlement_slice(&engine.store.pool, &first)
                .await
                .unwrap()
                .unwrap();
            let facts =
                store::transfer_effects::facts(&engine.store.pool, &slice.application_fact_uid)
                    .await
                    .unwrap();
            assert_eq!(facts.len(), 1);
            assert!(
                engine
                    .act(
                        Action::Compensate {
                            fact: facts[0].uid.clone()
                        },
                        None
                    )
                    .await
                    .is_err()
            );
            engine
                .act(
                    Action::AddQuantity {
                        target: transport.clone(),
                        delta: 2.0,
                    },
                    None,
                )
                .await
                .unwrap();
            let correction = Action::CompensateTransferOccurrenceSettlement {
                settlement: first,
                request_id: "correct".into(),
                person: Some(beto.uid.clone()),
            };
            engine.act(correction.clone(), None).await.unwrap();
            engine.act(correction, None).await.unwrap();
            quantities(
                &engine,
                &bike,
                &transport,
                ["-0.4", if ride_first { "2" } else { "1.6" }],
            )
            .await;
            assert_eq!(
                store::transfers::occurrence_settlement_progress(
                    &engine.store.pool,
                    &bike_occurrence
                )
                .await
                .unwrap()
                .unwrap()
                .settled_quantity,
                1.0
            );
        }
    });
}

#[test]
fn stale_secondary_record_or_failed_effect_rolls_back_the_whole_group() {
    support::run_async_test("group-review", || async {
        let engine = support::engine().await;
        let ana = support::person(&engine, "ana").await;
        let beto = support::person(&engine, "beto").await;
        let bike = support::plain(&engine, "bike", -1.0).await;
        let transport = support::plain(&engine, "transport", -1.0).await;
        let (fixture, occurrence) = ready(&engine, &ana, &beto, &bike, "group").await;
        let mut consumes = effect(&transport, EffectMode::Quantity);
        consumes.formula = "-incoming()".into();
        policy(
            &engine,
            &beto,
            &fixture,
            "group",
            vec![effect(&bike, EffectMode::Quantity), consumes],
        )
        .await;
        let review = support::settlement_preview(&engine, &occurrence, &beto, 1.0).await;
        engine
            .act(
                Action::AddQuantity {
                    target: transport.clone(),
                    delta: 3.0,
                },
                None,
            )
            .await
            .unwrap();
        assert!(
            engine
                .act(settle(&review, "stale"), None)
                .await
                .unwrap_err()
                .to_string()
                .contains("group changed")
        );
        quantities(&engine, &bike, &transport, ["-1", "2"]).await;
        engine
            .act(
                Action::SetRecordStockLimit {
                    record: transport.clone(),
                    person: beto.uid.clone(),
                    minimum: Some(store::exact::integer(2)),
                    expected_version: 0,
                    request_id: "limit".into(),
                },
                None,
            )
            .await
            .unwrap();
        let review = support::settlement_preview(&engine, &occurrence, &beto, 1.0).await;
        let before = engine.store.state_hash().await.unwrap();
        assert!(
            engine
                .act(settle(&review, "blocked"), None)
                .await
                .unwrap_err()
                .to_string()
                .contains("hard stock limit")
        );
        assert_eq!(before, engine.store.state_hash().await.unwrap());
        quantities(&engine, &bike, &transport, ["-1", "2"]).await;
        assert!(
            store::transfer_effects::quote(
                &engine.store.pool,
                &store::transfer_accounting::Binding {
                    transfer: &fixture.transfer,
                    exchange: "group",
                    occurrence: Some(&occurrence),
                    person: &beto.uid,
                    record: Some(&bike),
                    unit: Some("missing-unit"),
                    outgoing: false,
                },
                store::exact::integer(1)
            )
            .await
            .is_err()
        );
        let role = store::auth::ensure_role(&engine.store.pool, store::auth::ADMIN_ROLE)
            .await
            .unwrap();
        store::auth::create_credential(&engine.store.pool, &beto.uid, "beto-login", "hash", role)
            .await
            .unwrap();
        engine
            .set_read_filter(
                &beto.uid,
                Some(&protein::Predicate::Not(Box::new(
                    protein::Predicate::UidEq(transport.clone()),
                ))),
            )
            .await
            .unwrap();
        let denied = engine
            .act(settle(&review, "hidden"), Some(beto.uid.clone()))
            .await
            .unwrap_err()
            .to_string();
        assert!(denied.contains("outside what this login"), "{denied}");
        assert!(!denied.contains(&transport));
        let projection = protein::execute_for_with_signer(
            &engine.store,
            &protein::Protein {
                source: protein::Source::Transfer,
                filter: vec![protein::Predicate::UidEq(fixture.transfer.clone())],
                fields: None,
                include: Default::default(),
                aggregate: None,
                order: vec![],
                limit: None,
            },
            Some(&beto.uid),
            Some(&beto.uid),
        )
        .await
        .unwrap();
        assert!(
            !serde_json::to_string(&projection)
                .unwrap()
                .contains(&transport)
        );
        let preview = protein::execute_for_with_signer(
            &engine.store,
            &protein::Protein {
                source: protein::Source::TransferSettlementPreview,
                filter: vec![
                    protein::Predicate::UidEq(occurrence),
                    protein::Predicate::QuantityEq(store::exact::integer(1)),
                ],
                fields: None,
                include: Default::default(),
                aggregate: None,
                order: vec![],
                limit: None,
            },
            Some(&beto.uid),
            Some(&beto.uid),
        )
        .await
        .unwrap();
        assert!(preview.is_empty());
        quantities(&engine, &bike, &transport, ["-1", "2"]).await;
    });
}
