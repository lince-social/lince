mod support;

use engine::actions::{Action, TransferOccurrenceClaimRole, TransferReservePoint};
use nucleus::transfer::{AgreementType, disclosure::TransferItem, exchange::ExchangeRoute};
use support::{DraftOptions, TransferFixture, agree, create_transfer, person, plain, promise};

type Setup = (
    engine::Engine,
    support::Person,
    support::Person,
    TransferFixture,
    String,
    String,
);

async fn setup() -> Setup {
    setup_with(AgreementType::Full).await
}

async fn setup_with(agreement: AgreementType) -> Setup {
    let engine = support::engine().await;
    let ana = person(&engine, "cancel.ana").await;
    let beto = person(&engine, "cancel.beto").await;
    let stock = plain(&engine, "cancel.apples", 30.0).await;
    let mut terms = promise("apples", &stock, &ana, -10.0);
    terms.item = Some(TransferItem {
        title: "Apples".into(),
        exchange: Some(ExchangeRoute {
            uid: "apples".into(),
            giver: ana.uid.clone(),
            receiver: beto.uid.clone(),
        }),
        ..Default::default()
    });
    let fixture = create_transfer(
        &engine,
        &ana,
        std::slice::from_ref(&beto),
        vec![terms],
        DraftOptions {
            slug: "cancellation",
            agreement,
            reserve_default: TransferReservePoint::Agreed,
            require_confirmation: true,
        },
    )
    .await;
    for owner in [&ana, &beto] {
        agree(&engine, &fixture, owner, &format!("agree:{}", owner.uid)).await;
    }
    let occurrence = support::activate(&engine, &fixture, &ana, "apples", "activate").await;
    support::claim(
        &engine,
        &occurrence,
        &ana,
        TransferOccurrenceClaimRole::Delivery,
        "delivery",
    )
    .await;
    support::claim(
        &engine,
        &occurrence,
        &beto,
        TransferOccurrenceClaimRole::Receipt,
        "receipt",
    )
    .await;
    engine.set_signer(ana.signer.clone()).await.unwrap();
    engine
        .act(
            Action::SetRecordStockLimit {
                record: stock.clone(),
                person: ana.uid.clone(),
                minimum: Some(store::exact::zero()),
                expected_version: 0,
                request_id: "stock-limit".into(),
            },
            None,
        )
        .await
        .unwrap();
    engine
        .act(
            Action::AddQuantity {
                target: stock.clone(),
                delta: -1.0,
            },
            None,
        )
        .await
        .unwrap();
    let preview = support::settlement_preview(&engine, &occurrence, &ana, 4.0).await;
    support::settle_from_preview(&engine, &occurrence, &ana, "deliver-four", &preview).await;
    (engine, ana, beto, fixture, stock, occurrence)
}

async fn check_balance(engine: &engine::Engine, record: &str, expected: [f64; 3]) {
    let balance = store::transfer_balances::read(&engine.store.pool, record)
        .await
        .unwrap();
    assert!(balance.incomplete.is_empty(), "{:?}", balance.incomplete);
    assert_eq!(
        [
            balance.actual.to_f64(),
            balance.reserved.to_f64(),
            balance.surplus.to_f64()
        ],
        expected
    );
}

