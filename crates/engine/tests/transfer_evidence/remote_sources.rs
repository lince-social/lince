use super::published_events::{action, document, import, request, reviewed};
use super::*;
#[path = "workflow.rs"]
mod workflow;
use nucleus::transfer::{
    receipt::{
        ApprovalRole, Command, Content as PublicContent, Document, EventSource, ScopeRequest, Stage,
    },
    receipt_source::SourceRequest,
};

struct Remote {
    source: Fixture,
    receiver: Engine,
    receiver_owner: Authorizer,
    initial_review: String,
    selection: NeedSelection,
    measure: Measure,
}

impl Remote {
    async fn new() -> Self {
        let receiver = support::engine().await;
        let receiver_owner = Authorizer::new(&receiver, 99).await;
        let source = Fixture::configured_with_foreign(
            false,
            3,
            Some(receiver_owner.root.actor_uid.clone()),
            Some((&receiver, &receiver_owner)),
        )
        .await;
        let mut tx = store::write_tx(&receiver.store.pool).await.unwrap();
        store::records::create_with_uid_on(
            &mut tx,
            &source.receiver.uid,
            store::records::NewRecord {
                slug: None,
                kind: nucleus::RecordKind::Person,
                head: "Remote receiver",
                body: "",
                quantity: store::exact::integer(1),
            },
            &receiver_owner.root.actor_uid,
            None,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let role = store::auth::ensure_role(&receiver.store.pool, "Own receiver evidence")
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
        store::auth::create_credential(
            &receiver.store.pool,
            &source.receiver.uid,
            &source.receiver.uid,
            "hash",
            role,
        )
        .await
        .unwrap();
        receiver
            .adopt_introduction(&source.engine.introduction().await.unwrap(), 1)
            .await
            .unwrap();
        receiver
            .set_signer(source.receiver.signer.clone())
            .await
            .unwrap();
        let need = receiver
            .act(
                Action::CreateRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: "Receiver's private food Need".into(),
                    body: "Private context".into(),
                    quantity: -10.0,
                },
                Some(source.receiver.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let vocabulary =
            store::linguas::create(&receiver.store.pool, "Remote meals", None, "private")
                .await
                .unwrap();
        let concept = store::concepts::create_in(&receiver.store.pool, &vocabulary, "meal", &[])
            .await
            .unwrap();
        let unit = store::concepts::create_in(&receiver.store.pool, &vocabulary, "portion", &[])
            .await
            .unwrap();
        let assertion = store::assertions::assert(
            &receiver.store.pool,
            store::assertions::NewAssertion {
                subject_uid: &need,
                predicate_uid: &concept,
                object_uid: None,
                role: store::assertions::AssertionRole::Ordinary,
                quantity: Some(store::exact::integer(10)),
                unit_uid: Some(&unit),
                asserted_by: Some(&source.receiver.uid),
            },
        )
        .await
        .unwrap();
        let selection = NeedSelection {
            need: need.clone(),
            expected_need_revision: store::record_revisions::get_on(
                &mut *receiver.store.pool.acquire().await.unwrap(),
                &need,
            )
            .await
            .unwrap()
            .revision,
            assertion,
            vocabulary: vocabulary.clone(),
            unit_vocabulary: Some(vocabulary),
        };
        let terms = store::transfer_evidence::freeze_need_on(
            &mut *receiver.store.pool.acquire().await.unwrap(),
            &source.receiver.uid,
            &selection,
        )
        .await
        .unwrap();
        source
            .engine
            .set_signer(source.giver.signer.clone())
            .await
            .unwrap();
        let review = source
            .engine
            .act(
                action(
                    &source.giver,
                    "remote:initial",
                    Command::Prepare {
                        scope: ScopeRequest {
                            use_upfront_terms: true,
                            audience: nucleus::trust_analysis::contacts::Audience::Public,
                            public_person_roles: Vec::new(),
                            public_measurements: vec![],
                            occurrence: source.occurrence.clone(),
                            expected_revision: source.transfer.revision,
                            outcome_records: vec![],
                            stages: vec![Stage::Settled],
                            publish_agreed_quantity: false,
                            expires_at: nucleus::execution::now().timestamp() + 86400,
                        },
                    },
                ),
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let issuer = source.authorizer.as_ref().unwrap();
        let certificate = Document::Authority(
            source
                .engine
                .create_transfer_receipt_authority(
                    &issuer.root,
                    &issuer.cell,
                    nucleus::execution::now(),
                )
                .await
                .unwrap(),
        );
        let key = import(
            &source.engine,
            &source.giver,
            "remote:initial:key",
            &certificate,
        )
        .await;
        source
            .engine
            .act(
                action(
                    &source.giver,
                    "remote:initial:anchor",
                    Command::Approve {
                        record: review.clone(),
                        expected_version: 1,
                        role: ApprovalRole::Origin,
                        authority_record: key,
                        private_transfer: None,
                    },
                ),
                None,
            )
            .await
            .unwrap();
        let initial_review = super::ordinary_delivery::exchange(
            &source.engine,
            &source.giver,
            &review,
            &receiver,
            &source.receiver,
            "remote:initial:exchange",
        )
        .await;
        Self {
            source,
            receiver,
            receiver_owner,
            initial_review,
            selection,
            measure: terms.measure,
        }
    }

    fn capture(
        &self,
        id: &str,
        record: &str,
        outcome: Request,
        private_transfer: Option<String>,
        public_need: Option<nucleus::karma::CanonicalHash>,
    ) -> Action {
        action(
            &self.source.receiver,
            id,
            Command::RecordSource {
                request: SourceRequest {
                    receipt: record.into(),
                    expected_version: 1,
                    private_transfer,
                    outcome,
                    public_need,
                },
            },
        )
    }

    fn binding(&self, id: &str) -> Action {
        self.capture(
            id,
            &self.initial_review,
            Request::BindNeed {
                selection: self.selection.clone(),
                expected_measure: self.measure.clone(),
                supersedes: None,
            },
            Some(self.source.transfer.transfer.clone()),
            None,
        )
    }

    async fn bind(&self) -> String {
        self.receiver
            .set_signer(self.source.receiver.signer.clone())
            .await
            .unwrap();
        self.receiver
            .act(
                self.binding("remote:bind"),
                Some(self.source.receiver.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap()
    }

    async fn approve(&self, binding: &str) -> (String, String, String) {
        self.approve_with_stages(
            binding,
            vec![Stage::Need, Stage::Giver, Stage::Receiver, Stage::Settled],
        )
        .await
    }

    async fn approve_with_stages(
        &self,
        binding: &str,
        stages: Vec<Stage>,
    ) -> (String, String, String) {
        let f = &self.source;
        let frozen = store::receipt_sources::get(&self.receiver.store.pool, binding)
            .await
            .unwrap();
        f.engine.set_signer(f.giver.signer.clone()).await.unwrap();
        let review = f
            .engine
            .act(
                action(
                    &f.giver,
                    "remote:final:prepare",
                    Command::Prepare {
                        scope: ScopeRequest {
                            use_upfront_terms: true,
                            audience: nucleus::trust_analysis::contacts::Audience::Public,
                            public_person_roles: Vec::new(),
                            public_measurements: vec![frozen.snapshot.measurement],
                            occurrence: f.occurrence.clone(),
                            expected_revision: f.transfer.revision,
                            outcome_records: vec![],
                            stages,
                            publish_agreed_quantity: false,
                            expires_at: nucleus::execution::now().timestamp() + 86400,
                        },
                    },
                ),
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let owner = f.authorizer.as_ref().unwrap();
        let key = Document::Authority(
            f.engine
                .create_transfer_receipt_authority(
                    &owner.root,
                    &owner.cell,
                    nucleus::execution::now(),
                )
                .await
                .unwrap(),
        );
        let source_key = import(&f.engine, &f.giver, "remote:source:key", &key).await;
        for (role, version) in [(ApprovalRole::Origin, 1), (ApprovalRole::Giver, 2)] {
            f.engine
                .act(
                    action(
                        &f.giver,
                        &format!("remote:source:approve:{version}"),
                        Command::Approve {
                            record: review.clone(),
                            expected_version: version,
                            role,
                            authority_record: source_key.clone(),
                            private_transfer: None,
                        },
                    ),
                    None,
                )
                .await
                .unwrap();
        }
        let record = super::ordinary_delivery::exchange(
            &f.engine,
            &f.giver,
            &review,
            &self.receiver,
            &f.receiver,
            "remote:final:exchange",
        )
        .await;
        let key = Document::Authority(
            self.receiver
                .create_transfer_receipt_authority(
                    &self.receiver_owner.root,
                    &self.receiver_owner.cell,
                    nucleus::execution::now(),
                )
                .await
                .unwrap(),
        );
        let key = import(&self.receiver, &f.receiver, "remote:receiver:key", &key).await;
        self.receiver
            .act(
                action(
                    &f.receiver,
                    "remote:receiver:approve",
                    Command::Approve {
                        record: record.clone(),
                        expected_version: 1,
                        role: ApprovalRole::Receiver,
                        authority_record: key.clone(),
                        private_transfer: Some(f.transfer.transfer.clone()),
                    },
                ),
                Some(f.receiver.uid.clone()),
            )
            .await
            .unwrap();
        (record, key, source_key)
    }

    async fn publish(&self, record: &str, key: &str, snapshot: &str, id: &str) -> String {
        let input = request(
            &self.receiver,
            record,
            key,
            EventSource::Snapshot {
                record: snapshot.into(),
            },
        )
        .await;
        let command = reviewed(&self.receiver, &self.source.receiver, id, input).await;
        self.receiver
            .act(command, Some(self.source.receiver.uid.clone()))
            .await
            .unwrap()
            .created
            .unwrap()
    }
}

#[test]
fn three_organs_exchange_remote_need_reports_without_foreign_canonical_state_or_private_mappings() {
    support::run_async_test("remote-receipt-source-roundtrip", || async {
        let setup = Remote::new().await;
        let f = &setup.source;
        let binding = setup.bind().await;
        assert!(
            store::records::get(&f.engine.store.pool, &setup.selection.need)
                .await
                .unwrap()
                .is_none()
        );
        let captured = store::receipt_sources::get(&setup.receiver.store.pool, &binding)
            .await
            .unwrap();
        assert_eq!(captured.snapshot.content.measure(), &setup.measure);
        setup
            .receiver
            .act(
                Action::SetQuantityExact {
                    target: setup.selection.need.clone(),
                    amount: "0".into(),
                },
                Some(f.receiver.uid.clone()),
            )
            .await
            .unwrap();
        assert!(
            setup
                .receiver
                .act(setup.binding("remote:bind"), Some(f.receiver.uid.clone()))
                .await
                .unwrap()
                .facts
                .is_empty()
        );
        let (review, key, source_key) = setup.approve(&binding).await;
        let publication = setup
            .publish(&review, &key, &binding, "remote:publish:need")
            .await;
        let mut report = setup.capture(
            "remote:report",
            &publication,
            Request::State {
                role: Role::Receiver,
                measure: setup.measure.clone(),
                amount: "12.000000000000000001".into(),
                withdrawn: false,
                need_binding: Some(binding.clone()),
                supersedes: None,
            },
            Some(f.transfer.transfer.clone()),
            None,
        );
        if let Action::TransferReceipt {
            request: Command::RecordSource { request },
            ..
        } = &mut report
        {
            request.expected_version = 1;
        }
        let report = setup
            .receiver
            .act(report, Some(f.receiver.uid.clone()))
            .await
            .unwrap()
            .created
            .unwrap();
        setup
            .publish(&publication, &key, &report, "remote:publish:report")
            .await;
        let exported = setup
            .receiver
            .act(
                action(
                    &f.receiver,
                    "remote:export",
                    Command::Export {
                        record: publication.clone(),
                    },
                ),
                Some(f.receiver.uid.clone()),
            )
            .await
            .unwrap();
        let bytes = exported.data.unwrap()["receipt_export"]
            .as_str()
            .unwrap()
            .to_owned();
        for private in [
            &setup.selection.need,
            &binding,
            &report,
            &f.receiver.uid,
            &f.giver.uid,
            &f.stock,
            &f.transfer.transfer,
        ] {
            assert!(
                !bytes.contains(private),
                "Private reference was exported: {private}"
            );
        }
        assert!(!bytes.contains("\"clock\""));
        let history =
            Document::parse(bytes.as_bytes(), nucleus::execution::now().timestamp()).unwrap();
        let Document::Publication(bundle) = &history else {
            panic!("Expected public history");
        };
        assert_eq!(bundle.publication.events.len(), 2);
        assert!(
            matches!(&bundle.publication.events[0].event.content,PublicContent::Need {requested,..} if requested.canonical()=="10")
        );
        assert!(
            matches!(&bundle.publication.events[1].event.content,PublicContent::Statement {role:Role::Receiver,quantity,need:Some(_),..} if quantity.canonical()=="12.000000000000000001")
        );
        let reader = support::engine().await;
        reader
            .adopt_introduction(&f.engine.introduction().await.unwrap(), 1)
            .await
            .unwrap();
        reader
            .adopt_introduction(&setup.receiver.introduction().await.unwrap(), 1)
            .await
            .unwrap();
        let viewer = support::person(&reader, "remote.reader").await;
        super::ordinary_delivery::exchange(
            &setup.receiver,
            &f.receiver,
            &publication,
            &reader,
            &viewer,
            "remote:reader:exchange",
        )
        .await;
        let source_history = super::ordinary_delivery::exchange(
            &setup.receiver,
            &f.receiver,
            &publication,
            &f.engine,
            &f.giver,
            "remote:source:exchange",
        )
        .await;
        let need_event = bundle.publication.events[0].event.id().unwrap();
        let input = SourceRequest {
            receipt: source_history.clone(),
            expected_version: 1,
            private_transfer: Some(f.transfer.transfer.clone()),
            outcome: Request::State {
                role: Role::Giver,
                measure: setup.measure.clone(),
                amount: "100".into(),
                withdrawn: false,
                need_binding: None,
                supersedes: None,
            },
            public_need: Some(need_event),
        };
        let contributed = f
            .engine
            .act(
                action(
                    &f.giver,
                    "remote:given",
                    Command::RecordSource { request: input },
                ),
                Some(f.giver.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let input = request(
            &f.engine,
            &source_history,
            &source_key,
            EventSource::Snapshot {
                record: contributed.clone(),
            },
        )
        .await;
        let command = reviewed(&f.engine, &f.giver, "remote:publish:given", input).await;
        f.engine
            .act(command, Some(f.giver.uid.clone()))
            .await
            .unwrap();
        assert_eq!(
            store::records::get(&f.engine.store.pool, &f.stock)
                .await
                .unwrap()
                .unwrap()
                .quantity,
            store::exact::integer(20)
        );
        support::claim(
            &f.engine,
            &f.occurrence,
            &f.giver,
            engine::actions::TransferOccurrenceClaimRole::Delivery,
            "remote:delivery",
        )
        .await;
        support::claim(
            &f.engine,
            &f.occurrence,
            &f.receiver,
            engine::actions::TransferOccurrenceClaimRole::Receipt,
            "remote:receipt",
        )
        .await;
        let quote = support::settlement_preview(&f.engine, &f.occurrence, &f.giver, 10.0).await;
        support::settle_from_preview(&f.engine, &f.occurrence, &f.giver, "remote:settle", &quote)
            .await;
        let slice =
            store::transfers::occurrence_settlement_slices(&f.engine.store.pool, &f.occurrence)
                .await
                .unwrap()
                .remove(0);
        let input = request(
            &f.engine,
            &source_history,
            &source_key,
            EventSource::Settlement {
                fact: slice.evidence_fact_uid,
                supersedes: None,
            },
        )
        .await;
        f.engine.set_signer(f.giver.signer.clone()).await.unwrap();
        let preview = f
            .engine
            .act(
                action(
                    &f.giver,
                    "remote:canonical:preview",
                    Command::PreviewEvent {
                        request: input.clone(),
                    },
                ),
                None,
            )
            .await
            .unwrap()
            .data
            .unwrap()["receipt"]["document"]["value"]
            .clone();
        f.engine
            .act(
                action(
                    &f.giver,
                    "remote:canonical:publish",
                    Command::PublishEvent {
                        request: input,
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
        let complete = document(&f.engine, &source_history).await;
        let Document::Publication(complete_bundle) = &complete else {
            panic!("Expected complete publication");
        };
        assert_eq!(complete_bundle.publication.events.len(), 4);
        assert!(complete_bundle.publication.events.iter().any(|signed|matches!(&signed.event.content,PublicContent::Statement {role:Role::Giver,quantity,need:Some(_),..} if quantity.canonical()=="100")));
        assert!(complete_bundle.publication.events.iter().any(|signed|matches!(&signed.event.content,PublicContent::Settled {quantity} if quantity.value().unwrap()==10.0)));
        let minimal = serde_json::to_string(&complete).unwrap();
        assert!(!minimal.contains(&contributed));
        assert!(!minimal.contains(&setup.selection.need));
        super::ordinary_delivery::exchange(
            &f.engine,
            &f.giver,
            &source_history,
            &reader,
            &viewer,
            "remote:reader:complete",
        )
        .await;
        for node in [&setup.receiver, &reader] {
            let occurrences: i64 =
                store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_occurrence")
                    .fetch_one(&node.store.pool)
                    .await
                    .unwrap();
            let transfers: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer")
                .fetch_one(&node.store.pool)
                .await
                .unwrap();
            assert_eq!((occurrences, transfers), (0, 0));
        }
        assert_eq!(
            store::records::get(&f.engine.store.pool, &f.stock)
                .await
                .unwrap()
                .unwrap()
                .quantity,
            store::exact::integer(10)
        );
        assert_eq!(
            store::records::get(&setup.receiver.store.pool, &setup.selection.need)
                .await
                .unwrap()
                .unwrap()
                .quantity,
            store::exact::zero()
        );
    });
}

#[test]
fn reviewed_remote_correction_becomes_a_fragment_after_the_complete_bundle_is_full() {
    support::run_async_test("remote-publication-fragment-rollover", || async {
        let setup = Remote::new().await;
        let f = &setup.source;
        let binding = setup.bind().await;
        let (review, key, _) = setup.approve(&binding).await;
        let publication = setup
            .publish(&review, &key, &binding, "rollover:need")
            .await;
        let report = setup
            .receiver
            .act(
                setup.capture(
                    "rollover:report",
                    &publication,
                    Request::State {
                        role: Role::Receiver,
                        measure: setup.measure.clone(),
                        amount: "12".into(),
                        withdrawn: false,
                        need_binding: Some(binding.clone()),
                        supersedes: None,
                    },
                    Some(f.transfer.transfer.clone()),
                    None,
                ),
                Some(f.receiver.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let publication = setup
            .publish(&publication, &key, &report, "rollover:report:publish")
            .await;
        let Document::Publication(mut bundle) = document(&setup.receiver, &publication).await
        else {
            panic!("complete receipt");
        };
        let original = bundle
            .publication
            .events
            .iter()
            .find(|signed| matches!(signed.event.content, PublicContent::Statement { .. }))
            .unwrap()
            .event
            .clone();
        let mut parent = original.id().unwrap();
        for index in 0..254 {
            let mut event = original.clone();
            event.supersedes = Some(parent);
            event.source_commitment =
                nucleus::karma::canonical_hash("lince.rollover.test.v1", &index).unwrap();
            let mut signed = nucleus::transfer::receipt::SignedEvent {
                event,
                key_id: setup.receiver_owner.writer.key_id.clone(),
                signature: String::new(),
            };
            signed.signature = setup
                .receiver_owner
                .writer
                .sign_bytes(&signed.signing_bytes().unwrap());
            parent = signed.event.id().unwrap();
            bundle.publication.events.push(signed);
        }
        assert_eq!(bundle.publication.events.len(), 256);
        assert_eq!(
            import(
                &setup.receiver,
                &f.receiver,
                "rollover:fill",
                &Document::Publication(bundle)
            )
            .await,
            publication
        );
        let version = store::transfer_receipts::get(&setup.receiver.store.pool, &publication)
            .await
            .unwrap()
            .version;
        let mut capture = setup.capture(
            "rollover:correction",
            &publication,
            Request::State {
                role: Role::Receiver,
                measure: setup.measure.clone(),
                amount: "11".into(),
                withdrawn: false,
                need_binding: Some(binding),
                supersedes: Some(report),
            },
            Some(f.transfer.transfer.clone()),
            None,
        );
        if let Action::TransferReceipt {
            request: Command::RecordSource { request },
            ..
        } = &mut capture
        {
            request.expected_version = version;
        }
        let correction = setup
            .receiver
            .act(capture, Some(f.receiver.uid.clone()))
            .await
            .unwrap()
            .created
            .unwrap();
        let input = request(
            &setup.receiver,
            &publication,
            &key,
            EventSource::Snapshot {
                record: correction.clone(),
            },
        )
        .await;
        let preview = setup
            .receiver
            .act(
                action(
                    &f.receiver,
                    "rollover:preview",
                    Command::PreviewEvent { request: input },
                ),
                Some(f.receiver.uid.clone()),
            )
            .await
            .unwrap();
        assert_eq!(
            preview.data.unwrap()["receipt"]["document"]["value"]["artifact_kind"],
            "proof_fragment"
        );
        let fragment = setup
            .publish(&publication, &key, &correction, "rollover:sign")
            .await;
        assert_ne!(fragment, publication);
        let Document::ProofFragment(bundle) = document(&setup.receiver, &fragment).await else {
            panic!("signed fragment");
        };
        assert_eq!(bundle.publication.events.len(), 1);
        assert_eq!(
            bundle.publication.events[0].event.supersedes,
            Some(original.id().unwrap())
        );
        let Document::Publication(unchanged) = document(&setup.receiver, &publication).await else {
            panic!("original complete receipt");
        };
        assert_eq!(unchanged.publication.events.len(), 256);
        let canonical: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_occurrence")
            .fetch_one(&setup.receiver.store.pool)
            .await
            .unwrap();
        assert_eq!(canonical, 0);
    });
}

#[test]
fn remote_snapshot_rejects_wrong_role_stale_need_and_namespace_rewrites_without_effects() {
    support::run_async_test("remote-receipt-source-guards", || async {
        let setup = Remote::new().await;
        let f = &setup.source;
        let Document::Review(mut unsigned) = document(&setup.receiver, &setup.initial_review).await
        else {
            panic!("Expected review");
        };
        unsigned.approvals.clear();
        unsigned.scope.source.occurrence =
            nucleus::karma::canonical_hash("lince.transfer.test.v1", &"unconfirmed source")
                .unwrap();
        let unconfirmed = import(
            &setup.receiver,
            &f.receiver,
            "remote:unconfirmed:review",
            &Document::Review(unsigned),
        )
        .await;
        let attempt = setup.capture(
            "remote:unconfirmed:source",
            &unconfirmed,
            Request::BindNeed {
                selection: setup.selection.clone(),
                expected_measure: setup.measure.clone(),
                supersedes: None,
            },
            Some(f.transfer.transfer.clone()),
            None,
        );
        assert!(
            setup
                .receiver
                .act(attempt, Some(f.receiver.uid.clone()))
                .await
                .unwrap_err()
                .to_string()
                .contains("origin must approve")
        );
        let mut wrong = setup.binding("remote:wrong");
        if let Action::TransferReceipt {
            request: Command::RecordSource { request },
            ..
        } = &mut wrong
        {
            request.private_transfer = Some(nucleus::new_uid("r"));
        }
        assert_eq!(
            setup
                .receiver
                .act(wrong, Some(f.receiver.uid.clone()))
                .await
                .unwrap_err()
                .code(),
            Some("forbidden")
        );
        let mut stale = setup.binding("remote:stale");
        if let Action::TransferReceipt {
            request: Command::RecordSource { request },
            ..
        } = &mut stale
            && let Request::BindNeed { selection, .. } = &mut request.outcome
        {
            selection.expected_need_revision += 1;
        }
        assert!(
            setup
                .receiver
                .act(stale, Some(f.receiver.uid.clone()))
                .await
                .is_err()
        );
        let count: i64 =
            store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_receipt_source_record")
                .fetch_one(&setup.receiver.store.pool)
                .await
                .unwrap();
        assert_eq!(count, 0);
        let binding = setup.bind().await;
        assert!(
            setup
                .receiver
                .act(
                    Action::SetExtension {
                        target: binding.clone(),
                        namespace: nucleus::transfer::receipt_source::NAMESPACE.into(),
                        fds: json!({"requested":10000})
                    },
                    Some(f.receiver.uid.clone())
                )
                .await
                .is_err()
        );
        assert!(
            store::sqlx::query(
                "UPDATE transfer_receipt_source_record SET payload_json='{}' WHERE record_uid=?"
            )
            .bind(&binding)
            .execute(&setup.receiver.store.pool)
            .await
            .is_err()
        );
        assert!(
            setup
                .receiver
                .act(
                    setup.binding("remote:duplicate"),
                    Some(f.receiver.uid.clone())
                )
                .await
                .is_err()
        );
        let intruder = support::person(&setup.receiver, "remote.intruder").await;
        let record = import(
            &setup.receiver,
            &intruder,
            "remote:intruder:review",
            &document(&setup.receiver, &setup.initial_review).await,
        )
        .await;
        let attempt = action(
            &intruder,
            "remote:intruder:claim",
            Command::RecordSource {
                request: SourceRequest {
                    receipt: record,
                    expected_version: 1,
                    private_transfer: Some(f.transfer.transfer.clone()),
                    outcome: Request::State {
                        role: Role::Receiver,
                        measure: setup.measure.clone(),
                        amount: "1000".into(),
                        withdrawn: false,
                        need_binding: None,
                        supersedes: None,
                    },
                    public_need: None,
                },
            },
        );
        assert_eq!(
            setup.receiver.act(attempt, None).await.unwrap_err().code(),
            Some("forbidden")
        );
        let count: i64 =
            store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_receipt_source_record")
                .fetch_one(&setup.receiver.store.pool)
                .await
                .unwrap();
        assert_eq!(count, 1);
    });
}

#[test]
fn remote_reports_keep_conflicting_corrections_and_withdrawals_as_signed_branches() {
    support::run_async_test("remote-receipt-source-corrections", || async {
        let setup = Remote::new().await;
        let f = &setup.source;
        let binding = setup.bind().await;
        let (review, key, _) = setup.approve(&binding).await;
        let publication = setup
            .publish(&review, &key, &binding, "remote:branch:need")
            .await;
        let mut first = None;
        let mut branch = None;
        for (id, amount, parent, withdrawn) in [
            ("root", "12", None, false),
            ("a", "8", Some("root"), false),
            ("b", "9", Some("root"), false),
            ("withdraw", "0", Some("a"), true),
        ] {
            let previous = match parent {
                Some("root") => first.clone(),
                Some("a") => branch.clone(),
                _ => None,
            };
            let mut command = setup.capture(
                &format!("remote:branch:{id}"),
                &publication,
                Request::State {
                    role: Role::Receiver,
                    measure: setup.measure.clone(),
                    amount: amount.into(),
                    withdrawn,
                    need_binding: Some(binding.clone()),
                    supersedes: previous,
                },
                Some(f.transfer.transfer.clone()),
                None,
            );
            if let Action::TransferReceipt {
                request: Command::RecordSource { request },
                ..
            } = &mut command
            {
                request.expected_version =
                    store::transfer_receipts::get(&setup.receiver.store.pool, &publication)
                        .await
                        .unwrap()
                        .version;
            }
            setup
                .receiver
                .set_signer(f.receiver.signer.clone())
                .await
                .unwrap();
            let record = setup
                .receiver
                .act(command, Some(f.receiver.uid.clone()))
                .await
                .unwrap()
                .created
                .unwrap();
            if id == "root" {
                first = Some(record.clone());
            }
            if id == "a" {
                branch = Some(record.clone());
            }
            setup
                .publish(
                    &publication,
                    &key,
                    &record,
                    &format!("remote:branch:publish:{id}"),
                )
                .await;
        }
        let history = document(&setup.receiver, &publication).await;
        let Document::Publication(bundle) = &history else {
            panic!("Expected public history");
        };
        assert_eq!(bundle.publication.events.len(), 5);
        let verified = setup
            .receiver
            .verify_transfer_receipt(bundle, nucleus::execution::now())
            .await
            .unwrap();
        assert!(verified.heads().values().any(|heads| heads.len() == 2));
        assert!(bundle.publication.events.iter().any(|signed|matches!(&signed.event.content,PublicContent::Statement {withdrawn:true,quantity,..} if quantity.is_zero())));
        let reader = support::engine().await;
        reader
            .adopt_introduction(&f.engine.introduction().await.unwrap(), 1)
            .await
            .unwrap();
        reader
            .adopt_introduction(&setup.receiver.introduction().await.unwrap(), 1)
            .await
            .unwrap();
        reader
            .verify_transfer_receipt(bundle, nucleus::execution::now())
            .await
            .unwrap();
        let viewer = support::person(&reader, "remote.corrections.reader").await;
        let imported = super::ordinary_delivery::exchange(
            &setup.receiver,
            &f.receiver,
            &publication,
            &reader,
            &viewer,
            "remote:corrections:exchange",
        )
        .await;
        let Document::Publication(imported) = document(&reader, &imported).await else {
            panic!("Expected imported correction history");
        };
        assert_eq!(imported, *bundle);
        let verified = reader
            .verify_transfer_receipt(&imported, nucleus::execution::now())
            .await
            .unwrap();
        assert!(verified.heads().values().any(|heads| heads.len() == 2));
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
fn denied_snapshot_creation_rolls_back_the_record_fact_index_and_private_audience() {
    support::run_async_test("remote-receipt-source-policy", || async {
        let setup = Remote::new().await;
        let role = store::auth::ensure_role(&setup.receiver.store.pool, "Own receiver evidence")
            .await
            .unwrap();
        store::role_policies::set(
            &setup.receiver.store.pool,
            role,
            &json!({"read":{"all":[]},"grants":[]}),
            0,
        )
        .await
        .unwrap();
        let baseline:(i64,i64,i64)=store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM sync_op)").fetch_one(&setup.receiver.store.pool).await.unwrap();
        assert_eq!(
            setup
                .receiver
                .act(
                    setup.binding("remote:denied"),
                    Some(setup.source.receiver.uid.clone())
                )
                .await
                .unwrap_err()
                .code(),
            Some("forbidden")
        );
        let after:(i64,i64,i64)=store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM sync_op)").fetch_one(&setup.receiver.store.pool).await.unwrap();
        assert_eq!(baseline, after);
        let count: i64 =
            store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_receipt_source_record")
                .fetch_one(&setup.receiver.store.pool)
                .await
                .unwrap();
        assert_eq!(count, 0);
    });
}
