use super::fixtures::{engine, plain, protein};
use engine::Engine;
use engine::actions::{
    Action, TransferOccurrenceClaimRole, TransferPromiseInput, TransferReservePoint,
    TransferSatiation, TransferVisibility,
};
use engine::trust::Signer;
use nucleus::transfer::{AgreementType, OpenPromiseReusePolicy, TransferRemainderPolicy};
use nucleus::{DecimalValue, RecordKind};
use protein::{Predicate, Source, execute, execute_for_with_signer};
use serde_json::Value;
use store::records::NewRecord;

async fn person(e: &Engine, slug: &str) -> String {
    store::records::create(
        &e.store.pool,
        NewRecord {
            slug: Some(slug),
            kind: RecordKind::Person,
            head: slug,
            body: "",
            quantity: store::exact::one(),
        },
    )
    .await
    .expect("person")
    .uid
}

#[test]
fn transfer_source_derives_the_occurrence_status_ladder() {
    std::thread::Builder::new()
        .name("protein-transfer-status".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    let e = engine().await;
                    let apples = plain(&e, "status.apples", 10.0).await;
                    let ana = person(&e, "status.ana").await;
                    let bia = person(&e, "status.bia").await;
                    let ana_signer = Signer::generate(&ana, "test:status:ana");
                    let bia_signer = Signer::generate(&bia, "test:status:bia");

                    let promise = |uid: &str, person: &str, delta: f64| TransferPromiseInput {
                        item: None,
                        uid: Some(uid.into()),
                        record: apples.clone(),
                        party: Some(person.into()),
                        open: false,
                        delta,
                        unit: None,
                        window_start: None,
                        window_end: None,
                        place: None,
                        condition: None,
                        reserve_from: Some(TransferReservePoint::Active),
                        reuse_policy: OpenPromiseReusePolicy::Duplicate,
                        withdrawn: false,
                    };
                    e.set_signer(ana_signer.clone()).await.unwrap();
                    let transfer = e
                        .act(
                            Action::CreateTransferDraft {
                                public_history: None,
                                request_id: "status:create".into(),
                                creator: Some(ana.clone()),
                                slug: Some("status.transfer".into()),
                                head: "Status".into(),
                                agreement: AgreementType::Full,
                                agreement_pct: None,
                                satiation: TransferSatiation::None,
                                parent: None,
                                source: None,
                                visibility: TransferVisibility::Hidden,
                                max_proximity: None,
                                reserve_default: TransferReservePoint::Active,
                                require_confirmation: true,
                                default_place: None,
                                invitees: vec![bia.clone()],
                                promises: vec![
                                    promise("status-give", &ana, -5.0),
                                    promise("status-receive", &bia, 5.0),
                                ],
                                dependencies: Vec::new(),
                            },
                            None,
                        )
                        .await
                        .unwrap()
                        .created
                        .unwrap();
                    let invitation =
                        store::transfers::invitations_for_transfer(&e.store.pool, &transfer)
                            .await
                            .unwrap()
                            .into_iter()
                            .find(|invitation| invitation.addressed_person_uid == bia)
                            .unwrap();
                    let revision = transfer_revision(&e, &transfer).await;
                    e.set_signer(bia_signer.clone()).await.unwrap();
                    e.act(
                        Action::AcceptTransferInvitation {
                            invitation: invitation.uid,
                            expected_revision: revision,
                            request_id: "status:accept".into(),
                            transfer: Some(transfer.clone()),
                            person: Some(bia.clone()),
                        },
                        None,
                    )
                    .await
                    .unwrap();
                    let revision = transfer_revision(&e, &transfer).await;
                    assert_eq!(transfer_status(&e, &transfer).await, "proposed");

                    for (person, signer, prefix) in [
                        (&ana, &ana_signer, "status:ana"),
                        (&bia, &bia_signer, "status:bia"),
                    ] {
                        for (level, suffix) in [(1, "checked"), (2, "agreed")] {
                            e.set_signer(signer.clone()).await.unwrap();
                            e.act(
                                Action::SetTransferAgreementLevel {
                                    transfer: transfer.clone(),
                                    expected_revision: revision,
                                    request_id: format!("{prefix}:{suffix}"),
                                    person: Some(person.clone()),
                                    level,
                                },
                                None,
                            )
                            .await
                            .unwrap();
                        }
                    }
                    assert_eq!(transfer_status(&e, &transfer).await, "agreed");

                    let mut occurrences = Vec::new();
                    for (person, signer, promise_uid, request_id) in [
                        (&ana, &ana_signer, "status-give", "status:activate:give"),
                        (
                            &bia,
                            &bia_signer,
                            "status-receive",
                            "status:activate:receive",
                        ),
                    ] {
                        e.set_signer(signer.clone()).await.unwrap();
                        let occurrence = e
                            .act(
                                Action::ActivateTransferOccurrence {
                                    original_authorities: Vec::new(),
                                    transfer: transfer.clone(),
                                    promise: promise_uid.into(),
                                    expected_revision: revision,
                                    request_id: request_id.into(),
                                    person: Some(person.clone()),
                                },
                                None,
                            )
                            .await
                            .unwrap()
                            .created
                            .unwrap();
                        occurrences.push((person.clone(), occurrence));
                    }
                    assert_eq!(transfer_status(&e, &transfer).await, "in_transfer");

                    for (index, (owner, occurrence)) in occurrences.iter().enumerate() {
                        for (person, signer, role, suffix) in [
                            (
                                &ana,
                                &ana_signer,
                                TransferOccurrenceClaimRole::Delivery,
                                "delivery",
                            ),
                            (
                                &bia,
                                &bia_signer,
                                TransferOccurrenceClaimRole::Receipt,
                                "receipt",
                            ),
                        ] {
                            e.set_signer(signer.clone()).await.unwrap();
                            e.act(
                                Action::SetTransferOccurrenceClaim {
                                    occurrence: occurrence.clone(),
                                    request_id: format!("status:{index}:{suffix}"),
                                    person: Some(person.clone()),
                                    role,
                                    claimed: true,
                                },
                                None,
                            )
                            .await
                            .unwrap();
                        }
                        let signer = if owner == &ana {
                            &ana_signer
                        } else {
                            &bia_signer
                        };
                        e.set_signer(signer.clone()).await.unwrap();
                        let preview = settlement_preview(&e, occurrence, owner, 5.0).await;
                        e.act(
                            Action::SettleTransferOccurrence {
                                occurrence: occurrence.clone(),
                                request_id: format!("status:settle:{index}"),
                                person: Some(owner.clone()),
                                canonical_quantity: 5.0,
                                expected_remaining_quantity: preview["expected_remaining_quantity"]
                                    .as_f64()
                                    .unwrap(),
                                expected_local_delta: preview["expected_local_delta"]
                                    .as_f64()
                                    .unwrap(),
                                expected_effects_hash: preview["expected_effects_hash"]
                                    .as_str()
                                    .map(str::to_owned),
                                expected_application_formula_hash:
                                    preview["expected_application_formula_hash"]
                                        .as_str()
                                        .unwrap()
                                        .into(),
                                expected_application_formula_version:
                                    preview["expected_application_formula_version"]
                                        .as_u64()
                                        .unwrap(),
                                expected_remainder_policy: TransferRemainderPolicy::Visible,
                            },
                            None,
                        )
                        .await
                        .unwrap();
                        assert_eq!(
                            transfer_status(&e, &transfer).await,
                            if index == 0 {
                                "partially_settled"
                            } else {
                                "settled"
                            }
                        );
                    }
                });
        })
        .unwrap()
        .join()
        .unwrap();
}

