use super::*;
use nucleus::transfer::receipt_delivery::{
    Artifact, Command as DeliveryCommand, Delivery, Request, Source as DeliverySource,
};
use serde_json::{Value, json};

fn action(person: &str, id: &str, request: DeliveryCommand) -> Action {
    receipt_action(person, id, Command::Delivery { request })
}

struct Mail {
    fixture: Fixture,
    receiver: Engine,
    person: support::Person,
    reader: support::Person,
    record: String,
    root: String,
    thread: String,
    recipient: String,
    role: i64,
}

impl Mail {
    async fn new() -> Self {
        let fixture = Fixture::new(true).await;
        let sender = &fixture.reader;
        let receiver = support::engine().await;
        let person = support::person(sender, "artifact.sender").await;
        sender.set_signer(person.signer.clone()).await.unwrap();
        let role = store::auth::ensure_role(&sender.store.pool, "Artifact author")
            .await
            .unwrap();
        for operation in ["create", "read", "update"] {
            let permission =
                store::auth::ensure_permission(&sender.store.pool, "record", operation)
                    .await
                    .unwrap();
            store::auth::grant(&sender.store.pool, role, permission)
                .await
                .unwrap();
        }
        store::auth::create_credential(&sender.store.pool, &person.uid, &person.uid, "hash", role)
            .await
            .unwrap();
        let reader = support::person(&receiver, "artifact.receiver").await;
        receiver.set_signer(reader.signer.clone()).await.unwrap();
        for root in [&fixture.giver_root, &fixture.receiver_root] {
            receiver.publish_root_key(root).await.unwrap();
        }
        let recipient = sender
            .adopt_introduction(&receiver.introduction().await.unwrap(), 1)
            .await
            .unwrap();
        let origin = receiver
            .adopt_introduction(&sender.introduction().await.unwrap(), 1)
            .await
            .unwrap();
        store::organs::set_trust(&sender.store.pool, &recipient, "known")
            .await
            .unwrap();
        store::organs::set_sync_policy(&sender.store.pool, &recipient, false, false)
            .await
            .unwrap();
        store::organs::set_trust(&receiver.store.pool, &origin, "known")
            .await
            .unwrap();
        store::organs::set_sync_policy(&receiver.store.pool, &origin, false, false)
            .await
            .unwrap();
        let (root, thread) = sender
            .start_conversation(&recipient, "Reviewed artifacts")
            .await
            .unwrap();
        store::replica::accept(&sender.store.pool, &root, &recipient)
            .await
            .unwrap();
        store::replica::accept(&receiver.store.pool, &root, &origin)
            .await
            .unwrap();
        for target in [&root, &thread] {
            store::visibility::grant(&sender.store.pool, "actor", Some(&person.uid), target)
                .await
                .unwrap();
        }
        let record = sender
            .act(
                import_action(
                    &person.uid,
                    "artifact:source",
                    &Document::Publication(fixture.bundle.clone()),
                ),
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        Self {
            fixture,
            receiver,
            person,
            reader,
            record,
            root,
            thread,
            recipient,
            role,
        }
    }

    fn request(&self) -> Request {
        Request {
            source: DeliverySource::Document {
                record: self.record.clone(),
                expected_version: 1,
            },
            thread: self.thread.clone(),
            recipients: vec![self.recipient.clone()],
            expires_at: self.fixture.now.timestamp() + 3600,
        }
    }

    async fn preview(&self, request: Request) -> Value {
        self.fixture
            .reader
            .act(
                action(
                    &self.person.uid,
                    "artifact:preview",
                    DeliveryCommand::Preview { request },
                ),
                None,
            )
            .await
            .unwrap()
            .data
            .unwrap()["receipt"]["document"]["value"]
            .clone()
    }

    fn send(&self, id: &str, preview: &Value) -> Action {
        action(
            &self.person.uid,
            id,
            DeliveryCommand::Send {
                request: serde_json::from_value(preview["request"].clone()).unwrap(),
                reviewed_at: preview["reviewed_at"].as_i64().unwrap(),
                expected_projection: serde_json::from_value(preview["expected_projection"].clone())
                    .unwrap(),
            },
        )
    }

    async fn push(&self) -> usize {
        self.fixture
            .reader
            .drain_outbox(|contact, root, batch| async move {
                if contact.record_uid != self.recipient {
                    return engine::sync::Delivery::Failed("No such test destination".into());
                }
                if root.is_some() {assert!(!serde_json::to_string(&batch).unwrap().contains(&self.person.uid));}
                match match root {
                    Some(root) => self.receiver.import_grant_batch(&root, &batch).await,
                    None => self.receiver.import_op_batch(&batch).await,
                } {
                    Ok(_) => engine::sync::Delivery::Sent,
                    Err(error) => {
                        let recent:Vec<(String,String,String)>=store::sqlx::query_as("SELECT tbl,uid,field FROM sync_op WHERE organ_uid=? ORDER BY seq DESC LIMIT 4").bind(&batch.from_organ).fetch_all(&self.receiver.store.pool).await.unwrap();
                        panic!("artifact delivery: {error}; recent operations: {recent:?}");
                    },
                }
            })
            .await
            .unwrap()
    }
}

#[test]
fn ordinary_conversation_preserves_signed_fragments_without_promoting_incomplete_history() {
    support::run_async_test("receipt-fragment-delivery", || async {
        let mut mail = Mail::new().await;
        let mut bundle = mail.fixture.bundle.clone();
        bundle.publication.events = vec![bundle.publication.events[2].clone()];
        let document = Document::ProofFragment(bundle);
        mail.record = mail
            .fixture
            .reader
            .act(
                import_action(&mail.person.uid, "fragment:outgoing", &document),
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let preview = mail.preview(mail.request()).await;
        let message = mail
            .fixture
            .reader
            .act(mail.send("fragment:send", &preview), None)
            .await
            .unwrap()
            .created
            .unwrap();
        assert!(mail.push().await > 0);
        let incoming = mail
            .receiver
            .act(
                action(
                    &mail.reader.uid,
                    "fragment:inspect",
                    DeliveryCommand::Inspect {
                        message: message.clone(),
                    },
                ),
                None,
            )
            .await
            .unwrap()
            .data
            .unwrap()["receipt"]["document"]["value"]
            .clone();
        let delivery: Delivery = serde_json::from_value(incoming["artifact"].clone()).unwrap();
        assert_eq!(delivery.artifact, Artifact::Document(document.clone()));
        let saved = mail
            .receiver
            .act(
                action(
                    &mail.reader.uid,
                    "fragment:owned-import",
                    DeliveryCommand::Import {
                        message,
                        expected_projection: delivery.hash().unwrap(),
                    },
                ),
                None,
            )
            .await
            .unwrap();
        let record = saved.created.unwrap();
        assert_eq!(
            saved.data.unwrap()["receipt"]["state"],
            "authenticated_fragment"
        );
        let exported = mail
            .receiver
            .act(
                receipt_action(
                    &mail.reader.uid,
                    "fragment:export",
                    Command::ExportVersion { record, version: 1 },
                ),
                None,
            )
            .await
            .unwrap()
            .data
            .unwrap();
        let original: Document =
            serde_json::from_str(exported["receipt_export"].as_str().unwrap()).unwrap();
        assert_eq!(original, document);
        let history = mail
            .receiver
            .act(
                receipt_action(
                    &mail.reader.uid,
                    "fragment:history",
                    Command::History {
                        origin: mail.fixture.bundle.publication.scope.source.origin.clone(),
                        occurrence: mail
                            .fixture
                            .bundle
                            .publication
                            .scope
                            .source
                            .occurrence
                            .clone(),
                        before: None,
                    },
                ),
                None,
            )
            .await
            .unwrap()
            .data
            .unwrap()["receipt"]["document"]["value"]
            .clone();
        assert!(
            history["history"]["outcomes"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(history["unresolved"][0]["state"], "incomplete");
        let canonical: i64 = store::sqlx::query_scalar(
            "SELECT (SELECT COUNT(*) FROM transfer_occurrence)+(SELECT COUNT(*) FROM transfer)",
        )
        .fetch_one(&mail.receiver.store.pool)
        .await
        .unwrap();
        assert_eq!(canonical, 0);
    });
}

#[test]
fn delivery_policy_denial_rolls_back_message_audience_fact_index_and_queue() {
    support::run_async_test("receipt-delivery-denial", || async {
        let mail = Mail::new().await;
        let preview = mail.preview(mail.request()).await;
        store::role_policies::set(
            &mail.fixture.reader.store.pool,
            mail.role,
            &json!({"read":{"all":[]},"grants":[]}),
            0,
        )
        .await
        .unwrap();
        let baseline:(i64,i64,i64,i64)=store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM sync_op),(SELECT COUNT(*) FROM sync_outbox)").fetch_one(&mail.fixture.reader.store.pool).await.unwrap();
        let denied = mail
            .fixture
            .reader
            .act(
                mail.send("artifact:denied-send", &preview),
                Some(mail.person.uid.clone()),
            )
            .await
            .unwrap_err();
        assert_eq!(denied.code(), Some("forbidden"));
        let after:(i64,i64,i64,i64)=store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM sync_op),(SELECT COUNT(*) FROM sync_outbox)").fetch_one(&mail.fixture.reader.store.pool).await.unwrap();
        assert_eq!(after, baseline);
        let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record_artifact_delivery")
            .fetch_one(&mail.fixture.reader.store.pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    });
}

#[test]
fn ordinary_conversation_delivers_original_receipt_without_automatic_import_or_quantity_effects() {
    support::run_async_test("receipt-normal-delivery", || async {
        let mail = Mail::new().await;
        let preview = mail.preview(mail.request()).await;
        let send = mail.send("artifact:send", &preview);
        let invalidation = mail.fixture.reader.watch_query_changes();
        let sent = mail.fixture.reader.act(send.clone(), None).await.unwrap();
        assert!(invalidation.has_changed().unwrap());
        let message = sent.created.unwrap();
        let audit = sent.facts[0].record_uid.clone();
        assert_ne!(audit, message);
        assert_eq!(sent.facts.len(), 1);
        assert!(
            mail.fixture
                .reader
                .act(send, None)
                .await
                .unwrap()
                .facts
                .is_empty()
        );
        let failed = mail
            .fixture
            .reader
            .drain_outbox(|_, _, _| async { engine::sync::Delivery::Failed("offline".into()) })
            .await
            .unwrap();
        assert_eq!(failed, 0);
        let retried = mail
            .fixture
            .reader
            .act(
                action(
                    &mail.person.uid,
                    "artifact:retry",
                    DeliveryCommand::Retry {
                        message: message.clone(),
                    },
                ),
                None,
            )
            .await
            .unwrap();
        assert!(retried.facts.is_empty());
        assert!(mail.push().await > 0);
        assert!(
            store::records::get(&mail.receiver.store.pool, &audit)
                .await
                .unwrap()
                .is_none()
        );
        let query:protein::Protein=serde_json::from_value(json!({"source":"record","where":[{"uid_eq":mail.root}],"fields":["uid","threads"],"include":{"threads":{"messages_limit":64}},"limit":1})).unwrap();
        let hierarchy = protein::execute(&mail.receiver.store, &query)
            .await
            .unwrap();
        assert!(
            hierarchy
                .iter()
                .flat_map(|row| row["threads"].as_array().into_iter().flatten())
                .flat_map(|thread| thread["messages"].as_array().into_iter().flatten())
                .any(|row| row["uid"] == message)
        );
        let inbox = mail
            .receiver
            .act(
                action(
                    &mail.reader.uid,
                    "artifact:inbox",
                    DeliveryCommand::Inbox { before: None },
                ),
                None,
            )
            .await
            .unwrap()
            .data
            .unwrap();
        assert!(
            inbox["receipt"]["document"]["value"]["messages"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry["message"] == message)
        );
        let incoming = mail
            .receiver
            .act(
                action(
                    &mail.reader.uid,
                    "artifact:inspect",
                    DeliveryCommand::Inspect {
                        message: message.clone(),
                    },
                ),
                None,
            )
            .await
            .unwrap()
            .data
            .unwrap()["receipt"]["document"]["value"]
            .clone();
        assert_eq!(incoming["status"]["verification"]["state"], "verified");
        let delivery: Delivery = serde_json::from_value(incoming["artifact"].clone()).unwrap();
        assert_eq!(
            delivery.artifact,
            Artifact::Document(Document::Publication(mail.fixture.bundle.clone()))
        );
        assert!(
            !serde_json::to_string(&delivery)
                .unwrap()
                .contains(&mail.record)
        );
        let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_receipt_record")
            .fetch_one(&mail.receiver.store.pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
        let import = action(
            &mail.reader.uid,
            "artifact:import",
            DeliveryCommand::Import {
                message: message.clone(),
                expected_projection: delivery.hash().unwrap(),
            },
        );
        let saved = mail.receiver.act(import.clone(), None).await.unwrap();
        assert_eq!(
            saved.data.unwrap()["receipt"]["document"],
            json!(Document::Publication(mail.fixture.bundle.clone()))
        );
        assert!(
            mail.receiver
                .act(import, None)
                .await
                .unwrap()
                .facts
                .is_empty()
        );
        let canonical: i64 = store::sqlx::query_scalar(
            "SELECT (SELECT COUNT(*) FROM transfer_occurrence)+(SELECT COUNT(*) FROM transfer)",
        )
        .fetch_one(&mail.receiver.store.pool)
        .await
        .unwrap();
        assert_eq!(canonical, 0);
        let outsider = nucleus::new_uid("r");
        store::organs::add_contact(
            &mail.receiver.store.pool,
            &outsider,
            None,
            "Unapproved onward reader",
            "http://onward.test",
            1,
        )
        .await
        .unwrap();
        store::organs::set_trust(&mail.receiver.store.pool, &outsider, "known")
            .await
            .unwrap();
        store::replica::accept(&mail.receiver.store.pool, &mail.root, &outsider)
            .await
            .unwrap();
        let onward = mail
            .receiver
            .export_conversation_page(&outsider, &mail.root, &[], 2000)
            .await
            .unwrap();
        assert!(!onward.batch.ops.iter().any(|op| {
            op.uid == message
                || op
                    .fact
                    .as_ref()
                    .is_some_and(|fact| fact.record_uid == message)
        }));
        assert_eq!(mail.push().await, 0);
        assert!(
            store::records::set_extension(
                &mail.fixture.reader.store.pool,
                &message,
                "lince.message-data.0.0",
                &json!({"data":"changed"})
            )
            .await
            .is_err()
        );
    });
}

#[test]
fn private_reviews_and_changed_conversation_audiences_cannot_escape_the_reviewed_recipients() {
    support::run_async_test("receipt-audience-review", || async {
        let mail = Mail::new().await;
        let preview = mail.preview(mail.request()).await;
        store::sqlx::query("UPDATE record SET slug='delivery.thread' WHERE uid=?")
            .bind(&mail.thread)
            .execute(&mail.fixture.reader.store.pool)
            .await
            .unwrap();
        let mut alias = mail.request();
        alias.thread = "delivery.thread".into();
        assert!(
            mail.fixture
                .reader
                .act(
                    action(
                        &mail.person.uid,
                        "artifact:alias",
                        DeliveryCommand::Preview { request: alias }
                    ),
                    None
                )
                .await
                .is_err()
        );
        let outsider = nucleus::new_uid("r");
        store::organs::add_contact(
            &mail.fixture.reader.store.pool,
            &outsider,
            None,
            "Unreviewed Organ",
            "http://extra.test",
            1,
        )
        .await
        .unwrap();
        store::organs::set_trust(&mail.fixture.reader.store.pool, &outsider, "known")
            .await
            .unwrap();
        store::organs::set_sync_policy(&mail.fixture.reader.store.pool, &outsider, true, true)
            .await
            .unwrap();
        store::replica::accept(&mail.fixture.reader.store.pool, &mail.root, &outsider)
            .await
            .unwrap();
        assert!(
            mail.fixture
                .reader
                .act(mail.send("artifact:changed-audience", &preview), None)
                .await
                .is_err()
        );
        store::replica::revoke(&mail.fixture.reader.store.pool, &mail.root, &outsider)
            .await
            .unwrap();
        let source = Document::Review(Review {
            person_approvals: Vec::new(),
            scope: mail.fixture.bundle.publication.scope.clone(),
            approvals: vec![],
            authorities: mail.fixture.bundle.authorities.clone(),
        });
        let review = mail
            .fixture
            .reader
            .act(
                import_action(&mail.person.uid, "artifact:private-review", &source),
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let mut request = mail.request();
        request.source = DeliverySource::Document {
            record: review,
            expected_version: 1,
        };
        assert!(
            mail.fixture
                .reader
                .act(
                    action(
                        &mail.person.uid,
                        "artifact:wrong-private-audience",
                        DeliveryCommand::Preview { request }
                    ),
                    None
                )
                .await
                .is_err()
        );
        let sent = mail
            .fixture
            .reader
            .act(mail.send("artifact:send-after-review", &preview), None)
            .await
            .unwrap()
            .created
            .unwrap();
        store::replica::accept(&mail.fixture.reader.store.pool, &mail.root, &outsider)
            .await
            .unwrap();
        let permitted = mail
            .fixture
            .reader
            .export_conversation_page(&mail.recipient, &mail.root, &[], 2000)
            .await
            .unwrap();
        assert!(permitted.batch.ops.iter().any(|op| op.uid == sent));
        let withheld = mail
            .fixture
            .reader
            .export_conversation_page(&outsider, &mail.root, &[], 2000)
            .await
            .unwrap();
        assert!(!withheld.batch.ops.iter().any(
            |op| op.uid == sent || op.fact.as_ref().is_some_and(|fact| fact.record_uid == sent)
        ));
        let mut leaked = false;
        mail.fixture
            .reader
            .drain_outbox(|contact, _, batch| {
                if contact.record_uid == outsider
                    && batch.ops.iter().any(|op| {
                        op.uid == sent
                            || op.fact.as_ref().is_some_and(|fact| fact.record_uid == sent)
                    })
                {
                    leaked = true;
                }
                async { engine::sync::Delivery::Sent }
            })
            .await
            .unwrap();
        assert!(!leaked);
    });
}

#[test]
fn cancellation_expiry_and_revocation_stop_retries_without_rewriting_the_artifact() {
    support::run_async_test("receipt-delivery-lifetime", || async {
        let mail = Mail::new().await;
        let preview = mail.preview(mail.request()).await;
        let sent = mail
            .fixture
            .reader
            .act(mail.send("artifact:cancel-send", &preview), None)
            .await
            .unwrap()
            .created
            .unwrap();
        mail.fixture
            .reader
            .act(
                action(
                    &mail.person.uid,
                    "artifact:cancel",
                    DeliveryCommand::Cancel {
                        message: sent.clone(),
                    },
                ),
                None,
            )
            .await
            .unwrap();
        assert!(
            mail.fixture
                .reader
                .act(
                    action(
                        &mail.person.uid,
                        "artifact:cancel-retry",
                        DeliveryCommand::Retry {
                            message: sent.clone()
                        }
                    ),
                    None
                )
                .await
                .is_err()
        );
        mail.push().await;
        assert!(
            store::records::get(&mail.receiver.store.pool, &sent)
                .await
                .unwrap()
                .is_none()
        );
        let mut request = mail.request();
        request.expires_at = mail.fixture.now.timestamp() - 1;
        assert!(
            mail.fixture
                .reader
                .act(
                    action(
                        &mail.person.uid,
                        "artifact:expired",
                        DeliveryCommand::Preview { request }
                    ),
                    None
                )
                .await
                .is_err()
        );
        let sent = mail
            .fixture
            .reader
            .act(mail.send("artifact:revoke-send", &preview), None)
            .await
            .unwrap()
            .created
            .unwrap();
        let (key, signature) = mail
            .fixture
            .reader
            .revocation_certificate(&mail.fixture.receiver_root);
        mail.fixture
            .reader
            .adopt_revocation(&mail.fixture.receiver_root.actor_uid, &key, &signature)
            .await
            .unwrap();
        assert!(
            mail.fixture
                .reader
                .act(
                    action(
                        &mail.person.uid,
                        "artifact:revoked-retry",
                        DeliveryCommand::Retry {
                            message: sent.clone()
                        }
                    ),
                    None
                )
                .await
                .is_err()
        );
        mail.push().await;
        assert!(
            store::records::get(&mail.receiver.store.pool, &sent)
                .await
                .unwrap()
                .is_none()
        );
    });
}

#[test]
fn delivered_receipt_import_rechecks_policy_and_rolls_back_without_consuming_the_message() {
    support::run_async_test("receipt-delivery-import-policy", || async {
        let mail = Mail::new().await;
        let preview = mail.preview(mail.request()).await;
        let message = mail
            .fixture
            .reader
            .act(mail.send("artifact:import-policy:send", &preview), None)
            .await
            .unwrap()
            .created
            .unwrap();
        assert!(mail.push().await > 0);
        let role = store::auth::ensure_role(&mail.receiver.store.pool, "Artifact importer")
            .await
            .unwrap();
        for operation in ["read", "create"] {
            let permission =
                store::auth::ensure_permission(&mail.receiver.store.pool, "record", operation)
                    .await
                    .unwrap();
            store::auth::grant(&mail.receiver.store.pool, role, permission)
                .await
                .unwrap();
        }
        store::auth::create_credential(
            &mail.receiver.store.pool,
            &mail.reader.uid,
            &mail.reader.uid,
            "hash",
            role,
        )
        .await
        .unwrap();
        store::role_policies::set(
            &mail.receiver.store.pool,
            role,
            &json!({"read":{"all":[]},"grants":[]}),
            0,
        )
        .await
        .unwrap();
        let inspect = action(
            &mail.reader.uid,
            "artifact:import-policy:inspect",
            DeliveryCommand::Inspect {
                message: message.clone(),
            },
        );
        assert_eq!(
            mail.receiver
                .act(inspect.clone(), Some(mail.reader.uid.clone()))
                .await
                .unwrap_err()
                .code(),
            Some("forbidden")
        );
        store::visibility::grant(
            &mail.receiver.store.pool,
            "actor",
            Some(&mail.reader.uid),
            &message,
        )
        .await
        .unwrap();
        let inspected = mail
            .receiver
            .act(inspect.clone(), Some(mail.reader.uid.clone()))
            .await
            .unwrap();
        let value = inspected.data.unwrap()["receipt"]["document"]["value"].clone();
        let import = action(
            &mail.reader.uid,
            "artifact:import-policy:import",
            DeliveryCommand::Import {
                message,
                expected_projection: serde_json::from_value(value["hash"].clone()).unwrap(),
            },
        );
        let baseline: (i64, i64, i64, i64, i64) = store::sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM sync_op),(SELECT COUNT(*) FROM sync_outbox),(SELECT COUNT(*) FROM transfer_receipt_version)",
        )
        .fetch_one(&mail.receiver.store.pool)
        .await
        .unwrap();
        let denied = mail
            .receiver
            .act(import.clone(), Some(mail.reader.uid.clone()))
            .await
            .unwrap_err();
        assert_eq!(denied.code(), Some("forbidden"));
        let after: (i64, i64, i64, i64, i64) = store::sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM sync_op),(SELECT COUNT(*) FROM sync_outbox),(SELECT COUNT(*) FROM transfer_receipt_version)",
        )
        .fetch_one(&mail.receiver.store.pool)
        .await
        .unwrap();
        assert_eq!(after, baseline);
        let unchanged = mail
            .receiver
            .act(inspect, Some(mail.reader.uid.clone()))
            .await
            .unwrap();
        assert_eq!(
            unchanged.data.unwrap()["receipt"]["document"]["value"]["artifact"],
            value["artifact"]
        );
        let saved = mail.receiver.act(import, None).await.unwrap();
        assert_eq!(saved.facts.len(), 1);
        assert!(saved.created.is_some());
    });
}

#[test]
fn contact_blocks_and_conversation_revocation_stop_reviewed_and_queued_deliveries() {
    support::run_async_test("receipt-delivery-contact-policy", || async {
        let mail = Mail::new().await;
        let preview = mail.preview(mail.request()).await;
        let message = mail
            .fixture
            .reader
            .act(mail.send("artifact:contact-policy:send", &preview), None)
            .await
            .unwrap()
            .created
            .unwrap();
        for trust in ["unknown", "blocked"] {
            store::organs::set_trust(&mail.fixture.reader.store.pool, &mail.recipient, trust)
                .await
                .unwrap();
            assert!(
                mail.fixture
                    .reader
                    .act(mail.send("artifact:contact-policy:stale", &preview), None)
                    .await
                    .is_err()
            );
            assert!(
                mail.fixture
                    .reader
                    .act(
                        action(
                            &mail.person.uid,
                            "artifact:contact-policy:retry",
                            DeliveryCommand::Retry {
                                message: message.clone()
                            }
                        ),
                        None
                    )
                    .await
                    .is_err()
            );
            let page = mail
                .fixture
                .reader
                .export_conversation_page(&mail.recipient, &mail.root, &[], 2000)
                .await;
            assert!(page.is_err() || !page.unwrap().batch.ops.iter().any(|op| op.uid == message));
        }
        store::organs::set_trust(&mail.fixture.reader.store.pool, &mail.recipient, "known")
            .await
            .unwrap();
        store::replica::revoke(&mail.fixture.reader.store.pool, &mail.root, &mail.recipient)
            .await
            .unwrap();
        assert!(
            mail.fixture
                .reader
                .export_conversation_page(&mail.recipient, &mail.root, &[], 2000)
                .await
                .is_err()
        );
        assert!(
            mail.fixture
                .reader
                .act(
                    action(
                        &mail.person.uid,
                        "artifact:contact-policy:revoked-retry",
                        DeliveryCommand::Retry {
                            message: message.clone()
                        }
                    ),
                    None
                )
                .await
                .is_err()
        );
        assert_eq!(mail.push().await, 0);
        assert!(
            store::records::get(&mail.receiver.store.pool, &message)
                .await
                .unwrap()
                .is_none()
        );
        let queued: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM sync_outbox WHERE contact_organ=? AND seq IN (SELECT seq FROM sync_op WHERE replica_root=?)")
            .bind(&mail.recipient)
            .bind(&mail.root)
            .fetch_one(&mail.fixture.reader.store.pool)
            .await
            .unwrap();
        assert_eq!(queued, 0);
    });
}

#[test]
fn inbox_pages_bound_candidates_keep_hidden_positions_opaque_and_recheck_permissions() {
    support::run_async_test("receipt-inbox-pages", || async {
        let node = support::engine().await;
        let reader = support::person(&node, "inbox.reader").await;
        let other = support::person(&node, "inbox.other").await;
        let organ = store::organs::local(&node.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let role = store::auth::ensure_role(&node.store.pool, "Inbox reader")
            .await
            .unwrap();
        let permission = store::auth::ensure_permission(&node.store.pool, "record", "read")
            .await
            .unwrap();
        store::auth::grant(&node.store.pool, role, permission)
            .await
            .unwrap();
        for person in [&reader, &other] {
            store::auth::create_credential(
                &node.store.pool,
                &person.uid,
                &person.uid,
                "hash",
                role,
            )
            .await
            .unwrap();
        }
        let now = nucleus::execution::now();
        let mut ids = Vec::new();
        let mut tx = store::write_tx(&node.store.pool).await.unwrap();
        for index in 0..340 {
            let uid = nucleus::new_uid("r");
            store::records::create_with_uid_on(
                &mut tx,
                &uid,
                store::records::NewRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Message,
                    head: "Artifact candidate",
                    body: "",
                    quantity: store::exact::one(),
                },
                &organ,
                None,
            )
            .await
            .unwrap();
            store::records::set_extension_on(
                &mut tx,
                &uid,
                "lince.message-content",
                &json!({"parts":[{"mime_type":nucleus::transfer::receipt_delivery::MIME}]}),
            )
            .await
            .unwrap();
            store::sqlx::query("UPDATE record SET created_at=? WHERE uid=?")
                .bind((now + chrono::Duration::milliseconds(index)).to_rfc3339())
                .bind(&uid)
                .execute(&mut *tx)
                .await
                .unwrap();
            store::data_visibility::ensure_private_for_person_on(
                &mut tx,
                &uid,
                nucleus::visibility::Data::Record,
                Some(if index < 80 { &reader.uid } else { &other.uid }),
            )
            .await
            .unwrap();
            ids.push(uid);
        }
        tx.commit().await.unwrap();
        assert_eq!(
            store::artifact_deliveries::inbox_page(&node.store.pool, None)
                .await
                .unwrap()
                .len(),
            257
        );
        let page = |before: Option<String>| {
            action(&reader.uid, "inbox:page", DeliveryCommand::Inbox { before })
        };
        let first = node
            .act(page(None), Some(reader.uid.clone()))
            .await
            .unwrap()
            .data
            .unwrap()["receipt"]["document"]["value"]
            .clone();
        assert!(first["messages"].as_array().unwrap().is_empty());
        assert_eq!(first["partial"], true);
        let cursor = first["next_before"].as_str().unwrap().to_owned();
        for hidden in ids.iter().skip(80) {
            assert!(!first.to_string().contains(hidden));
        }
        assert!(
            node.act(
                action(
                    &other.uid,
                    "inbox:stolen-cursor",
                    DeliveryCommand::Inbox {
                        before: Some(cursor.clone())
                    }
                ),
                Some(other.uid.clone())
            )
            .await
            .is_err()
        );
        let second = node
            .act(page(Some(cursor)), Some(reader.uid.clone()))
            .await
            .unwrap()
            .data
            .unwrap()["receipt"]["document"]["value"]
            .clone();
        let second_ids: Vec<_> = second["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["message"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(
            second_ids,
            ids[16..80].iter().rev().cloned().collect::<Vec<_>>()
        );
        let cursor = second["next_before"].as_str().unwrap().to_owned();
        let saved = store::data_visibility::policy(
            &node.store.pool,
            &ids[15],
            nucleus::visibility::Data::Record,
        )
        .await
        .unwrap()
        .unwrap();
        let mut tx = store::write_tx(&node.store.pool).await.unwrap();
        store::data_visibility::save_on(
            &mut tx,
            &ids[15],
            nucleus::visibility::Data::Record,
            None,
            &nucleus::visibility::Policy {
                include: vec![nucleus::visibility::Condition {
                    persons: vec![other.uid.clone()],
                    ..Default::default()
                }],
                exclude: vec![],
            },
            saved.revision,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let third = node
            .act(page(Some(cursor)), Some(reader.uid.clone()))
            .await
            .unwrap()
            .data
            .unwrap()["receipt"]["document"]["value"]
            .clone();
        let third_ids: Vec<_> = third["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["message"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(
            third_ids,
            ids[..15].iter().rev().cloned().collect::<Vec<_>>()
        );
        assert_eq!(third["partial"], false);
        assert!(third["next_before"].is_null());
        assert!(
            node.act(page(Some("x".repeat(2049))), Some(reader.uid.clone()))
                .await
                .is_err()
        );
        assert!(
            node.act(page(Some("malformed".into())), Some(reader.uid.clone()))
                .await
                .is_err()
        );
    });
}

#[test]
fn sealed_receipt_mail_keeps_original_bytes_and_holds_when_record_permissions_change() {
    support::run_async_test("receipt-sealed-delivery-policy", || async {
        let mail = Mail::new().await;
        let source = &mail.fixture.reader;
        let own_organ = store::organs::local(&source.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let own_cell = store::cells::local(&source.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let recipient_cell = store::cells::local(&mail.receiver.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let root = Signer::generate(&own_organ, ROOT_KEY_ID);
        let writer = Signer::generate(&own_organ, &engine::roster::cell_key_id(&own_cell));
        source.publish_root_key(&root).await.unwrap();
        source.set_organ_signer(writer.clone()).await.unwrap();
        let source_roster = source
            .publish_roster(
                &root,
                vec![engine::roster::CellEntry {
                    cell_uid: own_cell.clone(),
                    node_id: iroh::SecretKey::generate().public().to_string(),
                    label: "Sender".into(),
                    operational_key: writer.public_key_b64(),
                    sealing_key: None,
                    front_door: false,
                    capabilities: engine::roster::full_capabilities(),
                }],
            )
            .await
            .unwrap();
        let recipient_root = Signer::generate(&mail.recipient, ROOT_KEY_ID);
        let recipient_writer = Signer::generate(
            &mail.recipient,
            &engine::roster::cell_key_id(&recipient_cell),
        );
        let (secret, sealing) = engine::seal::generate(&recipient_cell, 1, "2099-01-01T00:00:00Z");
        mail.receiver
            .publish_root_key(&recipient_root)
            .await
            .unwrap();
        mail.receiver
            .set_organ_signer(recipient_writer.clone())
            .await
            .unwrap();
        let recipient_roster = mail
            .receiver
            .publish_roster(
                &recipient_root,
                vec![engine::roster::CellEntry {
                    cell_uid: recipient_cell,
                    node_id: iroh::SecretKey::generate().public().to_string(),
                    label: "Receiver".into(),
                    operational_key: recipient_writer.public_key_b64(),
                    sealing_key: Some(sealing.clone()),
                    front_door: false,
                    capabilities: engine::roster::full_capabilities(),
                }],
            )
            .await
            .unwrap();
        source
            .adopt_introduction(&mail.receiver.introduction().await.unwrap(), 1)
            .await
            .unwrap();
        mail.receiver
            .adopt_introduction(&source.introduction().await.unwrap(), 1)
            .await
            .unwrap();
        source.adopt_roster(&recipient_roster).await.unwrap();
        mail.receiver.adopt_roster(&source_roster).await.unwrap();
        store::organs::set_mode(&source.store.pool, &mail.recipient, "auto")
            .await
            .unwrap();
        let preview = mail.preview(mail.request()).await;
        let message = source
            .act(mail.send("artifact:sealed:send", &preview), None)
            .await
            .unwrap()
            .created
            .unwrap();
        let page = source
            .export_conversation_page(&mail.recipient, &mail.root, &[], 2000)
            .await
            .unwrap();
        let queued = source
            .prepare_outgoing_mail(&mail.recipient, Some(&mail.root), &page.batch)
            .await
            .unwrap();
        source
            .act(
                Action::CreateRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: "Unrelated private context".into(),
                    body: "Not part of the outgoing Message".into(),
                    quantity: 0.0,
                },
                Some(mail.person.uid.clone()),
            )
            .await
            .unwrap();
        let unchanged = source
            .prepare_outgoing_mail(&mail.recipient, Some(&mail.root), &page.batch)
            .await
            .unwrap();
        assert_eq!(unchanged.uid, queued.uid);
        assert!(
            store::sqlx::query("UPDATE mailbox_outbox_scope SET scope_json='{}' WHERE uid=?")
                .bind(&queued.uid)
                .execute(&source.store.pool)
                .await
                .is_err()
        );
        let sealed: engine::seal::SealedBundle = serde_json::from_str(&queued.body).unwrap();
        let key = engine::seal::verifying_key(&writer.public_key_b64()).unwrap();
        let opened =
            engine::seal::open(&sealed, &key, &[(sealing.key_id.clone(), secret)]).unwrap();
        assert_eq!(opened.root.as_deref(), Some(mail.root.as_str()));
        let bytes = serde_json::to_string(&opened.batch).unwrap();
        assert!(!bytes.contains(&mail.person.uid));
        assert!(!bytes.contains(&mail.record));
        assert!(opened.batch.ops.iter().any(|op| op.uid == message));
        mail.receiver.import_mailed_batch(&opened).await.unwrap();
        let receipt_count: i64 =
            store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_receipt_record")
                .fetch_one(&mail.receiver.store.pool)
                .await
                .unwrap();
        assert_eq!(receipt_count, 0);
        use base64::Engine as _;
        let blob = store::records::create_in_root(
            &source.store.pool,
            store::records::NewRecord {
                slug: None,
                kind: nucleus::RecordKind::Message,
                head: "Large ordinary attachment",
                body: "",
                quantity: store::exact::one(),
            },
            Some(&mail.root),
        )
        .await
        .unwrap();
        let parts = vec![nucleus::message::MessagePart::Attachment {
            name: "payload.bin".into(),
            mime_type: "application/octet-stream".into(),
            data: base64::engine::general_purpose::STANDARD.encode(vec![42u8; 950_000]),
        }];
        let mut tx = store::write_tx(&source.store.pool).await.unwrap();
        store::message_content::save_on(&mut tx, &blob.uid, &parts)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let target = &mail.receiver;
        let recipient = &mail.recipient;
        let key = &key;
        let key_id = &sealing.key_id;
        let mut attempts = 0;
        let failed = source
            .drain_outbox(|_, root, batch| {
                attempts += 1;
                let offline = attempts == 2;
                async move {
                    if offline {
                        return engine::sync::Delivery::Failed("Interrupted between pages".into());
                    }
                    assert!(
                        serde_json::to_vec(&batch).unwrap().len()
                            <= engine::mailbox::MAX_BUNDLE_BYTES / 2
                    );
                    let envelope = source
                        .prepare_outgoing_mail(recipient, root.as_deref(), &batch)
                        .await
                        .unwrap();
                    assert!(envelope.body.len() <= engine::mailbox::MAX_BUNDLE_BYTES);
                    let sealed = serde_json::from_str(&envelope.body).unwrap();
                    let opened =
                        engine::seal::open(&sealed, key, &[(key_id.clone(), secret)]).unwrap();
                    target.import_mailed_batch(&opened).await.unwrap();
                    assert!(target.batch_is_saved(&opened.batch).await.unwrap());
                    engine::sync::Delivery::Sent
                }
            })
            .await
            .unwrap();
        assert_eq!(failed, 0);
        assert_eq!(attempts, 2);
        assert!(
            store::message_content::load(&target.store.pool, &blob.uid)
                .await
                .map_or(true, |parts| parts.is_empty())
        );
        let mut pages = 0;
        let delivered = source
            .drain_outbox(|_, root, batch| {
                pages += 1;
                async move {
                    assert!(
                        serde_json::to_vec(&batch).unwrap().len()
                            <= engine::mailbox::MAX_BUNDLE_BYTES / 2
                    );
                    let envelope = source
                        .prepare_outgoing_mail(recipient, root.as_deref(), &batch)
                        .await
                        .unwrap();
                    assert!(envelope.body.len() <= engine::mailbox::MAX_BUNDLE_BYTES);
                    let sealed = serde_json::from_str(&envelope.body).unwrap();
                    let opened =
                        engine::seal::open(&sealed, key, &[(key_id.clone(), secret)]).unwrap();
                    target.import_mailed_batch(&opened).await.unwrap();
                    assert!(target.batch_is_saved(&opened.batch).await.unwrap());
                    engine::sync::Delivery::Sent
                }
            })
            .await
            .unwrap();
        assert_eq!(delivered, 1);
        assert!(pages > 1);
        assert!(pages < 8);
        assert_eq!(
            serde_json::to_value(
                store::message_content::load(&target.store.pool, &blob.uid)
                    .await
                    .unwrap()
            )
            .unwrap(),
            serde_json::to_value(&parts).unwrap()
        );
        store::visibility::set_hidden_from_organ(
            &source.store.pool,
            &mail.recipient,
            &message,
            true,
        )
        .await
        .unwrap();
        assert!(
            source
                .prepare_outgoing_mail(&mail.recipient, Some(&mail.root), &page.batch)
                .await
                .is_err()
        );
        let source = std::sync::Arc::new(mail.fixture.reader);
        let wire = engine::wire::Wire::bind(
            source.clone(),
            iroh::SecretKey::generate(),
            engine::wire::Reach::Local,
        )
        .await
        .unwrap();
        assert_eq!(wire.retry_saved_mail_once().await.unwrap(), 0);
        let error: Option<String> =
            store::sqlx::query_scalar("SELECT error FROM mailbox_outbox WHERE uid=?")
                .bind(&queued.uid)
                .fetch_one(&source.store.pool)
                .await
                .unwrap();
        assert!(
            error
                .unwrap()
                .contains("current sharing permissions differ")
        );
        let retained = store::mailbox::outbox::get(&source.store.pool, &queued.intent)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retained.body, queued.body);
        assert!(
            store::mailbox::outbox::receipts(&source.store.pool, &queued.uid)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            store::records::get(&mail.receiver.store.pool, &message)
                .await
                .unwrap()
                .is_some()
        );
        let canonical: i64 = store::sqlx::query_scalar(
            "SELECT (SELECT COUNT(*) FROM transfer)+(SELECT COUNT(*) FROM transfer_occurrence)",
        )
        .fetch_one(&mail.receiver.store.pool)
        .await
        .unwrap();
        assert_eq!(canonical, 0);
    });
}

#[test]
fn authenticated_retrieval_routes_keep_fixed_audiences_and_reject_invalid_arrivals() {
    support::run_async_test("receipt-wire-retrieval", || async {
        use base64::Engine as _;
        use engine::wire::{ALPN_THREAD, Reach, Wire, WireRequest, WireResponse};
        use std::sync::Arc;
        async fn request(
            wire: &Wire,
            addr: &iroh::EndpointAddr,
            body: &WireRequest,
        ) -> WireResponse {
            tokio::time::timeout(
                std::time::Duration::from_secs(15),
                wire.request(addr.clone(), ALPN_THREAD, body),
            )
            .await
            .unwrap()
            .unwrap()
        }
        let mail = Mail::new().await;
        let preview = mail.preview(mail.request()).await;
        let message = mail
            .fixture
            .reader
            .act(mail.send("artifact:wire:send", &preview), None)
            .await
            .unwrap()
            .created
            .unwrap();
        let own_organ = store::organs::local(&mail.fixture.reader.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let source = Arc::new(mail.fixture.reader);
        let receiver = Arc::new(mail.receiver);
        let server = Arc::new(
            Wire::bind(source.clone(), iroh::SecretKey::generate(), Reach::Local)
                .await
                .unwrap(),
        );
        let client = Arc::new(
            Wire::bind(receiver.clone(), iroh::SecretKey::generate(), Reach::Local)
                .await
                .unwrap(),
        );
        store::organs::set_node_id(
            &source.store.pool,
            &mail.recipient,
            Some(&client.node_id().to_string()),
        )
        .await
        .unwrap();
        store::organs::set_node_id(
            &receiver.store.pool,
            &own_organ,
            Some(&server.node_id().to_string()),
        )
        .await
        .unwrap();
        let port = server
            .endpoint()
            .bound_sockets()
            .into_iter()
            .next()
            .unwrap()
            .port();
        let addr = iroh::EndpointAddr::new(server.node_id())
            .with_ip_addr(std::net::SocketAddr::from(([127, 0, 0, 1], port)));
        let client_port = client
            .endpoint()
            .bound_sockets()
            .into_iter()
            .next()
            .unwrap()
            .port();
        let client_addr = iroh::EndpointAddr::new(client.node_id())
            .with_ip_addr(std::net::SocketAddr::from(([127, 0, 0, 1], client_port)));
        let serving = tokio::spawn({
            let server = server.clone();
            async move { server.serve().await }
        });
        let receiving = tokio::spawn({
            let client = client.clone();
            async move { client.serve().await }
        });
        let history = WireRequest::FetchGrantOpsSince {
            root: mail.root.clone(),
            vector: vec![],
            limit: 2000,
        };
        let WireResponse::Ops {
            from_organ, ops, ..
        } = request(&client, &addr, &history).await
        else {
            panic!("Expected permitted conversation history");
        };
        assert!(ops.iter().any(|op| op.uid == message));
        let batch = engine::sync::OpBatch { from_organ, ops };
        assert!(
            !serde_json::to_string(&batch)
                .unwrap()
                .contains(&mail.person.uid)
        );
        receiver
            .import_grant_batch(&mail.root, &batch)
            .await
            .unwrap();
        let before: (i64, i64) = store::sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact)",
        )
        .fetch_one(&receiver.store.pool)
        .await
        .unwrap();
        receiver
            .import_grant_batch(&mail.root, &batch)
            .await
            .unwrap();
        let after: (i64, i64) = store::sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact)",
        )
        .fetch_one(&receiver.store.pool)
        .await
        .unwrap();
        assert_eq!(before, after);
        let probe = store::records::create_in_root(
            &source.store.pool,
            store::records::NewRecord {
                slug: None,
                kind: nucleus::RecordKind::Message,
                head: "Receipt reference",
                body: "",
                quantity: store::exact::one(),
            },
            Some(&mail.root),
        )
        .await
        .unwrap();
        let predicate = store::concepts::create(&source.store.pool, "receipt-context", &[])
            .await
            .unwrap();
        store::assertions::assert(
            &source.store.pool,
            store::assertions::NewAssertion {
                subject_uid: &probe.uid,
                predicate_uid: &predicate,
                object_uid: Some(&message),
                role: store::assertions::AssertionRole::Ordinary,
                quantity: None,
                unit_uid: None,
                asserted_by: Some(&own_organ),
            },
        )
        .await
        .unwrap();
        let reference = WireRequest::FetchReference {
            root: mail.root.clone(),
            record: message.clone(),
        };
        assert!(
            matches!(request(&client,&addr,&reference).await,WireResponse::Reference {row} if row["uid"]==message)
        );
        let general = WireRequest::FetchOpsSince {
            vector: vec![],
            limit: 2000,
        };
        assert!(
            matches!(request(&client,&addr,&general).await,WireResponse::Ops {ops,..} if ops.is_empty())
        );
        let outsider = Arc::new(support::engine().await);
        let outsider_wire = Wire::bind(outsider.clone(), iroh::SecretKey::generate(), Reach::Local)
            .await
            .unwrap();
        let outsider_uid = source
            .adopt_introduction(&outsider.introduction().await.unwrap(), 1)
            .await
            .unwrap();
        outsider
            .adopt_introduction(&source.introduction().await.unwrap(), 1)
            .await
            .unwrap();
        store::organs::set_trust(&source.store.pool, &outsider_uid, "known")
            .await
            .unwrap();
        store::organs::set_trust(&outsider.store.pool, &own_organ, "known")
            .await
            .unwrap();
        store::organs::set_sync_policy(&source.store.pool, &outsider_uid, false, false)
            .await
            .unwrap();
        store::organs::set_sync_policy(&outsider.store.pool, &own_organ, false, false)
            .await
            .unwrap();
        store::organs::set_node_id(
            &source.store.pool,
            &outsider_uid,
            Some(&outsider_wire.node_id().to_string()),
        )
        .await
        .unwrap();
        store::organs::set_node_id(
            &outsider.store.pool,
            &own_organ,
            Some(&server.node_id().to_string()),
        )
        .await
        .unwrap();
        store::replica::accept(&source.store.pool, &mail.root, &outsider_uid)
            .await
            .unwrap();
        let denied = request(&outsider_wire, &addr, &history).await;
        assert!(matches!(&denied, WireResponse::Ops { .. }));
        assert!(!serde_json::to_string(&denied).unwrap().contains(&message));
        assert!(matches!(
            request(&outsider_wire, &addr, &reference).await,
            WireResponse::Refused { .. }
        ));
        assert!(
            matches!(request(&outsider_wire,&addr,&general).await,WireResponse::Ops {ops,..} if !ops.iter().any(|op|op.uid==message))
        );
        for (label, bytes) in [
            (
                "malformed",
                b"{\"script\":\"should never execute\"}".to_vec(),
            ),
            (
                "oversized",
                vec![b'x'; nucleus::transfer::receipt_delivery::MAX_BYTES + 1],
            ),
        ] {
            let invalid = store::records::create_in_root(
                &source.store.pool,
                store::records::NewRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Message,
                    head: "Untrusted artifact",
                    body: "",
                    quantity: store::exact::one(),
                },
                Some(&mail.root),
            )
            .await
            .unwrap();
            let mut tx = store::write_tx(&source.store.pool).await.unwrap();
            store::message_content::save_on(
                &mut tx,
                &invalid.uid,
                &[nucleus::message::MessagePart::Attachment {
                    name: format!("{label}.json"),
                    mime_type: nucleus::transfer::receipt_delivery::MIME.into(),
                    data: base64::engine::general_purpose::STANDARD.encode(bytes),
                }],
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
            let ops = store::sync_ops::ops_missing_from_vector_in_root(
                &source.store.pool,
                &mail.root,
                &[],
                2000,
            )
            .await
            .unwrap()
            .into_iter()
            .filter(|op| op.uid == invalid.uid)
            .collect();
            let untrusted = engine::sync::OpBatch {
                from_organ: own_organ.clone(),
                ops: source.hydrate_ops(ops).await.unwrap(),
            };
            assert!(matches!(
                request(
                    &server,
                    &client_addr,
                    &WireRequest::PushGrantOps {
                        root: mail.root.clone(),
                        batch: untrusted
                    }
                )
                .await,
                WireResponse::BatchSaved { .. }
            ));
            let baseline:(i64,i64)=store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM transfer_receipt_version)").fetch_one(&receiver.store.pool).await.unwrap();
            assert!(
                receiver
                    .act(
                        action(
                            &mail.reader.uid,
                            &format!("artifact:wire:{label}:inspect"),
                            DeliveryCommand::Inspect {
                                message: invalid.uid.clone()
                            }
                        ),
                        None
                    )
                    .await
                    .is_err()
            );
            assert!(
                receiver
                    .act(
                        action(
                            &mail.reader.uid,
                            &format!("artifact:wire:{label}:import"),
                            DeliveryCommand::Import {
                                message: invalid.uid.clone(),
                                expected_projection: hash("untrusted")
                            }
                        ),
                        None
                    )
                    .await
                    .is_err()
            );
            let after:(i64,i64)=store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM transfer_receipt_version)").fetch_one(&receiver.store.pool).await.unwrap();
            assert_eq!(after, baseline);
            let filtered = request(&client, &addr, &history).await;
            assert!(
                !serde_json::to_string(&filtered)
                    .unwrap()
                    .contains(&invalid.uid)
            );
        }
        store::visibility::set_hidden_from_organ(
            &source.store.pool,
            &mail.recipient,
            &message,
            true,
        )
        .await
        .unwrap();
        assert!(matches!(
            request(&client, &addr, &reference).await,
            WireResponse::Refused { .. }
        ));
        let filtered = request(&client, &addr, &history).await;
        assert!(!serde_json::to_string(&filtered).unwrap().contains(&message));
        let canonical:i64=store::sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM transfer)+(SELECT COUNT(*) FROM transfer_occurrence)+(SELECT COUNT(*) FROM transfer_receipt_record)").fetch_one(&receiver.store.pool).await.unwrap();
        assert_eq!(canonical, 0);
        serving.abort();
        receiving.abort();
    });
}
