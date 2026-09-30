mod support;

use engine::actions::{Action, TransferOccurrenceClaimRole, TransferReservePoint};
use nucleus::transfer::{AgreementType, disclosure::TransferItem, exchange::ExchangeRoute};
use support::{DraftOptions, agree, create_transfer, person, plain, promise};

async fn balance(engine: &engine::Engine, record: &str, values: [&str; 3]) {
    let balance = store::transfer_balances::read(&engine.store.pool, record)
        .await
        .unwrap();
    assert!(balance.incomplete.is_empty(), "{:?}", balance.incomplete);
    for (actual, expected) in [balance.actual, balance.reserved, balance.surplus]
        .into_iter()
        .zip(values)
    {
        assert_eq!(
            actual.exact_numeric_cmp(nucleus::DecimalValue::parse_inferred(expected).unwrap()),
            std::cmp::Ordering::Equal,
            "{actual} != {expected}"
        );
    }
}

#[test]
fn remaining_private_obligations_follow_partial_delivery_and_corrections() {
    support::run_async_test("transfer-balances", || async {
        let engine = support::engine().await;
        let ana = person(&engine, "balance.ana").await;
        let beto = person(&engine, "balance.beto").await;
        let stock = plain(&engine, "balance.apples", 30.0).await;
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
        let transfer = create_transfer(
            &engine,
            &ana,
            std::slice::from_ref(&beto),
            vec![terms],
            DraftOptions {
                slug: "balance.transfer",
                agreement: AgreementType::Full,
                reserve_default: TransferReservePoint::Agreed,
                require_confirmation: true,
            },
        )
        .await;
        balance(&engine, &stock, ["30", "0", "30"]).await;
        for owner in [&ana, &beto] {
            agree(&engine, &transfer, owner, &format!("agree:{}", owner.uid)).await;
        }
        balance(&engine, &stock, ["30", "10", "20"]).await;
        engine.set_signer(ana.signer.clone()).await.unwrap();
        engine
            .act(
                Action::SetRecordStockLimit {
                    record: stock.clone(),
                    person: ana.uid.clone(),
                    minimum: Some(store::exact::zero()),
                    expected_version: 0,
                    request_id: "apple-limit".into(),
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
        balance(&engine, &stock, ["29", "10", "19"]).await;
        let occurrence = support::activate(&engine, &transfer, &ana, "apples", "activate").await;
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
        let preview = support::settlement_preview(&engine, &occurrence, &ana, 4.0).await;
        let slice =
            support::settle_from_preview(&engine, &occurrence, &ana, "settle-four", &preview).await;
        balance(&engine, &stock, ["25", "6", "19"]).await;
        engine.set_signer(ana.signer.clone()).await.unwrap();
        engine
            .act(
                Action::CompensateTransferOccurrenceSettlement {
                    settlement: slice,
                    request_id: "correct-four".into(),
                    person: Some(ana.uid.clone()),
                },
                None,
            )
            .await
            .unwrap();
        balance(&engine, &stock, ["29", "6", "23"]).await;
        engine
            .act(
                Action::SetTransferPrivateApplicationPolicy {
                    transfer: transfer.transfer,
                    exchange: "apples".into(),
                    person: ana.uid,
                    effects: vec![nucleus::transfer::application::PrivateEffect {
                        record: stock.clone(),
                        formula: "-incoming() / 2".into(),
                        mode: Default::default(),
                    }],
                    expected_version: 0,
                    request_id: "half".into(),
                },
                None,
            )
            .await
            .unwrap();
        balance(&engine, &stock, ["29", "1", "28"]).await;
    });
}

#[test]
fn competing_promises_keep_shortages_and_private_policy_targets_separate() {
    support::run_async_test("transfer-shortages", || async {
        let engine = support::engine().await;
        let ana = person(&engine, "shortage.ana").await;
        let beto = person(&engine, "shortage.beto").await;
        let stock = plain(&engine, "shortage.stock", 3.0).await;
        let money = plain(&engine, "shortage.money", 10.0).await;
        for (uid, amount) in [("first", 2.0), ("second", 4.0)] {
            let mut terms = promise(uid, &stock, &ana, -amount);
            terms.item = Some(TransferItem {
                title: "Stock".into(),
                exchange: Some(ExchangeRoute {
                    uid: uid.into(),
                    giver: ana.uid.clone(),
                    receiver: beto.uid.clone(),
                }),
                ..Default::default()
            });
            create_transfer(
                &engine,
                &ana,
                std::slice::from_ref(&beto),
                vec![terms],
                DraftOptions {
                    slug: uid,
                    agreement: AgreementType::Full,
                    reserve_default: TransferReservePoint::Proposed,
                    require_confirmation: false,
                },
            )
            .await;
        }
        balance(&engine, &stock, ["3", "6", "-3"]).await;
        let current = store::transfer_balances::read(&engine.store.pool, &stock)
            .await
            .unwrap();
        assert!(current.can_offer.is_zero());
        assert_eq!(current.commitments.len(), 2);
        let transfer = current
            .commitments
            .iter()
            .find(|item| item.exchange == "second")
            .unwrap()
            .transfer
            .clone();
        engine.set_signer(ana.signer.clone()).await.unwrap();
        engine
            .act(
                Action::SetTransferPrivateApplicationPolicy {
                    transfer,
                    exchange: "second".into(),
                    person: ana.uid,
                    effects: vec![nucleus::transfer::application::PrivateEffect {
                        record: money.clone(),
                        formula: "-incoming() / 2".into(),
                        mode: Default::default(),
                    }],
                    expected_version: 0,
                    request_id: "other-record".into(),
                },
                None,
            )
            .await
            .unwrap();
        balance(&engine, &stock, ["3", "2", "1"]).await;
        balance(&engine, &money, ["10", "2", "8"]).await;
    });
}

#[test]
fn hard_stock_limits_serialize_competing_reservations_and_rollback_spending() {
    support::run_async_test("hard-stock", || async {
        let engine = support::engine().await;
        let ana = person(&engine, "hard.ana").await;
        let stock = plain(&engine, "hard.apples", 3.0).await;
        engine.set_signer(ana.signer.clone()).await.unwrap();
        let limit = Action::SetRecordStockLimit {
            record: stock.clone(),
            person: ana.uid.clone(),
            minimum: Some(store::exact::zero()),
            expected_version: 0,
            request_id: "limit".into(),
        };
        let result = engine.act(limit.clone(), None).await.unwrap();
        assert_eq!(
            result.created,
            engine.act(limit, None).await.unwrap().created
        );
        let promise = || store::misc::NewPromise {
            record_uid: Some(stock.clone()),
            party_uid: Some(ana.uid.clone()),
            delta: -2.0,
            reserve_from: Some("proposed".into()),
            ..Default::default()
        };
        let (first, second) = tokio::join!(
            store::misc::insert_promise(&engine.store.pool, promise()),
            store::misc::insert_promise(&engine.store.pool, promise())
        );
        assert_ne!(first.is_ok(), second.is_ok());
        balance(&engine, &stock, ["3", "2", "1"]).await;
        let facts: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
        let refused = engine
            .act(
                Action::AddQuantity {
                    target: stock.clone(),
                    delta: -2.0,
                },
                None,
            )
            .await
            .unwrap_err();
        assert!(
            refused.to_string().contains("hard stock limit"),
            "{refused}"
        );
        assert_eq!(
            facts,
            store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM fact")
                .fetch_one(&engine.store.pool)
                .await
                .unwrap()
        );
        balance(&engine, &stock, ["3", "2", "1"]).await;
        let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
        assert!(
            store::records::bump_imported_quantity(
                &mut tx,
                &stock,
                store::exact::integer(-1),
                &nucleus::execution::now().to_rfc3339(),
                "disconnected-cell"
            )
            .await
            .is_err()
        );
        tx.rollback().await.unwrap();
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
        balance(&engine, &stock, ["2", "2", "0"]).await;
        engine
            .act(
                Action::SetRecordStockLimit {
                    record: stock.clone(),
                    person: ana.uid,
                    minimum: None,
                    expected_version: 1,
                    request_id: "remove-limit".into(),
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
        balance(&engine, &stock, ["1", "2", "-1"]).await;
    });
}

#[test]
fn hard_stock_cannot_add_writers_or_ignore_a_disconnected_cells_authorization() {
    support::run_async_test("stock-authority", || async {
        let engine = support::engine().await;
        let ana = person(&engine, "authority.ana").await;
        let stock = plain(&engine, "authority.stock", 10.0).await;
        let cell = store::cells::local(&engine.store.pool)
            .await
            .unwrap()
            .unwrap();
        let organ = store::organs::local(&engine.store.pool)
            .await
            .unwrap()
            .unwrap();
        let root = engine::trust::Signer::generate(&organ.uid, engine::roster::ROOT_KEY_ID);
        engine.publish_root_key(&root).await.unwrap();
        let entry = |uid: &str| engine::roster::CellEntry {
            cell_uid: uid.into(),
            node_id: format!("node-{uid}"),
            label: uid.into(),
            operational_key: ana.signer.public_key_b64(),
            sealing_key: None,
            front_door: false,
            capabilities: engine::roster::full_capabilities(),
        };
        engine
            .publish_roster(&root, vec![entry(&cell.uid)])
            .await
            .unwrap();
        engine.set_signer(ana.signer.clone()).await.unwrap();
        let set = |version, request: &str| Action::SetRecordStockLimit {
            record: stock.clone(),
            person: ana.uid.clone(),
            minimum: Some(store::exact::zero()),
            expected_version: version,
            request_id: request.into(),
        };
        engine.act(set(0, "first-limit"), None).await.unwrap();
        assert!(
            engine
                .publish_roster(&root, vec![entry(&cell.uid), entry("second-cell")])
                .await
                .is_err()
        );
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
        engine
            .act(
                Action::SetRecordStockLimit {
                    record: stock.clone(),
                    person: ana.uid.clone(),
                    minimum: None,
                    expected_version: 1,
                    request_id: "remove".into(),
                },
                None,
            )
            .await
            .unwrap();
        engine
            .publish_roster(&root, vec![entry(&cell.uid), entry("second-cell")])
            .await
            .unwrap();
        assert!(
            engine
                .act(set(2, "two-writers"), None)
                .await
                .unwrap_err()
                .to_string()
                .contains("only writer")
        );
        engine
            .publish_roster(&root, vec![entry(&cell.uid)])
            .await
            .unwrap();
        assert!(
            engine
                .act(set(2, "disconnected-writer"), None)
                .await
                .unwrap_err()
                .to_string()
                .contains("unexpired")
        );
        let future = nucleus::execution::Execution::new(
            [9; 32],
            (nucleus::execution::now() + chrono::Duration::days(31)).timestamp_millis(),
        )
        .unwrap();
        assert!(
            future
                .scope(engine.act(
                    Action::AddQuantity {
                        target: stock.clone(),
                        delta: -1.0
                    },
                    None
                ))
                .await
                .unwrap_err()
                .to_string()
                .contains("current write authorization")
        );
        future
            .scope(engine.publish_roster(&root, vec![entry(&cell.uid)]))
            .await
            .unwrap();
        future
            .scope(engine.act(set(2, "after-old-authority-expires"), None))
            .await
            .unwrap();
    });
}

#[test]
fn alternatives_share_unstarted_stock_but_keep_every_active_obligation() {
    support::run_async_test("alternative-balances", || async {
        for activate_both in [false, true] {
            let engine = support::engine().await;
            let ana = person(&engine, "alternative.ana").await;
            let beto = person(&engine, "alternative.beto").await;
            let stock = plain(&engine, "alternative.stock", 30.0).await;
            let need = plain(&engine, "alternative.need", -1.0).await;
            let mut transfers = Vec::new();
            for (id, amount) in [("ten", 10), ("seven", 7)] {
                engine.set_signer(ana.signer.clone()).await.unwrap();
                let action:Action=serde_json::from_value(serde_json::json!({
                "action":"create-transfer-draft","creator":ana.uid,"head":id,"request_id":id,
                "agreement":"full","satiation":"first_completes","source":need,"reserve_default":"proposed","invitees":[beto.uid],
                "promises":[{"uid":id,"record":stock,"party":ana.uid,"delta":-amount,"item":{"title":"Apples","exchange":{"uid":id,"giver":ana.uid,"receiver":beto.uid}}}]
            })).unwrap();
                let uid = engine.act(action, None).await.unwrap().created.unwrap();
                let invitation =
                    store::transfers::invitations_for_transfer(&engine.store.pool, &uid)
                        .await
                        .unwrap()
                        .pop()
                        .unwrap();
                engine.set_signer(beto.signer.clone()).await.unwrap();
                engine
                    .act(
                        Action::AcceptTransferInvitation {
                            invitation: invitation.uid,
                            expected_revision: 1,
                            request_id: format!("accept-{id}"),
                            transfer: Some(uid.clone()),
                            person: Some(beto.uid.clone()),
                        },
                        None,
                    )
                    .await
                    .unwrap();
                let fixture = support::TransferFixture {
                    transfer: uid,
                    revision: 2,
                };
                for owner in [&ana, &beto] {
                    agree(&engine, &fixture, owner, &format!("{id}-{}", owner.uid)).await;
                }
                transfers.push(fixture);
            }
            balance(&engine, &stock, ["30", "10", "20"]).await;
            let first = support::activate(&engine, &transfers[0], &ana, "ten", "active-ten").await;
            balance(&engine, &stock, ["30", "10", "20"]).await;
            if activate_both {
                support::activate(&engine, &transfers[1], &ana, "seven", "active-seven").await;
                balance(&engine, &stock, ["30", "17", "13"]).await;
            }
            support::claim(
                &engine,
                &first,
                &ana,
                TransferOccurrenceClaimRole::Delivery,
                "ten-delivered",
            )
            .await;
            support::claim(
                &engine,
                &first,
                &beto,
                TransferOccurrenceClaimRole::Receipt,
                "ten-received",
            )
            .await;
            let preview = support::settlement_preview(&engine, &first, &ana, 10.0).await;
            support::settle_from_preview(&engine, &first, &ana, "settle-ten", &preview).await;
            balance(
                &engine,
                &stock,
                if activate_both {
                    ["20", "7", "13"]
                } else {
                    ["20", "0", "20"]
                },
            )
            .await;
        }
    });
}
