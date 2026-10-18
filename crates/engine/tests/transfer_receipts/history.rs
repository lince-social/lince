use super::*;
use serde_json::Value;

pub(super) fn republish(fixture: &Fixture, mut bundle: Bundle) -> Bundle {
    bundle.publication.scope.expires_at += 3600;
    let scope = &bundle.publication.scope;
    for approval in &mut bundle.publication.approvals {
        let signer = if approval.role == ApprovalRole::Receiver {
            &fixture.receiver_writer
        } else {
            &fixture.giver_writer
        };
        approval.signature = signer.sign_bytes(&approval.signing_bytes(scope).unwrap());
    }
    let mut ids = std::collections::BTreeMap::<CanonicalHash, CanonicalHash>::new();
    for signed in &mut bundle.publication.events {
        let previous = signed.event.id().unwrap();
        signed.event.scope = scope.id().unwrap();
        if let Some(parent) = &mut signed.event.supersedes {
            *parent = ids[parent].clone();
        }
        if let Content::Statement {
            need: Some(need), ..
        } = &mut signed.event.content
        {
            *need = ids[need].clone();
        }
        let signer = if signed.event.content.role() == ApprovalRole::Receiver {
            &fixture.receiver_writer
        } else {
            &fixture.giver_writer
        };
        *signed = sign_event(signed.event.clone(), signer);
        ids.insert(previous, signed.event.id().unwrap());
    }
    bundle
}

async fn history(engine: &Engine, person: &str, bundle: &Bundle, actor: Option<String>) -> Value {
    let result = engine
        .act(
            receipt_action(
                person,
                "receipt:history",
                Command::History {
                    before:None,
                    origin: bundle.publication.scope.source.origin.clone(),
                    occurrence: bundle.publication.scope.source.occurrence.clone(),
                },
            ),
            actor,
        )
        .await
        .unwrap();
    assert!(result.facts.is_empty());
    assert!(result.created.is_none());
    let mut value = result.data.unwrap()["receipt"]["document"]["value"].clone();
    assert!(value["checked_at"].as_i64().is_some());
    value.as_object_mut().unwrap().remove("checked_at");
    value
}

pub(super) async fn reader(engine: &Engine, label: &str) -> support::Person {
    let person = support::person(engine, label).await;
    let role = store::auth::ensure_role(&engine.store.pool, "history reader")
        .await
        .unwrap();
    for operation in ["create", "read", "update"] {
        let permission = store::auth::ensure_permission(&engine.store.pool, "record", operation)
            .await
            .unwrap();
        store::auth::grant(&engine.store.pool, role, permission)
            .await
            .unwrap();
    }
    store::auth::create_credential(&engine.store.pool, &person.uid, &person.uid, "hash", role)
        .await
        .unwrap();
    person
}

