use super::*;

async fn setup() -> PublicationFixture {
    PublicationFixture::with_options(
        Vec::new(),
        vec![
            Stage::Need,
            Stage::Receiver,
            Stage::Settled,
            Stage::SettlementStatus,
            Stage::Cancelled,
            Stage::Compensated,
            Stage::Dispute,
        ],
    )
    .await
}

async fn giver_record(setup: &PublicationFixture) -> String {
    import(
        &setup.fixture.engine,
        &setup.fixture.giver,
        "lifecycle:review",
        &document(&setup.fixture.engine, &setup.receiver_review).await,
    )
    .await
}

async fn claimed(setup: &PublicationFixture) {
    let f = &setup.fixture;
    support::claim(
        &f.engine,
        &f.occurrence,
        &f.giver,
        engine::actions::TransferOccurrenceClaimRole::Delivery,
        "lifecycle:delivery",
    )
    .await;
    support::claim(
        &f.engine,
        &f.occurrence,
        &f.receiver,
        engine::actions::TransferOccurrenceClaimRole::Receipt,
        "lifecycle:receipt",
    )
    .await;
}

async fn settle(
    setup: &PublicationFixture,
    amount: f64,
    id: &str,
) -> store::transfers::OccurrenceSettlementSliceRow {
    let f = &setup.fixture;
    let preview = support::settlement_preview(&f.engine, &f.occurrence, &f.giver, amount).await;
    support::settle_from_preview(&f.engine, &f.occurrence, &f.giver, id, &preview).await;
    store::transfers::occurrence_settlement_slice_for_request(&f.engine.store.pool, id)
        .await
        .unwrap()
        .unwrap()
}

async fn preview(
    setup: &PublicationFixture,
    record: &str,
    source: EventSource,
    id: &str,
) -> Result<(EventRequest, Value), engine::EngineError> {
    let f = &setup.fixture;
    f.engine.set_signer(f.giver.signer.clone()).await.unwrap();
    let input = request(&f.engine, record, &setup.giver_key, source).await;
    let result = f
        .engine
        .act(
            action(
                &f.giver,
                id,
                Command::PreviewEvent {
                    request: input.clone(),
                },
            ),
            None,
        )
        .await?;
    assert!(result.facts.is_empty());
    Ok((
        input,
        result.data.unwrap()["receipt"]["document"]["value"].clone(),
    ))
}

async fn publish(
    setup: &PublicationFixture,
    record: &mut String,
    source: EventSource,
    id: &str,
) -> nucleus::karma::CanonicalHash {
    let (request, value) = preview(setup, record, source, &format!("{id}:preview"))
        .await
        .unwrap();
    let published = setup
        .fixture
        .engine
        .act(
            action(
                &setup.fixture.giver,
                id,
                Command::PublishEvent {
                    request,
                    reviewed_at: value["reviewed_at"].as_i64().unwrap(),
                    expected_projection: serde_json::from_value(
                        value["expected_projection"].clone(),
                    )
                    .unwrap(),
                },
            ),
            None,
        )
        .await
        .unwrap();
    *record = published.created.unwrap();
    serde_json::from_value::<nucleus::transfer::receipt::Event>(value["event"].clone())
        .unwrap()
        .id()
        .unwrap()
}

