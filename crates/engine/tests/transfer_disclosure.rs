mod support;

use engine::actions::{
    Action, TransferDraftRevisionInput, TransferReservePoint, TransferSatiation, TransferVisibility,
};
use nucleus::transfer::{
    AgreementType,
    disclosure::{AudienceScope, TransferItem},
};
use protein::{Include, Predicate, Protein, Source};
use support::{DraftOptions, create_transfer, person, plain, promise};

fn query(source: Source, filters: Vec<Predicate>) -> Protein {
    Protein {
        source,
        filter: filters,
        fields: None,
        include: Include::default(),
        aggregate: None,
        order: Vec::new(),
        limit: None,
    }
}

#[test]
fn item_disclosure_applies_to_reads_filters_history_and_revisions() {
    support::run_async_test("transfer-disclosure", || async {
        let engine = support::engine().await;
        let ana = person(&engine, "disclosure.ana").await;
        let beto = person(&engine, "disclosure.beto").await;
        let source = plain(&engine, "secret.inventory", 99.0).await;
        let role = store::auth::ensure_role(&engine.store.pool, "Disclosure reader")
            .await
            .unwrap();
        for resource in ["record", "transfer"] {
            let permission = store::auth::ensure_permission(&engine.store.pool, resource, "read")
                .await
                .unwrap();
            store::auth::grant(&engine.store.pool, role, permission)
                .await
                .unwrap();
        }
        let create_permission =
            store::auth::ensure_permission(&engine.store.pool, "transfer", "create")
                .await
                .unwrap();
        store::auth::grant(&engine.store.pool, role, create_permission)
            .await
            .unwrap();
        store::auth::create_credential(
            &engine.store.pool,
            &beto.uid,
            "disclosure.beto",
            "test-hash",
            role,
        )
        .await
        .unwrap();
        let mut terms = promise("disclosure-item", &source, &ana, -17.0);
        let mut item = TransferItem {
            title: "City bike".into(),
            description: "Blue frame".into(),
            ..Default::default()
        };
        item.disclosure.parties.scope = AudienceScope::Owner;
        item.disclosure.quantity.scope = AudienceScope::Owner;
        terms.item = Some(item);
        let fixture = create_transfer(
            &engine,
            &ana,
            &[],
            vec![terms.clone()],
            DraftOptions {
                slug: "disclosure.transfer",
                agreement: AgreementType::Full,
                reserve_default: TransferReservePoint::None,
                require_confirmation: true,
            },
        )
        .await;
        store::visibility::grant(
            &engine.store.pool,
            "actor",
            Some(&beto.uid),
            &fixture.transfer,
        )
        .await
        .unwrap();
        let request = query(
            Source::Transfer,
            vec![Predicate::UidEq(fixture.transfer.clone())],
        );
        let rows = protein::execute_for(&engine.store, &request, Some(&beto.uid))
            .await
            .unwrap();
        let row = rows.iter().find(|row| row["kind"] == "transfer").unwrap();
        assert_eq!(row["promises"][0]["title"], "City bike");
        assert_eq!(row["promises"][0]["description"], "Blue frame");
        assert!(row["promises"][0]["delta"].is_null());
        for secret in [&source, &ana.uid, "secret.inventory"] {
            assert!(!row.to_string().contains(secret), "{row}");
        }
        for predicate in [
            Predicate::RecordEq(source.clone()),
            Predicate::PersonEq(ana.uid.clone()),
        ] {
            let filtered = protein::execute_for(
                &engine.store,
                &query(Source::Transfer, vec![predicate]),
                Some(&beto.uid),
            )
            .await
            .unwrap();
            assert!(!filtered.iter().any(|row| row["kind"] == "transfer"));
        }
        let history = protein::execute_for(
            &engine.store,
            &query(
                Source::Fact,
                vec![Predicate::RecordEq(fixture.transfer.clone())],
            ),
            Some(&beto.uid),
        )
        .await
        .unwrap();
        assert!(!history.is_empty());
        assert!(
            history
                .iter()
                .all(|fact| fact["payload"].is_null() && fact["actor"].is_null())
        );
        assert!(
            !engine
                .may_read_record(Some(&beto.uid), &source)
                .await
                .unwrap()
        );
        engine.set_signer(beto.signer.clone()).await.unwrap();
        let forbidden: Action = serde_json::from_value(serde_json::json!({
            "action":"create-transfer-draft", "request_id":"disclosure:private-source", "creator":beto.uid,
            "head":"Hidden binding attempt", "promises":[{"uid":"hidden-binding", "record":source,
                "party":beto.uid,"delta":-1,"item":{"title":"Unauthorized binding"}}]
        })).unwrap();
        let error = engine
            .act(forbidden, Some(beto.uid.clone()))
            .await
            .unwrap_err();
        assert!(
            matches!(error, engine::EngineError::Forbidden(_)),
            "{error:?}"
        );
        store::sqlx::query("INSERT INTO transfer_delivery_policy (uid, transfer_uid, origin_organ_uid, recipient_person_uid, recipient_organ_uid, created_at, updated_at) VALUES ('disclosure-policy', ?, 'origin', ?, 'recipient', '2026-01-01', '2026-01-01')")
            .bind(&fixture.transfer).bind(&beto.uid).execute(&engine.store.pool).await.unwrap();
        store::sqlx::query("INSERT INTO transfer_delivery_outbox (uid, envelope_uid, delivery_uid, cursor, transfer_revision, payload, payload_hash, next_attempt_at, created_at) VALUES ('disclosure-outbox', 'disclosure-envelope', 'disclosure-policy', 1, ?, '{}', 'hash', '2026-01-01', '2026-01-01')")
            .bind(fixture.revision as i64).execute(&engine.store.pool).await.unwrap();
        engine.set_signer(ana.signer.clone()).await.unwrap();
        terms.item.as_mut().unwrap().title = "Revised bike".into();
        let updated = terms.item.as_mut().unwrap();
        updated.disclosure.quantity.scope = AudienceScope::Selected;
        updated.disclosure.quantity.people = vec![beto.uid.clone()];
        engine
            .act(
                Action::ReviseTransferDraft {
                    transfer: fixture.transfer.clone(),
                    expected_revision: fixture.revision,
                    request_id: "disclosure:revise".into(),
                    draft: TransferDraftRevisionInput {
                        creator: ana.uid.clone(),
                        slug: Some("disclosure.transfer".into()),
                        head: "Bike".into(),
                        agreement: AgreementType::Full,
                        agreement_pct: None,
                        satiation: TransferSatiation::None,
                        parent: None,
                        source: None,
                        visibility: TransferVisibility::Hidden,
                        max_proximity: None,
                        reserve_default: TransferReservePoint::None,
                        require_confirmation: true,
                        default_place: None,
                        invitees: Vec::new(),
                        promises: vec![terms],
                        dependencies: Vec::new(),
                    },
                },
                None,
            )
            .await
            .unwrap();
        let queued: String = store::sqlx::query_scalar(
            "SELECT status FROM transfer_delivery_outbox WHERE uid = 'disclosure-outbox'",
        )
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
        assert_eq!(queued, "cancelled");
        store::visibility::grant(
            &engine.store.pool,
            "actor",
            Some(&beto.uid),
            &fixture.transfer,
        )
        .await
        .unwrap();
        let rows = protein::execute_for(&engine.store, &request, Some(&beto.uid))
            .await
            .unwrap();
        let row = rows.iter().find(|row| row["kind"] == "transfer").unwrap();
        assert_eq!(row["promises"][0]["title"], "Revised bike");
        assert_eq!(row["promises"][0]["delta"], -17.0);
        assert!(!row.to_string().contains(&source));
        assert_eq!(
            store::records::get(&engine.store.pool, &source)
                .await
                .unwrap()
                .unwrap()
                .quantity_f64(),
            99.0
        );
    });
}

