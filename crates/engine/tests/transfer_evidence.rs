#[path = "transfer_evidence/ordinary_delivery.rs"]
mod ordinary_delivery;
#[path = "transfer_evidence/published_events.rs"]
mod published_events;
#[path = "transfer_evidence/remote_sources.rs"]
mod remote_sources;
#[path = "transfer_evidence/analysis.rs"]
mod analysis;
#[path = "transfer_evidence/public_history.rs"]
mod public_history;
pub mod support;

use base64::Engine as _;
use engine::{Engine, actions::Action};
use nucleus::transfer::outcome::{Content, Measure, NeedSelection, Request, Role};
use protein::{Protein, Source};
use serde_json::{Value, json};
use support::{DraftOptions, Person, TransferFixture};

struct Fixture {
    engine: Engine,
    giver: Person,
    receiver: Person,
    stranger: Person,
    transfer: TransferFixture,
    occurrence: String,
    stock: String,
    selection: NeedSelection,
    measure: Measure,
    authority_records: Vec<String>,
    authorizer: Option<Authorizer>,
}

struct Authorizer {
    root: engine::trust::Signer,
    writer: engine::trust::Signer,
    cell: String,
    _directory: tempfile::TempDir,
}

impl Authorizer {
    async fn new(engine: &Engine, seed: u8) -> Self {
        let organ = store::organs::local(&engine.store.pool)
            .await
            .unwrap()
            .unwrap();
        let cell = store::cells::local(&engine.store.pool)
            .await
            .unwrap()
            .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("root-key");
        let root =
            engine::trust::Signer::load_or_create(&path, &organ.uid, engine::roster::ROOT_KEY_ID)
                .unwrap();
        engine.set_root_key_path(path);
        let writer =
            engine::trust::Signer::generate(&organ.uid, &engine::roster::cell_key_id(&cell.uid));
        engine.publish_root_key(&root).await.unwrap();
        engine.set_organ_signer(writer.clone()).await.unwrap();
        engine
            .publish_roster(
                &root,
                vec![engine::roster::CellEntry {
                    cell_uid: cell.uid.clone(),
                    node_id: iroh::SecretKey::from_bytes(&[seed; 32])
                        .public()
                        .to_string(),
                    label: "Private original-authority device".into(),
                    operational_key: writer.public_key_b64(),
                    sealing_key: None,
                    front_door: false,
                    capabilities: engine::roster::full_capabilities(),
                }],
            )
            .await
            .unwrap();
        Self {
            root,
            writer,
            cell: cell.uid,
            _directory: directory,
        }
    }
}