#[test]
fn lifecycle_partial_full_and_each_compensation_preserve_canonical_history_without_private_effects()
{
    support::run_async_test("receipt-lifecycle-settlement", || async {
        let setup = setup().await;
        let f = &setup.fixture;
        let mut record = giver_record(&setup).await;
        claimed(&setup).await;
        let first = settle(&setup, 4.0, "lifecycle:first").await;
        let first_event = publish(
            &setup,
            &mut record,
            EventSource::SettlementStatus {
                fact: first.evidence_fact_uid.clone(),
                supersedes: None,
            },
            "lifecycle:first:public",
        )
        .await;
        assert!(
            store::sqlx::query(
                "UPDATE transfer_occurrence_settlement_slice SET remaining_after = 5 WHERE uid = ?",
            )
            .bind(&first.uid)
            .execute(&f.engine.store.pool)
            .await
            .is_err()
        );
        assert!(
            preview(
                &setup,
                &record,
                EventSource::SettlementStatus {
                    fact: first.application_fact_uid.clone(),
                    supersedes: Some(first_event.clone()),
                },
                "lifecycle:corrupt-cache"
            )
            .await
            .is_err()
        );
        let second = settle(&setup, 6.0, "lifecycle:second").await;
        publish(
            &setup,
            &mut record,
            EventSource::SettlementStatus {
                fact: second.evidence_fact_uid.clone(),
                supersedes: Some(first_event),
            },
            "lifecycle:full:public",
        )
        .await;
        for (index, slice) in [&first, &second].into_iter().enumerate() {
            f.engine.set_signer(f.giver.signer.clone()).await.unwrap();
            let id = format!("lifecycle:compensate:{index}");
            let result = f
                .engine
                .act(
                    Action::CompensateTransferOccurrenceSettlement {
                        settlement: slice.uid.clone(),
                        request_id: id.clone(),
                        person: Some(f.giver.uid.clone()),
                    },
                    None,
                )
                .await
                .unwrap();
            let correction = store::transfers::occurrence_settlement_compensation_for_request(
                &f.engine.store.pool,
                &id,
            )
            .await
            .unwrap()
            .unwrap();
            assert!(!result.facts.is_empty());
            publish(
                &setup,
                &mut record,
                EventSource::Compensation {
                    fact: correction.fact.uid,
                    supersedes: None,
                },
                &format!("lifecycle:compensation:public:{index}"),
            )
            .await;
        }
        let history = document(&f.engine, &record).await;
        let Document::Publication(bundle) = &history else {
            panic!("Publication expected");
        };
        assert_eq!(bundle.publication.events.len(), 4);
        assert!(bundle.publication.events.iter().any(|signed| matches!(&signed.event.content,
            PublicContent::SettlementStatus { quantity, remaining_after } if quantity.value().unwrap() == 4.0 && remaining_after.value().unwrap() == 6.0)));
        assert!(bundle.publication.events.iter().any(|signed| matches!(&signed.event.content,
            PublicContent::SettlementStatus { quantity, remaining_after } if quantity.value().unwrap() == 10.0 && remaining_after.value().unwrap() == 0.0)));
        let slots = bundle
            .publication
            .events
            .iter()
            .filter(|signed| matches!(signed.event.content, PublicContent::Compensated { .. }))
            .map(|signed| signed.event.slot(&bundle.publication.scope).unwrap())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(slots.len(), 2);
        assert_eq!(
            store::transfers::occurrence_settlement_progress(&f.engine.store.pool, &f.occurrence)
                .await
                .unwrap()
                .unwrap()
                .settled_quantity,
            10.0
        );
        assert_eq!(
            store::records::get(&f.engine.store.pool, &f.stock)
                .await
                .unwrap()
                .unwrap()
                .quantity,
            store::exact::integer(20)
        );
        let bytes = serde_json::to_string(&history).unwrap();
        for private in [
            &f.stock,
            &f.transfer.transfer,
            &f.selection.need,
            &f.giver.uid,
            &f.receiver.uid,
            &first.uid,
            &first.application_fact_uid,
            &second.application_fact_uid,
        ] {
            assert!(!bytes.contains(private), "Leaked {private}");
        }
        let reader = support::engine().await;
        reader
            .adopt_introduction(&f.engine.introduction().await.unwrap(), 1)
            .await
            .unwrap();
        let observer = support::person(&reader, "lifecycle.reader").await;
        let imported = import(&reader, &observer, "lifecycle:import", &history).await;
        assert_eq!(document(&reader, &imported).await, history);
        let canonical: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_occurrence")
            .fetch_one(&reader.store.pool)
            .await
            .unwrap();
        assert_eq!(canonical, 0);
        let effects: i64 = store::sqlx::query_scalar(
            "SELECT COUNT(*) FROM transfer_occurrence_settlement_compensation",
        )
        .fetch_one(&reader.store.pool)
        .await
        .unwrap();
        assert_eq!(effects, 0);
    });
}