pub(super) async fn import(fixture: &Fixture, person: &support::Person, key: &str, bundle: &Bundle) -> String {
    fixture
        .reader
        .set_signer(person.signer.clone())
        .await
        .unwrap();
    let record = fixture
        .reader
        .act(
            import_action(&person.uid, key, &Document::Publication(bundle.clone())),
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    store::visibility::grant(
        &fixture.reader.store.pool,
        "actor",
        Some(&person.uid),
        &record,
    )
    .await
    .unwrap();
    record
}

#[test]
fn source_history_normalizes_copies_scopes_and_corrections_without_quantity_effects() {
    support::run_async_test("receipt-history-scopes", || async {
        let fixture = Fixture::new(true).await;
        let person = reader(&fixture.reader, "history.reader").await;
        let stock = support::plain(&fixture.reader, "history.stock", 30.0).await;
        let first = import(&fixture, &person, "history:first", &fixture.bundle).await;
        let mut extended = fixture.bundle.clone();
        let mut correction = extended.publication.events[2].event.clone();
        correction.supersedes = Some(correction.id().unwrap());
        correction.source_commitment = hash("history.receiver.correction");
        if let Content::Statement { quantity, .. } = &mut correction.content {
            *quantity = DecimalValue::parse_inferred("10").unwrap();
        }
        extended
            .publication
            .events
            .push(sign_event(correction, &fixture.receiver_writer));
        let second_bundle = republish(&fixture, extended);
        import(&fixture, &person, "history:other-scope", &second_bundle).await;
        let baseline:(i64,i64,i64)=store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM transfer_occurrence),(SELECT COUNT(*) FROM transfer_receipt_version)").fetch_one(&fixture.reader.store.pool).await.unwrap();
        let view = history(
            &fixture.reader,
            &person.uid,
            &fixture.bundle,
            Some(person.uid.clone()),
        )
        .await;
        assert_eq!(view["partial"], false);
        assert_eq!(view["history"]["source_conflict"], false);
        let outcomes = view["history"]["outcomes"].as_array().unwrap();
        assert_eq!(outcomes.len(), 5);
        assert_eq!(
            outcomes
                .iter()
                .map(|outcome| outcome["copies"].as_array().unwrap().len())
                .sum::<usize>(),
            9
        );
        let slots = view["history"]["slots"].as_array().unwrap();
        assert_eq!(slots.len(), 4);
        assert!(slots.iter().all(
            |slot| slot["conflicted"] == false && slot["heads"].as_array().unwrap().len() == 1
        ));
        let after:(i64,i64,i64)=store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM transfer_occurrence),(SELECT COUNT(*) FROM transfer_receipt_version)").fetch_one(&fixture.reader.store.pool).await.unwrap();
        assert_eq!(baseline, after);
        assert_eq!(after.1, 0);
        assert_eq!(
            store::records::get(&fixture.reader.store.pool, &stock)
                .await
                .unwrap()
                .unwrap()
                .quantity
                .canonical(),
            "30"
        );
        let original = store::transfer_receipts::get(&fixture.reader.store.pool, &first)
            .await
            .unwrap();
        assert_eq!(
            original.document,
            Document::Publication(fixture.bundle.clone())
        );
        assert_eq!(
            store::receipt_events::checked(&fixture.reader.store.pool, &original)
                .await
                .unwrap()
                .len(),
            4
        );
        assert!(
            store::sqlx::query(
                "UPDATE transfer_receipt_event SET issuer_organ='forged' WHERE record_uid=?"
            )
            .bind(&first)
            .execute(&fixture.reader.store.pool)
            .await
            .is_err()
        );
        let fourth = support::engine().await;
        for root in [&fixture.giver_root, &fixture.receiver_root] {
            fourth.publish_root_key(root).await.unwrap();
        }
        let custodian = reader(&fourth, "history.retained-copy").await;
        fourth.set_signer(custodian.signer.clone()).await.unwrap();
        for (index, bundle) in [&fixture.bundle, &second_bundle].into_iter().enumerate() {
            fourth
                .act(
                    import_action(
                        &custodian.uid,
                        &format!("retained:{index}"),
                        &Document::Publication(bundle.clone()),
                    ),
                    None,
                )
                .await
                .unwrap();
        }
        let retained = history(&fourth, &custodian.uid, &fixture.bundle, None).await;
        assert_eq!(retained["history"]["slots"], view["history"]["slots"]);
        let retained_ids = retained["history"]["outcomes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|outcome| &outcome["id"])
            .collect::<Vec<_>>();
        let original_ids = outcomes
            .iter()
            .map(|outcome| &outcome["id"])
            .collect::<Vec<_>>();
        assert_eq!(retained_ids, original_ids);
    });
}

