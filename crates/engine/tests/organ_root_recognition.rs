pub mod support;
use base64::Engine as _;
use engine::{Engine, actions::Action, trust::Signer, trust_analysis::IdentityAuthenticity};
use nucleus::{
    identity::{Command, roots::Proof},
    trust_analysis::{
        Identity,
        claims::{self, Authorship, Comparator, Descriptor, Metric},
        contacts::Audience,
        query::{CounterpartyBasis, EvidenceLevel},
    },
};
use serde_json::{Value, json};

async fn authorize(node: &Engine, person: &support::Person) {
    let role = store::auth::ensure_role(&node.store.pool, "Root recognition administrator")
        .await
        .unwrap();
    for (resource, operation) in [
        ("record", "read"),
        ("record", "create"),
        ("record", "update"),
        ("record", "delete"),
        ("organ", "update"),
    ] {
        let permission = store::auth::ensure_permission(&node.store.pool, resource, operation)
            .await
            .unwrap();
        store::auth::grant(&node.store.pool, role, permission)
            .await
            .unwrap();
    }
    store::auth::create_credential(&node.store.pool, &person.uid, &person.uid, "fixture", role)
        .await
        .unwrap();
    node.set_signer(person.signer.clone()).await.unwrap();
}

async fn recognize_original(node: &Engine, organ: &str, root: &Signer) {
    let local = store::organs::local(&node.store.pool)
        .await
        .unwrap()
        .unwrap();
    let mut tx = store::write_tx(&node.store.pool).await.unwrap();
    store::records::create_with_uid_on(
        &mut tx,
        organ,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Organ,
            head: "Independently recognized cooperative",
            body: "",
            quantity: store::exact::zero(),
        },
        &local.uid,
        None,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    engine::trust::adopt_key(
        &node.store,
        organ,
        engine::roster::ROOT_KEY_ID,
        &root.public_key_b64(),
    )
    .await
    .unwrap();
}

fn proof(organ: &str, root: &Signer) -> Proof {
    let now = nucleus::execution::now().timestamp();
    let mut proof = Proof {
        organ: organ.into(),
        public_key: root.public_key_b64(),
        nonce: nucleus::new_uid("r"),
        issued_at: now,
        signature: String::new(),
    };
    engine::identity::sign_organ_root_proof(&mut proof, root).unwrap();
    proof
}

fn claim(organ: &str, root: &Signer) -> claims::Document {
    let now = nucleus::execution::now().timestamp();
    let mut document = claims::Document {
        statement: claims::Statement {
            title: "Cooperative deliveries".into(),
            issuer: Identity::Organ(organ.into()),
            claim: nucleus::new_uid("r"),
            revision: 1,
            descriptor: Descriptor {
                subject: Identity::Organ(organ.into()),
                role: nucleus::transfer::outcome::Role::Giver,
                measurement: nucleus::transfer::receipt::Measurement {
                    meaning: nucleus::vocabulary::reference::MeaningReference {
                        vocabulary_uid: nucleus::new_uid("g"),
                        concept_uid: nucleus::new_uid("c"),
                        revision: 1,
                        definition_hash: "a".repeat(64),
                    },
                    unit: None,
                    object_commitment: None,
                },
                level: EvidenceLevel::NeedLinkedReceiver,
                metric: Metric::Quantity,
                counterparty_basis: CounterpartyBasis::Organ,
                from: None,
                until: None,
            },
            comparator: Comparator::AtLeast,
            amount: nucleus::DecimalValue::parse_inferred("10").unwrap(),
            as_of: now,
            issued_at: now,
            expires_at: now + 3600,
            withdrawn: false,
            audience: Audience::Public,
            authorship: Authorship::Manual,
        },
        key_id: root.key_id.clone(),
        public_key: root.public_key_b64(),
        signature: String::new(),
    };
    engine::trust_analysis::sign_claim_document(&mut document, root).unwrap();
    document
}

async fn preview(node: &Engine, person: &support::Person, proof: &Proof) -> Value {
    node.act(
        Action::Identity {
            request: Command::PreviewOrganRoot {
                subject: proof.organ.clone(),
                document: serde_json::to_string(proof).unwrap(),
            },
        },
        Some(person.uid.clone()),
    )
    .await
    .unwrap()
    .data
    .unwrap()["identity"]
        .clone()
}

fn accept(value: &Value, id: &str) -> Action {
    Action::Identity {
        request: Command::AcceptOrganRoot {
            request_id: id.into(),
            review: serde_json::from_value(value["review"].clone()).unwrap(),
            expected_review: serde_json::from_value(value["review_hash"].clone()).unwrap(),
        },
    }
}

