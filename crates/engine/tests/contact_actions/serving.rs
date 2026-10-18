use super::*;
use nucleus::{
    social::{Command as Social, PublicRequest, ask::ContactConsent},
    trust_analysis::{
        Condition, Filter,
        serving::{
            self as model, Artifact, Chunk, Command as Serving, Request, Source, SourceKind,
        },
    },
};

pub(super) async fn peer(
    node: &Engine,
    other: &Engine,
    person: &support::Person,
    seed: u8,
) -> (String, String, engine::trust::Signer) {
    let organ = store::organs::local(&other.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let endpoint = iroh::SecretKey::from_bytes(&[seed; 32])
        .public()
        .to_string();
    store::organs::add_contact(&node.store.pool, &organ, None, "Query reader", "", 1)
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
    let root = engine::trust::Signer::generate(&organ, engine::roster::ROOT_KEY_ID);
    other.publish_root_key(&root).await.unwrap();
    node.publish_root_key(&root).await.unwrap();
    let cell = store::cells::local(&other.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let operational = engine::trust::Signer::generate(&organ, &engine::roster::cell_key_id(&cell));
    other.set_organ_signer(operational.clone()).await.unwrap();
    let roster = other
        .publish_roster(
            &root,
            vec![engine::roster::CellEntry {
                cell_uid: cell.clone(),
                node_id: endpoint.clone(),
                label: "Reader Cell".into(),
                operational_key: operational.public_key_b64(),
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
                organ: organ.clone(),
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
    (organ, endpoint, root)
}

pub(super) fn command(person: &support::Person, id: &str, request: Serving) -> Action {
    action(person, id, Command::Serving { request })
}
pub(super) async fn review(
    node: &Engine,
    person: &support::Person,
    record: &str,
    kind: SourceKind,
    audience: Filter,
) -> Value {
    node.act(
        command(
            person,
            &nucleus::new_uid("serving-test"),
            Serving::Preview {
                request: Request {
                    scope: model::Scope::Fixed,
                    as_organ: false,
                    source: Source {
                        record: record.into(),
                        kind,
                    },
                    audience,
                    lifetime_seconds: 86400,
                },
            },
        ),
        Some(person.uid.clone()),
    )
    .await
    .unwrap()
    .data
    .unwrap()["trust"]
        .clone()
}
pub(super) async fn approve(
    node: &Engine,
    person: &support::Person,
    review: &Value,
    id: &str,
) -> Result<engine::actions::ActionOutcome, engine::EngineError> {
    node.act(
        command(
            person,
            id,
            Serving::Publish {
                review: Box::new(serde_json::from_value(review["review"].clone()).unwrap()),
                expected_review: serde_json::from_value(review["hash"].clone()).unwrap(),
            },
        ),
        Some(person.uid.clone()),
    )
    .await
}
pub(super) async fn fetch(
    node: &Engine,
    endpoint: &str,
    hash: &nucleus::karma::CanonicalHash,
) -> Result<Value, engine::EngineError> {
    node.social_public_request(
        endpoint,
        &iroh::SecretKey::from_bytes(&[230; 32]).public().to_string(),
        PublicRequest::FetchEvidence {
            hash: hash.clone(),
            offset: 0,
        },
        nucleus::execution::now().timestamp(),
    )
    .await
}
pub(super) async fn counts(node: &Engine) -> (i64, i64, i64, i64) {
    store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM record_evidence_serving),(SELECT COUNT(*) FROM transfer_occurrence)").fetch_one(&node.store.pool).await.unwrap()
}
pub(super) fn all() -> Filter {
    Filter {
        include: vec![Condition::default()],
        exclude: Vec::new(),
    }
}

#[test]
fn serving_requires_both_peer_consent_and_composed_audience_without_exposing_private_holders() {
    support::run_async_test("serving-audience", || async {
        let node = support::engine().await;
        let reader = support::engine().await;
        let outsider = support::engine().await;
        let a = support::person(&node, "serving.a").await;
        let b = support::person(&node, "serving.b").await;
        authorize(&node, &b).await;
        authorize(&node, &a).await;
        let (reader_organ, endpoint, reader_root) = peer(&node, &reader, &a, 231).await;
        let (excluded, excluded_endpoint, _) = peer(&node, &outsider, &a, 232).await;
        let original = preview(
            &node,
            &a,
            draft(&a, &b, 0, false),
            "serving:contact-preview",
        )
        .await;
        let record = publish(&node, &a, &original, "serving:contact").await;
        let private = review(&node, &a, &record, SourceKind::Contact, Filter::default()).await;
        let grant = approve(&node, &a, &private, "serving:private")
            .await
            .unwrap()
            .created
            .unwrap();
        let hash = serde_json::from_value(private["artifact_hash"].clone()).unwrap();
        assert!(fetch(&node, &endpoint, &hash).await.is_err());
        assert!(!node.may_read_record(Some(&b.uid), &grant).await.unwrap());
        assert!(!node.may_read_record(Some(&b.uid), &record).await.unwrap());
        let audience = Filter {
            include: vec![
                Condition {
                    lower: Some(nucleus::visibility::Bound {
                        value: 1,
                        inclusive: true,
                    }),
                    upper: Some(nucleus::visibility::Bound {
                        value: 3,
                        inclusive: false,
                    }),
                    ..Default::default()
                },
                Condition {
                    identities: vec![Identity::Organ(excluded.clone())],
                    ..Default::default()
                },
            ],
            exclude: vec![Condition {
                identities: vec![Identity::Organ(excluded)],
                ..Default::default()
            }],
        };
        let reviewed = review(&node, &a, &record, SourceKind::Contact, audience).await;
        let granted = approve(&node, &a, &reviewed, "serving:ranged")
            .await
            .unwrap();
        let approved = granted.created.unwrap();
        assert_eq!(granted.facts.len(), 1);
        assert!(
            node.may_read_record(Some(&a.uid), &approved).await.unwrap(),
            "owner reads approval"
        );
        assert!(
            node.may_read_record(Some(&a.uid), &reader_organ)
                .await
                .unwrap(),
            "owner reads query contact"
        );
        let mut connection = node.store.pool.acquire().await.unwrap();
        let saved = store::evidence_serving::get_on(&mut connection, &approved)
            .await
            .unwrap();
        assert!(saved.active && saved.current, "active current approval");
        drop(connection);
        let checked = review(
            &node,
            &a,
            &record,
            SourceKind::Contact,
            saved.review.request.audience.clone(),
        )
        .await;
        assert_eq!(
            checked["artifact_hash"], reviewed["artifact_hash"],
            "same original document"
        );
        assert_eq!(
            checked["review"]["source_version"], reviewed["review"]["source_version"],
            "same source version"
        );
        assert_eq!(
            checked["review"]["source_revision"], reviewed["review"]["source_revision"],
            "same source revision"
        );
        let before = counts(&node).await;
        let value = fetch(&node, &endpoint, &hash).await.unwrap();
        let chunk: Chunk = serde_json::from_value(value["chunk"].clone()).unwrap();
        let bytes = chunk.decode(&hash, 0).unwrap();
        let packet = String::from_utf8(bytes).unwrap();
        assert!(!packet.contains(&record));
        assert!(!packet.contains(&approved));
        assert!(!packet.contains("owner_person_uid"));
        assert!(matches!(
            model::assemble(&hash, &[chunk], nucleus::execution::now().timestamp()).unwrap(),
            Artifact::Contact(_)
        ));
        assert!(fetch(&node, &excluded_endpoint, &hash).await.is_err());
        assert!(
            fetch(
                &node,
                &iroh::SecretKey::from_bytes(&[233; 32]).public().to_string(),
                &hash
            )
            .await
            .is_err()
        );
        assert!(
            approve(&node, &a, &reviewed, "serving:ranged")
                .await
                .unwrap()
                .facts
                .is_empty()
        );
        assert_eq!(counts(&node).await, before);
        let original_roster = reader.roster_of(&reader_organ).await.unwrap().unwrap();
        node.set_read_filter(
            &a.uid,
            Some(&protein::Predicate::UidEq(nucleus::new_uid("r"))),
        )
        .await
        .unwrap();
        assert!(fetch(&node, &endpoint, &hash).await.is_err());
        node.set_read_filter(&a.uid, None).await.unwrap();
        let removed = reader
            .publish_roster(&reader_root, Vec::new())
            .await
            .unwrap();
        node.adopt_roster(&removed).await.unwrap();
        assert!(fetch(&node, &endpoint, &hash).await.is_err());
        let restored = reader
            .publish_roster(&reader_root, original_roster.roster.cells)
            .await
            .unwrap();
        node.adopt_roster(&restored).await.unwrap();
        assert!(fetch(&node, &endpoint, &hash).await.is_ok());
        let local = store::organs::local(&node.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        store::sqlx::query(
            "UPDATE contact_rate SET count=128 WHERE from_organ=? AND kind='evidence-serve'",
        )
        .bind(local)
        .execute(&node.store.pool)
        .await
        .unwrap();
        assert!(fetch(&node, &endpoint, &hash).await.is_err());
        node.social_command(
            Social::ConfigureAsk { enabled: false },
            None,
            nucleus::execution::now(),
        )
        .await
        .unwrap();
        assert!(fetch(&node, &endpoint, &hash).await.is_err());
        assert!(
            serde_json::from_value::<Artifact>(
                json!({"kind":"action","action":{"type":"delete-record","target":record}})
            )
            .is_err()
        );
    });
}

#[test]
fn serving_stale_reviews_stopping_and_hidden_newer_controls_never_restore_an_old_permission() {
    support::run_async_test("serving-controls", || async {
        let node = support::engine().await;
        let reader = support::engine().await;
        let a = support::person(&node, "serving.controls.a").await;
        let b = support::person(&node, "serving.controls.b").await;
        authorize(&node, &b).await;
        authorize(&node, &a).await;
        let (_, endpoint, _) = peer(&node, &reader, &a, 234).await;
        let original = preview(
            &node,
            &a,
            draft(&a, &b, 0, false),
            "serving:controls-preview",
        )
        .await;
        let record = publish(&node, &a, &original, "serving:controls-contact").await;
        let initial = review(&node, &a, &record, SourceKind::Contact, all()).await;
        approve(&node, &a, &initial, "serving:first").await.unwrap();
        let hash = serde_json::from_value(initial["artifact_hash"].clone()).unwrap();
        assert!(fetch(&node, &endpoint, &hash).await.is_ok());
        let narrow = review(&node, &a, &record, SourceKind::Contact, Filter::default()).await;
        let stale = review(&node, &a, &record, SourceKind::Contact, all()).await;
        let last = approve(&node, &a, &narrow, "serving:narrow")
            .await
            .unwrap()
            .created
            .unwrap();
        let before = counts(&node).await;
        assert!(
            approve(&node, &a, &stale, "serving:stale-head")
                .await
                .is_err()
        );
        assert!(
            approve(&node, &b, &stale, "serving:wrong-person")
                .await
                .is_err()
        );
        assert!(approve(&node, &a, &stale, "serving:narrow").await.is_err());
        assert_eq!(counts(&node).await, before);
        node.act(Action::DeleteRecord { target: last }, None)
            .await
            .unwrap();
        assert!(fetch(&node, &endpoint, &hash).await.is_err());
        let restored = review(&node, &a, &record, SourceKind::Contact, all()).await;
        let approval = approve(&node, &a, &restored, "serving:restore")
            .await
            .unwrap()
            .created
            .unwrap();
        assert!(fetch(&node, &endpoint, &hash).await.is_ok());
        assert!(
            store::records::set_extension(
                &node.store.pool,
                &approval,
                model::NAMESPACE,
                &json!({})
            )
            .await
            .is_err()
        );
        assert!(
            store::records::delete_extension(&node.store.pool, &approval, model::NAMESPACE)
                .await
                .is_err()
        );
        assert!(
            store::sqlx::query(
                "UPDATE record_evidence_serving SET expires_at=0 WHERE record_uid=?"
            )
            .bind(&approval)
            .execute(&node.store.pool)
            .await
            .is_err()
        );
        let withdrawn = preview(
            &node,
            &a,
            draft(&a, &b, 1, true),
            "serving:withdraw-preview",
        )
        .await;
        publish(&node, &a, &withdrawn, "serving:withdraw").await;
        assert!(fetch(&node, &endpoint, &hash).await.is_err());
    });
}

#[test]
fn serving_cannot_widen_a_person_audience_and_revoked_claim_keys_stop_current_fetches() {
    support::run_async_test("serving-original-audience", || async {
        let node = support::engine().await;
        let reader = support::engine().await;
        let outsider = support::engine().await;
        let a = support::person(&node, "serving.claim.a").await;
        authorize(&node, &a).await;
        let (organ, endpoint, _) = peer(&node, &reader, &a, 235).await;
        let (_, excluded, _) = peer(&node, &outsider, &a, 236).await;
        let first =
            claims::reviewed(&node, &a, claims::draft(&a, false), "serving:claim-preview").await;
        let record = claims::publish(&node, &a, &first, "serving:claim-private")
            .await
            .unwrap()
            .created
            .unwrap();
        let private = review(&node, &a, &record, SourceKind::Claim, all()).await;
        approve(&node, &a, &private, "serving:claim-person")
            .await
            .unwrap();
        assert!(
            fetch(
                &node,
                &endpoint,
                &serde_json::from_value(private["artifact_hash"].clone()).unwrap()
            )
            .await
            .is_err()
        );
        let mut draft = claims::draft(&a, false);
        draft.audience = Audience::Selected {
            viewers: vec![Identity::Organ(organ)],
        };
        let approved = claims::reviewed(&node, &a, draft, "serving:claim-organ-preview").await;
        let record = claims::publish(&node, &a, &approved, "serving:claim-organ")
            .await
            .unwrap()
            .created
            .unwrap();
        let reviewed = review(&node, &a, &record, SourceKind::Claim, all()).await;
        approve(&node, &a, &reviewed, "serving:claim-organ-grant")
            .await
            .unwrap();
        let hash = serde_json::from_value(reviewed["artifact_hash"].clone()).unwrap();
        assert!(fetch(&node, &endpoint, &hash).await.is_ok());
        assert!(fetch(&node, &excluded, &hash).await.is_err());
        let (key, signature) = node.revocation_certificate(&a.signer);
        node.adopt_revocation(&a.uid, &key, &signature)
            .await
            .unwrap();
        assert!(fetch(&node, &endpoint, &hash).await.is_err());
    });
}

#[test]
fn serving_an_owned_organ_only_copy_requires_explicit_mode_and_current_administration() {
    support::run_async_test("serving-source-reader", || async {
        let producer = support::engine().await;
        let node = support::engine().await;
        let recipient = support::engine().await;
        let issuer = support::person(&producer, "serving.source.issuer").await;
        let controller = support::person(&node, "serving.source.controller").await;
        authorize(&producer, &issuer).await;
        authorize(&node, &controller).await;
        recognize(&node, &issuer).await;
        let role = store::auth::ensure_role(&node.store.pool, "Contact evidence owner")
            .await
            .unwrap();
        let permission = store::auth::ensure_permission(&node.store.pool, "organ", "update")
            .await
            .unwrap();
        store::auth::grant(&node.store.pool, role, permission)
            .await
            .unwrap();
        let (recipient_organ, endpoint, _) = peer(&node, &recipient, &controller, 237).await;
        let local = store::organs::local(&node.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let mut viewers = vec![Identity::Organ(local), Identity::Organ(recipient_organ)];
        viewers.sort();
        let mut draft = claims::draft(&issuer, false);
        draft.audience = Audience::Selected { viewers };
        let reviewed =
            claims::reviewed(&producer, &issuer, draft, "serving:source-claim-preview").await;
        let original = claims::publish(&producer, &issuer, &reviewed, "serving:source-claim")
            .await
            .unwrap()
            .created
            .unwrap();
        let document = store::profile_claims::get_on(
            &mut *producer.store.pool.acquire().await.unwrap(),
            &original,
        )
        .await
        .unwrap()
        .document;
        let owned = node
            .act(
                action(
                    &controller,
                    "serving:source-import",
                    Command::Claims {
                        request: nucleus::trust_analysis::claims::commands::Command::Import {
                            document: serde_json::to_string(&document).unwrap(),
                            as_organ: true,
                        },
                    },
                ),
                Some(controller.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        assert!(
            node.may_read_record(Some(&controller.uid), &owned)
                .await
                .unwrap()
        );
        let preview = |as_organ| {
            command(
                &controller,
                &nucleus::new_uid("serving-test"),
                Serving::Preview {
                    request: Request {
                        scope: model::Scope::Fixed,
                        source: Source {
                            record: owned.clone(),
                            kind: SourceKind::Claim,
                        },
                        as_organ,
                        audience: all(),
                        lifetime_seconds: 86400,
                    },
                },
            )
        };
        let before = counts(&node).await;
        assert!(
            node.act(preview(false), Some(controller.uid.clone()))
                .await
                .is_err()
        );
        assert_eq!(counts(&node).await, before);
        let reviewed = node
            .act(preview(true), Some(controller.uid.clone()))
            .await
            .unwrap()
            .data
            .unwrap()["trust"]
            .clone();
        assert_eq!(reviewed["review"]["request"]["as_organ"], true);
        let mut changed: model::Review =
            serde_json::from_value(reviewed["review"].clone()).unwrap();
        changed.request.as_organ = false;
        let changed = json!({"hash":changed.hash().unwrap(),"review":changed});
        assert!(
            approve(&node, &controller, &changed, "serving:source-mode-changed")
                .await
                .is_err()
        );
        let approved = approve(&node, &controller, &reviewed, "serving:source-approved")
            .await
            .unwrap()
            .created
            .unwrap();
        let inspect = || {
            command(
                &controller,
                &nucleus::new_uid("serving-test"),
                Serving::Inspect {
                    record: approved.clone(),
                },
            )
        };
        assert!(
            node.act(inspect(), Some(controller.uid.clone()))
                .await
                .is_ok()
        );
        let hash = serde_json::from_value(reviewed["artifact_hash"].clone()).unwrap();
        assert!(fetch(&node, &endpoint, &hash).await.is_ok());
        let question =
            super::discovery::question(nucleus::trust_analysis::discovery::Question::ClaimSeries {
                issuer: Identity::Person(issuer.uid.clone()),
                claim: document.statement.claim.clone(),
            });
        assert_eq!(
            super::discovery::discover(&node, &endpoint, &question)
                .await
                .unwrap()
                .headers
                .len(),
            1
        );
        store::auth::revoke(&node.store.pool, role, permission)
            .await
            .unwrap();
        assert!(
            node.may_read_record(Some(&controller.uid), &owned)
                .await
                .unwrap()
        );
        let before = counts(&node).await;
        assert!(
            super::discovery::discover(&node, &endpoint, &question)
                .await
                .unwrap()
                .headers
                .is_empty()
        );
        assert!(
            node.act(inspect(), Some(controller.uid.clone()))
                .await
                .is_err()
        );
        assert!(fetch(&node, &endpoint, &hash).await.is_err());
        assert!(
            node.act(preview(true), Some(controller.uid.clone()))
                .await
                .is_err()
        );
        assert!(
            approve(
                &node,
                &controller,
                &reviewed,
                "serving:source-permission-lost"
            )
            .await
            .is_err()
        );
        assert_eq!(counts(&node).await, before);
    });
}

#[test]
fn serving_reviews_reject_expiry_permission_loss_and_source_edits_without_partial_records() {
    support::run_async_test("serving-review-atomicity", || async {
        let node = support::engine().await;
        let a = support::person(&node, "serving.atomic.a").await;
        let b = support::person(&node, "serving.atomic.b").await;
        authorize(&node, &a).await;
        let original = preview(
            &node,
            &a,
            draft(&a, &b, 0, false),
            "serving:atomic-contact-preview",
        )
        .await;
        let record = publish(&node, &a, &original, "serving:atomic-contact").await;
        let reviewed = review(&node, &a, &record, SourceKind::Contact, all()).await;
        let before = counts(&node).await;
        let mut old: model::Review = serde_json::from_value(reviewed["review"].clone()).unwrap();
        old.reviewed_at -= 301;
        let expired = json!({"review":old,"hash":old.hash().unwrap()});
        assert!(
            approve(&node, &a, &expired, "serving:expired-review")
                .await
                .is_err()
        );
        let role = store::auth::ensure_role(&node.store.pool, "Contact evidence owner")
            .await
            .unwrap();
        let permission = store::auth::ensure_permission(&node.store.pool, "record", "create")
            .await
            .unwrap();
        store::auth::revoke(&node.store.pool, role, permission)
            .await
            .unwrap();
        assert!(
            approve(&node, &a, &reviewed, "serving:creation-denied")
                .await
                .is_err()
        );
        store::auth::grant(&node.store.pool, role, permission)
            .await
            .unwrap();
        assert_eq!(counts(&node).await, before);
        node.act(
            Action::EditRecordText {
                target: record.clone(),
                head: Some("Changed private source metadata".into()),
                body: None,
            },
            Some(a.uid.clone()),
        )
        .await
        .unwrap();
        let edited = counts(&node).await;
        assert!(
            approve(&node, &a, &reviewed, "serving:source-edited")
                .await
                .is_err()
        );
        assert_eq!(counts(&node).await, edited);
    });
}