#[test]
fn local_parties_approve_their_own_role_without_reading_each_others_private_need() {
    support::run_async_test("receipt-local-party-approvals", || async {
        use nucleus::transfer::receipt::{ApprovalRole, Command, Document, ScopeRequest, Stage};
        let fixture = Fixture::new().await;
        let binding = fixture.bind("receipt:local:need").await;
        let mut policy_tx = store::write_tx(&fixture.engine.store.pool).await.unwrap();
        store::data_visibility::ensure_private_for_person_on(
            &mut policy_tx,
            &fixture.selection.need,
            nucleus::visibility::Data::Record,
            Some(&fixture.receiver.uid),
        )
        .await
        .unwrap();
        policy_tx.commit().await.unwrap();
        assert!(
            !fixture
                .engine
                .may_read_record(Some(&fixture.giver.uid), &fixture.selection.need)
                .await
                .unwrap()
        );
        let organ = store::organs::local(&fixture.engine.store.pool)
            .await
            .unwrap()
            .unwrap();
        let cell = store::cells::local(&fixture.engine.store.pool)
            .await
            .unwrap()
            .unwrap();
        let root = engine::trust::Signer::generate(&organ.uid, engine::roster::ROOT_KEY_ID);
        let writer =
            engine::trust::Signer::generate(&organ.uid, &engine::roster::cell_key_id(&cell.uid));
        fixture.engine.publish_root_key(&root).await.unwrap();
        fixture
            .engine
            .set_organ_signer(writer.clone())
            .await
            .unwrap();
        fixture
            .engine
            .publish_roster(
                &root,
                vec![engine::roster::CellEntry {
                    cell_uid: cell.uid.clone(),
                    node_id: iroh::SecretKey::from_bytes(&[82; 32]).public().to_string(),
                    label: "Private local device".into(),
                    operational_key: writer.public_key_b64(),
                    sealing_key: None,
                    front_door: false,
                    capabilities: engine::roster::full_capabilities(),
                }],
            )
            .await
            .unwrap();
        let authority = Document::Authority(
            fixture
                .engine
                .create_transfer_receipt_authority(&root, &cell.uid, nucleus::execution::now())
                .await
                .unwrap(),
        );
        let action = |person: &str, id: &str, request: Command| Action::TransferReceipt {
            person: Some(person.into()),
            request_id: id.into(),
            request,
        };
        let imported = |person: &str, id: &str, document: &Document| {
            action(
                person,
                id,
                Command::Import {
                    document: serde_json::to_string(document).unwrap(),
                },
            )
        };
        let receiver_authority = fixture
            .engine
            .act(
                imported(
                    &fixture.receiver.uid,
                    "receipt:local:receiver-key",
                    &authority,
                ),
                Some(fixture.receiver.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let record = fixture
            .engine
            .act(
                action(
                    &fixture.receiver.uid,
                    "receipt:local:review",
                    Command::Prepare {
                        scope: ScopeRequest {
                            use_upfront_terms: true,
                            audience: nucleus::trust_analysis::contacts::Audience::Public,
                            public_person_roles: Vec::new(),
                            public_measurements: Vec::new(),
                            occurrence: fixture.occurrence.clone(),
                            expected_revision: fixture.transfer.revision,
                            outcome_records: vec![binding],
                            stages: vec![Stage::Need, Stage::Receiver],
                            publish_agreed_quantity: false,
                            expires_at: nucleus::execution::now().timestamp() + 86400,
                        },
                    },
                ),
                Some(fixture.receiver.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let approval = |person: &str, id: &str, record: &str, role, certificate: &str| {
            action(
                person,
                id,
                Command::Approve {
                    private_transfer: None,
                    record: record.into(),
                    expected_version: 1,
                    role,
                    authority_record: certificate.into(),
                },
            )
        };
        assert_eq!(
            fixture
                .engine
                .act(
                    approval(
                        &fixture.receiver.uid,
                        "receipt:local:wrong-role",
                        &record,
                        ApprovalRole::Giver,
                        &receiver_authority
                    ),
                    Some(fixture.receiver.uid.clone())
                )
                .await
                .unwrap_err()
                .code(),
            Some("forbidden")
        );
        let receiver_approval = approval(
            &fixture.receiver.uid,
            "receipt:local:receiver-approval",
            &record,
            ApprovalRole::Receiver,
            &receiver_authority,
        );
        fixture
            .engine
            .act(
                receiver_approval.clone(),
                Some(fixture.receiver.uid.clone()),
            )
            .await
            .unwrap();
        assert!(
            fixture
                .engine
                .act(receiver_approval, Some(fixture.receiver.uid.clone()))
                .await
                .unwrap()
                .facts
                .is_empty()
        );
        let reviewed = store::transfer_receipts::get(&fixture.engine.store.pool, &record)
            .await
            .unwrap()
            .document;
        fixture
            .engine
            .set_signer(fixture.giver.signer.clone())
            .await
            .unwrap();
        assert!(
            fixture
                .engine
                .act(
                    action(
                        &fixture.giver.uid,
                        "receipt:local:cannot-read-private-review",
                        Command::Inspect {
                            record: record.clone()
                        }
                    ),
                    Some(fixture.giver.uid.clone())
                )
                .await
                .is_err()
        );
        let giver_authority = fixture
            .engine
            .act(
                imported(&fixture.giver.uid, "receipt:local:giver-key", &authority),
                Some(fixture.giver.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let giver_review = fixture
            .engine
            .act(
                imported(&fixture.giver.uid, "receipt:local:giver-review", &reviewed),
                Some(fixture.giver.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        fixture
            .engine
            .act(
                approval(
                    &fixture.giver.uid,
                    "receipt:local:giver-approval",
                    &giver_review,
                    ApprovalRole::Giver,
                    &giver_authority,
                ),
                Some(fixture.giver.uid.clone()),
            )
            .await
            .unwrap();
        let Document::Review(review) =
            store::transfer_receipts::get(&fixture.engine.store.pool, &giver_review)
                .await
                .unwrap()
                .document
        else {
            panic!("Review expected")
        };
        assert_eq!(review.approvals.len(), 2);
        assert!(
            review
                .approvals
                .iter()
                .any(|approval| approval.role == ApprovalRole::Receiver)
        );
        assert!(
            review
                .approvals
                .iter()
                .any(|approval| approval.role == ApprovalRole::Giver)
        );
        fixture
            .engine
            .verify_transfer_receipt_review(&review, nucleus::execution::now())
            .await
            .unwrap();
        assert!(
            fixture
                .engine
                .verify_transfer_receipt(&review.as_bundle(), nucleus::execution::now())
                .await
                .is_err()
        );
    });
}

#[test]
fn persisted_publication_review_keeps_its_original_mapping_private_and_is_idempotent() {
    support::run_async_test("receipt-private-source-mapping", || async {
        let fixture = Fixture::new().await;
        let binding = fixture.bind("receipt:persist:need").await;
        let action = Action::TransferReceipt {
            person: Some(fixture.receiver.uid.clone()),
            request_id: "receipt:persist:review".into(),
            request: nucleus::transfer::receipt::Command::Prepare {
                scope: nucleus::transfer::receipt::ScopeRequest {
                            use_upfront_terms: true,
                            audience: nucleus::trust_analysis::contacts::Audience::Public,
                    public_person_roles: Vec::new(),
                    public_measurements: Vec::new(),
                    occurrence: fixture.occurrence.clone(),
                    expected_revision: fixture.transfer.revision,
                    outcome_records: vec![binding.clone()],
                    stages: vec![
                        nucleus::transfer::receipt::Stage::Need,
                        nucleus::transfer::receipt::Stage::Receiver,
                    ],
                    publish_agreed_quantity: false,
                    expires_at: nucleus::execution::now().timestamp() + 86400,
                },
            },
        };
        let created = fixture
            .engine
            .act(action.clone(), Some(fixture.receiver.uid.clone()))
            .await
            .unwrap();
        let uid = created.created.unwrap();
        assert_eq!(created.data.unwrap()["receipt"]["state"], "private_review");
        assert!(
            fixture
                .engine
                .act(action, Some(fixture.receiver.uid.clone()))
                .await
                .unwrap()
                .facts
                .is_empty()
        );
        let mapping = store::transfer_receipts::local_source(&fixture.engine.store.pool, &uid)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(mapping.occurrence, fixture.occurrence);
        let export = fixture
            .engine
            .act(
                Action::TransferReceipt {
                    person: Some(fixture.receiver.uid.clone()),
                    request_id: "receipt:persist:export".into(),
                    request: nucleus::transfer::receipt::Command::Export {
                        record: uid.clone(),
                    },
                },
                Some(fixture.receiver.uid.clone()),
            )
            .await
            .unwrap()
            .data
            .unwrap();
        let public = export["receipt_export"].as_str().unwrap();
        for private in [
            &fixture.giver.uid,
            &fixture.receiver.uid,
            &fixture.selection.need,
            &fixture.stock,
            &fixture.transfer.transfer,
            &fixture.occurrence,
            &binding,
        ] {
            assert!(!public.contains(private));
        }
        assert!(
            fixture
                .engine
                .act(
                    Action::TransferReceipt {
                        person: Some(fixture.stranger.uid.clone()),
                        request_id: "receipt:persist:hidden".into(),
                        request: nucleus::transfer::receipt::Command::Inspect { record: uid }
                    },
                    Some(fixture.stranger.uid.clone())
                )
                .await
                .is_err()
        );
    });
}

#[test]
fn receipt_review_checks_original_source_and_omits_private_need_person_and_stock_ids() {
    support::run_async_test("receipt-original-source-review", || async {
        let fixture = Fixture::new().await;
        let binding = fixture.bind("receipt:review:need").await;
        let now = nucleus::execution::now();
        let request = nucleus::transfer::receipt::ScopeRequest {
                            use_upfront_terms: true,
                            audience: nucleus::trust_analysis::contacts::Audience::Public,
            public_person_roles: Vec::new(),
            public_measurements: Vec::new(),
            occurrence: fixture.occurrence.clone(),
            expected_revision: fixture.transfer.revision,
            outcome_records: vec![binding.clone()],
            stages: vec![
                nucleus::transfer::receipt::Stage::Need,
                nucleus::transfer::receipt::Stage::Receiver,
            ],
            publish_agreed_quantity: true,
            expires_at: now.timestamp() + 86400,
        };
        let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
            .fetch_one(&fixture.engine.store.pool)
            .await
            .unwrap();
        let scope = fixture
            .engine
            .prepare_transfer_receipt_scope(request.clone(), None, now)
            .await
            .unwrap();
        assert_eq!(scope.measurements.len(), 1);
        assert_eq!(scope.measurements[0].meaning, fixture.measure.meaning);
        assert_eq!(scope.measurements[0].unit, fixture.measure.unit);
        assert_eq!(
            scope.agreed_quantity.as_ref().unwrap().value().unwrap(),
            10.0
        );
        assert!(scope.canonical_unit.is_none());
        let public = serde_json::to_string(&scope).unwrap();
        for private in [
            &fixture.selection.need,
            &fixture.giver.uid,
            &fixture.receiver.uid,
            &fixture.stock,
            &fixture.transfer.transfer,
            &fixture.occurrence,
            &binding,
        ] {
            assert!(!public.contains(private));
        }
        let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
            .fetch_one(&fixture.engine.store.pool)
            .await
            .unwrap();
        assert_eq!(before, after);
        let mut stale = request.clone();
        stale.expected_revision += 1;
        assert!(
            fixture
                .engine
                .prepare_transfer_receipt_scope(stale, None, now)
                .await
                .is_err()
        );
        assert!(
            fixture
                .engine
                .prepare_transfer_receipt_scope(request.clone(), Some(&fixture.stranger.uid), now)
                .await
                .is_err()
        );
        store::sqlx::query("UPDATE transfer_occurrence SET quantity=11 WHERE uid=?")
            .bind(&fixture.occurrence)
            .execute(&fixture.engine.store.pool)
            .await
            .unwrap();
        assert!(
            fixture
                .engine
                .prepare_transfer_receipt_scope(request, None, now)
                .await
                .is_err()
        );
    });
}

#[test]
fn receipt_review_withholds_an_outcome_whose_original_signing_key_was_revoked() {
    support::run_async_test("receipt-revoked-source-author", || async {
        let fixture = Fixture::new().await;
        let binding = fixture.bind("receipt:review:revoked").await;
        let original = store::transfer_evidence::get(&fixture.engine.store.pool, &binding)
            .await
            .unwrap();
        assert!(
            engine::trust::verify_fact(&fixture.engine.store, &original.fact)
                .await
                .unwrap()
        );
        let action = fixture.action(
            &fixture.receiver,
            "receipt:review:revoked",
            Request::BindNeed {
                selection: fixture.selection.clone(),
                expected_measure: fixture.measure.clone(),
                supersedes: None,
            },
        );
        let action_base64 =
            base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(&action).unwrap());
        let intent_signature =
            fixture
                .receiver
                .signer
                .sign_bytes(&nucleus::action_intent::signing_bytes(
                    "receipt-session",
                    "receipt-challenge",
                    1,
                    "receipt-message",
                    &action_base64,
                ));
        let intent = store::action_intents::insert_pending(
            &fixture.engine.store.pool,
            store::action_intents::NewSignedActionIntent {
                session_id: "receipt-session",
                session_challenge: "receipt-challenge",
                sequence: 1,
                message_id: "receipt-message",
                actor_person_uid: &fixture.receiver.uid,
                key_id: &fixture.receiver.signer.key_id,
                action_base64: &action_base64,
                signature: &intent_signature,
            },
            nucleus::execution::now(),
        )
        .await
        .unwrap();
        store::action_intents::mark_committed(
            &fixture.engine.store.pool,
            &intent.uid,
            std::slice::from_ref(&original.fact.uid),
            nucleus::execution::now(),
        )
        .await
        .unwrap();
        let mut intent_only = original.fact.clone();
        intent_only.signature = None;
        assert!(
            engine::trust::verify_fact(&fixture.engine.store, &intent_only)
                .await
                .unwrap()
        );
        let (key, signature) = fixture
            .engine
            .revocation_certificate(&fixture.receiver.signer);
        assert!(
            fixture
                .engine
                .adopt_revocation(&fixture.receiver.uid, &key, &signature)
                .await
                .unwrap()
        );
        assert!(
            !engine::trust::verify_fact(&fixture.engine.store, &original.fact)
                .await
                .unwrap()
        );
        assert!(
            !engine::trust::verify_fact(&fixture.engine.store, &intent_only)
                .await
                .unwrap()
        );
        let now = nucleus::execution::now();
        let request = nucleus::transfer::receipt::ScopeRequest {
                            use_upfront_terms: true,
                            audience: nucleus::trust_analysis::contacts::Audience::Public,
            public_person_roles: Vec::new(),
            public_measurements: Vec::new(),
            occurrence: fixture.occurrence.clone(),
            expected_revision: fixture.transfer.revision,
            outcome_records: vec![binding.clone()],
            stages: vec![nucleus::transfer::receipt::Stage::Need],
            publish_agreed_quantity: false,
            expires_at: now.timestamp() + 86400,
        };
        assert!(
            fixture
                .engine
                .prepare_transfer_receipt_scope(request, None, now)
                .await
                .is_err()
        );
        assert_eq!(
            store::transfer_evidence::get(&fixture.engine.store.pool, &binding)
                .await
                .unwrap()
                .fact
                .hash,
            original.fact.hash
        );
    });
}

impl Fixture {
    async fn new() -> Self {
        Self::with_receiver_move_after_agreement(false).await
    }

    async fn with_receiver_move_after_agreement(move_receiver: bool) -> Self {
        Self::configured(move_receiver, 0, None).await
    }

    async fn configured(
        move_receiver: bool,
        proof_count: usize,
        receiver_organ: Option<String>,
    ) -> Self {
        Self::configured_with_foreign(move_receiver, proof_count, receiver_organ, None).await
    }

    async fn configured_with_foreign(
        move_receiver: bool,
        proof_count: usize,
        receiver_organ: Option<String>,
        foreign: Option<(&Engine, &Authorizer)>,
    ) -> Self {
        Self::configured_with_history(move_receiver, proof_count, receiver_organ, foreign, None).await
    }

    async fn configured_with_history(
        move_receiver: bool,
        proof_count: usize,
        receiver_organ: Option<String>,
        foreign: Option<(&Engine, &Authorizer)>,
        public_history: Option<nucleus::transfer::public_history::Terms>,
    ) -> Self {
        let engine = support::engine().await;
        let giver = support::person(&engine, "outcomes.giver").await;
        let receiver = support::person(&engine, "outcomes.receiver").await;
        let stranger = support::person(&engine, "outcomes.stranger").await;
        if let Some((node, _)) = foreign {
            engine
                .adopt_introduction(&node.introduction().await.unwrap(), 1)
                .await
                .unwrap();
        }
        if let Some(organ) = receiver_organ {
            store::organs::add_contact(
                &engine.store.pool,
                &organ,
                None,
                "Receiver Organ",
                "http://receiver.test",
                1,
            )
            .await
            .unwrap();
            store::sqlx::query("UPDATE record SET organ_uid = ? WHERE uid = ?")
                .bind(organ)
                .bind(&receiver.uid)
                .execute(&engine.store.pool)
                .await
                .unwrap();
        }
        let role = store::auth::ensure_role(&engine.store.pool, store::auth::ADMIN_ROLE)
            .await
            .unwrap();
        for (resource, action) in [
            ("record", "create"),
            ("record", "read"),
            ("record", "update"),
            ("transfer", "read"),
            ("transfer", "update"),
            ("fact", "read"),
        ] {
            let permission = store::auth::ensure_permission(&engine.store.pool, resource, action)
                .await
                .unwrap();
            store::auth::grant(&engine.store.pool, role, permission)
                .await
                .unwrap();
        }
        for person in [&giver, &receiver, &stranger] {
            store::auth::create_credential(
                &engine.store.pool,
                &person.uid,
                &person.uid,
                "hash",
                role,
            )
            .await
            .unwrap();
        }
        engine.set_signer(receiver.signer.clone()).await.unwrap();
        let need = engine
            .act(
                Action::CreateRecord {
                    slug: Some("outcomes.need".into()),
                    kind: nucleus::RecordKind::Plain,
                    head: "Food for this week".into(),
                    body: String::new(),
                    quantity: -10.0,
                },
                Some(receiver.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let vocabulary = store::linguas::create(&engine.store.pool, "Meals", None, "private")
            .await
            .unwrap();
        let concept = store::concepts::create_in(&engine.store.pool, &vocabulary, "meal", &[])
            .await
            .unwrap();
        let unit = store::concepts::create_in(&engine.store.pool, &vocabulary, "portion", &[])
            .await
            .unwrap();
        let assertion = store::assertions::assert(
            &engine.store.pool,
            store::assertions::NewAssertion {
                subject_uid: &need,
                predicate_uid: &concept,
                object_uid: None,
                role: store::assertions::AssertionRole::Ordinary,
                quantity: Some(store::exact::integer(10)),
                unit_uid: Some(&unit),
                asserted_by: Some(&receiver.uid),
            },
        )
        .await
        .unwrap();
        let revision = store::record_revisions::get_on(
            &mut *engine.store.pool.acquire().await.unwrap(),
            &need,
        )
        .await
        .unwrap()
        .revision;
        let selection = NeedSelection {
            need,
            expected_need_revision: revision,
            assertion,
            vocabulary: vocabulary.clone(),
            unit_vocabulary: Some(vocabulary),
        };
        let terms = store::transfer_evidence::freeze_need_on(
            &mut *engine.store.pool.acquire().await.unwrap(),
            &receiver.uid,
            &selection,
        )
        .await
        .unwrap();
        let stock = support::plain(&engine, "outcomes.stock", 20.0).await;
        let transfer = support::create_transfer_with_history(
            &engine,
            &giver,
            std::slice::from_ref(&receiver),
            vec![support::promise("outcome-food", &stock, &giver, -10.0)],
            DraftOptions {
                slug: "outcomes.transfer",
                agreement: nucleus::transfer::AgreementType::Full,
                reserve_default: engine::actions::TransferReservePoint::Active,
                require_confirmation: true,
            },
            public_history,
        )
        .await;
        support::agree(&engine, &transfer, &giver, "outcomes:agree:giver").await;
        support::agree(&engine, &transfer, &receiver, "outcomes:agree:receiver").await;
        if move_receiver {
            let foreign = nucleus::new_uid("r");
            store::organs::add_contact(
                &engine.store.pool,
                &foreign,
                None,
                "Later Organ",
                "http://later.test",
                1,
            )
            .await
            .unwrap();
            store::sqlx::query("UPDATE record SET organ_uid = ? WHERE uid = ?")
                .bind(foreign)
                .bind(&receiver.uid)
                .execute(&engine.store.pool)
                .await
                .unwrap();
        }
        let mut authority_records = Vec::new();
        let authorizer = if proof_count > 0 {
            let authorizer = Authorizer::new(&engine, 91).await;
            engine.set_signer(giver.signer.clone()).await.unwrap();
            for (index, party) in [None, Some(giver.uid.as_str()), Some(receiver.uid.as_str())]
                .into_iter()
                .take(proof_count)
                .enumerate()
            {
                let request = engine
                    .prepare_transfer_participation(
                        &transfer.transfer,
                        transfer.revision,
                        party,
                        None,
                    )
                    .await
                    .unwrap();
                let (node, issuer) = if party == Some(receiver.uid.as_str()) {
                    foreign.unwrap_or((&engine, &authorizer))
                } else {
                    (&engine, &authorizer)
                };
                let authority = node
                    .create_transfer_participation_authority(
                        &issuer.root,
                        &issuer.cell,
                        request.terms,
                        nucleus::execution::now(),
                    )
                    .await
                    .unwrap();
                let imported = engine
                    .act(
                        Action::TransferReceipt {
                            person: Some(giver.uid.clone()),
                            request_id: format!("original:import:{index}"),
                            request: nucleus::transfer::receipt::Command::Import {
                                document: serde_json::to_string(
                                    &nucleus::transfer::receipt::Document::Authority(authority),
                                )
                                .unwrap(),
                            },
                        },
                        None,
                    )
                    .await
                    .unwrap();
                authority_records.push(imported.created.unwrap());
            }
            Some(authorizer)
        } else {
            None
        };
        engine.set_signer(giver.signer.clone()).await.unwrap();
        let occurrence = engine
            .act(
                Action::ActivateTransferOccurrence {
                    transfer: transfer.transfer.clone(),
                    promise: "outcome-food".into(),
                    expected_revision: transfer.revision,
                    request_id: "outcomes:activate".into(),
                    person: Some(giver.uid.clone()),
                    original_authorities: authority_records.clone(),
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        Self {
            engine,
            giver,
            receiver,
            stranger,
            transfer,
            occurrence,
            stock,
            selection,
            measure: terms.measure,
            authority_records,
            authorizer,
        }
    }

    fn action(&self, person: &Person, id: &str, request: Request) -> Action {
        Action::RecordTransferOutcome {
            occurrence: self.occurrence.clone(),
            expected_revision: self.transfer.revision,
            request_id: id.into(),
            person: Some(person.uid.clone()),
            request,
        }
    }

    async fn bind(&self, id: &str) -> String {
        self.engine
            .set_signer(self.receiver.signer.clone())
            .await
            .unwrap();
        self.engine
            .act(
                self.action(
                    &self.receiver,
                    id,
                    Request::BindNeed {
                        selection: self.selection.clone(),
                        expected_measure: self.measure.clone(),
                        supersedes: None,
                    },
                ),
                Some(self.receiver.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap()
    }

    fn statement(
        &self,
        role: Role,
        amount: &str,
        binding: Option<&str>,
        supersedes: Option<&str>,
    ) -> Request {
        Request::State {
            role,
            measure: self.measure.clone(),
            amount: amount.into(),
            withdrawn: false,
            need_binding: binding.map(str::to_owned),
            supersedes: supersedes.map(str::to_owned),
        }
    }

    async fn history(&self, person: &Person) -> Value {
        let query = Protein {
            source: Source::Transfer,
            filter: vec![protein::Predicate::UidEq(self.transfer.transfer.clone())],
            fields: None,
            include: Default::default(),
            aggregate: None,
            order: vec![],
            limit: None,
        };
        let rows = protein::execute_for_with_signer(
            &self.engine.store,
            &query,
            Some(&person.uid),
            Some(&person.uid),
        )
        .await
        .unwrap();
        rows.into_iter()
            .find(|row| row["uid"] == self.transfer.transfer)
            .unwrap()["occurrences"][0]
            .clone()
    }
}

#[test]
fn original_party_opening_allows_own_remote_approval_without_organ_administration() {
    support::run_async_test("receipt-original-party-opening", || async {
        use nucleus::transfer::receipt::{
            ApprovalRole, Command, Document, Review, ScopeRequest, Stage,
        };
        let receiver = support::engine().await;
        let authorizer = Authorizer::new(&receiver, 93).await;
        let fixture = Fixture::configured_with_foreign(
            false,
            3,
            Some(authorizer.root.actor_uid.clone()),
            Some((&receiver, &authorizer)),
        )
        .await;
        let mut tx = store::write_tx(&receiver.store.pool).await.unwrap();
        store::records::create_with_uid_on(
            &mut tx,
            &fixture.receiver.uid,
            store::records::NewRecord {
                slug: None,
                kind: nucleus::RecordKind::Person,
                head: "Actual receiving Person",
                body: "",
                quantity: store::exact::integer(1),
            },
            &authorizer.root.actor_uid,
            None,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let intruder = support::person(&receiver, "original.intruder").await;
        let role = store::auth::ensure_role(&receiver.store.pool, "Own receipt reviewer")
            .await
            .unwrap();
        for (resource, operation) in [
            ("record", "create"),
            ("record", "read"),
            ("record", "update"),
            ("transfer", "update"),
        ] {
            let permission =
                store::auth::ensure_permission(&receiver.store.pool, resource, operation)
                    .await
                    .unwrap();
            store::auth::grant(&receiver.store.pool, role, permission)
                .await
                .unwrap();
        }
        for person in [&fixture.receiver, &intruder] {
            store::auth::create_credential(
                &receiver.store.pool,
                &person.uid,
                &person.uid,
                "hash",
                role,
            )
            .await
            .unwrap();
        }
        receiver
            .adopt_introduction(&fixture.engine.introduction().await.unwrap(), 1)
            .await
            .unwrap();
        let now = nucleus::execution::now();
        let scope = fixture
            .engine
            .prepare_transfer_receipt_scope(
                ScopeRequest {
                            use_upfront_terms: true,
                            audience: nucleus::trust_analysis::contacts::Audience::Public,
                    public_person_roles: Vec::new(),
                    public_measurements: Vec::new(),
                    occurrence: fixture.occurrence.clone(),
                    expected_revision: fixture.transfer.revision,
                    outcome_records: vec![],
                    stages: vec![Stage::Settled],
                    publish_agreed_quantity: false,
                    expires_at: now.timestamp() + 86400,
                },
                None,
                now,
            )
            .await
            .unwrap();
        let review = Document::Review(Review {
            person_approvals: Vec::new(),
            scope: scope.clone(),
            approvals: vec![],
            authorities: vec![],
        });
        let certificate = Document::Authority(
            receiver
                .create_transfer_receipt_authority(&authorizer.root, &authorizer.cell, now)
                .await
                .unwrap(),
        );
        let import = |person: &Person, id: &str, document: &Document| Action::TransferReceipt {
            person: Some(person.uid.clone()),
            request_id: id.into(),
            request: Command::Import {
                document: serde_json::to_string(document).unwrap(),
            },
        };
        receiver
            .set_signer(fixture.receiver.signer.clone())
            .await
            .unwrap();
        let authority_record = receiver
            .act(
                import(&fixture.receiver, "opening:key", &certificate),
                Some(fixture.receiver.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let record = receiver
            .act(
                import(&fixture.receiver, "opening:scope", &review),
                Some(fixture.receiver.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let approval = |person: &Person, id: &str, record: &str, key: &str, transfer| {
            Action::TransferReceipt {
                person: Some(person.uid.clone()),
                request_id: id.into(),
                request: Command::Approve {
                    record: record.into(),
                    expected_version: 1,
                    role: ApprovalRole::Receiver,
                    authority_record: key.into(),
                    private_transfer: transfer,
                },
            }
        };
        assert_eq!(
            receiver
                .act(
                    approval(
                        &fixture.receiver,
                        "opening:no-reference",
                        &record,
                        &authority_record,
                        None
                    ),
                    Some(fixture.receiver.uid.clone())
                )
                .await
                .unwrap_err()
                .code(),
            Some("forbidden")
        );
        assert_eq!(
            receiver
                .act(
                    approval(
                        &fixture.receiver,
                        "opening:wrong-reference",
                        &record,
                        &authority_record,
                        Some(nucleus::new_uid("r"))
                    ),
                    Some(fixture.receiver.uid.clone())
                )
                .await
                .unwrap_err()
                .code(),
            Some("forbidden")
        );
        let confirmed = receiver
            .act(
                approval(
                    &fixture.receiver,
                    "opening:own",
                    &record,
                    &authority_record,
                    Some(fixture.transfer.transfer.clone()),
                ),
                Some(fixture.receiver.uid.clone()),
            )
            .await
            .unwrap();
        assert_eq!(confirmed.data.as_ref().unwrap()["receipt"]["version"], 2);
        let saved = store::transfer_receipts::get(&receiver.store.pool, &record)
            .await
            .unwrap();
        let public = serde_json::to_string(&saved.document).unwrap();
        assert!(!public.contains(&fixture.transfer.transfer));
        assert!(!public.contains(&fixture.receiver.uid));
        let Document::Review(approved) = saved.document else {
            panic!("review expected")
        };
        receiver
            .verify_transfer_receipt_review(&approved, nucleus::execution::now())
            .await
            .unwrap();
        assert_eq!(approved.approvals.len(), 1);
        receiver.set_signer(intruder.signer.clone()).await.unwrap();
        let intruder_key = receiver
            .act(
                import(&intruder, "opening:intruder-key", &certificate),
                Some(intruder.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let intruder_review = receiver
            .act(
                import(&intruder, "opening:intruder-review", &review),
                Some(intruder.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        assert_eq!(
            receiver
                .act(
                    approval(
                        &intruder,
                        "opening:intruder",
                        &intruder_review,
                        &intruder_key,
                        Some(fixture.transfer.transfer.clone())
                    ),
                    Some(intruder.uid.clone())
                )
                .await
                .unwrap_err()
                .code(),
            Some("forbidden")
        );
        let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_occurrence")
            .fetch_one(&receiver.store.pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
        let reader = support::engine().await;
        for node in [&fixture.engine, &receiver] {
            reader
                .adopt_introduction(&node.introduction().await.unwrap(), 1)
                .await
                .unwrap();
        }
        reader
            .verify_transfer_receipt_review(&approved, nucleus::execution::now())
            .await
            .unwrap();
    });
}

#[test]
fn original_organ_approvals_are_frozen_at_activation_and_verified_without_private_mappings() {
    support::run_async_test("receipt-original-organ-authority", || async {
        use nucleus::transfer::receipt::{Document, PartyAttribution, Review, ScopeRequest, Stage};
        let fixture = Fixture::configured(false, 3, None).await;
        let now = nucleus::execution::now();
        let request = ScopeRequest {
                            use_upfront_terms: true,
                            audience: nucleus::trust_analysis::contacts::Audience::Public,
            public_person_roles: Vec::new(),
            public_measurements: Vec::new(),
            occurrence: fixture.occurrence.clone(),
            expected_revision: fixture.transfer.revision,
            outcome_records: vec![],
            stages: vec![Stage::Settled],
            publish_agreed_quantity: true,
            expires_at: now.timestamp() + 86400,
        };
        let scope = fixture
            .engine
            .prepare_transfer_receipt_scope(request.clone(), None, now)
            .await
            .unwrap();
        assert_eq!(
            scope.party_attribution,
            PartyAttribution::OriginalOrganAuthority
        );
        assert_eq!(scope.original_authorities.len(), 3);
        let public = serde_json::to_string(&scope).unwrap();
        for uid in [
            &fixture.transfer.transfer,
            &fixture.giver.uid,
            &fixture.receiver.uid,
            &fixture.stock,
            &fixture.selection.need,
        ] {
            assert!(!public.contains(uid));
        }
        for uid in &fixture.authority_records {
            assert!(!public.contains(uid));
        }
        let reader = support::engine().await;
        let review = Review {
            person_approvals: Vec::new(),
            scope: scope.clone(),
            approvals: vec![],
            authorities: vec![],
        };
        assert_eq!(
            reader
                .verify_transfer_receipt_review(&review, now)
                .await
                .unwrap_err()
                .code(),
            Some("transfer_receipt_unknown_identity")
        );
        reader
            .adopt_introduction(&fixture.engine.introduction().await.unwrap(), 1)
            .await
            .unwrap();
        reader
            .verify_transfer_receipt_review(&review, now)
            .await
            .unwrap();
        let rows: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_occurrence")
            .fetch_one(&reader.store.pool)
            .await
            .unwrap();
        assert_eq!(rows, 0);
        let mut late = review.clone();
        let authority = &mut late.scope.original_authorities[0].authority;
        authority.issued_at = scope.source.activated_at + 1;
        authority.signature = fixture
            .authorizer
            .as_ref()
            .unwrap()
            .root
            .sign_bytes(&authority.signing_bytes().unwrap());
        assert!(
            reader
                .verify_transfer_receipt_review(&late, now)
                .await
                .is_err()
        );
        let mut wrong = review.clone();
        wrong.scope.original_authorities[1]
            .authority
            .participation
            .as_mut()
            .unwrap()
            .agreement_revision += 1;
        assert!(
            reader
                .verify_transfer_receipt_review(&wrong, now)
                .await
                .is_err()
        );
        let mut signature = review.clone();
        signature.scope.original_authorities[1].authority.signature =
            signature.scope.original_authorities[2]
                .authority
                .signature
                .clone();
        assert!(
            reader
                .verify_transfer_receipt_review(&signature, now)
                .await
                .is_err()
        );
        let mut misuse = review.clone();
        misuse
            .authorities
            .push(scope.original_authorities[1].authority.clone());
        assert!(
            reader
                .verify_transfer_receipt_review(&misuse, now)
                .await
                .is_err()
        );
        let occurrence =
            store::transfers::occurrence(&fixture.engine.store.pool, &fixture.occurrence)
                .await
                .unwrap()
                .unwrap();
        let original =
            store::facts::get(&fixture.engine.store.pool, &occurrence.activation_fact_uid)
                .await
                .unwrap()
                .unwrap();
        let snapshot: nucleus::transfer::TransferActivationEvidence =
            serde_json::from_str(original.payload.as_deref().unwrap()).unwrap();
        assert_eq!(snapshot.original_authorities.len(), 3);
        let certificate = store::transfer_receipts::get(
            &fixture.engine.store.pool,
            &fixture.authority_records[0],
        )
        .await
        .unwrap();
        assert!(
            matches!(certificate.document, Document::Authority(ref authority) if authority.participation.is_some())
        );
        let root = &fixture.authorizer.as_ref().unwrap().root;
        let roster = fixture.engine.publish_roster(root, vec![]).await.unwrap();
        reader.adopt_roster(&roster).await.unwrap();
        assert_eq!(
            reader
                .verify_transfer_receipt_review(&review, now)
                .await
                .unwrap_err()
                .code(),
            Some("transfer_receipt_authority_withheld")
        );
        assert_eq!(
            store::facts::get(&fixture.engine.store.pool, &original.uid)
                .await
                .unwrap()
                .unwrap()
                .payload,
            original.payload
        );
        assert_eq!(
            store::records::quantity(&fixture.engine.store.pool, &fixture.stock)
                .await
                .unwrap()
                .unwrap(),
            store::exact::integer(20)
        );
    });
}

#[test]
fn incomplete_original_authority_and_later_approvals_do_not_rewrite_started_history() {
    support::run_async_test("receipt-partial-original-authority", || async {
        use nucleus::transfer::receipt::{PartyAttribution, ScopeRequest, Stage};
        let fixture = Fixture::configured(false, 2, None).await;
        let now = nucleus::execution::now();
        let request = ScopeRequest {
                            use_upfront_terms: true,
                            audience: nucleus::trust_analysis::contacts::Audience::Public,
            public_person_roles: Vec::new(),
            public_measurements: Vec::new(),
            occurrence: fixture.occurrence.clone(),
            expected_revision: fixture.transfer.revision,
            outcome_records: vec![],
            stages: vec![Stage::Settled],
            publish_agreed_quantity: false,
            expires_at: now.timestamp() + 86400,
        };
        let before = fixture
            .engine
            .prepare_transfer_receipt_scope(request.clone(), None, now)
            .await
            .unwrap();
        assert_eq!(
            before.party_attribution,
            PartyAttribution::LaterOrganEndorsement
        );
        assert!(before.original_authorities.is_empty());
        let original = store::transfers::occurrences_for_activation_request(
            &fixture.engine.store.pool,
            "outcomes:activate",
        )
        .await
        .unwrap()
        .unwrap();
        fixture
            .engine
            .set_signer(fixture.giver.signer.clone())
            .await
            .unwrap();
        let conflict = fixture
            .engine
            .act(
                Action::ActivateTransferOccurrence {
                    transfer: fixture.transfer.transfer.clone(),
                    promise: "outcome-food".into(),
                    expected_revision: fixture.transfer.revision,
                    request_id: "outcomes:activate".into(),
                    person: Some(fixture.giver.uid.clone()),
                    original_authorities: vec![],
                },
                None,
            )
            .await
            .unwrap_err();
        assert_eq!(conflict.code(), Some("transfer_request_id_conflict"));
        let replay = fixture
            .engine
            .act(
                Action::ActivateTransferOccurrence {
                    transfer: fixture.transfer.transfer.clone(),
                    promise: "outcome-food".into(),
                    expected_revision: fixture.transfer.revision,
                    request_id: "outcomes:activate".into(),
                    person: Some(fixture.giver.uid.clone()),
                    original_authorities: fixture.authority_records.clone(),
                },
                None,
            )
            .await
            .unwrap();
        assert!(replay.facts.is_empty());
        assert_eq!(replay.created.as_deref(), Some(fixture.occurrence.as_str()));
        let source = fixture
            .engine
            .prepare_transfer_participation(
                &fixture.transfer.transfer,
                fixture.transfer.revision,
                Some(&fixture.receiver.uid),
                None,
            )
            .await
            .unwrap();
        let authorizer = fixture.authorizer.as_ref().unwrap();
        let authority = fixture
            .engine
            .create_transfer_participation_authority(
                &authorizer.root,
                &authorizer.cell,
                source.terms,
                now,
            )
            .await
            .unwrap();
        fixture
            .engine
            .verify_transfer_receipt_document(
                &nucleus::transfer::receipt::Document::Authority(authority),
                now,
            )
            .await
            .unwrap();
        let after = fixture
            .engine
            .prepare_transfer_receipt_scope(request, None, now)
            .await
            .unwrap();
        assert_eq!(before, after);
        assert_eq!(
            store::transfers::occurrences_for_activation_request(
                &fixture.engine.store.pool,
                "outcomes:activate"
            )
            .await
            .unwrap()
            .unwrap()
            .fact
            .payload,
            original.fact.payload
        );
    });
}

#[test]
fn remote_organ_approves_an_opened_private_request_without_importing_a_foreign_transfer() {
    support::run_async_test("receipt-remote-original-participation", || async {
        use nucleus::transfer::receipt::{Command, Document};
        let receiver = support::engine().await;
        let owner = support::person(&receiver, "remote.owner").await;
        let authorizer = Authorizer::new(&receiver, 92).await;
        let fixture = Fixture::configured(false, 0, Some(authorizer.root.actor_uid.clone())).await;
        let mut tx = store::write_tx(&receiver.store.pool).await.unwrap();
        store::records::create_with_uid_on(
            &mut tx,
            &fixture.receiver.uid,
            store::records::NewRecord {
                slug: None,
                kind: nucleus::RecordKind::Person,
                head: "Receiver Person",
                body: "",
                quantity: store::exact::integer(1),
            },
            &authorizer.root.actor_uid,
            None,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let request = fixture
            .engine
            .prepare_transfer_participation(
                &fixture.transfer.transfer,
                fixture.transfer.revision,
                Some(&fixture.receiver.uid),
                None,
            )
            .await
            .unwrap();
        assert_eq!(request.terms.organ, authorizer.root.actor_uid);
        store::replica::make_own_root(&fixture.engine.store.pool, &fixture.receiver.uid)
            .await
            .unwrap();
        for target in [&fixture.transfer.transfer, &fixture.receiver.uid] {
            store::visibility::grant(
                &fixture.engine.store.pool,
                "actor",
                Some(&fixture.giver.uid),
                target,
            )
            .await
            .unwrap();
            assert!(
                fixture
                    .engine
                    .may_read_record(Some(&fixture.giver.uid), target)
                    .await
                    .unwrap(),
                "Sender cannot read reviewed participation target {target}"
            );
        }
        fixture
            .engine
            .prepare_transfer_participation(
                &fixture.transfer.transfer,
                fixture.transfer.revision,
                Some(&fixture.receiver.uid),
                Some(&fixture.giver.uid),
            )
            .await
            .unwrap();
        let (message, delivered) = ordinary_delivery::receive(
            &fixture.engine,
            &fixture.giver,
            nucleus::transfer::receipt_delivery::Source::Participation {
                transfer: fixture.transfer.transfer.clone(),
                expected_revision: fixture.transfer.revision,
                party: Some(fixture.receiver.uid.clone()),
            },
            &receiver,
            &owner,
            "remote:participation:exchange",
        )
        .await;
        let nucleus::transfer::receipt_delivery::Artifact::Participation(opened) =
            &delivered.artifact
        else {
            panic!("Expected a private participation request");
        };
        assert_eq!(opened, &request);
        assert!(
            receiver
                .act(
                    Action::TransferReceipt {
                        person: Some(owner.uid.clone()),
                        request_id: "remote:participation:not-an-import".into(),
                        request: Command::Delivery {
                            request: nucleus::transfer::receipt_delivery::Command::Import {
                                message,
                                expected_projection: delivered.hash().unwrap()
                            }
                        },
                    },
                    None
                )
                .await
                .is_err()
        );
        receiver.set_signer(owner.signer.clone()).await.unwrap();
        let action =
            |request_id: &str, input: &nucleus::transfer::receipt::ParticipationRequest| {
                Action::TransferReceipt {
                    person: Some(owner.uid.clone()),
                    request_id: request_id.into(),
                    request: Command::AuthorizeParticipation {
                        cell: String::new(),
                        request: serde_json::to_string(input).unwrap(),
                    },
                }
            };
        let mut changed = request.clone();
        changed.party = Some(fixture.stranger.uid.clone());
        assert!(
            receiver
                .act(action("remote:wrong-opening", &changed), None)
                .await
                .is_err()
        );
        let role = store::auth::ensure_role(&receiver.store.pool, "Participation reviewer")
            .await
            .unwrap();
        for (resource, operation) in [("record", "create"), ("record", "read")] {
            let permission =
                store::auth::ensure_permission(&receiver.store.pool, resource, operation)
                    .await
                    .unwrap();
            store::auth::grant(&receiver.store.pool, role, permission)
                .await
                .unwrap();
        }
        store::auth::create_credential(
            &receiver.store.pool,
            &owner.uid,
            "remote.owner",
            "hash",
            role,
        )
        .await
        .unwrap();
        let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
            .fetch_one(&receiver.store.pool)
            .await
            .unwrap();
        assert_eq!(
            receiver
                .act(
                    action("remote:no-organ-authority", &request),
                    Some(owner.uid.clone())
                )
                .await
                .unwrap_err()
                .code(),
            Some("forbidden")
        );
        let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
            .fetch_one(&receiver.store.pool)
            .await
            .unwrap();
        assert_eq!(before, after);
        let permission = store::auth::ensure_permission(&receiver.store.pool, "organ", "update")
            .await
            .unwrap();
        store::auth::grant(&receiver.store.pool, role, permission)
            .await
            .unwrap();
        let result = receiver
            .act(action("remote:approve", &request), Some(owner.uid.clone()))
            .await
            .unwrap();
        let uid = result.created.unwrap();
        let saved = store::transfer_receipts::get(&receiver.store.pool, &uid)
            .await
            .unwrap();
        let document = serde_json::to_string(&saved.document).unwrap();
        assert!(!document.contains(&fixture.transfer.transfer));
        assert!(!document.contains(&fixture.receiver.uid));
        assert!(
            matches!(saved.document, Document::Authority(ref authority) if authority.participation.as_ref() == Some(&request.terms))
        );
        for table in ["transfer", "transfer_occurrence", "transfer_outcome_record"] {
            let count: i64 = store::sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(&receiver.store.pool)
                .await
                .unwrap();
            assert_eq!(count, 0);
        }
        let replay = receiver
            .act(action("remote:approve", &request), None)
            .await
            .unwrap();
        assert_eq!(replay.created.as_deref(), Some(uid.as_str()));
        assert!(replay.facts.is_empty());
        ordinary_delivery::exchange(
            &receiver,
            &owner,
            &uid,
            &fixture.engine,
            &fixture.giver,
            "remote:participation:certificate-return",
        )
        .await;
        let mut wrong_organ = request.clone();
        wrong_organ.terms.organ = nucleus::new_uid("r");
        assert!(
            receiver
                .act(action("remote:other-organ", &wrong_organ), None)
                .await
                .is_err()
        );
    });
}

#[test]
fn signed_party_associations_survive_a_move_before_activation_and_before_receipt_review() {
    support::run_async_test("receipt-historical-party-association", || async {
        use nucleus::transfer::receipt::{PartyAttribution, ScopeRequest, Stage};
        use nucleus::transfer::{
            TransferActivationEvidence, TransferAgreementTransitionEvidence,
            TransferRevisionEvidence,
        };
        let fixture = Fixture::with_receiver_move_after_agreement(true).await;
        let pool = &fixture.engine.store.pool;
        let original_organ = store::organs::local(pool).await.unwrap().unwrap().uid;
        let current_receiver = store::records::get(pool, &fixture.receiver.uid)
            .await
            .unwrap()
            .unwrap();
        assert_ne!(current_receiver.organ_uid.as_ref(), Some(&original_organ));
        let revision_fact = store::transfers::revision_fact(
            pool,
            &fixture.transfer.transfer,
            fixture.transfer.revision,
        )
        .await
        .unwrap()
        .unwrap();
        let revision: TransferRevisionEvidence =
            serde_json::from_str(revision_fact.payload.as_deref().unwrap()).unwrap();
        assert!(
            revision
                .terms
                .parties
                .iter()
                .all(|party| party.organ_uid.as_ref() == Some(&original_organ))
        );
        let events = store::transfers::agreement_events(
            pool,
            &fixture.transfer.transfer,
            Some(fixture.transfer.revision),
        )
        .await
        .unwrap();
        assert_eq!(events.len(), 4);
        for event in events {
            let fact = store::facts::get(pool, &event.fact_uid)
                .await
                .unwrap()
                .unwrap();
            assert!(
                engine::trust::verify_fact(&fixture.engine.store, &fact)
                    .await
                    .unwrap()
            );
            let assent: TransferAgreementTransitionEvidence =
                serde_json::from_str(fact.payload.as_deref().unwrap()).unwrap();
            assert_eq!(assent.organ_uid.as_ref(), Some(&original_organ));
            assert_eq!(
                assent.revision_fact_hash.as_ref(),
                Some(&revision_fact.hash)
            );
        }
        let occurrence = store::transfers::occurrence(pool, &fixture.occurrence)
            .await
            .unwrap()
            .unwrap();
        let activation_fact = store::facts::get(pool, &occurrence.activation_fact_uid)
            .await
            .unwrap()
            .unwrap();
        let activation: TransferActivationEvidence =
            serde_json::from_str(activation_fact.payload.as_deref().unwrap()).unwrap();
        assert_eq!(
            activation.revision_fact_hash.as_ref(),
            Some(&revision_fact.hash)
        );
        assert_eq!(
            activation.occurrences[0].receiver_organ_uid.as_ref(),
            Some(&original_organ)
        );
        assert_eq!(
            activation.occurrences[0].giver_organ_uid.as_ref(),
            Some(&original_organ)
        );
        let now = nucleus::execution::now();
        let request = ScopeRequest {
                            use_upfront_terms: true,
                            audience: nucleus::trust_analysis::contacts::Audience::Public,
            public_person_roles: Vec::new(),
            public_measurements: Vec::new(),
            occurrence: fixture.occurrence.clone(),
            expected_revision: fixture.transfer.revision,
            outcome_records: vec![],
            stages: vec![Stage::Settled],
            publish_agreed_quantity: true,
            expires_at: now.timestamp() + 86400,
        };
        let before = fixture
            .engine
            .prepare_transfer_receipt_scope(request.clone(), None, now)
            .await
            .unwrap();
        assert_eq!(before.giver, original_organ);
        assert_eq!(before.receiver, original_organ);
        assert_eq!(
            before.party_attribution,
            PartyAttribution::LaterOrganEndorsement
        );
        store::sqlx::query("UPDATE record SET organ_uid = ? WHERE uid IN (?, ?)")
            .bind(current_receiver.organ_uid.as_deref().unwrap())
            .bind(&fixture.giver.uid)
            .bind(&fixture.receiver.uid)
            .execute(pool)
            .await
            .unwrap();
        let after = fixture
            .engine
            .prepare_transfer_receipt_scope(request, None, now)
            .await
            .unwrap();
        assert_eq!(before, after);
        assert_eq!(
            store::facts::get(pool, &revision_fact.uid)
                .await
                .unwrap()
                .unwrap()
                .payload,
            revision_fact.payload
        );
        assert_eq!(
            store::facts::get(pool, &activation_fact.uid)
                .await
                .unwrap()
                .unwrap()
                .payload,
            activation_fact.payload
        );
        let public = serde_json::to_string(&after).unwrap();
        assert!(!public.contains(&fixture.giver.uid));
        assert!(!public.contains(&fixture.receiver.uid));
        assert!(!public.contains(current_receiver.organ_uid.as_deref().unwrap()));
    });
}

#[test]
fn changed_party_association_requires_new_terms_to_advance_but_allows_withdrawal() {
    support::run_async_test("transfer-moved-party-agreement", || async {
        let fixture = Fixture::with_receiver_move_after_agreement(true).await;
        let pool = &fixture.engine.store.pool;
        fixture
            .engine
            .set_signer(fixture.receiver.signer.clone())
            .await
            .unwrap();
        let action = |level, request_id: &str| Action::SetTransferAgreementLevel {
            transfer: fixture.transfer.transfer.clone(),
            expected_revision: fixture.transfer.revision,
            person: Some(fixture.receiver.uid.clone()),
            request_id: request_id.into(),
            level,
        };
        for (level, id) in [(1, "moved:withdraw:one"), (0, "moved:withdraw:zero")] {
            fixture.engine.act(action(level, id), None).await.unwrap();
        }
        let facts: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
            .fetch_one(pool)
            .await
            .unwrap();
        let events = store::transfers::agreement_events(pool, &fixture.transfer.transfer, None)
            .await
            .unwrap()
            .len();
        let error = fixture
            .engine
            .act(action(1, "moved:advance"), None)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("new Transfer revision"));
        let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(after, facts);
        assert_eq!(
            store::transfers::agreement_events(pool, &fixture.transfer.transfer, None)
                .await
                .unwrap()
                .len(),
            events
        );
        assert!(
            store::transfers::agreement_event_for_request(pool, "moved:advance")
                .await
                .unwrap()
                .is_none()
        );
    });
}

#[test]
fn validly_signed_activation_cannot_change_the_reviewed_organ_or_revision_binding() {
    support::run_async_test("receipt-activation-role-binding", || async {
        use nucleus::transfer::TransferActivationEvidence;
        use nucleus::transfer::receipt::{ScopeRequest, Stage};
        let fixture = Fixture::new().await;
        let pool = &fixture.engine.store.pool;
        let occurrence = store::transfers::occurrence(pool, &fixture.occurrence)
            .await
            .unwrap()
            .unwrap();
        let original = store::facts::get(pool, &occurrence.activation_fact_uid)
            .await
            .unwrap()
            .unwrap();
        let activation: TransferActivationEvidence =
            serde_json::from_str(original.payload.as_deref().unwrap()).unwrap();
        let now = nucleus::execution::now();
        let request = ScopeRequest {
                            use_upfront_terms: true,
                            audience: nucleus::trust_analysis::contacts::Audience::Public,
            public_person_roles: Vec::new(),
            public_measurements: Vec::new(),
            occurrence: fixture.occurrence.clone(),
            expected_revision: fixture.transfer.revision,
            outcome_records: vec![],
            stages: vec![Stage::Settled],
            publish_agreed_quantity: true,
            expires_at: now.timestamp() + 86400,
        };
        for kind in ["revision", "organ", "missing"] {
            let mut altered = activation.clone();
            match kind {
                "revision" => altered.revision_fact_hash = Some("another revision".into()),
                "organ" => altered.occurrences[0].receiver_organ_uid = Some(nucleus::new_uid("r")),
                _ => altered.occurrences[0].receiver_organ_uid = None,
            }
            let mut fact = nucleus::fact::seal(
                nucleus::NewFact {
                    uid: Some(original.uid.clone()),
                    record_uid: original.record_uid.clone(),
                    delta: original.delta,
                    at: Some(original.at),
                    actor_uid: original.actor_uid.clone(),
                    cause: original.cause.clone(),
                    payload: Some(serde_json::to_string(&altered).unwrap()),
                },
                &original.prev_hash,
                original.at,
            );
            fact.signature = Some(fixture.giver.signer.sign_hash(&fact.hash));
            assert!(
                engine::trust::verify_fact(&fixture.engine.store, &fact)
                    .await
                    .unwrap()
            );
            store::sqlx::query(
                "UPDATE fact SET hash = ?, signature = ?, payload = ? WHERE uid = ?",
            )
            .bind(&fact.hash)
            .bind(&fact.signature)
            .bind(&fact.payload)
            .bind(&fact.uid)
            .execute(pool)
            .await
            .unwrap();
            let error = fixture
                .engine
                .prepare_transfer_receipt_scope(request.clone(), None, now)
                .await
                .unwrap_err();
            assert!(error.to_string().contains(if kind == "revision" {
                "original signed occurrence"
            } else {
                "Activated party Organ differs"
            }));
        }
    });
}

#[test]
fn receiver_need_and_actual_receipt_keep_distinct_exact_amounts_and_pinned_meanings() {
    support::run_async_test("outcome-exact-need", || async {
        let fixture = Fixture::new().await;
        let binding = fixture.bind("need:bind").await;
        fixture
            .engine
            .set_signer(fixture.giver.signer.clone())
            .await
            .unwrap();
        let giver_report = fixture
            .engine
            .act(
                fixture.action(
                    &fixture.giver,
                    "giver:hundred",
                    fixture.statement(Role::Giver, "100", None, None),
                ),
                Some(fixture.giver.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        fixture
            .engine
            .set_signer(fixture.receiver.signer.clone())
            .await
            .unwrap();
        let action = fixture.action(
            &fixture.receiver,
            "receipt:twelve",
            fixture.statement(Role::Receiver, "12", Some(&binding), None),
        );
        let receipt = fixture
            .engine
            .act(action.clone(), Some(fixture.receiver.uid.clone()))
            .await
            .unwrap();
        let uid = receipt.created.clone().unwrap();
        let saved = store::transfer_evidence::get(&fixture.engine.store.pool, &uid)
            .await
            .unwrap();
        assert!(
            engine::trust::verify_fact(&fixture.engine.store, &saved.fact)
                .await
                .unwrap()
        );
        assert!(
            matches!(saved.evidence.content, Content::Statement { quantity, .. } if quantity == store::exact::integer(12))
        );
        let need = store::transfer_evidence::get(&fixture.engine.store.pool, &binding)
            .await
            .unwrap();
        assert!(
            matches!(need.evidence.content, Content::NeedBinding { terms, .. } if terms.requested == store::exact::integer(10))
        );
        let progress = store::transfers::occurrence_settlement_progress(
            &fixture.engine.store.pool,
            &fixture.occurrence,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(progress.settled_quantity, 0.0);
        let occurrence =
            store::transfers::occurrence(&fixture.engine.store.pool, &fixture.occurrence)
                .await
                .unwrap()
                .unwrap();
        assert!(!occurrence.receipt_claimed);
        assert!(!occurrence.delivery_claimed);
        assert_eq!(
            store::records::get(&fixture.engine.store.pool, &fixture.stock)
                .await
                .unwrap()
                .unwrap()
                .quantity,
            store::exact::integer(20)
        );
        let replay = fixture
            .engine
            .act(action, Some(fixture.receiver.uid.clone()))
            .await
            .unwrap();
        assert_eq!(replay.created, receipt.created);
        assert!(replay.facts.is_empty());
        let mut tx = store::write_tx(&fixture.engine.store.pool).await.unwrap();
        store::vocabulary::revise_definition_on(
            &mut tx,
            &fixture.measure.meaning.concept_uid,
            1,
            "Different meal definition",
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let saved = store::transfer_evidence::get(&fixture.engine.store.pool, &uid)
            .await
            .unwrap();
        assert_eq!(saved.evidence.content.measure(), &fixture.measure);
        let read = fixture.history(&fixture.receiver).await;
        assert_eq!(read["outcomes"]["entries"].as_array().unwrap().len(), 2);
        assert_eq!(read["settlement_progress"]["settled_quantity"], 0.0);
        assert_eq!(read["outcome_measure_options"][0]["need_binding"], binding);
        let policy = nucleus::visibility::Policy {
            include: vec![nucleus::visibility::Condition {
                persons: vec![fixture.giver.uid.clone(), fixture.receiver.uid.clone()],
                ..Default::default()
            }],
            ..Default::default()
        };
        store::sqlx::query(
            "UPDATE data_visibility SET policy_json=? WHERE record_uid=? AND data='record'",
        )
        .bind(serde_json::to_string(&policy).unwrap())
        .bind(giver_report)
        .execute(&fixture.engine.store.pool)
        .await
        .unwrap();
        let read = fixture.history(&fixture.receiver).await;
        assert_eq!(read["outcomes"]["entries"].as_array().unwrap().len(), 3);
        assert!(
            read["outcomes"]["entries"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry["evidence"]["content"]["role"] == "giver"
                    && entry["evidence"]["content"]["quantity"]["value"] == "100")
        );
        support::claim(
            &fixture.engine,
            &fixture.occurrence,
            &fixture.giver,
            engine::actions::TransferOccurrenceClaimRole::Delivery,
            "outcomes:completed:giver",
        )
        .await;
        support::claim(
            &fixture.engine,
            &fixture.occurrence,
            &fixture.receiver,
            engine::actions::TransferOccurrenceClaimRole::Receipt,
            "outcomes:completed:receiver",
        )
        .await;
        let preview =
            support::settlement_preview(&fixture.engine, &fixture.occurrence, &fixture.giver, 10.0)
                .await;
        support::settle_from_preview(
            &fixture.engine,
            &fixture.occurrence,
            &fixture.giver,
            "outcomes:settled",
            &preview,
        )
        .await;
        let read = fixture.history(&fixture.receiver).await;
        assert_eq!(read["settlement_progress"]["settled_quantity"], 10.0);
        let statements = read["outcomes"]["entries"].as_array().unwrap();
        assert!(
            statements
                .iter()
                .any(|entry| entry["evidence"]["content"]["quantity"]["value"] == "12")
        );
        assert_eq!(
            store::records::get(&fixture.engine.store.pool, &fixture.stock)
                .await
                .unwrap()
                .unwrap()
                .quantity,
            store::exact::integer(10)
        );
    });
}

#[test]
fn outcome_authority_rejects_impersonation_changed_meanings_and_other_peoples_needs() {
    support::run_async_test("outcome-authority", || async {
        let fixture = Fixture::new().await;
        fixture
            .engine
            .set_signer(fixture.giver.signer.clone())
            .await
            .unwrap();
        let impersonation = fixture.action(
            &fixture.receiver,
            "forged-person",
            fixture.statement(Role::Receiver, "100", None, None),
        );
        assert!(
            fixture
                .engine
                .act(impersonation, Some(fixture.giver.uid.clone()))
                .await
                .is_err()
        );
        let wrong_role = fixture.action(
            &fixture.giver,
            "forged-role",
            fixture.statement(Role::Receiver, "100", None, None),
        );
        assert!(
            fixture
                .engine
                .act(wrong_role, Some(fixture.giver.uid.clone()))
                .await
                .is_err()
        );
        let binding = fixture.action(
            &fixture.giver,
            "forged-need",
            Request::BindNeed {
                selection: fixture.selection.clone(),
                expected_measure: fixture.measure.clone(),
                supersedes: None,
            },
        );
        assert!(
            fixture
                .engine
                .act(binding, Some(fixture.giver.uid.clone()))
                .await
                .is_err()
        );
        let mut changed = fixture.measure.clone();
        changed.meaning.definition_hash = "0".repeat(64);
        let action = fixture.action(
            &fixture.giver,
            "forged-hash",
            Request::State {
                role: Role::Giver,
                measure: changed,
                amount: "100".into(),
                withdrawn: false,
                need_binding: None,
                supersedes: None,
            },
        );
        assert!(
            fixture
                .engine
                .act(action, Some(fixture.giver.uid.clone()))
                .await
                .is_err()
        );
        fixture
            .engine
            .set_signer(fixture.stranger.signer.clone())
            .await
            .unwrap();
        let action = fixture.action(
            &fixture.stranger,
            "outsider",
            fixture.statement(Role::Giver, "100", None, None),
        );
        assert!(
            fixture
                .engine
                .act(action, Some(fixture.stranger.uid.clone()))
                .await
                .is_err()
        );
        let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_outcome_record")
            .fetch_one(&fixture.engine.store.pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    });
}

#[test]
fn corrections_keep_conflicts_and_withdrawals_without_duplicate_credit_or_replay_side_effects() {
    support::run_async_test("outcome-corrections", || async {
        let fixture = Fixture::new().await;
        fixture
            .engine
            .set_signer(fixture.receiver.signer.clone())
            .await
            .unwrap();
        let first = fixture
            .engine
            .act(
                fixture.action(
                    &fixture.receiver,
                    "first",
                    fixture.statement(
                        Role::Receiver,
                        "9007199254740993.000000000000000001",
                        None,
                        None,
                    ),
                ),
                Some(fixture.receiver.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let duplicate = fixture.action(
            &fixture.receiver,
            "duplicate",
            fixture.statement(Role::Receiver, "8", None, None),
        );
        assert!(
            fixture
                .engine
                .act(duplicate, Some(fixture.receiver.uid.clone()))
                .await
                .is_err()
        );
        for (id, amount) in [("correction-a", "8"), ("correction-b", "9")] {
            fixture
                .engine
                .act(
                    fixture.action(
                        &fixture.receiver,
                        id,
                        fixture.statement(Role::Receiver, amount, None, Some(&first)),
                    ),
                    Some(fixture.receiver.uid.clone()),
                )
                .await
                .unwrap();
        }
        let history = fixture.history(&fixture.receiver).await;
        let entries = history["outcomes"]["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry["superseded"] == false && entry["conflicting"] == true)
                .count(),
            2
        );
        let old = entries
            .iter()
            .find(|entry| entry["evidence"]["record"] == first)
            .unwrap();
        assert_eq!(
            old["evidence"]["content"]["quantity"]["value"],
            "9007199254740993.000000000000000001"
        );
        let head = entries
            .iter()
            .find(|entry| entry["superseded"] == false)
            .unwrap()["evidence"]["record"]
            .as_str()
            .unwrap();
        let mut withdrawn = fixture.statement(Role::Receiver, "0", None, Some(head));
        if let Request::State { withdrawn, .. } = &mut withdrawn {
            *withdrawn = true;
        }
        fixture
            .engine
            .act(
                fixture.action(&fixture.receiver, "withdraw", withdrawn),
                Some(fixture.receiver.uid.clone()),
            )
            .await
            .unwrap();
        assert_eq!(
            fixture.history(&fixture.receiver).await["outcomes"]["entries"]
                .as_array()
                .unwrap()
                .len(),
            4
        );
        assert!(
            store::sqlx::query(
                "UPDATE transfer_outcome_record SET payload_json='{}' WHERE record_uid=?"
            )
            .bind(&first)
            .execute(&fixture.engine.store.pool)
            .await
            .is_err()
        );
        assert!(
            store::sqlx::query("DELETE FROM transfer_outcome_record WHERE record_uid=?")
                .bind(&first)
                .execute(&fixture.engine.store.pool)
                .await
                .is_err()
        );
        let overwrite = Action::SetExtension {
            target: first,
            namespace: nucleus::transfer::outcome::NAMESPACE.into(),
            fds: json!({"quantity":"1000"}),
        };
        assert!(fixture.engine.act(overwrite, None).await.is_err());
        assert!(
            store::records::set_extension(
                &fixture.engine.store.pool,
                head,
                nucleus::transfer::outcome::NAMESPACE,
                &json!({"quantity":"1000"})
            )
            .await
            .is_err()
        );
    });
}

#[test]
fn stale_need_reviews_and_mismatched_binding_units_are_rejected_atomically() {
    support::run_async_test("outcome-stale-meaning", || async {
        let fixture = Fixture::new().await;
        fixture
            .engine
            .set_signer(fixture.receiver.signer.clone())
            .await
            .unwrap();
        store::sqlx::query("UPDATE record SET head='Updated Need' WHERE uid=?")
            .bind(&fixture.selection.need)
            .execute(&fixture.engine.store.pool)
            .await
            .unwrap();
        let stale = fixture.action(
            &fixture.receiver,
            "stale:need",
            Request::BindNeed {
                selection: fixture.selection.clone(),
                expected_measure: fixture.measure.clone(),
                supersedes: None,
            },
        );
        assert!(
            fixture
                .engine
                .act(stale, Some(fixture.receiver.uid.clone()))
                .await
                .is_err()
        );
        let mut selection = fixture.selection.clone();
        selection.expected_need_revision = store::record_revisions::get_on(
            &mut *fixture.engine.store.pool.acquire().await.unwrap(),
            &selection.need,
        )
        .await
        .unwrap()
        .revision;
        let binding = fixture
            .engine
            .act(
                fixture.action(
                    &fixture.receiver,
                    "fresh:need",
                    Request::BindNeed {
                        selection,
                        expected_measure: fixture.measure.clone(),
                        supersedes: None,
                    },
                ),
                Some(fixture.receiver.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let mut measure = fixture.measure.clone();
        measure.unit = None;
        let wrong_unit = fixture.action(
            &fixture.receiver,
            "wrong:unit",
            Request::State {
                role: Role::Receiver,
                measure,
                amount: "10".into(),
                withdrawn: false,
                need_binding: Some(binding),
                supersedes: None,
            },
        );
        assert!(
            fixture
                .engine
                .act(wrong_unit, Some(fixture.receiver.uid.clone()))
                .await
                .is_err()
        );
        let stale = Action::RecordTransferOutcome {
            occurrence: fixture.occurrence.clone(),
            expected_revision: fixture.transfer.revision + 1,
            request_id: "stale:transfer".into(),
            person: Some(fixture.receiver.uid.clone()),
            request: fixture.statement(Role::Receiver, "10", None, None),
        };
        assert!(
            fixture
                .engine
                .act(stale, Some(fixture.receiver.uid.clone()))
                .await
                .is_err()
        );
        let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_outcome_record")
            .fetch_one(&fixture.engine.store.pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    });
}

#[test]
fn a_reviewed_need_unit_cannot_change_silently_when_its_definition_changes() {
    support::run_async_test("outcome-reviewed-unit", || async {
        let fixture = Fixture::new().await;
        let mut tx = store::write_tx(&fixture.engine.store.pool).await.unwrap();
        store::assertions::retract_tx(
            &mut tx,
            &fixture.selection.assertion,
            Some(&fixture.receiver.uid),
        )
        .await
        .unwrap();
        let assertion = store::assertions::insert_tx(
            &mut tx,
            &nucleus::new_uid("a"),
            store::assertions::NewAssertion {
                subject_uid: &fixture.selection.need,
                predicate_uid: &fixture.measure.meaning.concept_uid,
                object_uid: None,
                role: store::assertions::AssertionRole::Ordinary,
                quantity: None,
                unit_uid: None,
                asserted_by: Some(&fixture.receiver.uid),
            },
        )
        .await
        .unwrap()
        .uid;
        store::sqlx::query("UPDATE record SET unit_uid=? WHERE uid=?")
            .bind(&fixture.measure.unit.as_ref().unwrap().concept_uid)
            .bind(&fixture.selection.need)
            .execute(&mut *tx)
            .await
            .unwrap();
        let selection = NeedSelection {
            assertion,
            expected_need_revision: store::record_revisions::get_on(
                &mut tx,
                &fixture.selection.need,
            )
            .await
            .unwrap()
            .revision,
            ..fixture.selection.clone()
        };
        let reviewed =
            store::transfer_evidence::freeze_need_on(&mut tx, &fixture.receiver.uid, &selection)
                .await
                .unwrap();
        store::vocabulary::revise_definition_on(
            &mut tx,
            &fixture.measure.unit.as_ref().unwrap().concept_uid,
            1,
            "A larger portion",
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        fixture
            .engine
            .set_signer(fixture.receiver.signer.clone())
            .await
            .unwrap();
        let stale = fixture.action(
            &fixture.receiver,
            "stale:unit-review",
            Request::BindNeed {
                selection: selection.clone(),
                expected_measure: reviewed.measure,
                supersedes: None,
            },
        );
        assert!(
            fixture
                .engine
                .act(stale, Some(fixture.receiver.uid.clone()))
                .await
                .is_err()
        );
        let fresh = store::transfer_evidence::freeze_need_on(
            &mut *fixture.engine.store.pool.acquire().await.unwrap(),
            &fixture.receiver.uid,
            &selection,
        )
        .await
        .unwrap();
        let approved = fixture.action(
            &fixture.receiver,
            "fresh:unit-review",
            Request::BindNeed {
                selection,
                expected_measure: fresh.measure,
                supersedes: None,
            },
        );
        fixture
            .engine
            .act(approved, Some(fixture.receiver.uid.clone()))
            .await
            .unwrap();
        let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_outcome_record")
            .fetch_one(&fixture.engine.store.pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    });
}

#[test]
fn outcome_records_start_private_and_unshared_need_mappings_do_not_escape_through_history() {
    support::run_async_test("outcome-private", || async {
        let fixture = Fixture::new().await;
        let binding = fixture.bind("private:bind").await;
        let receipt = fixture
            .engine
            .act(
                fixture.action(
                    &fixture.receiver,
                    "private:received",
                    fixture.statement(Role::Receiver, "10", Some(&binding), None),
                ),
                Some(fixture.receiver.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        assert_eq!(
            fixture.history(&fixture.receiver).await["outcomes"]["entries"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(
            fixture.history(&fixture.giver).await["outcomes"]["entries"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        for record in [&binding, &receipt] {
            let raw: String = store::sqlx::query_scalar("SELECT value FROM sync_op WHERE tbl='data_visibility' AND uid=? AND field='record' AND kind='set' ORDER BY seq DESC LIMIT 1")
                .bind(record).fetch_one(&fixture.engine.store.pool).await.unwrap();
            let policy: nucleus::visibility::SavedPolicy = serde_json::from_str(&raw).unwrap();
            assert_eq!(
                policy.policy.include[0].persons,
                vec![fixture.receiver.uid.clone()]
            );
            assert!(policy.policy.exclude.is_empty());
        }
        let mut tx = store::write_tx(&fixture.engine.store.pool).await.unwrap();
        let policy = nucleus::visibility::Policy {
            include: vec![nucleus::visibility::Condition {
                persons: vec![fixture.giver.uid.clone(), fixture.receiver.uid.clone()],
                ..Default::default()
            }],
            ..Default::default()
        };
        for record in [&binding, &receipt] {
            store::sqlx::query(
                "UPDATE data_visibility SET policy_json=? WHERE record_uid=? AND data='record'",
            )
            .bind(serde_json::to_string(&policy).unwrap())
            .bind(record)
            .execute(&mut *tx)
            .await
            .unwrap();
        }
        store::data_visibility::ensure_private_for_person_on(
            &mut tx,
            &fixture.selection.need,
            nucleus::visibility::Data::Record,
            Some(&fixture.receiver.uid),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        assert!(
            fixture.history(&fixture.giver).await["outcomes"]["entries"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let query = Protein {
            source: Source::Record,
            filter: vec![protein::Predicate::UidEq(receipt)],
            fields: None,
            include: Default::default(),
            aggregate: None,
            order: vec![],
            limit: None,
        };
        assert!(
            protein::execute_for(&fixture.engine.store, &query, Some(&fixture.stranger.uid))
                .await
                .unwrap()
                .is_empty()
        );
    });
}
