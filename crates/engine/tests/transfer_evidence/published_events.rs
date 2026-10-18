use super::*;
#[path = "lifecycle.rs"]
mod lifecycle;
#[path="custody.rs"]
mod custody;
use nucleus::transfer::receipt::{
    ApprovalRole, Command, Content as PublicContent, Document, EventRequest, EventSource,
    ScopeRequest, Stage,
};

struct PublicationFixture {
    fixture: Fixture,
    binding: String,
    receiver_key: String,
    giver_key: String,
    receiver_review: String,
}

pub(super) fn action(person: &Person, id: &str, request: Command) -> Action {
    Action::TransferReceipt {
        person: Some(person.uid.clone()),
        request_id: id.into(),
        request,
    }
}

pub(super) async fn import(
    engine: &Engine,
    person: &Person,
    id: &str,
    document: &Document,
) -> String {
    engine.set_signer(person.signer.clone()).await.unwrap();
    engine
        .act(
            action(
                person,
                id,
                Command::Import {
                    document: serde_json::to_string(document).unwrap(),
                },
            ),
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap()
}

pub(super) async fn document(engine: &Engine, record: &str) -> Document {
    store::transfer_receipts::get(&engine.store.pool, record)
        .await
        .unwrap()
        .document
}

pub(super) async fn request(
    engine: &Engine,
    record: &str,
    key: &str,
    source: EventSource,
) -> EventRequest {
    let saved = store::transfer_receipts::get(&engine.store.pool, record)
        .await
        .unwrap();
    EventRequest {
        record: record.into(),
        expected_version: saved.version,
        authority_record: key.into(),
        source,
    }
}

pub(super) async fn reviewed(
    engine: &Engine,
    person: &Person,
    id: &str,
    request: EventRequest,
) -> Action {
    engine.set_signer(person.signer.clone()).await.unwrap();
    let preview = engine
        .act(
            action(
                person,
                id,
                Command::PreviewEvent {
                    request: request.clone(),
                },
            ),
            Some(person.uid.clone()),
        )
        .await
        .unwrap();
    assert!(preview.created.is_none());
    assert!(preview.facts.is_empty());
    let value = preview.data.unwrap()["receipt"]["document"]["value"].clone();
    action(
        person,
        &format!("{id}:sign"),
        Command::PublishEvent {
            request,
            reviewed_at: value["reviewed_at"].as_i64().unwrap(),
            expected_projection: serde_json::from_value(value["expected_projection"].clone())
                .unwrap(),
        },
    )
}

impl PublicationFixture {
    async fn new() -> Self {
        Self::with_person_roles(Vec::new()).await
    }

    async fn with_person_roles(public_person_roles: Vec<ApprovalRole>) -> Self {
        Self::with_options(
            public_person_roles,
            vec![
                Stage::Need,
                Stage::Giver,
                Stage::Receiver,
                Stage::Settled,
                Stage::Dispute,
            ],
        )
        .await
    }

    async fn with_options(public_person_roles: Vec<ApprovalRole>, stages: Vec<Stage>) -> Self {
        let fixture = Fixture::configured(false, 3, None).await;
        let binding = fixture.bind("events:need").await;
        let authority = Document::Authority(
            fixture
                .engine
                .create_transfer_receipt_authority(
                    &fixture.authorizer.as_ref().unwrap().root,
                    &fixture.authorizer.as_ref().unwrap().cell,
                    nucleus::execution::now(),
                )
                .await
                .unwrap(),
        );
        let receiver_key = import(
            &fixture.engine,
            &fixture.receiver,
            "events:receiver:key",
            &authority,
        )
        .await;
        let giver_key = import(
            &fixture.engine,
            &fixture.giver,
            "events:giver:key",
            &authority,
        )
        .await;
        fixture
            .engine
            .set_signer(fixture.receiver.signer.clone())
            .await
            .unwrap();
        let receiver_review = fixture
            .engine
            .act(
                action(
                    &fixture.receiver,
                    "events:review",
                    Command::Prepare {
                        scope: ScopeRequest {
                            use_upfront_terms: true,
                            audience: nucleus::trust_analysis::contacts::Audience::Public,
                            public_person_roles,
                            public_measurements: Vec::new(),
                            occurrence: fixture.occurrence.clone(),
                            expected_revision: fixture.transfer.revision,
                            outcome_records: vec![binding.clone()],
                            stages,
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
        fixture
            .engine
            .act(
                action(
                    &fixture.receiver,
                    "events:receiver:consent",
                    Command::Approve {
                        record: receiver_review.clone(),
                        expected_version: 1,
                        role: ApprovalRole::Receiver,
                        authority_record: receiver_key.clone(),
                        private_transfer: None,
                    },
                ),
                Some(fixture.receiver.uid.clone()),
            )
            .await
            .unwrap();
        let giver_review = import(
            &fixture.engine,
            &fixture.giver,
            "events:giver:review",
            &document(&fixture.engine, &receiver_review).await,
        )
        .await;
        for (role, version) in [(ApprovalRole::Giver, 1), (ApprovalRole::Origin, 2)] {
            fixture
                .engine
                .act(
                    action(
                        &fixture.giver,
                        &format!("events:consent:{version}"),
                        Command::Approve {
                            record: giver_review.clone(),
                            expected_version: version,
                            role,
                            authority_record: giver_key.clone(),
                            private_transfer: None,
                        },
                    ),
                    None,
                )
                .await
                .unwrap();
        }
        import(
            &fixture.engine,
            &fixture.receiver,
            "events:all:consent",
            &document(&fixture.engine, &giver_review).await,
        )
        .await;
        Self {
            fixture,
            binding,
            receiver_key,
            giver_key,
            receiver_review,
        }
    }

    async fn statement(
        &self,
        role: Role,
        amount: &str,
        binding: Option<&str>,
        previous: Option<&str>,
        id: &str,
    ) -> String {
        let person = match role {
            Role::Giver => &self.fixture.giver,
            Role::Receiver => &self.fixture.receiver,
        };
        self.fixture
            .engine
            .set_signer(person.signer.clone())
            .await
            .unwrap();
        self.fixture
            .engine
            .act(
                self.fixture.action(
                    person,
                    id,
                    self.fixture.statement(role, amount, binding, previous),
                ),
                Some(person.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap()
    }

    async fn publish(&self, record: &str, source: &str, role: Role, id: &str) -> String {
        let (person, key) = match role {
            Role::Giver => (&self.fixture.giver, &self.giver_key),
            Role::Receiver => (&self.fixture.receiver, &self.receiver_key),
        };
        let request = request(
            &self.fixture.engine,
            record,
            key,
            EventSource::Outcome {
                record: source.into(),
            },
        )
        .await;
        let command = reviewed(&self.fixture.engine, person, id, request).await;
        self.fixture
            .engine
            .act(command, Some(person.uid.clone()))
            .await
            .unwrap()
            .created
            .unwrap()
    }
}

#[test]
fn personal_attribution_requires_own_consent_and_an_independently_accepted_identity() {
    support::run_async_test("receipt-person-attribution", || async {
        use nucleus::transfer::receipt::PersonApproval;
        let setup = PublicationFixture::with_person_roles(vec![ApprovalRole::Receiver]).await;
        let f = &setup.fixture;
        let saved = store::transfer_receipts::get(&f.engine.store.pool, &setup.receiver_review)
            .await
            .unwrap();
        let scope = saved.document.scope().unwrap().clone();
        assert_eq!(scope.public_persons[0].person, f.receiver.uid);
        let Document::Review(review) = &saved.document else {
            panic!("Review expected");
        };
        assert_eq!(review.approvals.len(), 3);
        assert!(!review.is_complete());
        let source = request(
            &f.engine,
            &setup.receiver_review,
            &setup.receiver_key,
            EventSource::Outcome {
                record: setup.binding.clone(),
            },
        )
        .await;
        let denied = f
            .engine
            .act(
                action(
                    &f.receiver,
                    "person:before-consent",
                    Command::PreviewEvent { request: source },
                ),
                Some(f.receiver.uid.clone()),
            )
            .await
            .unwrap_err();
        assert_eq!(denied.code(), Some("transfer_receipt_incomplete"));
        let approval = PersonApproval {
            scope: scope.id().unwrap(),
            role: ApprovalRole::Receiver,
            person: f.receiver.uid.clone(),
            issued_at: nucleus::execution::now().timestamp(),
            key_id: String::new(),
            signature: String::new(),
        };
        let wrong_reference = action(
            &f.receiver,
            "person:wrong-opening",
            Command::ApprovePerson {
                record: setup.receiver_review.clone(),
                expected_version: saved.version,
                approval: approval.clone(),
                private_transfer: Some(nucleus::new_uid("r")),
            },
        );
        assert_eq!(
            f.engine
                .act(wrong_reference, Some(f.receiver.uid.clone()))
                .await
                .unwrap_err()
                .code(),
            Some("forbidden")
        );
        let giver_review = import(&f.engine, &f.giver, "person:member-copy", &saved.document).await;
        let giver_version = store::transfer_receipts::get(&f.engine.store.pool, &giver_review)
            .await
            .unwrap()
            .version;
        let impersonation = action(
            &f.giver,
            "person:other-member",
            Command::ApprovePerson {
                record: giver_review,
                expected_version: giver_version,
                approval: approval.clone(),
                private_transfer: None,
            },
        );
        assert_eq!(
            f.engine
                .act(impersonation, Some(f.giver.uid.clone()))
                .await
                .unwrap_err()
                .code(),
            Some("forbidden")
        );
        f.engine
            .set_signer(f.receiver.signer.clone())
            .await
            .unwrap();
        let consent = action(
            &f.receiver,
            "person:own-consent",
            Command::ApprovePerson {
                record: setup.receiver_review.clone(),
                expected_version: saved.version,
                approval: approval.clone(),
                private_transfer: Some(f.transfer.transfer.clone()),
            },
        );
        let result = f
            .engine
            .act(consent.clone(), Some(f.receiver.uid.clone()))
            .await
            .unwrap();
        assert_eq!(result.data.unwrap()["receipt"]["state"], "approved_scope");
        assert!(
            f.engine
                .act(consent, Some(f.receiver.uid.clone()))
                .await
                .unwrap()
                .facts
                .is_empty()
        );
        let next = request(
            &f.engine,
            &setup.receiver_review,
            &setup.receiver_key,
            EventSource::Outcome {
                record: setup.binding.clone(),
            },
        )
        .await;
        let publication = f
            .engine
            .act(
                reviewed(&f.engine, &f.receiver, "person:need", next).await,
                Some(f.receiver.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let portable = document(&f.engine, &publication).await;
        let Document::Publication(bundle) = &portable else {
            panic!("Publication expected");
        };
        assert_eq!(bundle.publication.person_approvals.len(), 1);
        let bytes = serde_json::to_string(&portable).unwrap();
        assert!(bytes.contains(&f.receiver.uid));
        for private in [
            &f.giver.uid,
            &f.transfer.transfer,
            &f.stock,
            &f.selection.need,
        ] {
            assert!(!bytes.contains(private));
        }
        let reader = support::engine().await;
        reader
            .publish_root_key(&f.authorizer.as_ref().unwrap().root)
            .await
            .unwrap();
        let owner = support::person(&reader, "person.history-reader").await;
        reader.set_signer(owner.signer.clone()).await.unwrap();
        let unknown = reader
            .verify_transfer_receipt_document(&portable, nucleus::execution::now())
            .await
            .unwrap_err();
        assert_eq!(unknown.code(), Some("transfer_receipt_unknown_identity"));
        let keys = engine::trust::keys_of(&reader.store, &f.receiver.uid)
            .await
            .unwrap();
        assert!(keys.is_empty());
        let foreign = reader
            .adopt_introduction(&f.engine.introduction().await.unwrap(), 1)
            .await
            .unwrap();
        let mut tx = store::write_tx(&reader.store.pool).await.unwrap();
        store::records::create_with_uid_on(
            &mut tx,
            &f.receiver.uid,
            store::records::NewRecord {
                slug: None,
                kind: nucleus::RecordKind::Person,
                head: "Independently recognized receiver",
                body: "",
                quantity: store::exact::integer(1),
            },
            &foreign,
            None,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let key = nucleus::identity::PersonKey {
            subject: f.receiver.uid.clone(),
            key_id: f.receiver.signer.key_id.clone(),
            public_key: f.receiver.signer.public_key_b64(),
        };
        let compared = reader
            .act(
                Action::Identity {
                    request: nucleus::identity::Command::PreviewPersonKey { key: key.clone() },
                },
                None,
            )
            .await
            .unwrap()
            .data
            .unwrap()["identity"]
            .clone();
        assert!(
            engine::trust::keys_of(&reader.store, &f.receiver.uid)
                .await
                .unwrap()
                .is_empty()
        );
        reader
            .act(
                Action::Identity {
                    request: nucleus::identity::Command::AcceptPersonKey {
                        key,
                        expected_profile_revision: compared["profile_revision"].as_i64().unwrap(),
                        expected_review: serde_json::from_value(compared["review_hash"].clone())
                            .unwrap(),
                    },
                },
                None,
            )
            .await
            .unwrap();
        let mut forged = portable.clone();
        let Document::Publication(forged_bundle) = &mut forged else {
            unreachable!();
        };
        forged_bundle.publication.person_approvals[0].signature = f
            .giver
            .signer
            .sign_bytes(&approval.signing_bytes().unwrap());
        assert_eq!(
            reader
                .verify_transfer_receipt_document(&forged, nucleus::execution::now())
                .await
                .unwrap_err()
                .code(),
            Some("transfer_receipt_invalid")
        );
        let retained = import(&reader, &owner, "person:retained", &portable).await;
        let history_action = || {
            action(
                &owner,
                "person:history",
                Command::History {
                    origin: scope.source.origin.clone(),
                    occurrence: scope.source.occurrence.clone(),
                    before: None,
                },
            )
        };
        let history = reader
            .act(history_action(), None)
            .await
            .unwrap()
            .data
            .unwrap()["receipt"]["document"]["value"]
            .clone();
        assert_eq!(
            history["history"]["outcomes"][0]["copies"][0]["persons"][0]["person"],
            f.receiver.uid
        );
        let canonical: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_occurrence")
            .fetch_one(&reader.store.pool)
            .await
            .unwrap();
        assert_eq!(canonical, 0);
        let (key, signature) = reader.revocation_certificate(&f.receiver.signer);
        assert!(
            reader
                .adopt_revocation(&f.receiver.uid, &key, &signature)
                .await
                .unwrap()
        );
        let withheld = reader
            .act(history_action(), None)
            .await
            .unwrap()
            .data
            .unwrap()["receipt"]["document"]["value"]
            .clone();
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
        assert_eq!(
            store::transfer_receipts::get(&reader.store.pool, &retained)
                .await
                .unwrap()
                .document,
            portable
        );
    });
}

#[test]
fn real_publication_roundtrip_preserves_need_reported_and_settled_amounts_without_private_mappings()
{
    support::run_async_test("receipt-public-event-roundtrip", || async {
        let setup = PublicationFixture::new().await;
        let f = &setup.fixture;
        let received = setup
            .statement(
                Role::Receiver,
                "12.000000000000000001",
                Some(&setup.binding),
                None,
                "events:received",
            )
            .await;
        let given = setup
            .statement(Role::Giver, "100", None, None, "events:given")
            .await;
        let need_request = request(
            &f.engine,
            &setup.receiver_review,
            &setup.receiver_key,
            EventSource::Outcome {
                record: setup.binding.clone(),
            },
        )
        .await;
        let command = reviewed(&f.engine, &f.receiver, "events:need:preview", need_request).await;
        let first = f
            .engine
            .act(command.clone(), Some(f.receiver.uid.clone()))
            .await
            .unwrap();
        let receiver_receipt = first.created.unwrap();
        let stock = store::records::get(&f.engine.store.pool, &f.stock)
            .await
            .unwrap()
            .unwrap()
            .quantity;
        assert_eq!(stock, store::exact::integer(20));
        setup
            .publish(
                &receiver_receipt,
                &received,
                Role::Receiver,
                "events:receiver:preview",
            )
            .await;
        let replay = f
            .engine
            .act(command, Some(f.receiver.uid.clone()))
            .await
            .unwrap();
        assert!(replay.facts.is_empty());
        assert_eq!(replay.data.unwrap()["receipt"]["version"], 1);
        let giver_receipt = import(
            &f.engine,
            &f.giver,
            "events:giver:history",
            &document(&f.engine, &receiver_receipt).await,
        )
        .await;
        setup
            .publish(&giver_receipt, &given, Role::Giver, "events:giver:preview")
            .await;
        support::claim(
            &f.engine,
            &f.occurrence,
            &f.giver,
            engine::actions::TransferOccurrenceClaimRole::Delivery,
            "events:delivery",
        )
        .await;
        support::claim(
            &f.engine,
            &f.occurrence,
            &f.receiver,
            engine::actions::TransferOccurrenceClaimRole::Receipt,
            "events:receipt",
        )
        .await;
        let quote = support::settlement_preview(&f.engine, &f.occurrence, &f.giver, 10.0).await;
        support::settle_from_preview(&f.engine, &f.occurrence, &f.giver, "events:settle", &quote)
            .await;
        let slice =
            store::transfers::occurrence_settlement_slices(&f.engine.store.pool, &f.occurrence)
                .await
                .unwrap()
                .remove(0);
        let settle_request = request(
            &f.engine,
            &giver_receipt,
            &setup.giver_key,
            EventSource::Settlement {
                fact: slice.evidence_fact_uid,
                supersedes: None,
            },
        )
        .await;
        let preview_command = action(
            &f.giver,
            "events:canonical:preview",
            Command::PreviewEvent {
                request: settle_request.clone(),
            },
        );
        f.engine.set_signer(f.giver.signer.clone()).await.unwrap();
        assert_eq!(
            f.engine
                .act(preview_command.clone(), Some(f.giver.uid.clone()))
                .await
                .unwrap_err()
                .code(),
            Some("forbidden")
        );
        let preview = f
            .engine
            .act(preview_command, None)
            .await
            .unwrap()
            .data
            .unwrap()["receipt"]["document"]["value"]
            .clone();
        f.engine
            .act(
                action(
                    &f.giver,
                    "events:canonical:publish",
                    Command::PublishEvent {
                        request: settle_request,
                        reviewed_at: preview["reviewed_at"].as_i64().unwrap(),
                        expected_projection: serde_json::from_value(
                            preview["expected_projection"].clone(),
                        )
                        .unwrap(),
                    },
                ),
                None,
            )
            .await
            .unwrap();
        let history = document(&f.engine, &giver_receipt).await;
        let Document::Publication(bundle) = &history else {
            panic!("Expected signed public receipt");
        };
        assert_eq!(bundle.publication.events.len(), 4);
        assert!(bundle.publication.events.iter().any(|signed| matches!(&signed.event.content, PublicContent::Need { requested, .. } if requested.canonical() == "10")));
        assert!(bundle.publication.events.iter().any(|signed| matches!(&signed.event.content, PublicContent::Statement { role: Role::Receiver, quantity, need: Some(_), .. } if quantity.canonical() == "12.000000000000000001")));
        assert!(bundle.publication.events.iter().any(|signed| matches!(&signed.event.content, PublicContent::Statement { role: Role::Giver, quantity, .. } if quantity.canonical() == "100")));
        assert!(bundle.publication.events.iter().any(|signed| matches!(&signed.event.content, PublicContent::Settled { quantity } if quantity.value().unwrap() == 10.0)));
        let exported = f
            .engine
            .act(
                action(
                    &f.giver,
                    "events:export",
                    Command::Export {
                        record: giver_receipt.clone(),
                    },
                ),
                Some(f.giver.uid.clone()),
            )
            .await
            .unwrap();
        assert!(exported.facts.is_empty());
        let bytes = exported.data.unwrap()["receipt_export"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(
            Document::parse(bytes.as_bytes(), nucleus::execution::now().timestamp()).unwrap(),
            history
        );
        for private in [
            &f.transfer.transfer,
            &f.stock,
            &f.selection.need,
            &f.giver.uid,
            &f.receiver.uid,
            &setup.binding,
            &received,
            &given,
        ] {
            assert!(
                !bytes.contains(private),
                "Leaked private identifier {private}"
            );
        }
        let reader = support::engine().await;
        reader
            .adopt_introduction(&f.engine.introduction().await.unwrap(), 1)
            .await
            .unwrap();
        let reader_person = support::person(&reader, "events.reader").await;
        let imported = import(&reader, &reader_person, "events:reader:import", &history).await;
        assert_eq!(document(&reader, &imported).await, history);
        let replay = import(&reader, &reader_person, "events:reader:repeat", &history).await;
        assert_eq!(imported, replay);
        super::analysis::inspect_original_outcomes(&reader,&reader_person,&bundle.publication.scope.giver,&bundle.publication.scope.receiver,&f.engine).await;
        custody::forwarded_receipts(&setup,&giver_receipt,&receiver_receipt,&received,&reader,&imported).await;
        let foreign: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_occurrence")
            .fetch_one(&reader.store.pool)
            .await
            .unwrap();
        assert_eq!(foreign, 0);
        assert_eq!(
            store::records::get(&f.engine.store.pool, &f.stock)
                .await
                .unwrap()
                .unwrap()
                .quantity,
            store::exact::integer(10)
        );
    });
}

#[test]
fn public_dispute_and_its_clearance_follow_the_original_partys_signed_history() {
    support::run_async_test("receipt-public-dispute", || async {
        let setup = PublicationFixture::new().await;
        let f = &setup.fixture;
        let mut record = setup.receiver_review.clone();
        let mut parent = None;
        for (id, active) in [("events:disputed", true), ("events:cleared", false)] {
            f.engine
                .set_signer(f.receiver.signer.clone())
                .await
                .unwrap();
            f.engine
                .act(
                    Action::SetTransferOccurrenceDispute {
                        occurrence: f.occurrence.clone(),
                        request_id: id.into(),
                        person: Some(f.receiver.uid.clone()),
                        disputed: active,
                    },
                    Some(f.receiver.uid.clone()),
                )
                .await
                .unwrap();
            let original =
                store::transfers::occurrence_dispute_for_request(&f.engine.store.pool, id)
                    .await
                    .unwrap()
                    .unwrap();
            let input = request(
                &f.engine,
                &record,
                &setup.receiver_key,
                EventSource::Dispute {
                    fact: original.fact.uid,
                    supersedes: parent,
                },
            )
            .await;
            let command = reviewed(&f.engine, &f.receiver, &format!("{id}:preview"), input).await;
            record = f
                .engine
                .act(command, Some(f.receiver.uid.clone()))
                .await
                .unwrap()
                .created
                .unwrap();
            let Document::Publication(bundle) = document(&f.engine, &record).await else {
                panic!("Expected publication");
            };
            let latest = bundle.publication.events.last().unwrap();
            assert!(
                matches!(latest.event.content, PublicContent::Dispute { role:Role::Receiver,active:value } if value==active)
            );
            parent = Some(latest.event.id().unwrap());
        }
        let Document::Publication(bundle) = document(&f.engine, &record).await else {
            panic!("Expected publication");
        };
        assert_eq!(bundle.publication.events.len(), 2);
        let verified = f
            .engine
            .verify_transfer_receipt(&bundle, nucleus::execution::now())
            .await
            .unwrap();
        assert_eq!(verified.heads().values().next().unwrap().len(), 1);
        assert_eq!(
            store::records::get(&f.engine.store.pool, &f.stock)
                .await
                .unwrap()
                .unwrap()
                .quantity,
            store::exact::integer(20)
        );
    });
}

#[test]
fn publication_keeps_conflicting_corrections_and_requires_their_public_predecessors() {
    support::run_async_test("receipt-public-corrections", || async {
        let setup = PublicationFixture::new().await;
        let f = &setup.fixture;
        let root = setup
            .statement(Role::Receiver, "10", None, None, "events:root")
            .await;
        let a = setup
            .statement(Role::Receiver, "8", None, Some(&root), "events:branch:a")
            .await;
        let b = setup
            .statement(Role::Receiver, "9", None, Some(&root), "events:branch:b")
            .await;
        let missing = request(
            &f.engine,
            &setup.receiver_review,
            &setup.receiver_key,
            EventSource::Outcome { record: a.clone() },
        )
        .await;
        assert_eq!(
            f.engine
                .act(
                    action(
                        &f.receiver,
                        "events:missing",
                        Command::PreviewEvent { request: missing }
                    ),
                    Some(f.receiver.uid.clone())
                )
                .await
                .unwrap_err()
                .code(),
            Some("transfer_receipt_incomplete")
        );
        let receipt = setup
            .publish(
                &setup.receiver_review,
                &root,
                Role::Receiver,
                "events:root:publish",
            )
            .await;
        setup
            .publish(&receipt, &a, Role::Receiver, "events:branch:a:publish")
            .await;
        setup
            .publish(&receipt, &b, Role::Receiver, "events:branch:b:publish")
            .await;
        let Document::Publication(bundle) = document(&f.engine, &receipt).await else {
            panic!("Expected publication");
        };
        let verified = f
            .engine
            .verify_transfer_receipt(&bundle, nucleus::execution::now())
            .await
            .unwrap();
        assert_eq!(verified.heads().len(), 1);
        assert_eq!(verified.heads().values().next().unwrap().len(), 2);
        assert_eq!(bundle.publication.events.len(), 3);
        let duplicate = request(
            &f.engine,
            &receipt,
            &setup.receiver_key,
            EventSource::Outcome { record: root },
        )
        .await;
        assert!(
            f.engine
                .act(
                    action(
                        &f.receiver,
                        "events:duplicate",
                        Command::PreviewEvent { request: duplicate }
                    ),
                    Some(f.receiver.uid.clone())
                )
                .await
                .is_err()
        );
    });
}

#[test]
fn event_review_cannot_be_reused_for_changed_inputs_or_another_party() {
    support::run_async_test("receipt-public-review-authority", || async {
        let setup = PublicationFixture::new().await;
        let f = &setup.fixture;
        let reported = setup
            .statement(Role::Receiver, "12", None, None, "events:review:report")
            .await;
        let input = request(
            &f.engine,
            &setup.receiver_review,
            &setup.receiver_key,
            EventSource::Outcome {
                record: setup.binding.clone(),
            },
        )
        .await;
        let original = reviewed(&f.engine, &f.receiver, "events:review:preview", input).await;
        let facts: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
            .fetch_one(&f.engine.store.pool)
            .await
            .unwrap();
        let mut changed = original.clone();
        if let Action::TransferReceipt {
            request: Command::PublishEvent { request, .. },
            ..
        } = &mut changed
        {
            request.source = EventSource::Outcome {
                record: reported.clone(),
            };
        }
        assert!(
            f.engine
                .act(changed, Some(f.receiver.uid.clone()))
                .await
                .unwrap_err()
                .to_string()
                .contains("projection changed")
        );
        let mut expired = original.clone();
        if let Action::TransferReceipt {
            request: Command::PublishEvent { reviewed_at, .. },
            ..
        } = &mut expired
        {
            *reviewed_at -= 301;
        }
        assert!(
            f.engine
                .act(expired, Some(f.receiver.uid.clone()))
                .await
                .is_err()
        );
        let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
            .fetch_one(&f.engine.store.pool)
            .await
            .unwrap();
        assert_eq!(facts, after);
        let Document::Review(mut unfinished) = document(&f.engine, &setup.receiver_review).await
        else {
            panic!("Expected private review");
        };
        unfinished.scope.expires_at -= 1;
        unfinished.approvals.clear();
        unfinished.authorities.clear();
        let unfinished = import(
            &f.engine,
            &f.receiver,
            "events:unsigned:scope",
            &Document::Review(unfinished),
        )
        .await;
        let input = request(
            &f.engine,
            &unfinished,
            &setup.receiver_key,
            EventSource::Outcome {
                record: setup.binding.clone(),
            },
        )
        .await;
        assert!(
            f.engine
                .act(
                    action(
                        &f.receiver,
                        "events:unsigned:preview",
                        Command::PreviewEvent { request: input }
                    ),
                    Some(f.receiver.uid.clone())
                )
                .await
                .is_err()
        );
        let stranger_record = import(
            &f.engine,
            &f.stranger,
            "events:intruder:review",
            &document(&f.engine, &setup.receiver_review).await,
        )
        .await;
        let stranger_key = import(
            &f.engine,
            &f.stranger,
            "events:intruder:key",
            &document(&f.engine, &setup.receiver_key).await,
        )
        .await;
        let input = request(
            &f.engine,
            &stranger_record,
            &stranger_key,
            EventSource::Outcome { record: reported },
        )
        .await;
        assert_eq!(
            f.engine
                .act(
                    action(
                        &f.stranger,
                        "events:intruder:preview",
                        Command::PreviewEvent { request: input }
                    ),
                    None
                )
                .await
                .unwrap_err()
                .code(),
            Some("forbidden")
        );
        f.engine
            .set_signer(f.receiver.signer.clone())
            .await
            .unwrap();
        assert!(
            f.engine
                .act(original, Some(f.receiver.uid.clone()))
                .await
                .is_ok()
        );
    });
}