#[test]
fn source_history_excludes_hidden_copies_and_preserves_permitted_conflicts() {
    support::run_async_test("receipt-history-audience", || async {
        let fixture = Fixture::new(true).await;
        let alice = reader(&fixture.reader, "history.alice").await;
        let bob = reader(&fixture.reader, "history.bob").await;
        import(&fixture, &alice, "history:alice", &fixture.bundle).await;
        let before = history(
            &fixture.reader,
            &alice.uid,
            &fixture.bundle,
            Some(alice.uid.clone()),
        )
        .await;
        let mut changed = fixture.bundle.clone();
        if let Content::Statement { quantity, .. } =
            &mut changed.publication.events[2].event.content
        {
            *quantity = DecimalValue::parse_inferred("1000").unwrap();
        }
        let changed = republish(&fixture, changed);
        let hidden = import(&fixture, &bob, "history:bob", &changed).await;
        let after = history(
            &fixture.reader,
            &alice.uid,
            &fixture.bundle,
            Some(alice.uid.clone()),
        )
        .await;
        assert_eq!(before, after);
        assert!(!serde_json::to_string(&after).unwrap().contains(&hidden));
        store::visibility::grant(
            &fixture.reader.store.pool,
            "actor",
            Some(&alice.uid),
            &hidden,
        )
        .await
        .unwrap();
        let mut tx = store::write_tx(&fixture.reader.store.pool).await.unwrap();
        store::data_visibility::save_on(
            &mut tx,
            &hidden,
            nucleus::visibility::Data::Record,
            None,
            &nucleus::visibility::Policy {
                include: vec![nucleus::visibility::Condition {
                    persons: vec![alice.uid.clone(), bob.uid.clone()],
                    ..Default::default()
                }],
                exclude: vec![],
            },
            1,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let shared = history(
            &fixture.reader,
            &alice.uid,
            &fixture.bundle,
            Some(alice.uid.clone()),
        )
        .await;
        let outcomes = shared["history"]["outcomes"].as_array().unwrap();
        assert_eq!(outcomes.len(), 4);
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| outcome["conflicted"] == true)
                .count(),
            1
        );
        assert_eq!(
            shared["history"]["slots"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|slot| slot["conflicted"] == true)
                .count(),
            1
        );
        let policy = nucleus::visibility::Policy {
            include: vec![nucleus::visibility::Condition {
                persons: vec![alice.uid.clone(), bob.uid.clone()],
                ..Default::default()
            }],
            exclude: vec![nucleus::visibility::Condition {
                persons: vec![alice.uid.clone()],
                ..Default::default()
            }],
        };
        let mut tx = store::write_tx(&fixture.reader.store.pool).await.unwrap();
        store::data_visibility::save_on(
            &mut tx,
            &hidden,
            nucleus::visibility::Data::Record,
            None,
            &policy,
            2,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let excluded = history(
            &fixture.reader,
            &alice.uid,
            &fixture.bundle,
            Some(alice.uid.clone()),
        )
        .await;
        assert_eq!(excluded, before);
    });
}

#[test]
fn source_history_rechecks_authority_and_refuses_an_incomplete_index() {
    support::run_async_test("receipt-history-proof-check", || async {
        let fixture = Fixture::new(true).await;
        let person = reader(&fixture.reader, "history.verifier").await;
        let record = import(&fixture, &person, "history:proof", &fixture.bundle).await;
        let original = store::transfer_receipts::get(&fixture.reader.store.pool, &record)
            .await
            .unwrap();
        store::sqlx::query("DELETE FROM transfer_receipt_event WHERE record_uid=?")
            .bind(&record)
            .execute(&fixture.reader.store.pool)
            .await
            .unwrap();
        let missing = history(
            &fixture.reader,
            &person.uid,
            &fixture.bundle,
            Some(person.uid.clone()),
        )
        .await;
        assert_eq!(missing["partial"], true);
        assert!(
            missing["history"]["outcomes"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            missing["failures"][0]["message"]
                .as_str()
                .unwrap()
                .contains("index")
        );
        let mut tx = store::write_tx(&fixture.reader.store.pool).await.unwrap();
        store::receipt_events::index_on(&mut tx, &record, original.version, &original.document)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let (revoked, signature) = fixture
            .reader
            .revocation_certificate(&fixture.receiver_root);
        assert!(
            fixture
                .reader
                .adopt_revocation(&fixture.receiver_root.actor_uid, &revoked, &signature)
                .await
                .unwrap()
        );
        let withheld = history(
            &fixture.reader,
            &person.uid,
            &fixture.bundle,
            Some(person.uid.clone()),
        )
        .await;
        assert_eq!(withheld["partial"], true);
        assert!(
            withheld["history"]["outcomes"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            withheld["failures"][0]["state"],
            "transfer_receipt_authority_withheld"
        );
    });
}
