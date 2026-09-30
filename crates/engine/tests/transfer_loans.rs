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
    loans::{Interval, Reference},
};
use support::{DraftOptions, Person, TransferFixture};

async fn draft(
    engine: &Engine,
    owner: &Person,
    other: &Person,
    record: &str,
    id: &str,
    delta: f64,
    item: TransferItem,
) -> TransferFixture {
    let mut promise = support::promise(id, record, owner, delta);
    promise.item = Some(item);
    support::create_transfer(
        engine,
        owner,
        std::slice::from_ref(other),
        vec![promise],
        DraftOptions {
            slug: id,
            agreement: AgreementType::Full,
            reserve_default: TransferReservePoint::None,
            require_confirmation: true,
        },
    )
    .await
}

async fn receive(
    engine: &Engine,
    fixture: &TransferFixture,
    giver: &Person,
    receiver: &Person,
    owner: &Person,
    id: &str,
) {
    for person in [giver, receiver] {
        support::agree(engine, fixture, person, &format!("{id}:{}", person.uid)).await;
    }
    let occurrence = support::activate(engine, fixture, owner, id, &format!("{id}:activate")).await;
    support::claim(
        engine,
        &occurrence,
        giver,
        TransferOccurrenceClaimRole::Delivery,
        &format!("{id}:delivery"),
    )
    .await;
    support::claim(
        engine,
        &occurrence,
        receiver,
        TransferOccurrenceClaimRole::Receipt,
        &format!("{id}:receipt"),
    )
    .await;
    let preview = support::settlement_preview(engine, &occurrence, owner, 1.0).await;
    support::settle_from_preview(
        engine,
        &occurrence,
        owner,
        &format!("{id}:settle"),
        &preview,
    )
    .await;
}

