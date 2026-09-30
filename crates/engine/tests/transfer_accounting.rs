mod support;

use engine::actions::{Action, TransferOccurrenceClaimRole, TransferReservePoint};
use nucleus::transfer::{AgreementType, disclosure::TransferItem, exchange::ExchangeRoute};
use serde_json::json;
use support::{DraftOptions, agree, create_transfer, person, plain, promise};

#[test]
fn private_outgoing_policy_splits_exactly_and_corrections_reverse_recorded_effects() {
    support::run_async_test("private-accounting", || async {
        let engine = support::engine().await;
        let ana = person(&engine, "accounting.ana").await;
        let beto = person(&engine, "accounting.beto").await;
        let stock = plain(&engine, "accounting.stock", 30.0).await;
        let public_source = plain(&engine, "accounting.source", 30.0).await;
        let mut terms = promise("payment", &public_source, &ana, -0.3);
        terms.item = Some(TransferItem {
            title: "Payment".into(),
            exchange: Some(ExchangeRoute {
                uid: "payment".into(),
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
                slug: "accounting.transfer",
                agreement: AgreementType::Full,
                reserve_default: TransferReservePoint::None,
                require_confirmation: true,
            },
        )
        .await;
        for owner in [&ana, &beto] {
            agree(&engine, &fixture, owner, &format!("agree:{}", owner.uid)).await;
        }
        let occurrence = support::activate(&engine, &fixture, &ana, "payment", "activate").await;
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
        let policy =
            |formula: &str, version, request: &str| Action::SetTransferPrivateApplicationPolicy {
                transfer: fixture.transfer.clone(),
                exchange: "payment".into(),
                effects: vec![nucleus::transfer::application::PrivateEffect {
                    record: stock.clone(),
                    formula: formula.into(),
                    mode: Default::default(),
                }],
                person: ana.uid.clone(),
                expected_version: version,
                request_id: request.into(),
            };
        let original = support::settlement_preview(&engine, &occurrence, &ana, 0.1).await;
        let saved = engine
            .act(policy("-incoming() / 3", 0, "policy"), None)
            .await
            .unwrap();
        let replay = engine
            .act(policy("-incoming() / 3", 0, "policy"), None)
            .await
            .unwrap();
        assert_eq!(saved.created, replay.created);
        assert!(
            engine
                .act(policy("-incoming()", 0, "policy"), None)
                .await
                .is_err()
        );
        assert!(
            engine
                .act(policy("-incoming()", 0, "stale-policy"), None)
                .await
                .is_err()
        );
        engine.set_signer(beto.signer.clone()).await.unwrap();
        assert!(
            engine
                .act(policy("-incoming()", 1, "wrong-owner"), None)
                .await
                .is_err()
        );
        engine.set_signer(ana.signer.clone()).await.unwrap();
        let stale: Action = serde_json::from_value(json!({
            "action":"settle-transfer-occurrence","occurrence":occurrence,"person":ana.uid,"request_id":"stale",
            "canonical_quantity":0.1,"expected_remaining_quantity":original["expected_remaining_quantity"],
            "expected_local_delta":original["expected_local_delta"],"expected_application_formula_hash":original["expected_application_formula_hash"],
            "expected_application_formula_version":original["expected_application_formula_version"],"expected_remainder_policy":original["expected_remainder_policy"]
        })).unwrap();
        assert!(
            engine
                .act(stale, None)
                .await
                .unwrap_err()
                .to_string()
                .contains("formula")
        );
        let mut slices = Vec::new();
        for (i, amount) in [0.1, 0.2].into_iter().enumerate() {
            let preview = support::settlement_preview(&engine, &occurrence, &ana, amount).await;
            let id = format!("settle:{i}");
            let slice =
                support::settle_from_preview(&engine, &occurrence, &ana, &id, &preview).await;
            assert_eq!(
                slice,
                support::settle_from_preview(&engine, &occurrence, &ana, &id, &preview).await
            );
            slices.push(slice);
        }
        let quantity = store::records::quantity(&engine.store.pool, &stock)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            quantity.exact_numeric_cmp(nucleus::DecimalValue::parse_inferred("29.9").unwrap()),
            std::cmp::Ordering::Equal
        );
        let progress =
            store::transfers::occurrence_settlement_progress(&engine.store.pool, &occurrence)
                .await
                .unwrap()
                .unwrap();
        assert_eq!(progress.remaining_quantity, 0.0);
        engine
            .act(policy("-incoming() * 100", 1, "policy-later"), None)
            .await
            .unwrap();
        let changed_unit = store::concepts::create(&engine.store.pool, "changed-unit", &[])
            .await
            .unwrap();
        engine
            .act(
                Action::SetUnit {
                    target: stock.clone(),
                    unit: Some(changed_unit),
                },
                None,
            )
            .await
            .unwrap();
        let stale_unit = Action::CompensateTransferOccurrenceSettlement {
            settlement: slices[0].clone(),
            request_id: "wrong-unit-correction".into(),
            person: Some(ana.uid.clone()),
        };
        assert!(
            engine
                .act(stale_unit, None)
                .await
                .unwrap_err()
                .to_string()
                .contains("unit")
        );
        engine
            .act(
                Action::SetUnit {
                    target: stock.clone(),
                    unit: None,
                },
                None,
            )
            .await
            .unwrap();
        for (i, slice) in slices.into_iter().enumerate() {
            let action = Action::CompensateTransferOccurrenceSettlement {
                settlement: slice,
                request_id: format!("compensate:{i}"),
                person: Some(ana.uid.clone()),
            };
            engine.act(action.clone(), None).await.unwrap();
            engine.act(action, None).await.unwrap();
        }
        assert_eq!(
            store::records::quantity(&engine.store.pool, &stock)
                .await
                .unwrap()
                .unwrap()
                .exact_numeric_cmp(nucleus::DecimalValue::parse_inferred("30").unwrap()),
            std::cmp::Ordering::Equal
        );
        let history: i64 =
            store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_private_policy_event")
                .fetch_one(&engine.store.pool)
                .await
                .unwrap();
        assert_eq!(history, 2);
        let public = protein::execute_for_with_signer(
            &engine.store,
            &protein::Protein {
                source: protein::Source::Transfer,
                filter: vec![protein::Predicate::UidEq(fixture.transfer)],
                fields: None,
                include: Default::default(),
                aggregate: None,
                order: vec![],
                limit: None,
            },
            None,
            Some(&ana.uid),
        )
        .await
        .unwrap();
        assert!(!public.is_empty());
        let mut public = public;
        for row in &mut public {
            nucleus::transfer::disclosure::project_transfer(row, Some(&beto.uid), false);
        }
        assert!(
            !serde_json::to_string(&public)
                .unwrap()
                .contains("incoming() * 100")
        );
    });
}

#[test]
fn private_conversion_is_exact_and_rejects_incompatible_units() {
    support::run_async_test("private-unit-conversion", || async {
        let engine = support::engine().await;
        let owner = person(&engine, "units.owner").await;
        let stock = plain(&engine, "units.stock", 0.0).await;
        let mass = store::concepts::create(&engine.store.pool, "mass", &[])
            .await
            .unwrap();
        let kg = store::concepts::create(&engine.store.pool, "kg", &[&mass])
            .await
            .unwrap();
        let grams = store::concepts::create(&engine.store.pool, "grams", &[&mass])
            .await
            .unwrap();
        let length = store::concepts::create(&engine.store.pool, "length", &[])
            .await
            .unwrap();
        store::concepts::set_conversion(&engine.store.pool, &kg, &grams, 1000.0)
            .await
            .unwrap();
        engine
            .act(
                Action::SetUnit {
                    target: stock.clone(),
                    unit: Some(grams.clone()),
                },
                None,
            )
            .await
            .unwrap();
        let now = chrono::Utc::now();
        let policy = store::transfer_accounting::set_policy(
            &engine.store.pool,
            store::transfer_accounting::PolicyInput {
                transfer: "remote-transfer".into(),
                exchange: "weight".into(),
                person: owner.uid.clone(),
                effects: vec![nucleus::transfer::application::PrivateEffect {
                    record: stock.clone(),
                    formula: "-incoming () / 2".into(),
                    mode: Default::default(),
                }],
                expected_version: 0,
                request_id: "unit-policy".into(),
            },
            now,
            &owner.signer.key_id,
            &owner.signer.public_key_b64(),
            |hash| Some(owner.signer.sign_hash(hash)),
        )
        .await
        .unwrap();
        let mut binding = store::transfer_accounting::Binding {
            transfer: "remote-transfer",
            exchange: "weight",
            occurrence: None,
            person: &owner.uid,
            record: Some(&stock),
            unit: Some(&kg),
            outgoing: true,
        };
        let application = store::transfer_accounting::effective(&engine.store.pool, &binding)
            .await
            .unwrap();
        assert_eq!(application.version, 1);
        let total = nucleus::DecimalValue::parse_inferred("0.3").unwrap();
        let (delta, _) = store::transfer_accounting::calculate(
            &application.formula,
            total,
            store::exact::zero(),
        )
        .unwrap();
        assert_eq!(delta.to_string(), "-150");
        let half = nucleus::DecimalValue::parse_inferred("0.1").unwrap();
        let (first, _) =
            store::transfer_accounting::calculate(&application.formula, half, store::exact::zero())
                .unwrap();
        let (last, _) =
            store::transfer_accounting::calculate(&application.formula, total, first).unwrap();
        assert_eq!(
            store::exact::sum_exact([first, last])
                .unwrap()
                .exact_numeric_cmp(delta),
            std::cmp::Ordering::Equal
        );
        binding.unit = Some(&length);
        assert!(
            store::transfer_accounting::effective(&engine.store.pool, &binding)
                .await
                .is_err()
        );
        binding.unit = Some(&kg);
        store::concepts::set_conversion(&engine.store.pool, &kg, &grams, 999.0)
            .await
            .unwrap();
        assert_ne!(
            application.formula_hash,
            store::transfer_accounting::effective(&engine.store.pool, &binding)
                .await
                .unwrap()
                .formula_hash
        );
        assert!(
            store::sqlx::query(
                "UPDATE transfer_private_policy_event SET formula = 'incoming()' WHERE uid = ?"
            )
            .bind(&policy.uid)
            .execute(&engine.store.pool)
            .await
            .is_err()
        );
        let stored: (String, String) = store::sqlx::query_as(
            "SELECT policy_hash, signature FROM transfer_private_policy_event WHERE uid = ?",
        )
        .bind(&policy.uid)
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
        assert_eq!(stored.1, owner.signer.sign_hash(&stored.0));
    });
}