#[test]
fn agreed_cancellation_releases_only_the_remainder_and_retains_settled_evidence() {
    support::run_async_test("agreed-cancellation", || async {
        for agreement in [AgreementType::Full, AgreementType::Percentage] {
            let (engine, ana, beto, mut fixture, stock, occurrence) = setup_with(agreement).await;
            check_balance(&engine, &stock, [25.0, 6.0, 19.0]).await;
            let old_preview = support::settlement_preview(&engine, &occurrence, &ana, 6.0).await;
            let action = Action::ProposeTransferCancellation {
                transfer: fixture.transfer.clone(),
                occurrence: occurrence.clone(),
                expected_revision: fixture.revision,
                expected_remaining_quantity: store::exact::integer(6),
                person: Some(ana.uid.clone()),
                request_id: "propose-cancel".into(),
            };
            let cancellation = engine
                .act(action.clone(), None)
                .await
                .unwrap()
                .created
                .unwrap();
            assert_eq!(
                engine.act(action, None).await.unwrap().created.as_deref(),
                Some(cancellation.as_str())
            );
            let mut changed = Action::ProposeTransferCancellation {
                transfer: fixture.transfer.clone(),
                occurrence: occurrence.clone(),
                expected_revision: fixture.revision,
                expected_remaining_quantity: store::exact::integer(5),
                person: Some(ana.uid.clone()),
                request_id: "propose-cancel".into(),
            };
            assert!(engine.act(changed.clone(), None).await.is_err());
            if let Action::ProposeTransferCancellation {
                request_id,
                expected_remaining_quantity,
                ..
            } = &mut changed
            {
                *request_id = "deliver-four".into();
                *expected_remaining_quantity = store::exact::integer(6);
            }
            assert!(engine.act(changed, None).await.is_err());
            fixture.revision += 1;
            let fact = store::transfers::revision_fact(
                &engine.store.pool,
                &fixture.transfer,
                fixture.revision,
            )
            .await
            .unwrap()
            .unwrap();
            let terms: serde_json::Value =
                serde_json::from_str(fact.payload.as_ref().unwrap()).unwrap();
            assert_eq!(terms["terms"]["cancellations"][0]["quantity"]["value"], "6");
            assert!(
                engine::trust::verify_fact(&engine.store, &fact)
                    .await
                    .unwrap()
            );
            let finalize = Action::ApplyTransferCancellation {
                transfer: fixture.transfer.clone(),
                cancellation: cancellation.clone(),
                expected_revision: fixture.revision,
                person: Some(beto.uid.clone()),
                request_id: "complete-cancel".into(),
            };
            agree(&engine, &fixture, &ana, "ana-agrees-cancel").await;
            engine.set_signer(beto.signer.clone()).await.unwrap();
            assert!(
                engine
                    .act(finalize.clone(), None)
                    .await
                    .unwrap_err()
                    .to_string()
                    .contains("all affected participants")
            );
            check_balance(&engine, &stock, [25.0, 6.0, 19.0]).await;
            agree(&engine, &fixture, &beto, "beto-agrees-cancel").await;
            let mut collision = finalize.clone();
            if let Action::ApplyTransferCancellation { request_id, .. } = &mut collision {
                *request_id = "deliver-four".into();
            }
            assert!(
                engine
                    .act(collision, None)
                    .await
                    .unwrap_err()
                    .to_string()
                    .contains("request id")
            );
            let applied = engine.act(finalize.clone(), None).await.unwrap();
            assert_eq!(
                applied.created,
                engine.act(finalize, None).await.unwrap().created
            );
            check_balance(&engine, &stock, [25.0, 0.0, 25.0]).await;
            let progress =
                store::transfers::occurrence_settlement_progress(&engine.store.pool, &occurrence)
                    .await
                    .unwrap()
                    .unwrap();
            assert_eq!(progress.settled_quantity, 4.0);
            assert_eq!(progress.cancelled_quantity, 6.0);
            assert_eq!(progress.remaining_quantity, 0.0);
            assert!(!progress.settled);
            assert_eq!(progress.slices.len(), 1);
            let cancellation_count: i64 =
                store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_occurrence_cancellation")
                    .fetch_one(&engine.store.pool)
                    .await
                    .unwrap();
            assert_eq!(cancellation_count, 1);
            assert!(
                store::sqlx::query(
                    "UPDATE transfer_occurrence_cancellation SET quantity_mantissa = '7'"
                )
                .execute(&engine.store.pool)
                .await
                .is_err()
            );
            assert!(
                store::sqlx::query("DELETE FROM transfer_cancellation_application")
                    .execute(&engine.store.pool)
                    .await
                    .is_err()
            );
            let query = protein::Protein {
                source: protein::Source::Transfer,
                filter: vec![protein::Predicate::UidEq(fixture.transfer.clone())],
                fields: None,
                include: Default::default(),
                aggregate: None,
                order: vec![],
                limit: None,
            };
            let rows =
                protein::execute_for_with_signer(&engine.store, &query, None, Some(&beto.uid))
                    .await
                    .unwrap();
            let transfer = rows
                .iter()
                .find(|row| row["uid"] == fixture.transfer)
                .unwrap();
            assert_eq!(transfer["primary_status"], "cancelled");
            assert_eq!(transfer["status"], "cancelled");
            assert_eq!(transfer["occurrences"][0]["status"], "cancelled");
            engine.set_signer(ana.signer.clone()).await.unwrap();
            let stale: Action = serde_json::from_value(serde_json::json!({
            "action":"settle-transfer-occurrence","occurrence":occurrence,"person":ana.uid,"request_id":"settle-cancelled-remainder",
            "canonical_quantity":6,"expected_remaining_quantity":old_preview["expected_remaining_quantity"],
            "expected_local_delta":old_preview["expected_local_delta"],"expected_application_formula_hash":old_preview["expected_application_formula_hash"],
            "expected_application_formula_version":old_preview["expected_application_formula_version"],"expected_remainder_policy":old_preview["expected_remainder_policy"]
        })).unwrap();
            assert!(engine.act(stale, None).await.is_err());
            check_balance(&engine, &stock, [25.0, 0.0, 25.0]).await;
            if matches!(agreement, AgreementType::Percentage) {
                let coalition = store::transfers::agreement_coalition(
                    &engine.store.pool,
                    &fixture.transfer,
                    fixture.revision,
                )
                .await
                .unwrap()
                .unwrap();
                assert_eq!(coalition.party_uids.len(), 2);
            }
        }
    });
}