#[test]
fn lifecycle_cancellation_requires_applied_signed_agreements_and_keeps_original_remaining_amount() {
    support::run_async_test("receipt-lifecycle-cancellation", || async {
        let setup = setup().await;
        let f = &setup.fixture;
        let mut record = giver_record(&setup).await;
        claimed(&setup).await;
        let slice = settle(&setup, 4.0, "lifecycle:four").await;
        publish(
            &setup,
            &mut record,
            EventSource::SettlementStatus {
                fact: slice.evidence_fact_uid.clone(),
                supersedes: None,
            },
            "lifecycle:partial",
        )
        .await;
        let (stale_request, stale_projection) = preview(
            &setup,
            &record,
            EventSource::Settlement {
                fact: slice.evidence_fact_uid.clone(),
                supersedes: None,
            },
            "lifecycle:before-cancellation-revision",
        )
        .await
        .unwrap();
        f.engine.set_signer(f.giver.signer.clone()).await.unwrap();
        let cancellation = f
            .engine
            .act(
                Action::ProposeTransferCancellation {
                    transfer: f.transfer.transfer.clone(),
                    occurrence: f.occurrence.clone(),
                    expected_revision: f.transfer.revision,
                    expected_remaining_quantity: store::exact::integer(6),
                    request_id: "lifecycle:cancel:propose".into(),
                    person: Some(f.giver.uid.clone()),
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let stale = f
            .engine
            .act(
                action(
                    &f.giver,
                    "lifecycle:stale-revision",
                    Command::PublishEvent {
                        request: stale_request,
                        reviewed_at: stale_projection["reviewed_at"].as_i64().unwrap(),
                        expected_projection: serde_json::from_value(
                            stale_projection["expected_projection"].clone(),
                        )
                        .unwrap(),
                    },
                ),
                None,
            )
            .await
            .unwrap_err();
        assert!(stale.to_string().contains("changed after review"));
        let revised = TransferFixture {
            transfer: f.transfer.transfer.clone(),
            revision: f.transfer.revision + 1,
        };
        let proposal = store::transfers::revision_fact(
            &f.engine.store.pool,
            &revised.transfer,
            revised.revision,
        )
        .await
        .unwrap()
        .unwrap();
        assert!(
            preview(
                &setup,
                &record,
                EventSource::Cancellation {
                    fact: proposal.uid,
                    supersedes: None,
                },
                "lifecycle:proposal:not:applied"
            )
            .await
            .is_err()
        );
        for person in [&f.giver, &f.receiver] {
            support::agree(
                &f.engine,
                &revised,
                person,
                &format!("lifecycle:cancel:agree:{}", person.uid),
            )
            .await;
        }
        f.engine
            .set_signer(f.receiver.signer.clone())
            .await
            .unwrap();
        let applied = f
            .engine
            .act(
                Action::ApplyTransferCancellation {
                    transfer: revised.transfer,
                    cancellation,
                    expected_revision: revised.revision,
                    request_id: "lifecycle:cancel:apply".into(),
                    person: Some(f.receiver.uid.clone()),
                },
                None,
            )
            .await
            .unwrap();
        let fact = applied.facts.last().unwrap().uid.clone();
        let (input, value) = preview(
            &setup,
            &record,
            EventSource::Cancellation {
                fact: fact.clone(),
                supersedes: None,
            },
            "lifecycle:cancel:review",
        )
        .await
        .unwrap();
        let original = document(&f.engine, &record).await;
        let progress =
            store::transfers::occurrence_settlement_progress(&f.engine.store.pool, &f.occurrence)
                .await
                .unwrap()
                .unwrap();
        assert_eq!(
            (
                progress.settled_quantity,
                progress.cancelled_quantity,
                progress.remaining_quantity
            ),
            (4.0, 6.0, 0.0)
        );
        let applied_fact = store::facts::get(&f.engine.store.pool, &fact)
            .await
            .unwrap()
            .unwrap();
        let payload: Value =
            serde_json::from_str(applied_fact.payload.as_deref().unwrap()).unwrap();
        let agreement = payload["agreement_facts"][0].as_str().unwrap();
        let original_signature = store::facts::get(&f.engine.store.pool, agreement)
            .await
            .unwrap()
            .unwrap()
            .signature;
        store::sqlx::query("UPDATE fact SET signature = NULL WHERE uid = ?")
            .bind(agreement)
            .execute(&f.engine.store.pool)
            .await
            .unwrap();
        assert!(
            f.engine
                .act(
                    action(
                        &f.giver,
                        "lifecycle:cancel:stale-signature",
                        Command::PublishEvent {
                            request: input,
                            reviewed_at: value["reviewed_at"].as_i64().unwrap(),
                            expected_projection: serde_json::from_value(
                                value["expected_projection"].clone()
                            )
                            .unwrap(),
                        }
                    ),
                    None
                )
                .await
                .is_err()
        );
        assert_eq!(document(&f.engine, &record).await, original);
        store::sqlx::query("UPDATE fact SET signature = ? WHERE uid = ?")
            .bind(original_signature)
            .bind(agreement)
            .execute(&f.engine.store.pool)
            .await
            .unwrap();
        publish(
            &setup,
            &mut record,
            EventSource::Cancellation {
                fact,
                supersedes: None,
            },
            "lifecycle:cancel:public",
        )
        .await;
        let final_document = document(&f.engine, &record).await;
        let Document::Publication(bundle) = &final_document else {
            panic!("Publication expected");
        };
        assert!(bundle.publication.events.iter().any(|event| matches!(&event.event.content,
            PublicContent::Cancelled { quantity, revision } if quantity.canonical() == "6" && *revision == revised.revision)));
        assert!(bundle.publication.events.iter().any(|event| matches!(&event.event.content,
            PublicContent::SettlementStatus { remaining_after, .. } if remaining_after.value().unwrap() == 6.0)));
    });
}

#[test]
fn lifecycle_new_fields_need_new_scope_consent_and_compensation_needs_the_original_owner() {
    support::run_async_test("receipt-lifecycle-consent", || async {
        let setup =
            PublicationFixture::with_options(Vec::new(), vec![Stage::Settled, Stage::Compensated])
                .await;
        let f = &setup.fixture;
        let record = giver_record(&setup).await;
        claimed(&setup).await;
        let slice = settle(&setup, 4.0, "lifecycle:private:four").await;
        assert!(
            preview(
                &setup,
                &record,
                EventSource::SettlementStatus {
                    fact: slice.evidence_fact_uid.clone(),
                    supersedes: None,
                },
                "lifecycle:unapproved:remaining"
            )
            .await
            .is_err()
        );
        let saved = document(&f.engine, &record).await;
        let result = f
            .engine
            .act(
                Action::CompensateTransferOccurrenceSettlement {
                    settlement: slice.uid,
                    request_id: "lifecycle:private:compensate".into(),
                    person: Some(f.giver.uid.clone()),
                },
                None,
            )
            .await
            .unwrap();
        let fact = result
            .facts
            .iter()
            .find(|fact| fact.cause.kind == nucleus::CauseKind::Compensation)
            .unwrap();
        f.engine
            .set_signer(f.receiver.signer.clone())
            .await
            .unwrap();
        let input = request(
            &f.engine,
            &setup.receiver_review,
            &setup.receiver_key,
            EventSource::Compensation {
                fact: fact.uid.clone(),
                supersedes: None,
            },
        )
        .await;
        assert!(
            f.engine
                .act(
                    action(
                        &f.receiver,
                        "lifecycle:wrong:owner",
                        Command::PreviewEvent { request: input }
                    ),
                    None
                )
                .await
                .is_err()
        );
        assert_eq!(document(&f.engine, &record).await, saved);
    });
}