#[test]
fn accepted_deadline_survives_extension_proposal_and_returns_do_not_debit_twice() {
    support::run_async_test("loan", || async {
        let directory = tempfile::tempdir().unwrap();
        let database = store::Store::open(&format!(
            "sqlite://{}",
            directory.path().join("loan.sqlite").display()
        ))
        .await
        .unwrap();
        let engine = Engine::new(database).await.unwrap();
        store::organs::ensure_local(&engine.store.pool, "http://transfer.test")
            .await
            .unwrap();
        let ana = support::person(&engine, "ana").await;
        let beto = support::person(&engine, "beto").await;
        let bike = support::plain(&engine, "bike", -1.0).await;
        let transport = support::plain(&engine, "transport", -1.0).await;
        let from = nucleus::execution::now().timestamp_millis() + 100_000;
        let end = from + 3 * 86_400_000;
        let date = |ms| {
            chrono::DateTime::from_timestamp_millis(ms)
                .unwrap()
                .to_rfc3339()
        };
        let fixture = draft(
            &engine,
            &beto,
            &ana,
            &bike,
            "loan",
            1.0,
            TransferItem {
                title: "Bike".into(),
                exchange: Some(ExchangeRoute {
                    uid: "bike-loan".into(),
                    giver: ana.uid.clone(),
                    receiver: beto.uid.clone(),
                }),
                loan: Some(Interval {
                    from: date(from),
                    until: date(end),
                }),
                ..Default::default()
            },
        )
        .await;
        engine.set_signer(beto.signer.clone()).await.unwrap();
        engine
            .act(
                Action::SetTransferPrivateApplicationPolicy {
                    transfer: fixture.transfer.clone(),
                    exchange: "bike-loan".into(),
                    person: beto.uid.clone(),
                    expected_version: 0,
                    request_id: "loan-effects".into(),
                    effects: vec![
                        PrivateEffect {
                            record: bike.clone(),
                            formula: "incoming()".into(),
                            mode: EffectMode::Quantity,
                        },
                        PrivateEffect {
                            record: transport.clone(),
                            formula: "incoming()".into(),
                            mode: EffectMode::Fulfilment,
                        },
                    ],
                },
                None,
            )
            .await
            .unwrap();
        receive(&engine, &fixture, &ana, &beto, &beto, "loan").await;
        let pool = &engine.store.pool;
        for (at, expected) in [(from - 1, -1), (from, 0), (end - 1, 0), (end, -1)] {
            let adjustments = store::transfer_loans::adjustments(pool, at).await.unwrap();
            assert_eq!(adjustments.len(), 2);
            assert!(adjustments.iter().all(|effect| {
                effect
                    .delta
                    .exact_numeric_cmp(store::exact::integer(expected))
                    .is_eq()
            }));
        }
        let changed_unit = store::concepts::create(pool, "changed-unit", &[])
            .await
            .unwrap();
        engine
            .act(
                Action::SetUnit {
                    target: bike.clone(),
                    unit: Some(changed_unit),
                },
                None,
            )
            .await
            .unwrap();
        let adjustments = store::transfer_loans::adjustments(pool, end).await.unwrap();
        assert!(
            adjustments.iter().any(|effect| effect.record == bike
                && effect.unit_changed
                && effect.delta.is_zero())
        );
        assert!(adjustments.iter().any(|effect| effect.record == transport
            && !effect.unit_changed
            && effect.delta.is_negative()));
        let balance = store::transfer_balances::read(pool, &bike).await.unwrap();
        assert!(balance.can_offer.is_zero());
        assert!(
            balance
                .incomplete
                .iter()
                .any(|reason| reason.contains("original Record unit"))
        );
        let unaffected = store::transfer_balances::read(pool, &transport)
            .await
            .unwrap();
        assert!(
            !unaffected
                .incomplete
                .iter()
                .any(|reason| reason.contains("original Record unit"))
        );
        engine
            .act(
                Action::SetUnit {
                    target: bike.clone(),
                    unit: None,
                },
                None,
            )
            .await
            .unwrap();
        let metrics = engine::projection::Metrics::default();
        let projection = engine::projection::calculate(
            &engine.store,
            &nucleus::projection::Context {
                actor: None,
                window: nucleus::projection::Window {
                    from_ms: from,
                    until_ms: end + 1,
                    timezone: "America/Sao_Paulo".into(),
                },
            },
            from,
            None,
            &metrics,
        )
        .await
        .unwrap();
        assert_eq!(projection.incomplete, None);
        for record in [&bike, &transport] {
            assert!(
                projection
                    .spans
                    .iter()
                    .any(|span| span.record.as_str() == record
                        && span.from_ms == end
                        && span.quantity.value.is_negative())
            );
            assert!(
                store::records::quantity(pool, record)
                    .await
                    .unwrap()
                    .unwrap()
                    .is_zero()
            );
        }
        engine.set_signer(beto.signer.clone()).await.unwrap();
        let extension = Action::ProposeTransferLoanExtension {
            transfer: fixture.transfer.clone(),
            exchange: "bike-loan".into(),
            until: date(end + 86_400_000),
            expected_revision: fixture.revision,
            person: beto.uid.clone(),
            request_id: "extend".into(),
        };
        engine.act(extension.clone(), None).await.unwrap();
        engine.act(extension, None).await.unwrap();
        assert_eq!(
            store::transfer_loans::accepted(pool, &fixture.transfer)
                .await
                .unwrap()[0]
                .until_ms,
            end
        );
        let revised = TransferFixture {
            transfer: fixture.transfer.clone(),
            revision: fixture.revision + 1,
            ..fixture
        };
        support::agree(&engine, &revised, &beto, "extend:beto").await;
        assert_eq!(
            store::transfer_loans::accepted(pool, &revised.transfer)
                .await
                .unwrap()[0]
                .until_ms,
            end
        );
        support::agree(&engine, &revised, &ana, "extend:ana").await;
        assert_eq!(
            store::transfer_loans::accepted(pool, &revised.transfer)
                .await
                .unwrap()[0]
                .until_ms,
            end + 86_400_000
        );
        let origin = store::organs::local(pool).await.unwrap().unwrap().uid;
        let item = TransferItem {
            title: "Return bike".into(),
            exchange: Some(ExchangeRoute {
                uid: "bike-return".into(),
                giver: beto.uid.clone(),
                receiver: ana.uid.clone(),
            }),
            return_of: Some(Reference {
                origin,
                transfer: revised.transfer.clone(),
                exchange: "bike-loan".into(),
            }),
            ..Default::default()
        };
        let mut forged = item.clone();
        forged.exchange.as_mut().unwrap().receiver = beto.uid.clone();
        assert!(
            store::transfer_loans::validate_link(pool, &forged, &beto.uid)
                .await
                .is_err()
        );
        let returned = draft(&engine, &beto, &ana, &bike, "return", -1.0, item).await;
        receive(&engine, &returned, &beto, &ana, &beto, "return").await;
        let adjustments = store::transfer_loans::adjustments(pool, from + 1)
            .await
            .unwrap();
        assert!(
            adjustments
                .iter()
                .find(|a| a.record == bike)
                .unwrap()
                .delta
                .is_zero()
        );
        assert_eq!(
            adjustments
                .iter()
                .find(|a| a.record == transport)
                .unwrap()
                .delta
                .exact_numeric_cmp(store::exact::integer(-1)),
            std::cmp::Ordering::Equal
        );
        assert_eq!(
            store::records::quantity(pool, &bike)
                .await
                .unwrap()
                .unwrap()
                .exact_numeric_cmp(store::exact::integer(-1)),
            std::cmp::Ordering::Equal
        );
        assert!(
            store::records::quantity(pool, &transport)
                .await
                .unwrap()
                .unwrap()
                .is_zero()
        );
        let changed_unit = store::concepts::create(pool, "returned-bike-unit", &[])
            .await
            .unwrap();
        engine
            .act(
                Action::SetUnit {
                    target: bike.clone(),
                    unit: Some(changed_unit),
                },
                None,
            )
            .await
            .unwrap();
        let returned = store::transfer_loans::adjustments(pool, from + 1)
            .await
            .unwrap();
        assert!(
            returned.iter().any(|effect| effect.record == bike
                && !effect.unit_changed
                && effect.delta.is_zero())
        );
        let ride = draft(
            &engine,
            &beto,
            &ana,
            &transport,
            "ride",
            1.0,
            TransferItem {
                title: "Ride".into(),
                exchange: Some(ExchangeRoute {
                    uid: "ride".into(),
                    giver: ana.uid.clone(),
                    receiver: beto.uid.clone(),
                }),
                ..Default::default()
            },
        )
        .await;
        engine.set_signer(beto.signer.clone()).await.unwrap();
        engine
            .act(
                Action::SetTransferPrivateApplicationPolicy {
                    transfer: ride.transfer.clone(),
                    exchange: "ride".into(),
                    person: beto.uid.clone(),
                    expected_version: 0,
                    request_id: "ride-policy".into(),
                    effects: vec![PrivateEffect {
                        record: transport.clone(),
                        formula: "incoming()".into(),
                        mode: EffectMode::Fulfilment,
                    }],
                },
                None,
            )
            .await
            .unwrap();
        receive(&engine, &ride, &ana, &beto, &beto, "ride").await;
        let physical = store::records::quantity(pool, &transport)
            .await
            .unwrap()
            .unwrap();
        let adjustment = store::transfer_loans::adjustments(pool, from + 1)
            .await
            .unwrap()
            .into_iter()
            .find(|effect| effect.record == transport)
            .unwrap();
        assert!(
            store::exact::sum_exact([physical, adjustment.delta])
                .unwrap()
                .is_zero()
        );
        assert_eq!(
            physical.exact_numeric_cmp(store::exact::integer(1)),
            std::cmp::Ordering::Equal
        );
    });
}
