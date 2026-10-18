#[path = "transfer_receipts/browsing.rs"]
mod browsing;
#[path = "transfer_receipts/delivery.rs"]
mod delivery;
#[path = "transfer_receipts/fragments.rs"]
mod fragments;
#[path = "transfer_receipts/history.rs"]
mod history;
#[path = "transfer_receipts/person.rs"]
mod person;
#[path = "transfer_receipts/serving.rs"]
mod serving;
pub mod support;

use chrono::{DateTime, Duration, Utc};
use engine::actions::Action;
use engine::{Engine, roster::ROOT_KEY_ID, trust::Signer};
use nucleus::{
    DecimalValue,
    karma::{CanonicalHash, canonical_hash},
    transfer::{
        outcome::Role,
        receipt::{
            Approval, ApprovalRole, Authority, Bundle, Command, Content, Document, Event,
            Measurement, Publication, RecordedFloat, Review, Scope, SignedEvent, Source, Stage,
        },
    },
    vocabulary::reference::MeaningReference,
};

fn hash(label: &str) -> CanonicalHash {
    canonical_hash("lince.transfer.receipt.test.v1", &label).unwrap()
}

fn receipt_action(person: &str, id: &str, request: Command) -> Action {
    Action::TransferReceipt {
        person: Some(person.into()),
        request_id: id.into(),
        request,
    }
}

fn import_action(person: &str, id: &str, document: &Document) -> Action {
    receipt_action(
        person,
        id,
        Command::Import {
            document: serde_json::to_string(document).unwrap(),
        },
    )
}

#[test]
fn private_review_versions_accumulate_approvals_and_export_only_the_minimal_document() {
    support::run_async_test("persisted-receipt-reviews", || async {
        let fixture = Fixture::new(true).await;
        let person = support::person(&fixture.reader, "receipt.reader").await;
        fixture
            .reader
            .set_signer(person.signer.clone())
            .await
            .unwrap();
        let mut review = Review {
            person_approvals: Vec::new(),
            scope: fixture.bundle.publication.scope.clone(),
            approvals: Vec::new(),
            authorities: fixture.bundle.authorities.clone(),
        };
        let initial = import_action(
            &person.uid,
            "receipt:empty",
            &Document::Review(review.clone()),
        );
        let created = fixture.reader.act(initial.clone(), None).await.unwrap();
        let uid = created.created.unwrap();
        assert_eq!(created.data.unwrap()["receipt"]["state"], "private_review");
        assert!(
            fixture
                .reader
                .act(initial.clone(), None)
                .await
                .unwrap()
                .facts
                .is_empty()
        );
        for (index, approval) in fixture.bundle.publication.approvals.iter().enumerate() {
            review.approvals.push(approval.clone());
            let saved = fixture
                .reader
                .act(
                    import_action(
                        &person.uid,
                        &format!("receipt:approval:{index}"),
                        &Document::Review(review.clone()),
                    ),
                    None,
                )
                .await
                .unwrap();
            assert_eq!(saved.created.as_deref(), Some(uid.as_str()));
            assert_eq!(
                saved.data.unwrap()["receipt"]["version"],
                (index + 2) as u64
            );
        }
        let replay = fixture.reader.act(initial, None).await.unwrap();
        assert_eq!(replay.data.unwrap()["receipt"]["version"], 1);
        let exported = fixture
            .reader
            .act(
                receipt_action(
                    &person.uid,
                    "receipt:export",
                    Command::Export {
                        record: uid.clone(),
                    },
                ),
                None,
            )
            .await
            .unwrap();
        let data = exported.data.unwrap();
        assert_eq!(data["receipt"]["state"], "approved_scope");
        let bytes = data["receipt_export"].as_str().unwrap();
        assert!(!bytes.contains(&person.uid));
        assert!(!bytes.contains(&uid));
        assert!(!bytes.contains("fact"));
        assert_eq!(
            Document::parse(bytes.as_bytes(), fixture.now.timestamp()).unwrap(),
            Document::Review(review)
        );
        let version_count: i64 = store::sqlx::query_scalar(
            "SELECT COUNT(*) FROM transfer_receipt_version WHERE record_uid=?",
        )
        .bind(&uid)
        .fetch_one(&fixture.reader.store.pool)
        .await
        .unwrap();
        assert_eq!(version_count, 4);
        let policy: String = store::sqlx::query_scalar(
            "SELECT policy_json FROM data_visibility WHERE record_uid=? AND data='record'",
        )
        .bind(&uid)
        .fetch_one(&fixture.reader.store.pool)
        .await
        .unwrap();
        assert!(policy.contains(&person.uid));
        let record = store::records::get(&fixture.reader.store.pool, &uid)
            .await
            .unwrap()
            .unwrap();
        assert!(record.quantity.is_zero());
        let mut tx = store::write_tx(&fixture.reader.store.pool).await.unwrap();
        assert!(
            store::records::set_extension_on(
                &mut tx,
                &uid,
                nucleus::transfer::receipt::NAMESPACE,
                &serde_json::json!({"forged":true})
            )
            .await
            .is_err()
        );
        tx.rollback().await.unwrap();
        assert!(
            fixture
                .reader
                .act(
                    Action::SetExtension {
                        target: uid.clone(),
                        namespace: nucleus::transfer::receipt::NAMESPACE.into(),
                        fds: serde_json::json!({"forged":true})
                    },
                    None
                )
                .await
                .is_err()
        );
        assert!(
            store::records::delete_extension(
                &fixture.reader.store.pool,
                &uid,
                nucleus::transfer::receipt::NAMESPACE
            )
            .await
            .is_err()
        );
        assert!(
            store::sqlx::query(
                "UPDATE transfer_receipt_version SET document_json='{}' WHERE record_uid=?"
            )
            .bind(&uid)
            .execute(&fixture.reader.store.pool)
            .await
            .is_err()
        );
        let publication = fixture
            .reader
            .act(
                import_action(
                    &person.uid,
                    "receipt:publication",
                    &Document::Publication(fixture.bundle.clone()),
                ),
                None,
            )
            .await
            .unwrap();
        assert_ne!(publication.created.as_deref(), Some(uid.as_str()));
        assert_eq!(
            publication.data.unwrap()["receipt"]["state"],
            "verified_receipt"
        );
        assert!(
            fixture
                .reader
                .act(
                    import_action(
                        &person.uid,
                        "receipt:publication:duplicate",
                        &Document::Publication(fixture.bundle.clone())
                    ),
                    None
                )
                .await
                .unwrap()
                .facts
                .is_empty()
        );
        let canonical: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_occurrence")
            .fetch_one(&fixture.reader.store.pool)
            .await
            .unwrap();
        assert_eq!(canonical, 0);
    });
}