async fn transfer_revision(engine: &Engine, transfer: &str) -> u64 {
    let transfer = store::transfers::get(&engine.store.pool, transfer)
        .await
        .expect("transfer query")
        .expect("transfer exists");
    u64::try_from(transfer.revision).expect("nonnegative transfer revision")
}

async fn transfer_status(engine: &Engine, transfer: &str) -> String {
    let mut query = protein(Source::Transfer);
    query.filter = vec![Predicate::UidEq(transfer.into())];
    execute(&engine.store, &query)
        .await
        .expect("transfer query")
        .into_iter()
        .find(|row| row["uid"] == transfer)
        .expect("transfer row")["status"]
        .as_str()
        .expect("transfer status")
        .into()
}

async fn settlement_preview(
    engine: &Engine,
    occurrence: &str,
    person: &str,
    quantity: f64,
) -> Value {
    let mut query = protein(Source::TransferSettlementPreview);
    query.filter = vec![
        Predicate::UidEq(occurrence.into()),
        Predicate::QuantityEq(DecimalValue::from_f64_lossy(quantity).expect("fixture quantity")),
    ];
    execute_for_with_signer(&engine.store, &query, None, Some(person))
        .await
        .expect("settlement preview query")
        .into_iter()
        .next()
        .expect("settlement preview row")
}
