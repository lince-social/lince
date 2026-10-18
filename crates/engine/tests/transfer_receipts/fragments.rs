use super::history::reader;
use super::*;
use serde_json::Value;

async fn import_fragment(
    fixture: &Fixture,
    person: &support::Person,
    key: &str,
    events: Vec<SignedEvent>,
) -> String {
    fixture
        .reader
        .set_signer(person.signer.clone())
        .await
        .unwrap();
    let mut bundle = fixture.bundle.clone();
    bundle.publication.events = events;
    let result = fixture
        .reader
        .act(
            import_action(&person.uid, key, &Document::ProofFragment(bundle)),
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        result.data.as_ref().unwrap()["receipt"]["state"],
        "authenticated_fragment"
    );
    let record = result.created.unwrap();
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

async fn history(fixture: &Fixture, person: &str) -> Value {
    fixture
        .reader
        .act(
            receipt_action(
                person,
                "fragment:history",
                Command::History {
                    origin: fixture.bundle.publication.scope.source.origin.clone(),
                    occurrence: fixture.bundle.publication.scope.source.occurrence.clone(),
                    before: None,
                },
            ),
            Some(person.into()),
        )
        .await
        .unwrap()
        .data
        .unwrap()["receipt"]["document"]["value"]
        .clone()
}

#[test]
fn missing_fragment_reference_cache_is_rejected_and_only_explicit_import_repairs_it() {
    support::run_async_test("fragment-reference-cache", || async {
        let fixture = Fixture::new(true).await;
        let person = reader(&fixture.reader, "fragment.cache-reader").await;
        let events = vec![fixture.bundle.publication.events[0].clone()];
        let record =
            import_fragment(&fixture, &person, "fragment:cache-first", events.clone()).await;
        store::sqlx::query("DELETE FROM transfer_receipt_event_reference WHERE record_uid=?")
            .bind(&record)
            .execute(&fixture.reader.store.pool)
            .await
            .unwrap();
        let rejected = history(&fixture, &person.uid).await;
        assert!(
            rejected["history"]["outcomes"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(rejected["failures"].as_array().unwrap().len(), 1);
        let cached: i64 = store::sqlx::query_scalar(
            "SELECT COUNT(*) FROM transfer_receipt_event_reference WHERE record_uid=?",
        )
        .bind(&record)
        .fetch_one(&fixture.reader.store.pool)
        .await
        .unwrap();
        assert_eq!(cached, 0);
        let before: (i64, i64) = store::sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM transfer_receipt_version)",
        )
        .fetch_one(&fixture.reader.store.pool)
        .await
        .unwrap();
        let repaired = import_fragment(&fixture, &person, "fragment:cache-repair", events).await;
        assert_eq!(repaired, record);
        let after: (i64, i64) = store::sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM transfer_receipt_version)",
        )
        .fetch_one(&fixture.reader.store.pool)
        .await
        .unwrap();
        assert_eq!(after, before);
        let accepted = history(&fixture, &person.uid).await;
        assert_eq!(accepted["history"]["outcomes"].as_array().unwrap().len(), 1);
        assert!(accepted["failures"].as_array().unwrap().is_empty());
    });
}

#[test]
fn child_first_fragments_stay_incomplete_until_the_need_is_permitted_and_verified() {
    support::run_async_test("fragment-child-first", || async {
        let fixture = Fixture::new(true).await;
        let alice = reader(&fixture.reader, "fragment.alice").await;
        let bob = reader(&fixture.reader, "fragment.bob").await;
        let child = import_fragment(
            &fixture,
            &alice,
            "fragment:child",
            vec![fixture.bundle.publication.events[2].clone()],
        )
        .await;
        let incomplete = history(&fixture, &alice.uid).await;
        assert!(
            incomplete["history"]["outcomes"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(incomplete["unresolved"][0]["state"], "incomplete");
        assert_eq!(incomplete["missing"].as_array().unwrap().len(), 1);
        let parent = import_fragment(
            &fixture,
            &bob,
            "fragment:need",
            vec![fixture.bundle.publication.events[0].clone()],
        )
        .await;
        let hidden = history(&fixture, &alice.uid).await;
        assert!(hidden["history"]["outcomes"].as_array().unwrap().is_empty());
        assert!(!serde_json::to_string(&hidden).unwrap().contains(&parent));
        store::visibility::grant(
            &fixture.reader.store.pool,
            "actor",
            Some(&alice.uid),
            &parent,
        )
        .await
        .unwrap();
        let mut tx = store::write_tx(&fixture.reader.store.pool).await.unwrap();
        store::data_visibility::save_on(
            &mut tx,
            &parent,
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
        let complete = history(&fixture, &alice.uid).await;
        assert!(complete["unresolved"].as_array().unwrap().is_empty());
        assert_eq!(complete["history"]["outcomes"].as_array().unwrap().len(), 2);
        assert!(complete["missing"].as_array().unwrap().is_empty());
        assert!(store::sqlx::query("UPDATE transfer_receipt_event_reference SET issuer_organ='forged' WHERE record_uid=?").bind(&child).execute(&fixture.reader.store.pool).await.is_err());
        let stock: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_occurrence")
            .fetch_one(&fixture.reader.store.pool)
            .await
            .unwrap();
        assert_eq!(stock, 0);
    });
}

#[test]
fn correction_history_assembles_across_artifacts_without_raising_individual_bundle_limits() {
    support::run_async_test("fragment-long-chain", || async {
        let fixture = Fixture::new(true).await;
        let person = reader(&fixture.reader, "fragment.long-reader").await;
        let original = fixture.bundle.publication.events[2].event.clone();
        let mut events = vec![
            fixture.bundle.publication.events[0].clone(),
            fixture.bundle.publication.events[2].clone(),
        ];
        let mut previous = original.id().unwrap();
        for index in 0..300 {
            let mut event = original.clone();
            event.supersedes = Some(previous);
            event.source_commitment = hash(&format!("fragment:correction:{index}"));
            previous = event.id().unwrap();
            events.push(sign_event(event, &fixture.receiver_writer));
        }
        let mut oversized = fixture.bundle.clone();
        oversized.publication.events = events.clone();
        assert!(
            oversized
                .validate_fragment(fixture.now.timestamp())
                .is_err()
        );
        let last = events.split_off(200);
        import_fragment(&fixture, &person, "fragment:last", last).await;
        assert!(
            history(&fixture, &person.uid).await["history"]["outcomes"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        import_fragment(&fixture, &person, "fragment:first", events).await;
        let complete = history(&fixture, &person.uid).await;
        assert_eq!(
            complete["history"]["outcomes"].as_array().unwrap().len(),
            302
        );
        assert!(complete["unresolved"].as_array().unwrap().is_empty());
        let slots = complete["history"]["slots"].as_array().unwrap();
        assert_eq!(slots.len(), 2);
        assert!(slots.iter().all(
            |slot| slot["conflicted"] == false && slot["heads"].as_array().unwrap().len() == 1
        ));
    });
}
