use super::super::ordinary_delivery::exchange;
use super::*;

async fn publish_origin(
    setup: &Remote,
    record: &mut String,
    key: &str,
    source: EventSource,
    id: &str,
) {
    let f = &setup.source;
    f.engine.set_signer(f.giver.signer.clone()).await.unwrap();
    let request = request(&f.engine, record, key, source).await;
    let preview = f
        .engine
        .act(
            action(
                &f.giver,
                &format!("{id}:preview"),
                Command::PreviewEvent {
                    request: request.clone(),
                },
            ),
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap()["receipt"]["document"]["value"]
        .clone();
    *record = f
        .engine
        .act(
            action(
                &f.giver,
                id,
                Command::PublishEvent {
                    request,
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
        .unwrap()
        .created
        .unwrap();
}

async fn receiver_statement(
    setup: &Remote,
    record: &mut String,
    key: &str,
    binding: &str,
    previous: Option<String>,
    amount: &str,
    withdrawn: bool,
    id: &str,
) -> String {
    setup
        .receiver
        .set_signer(setup.source.receiver.signer.clone())
        .await
        .unwrap();
    let mut command = setup.capture(
        id,
        record,
        Request::State {
            role: Role::Receiver,
            measure: setup.measure.clone(),
            amount: amount.into(),
            withdrawn,
            need_binding: Some(binding.into()),
            supersedes: previous,
        },
        Some(setup.source.transfer.transfer.clone()),
        None,
    );
    if let Action::TransferReceipt {
        request: Command::RecordSource { request },
        ..
    } = &mut command
    {
        request.expected_version =
            store::transfer_receipts::get(&setup.receiver.store.pool, record)
                .await
                .unwrap()
                .version;
    }
    let source = setup
        .receiver
        .act(command, Some(setup.source.receiver.uid.clone()))
        .await
        .unwrap()
        .created
        .unwrap();
    *record = setup
        .publish(record, key, &source, &format!("{id}:public"))
        .await;
    source
}

async fn reader_permissions(node: &Engine, person: &Person) {
    let role = store::auth::ensure_role(&node.store.pool, "Receipt reader")
        .await
        .unwrap();
    for operation in ["read", "create", "update"] {
        let permission = store::auth::ensure_permission(&node.store.pool, "record", operation)
            .await
            .unwrap();
        store::auth::grant(&node.store.pool, role, permission)
            .await
            .unwrap();
    }
    store::auth::create_credential(&node.store.pool, &person.uid, &person.uid, "fixture", role)
        .await
        .unwrap();
}

#[test]
fn four_lince_publication_lifecycle_roundtrip_preserves_independent_corrections_and_no_observer_credit()
 {
    support::run_async_test("four-lince-receipt-workflow", || async {
        let setup = Remote::new().await;
        let f = &setup.source;
        let observer = support::engine().await;
        let _observer_authority = Authorizer::new(&observer, 101).await;
        let observer_person = support::person(&observer, "workflow.observer").await;
        let reader = support::engine().await;
        let viewer = support::person(&reader, "workflow.reader").await;
        reader_permissions(&reader, &viewer).await;
        reader_permissions(&observer, &observer_person).await;
        for node in [&observer, &reader] {
            for origin in [&f.engine, &setup.receiver] {
                node.adopt_introduction(&origin.introduction().await.unwrap(), 1)
                    .await
                    .unwrap();
            }
        }
        let binding = setup.bind().await;
        let (review, receiver_key, origin_key) = setup
            .approve_with_stages(
                &binding,
                vec![
                    Stage::Need,
                    Stage::Giver,
                    Stage::Receiver,
                    Stage::Settled,
                    Stage::SettlementStatus,
                    Stage::Cancelled,
                    Stage::Compensated,
                    Stage::Dispute,
                ],
            )
            .await;
        let mut receiver_record = setup
            .publish(&review, &receiver_key, &binding, "workflow:need")
            .await;
        let received = receiver_statement(
            &setup,
            &mut receiver_record,
            &receiver_key,
            &binding,
            None,
            "4",
            false,
            "workflow:received",
        )
        .await;
        let first = document(&setup.receiver, &receiver_record).await;
        let scope = first.scope().unwrap().clone();
        assert_ne!(scope.giver, scope.receiver);
        assert_ne!(
            f.engine.introduction().await.unwrap().organ_uid,
            setup.receiver.introduction().await.unwrap().organ_uid
        );
        let mut observed = exchange(
            &setup.receiver,
            &f.receiver,
            &receiver_record,
            &observer,
            &observer_person,
            "workflow:first:observer",
        )
        .await;
        let mut read = exchange(
            &observer,
            &observer_person,
            &observed,
            &reader,
            &viewer,
            "workflow:first:reader",
        )
        .await;
        let initial_reader_version = store::transfer_receipts::get(&reader.store.pool, &read)
            .await
            .unwrap()
            .version;
        let mut origin_record = exchange(
            &setup.receiver,
            &f.receiver,
            &receiver_record,
            &f.engine,
            &f.giver,
            "workflow:first:origin",
        )
        .await;
        let Document::Publication(initial) = &first else {
            panic!("Publication expected");
        };
        let public_need = initial
            .publication
            .events
            .iter()
            .find(|signed| matches!(signed.event.content, PublicContent::Need { .. }))
            .unwrap()
            .event
            .id()
            .unwrap();
        f.engine.set_signer(f.giver.signer.clone()).await.unwrap();
        let contribution = f
            .engine
            .act(
                action(
                    &f.giver,
                    "workflow:giver:claim",
                    Command::RecordSource {
                        request: SourceRequest {
                            receipt: origin_record.clone(),
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
                            public_need: Some(public_need),
                        },
                    },
                ),
                Some(f.giver.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        publish_origin(
            &setup,
            &mut origin_record,
            &origin_key,
            EventSource::Snapshot {
                record: contribution.clone(),
            },
            "workflow:giver:public",
        )
        .await;
        for (person, role) in [
            (
                &f.giver,
                engine::actions::TransferOccurrenceClaimRole::Delivery,
            ),
            (
                &f.receiver,
                engine::actions::TransferOccurrenceClaimRole::Receipt,
            ),
        ] {
            support::claim(
                &f.engine,
                &f.occurrence,
                person,
                role,
                &format!("workflow:confirmed:{}", person.uid),
            )
            .await;
        }
        let quote = support::settlement_preview(&f.engine, &f.occurrence, &f.giver, 4.0).await;
        support::settle_from_preview(
            &f.engine,
            &f.occurrence,
            &f.giver,
            "workflow:settle",
            &quote,
        )
        .await;
        let slice = store::transfers::occurrence_settlement_slice_for_request(
            &f.engine.store.pool,
            "workflow:settle",
        )
        .await
        .unwrap()
        .unwrap();
        publish_origin(
            &setup,
            &mut origin_record,
            &origin_key,
            EventSource::SettlementStatus {
                fact: slice.evidence_fact_uid.clone(),
                supersedes: None,
            },
            "workflow:partial",
        )
        .await;
        let cancellation = f
            .engine
            .act(
                Action::ProposeTransferCancellation {
                    transfer: f.transfer.transfer.clone(),
                    occurrence: f.occurrence.clone(),
                    expected_revision: f.transfer.revision,
                    expected_remaining_quantity: store::exact::integer(6),
                    request_id: "workflow:cancel:propose".into(),
                    person: Some(f.giver.uid.clone()),
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let revised = TransferFixture {
            transfer: f.transfer.transfer.clone(),
            revision: f.transfer.revision + 1,
        };
        for person in [&f.giver, &f.receiver] {
            support::agree(
                &f.engine,
                &revised,
                person,
                &format!("workflow:cancel:agree:{}", person.uid),
            )
            .await;
        }
        f.engine.set_signer(f.giver.signer.clone()).await.unwrap();
        let applied = f
            .engine
            .act(
                Action::ApplyTransferCancellation {
                    transfer: revised.transfer,
                    cancellation,
                    expected_revision: revised.revision,
                    request_id: "workflow:cancel:apply".into(),
                    person: Some(f.giver.uid.clone()),
                },
                None,
            )
            .await
            .unwrap();
        let cancellation_fact = applied.facts.last().unwrap().uid.clone();
        publish_origin(
            &setup,
            &mut origin_record,
            &origin_key,
            EventSource::Cancellation {
                fact: cancellation_fact,
                supersedes: None,
            },
            "workflow:cancel:public",
        )
        .await;
        f.engine
            .act(
                Action::CompensateTransferOccurrenceSettlement {
                    settlement: slice.uid.clone(),
                    request_id: "workflow:compensate".into(),
                    person: Some(f.giver.uid.clone()),
                },
                None,
            )
            .await
            .unwrap();
        let compensation = store::transfers::occurrence_settlement_compensation_for_request(
            &f.engine.store.pool,
            "workflow:compensate",
        )
        .await
        .unwrap()
        .unwrap();
        publish_origin(
            &setup,
            &mut origin_record,
            &origin_key,
            EventSource::Compensation {
                fact: compensation.fact.uid,
                supersedes: None,
            },
            "workflow:compensate:public",
        )
        .await;
        observed = exchange(
            &f.engine,
            &f.giver,
            &origin_record,
            &observer,
            &observer_person,
            "workflow:lifecycle:observer",
        )
        .await;
        read = exchange(
            &observer,
            &observer_person,
            &observed,
            &reader,
            &viewer,
            "workflow:lifecycle:reader",
        )
        .await;
        assert!(
            store::transfer_receipts::get(&reader.store.pool, &read)
                .await
                .unwrap()
                .version
                > initial_reader_version
        );
        receiver_record = exchange(
            &f.engine,
            &f.giver,
            &origin_record,
            &setup.receiver,
            &f.receiver,
            "workflow:lifecycle:receiver",
        )
        .await;
        let revised_source = receiver_statement(
            &setup,
            &mut receiver_record,
            &receiver_key,
            &binding,
            Some(received.clone()),
            "3",
            false,
            "workflow:correct",
        )
        .await;
        let withdrawn = receiver_statement(
            &setup,
            &mut receiver_record,
            &receiver_key,
            &binding,
            Some(revised_source.clone()),
            "0",
            true,
            "workflow:withdraw",
        )
        .await;
        let final_document = document(&setup.receiver, &receiver_record).await;
        let exported = setup
            .receiver
            .act(
                action(
                    &f.receiver,
                    "workflow:export",
                    Command::Export {
                        record: receiver_record.clone(),
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
        assert_eq!(
            Document::parse(bytes.as_bytes(), nucleus::execution::now().timestamp()).unwrap(),
            final_document
        );
        for private in [
            &setup.selection.need,
            &binding,
            &received,
            &revised_source,
            &withdrawn,
            &contribution,
            &f.stock,
            &f.transfer.transfer,
            &f.giver.uid,
            &f.receiver.uid,
            &slice.uid,
            &slice.application_fact_uid,
        ] {
            assert!(
                !bytes.contains(private),
                "Private reference leaked: {private}"
            );
        }
        observed = exchange(
            &setup.receiver,
            &f.receiver,
            &receiver_record,
            &observer,
            &observer_person,
            "workflow:correction:observer",
        )
        .await;
        read = exchange(
            &observer,
            &observer_person,
            &observed,
            &reader,
            &viewer,
            "workflow:correction:reader",
        )
        .await;
        let saved = store::transfer_receipts::get(&reader.store.pool, &read)
            .await
            .unwrap();
        let repeated = import(&reader, &viewer, "workflow:repeat", &final_document).await;
        assert_eq!(repeated, read);
        assert_eq!(
            store::transfer_receipts::get(&reader.store.pool, &read)
                .await
                .unwrap()
                .version,
            saved.version
        );
        let Document::Publication(bundle) = &saved.document else {
            panic!("Publication expected");
        };
        assert_eq!(bundle.publication.events.len(), 8);
        assert!(bundle.publication.events.iter().any(|event| matches!(&event.event.content, PublicContent::Statement { role: Role::Giver, quantity, .. } if quantity.canonical() == "100")));
        for (quantity, withdrawn) in [("4", false), ("3", false), ("0", true)] {
            assert!(bundle.publication.events.iter().any(|event| matches!(&event.event.content, PublicContent::Statement { role: Role::Receiver, quantity: amount, withdrawn: state, .. } if amount.canonical() == quantity && *state == withdrawn)));
        }
        assert!(bundle.publication.events.iter().any(|event| matches!(&event.event.content, PublicContent::SettlementStatus { quantity, remaining_after } if quantity.value().unwrap() == 4.0 && remaining_after.value().unwrap() == 6.0)));
        assert!(bundle.publication.events.iter().any(|event| matches!(&event.event.content, PublicContent::Cancelled { quantity, .. } if quantity.canonical() == "6")));
        assert!(
            bundle
                .publication
                .events
                .iter()
                .any(|event| matches!(event.event.content, PublicContent::Compensated { .. }))
        );
        reader.set_signer(viewer.signer.clone()).await.unwrap();
        let history = reader
            .act(
                action(
                    &viewer,
                    "workflow:history",
                    Command::History {
                        origin: scope.source.origin.clone(),
                        occurrence: scope.source.occurrence.clone(),
                        before: None,
                    },
                ),
                Some(viewer.uid.clone()),
            )
            .await
            .unwrap()
            .data
            .unwrap()["receipt"]["document"]["value"]
            .clone();
        assert_eq!(history["history"]["source_conflict"], false);
        assert!(
            !history["history"]["outcomes"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        for node in [&setup.receiver, &observer, &reader] {
            let counts: (i64, i64, i64, i64) = store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM transfer), (SELECT COUNT(*) FROM transfer_occurrence), (SELECT COUNT(*) FROM transfer_occurrence_settlement_slice), (SELECT COUNT(*) FROM transfer_occurrence_settlement_compensation)")
                .fetch_one(&node.store.pool).await.unwrap();
            assert_eq!(counts, (0, 0, 0, 0));
        }
        for (node, uid) in [(&observer, &observed), (&reader, &read)] {
            assert!(
                store::records::get(&node.store.pool, uid)
                    .await
                    .unwrap()
                    .unwrap()
                    .quantity
                    .is_zero()
            );
            assert!(
                store::records::get(&node.store.pool, &f.stock)
                    .await
                    .unwrap()
                    .is_none()
            );
        }
        assert_eq!(
            store::records::get(&f.engine.store.pool, &f.stock)
                .await
                .unwrap()
                .unwrap()
                .quantity,
            store::exact::integer(20)
        );
        let invalid = reader
            .act(
                action(
                    &viewer,
                    "workflow:instruction",
                    Command::Import {
                        document:
                            json!({"action":"set-quantity-exact","target":read,"amount":"1000000"})
                                .to_string(),
                    },
                ),
                Some(viewer.uid.clone()),
            )
            .await;
        assert!(invalid.is_err());
        assert!(
            store::records::get(&reader.store.pool, &read)
                .await
                .unwrap()
                .unwrap()
                .quantity
                .is_zero()
        );
    });
}
