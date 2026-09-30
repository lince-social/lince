mod support;

use engine::actions::{Action, TransferOccurrenceClaimRole, TransferReservePoint};
use nucleus::transfer::{AgreementType, disclosure::TransferItem, exchange::ExchangeRoute};
use support::{DraftOptions, agree, create_transfer, person, plain, promise};

fn route(uid: &str, giver: &support::Person, receiver: &support::Person) -> TransferItem {
    TransferItem {
        title: "Crates".into(),
        exchange: Some(ExchangeRoute {
            uid: uid.into(),
            giver: giver.uid.clone(),
            receiver: receiver.uid.clone(),
        }),
        ..Default::default()
    }
}

#[test]
fn repeated_items_follow_their_routes_and_settle_independently() {
    support::run_async_test("transfer-routes", || async {
        let engine = support::engine().await;
        let ana = person(&engine, "routes.ana").await;
        let beto = person(&engine, "routes.beto").await;
        let carla = person(&engine, "routes.carla").await;
        let ana_stock = plain(&engine, "routes.ana-stock", 30.0).await;
        let carla_stock = plain(&engine, "routes.carla-stock", 5.0).await;
        let mut promises = Vec::new();
        for (uid, giver, receiver, record, delta) in [
            ("ana-beto", &ana, &beto, &ana_stock, -2.0),
            ("carla-ana", &carla, &ana, &carla_stock, -1.0),
            ("carla-beto", &carla, &beto, &carla_stock, -1.0),
        ] {
            let mut terms = promise(uid, record, giver, delta);
            terms.item = Some(route(uid, giver, receiver));
            promises.push(terms);
        }
        let mut invalid = promises.clone();
        invalid[2]
            .item
            .as_mut()
            .unwrap()
            .exchange
            .as_mut()
            .unwrap()
            .uid = "carla-ana".into();
        engine.set_signer(ana.signer.clone()).await.unwrap();
        let invalid_action: Action = serde_json::from_value(serde_json::json!({
            "action":"create-transfer-draft","request_id":"routes:invalid","creator":ana.uid,
            "slug":"routes.invalid","head":"Invalid shared route","invitees":[beto.uid,carla.uid],"promises":invalid
        })).unwrap();
        let error = engine.act(invalid_action, None).await.unwrap_err();
        assert!(
            error.to_string().contains("both sides of an exchange"),
            "{error}"
        );
        assert!(
            store::records::resolve(&engine.store.pool, "routes.invalid")
                .await
                .unwrap()
                .is_none()
        );
        let fixture = create_transfer(
            &engine,
            &ana,
            &[beto.clone(), carla.clone()],
            promises,
            DraftOptions {
                slug: "routes.transfer",
                agreement: AgreementType::Full,
                reserve_default: TransferReservePoint::None,
                require_confirmation: true,
            },
        )
        .await;
        for participant in [&ana, &beto, &carla] {
            agree(
                &engine,
                &fixture,
                participant,
                &format!("agree:{}", participant.uid),
            )
            .await;
        }
        let occurrence =
            support::activate(&engine, &fixture, &carla, "carla-ana", "activate:carla-ana").await;
        let row = store::transfers::occurrence(&engine.store.pool, &occurrence)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.giver_person_uid, carla.uid);
        assert_eq!(row.receiver_person_uid, ana.uid);
        assert_eq!(row.exchange_uid.as_deref(), Some("carla-ana"));
        support::claim(
            &engine,
            &occurrence,
            &carla,
            TransferOccurrenceClaimRole::Delivery,
            "route:delivery",
        )
        .await;
        support::claim(
            &engine,
            &occurrence,
            &ana,
            TransferOccurrenceClaimRole::Receipt,
            "route:receipt",
        )
        .await;
        let preview = support::settlement_preview(&engine, &occurrence, &carla, 1.0).await;
        support::settle_from_preview(&engine, &occurrence, &carla, "route:settle", &preview).await;
        assert_eq!(
            store::records::get(&engine.store.pool, &carla_stock)
                .await
                .unwrap()
                .unwrap()
                .quantity_f64(),
            4.0
        );
        for uid in ["ana-beto", "carla-beto"] {
            assert_eq!(
                store::misc::get_promise(&engine.store.pool, uid)
                    .await
                    .unwrap()
                    .unwrap()
                    .state,
                nucleus::PromiseState::Agreed
            );
        }
        let next = support::activate(
            &engine,
            &fixture,
            &carla,
            "carla-beto",
            "activate:carla-beto",
        )
        .await;
        let row = store::transfers::occurrence(&engine.store.pool, &next)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.receiver_person_uid, beto.uid);
        assert_eq!(row.exchange_uid.as_deref(), Some("carla-beto"));
        assert_eq!(
            store::records::get(&engine.store.pool, &ana_stock)
                .await
                .unwrap()
                .unwrap()
                .quantity_f64(),
            30.0
        );
    });
}

