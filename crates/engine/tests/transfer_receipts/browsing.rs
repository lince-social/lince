use super::history::{import, reader, republish};
use super::*;
use serde_json::Value;

async fn read(engine: &Engine, person: &str, request: Command) -> Value {
    let result = engine
        .act(
            receipt_action(person, "browsing:read", request),
            Some(person.into()),
        )
        .await
        .unwrap();
    assert!(result.facts.is_empty());
    result.data.unwrap()
}

fn history_request(bundle: &Bundle, before: Option<String>) -> Command {
    Command::History {
        origin: bundle.publication.scope.source.origin.clone(),
        occurrence: bundle.publication.scope.source.occurrence.clone(),
        before,
    }
}

async fn set_audience(engine: &Engine, record: &str, persons: Vec<String>, revision: u64) {
    let policy = nucleus::visibility::Policy {
        include: persons
            .into_iter()
            .map(|person| nucleus::visibility::Condition {
                persons: vec![person],
                ..Default::default()
            })
            .collect(),
        exclude: vec![],
    };
    let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
    store::data_visibility::save_on(
        &mut tx,
        record,
        nucleus::visibility::Data::Record,
        None,
        &policy,
        revision,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

#[test]
fn source_continuation_crosses_hidden_records_without_exposing_them_or_carrying_access() {
    support::run_async_test("receipt-source-continuation", || async {
        let fixture = Fixture::new(true).await;
        let alice = reader(&fixture.reader, "browsing.alice").await;
        let bob = reader(&fixture.reader, "browsing.bob").await;
        let original = import(&fixture, &alice, "browsing:original", &fixture.bundle).await;
        let mut hidden = Vec::new();
        for index in 0..64 {
            let mut bundle = fixture.bundle.clone();
            bundle.publication.events.clear();
            bundle.publication.scope.expires_at += index + 1;
            let bundle = republish(&fixture, bundle);
            hidden.push(import(&fixture, &bob, &format!("browsing:hidden:{index}"), &bundle).await);
        }
        let first = read(
            &fixture.reader,
            &alice.uid,
            history_request(&fixture.bundle, None),
        )
        .await;
        let first = &first["receipt"]["document"]["value"];
        assert!(first["history"]["outcomes"].as_array().unwrap().is_empty());
        assert!(first["failures"].as_array().unwrap().is_empty());
        assert_eq!(first["partial"], true);
        let cursor = first["next_before"].as_str().unwrap().to_owned();
        let json = serde_json::to_string(first).unwrap();
        for record in &hidden {
            assert!(!json.contains(record));
        }
        let second = read(
            &fixture.reader,
            &alice.uid,
            history_request(&fixture.bundle, Some(cursor.clone())),
        )
        .await;
        let second = &second["receipt"]["document"]["value"];
        assert_eq!(second["history"]["outcomes"].as_array().unwrap().len(), 4);
        assert_eq!(second["page_only"], true);
        assert_eq!(second["partial"], true);
        assert!(second["next_before"].is_null());
        assert!(
            fixture
                .reader
                .act(
                    receipt_action(
                        &bob.uid,
                        "browsing:wrong-person",
                        history_request(&fixture.bundle, Some(cursor.clone()))
                    ),
                    Some(bob.uid.clone())
                )
                .await
                .is_err()
        );
        let mut other = fixture.bundle.clone();
        other.publication.scope.source.occurrence = hash("different-source");
        assert!(
            fixture
                .reader
                .act(
                    receipt_action(
                        &alice.uid,
                        "browsing:wrong-source",
                        history_request(&other, Some(cursor.clone()))
                    ),
                    Some(alice.uid.clone())
                )
                .await
                .is_err()
        );
        assert!(
            fixture
                .reader
                .act(
                    receipt_action(
                        &alice.uid,
                        "browsing:tampered",
                        history_request(&fixture.bundle, Some(format!("{cursor}a")))
                    ),
                    Some(alice.uid.clone())
                )
                .await
                .is_err()
        );
        set_audience(&fixture.reader, &original, vec![], 1).await;
        let denied = read(
            &fixture.reader,
            &alice.uid,
            history_request(&fixture.bundle, Some(cursor)),
        )
        .await;
        let denied = &denied["receipt"]["document"]["value"];
        assert!(denied["history"]["outcomes"].as_array().unwrap().is_empty());
        assert!(!serde_json::to_string(denied).unwrap().contains(&original));
    });
}

#[test]
fn exact_versions_proof_pages_and_headers_preserve_originals_and_recheck_authority() {
    support::run_async_test("receipt-version-proof-pages", || async {
        let fixture = Fixture::new(true).await;
        let alice = reader(&fixture.reader, "proof.alice").await;
        let record = import(&fixture, &alice, "proof:original", &fixture.bundle).await;
        let mut bundle = fixture.bundle.clone();
        let mut parent = bundle.publication.events[2].event.id().unwrap();
        for index in 0..34 {
            let mut event = bundle.publication.events[2].event.clone();
            event.supersedes = Some(parent);
            event.source_commitment = hash(&format!("proof:correction:{index}"));
            if let Content::Statement { quantity, .. } = &mut event.content {
                *quantity = DecimalValue::parse_inferred(&(index + 13).to_string()).unwrap();
            }
            parent = event.id().unwrap();
            bundle
                .publication
                .events
                .push(sign_event(event, &fixture.receiver_writer));
            assert_eq!(
                import(&fixture, &alice, &format!("proof:version:{index}"), &bundle).await,
                record
            );
        }
        let baseline: (i64,i64,i64) = store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM transfer_receipt_version)")
            .fetch_one(&fixture.reader.store.pool)
            .await
            .unwrap();
        let original = read(
            &fixture.reader,
            &alice.uid,
            Command::InspectVersion {
                record: record.clone(),
                version: 1,
            },
        )
        .await;
        assert_eq!(original["receipt"]["historical"], true);
        assert_eq!(
            original["receipt"]["document"],
            serde_json::to_value(Document::Publication(fixture.bundle.clone())).unwrap()
        );
        let exported = read(
            &fixture.reader,
            &alice.uid,
            Command::ExportVersion {
                record: record.clone(),
                version: 1,
            },
        )
        .await;
        assert_eq!(
            Document::parse(
                exported["receipt_export"].as_str().unwrap().as_bytes(),
                fixture.now.timestamp()
            )
            .unwrap(),
            Document::Publication(fixture.bundle.clone())
        );
        let versions = read(
            &fixture.reader,
            &alice.uid,
            Command::Versions {
                record: record.clone(),
                before: None,
            },
        )
        .await;
        let versions = &versions["receipt"]["document"]["value"];
        assert_eq!(versions["versions"].as_array().unwrap().len(), 32);
        assert_eq!(versions["versions"][0]["version"], 35);
        let version_cursor = versions["next_before"].as_str().unwrap().to_owned();
        let older = read(
            &fixture.reader,
            &alice.uid,
            Command::Versions {
                record: record.clone(),
                before: Some(version_cursor.clone()),
            },
        )
        .await;
        let older = &older["receipt"]["document"]["value"];
        assert_eq!(older["versions"].as_array().unwrap().len(), 3);
        assert_eq!(older["versions"][2]["version"], 1);
        assert!(older["next_before"].is_null());
        let first = read(
            &fixture.reader,
            &alice.uid,
            Command::Proof {
                record: record.clone(),
                version: 35,
                before: None,
            },
        )
        .await;
        let first = &first["receipt"]["document"]["value"];
        assert_eq!(first["events"].as_array().unwrap().len(), 32);
        assert_eq!(first["total"], 38);
        let cursor = first["next_before"].as_str().unwrap().to_owned();
        let second = read(
            &fixture.reader,
            &alice.uid,
            Command::Proof {
                record: record.clone(),
                version: 35,
                before: Some(cursor.clone()),
            },
        )
        .await;
        let second = &second["receipt"]["document"]["value"];
        assert_eq!(second["events"].as_array().unwrap().len(), 6);
        assert_eq!(second["from"], 32);
        let page_document = serde_json::json!({"kind":"receipt_proof","value":first});
        let rejected = fixture
            .reader
            .act(
                receipt_action(
                    &alice.uid,
                    "proof:page-is-not-an-import",
                    Command::Import {
                        document: page_document.to_string(),
                    },
                ),
                Some(alice.uid.clone()),
            )
            .await
            .unwrap_err();
        assert_eq!(rejected.code(), Some("transfer_receipt_invalid"));
        assert_eq!(second["hash"], first["hash"]);
        assert!(second["next_before"].is_null());
        for request in [
            Command::Proof {
                record: record.clone(),
                version: 1,
                before: Some(cursor.clone()),
            },
            Command::Proof {
                record: record.clone(),
                version: 35,
                before: Some(version_cursor),
            },
            Command::InspectVersion {
                record: record.clone(),
                version: 0,
            },
        ] {
            assert!(
                fixture
                    .reader
                    .act(
                        receipt_action(&alice.uid, "proof:bad-page", request),
                        Some(alice.uid.clone())
                    )
                    .await
                    .is_err()
            );
        }
        assert!(
            fixture
                .reader
                .act_at(
                    receipt_action(
                        &alice.uid,
                        "proof:expired",
                        Command::Proof {
                            record: record.clone(),
                            version: 35,
                            before: Some(cursor.clone())
                        }
                    ),
                    Some(alice.uid.clone()),
                    nucleus::execution::now() + Duration::minutes(16)
                )
                .await
                .is_err()
        );
        let after: (i64,i64,i64) = store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM transfer_receipt_version)")
            .fetch_one(&fixture.reader.store.pool)
            .await
            .unwrap();
        assert_eq!(baseline, after);
        let bob = reader(&fixture.reader, "proof.bob").await;
        store::visibility::grant(&fixture.reader.store.pool, "actor", Some(&bob.uid), &record)
            .await
            .unwrap();
        set_audience(
            &fixture.reader,
            &record,
            vec![alice.uid.clone(), bob.uid.clone()],
            1,
        )
        .await;
        read(
            &fixture.reader,
            &bob.uid,
            Command::InspectVersion {
                record: record.clone(),
                version: 1,
            },
        )
        .await;
        assert!(
            fixture
                .reader
                .act(
                    receipt_action(
                        &bob.uid,
                        "proof:foreign-export",
                        Command::ExportVersion {
                            record: record.clone(),
                            version: 1
                        }
                    ),
                    Some(bob.uid.clone())
                )
                .await
                .is_err()
        );
        assert!(
            fixture
                .reader
                .act(
                    receipt_action(
                        &bob.uid,
                        "proof:foreign-cursor",
                        Command::Proof {
                            record: record.clone(),
                            version: 35,
                            before: Some(cursor.clone())
                        }
                    ),
                    Some(bob.uid.clone())
                )
                .await
                .is_err()
        );
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
        let withheld = fixture
            .reader
            .act(
                receipt_action(
                    &alice.uid,
                    "proof:withheld",
                    Command::Proof {
                        record: record.clone(),
                        version: 35,
                        before: Some(cursor.clone()),
                    },
                ),
                Some(alice.uid.clone()),
            )
            .await
            .unwrap_err();
        assert_eq!(withheld.code(), Some("transfer_receipt_authority_withheld"));
        set_audience(&fixture.reader, &record, vec![], 2).await;
        let denied = fixture
            .reader
            .act(
                receipt_action(
                    &alice.uid,
                    "proof:hidden",
                    Command::Proof {
                        record,
                        version: 35,
                        before: Some(cursor),
                    },
                ),
                Some(alice.uid.clone()),
            )
            .await
            .unwrap_err();
        assert_eq!(denied.code(), Some("forbidden"));
    });
}