async fn counts(node: &Engine) -> (i64, i64, i64, i64) {
    store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM organ_root_recognition),(SELECT COUNT(*) FROM transfer_occurrence)").fetch_one(&node.store.pool).await.unwrap()
}

#[test]
fn fresh_transfer_authority_starts_at_the_reviewed_root_without_reviving_revoked_ancestry() {
    support::run_async_test("root-transfer-authority", || async {
        let node = support::engine().await;
        let admin = support::person(&node, "root.receipt-admin").await;
        authorize(&node, &admin).await;
        let organ = store::organs::local(&node.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let cell = store::cells::local(&node.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let old = Signer::generate(&organ, engine::roster::ROOT_KEY_ID);
        let fresh = Signer::generate(&organ, engine::roster::ROOT_KEY_ID);
        let operational = Signer::generate(&organ, &engine::roster::cell_key_id(&cell));
        node.publish_root_key(&old).await.unwrap();
        node.sign_succession(&old, &fresh.public_key_b64())
            .await
            .unwrap();
        node.set_organ_signer(operational.clone()).await.unwrap();
        node.publish_roster(
            &fresh,
            vec![engine::roster::CellEntry {
                cell_uid: cell.clone(),
                node_id: iroh::SecretKey::from_bytes(&[76; 32]).public().to_string(),
                label: "Receipt Cell".into(),
                operational_key: operational.public_key_b64(),
                sealing_key: None,
                front_door: false,
                capabilities: engine::roster::full_capabilities(),
            }],
        )
        .await
        .unwrap();
        let original_authority = node
            .create_transfer_receipt_authority(&fresh, &cell, nucleus::execution::now())
            .await
            .unwrap();
        assert_eq!(original_authority.successions.len(), 1);
        let document = serde_json::to_string(&nucleus::transfer::receipt::Document::Authority(
            original_authority,
        ))
        .unwrap();
        let (key, signature) = node.revocation_certificate(&old);
        node.adopt_revocation(&organ, &key, &signature)
            .await
            .unwrap();
        assert!(
            node.create_transfer_receipt_authority(&fresh, &cell, nucleus::execution::now())
                .await
                .is_err()
        );
        let value = preview(&node, &admin, &proof(&organ, &fresh)).await;
        assert!(
            node.act(
                Action::TransferReceipt {
                    person: Some(admin.uid.clone()),
                    request_id: "root:authority-before-review".into(),
                    request: nucleus::transfer::receipt::Command::Import {
                        document: document.clone()
                    }
                },
                Some(admin.uid.clone())
            )
            .await
            .is_err()
        );
        let before = serde_json::to_value(node.roster_of(&organ).await.unwrap().unwrap()).unwrap();
        node.act(
            accept(&value, "root:receipt-recovery"),
            Some(admin.uid.clone()),
        )
        .await
        .unwrap();
        assert_eq!(
            serde_json::to_value(node.roster_of(&organ).await.unwrap().unwrap()).unwrap(),
            before
        );
        node.act(
            Action::TransferReceipt {
                person: Some(admin.uid.clone()),
                request_id: "root:authority-after-review".into(),
                request: nucleus::transfer::receipt::Command::Import { document },
            },
            Some(admin.uid.clone()),
        )
        .await
        .unwrap();
        let authority = node
            .create_transfer_receipt_authority(&fresh, &cell, nucleus::execution::now())
            .await
            .unwrap();
        assert_eq!(authority.root_key, fresh.public_key_b64());
        assert!(authority.successions.is_empty());
        authority
            .validate(nucleus::execution::now().timestamp())
            .unwrap();
        assert_eq!(node.published_successions(&organ).await.unwrap().len(), 1);
        assert_eq!(
            engine::trust::key_of(&node.store, &organ, engine::roster::ROOT_KEY_ID)
                .await
                .unwrap(),
            Some(old.public_key_b64())
        );
        assert_eq!(counts(&node).await.3, 0);
    });
}

#[test]
fn creation_permission_and_current_profile_visibility_fail_without_partial_acceptance() {
    support::run_async_test("root-acceptance-rollback", || async {
        let source = support::engine().await;
        let node = support::engine().await;
        let admin = support::person(&node, "root.atomic-admin").await;
        authorize(&node, &admin).await;
        let organ = store::organs::local(&source.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let old = Signer::generate(&organ, engine::roster::ROOT_KEY_ID);
        let fresh = Signer::generate(&organ, engine::roster::ROOT_KEY_ID);
        recognize_original(&node, &organ, &old).await;
        let proof = proof(&organ, &fresh);
        let value = preview(&node, &admin, &proof).await;
        let before = counts(&node).await;
        let role = store::auth::ensure_role(&node.store.pool, "Root recognition administrator")
            .await
            .unwrap();
        let create = store::auth::ensure_permission(&node.store.pool, "record", "create")
            .await
            .unwrap();
        store::sqlx::query("DELETE FROM role_permission WHERE role_id=? AND permission_id=?")
            .bind(role)
            .bind(create)
            .execute(&node.store.pool)
            .await
            .unwrap();
        assert!(
            node.act(accept(&value, "root:no-create"), Some(admin.uid.clone()))
                .await
                .is_err()
        );
        assert_eq!(counts(&node).await, before);
        store::auth::grant(&node.store.pool, role, create)
            .await
            .unwrap();
        node.set_read_filter(
            &admin.uid,
            Some(&protein::Predicate::KindEq("person".into())),
        )
        .await
        .unwrap();
        assert!(
            node.act(accept(&value, "root:no-read"), Some(admin.uid.clone()))
                .await
                .is_err()
        );
        assert!(
            node.act(
                Action::Identity {
                    request: Command::PreviewOrganRoot {
                        subject: organ.clone(),
                        document: serde_json::to_string(&proof).unwrap()
                    }
                },
                Some(admin.uid.clone())
            )
            .await
            .is_err()
        );
        assert_eq!(counts(&node).await, before);
        assert_eq!(
            engine::trust::key_of(&node.store, &organ, engine::roster::ROOT_KEY_ID)
                .await
                .unwrap(),
            Some(old.public_key_b64())
        );
    });
}

#[test]
fn a_signed_client_intent_authorizes_the_audit_without_the_person_private_key_on_the_server() {
    support::run_async_test("root-client-intent", || async {
        let source = support::engine().await;
        let node = support::engine().await;
        let admin = support::person(&node, "root.remote-admin").await;
        authorize(&node, &admin).await;
        let organ = store::organs::local(&source.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let old = Signer::generate(&organ, engine::roster::ROOT_KEY_ID);
        let fresh = Signer::generate(&organ, engine::roster::ROOT_KEY_ID);
        recognize_original(&node, &organ, &old).await;
        let review = preview(&node, &admin, &proof(&organ, &fresh)).await;
        let runtime = support::person(&node, "root.runtime-only").await;
        node.set_signer(runtime.signer).await.unwrap();
        let action = accept(&review, "root:client-signed");
        let mut session = node.begin_action_intent_session(&admin.uid).await.unwrap();
        let mut authentication = nucleus::action_intent::ActionIntentSessionProof {
            session_id: session.session_id().into(),
            session_challenge: session.challenge().into(),
            person_uid: admin.uid.clone(),
            key_id: admin.signer.key_id.clone(),
            public_key_base64: admin.signer.public_key_b64(),
            signature: String::new(),
        };
        authentication.signature = admin.signer.sign_bytes(&authentication.signing_bytes());
        node.authenticate_action_intent_session(&mut session, authentication)
            .await
            .unwrap();
        let mut intent = nucleus::action_intent::SignedActionIntent {
            session_id: session.session_id().into(),
            session_challenge: session.challenge().into(),
            sequence: 1,
            message_id: "root-signed-client".into(),
            action_base64: base64::engine::general_purpose::STANDARD
                .encode(serde_json::to_vec(&action).unwrap()),
            signature: String::new(),
        };
        intent.signature = admin.signer.sign_bytes(&intent.signing_bytes());
        let verified = node
            .verify_action_intent(&mut session, intent)
            .await
            .unwrap();
        let outcome = node.act_verified_intent(verified).await.unwrap();
        assert_eq!(outcome.facts.len(), 1);
        assert_eq!(
            outcome.facts[0].actor_uid.as_deref(),
            Some(admin.uid.as_str())
        );
        assert!(outcome.facts[0].signature.is_none());
        assert!(
            engine::trust::verify_fact(&node.store, &outcome.facts[0])
                .await
                .unwrap()
        );
        assert!(
            node.key_chains(&organ, &fresh.public_key_b64())
                .await
                .unwrap()
        );
    });
}

#[test]
fn three_linces_recover_only_after_independent_review_preserving_revocations_and_original_anchors()
{
    support::run_async_test("root-independent-recovery", || async {
        let source = support::engine().await;
        let reader = support::engine().await;
        let observer = support::engine().await;
        let admin = support::person(&reader, "root.admin").await;
        authorize(&reader, &admin).await;
        let organ = store::organs::local(&source.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let old = Signer::generate(&organ, engine::roster::ROOT_KEY_ID);
        let fresh = Signer::generate(&organ, engine::roster::ROOT_KEY_ID);
        source.publish_root_key(&old).await.unwrap();
        source
            .sign_succession(&old, &fresh.public_key_b64())
            .await
            .unwrap();
        let certificate = source
            .published_successions(&organ)
            .await
            .unwrap()
            .pop()
            .unwrap();
        for node in [&reader, &observer] {
            recognize_original(node, &organ, &old).await;
            assert!(
                node.adopt_succession(
                    &organ,
                    &certificate.old_key,
                    &certificate.new_key,
                    &certificate.created_at,
                    &certificate.signature
                )
                .await
                .unwrap()
            );
            let (key, signature) = source.revocation_certificate(&old);
            assert!(
                node.adopt_revocation(&organ, &key, &signature)
                    .await
                    .unwrap()
            );
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("owner-key");
        std::fs::write(&path, fresh.secret_bytes()).unwrap();
        source.set_root_key_path(path.clone());
        let issue_clock = nucleus::execution::Execution::new(
            [74; 32],
            nucleus::execution::now().timestamp_millis() - 90 * 86400 * 1000,
        )
        .unwrap();
        let exported = issue_clock
            .scope(source.act(
                Action::Identity {
                    request: Command::ExportOrganRoot {
                        subject: organ.clone(),
                    },
                },
                None,
            ))
            .await
            .unwrap();
        assert!(exported.created.is_none());
        assert!(exported.facts.is_empty());
        let exported = exported.data.unwrap();
        let document = exported["identity"]["document"].as_str().unwrap();
        let proof = Proof::parse(document.as_bytes()).unwrap();
        let original = claim(&organ, &old);
        let current = claim(&organ, &fresh);
        let old_key = old.public_key_b64();
        let fresh_key = fresh.public_key_b64();
        std::fs::remove_file(&path).unwrap();
        drop(old);
        drop(fresh);
        assert!(proof.issued_at < nucleus::execution::now().timestamp() - 89 * 86400);
        for node in [&reader, &observer] {
            assert_eq!(
                node.authenticate_profile_claim(&current, nucleus::execution::now().timestamp())
                    .await
                    .unwrap(),
                IdentityAuthenticity::WithheldAuthority
            );
        }
        let baseline = counts(&reader).await;
        let value = preview(&reader, &admin, &proof).await;
        assert_eq!(counts(&reader).await, baseline);
        let saved = reader
            .act(accept(&value, "root:accept"), Some(admin.uid.clone()))
            .await
            .unwrap();
        assert_eq!(saved.facts.len(), 1);
        let record = saved.created.unwrap();
        assert_eq!(
            reader.current_root_key(&organ).await.unwrap(),
            Some(fresh_key.clone())
        );
        assert_eq!(
            reader
                .authenticate_profile_claim(&current, nucleus::execution::now().timestamp())
                .await
                .unwrap(),
            IdentityAuthenticity::Accepted
        );
        assert_eq!(
            reader
                .authenticate_profile_claim(&original, nucleus::execution::now().timestamp())
                .await
                .unwrap(),
            IdentityAuthenticity::WithheldAuthority
        );
        assert_eq!(
            observer
                .authenticate_profile_claim(&current, nucleus::execution::now().timestamp())
                .await
                .unwrap(),
            IdentityAuthenticity::WithheldAuthority
        );
        assert_eq!(
            engine::trust::key_of(&reader.store, &organ, engine::roster::ROOT_KEY_ID)
                .await
                .unwrap(),
            Some(old_key.clone())
        );
        assert!(!reader.key_chains(&organ, &old_key).await.unwrap());
        assert!(reader.key_chains(&organ, &fresh_key).await.unwrap());
        let replay = reader
            .act(accept(&value, "root:accept"), Some(admin.uid.clone()))
            .await
            .unwrap();
        assert!(replay.facts.is_empty());
        assert_eq!(replay.created, Some(record.clone()));
        let after = counts(&reader).await;
        assert_eq!(after, (baseline.0 + 1, baseline.1 + 1, 1, baseline.3));
        let roots = reader
            .act(
                Action::Identity {
                    request: Command::OrganRoots {
                        subject: organ.clone(),
                    },
                },
                Some(admin.uid.clone()),
            )
            .await
            .unwrap()
            .data
            .unwrap();
        assert!(!serde_json::to_string(&roots).unwrap().contains(&admin.uid));
        assert_eq!(
            roots["identity"]["state"]["current"]["public_key"],
            fresh_key
        );
        assert!(
            store::sqlx::query("UPDATE organ_root_recognition SET public_key=? WHERE record_uid=?")
                .bind(&old_key)
                .bind(&record)
                .execute(&reader.store.pool)
                .await
                .is_err()
        );
        assert!(
            store::sqlx::query("DELETE FROM organ_root_recognition WHERE record_uid=?")
                .bind(&record)
                .execute(&reader.store.pool)
                .await
                .is_err()
        );
        assert!(
            store::records::set_extension(
                &reader.store.pool,
                &record,
                nucleus::identity::roots::NAMESPACE,
                &json!({"public_key":old_key})
            )
            .await
            .is_err()
        );
        assert!(
            store::records::delete_extension(
                &reader.store.pool,
                &record,
                nucleus::identity::roots::NAMESPACE
            )
            .await
            .is_err()
        );
        reader
            .act(
                Action::DeleteRecord { target: record },
                Some(admin.uid.clone()),
            )
            .await
            .unwrap();
        assert_eq!(
            reader
                .authenticate_profile_claim(&current, nucleus::execution::now().timestamp())
                .await
                .unwrap(),
            IdentityAuthenticity::WithheldAuthority
        );
        assert!(!reader.key_chains(&organ, &fresh_key).await.unwrap());
        assert_eq!(reader.current_root_key(&organ).await.unwrap(), None);
        let review = preview(&reader, &admin, &proof).await;
        reader
            .act(
                accept(&review, "root:restore-explicit"),
                Some(admin.uid.clone()),
            )
            .await
            .unwrap();
        assert_eq!(
            reader
                .authenticate_profile_claim(&current, nucleus::execution::now().timestamp())
                .await
                .unwrap(),
            IdentityAuthenticity::Accepted
        );
    });
}

#[test]
fn wrong_org_forged_revoked_and_stale_reviews_cannot_change_identity() {
    support::run_async_test("root-review-validation", || async {
        let clock = nucleus::execution::Execution::new([73; 32], 1_800_000_000_000).unwrap();
        clock
            .scope(async {
                let source = support::engine().await;
                let node = support::engine().await;
                let admin = support::person(&node, "root.reviewer").await;
                authorize(&node, &admin).await;
                let organ = store::organs::local(&source.store.pool)
                    .await
                    .unwrap()
                    .unwrap()
                    .uid;
                let old = Signer::generate(&organ, engine::roster::ROOT_KEY_ID);
                let fresh = Signer::generate(&organ, engine::roster::ROOT_KEY_ID);
                recognize_original(&node, &organ, &old).await;
                let proof = proof(&organ, &fresh);
                let baseline = counts(&node).await;
                let mut forged = proof.clone();
                forged.nonce = nucleus::new_uid("r");
                let mut future = proof.clone();
                future.issued_at += 3600;
                future.signature.clear();
                engine::identity::sign_organ_root_proof(&mut future, &fresh).unwrap();
                for (subject, document) in [
                    (organ.clone(), serde_json::to_string(&forged).unwrap()),
                    (organ.clone(), serde_json::to_string(&future).unwrap()),
                    (admin.uid.clone(), serde_json::to_string(&proof).unwrap()),
                    (organ.clone(), "{\"action\":\"settle-transfer\"}".into()),
                ] {
                    assert!(
                        node.act(
                            Action::Identity {
                                request: Command::PreviewOrganRoot { subject, document }
                            },
                            Some(admin.uid.clone())
                        )
                        .await
                        .is_err()
                    );
                }
                let value = preview(&node, &admin, &proof).await;
                let mut changed = value.clone();
                changed["review"]["proof"]["public_key"] = json!(old.public_key_b64());
                assert!(
                    node.act(accept(&changed, "root:edited"), Some(admin.uid.clone()))
                        .await
                        .is_err()
                );
                node.act(
                    Action::EditRecordText {
                        target: organ.clone(),
                        head: Some("Reviewed profile changed".into()),
                        body: None,
                    },
                    Some(admin.uid.clone()),
                )
                .await
                .unwrap();
                assert!(
                    node.act(
                        accept(&value, "root:stale-profile"),
                        Some(admin.uid.clone())
                    )
                    .await
                    .is_err()
                );
                let value = preview(&node, &admin, &proof).await;
                clock
                    .set_time(clock.now().timestamp_millis() + 301000)
                    .unwrap();
                assert!(
                    node.act(accept(&value, "root:old-review"), Some(admin.uid.clone()))
                        .await
                        .is_err()
                );
                let value = preview(&node, &admin, &proof).await;
                let (key, signature) = node.revocation_certificate(&fresh);
                node.adopt_revocation(&organ, &key, &signature)
                    .await
                    .unwrap();
                assert!(
                    node.act(
                        accept(&value, "root:revoked-after-preview"),
                        Some(admin.uid.clone())
                    )
                    .await
                    .is_err()
                );
                assert!(
                    node.act(
                        Action::Identity {
                            request: Command::PreviewOrganRoot {
                                subject: organ.clone(),
                                document: serde_json::to_string(&proof).unwrap()
                            }
                        },
                        Some(admin.uid.clone())
                    )
                    .await
                    .is_err()
                );
                clock
                    .set_time(clock.now().timestamp_millis() + 86400000)
                    .unwrap();
                assert!(
                    node.act(
                        Action::Identity {
                            request: Command::PreviewOrganRoot {
                                subject: organ,
                                document: serde_json::to_string(&proof).unwrap()
                            }
                        },
                        Some(admin.uid.clone())
                    )
                    .await
                    .is_err()
                );
                let after = counts(&node).await;
                assert_eq!(after.0, baseline.0);
                assert_eq!(after.2, 0);
                assert_eq!(after.3, 0);
            })
            .await;
    });
}

#[test]
fn current_permissions_reader_identity_and_anchor_revision_gate_acceptance_and_replay() {
    support::run_async_test("root-actor-control", || async {
        let source = support::engine().await;
        let node = support::engine().await;
        let admin = support::person(&node, "root.primary-admin").await;
        let other = support::person(&node, "root.other-admin").await;
        authorize(&node, &admin).await;
        authorize(&node, &other).await;
        node.set_signer(admin.signer.clone()).await.unwrap();
        let organ = store::organs::local(&source.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let old = Signer::generate(&organ, engine::roster::ROOT_KEY_ID);
        recognize_original(&node, &organ, &old).await;
        let fresh = Signer::generate(&organ, engine::roster::ROOT_KEY_ID);
        let newer = Signer::generate(&organ, engine::roster::ROOT_KEY_ID);
        let value = preview(&node, &admin, &proof(&organ, &fresh)).await;
        let stale = preview(&node, &admin, &proof(&organ, &newer)).await;
        assert!(
            node.act(
                accept(&value, "root:foreign-person"),
                Some(other.uid.clone())
            )
            .await
            .is_err()
        );
        node.act(accept(&value, "root:primary"), Some(admin.uid.clone()))
            .await
            .unwrap();
        assert!(
            node.key_chains(&organ, &fresh.public_key_b64())
                .await
                .unwrap()
        );
        assert!(
            !node
                .key_chains(&organ, &old.public_key_b64())
                .await
                .unwrap()
        );
        assert_eq!(
            node.authenticate_profile_claim(
                &claim(&organ, &fresh),
                nucleus::execution::now().timestamp()
            )
            .await
            .unwrap(),
            IdentityAuthenticity::Accepted
        );
        assert!(
            node.act(accept(&stale, "root:stale-anchor"), Some(admin.uid.clone()))
                .await
                .is_err()
        );
        let mut changed = value.clone();
        changed["review"]["proof"]["nonce"] = json!(nucleus::new_uid("r"));
        assert!(
            node.act(accept(&changed, "root:primary"), Some(admin.uid.clone()))
                .await
                .is_err()
        );
        let fresh = preview(&node, &admin, &proof(&organ, &newer)).await;
        let role = store::auth::ensure_role(&node.store.pool, "Root recognition administrator")
            .await
            .unwrap();
        let permission = store::auth::ensure_permission(&node.store.pool, "organ", "update")
            .await
            .unwrap();
        store::sqlx::query("DELETE FROM role_permission WHERE role_id=? AND permission_id=?")
            .bind(role)
            .bind(permission)
            .execute(&node.store.pool)
            .await
            .unwrap();
        assert!(
            node.act(
                accept(&fresh, "root:permission-lost"),
                Some(admin.uid.clone())
            )
            .await
            .is_err()
        );
        assert_eq!(counts(&node).await.2, 1);
    });
}