#[test]
fn changing_a_route_requires_fresh_agreement_before_activation() {
    support::run_async_test("transfer-route-revision", || async {
        let engine = support::engine().await;
        let ana = person(&engine, "revision.ana").await;
        let beto = person(&engine, "revision.beto").await;
        let carla = person(&engine, "revision.carla").await;
        let stock = plain(&engine, "revision.stock", 10.0).await;
        let mut terms = promise("revision-route", &stock, &ana, -1.0);
        terms.item = Some(route("revision-route", &ana, &beto));
        let fixture = create_transfer(
            &engine,
            &ana,
            &[beto.clone(), carla.clone()],
            vec![terms.clone()],
            DraftOptions {
                slug: "revision.transfer",
                agreement: AgreementType::Full,
                reserve_default: TransferReservePoint::None,
                require_confirmation: true,
            },
        )
        .await;
        for participant in [&ana, &beto, &carla] {
            agree(
                &engine,
                &fixture,
                participant,
                &format!("first:{}", participant.uid),
            )
            .await;
        }
        terms
            .item
            .as_mut()
            .unwrap()
            .exchange
            .as_mut()
            .unwrap()
            .receiver = carla.uid.clone();
        engine.set_signer(ana.signer.clone()).await.unwrap();
        let action: Action = serde_json::from_value(serde_json::json!({
            "action":"revise-transfer-draft","transfer":fixture.transfer,"expected_revision":fixture.revision,
            "request_id":"change-route","draft":{
                "creator":ana.uid,"head":"Changed recipient","agreement":"full","reserve_default":"none",
                "promises":[terms],"invitees":[],"dependencies":[]
            }
        })).unwrap();
        engine.act(action, None).await.unwrap();
        for revision in [fixture.revision, fixture.revision + 1] {
            let refused = engine
                .act(
                    Action::ActivateTransferOccurrence {
                        transfer: fixture.transfer.clone(),
                        promise: "revision-route".into(),
                        expected_revision: revision,
                        request_id: format!("stale-route:{revision}"),
                        person: Some(ana.uid.clone()),
                    },
                    None,
                )
                .await;
            assert!(refused.is_err());
        }
        let current = support::TransferFixture {
            transfer: fixture.transfer.clone(),
            revision: fixture.revision + 1,
        };
        for participant in [&ana, &beto, &carla] {
            agree(
                &engine,
                &current,
                participant,
                &format!("second:{}", participant.uid),
            )
            .await;
        }
        let occurrence =
            support::activate(&engine, &current, &ana, "revision-route", "revised-route").await;
        let row = store::transfers::occurrence(&engine.store.pool, &occurrence)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.receiver_person_uid, carla.uid);
        assert_eq!(row.exchange_uid.as_deref(), Some("revision-route"));
        assert_eq!(
            store::records::get(&engine.store.pool, &stock)
                .await
                .unwrap()
                .unwrap()
                .quantity_f64(),
            10.0
        );
    });
}

#[test]
fn guessed_upstream_is_refused_and_observing_never_joins_its_parties() {
    support::run_async_test("transfer-observer", || async {
        let engine = support::engine().await;
        let ana = person(&engine, "observer.ana").await;
        let dora = person(&engine, "observer.dora").await;
        let stock = plain(&engine, "observer.stock", 1.0).await;
        let fixture = create_transfer(
            &engine,
            &ana,
            &[],
            vec![promise("road", &stock, &ana, -1.0)],
            DraftOptions {
                slug: "observer.road",
                agreement: AgreementType::Full,
                reserve_default: TransferReservePoint::None,
                require_confirmation: true,
            },
        )
        .await;
        let role = store::auth::ensure_role(&engine.store.pool, "Observer")
            .await
            .unwrap();
        for (resource, verb) in [
            ("record", "read"),
            ("transfer", "read"),
            ("transfer", "create"),
            ("transfer", "update"),
        ] {
            let permission = store::auth::ensure_permission(&engine.store.pool, resource, verb)
                .await
                .unwrap();
            store::auth::grant(&engine.store.pool, role, permission)
                .await
                .unwrap();
        }
        store::auth::create_credential(
            &engine.store.pool,
            &dora.uid,
            "observer.dora",
            "test-hash",
            role,
        )
        .await
        .unwrap();
        engine.set_signer(dora.signer.clone()).await.unwrap();
        let action: Action = serde_json::from_value(serde_json::json!({
            "action":"create-transfer-draft","request_id":"observer:follow","creator":dora.uid,
            "head":"After the road is ready","agreement":"dependency","visibility":"hidden",
            "promises":[{"uid":"dora-need","item":{"title":"Use the road"},"record":"","party":dora.uid,"delta":1}],
            "dependencies":[{"scope":"transfer","upstream_kind":"transfer","upstream":fixture.transfer,"required_state":"kept"}]
        })).unwrap();
        assert!(matches!(
            engine.act(action.clone(), Some(dora.uid.clone())).await,
            Err(engine::EngineError::Forbidden(_))
        ));
        store::visibility::grant(
            &engine.store.pool,
            "actor",
            Some(&dora.uid),
            &fixture.transfer,
        )
        .await
        .unwrap();
        let before = store::transfers::party_levels(&engine.store.pool, &fixture.transfer)
            .await
            .unwrap();
        let observer = engine
            .act(action, Some(dora.uid.clone()))
            .await
            .unwrap()
            .created
            .unwrap();
        assert_eq!(
            store::transfers::party_levels(&engine.store.pool, &fixture.transfer)
                .await
                .unwrap(),
            before
        );
        assert_eq!(
            store::transfers::dependencies_of(&engine.store.pool, &observer)
                .await
                .unwrap()[0]
                .upstream_uid,
            fixture.transfer
        );
        assert_eq!(
            store::transfers::get(&engine.store.pool, &observer)
                .await
                .unwrap()
                .unwrap()
                .visibility,
            "hidden"
        );
        assert_eq!(
            store::records::get(&engine.store.pool, &stock)
                .await
                .unwrap()
                .unwrap()
                .quantity_f64(),
            1.0
        );
    });
}