#[test]
fn delivery_after_a_cancellation_proposal_requires_new_remaining_terms() {
    support::run_async_test("stale-cancellation", || async {
        let (engine, ana, beto, mut fixture, stock, occurrence) = setup().await;
        let cancellation = engine
            .act(
                Action::ProposeTransferCancellation {
                    transfer: fixture.transfer.clone(),
                    occurrence: occurrence.clone(),
                    expected_revision: fixture.revision,
                    expected_remaining_quantity: store::exact::integer(6),
                    person: Some(ana.uid.clone()),
                    request_id: "propose-six".into(),
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        fixture.revision += 1;
        let preview = support::settlement_preview(&engine, &occurrence, &ana, 1.0).await;
        support::settle_from_preview(&engine, &occurrence, &ana, "deliver-one-more", &preview)
            .await;
        for owner in [&ana, &beto] {
            agree(
                &engine,
                &fixture,
                owner,
                &format!("cancel-agree:{}", owner.uid),
            )
            .await;
        }
        let error = engine
            .act(
                Action::ApplyTransferCancellation {
                    transfer: fixture.transfer.clone(),
                    cancellation,
                    expected_revision: fixture.revision,
                    person: Some(beto.uid.clone()),
                    request_id: "cancel-stale-six".into(),
                },
                None,
            )
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("remaining quantity changed"),
            "{error}"
        );
        check_balance(&engine, &stock, [24.0, 5.0, 19.0]).await;
        engine.set_signer(ana.signer.clone()).await.unwrap();
        engine
            .act(
                Action::ProposeTransferCancellation {
                    transfer: fixture.transfer,
                    occurrence,
                    expected_revision: fixture.revision,
                    expected_remaining_quantity: store::exact::integer(5),
                    person: Some(ana.uid),
                    request_id: "propose-five".into(),
                },
                None,
            )
            .await
            .unwrap();
        check_balance(&engine, &stock, [24.0, 5.0, 19.0]).await;
    });
}

#[test]
fn pending_applications_and_nonparticipants_cannot_cancel_reserved_work() {
    support::run_async_test("pending-cancellation", || async {
        let (engine, ana, beto, fixture, stock, occurrence) = setup().await;
        let outsider = person(&engine, "cancel.outsider").await;
        engine.set_signer(outsider.signer.clone()).await.unwrap();
        let mut action = Action::ProposeTransferCancellation {
            transfer: fixture.transfer.clone(),
            occurrence: occurrence.clone(),
            expected_revision: fixture.revision,
            expected_remaining_quantity: store::exact::integer(6),
            person: Some(outsider.uid),
            request_id: "outsider-cancel".into(),
        };
        assert!(engine.act(action.clone(), None).await.is_err());
        check_balance(&engine, &stock, [25.0, 6.0, 19.0]).await;
        let mut terms = promise("pending-resource", "", &ana, -10.0);
        terms.item = Some(TransferItem {
            title: "Apples".into(),
            exchange: Some(ExchangeRoute {
                uid: "pending-resource".into(),
                giver: ana.uid.clone(),
                receiver: beto.uid.clone(),
            }),
            ..Default::default()
        });
        let fixture = create_transfer(
            &engine,
            &ana,
            std::slice::from_ref(&beto),
            vec![terms],
            DraftOptions {
                slug: "pending-application",
                agreement: AgreementType::Full,
                reserve_default: TransferReservePoint::None,
                require_confirmation: true,
            },
        )
        .await;
        for owner in [&ana, &beto] {
            agree(
                &engine,
                &fixture,
                owner,
                &format!("pending-agree:{}", owner.uid),
            )
            .await;
        }
        let occurrence = support::activate(
            &engine,
            &fixture,
            &ana,
            "pending-resource",
            "pending-activate",
        )
        .await;
        support::claim(
            &engine,
            &occurrence,
            &ana,
            TransferOccurrenceClaimRole::Delivery,
            "pending-delivery",
        )
        .await;
        support::claim(
            &engine,
            &occurrence,
            &beto,
            TransferOccurrenceClaimRole::Receipt,
            "pending-receipt",
        )
        .await;
        engine.set_signer(ana.signer.clone()).await.unwrap();
        engine
            .act(
                Action::BeginTransferSettlement {
                    transfer: fixture.transfer.clone(),
                    occurrence: occurrence.clone(),
                    person: ana.uid.clone(),
                    expected_revision: fixture.revision,
                    canonical_quantity: 1.0,
                    expected_remaining_quantity: 10.0,
                    request_id: "pending-one".into(),
                },
                None,
            )
            .await
            .unwrap();
        if let Action::ProposeTransferCancellation {
            person,
            request_id,
            transfer: target,
            occurrence: selected,
            expected_revision,
            expected_remaining_quantity,
        } = &mut action
        {
            *person = Some(ana.uid);
            *target = fixture.transfer.clone();
            *selected = occurrence;
            *expected_revision = fixture.revision;
            *expected_remaining_quantity = store::exact::integer(10);
            *request_id = "pending-cancel".into();
        }
        let error = engine.act(action, None).await.unwrap_err();
        assert!(error.to_string().contains("pending settlement"), "{error}");
        assert_eq!(
            support::current_revision(&engine, &fixture.transfer).await,
            fixture.revision
        );
        check_balance(&engine, &stock, [25.0, 6.0, 19.0]).await;
    });
}