#[test]
fn import_reports_unknown_identity_incomplete_proof_bad_signature_and_withheld_authority() {
    support::run_async_test("receipt-import-states", || async {
        let fixture = Fixture::new(false).await;
        let person = support::person(&fixture.reader, "receipt.importer").await;
        let role = store::auth::ensure_role(&fixture.reader.store.pool, "receipt.reader-only")
            .await
            .unwrap();
        for (subject, action) in [("record", "create"), ("record", "read")] {
            let permission =
                store::auth::ensure_permission(&fixture.reader.store.pool, subject, action)
                    .await
                    .unwrap();
            store::auth::grant(&fixture.reader.store.pool, role, permission)
                .await
                .unwrap();
        }
        store::auth::create_credential(
            &fixture.reader.store.pool,
            &person.uid,
            "receipt-importer",
            "hash",
            role,
        )
        .await
        .unwrap();
        fixture
            .reader
            .set_signer(person.signer.clone())
            .await
            .unwrap();
        let action = import_action(
            &person.uid,
            "receipt:unknown",
            &Document::Publication(fixture.bundle.clone()),
        );
        assert_eq!(
            fixture
                .reader
                .act(action.clone(), Some(person.uid.clone()))
                .await
                .unwrap_err()
                .code(),
            Some("transfer_receipt_unknown_identity")
        );
        let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_receipt_record")
            .fetch_one(&fixture.reader.store.pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
        for root in [&fixture.giver_root, &fixture.receiver_root] {
            fixture.reader.publish_root_key(root).await.unwrap();
        }
        let baseline: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
            .fetch_one(&fixture.reader.store.pool)
            .await
            .unwrap();
        let denied_policy = store::role_policies::set(
            &fixture.reader.store.pool,
            role,
            &serde_json::json!({"read":{"all":[]},"grants":[]}),
            0,
        )
        .await
        .unwrap();
        assert_eq!(
            fixture
                .reader
                .act(action.clone(), Some(person.uid.clone()))
                .await
                .unwrap_err()
                .code(),
            Some("forbidden")
        );
        let unchanged: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
            .fetch_one(&fixture.reader.store.pool)
            .await
            .unwrap();
        assert_eq!(unchanged, baseline);
        let records: i64 =
            store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_receipt_record")
                .fetch_one(&fixture.reader.store.pool)
                .await
                .unwrap();
        assert_eq!(records, 0);
        let indexed: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_receipt_event")
            .fetch_one(&fixture.reader.store.pool)
            .await
            .unwrap();
        assert_eq!(indexed, 0);
        let mut properties = std::collections::BTreeSet::from([
            protein::authority::Property::Kind,
            protein::authority::Property::Head,
            protein::authority::Property::Body,
            protein::authority::Property::Quantity,
            protein::authority::Property::Organ,
        ]);
        for field in ["kind", "value"] {
            properties.insert(protein::authority::Property::Extension(
                protein::authority::ExtensionProperty {
                    namespace: nucleus::transfer::receipt::NAMESPACE.into(),
                    field: field.into(),
                },
            ));
        }
        let policy = protein::authority::RolePolicy {
            read: protein::Predicate::All(Vec::new()),
            grants: vec![protein::authority::MutationGrant {
                operation: protein::authority::Operation::Create,
                selector: protein::Predicate::All(vec![
                    protein::Predicate::KindEq("plain".into()),
                    protein::Predicate::QuantityEq(store::exact::zero()),
                ]),
                properties,
                assertions_add: Vec::new(),
                assertions_remove: Vec::new(),
            }],
        };
        store::role_policies::set(
            &fixture.reader.store.pool,
            role,
            &serde_json::to_value(policy).unwrap(),
            denied_policy.revision,
        )
        .await
        .unwrap();
        let mut incomplete = fixture.bundle.clone();
        incomplete.publication.events.remove(0);
        assert_eq!(
            fixture
                .reader
                .act(
                    import_action(
                        &person.uid,
                        "receipt:incomplete",
                        &Document::Publication(incomplete)
                    ),
                    Some(person.uid.clone())
                )
                .await
                .unwrap_err()
                .code(),
            Some("transfer_receipt_incomplete")
        );
        let mut invalid = fixture.bundle.clone();
        invalid.publication.events[2].signature =
            invalid.publication.approvals[0].signature.clone();
        assert_eq!(
            fixture
                .reader
                .act(
                    import_action(
                        &person.uid,
                        "receipt:invalid",
                        &Document::Publication(invalid)
                    ),
                    Some(person.uid.clone())
                )
                .await
                .unwrap_err()
                .code(),
            Some("transfer_receipt_signature_invalid")
        );
        let saved = fixture
            .reader
            .act(action, Some(person.uid.clone()))
            .await
            .unwrap()
            .created
            .unwrap();
        assert!(
            fixture
                .reader
                .act(
                    import_action(
                        &person.uid,
                        "receipt:reader:duplicate",
                        &Document::Publication(fixture.bundle.clone())
                    ),
                    Some(person.uid.clone())
                )
                .await
                .unwrap()
                .facts
                .is_empty()
        );
        let original = store::transfer_receipts::get(&fixture.reader.store.pool, &saved)
            .await
            .unwrap();
        let (key, signature) = fixture
            .reader
            .revocation_certificate(&fixture.receiver_root);
        fixture
            .reader
            .adopt_revocation(&fixture.receiver_root.actor_uid, &key, &signature)
            .await
            .unwrap();
        assert_eq!(
            fixture
                .reader
                .act(
                    receipt_action(
                        &person.uid,
                        "receipt:withheld",
                        Command::Inspect {
                            record: saved.clone()
                        }
                    ),
                    Some(person.uid.clone())
                )
                .await
                .unwrap_err()
                .code(),
            Some("transfer_receipt_authority_withheld")
        );
        assert_eq!(
            store::transfer_receipts::get(&fixture.reader.store.pool, &saved)
                .await
                .unwrap()
                .document,
            original.document
        );
    });
}

#[test]
fn remote_role_approval_requires_explicit_administration_and_current_device_authority() {
    support::run_async_test("receipt-role-approval", || async {
        let node = support::engine().await;
        let local = store::organs::local(&node.store.pool)
            .await
            .unwrap()
            .unwrap();
        let cell = store::cells::local(&node.store.pool)
            .await
            .unwrap()
            .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("root.key");
        let root = Signer::load_or_create(&path, &local.uid, ROOT_KEY_ID).unwrap();
        node.set_root_key_path(path);
        node.publish_root_key(&root).await.unwrap();
        let writer = Signer::generate(&local.uid, &engine::roster::cell_key_id(&cell.uid));
        node.set_organ_signer(writer.clone()).await.unwrap();
        node.publish_roster(
            &root,
            vec![engine::roster::CellEntry {
                cell_uid: cell.uid.clone(),
                node_id: iroh::SecretKey::from_bytes(&[81; 32]).public().to_string(),
                label: "Private device name".into(),
                operational_key: writer.public_key_b64(),
                sealing_key: None,
                front_door: false,
                capabilities: engine::roster::full_capabilities(),
            }],
        )
        .await
        .unwrap();
        let role = store::auth::ensure_role(&node.store.pool, "receipt.member")
            .await
            .unwrap();
        for (subject, action) in [
            ("record", "create"),
            ("record", "read"),
            ("record", "update"),
            ("transfer", "read"),
            ("transfer", "update"),
        ] {
            let permission = store::auth::ensure_permission(&node.store.pool, subject, action)
                .await
                .unwrap();
            store::auth::grant(&node.store.pool, role, permission)
                .await
                .unwrap();
        }
        let person = store::auth::create_person_login(
            &node.store.pool,
            "Receipt reviewer",
            "receipt-reviewer",
            "hash",
            role,
        )
        .await
        .unwrap();
        node.set_signer(Signer::generate(&person, "receipt:test:person"))
            .await
            .unwrap();
        let authority_record = node
            .act(
                receipt_action(
                    &person,
                    "receipt:device",
                    Command::IssueAuthority {
                        cell: cell.uid.clone(),
                    },
                ),
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let now = nucleus::execution::now();
        let origin = nucleus::new_uid("r");
        let review = Review {
            person_approvals: Vec::new(),
            scope: Scope {
            audience: nucleus::trust_analysis::contacts::Audience::Public,
            upfront_terms: None,
                public_persons: Vec::new(),
                original_authorities: Vec::new(),
                party_attribution:
                    nucleus::transfer::receipt::PartyAttribution::LaterOrganEndorsement,
                source: Source {
                    origin: origin.clone(),
                    occurrence: Source::occurrence_id(&origin, &nucleus::new_uid("toc")).unwrap(),
                    activated_at: now.timestamp() - 30,
                    agreement_revision: 1,
                    agreement_commitment: hash("agreement"),
                    activation_commitment: hash("activation"),
                },
                giver: origin,
                receiver: local.uid.clone(),
                measurements: Vec::new(),
                stages: vec![Stage::Settled],
                agreed_quantity: None,
                canonical_unit: None,
                issued_at: now.timestamp(),
                expires_at: now.timestamp() + 3600,
            },
            approvals: Vec::new(),
            authorities: Vec::new(),
        };
        let record = node
            .act(
                import_action(&person, "receipt:remote-review", &Document::Review(review)),
                Some(person.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let approve = receipt_action(
            &person,
            "receipt:approve",
            Command::Approve {
                private_transfer: None,
                record: record.clone(),
                expected_version: 1,
                role: ApprovalRole::Receiver,
                authority_record,
            },
        );
        assert_eq!(
            node.act(approve.clone(), Some(person.clone()))
                .await
                .unwrap_err()
                .code(),
            Some("forbidden")
        );
        let permission = store::auth::ensure_permission(&node.store.pool, "organ", "update")
            .await
            .unwrap();
        store::auth::grant(&node.store.pool, role, permission)
            .await
            .unwrap();
        let create_permission =
            store::auth::ensure_permission(&node.store.pool, "record", "create")
                .await
                .unwrap();
        store::auth::revoke(&node.store.pool, role, create_permission)
            .await
            .unwrap();
        let approved = node
            .act(approve.clone(), Some(person.clone()))
            .await
            .unwrap();
        assert_eq!(approved.data.unwrap()["receipt"]["version"], 2);
        assert!(
            node.act(approve, Some(person.clone()))
                .await
                .unwrap()
                .facts
                .is_empty()
        );
        let document = store::transfer_receipts::get(&node.store.pool, &record)
            .await
            .unwrap()
            .document;
        let Document::Review(review) = document else {
            panic!("Review expected")
        };
        assert_eq!(review.approvals.len(), 1);
        assert_eq!(review.approvals[0].role, ApprovalRole::Receiver);
        assert_eq!(review.approvals[0].key_id, writer.key_id);
        assert!(
            node.verify_transfer_receipt(&review.as_bundle(), now)
                .await
                .is_err()
        );
        assert!(
            !serde_json::to_string(&review)
                .unwrap()
                .contains("Private device name")
        );
        node.verify_transfer_receipt_review(&review, nucleus::execution::now())
            .await
            .unwrap();
    });
}

fn authority(root: &Signer, writer: &Signer, now: DateTime<Utc>) -> Authority {
    let mut authority = Authority {
        organ: root.actor_uid.clone(),
        roster_version: 1,
        root_key: root.public_key_b64(),
        signing_key: writer.public_key_b64(),
        key_id: writer.key_id.clone(),
        issued_at: now.timestamp() - 60,
        expires_at: now.timestamp() + 3600,
        successions: Vec::new(),
        participation: None,
        signature: String::new(),
    };
    authority.signature = root.sign_bytes(&authority.signing_bytes().unwrap());
    authority
}

fn sign_event(event: Event, signer: &Signer) -> SignedEvent {
    let mut signed = SignedEvent {
        event,
        key_id: signer.key_id.clone(),
        signature: String::new(),
    };
    signed.signature = signer.sign_bytes(&signed.signing_bytes().unwrap());
    signed
}

struct Fixture {
    reader: Engine,
    bundle: Bundle,
    giver_root: Signer,
    receiver_root: Signer,
    giver_writer: Signer,
    receiver_writer: Signer,
    now: DateTime<Utc>,
}

impl Fixture {
    async fn new(recognize: bool) -> Self {
        let giver = support::engine().await;
        let receiver = support::engine().await;
        let reader = support::engine().await;
        let now = nucleus::execution::now();
        let giver_organ = store::organs::local(&giver.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let receiver_organ = store::organs::local(&receiver.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let giver_root = Signer::generate(&giver_organ, ROOT_KEY_ID);
        let receiver_root = Signer::generate(&receiver_organ, ROOT_KEY_ID);
        let giver_writer = Signer::generate(
            &giver_organ,
            &engine::roster::cell_key_id(&nucleus::new_uid("r")),
        );
        let receiver_writer = Signer::generate(
            &receiver_organ,
            &engine::roster::cell_key_id(&nucleus::new_uid("r")),
        );
        for node in [&giver, &receiver] {
            for root in [&giver_root, &receiver_root] {
                node.publish_root_key(root).await.unwrap();
            }
        }
        if recognize {
            for root in [&giver_root, &receiver_root] {
                reader.publish_root_key(root).await.unwrap();
            }
        }
        let measurement = Measurement {
            meaning: MeaningReference {
                vocabulary_uid: nucleus::new_uid("g"),
                concept_uid: nucleus::new_uid("c"),
                revision: 1,
                definition_hash: "a".repeat(64),
            },
            unit: Some(MeaningReference {
                vocabulary_uid: nucleus::new_uid("g"),
                concept_uid: nucleus::new_uid("c"),
                revision: 1,
                definition_hash: "b".repeat(64),
            }),
            object_commitment: None,
        };
        let scope = Scope {
            audience: nucleus::trust_analysis::contacts::Audience::Public,
            upfront_terms: None,
            public_persons: Vec::new(),
            original_authorities: Vec::new(),
            party_attribution: nucleus::transfer::receipt::PartyAttribution::LaterOrganEndorsement,
            source: Source {
                origin: giver_organ.clone(),
                occurrence: Source::occurrence_id(&giver_organ, &nucleus::new_uid("toc")).unwrap(),
                activated_at: now.timestamp() - 40,
                agreement_revision: 1,
                agreement_commitment: hash("private-agreement"),
                activation_commitment: hash("private-activation"),
            },
            giver: giver_organ,
            receiver: receiver_organ,
            measurements: vec![measurement.clone()],
            stages: vec![
                Stage::Need,
                Stage::Giver,
                Stage::Receiver,
                Stage::Settled,
                Stage::Dispute,
            ],
            agreed_quantity: Some(RecordedFloat::new(10.0).unwrap()),
            canonical_unit: measurement.unit.clone(),
            issued_at: now.timestamp() - 20,
            expires_at: now.timestamp() + 86400,
        };
        let approvals = [
            (ApprovalRole::Origin, &giver_writer),
            (ApprovalRole::Giver, &giver_writer),
            (ApprovalRole::Receiver, &receiver_writer),
        ]
        .into_iter()
        .map(|(role, signer)| {
            let mut approval = Approval {
                role,
                issued_at: scope.issued_at,
                key_id: signer.key_id.clone(),
                signature: String::new(),
            };
            approval.signature = signer.sign_bytes(&approval.signing_bytes(&scope).unwrap());
            approval
        })
        .collect();
        let scope_id = scope.id().unwrap();
        let event = |content, label| Event {
            scope: scope_id.clone(),
            source_commitment: hash(label),
            recorded_at: now.timestamp() - 30,
            issued_at: now.timestamp() - 10,
            supersedes: None,
            content,
        };
        let need = sign_event(
            event(
                Content::Need {
                    measurement: measurement.clone(),
                    requested: DecimalValue::parse_inferred("10").unwrap(),
                    need_commitment: hash("receiver-private-need"),
                },
                "need-source",
            ),
            &receiver_writer,
        );
        let need_id = need.event.id().unwrap();
        let reported = |role, amount| Content::Statement {
            role,
            measurement: measurement.clone(),
            quantity: DecimalValue::parse_inferred(amount).unwrap(),
            need: Some(need_id.clone()),
            withdrawn: false,
        };
        let events = vec![
            need,
            sign_event(
                event(reported(Role::Giver, "100"), "giver-source"),
                &giver_writer,
            ),
            sign_event(
                event(reported(Role::Receiver, "12"), "receiver-source"),
                &receiver_writer,
            ),
            sign_event(
                event(
                    Content::Settled {
                        quantity: RecordedFloat::new(10.0).unwrap(),
                    },
                    "settlement-source",
                ),
                &giver_writer,
            ),
        ];
        let bundle = Bundle {
            publication: Publication {
                person_approvals: Vec::new(),
                scope,
                approvals,
                events,
            },
            authorities: vec![
                authority(&giver_root, &giver_writer, now),
                authority(&receiver_root, &receiver_writer, now),
            ],
        };
        giver.verify_transfer_receipt(&bundle, now).await.unwrap();
        receiver
            .verify_transfer_receipt(&bundle, now)
            .await
            .unwrap();
        Self {
            reader,
            bundle,
            giver_root,
            receiver_root,
            giver_writer,
            receiver_writer,
            now,
        }
    }
}

#[test]
fn three_nodes_verify_exact_separate_outcomes_without_running_actions_or_adopting_writer_keys() {
    support::run_async_test("portable-receipt-three-nodes", || async {
        let fixture = Fixture::new(true).await;
        let stock = support::plain(&fixture.reader, "receipt.stock", 20.0).await;
        let before: (i64, i64) = store::sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM record), (SELECT COUNT(*) FROM fact)",
        )
        .fetch_one(&fixture.reader.store.pool)
        .await
        .unwrap();
        let bytes = serde_json::to_vec(&fixture.bundle).unwrap();
        let parsed = Bundle::parse(&bytes, fixture.now.timestamp()).unwrap();
        let verified = fixture
            .reader
            .verify_transfer_receipt(&parsed, fixture.now)
            .await
            .unwrap();
        assert_eq!(verified.heads().len(), 4);
        assert_eq!(verified.bundle(), &fixture.bundle);
        let after: (i64, i64) = store::sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM record), (SELECT COUNT(*) FROM fact)",
        )
        .fetch_one(&fixture.reader.store.pool)
        .await
        .unwrap();
        assert_eq!(before, after);
        let quantity = store::records::get(&fixture.reader.store.pool, &stock)
            .await
            .unwrap()
            .unwrap()
            .quantity;
        assert_eq!(quantity.canonical(), "20");
        assert!(
            engine::trust::key_of(
                &fixture.reader.store,
                &fixture.receiver_writer.actor_uid,
                &fixture.receiver_writer.key_id
            )
            .await
            .unwrap()
            .is_none()
        );
        let public = String::from_utf8(bytes).unwrap();
        assert!(!public.contains("receiver-private-need"));
        assert!(!public.contains("person_uid"));
        assert!(!public.contains("private-agreement"));
    });
}

#[test]
fn embedded_authority_is_not_an_identity_anchor_and_import_cannot_execute_an_action() {
    support::run_async_test("portable-receipt-unknown-identity", || async {
        let fixture = Fixture::new(false).await;
        assert!(
            fixture
                .reader
                .verify_transfer_receipt(&fixture.bundle, fixture.now)
                .await
                .is_err()
        );
        for signer in [
            &fixture.giver_root,
            &fixture.receiver_root,
            &fixture.giver_writer,
            &fixture.receiver_writer,
        ] {
            assert!(
                engine::trust::key_of(&fixture.reader.store, &signer.actor_uid, &signer.key_id)
                    .await
                    .unwrap()
                    .is_none()
            );
        }
        let mut bytes = serde_json::to_value(&fixture.bundle).unwrap();
        bytes["action"] = serde_json::json!({"action": "create_record", "quantity": 1000000});
        assert!(
            Bundle::parse(
                &serde_json::to_vec(&bytes).unwrap(),
                fixture.now.timestamp()
            )
            .is_err()
        );
    });
}

#[test]
fn changed_scope_wrong_role_and_invalid_signatures_are_rejected() {
    support::run_async_test("portable-receipt-forgery", || async {
        let fixture = Fixture::new(true).await;
        let mut tampered = fixture.bundle.clone();
        tampered.publication.scope.expires_at += 1;
        assert!(
            fixture
                .reader
                .verify_transfer_receipt(&tampered, fixture.now)
                .await
                .is_err()
        );
        let mut tampered = fixture.bundle.clone();
        tampered.publication.events[2] = sign_event(
            tampered.publication.events[2].event.clone(),
            &fixture.giver_writer,
        );
        assert!(
            fixture
                .reader
                .verify_transfer_receipt(&tampered, fixture.now)
                .await
                .is_err()
        );
        let mut tampered = fixture.bundle.clone();
        tampered.publication.approvals[2].signature =
            tampered.publication.approvals[0].signature.clone();
        assert!(
            fixture
                .reader
                .verify_transfer_receipt(&tampered, fixture.now)
                .await
                .is_err()
        );
        let mut tampered = fixture.bundle.clone();
        tampered.authorities[1].signing_key = fixture.giver_writer.public_key_b64();
        assert!(
            fixture
                .reader
                .verify_transfer_receipt(&tampered, fixture.now)
                .await
                .is_err()
        );
    });
}

#[test]
fn valid_history_outlives_a_certificate_but_revocation_and_out_of_term_signatures_fail() {
    support::run_async_test("portable-receipt-historical-authority", || async {
        let fixture = Fixture::new(true).await;
        fixture
            .reader
            .verify_transfer_receipt(&fixture.bundle, fixture.now + Duration::hours(2))
            .await
            .unwrap();
        let mut late = fixture.bundle.clone();
        let mut event = late.publication.events[2].event.clone();
        event.issued_at = fixture.now.timestamp() + 7200;
        late.publication.events[2] = sign_event(event, &fixture.receiver_writer);
        assert!(
            fixture
                .reader
                .verify_transfer_receipt(&late, fixture.now + Duration::hours(2))
                .await
                .is_err()
        );
        let (key, signature) = fixture
            .reader
            .revocation_certificate(&fixture.receiver_root);
        assert!(
            fixture
                .reader
                .adopt_revocation(&fixture.receiver_root.actor_uid, &key, &signature)
                .await
                .unwrap()
        );
        assert!(
            fixture
                .reader
                .verify_transfer_receipt(&fixture.bundle, fixture.now)
                .await
                .is_err()
        );
    });
}

#[test]
fn competing_corrections_are_retained_without_selecting_or_adding_them() {
    support::run_async_test("portable-receipt-competing-heads", || async {
        let mut fixture = Fixture::new(true).await;
        let original = fixture.bundle.publication.events[2].event.clone();
        let parent = original.id().unwrap();
        for amount in ["11", "13"] {
            let mut correction = original.clone();
            correction.supersedes = Some(parent.clone());
            correction.source_commitment = hash(&format!("correction:{amount}"));
            correction.issued_at += 1;
            if let Content::Statement { quantity, .. } = &mut correction.content {
                *quantity = DecimalValue::parse_inferred(amount).unwrap();
            }
            fixture
                .bundle
                .publication
                .events
                .push(sign_event(correction, &fixture.receiver_writer));
        }
        let verified = fixture
            .reader
            .verify_transfer_receipt(&fixture.bundle, fixture.now)
            .await
            .unwrap();
        assert_eq!(verified.heads().len(), 4);
        assert_eq!(
            verified
                .heads()
                .get(&original.slot(&fixture.bundle.publication.scope).unwrap())
                .unwrap()
                .len(),
            2
        );
        assert!(!verified.heads().values().flatten().any(|id| id == &parent));
    });
}

#[test]
fn anchored_root_succession_is_checked_without_pinning_new_keys_from_the_receipt() {
    support::run_async_test("portable-receipt-root-succession", || async {
        let mut fixture = Fixture::new(true).await;
        let new_root = Signer::generate(&fixture.receiver_root.actor_uid, ROOT_KEY_ID);
        let created_at = (fixture.now - Duration::minutes(2)).to_rfc3339();
        let change = nucleus::social::RootSuccession {
            old_key: fixture.receiver_root.public_key_b64(),
            new_key: new_root.public_key_b64(),
            signature: fixture.receiver_root.sign_bytes(
                &engine::roster::succession_signing_payload(
                    &fixture.receiver_root.actor_uid,
                    &fixture.receiver_root.public_key_b64(),
                    &new_root.public_key_b64(),
                    &created_at,
                ),
            ),
            created_at,
        };
        fixture.bundle.authorities[1].root_key = new_root.public_key_b64();
        fixture.bundle.authorities[1].successions = vec![change.clone()];
        fixture.bundle.authorities[1].signature =
            new_root.sign_bytes(&fixture.bundle.authorities[1].signing_bytes().unwrap());
        fixture
            .reader
            .verify_transfer_receipt(&fixture.bundle, fixture.now)
            .await
            .unwrap();
        assert_eq!(
            engine::trust::key_of(
                &fixture.reader.store,
                &fixture.receiver_root.actor_uid,
                ROOT_KEY_ID
            )
            .await
            .unwrap(),
            Some(fixture.receiver_root.public_key_b64())
        );
        fixture.bundle.authorities[1].successions[0].signature = "a".repeat(88);
        fixture.bundle.authorities[1].signature =
            new_root.sign_bytes(&fixture.bundle.authorities[1].signing_bytes().unwrap());
        assert!(
            fixture
                .reader
                .verify_transfer_receipt(&fixture.bundle, fixture.now)
                .await
                .is_err()
        );
        assert!(
            fixture
                .reader
                .adopt_succession(
                    &fixture.receiver_root.actor_uid,
                    &change.old_key,
                    &change.new_key,
                    &change.created_at,
                    &change.signature
                )
                .await
                .unwrap()
        );
        fixture.bundle.authorities[1].successions.clear();
        fixture.bundle.authorities[1].signature =
            new_root.sign_bytes(&fixture.bundle.authorities[1].signing_bytes().unwrap());
        fixture
            .reader
            .verify_transfer_receipt(&fixture.bundle, fixture.now)
            .await
            .unwrap();
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
        assert!(
            fixture
                .reader
                .key_chains(&fixture.receiver_root.actor_uid, &new_root.public_key_b64())
                .await
                .unwrap()
        );
        assert!(
            fixture
                .reader
                .verify_transfer_receipt(&fixture.bundle, fixture.now)
                .await
                .is_err()
        );
    });
}

#[test]
fn even_root_signed_conflicting_key_bindings_and_unrelated_certificates_are_rejected() {
    support::run_async_test("portable-receipt-authority-scope", || async {
        let mut fixture = Fixture::new(true).await;
        let replacement = Signer::generate(
            &fixture.receiver_writer.actor_uid,
            &fixture.receiver_writer.key_id,
        );
        fixture.bundle.authorities.push(authority(
            &fixture.receiver_root,
            &replacement,
            fixture.now,
        ));
        assert!(
            fixture
                .reader
                .verify_transfer_receipt(&fixture.bundle, fixture.now)
                .await
                .is_err()
        );
        fixture.bundle.authorities.pop();
        let outsider = Signer::generate(&nucleus::new_uid("r"), ROOT_KEY_ID);
        let writer = Signer::generate(
            &outsider.actor_uid,
            &engine::roster::cell_key_id(&nucleus::new_uid("r")),
        );
        fixture.reader.publish_root_key(&outsider).await.unwrap();
        fixture
            .bundle
            .authorities
            .push(authority(&outsider, &writer, fixture.now));
        assert!(
            fixture
                .reader
                .verify_transfer_receipt(&fixture.bundle, fixture.now)
                .await
                .is_err()
        );
    });
}

#[test]
fn owner_issues_only_a_scoped_certificate_for_a_current_representative_device() {
    support::run_async_test("portable-receipt-certificate-issuance", || async {
        let node = support::engine().await;
        let organ = store::organs::local(&node.store.pool)
            .await
            .unwrap()
            .unwrap();
        let cell = store::cells::local(&node.store.pool)
            .await
            .unwrap()
            .unwrap();
        let root = Signer::generate(&organ.uid, ROOT_KEY_ID);
        let writer = Signer::generate(&organ.uid, &engine::roster::cell_key_id(&cell.uid));
        node.publish_root_key(&root).await.unwrap();
        let mut entry = engine::roster::CellEntry {
            cell_uid: cell.uid.clone(),
            node_id: iroh::SecretKey::from_bytes(&[71; 32]).public().to_string(),
            label: "Private device label".into(),
            operational_key: writer.public_key_b64(),
            sealing_key: None,
            front_door: false,
            capabilities: engine::roster::full_capabilities(),
        };
        let roster = node
            .publish_roster(&root, vec![entry.clone()])
            .await
            .unwrap();
        let now = nucleus::execution::now();
        let authority = node
            .create_transfer_receipt_authority(&root, &cell.uid, now)
            .await
            .unwrap();
        assert_eq!(authority.signing_key, writer.public_key_b64());
        assert_eq!(authority.key_id, writer.key_id);
        assert!(
            authority.expires_at
                <= DateTime::parse_from_rfc3339(&roster.roster.not_after)
                    .unwrap()
                    .timestamp()
        );
        assert!(
            authority.expires_at - authority.issued_at
                <= nucleus::transfer::receipt::MAX_AUTHORITY_SECONDS
        );
        assert!(
            !serde_json::to_string(&authority)
                .unwrap()
                .contains("Private device label")
        );
        let impostor = Signer::generate(&organ.uid, ROOT_KEY_ID);
        assert!(
            node.create_transfer_receipt_authority(&impostor, &cell.uid, now)
                .await
                .is_err()
        );
        entry.capabilities = engine::roster::relay_capabilities();
        node.publish_roster(&root, vec![entry]).await.unwrap();
        assert!(
            node.create_transfer_receipt_authority(&root, &cell.uid, now)
                .await
                .is_err()
        );
        assert!(
            node.create_transfer_receipt_authority(&root, &cell.uid, now + Duration::days(31))
                .await
                .is_err()
        );
    });
}

#[test]
fn known_device_removal_withholds_a_still_unexpired_receipt_certificate() {
    support::run_async_test("portable-receipt-device-removal", || async {
        let fixture = Fixture::new(true).await;
        fixture
            .reader
            .verify_transfer_receipt(&fixture.bundle, fixture.now)
            .await
            .unwrap();
        let roster = engine::roster::Roster {
            organ_uid: fixture.receiver_root.actor_uid.clone(),
            root_key: fixture.receiver_root.public_key_b64(),
            version: 2,
            not_after: (fixture.now + Duration::days(30)).to_rfc3339(),
            cells: Vec::new(),
            pickup: Vec::new(),
        };
        let signed = engine::roster::SignedRoster {
            signature: fixture
                .receiver_root
                .sign_bytes(&engine::roster::roster_signing_payload(&roster).unwrap()),
            roster,
        };
        assert_eq!(
            fixture.reader.adopt_roster(&signed).await.unwrap(),
            engine::roster::RosterOutcome::Accepted,
        );
        assert!(
            fixture
                .reader
                .verify_transfer_receipt(&fixture.bundle, fixture.now)
                .await
                .is_err()
        );
    });
}