#[test]
fn counteroffer_preserves_private_bindings_hidden_terms_and_disclosure() {
    support::run_async_test("transfer-private-counteroffer", || async {
        let engine = support::engine().await;
        let ana = person(&engine, "private.ana").await;
        let beto = person(&engine, "private.beto").await;
        let source = plain(&engine, "private.stock", 20.0).await;
        let mut terms = promise("private-offer", &source, &ana, -3.0);
        let mut item = TransferItem {
            title: "Apples".into(),
            ..Default::default()
        };
        item.disclosure.quantity.scope = AudienceScope::Owner;
        item.disclosure.parties.scope = AudienceScope::Owner;
        terms.item = Some(item.clone());
        let fixture = create_transfer(
            &engine,
            &ana,
            &[beto.clone()],
            vec![terms.clone()],
            DraftOptions {
                slug: "private.counteroffer",
                agreement: AgreementType::Full,
                reserve_default: TransferReservePoint::None,
                require_confirmation: true,
            },
        )
        .await;
        terms.record.clear();
        terms.delta = -1.0;
        terms.party = None;
        terms.item = Some(TransferItem {
            title: "Fresh apples".into(),
            ..Default::default()
        });
        engine.set_signer(beto.signer.clone()).await.unwrap();
        engine
            .act(
                Action::CounterofferTransfer {
                    transfer: fixture.transfer.clone(),
                    expected_revision: fixture.revision,
                    request_id: "private:counteroffer".into(),
                    person: Some(beto.uid.clone()),
                    draft: TransferDraftRevisionInput {
                        creator: ana.uid.clone(),
                        slug: Some("private.counteroffer".into()),
                        head: "Apples".into(),
                        agreement: AgreementType::Full,
                        agreement_pct: None,
                        satiation: TransferSatiation::None,
                        parent: None,
                        source: None,
                        visibility: TransferVisibility::Hidden,
                        max_proximity: None,
                        reserve_default: TransferReservePoint::None,
                        require_confirmation: true,
                        default_place: None,
                        invitees: Vec::new(),
                        promises: vec![terms],
                        dependencies: Vec::new(),
                    },
                },
                None,
            )
            .await
            .unwrap();
        let promises = store::transfers::promises_of(&engine.store.pool, &fixture.transfer)
            .await
            .unwrap();
        assert_eq!(promises[0].record_uid.as_deref(), Some(source.as_str()));
        assert_eq!(promises[0].delta, -3.0);
        assert_eq!(promises[0].party_uid.as_deref(), Some(ana.uid.as_str()));
        let saved = store::transfers::item_of(&engine.store.pool, &promises[0].uid)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.title, "Fresh apples");
        assert_eq!(saved.disclosure, item.disclosure);
    });
}

#[test]
fn public_item_can_be_drafted_without_a_private_record_binding() {
    support::run_async_test("transfer-unbound-item", || async {
        let engine = support::engine().await;
        let ana = person(&engine, "unbound.ana").await;
        let mut terms = promise("unbound-promise", "", &ana, -1.0);
        terms.item = Some(TransferItem {
            title: "A ride".into(),
            ..Default::default()
        });
        let fixture = create_transfer(
            &engine,
            &ana,
            &[],
            vec![terms],
            DraftOptions {
                slug: "unbound.transfer",
                agreement: AgreementType::Full,
                reserve_default: TransferReservePoint::None,
                require_confirmation: true,
            },
        )
        .await;
        let promises = store::transfers::promises_of(&engine.store.pool, &fixture.transfer)
            .await
            .unwrap();
        assert!(promises[0].record_uid.is_none());
        assert_eq!(
            store::transfers::item_of(&engine.store.pool, &promises[0].uid)
                .await
                .unwrap()
                .unwrap()
                .title,
            "A ride"
        );
    });
}
