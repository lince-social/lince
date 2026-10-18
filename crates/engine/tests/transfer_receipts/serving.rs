use super::*;
use nucleus::{
    social::{Command as Social, PublicRequest, ask::ContactConsent},
    trust_analysis::{
        Condition, Filter,
        commands::Command as Trust,
        serving::{
            self as model, Artifact, Chunk, Command as Serving, Request, Source, SourceKind,
        },
    },
};
use serde_json::{Value, json};

fn action(person: &support::Person, id: &str, request: Serving) -> Action {
    Action::TrustAnalysis {
        person: Some(person.uid.clone()),
        request_id: id.into(),
        request: Trust::Serving { request },
    }
}

#[test]
fn large_receipt_serving_chunks_preserve_originals_recheck_stop_and_revocation_without_accounting()
{
    support::run_async_test("receipt-serving-chunks", || async {
        let fixture = Fixture::new(true).await;
        let node = &fixture.reader;
        let person = history::reader(node, "receipt.serving.owner").await;
        for organ in [
            &fixture.giver_root.actor_uid,
            &fixture.receiver_root.actor_uid,
        ] {
            if store::records::get(&node.store.pool, organ)
                .await
                .unwrap()
                .is_none()
            {
                store::organs::add_contact(
                    &node.store.pool,
                    organ,
                    None,
                    "Original evidence party",
                    "",
                    1,
                )
                .await
                .unwrap();
            }
            store::replica::make_own_root(&node.store.pool, organ)
                .await
                .unwrap();
            store::visibility::grant(&node.store.pool, "actor", Some(&person.uid), organ)
                .await
                .unwrap();
        }
        let peer = support::engine().await;
        let organ = store::organs::local(&peer.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let endpoint = iroh::SecretKey::from_bytes(&[237; 32]).public().to_string();
        let server = iroh::SecretKey::from_bytes(&[238; 32]).public().to_string();
        store::organs::add_contact(&node.store.pool, &organ, None, "Evidence reader", "", 1)
            .await
            .unwrap();
        store::organs::set_trust(&node.store.pool, &organ, "known")
            .await
            .unwrap();
        store::organs::set_node_id(&node.store.pool, &organ, Some(&endpoint))
            .await
            .unwrap();
        store::replica::make_own_root(&node.store.pool, &organ)
            .await
            .unwrap();
        store::visibility::grant(&node.store.pool, "actor", Some(&person.uid), &organ)
            .await
            .unwrap();
        let root = Signer::generate(&organ, ROOT_KEY_ID);
        peer.publish_root_key(&root).await.unwrap();
        node.publish_root_key(&root).await.unwrap();
        let cell = store::cells::local(&peer.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let roster = peer
            .publish_roster(
                &root,
                vec![engine::roster::CellEntry {
                    cell_uid: cell.clone(),
                    node_id: endpoint.clone(),
                    label: "Reader Cell".into(),
                    operational_key: Signer::generate(&organ, &engine::roster::cell_key_id(&cell))
                        .public_key_b64(),
                    sealing_key: None,
                    front_door: false,
                    capabilities: engine::roster::full_capabilities(),
                }],
            )
            .await
            .unwrap();
        node.adopt_roster(&roster).await.unwrap();
        for request in [
            Social::ConfigureAsk { enabled: true },
            Social::SetAskContact {
                choice: ContactConsent {
                    organ,
                    ask: false,
                    answer: true,
                    forward: false,
                },
            },
        ] {
            node.social_command(request, None, nucleus::execution::now())
                .await
                .unwrap();
        }
        let mut bundle = fixture.bundle.clone();
        let original = bundle.publication.events[2].event.clone();
        let mut previous = original.id().unwrap();
        for index in 0..100 {
            let mut event = original.clone();
            event.supersedes = Some(previous);
            event.source_commitment = hash(&format!("serving:receipt-correction:{index}"));
            previous = event.id().unwrap();
            bundle
                .publication
                .events
                .push(sign_event(event, &fixture.receiver_writer));
        }
        let record = history::import(&fixture, &person, "serving:receipt-import", &bundle).await;
        let preview = || {
            action(
                &person,
                "serving:receipt-preview",
                Serving::Preview {
                    request: Request {
                        scope: nucleus::trust_analysis::serving::Scope::Fixed,
                        as_organ: false,
                        source: Source {
                            record: record.clone(),
                            kind: SourceKind::Receipt,
                        },
                        audience: Filter {
                            include: vec![Condition::default()],
                            exclude: Vec::new(),
                        },
                        lifetime_seconds: 86400,
                    },
                },
            )
        };
        let reviewed = node
            .act(preview(), Some(person.uid.clone()))
            .await
            .unwrap()
            .data
            .unwrap()["trust"]
            .clone();
        let content_hash: CanonicalHash =
            serde_json::from_value(reviewed["artifact_hash"].clone()).unwrap();
        let approve = |value: &Value, id: &str| {
            action(
                &person,
                id,
                Serving::Publish {
                    review: Box::new(serde_json::from_value(value["review"].clone()).unwrap()),
                    expected_review: serde_json::from_value(value["hash"].clone()).unwrap(),
                },
            )
        };
        let grant = node
            .act(
                approve(&reviewed, "serving:receipt-grant"),
                Some(person.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let discovery = |question| {
            let now = nucleus::execution::now().timestamp();
            PublicRequest::DiscoverEvidence {
                document: Box::new(nucleus::trust_analysis::discovery::Request {
                    id: nucleus::new_uid("ask"),
                    question,
                    issued_at: now,
                    deadline: now + 15,
                    after: None,
                }),
            }
        };
        let sources = node
            .social_public_request(
                &endpoint,
                &server,
                discovery(
                    nucleus::trust_analysis::discovery::Question::ReceiptSource {
                        origin: bundle.publication.scope.source.origin.clone(),
                        occurrence: bundle.publication.scope.source.occurrence.clone(),
                    },
                ),
                nucleus::execution::now().timestamp(),
            )
            .await
            .unwrap();
        assert_eq!(sources["page"]["headers"].as_array().unwrap().len(), 1);
        assert_eq!(
            sources["page"]["headers"][0]["hash"],
            serde_json::to_value(&content_hash).unwrap()
        );
        let activity = node
            .social_public_request(
                &endpoint,
                &server,
                discovery(nucleus::trust_analysis::discovery::Question::Activity {
                    subject: nucleus::trust_analysis::Identity::Organ(
                        bundle.publication.scope.giver.clone(),
                    ),
                    role: Role::Giver,
                    measurements: bundle.publication.scope.measurements.clone(),
                    from: None,
                    until: None,
                }),
                nucleus::execution::now().timestamp(),
            )
            .await
            .unwrap();
        assert_eq!(activity["page"]["headers"].as_array().unwrap().len(), 1);
        let personal = node
            .social_public_request(
                &endpoint,
                &server,
                discovery(nucleus::trust_analysis::discovery::Question::Activity {
                    subject: nucleus::trust_analysis::Identity::Person(person.uid.clone()),
                    role: Role::Giver,
                    measurements: Vec::new(),
                    from: None,
                    until: None,
                }),
                nucleus::execution::now().timestamp(),
            )
            .await
            .unwrap();
        assert!(personal["page"]["headers"].as_array().unwrap().is_empty());
        let first = node
            .social_public_request(
                &endpoint,
                &server,
                PublicRequest::FetchEvidence {
                    hash: content_hash.clone(),
                    offset: 0,
                },
                nucleus::execution::now().timestamp(),
            )
            .await
            .unwrap();
        let first: Chunk = serde_json::from_value(first["chunk"].clone()).unwrap();
        assert!(first.total as usize > model::CHUNK_BYTES);
        node.act(
            Action::DeleteRecord {
                target: grant.clone(),
            },
            None,
        )
        .await
        .unwrap();
        assert!(
            node.social_public_request(
                &endpoint,
                &server,
                PublicRequest::FetchEvidence {
                    hash: content_hash.clone(),
                    offset: model::CHUNK_BYTES as u32
                },
                nucleus::execution::now().timestamp()
            )
            .await
            .is_err()
        );
        let reviewed = node
            .act(preview(), Some(person.uid.clone()))
            .await
            .unwrap()
            .data
            .unwrap()["trust"]
            .clone();
        node.act(
            approve(&reviewed, "serving:receipt-restart"),
            Some(person.uid.clone()),
        )
        .await
        .unwrap();
        let before:(i64,i64,i64)=store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM transfer_occurrence)").fetch_one(&node.store.pool).await.unwrap();
        let mut chunks = Vec::new();
        let mut offset = 0;
        loop {
            let value = node
                .social_public_request(
                    &endpoint,
                    &server,
                    PublicRequest::FetchEvidence {
                        hash: content_hash.clone(),
                        offset,
                    },
                    nucleus::execution::now().timestamp(),
                )
                .await
                .unwrap();
            assert!(
                !serde_json::to_string(&value)
                    .unwrap()
                    .contains("source_record_uid")
            );
            let chunk: Chunk = serde_json::from_value(value["chunk"].clone()).unwrap();
            offset += chunk.decode(&content_hash, offset).unwrap().len() as u32;
            let complete = offset == chunk.total;
            chunks.push(chunk);
            if complete {
                break;
            }
        }
        let artifact = model::assemble(
            &content_hash,
            &chunks,
            nucleus::execution::now().timestamp(),
        )
        .unwrap();
        assert_eq!(
            artifact,
            Artifact::Receipt(Box::new(Document::Publication(bundle)))
        );
        let payload = String::from_utf8(artifact.bytes().unwrap()).unwrap();
        assert!(!payload.contains(&record));
        assert!(!payload.contains(&grant));
        assert!(
            model::assemble(
                &content_hash,
                &chunks[..chunks.len() - 1],
                nucleus::execution::now().timestamp()
            )
            .is_err()
        );
        let mut reordered = chunks.clone();
        reordered.swap(0, 1);
        assert!(
            model::assemble(
                &content_hash,
                &reordered,
                nucleus::execution::now().timestamp()
            )
            .is_err()
        );
        let mut changed = chunks.clone();
        changed[0].total += 1;
        assert!(
            model::assemble(
                &content_hash,
                &changed,
                nucleus::execution::now().timestamp()
            )
            .is_err()
        );
        let mut changed = chunks;
        changed[0].bytes.replace_range(..4, "AAAA");
        assert!(
            model::assemble(
                &content_hash,
                &changed,
                nucleus::execution::now().timestamp()
            )
            .is_err()
        );
        let after:(i64,i64,i64)=store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM transfer_occurrence)").fetch_one(&node.store.pool).await.unwrap();
        assert_eq!(before, after);
        assert_eq!(after.2, 0);
        let (key, signature) = node.revocation_certificate(&fixture.receiver_root);
        node.adopt_revocation(&fixture.receiver_root.actor_uid, &key, &signature)
            .await
            .unwrap();
        assert!(
            node.social_public_request(
                &endpoint,
                &server,
                PublicRequest::FetchEvidence {
                    hash: content_hash,
                    offset: 0
                },
                nucleus::execution::now().timestamp()
            )
            .await
            .is_err()
        );
        assert!(
            serde_json::from_value::<Artifact>(
                json!({"kind":"receipt","document":{"kind":"review","value":{}}})
            )
            .is_err()
        );
    });
}
