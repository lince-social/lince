mod support;

use engine::{Engine, actions::Action};
use nucleus::transfer::{AgreementType, karma::Snapshot};
use support::{DraftOptions, Person, TransferFixture};

fn run(test: impl std::future::Future<Output = ()> + Send + 'static) {
    support::run_async_test("karma-transfer-guard", || async move {
        nucleus::execution::Execution::new([38; 32], 1_893_456_000_000)
            .unwrap()
            .scope(test)
            .await;
    });
}

async fn fixture() -> (Engine, Person, Person, TransferFixture) {
    let engine = support::karma::engine().await;
    let a = support::person(&engine, "a").await;
    let b = support::person(&engine, "b").await;
    let item = support::plain(&engine, "item", 10.0).await;
    let transfer = support::create_transfer(
        &engine,
        &a,
        &[b.clone()],
        vec![
            support::promise("promise-a", &item, &a, -1.0),
            support::promise("promise-b", &item, &b, 1.0),
        ],
        DraftOptions {
            slug: "trade",
            agreement: AgreementType::Full,
            reserve_default: engine::actions::TransferReservePoint::Active,
            require_confirmation: true,
        },
    )
    .await;
    engine.set_signer(a.signer.clone()).await.unwrap();
    (engine, a, b, transfer)
}

fn assign(
    transfer: &TransferFixture,
    person: &Person,
    level: u8,
    state: Option<Snapshot>,
) -> Action {
    Action::AssignTransferAgreementLevel {
        transfer: transfer.transfer.clone(),
        person: Some(person.uid.clone()),
        expected_revision: transfer.revision,
        request_id: nucleus::new_uid("guard"),
        level,
        expected: state
            .as_ref()
            .and_then(|state| state.participants.get(&person.uid))
            .map(|participant| participant.guard.clone()),
        expected_state: state.as_ref().map(nucleus::transfer::karma::Guard::exact),
    }
}

async fn change(engine: &Engine, transfer: &TransferFixture, person: &Person, level: u8) {
    engine.set_signer(person.signer.clone()).await.unwrap();
    engine
        .act(assign(transfer, person, level, None), None)
        .await
        .unwrap();
}

#[test]
fn another_observed_participant_is_guarded_inside_assignment_and_publication() {
    run(async {
        let (engine, a, b, transfer) = fixture().await;
        let source = store::transfers::karma_snapshot::read(&engine.store.pool, &transfer.transfer)
            .await
            .unwrap();
        change(&engine, &transfer, &b, 1).await;
        engine.set_signer(a.signer.clone()).await.unwrap();
        let before = engine.store.state_hash().await.unwrap();
        let failure = engine
            .act(assign(&transfer, &a, 1, Some(source.clone())), None)
            .await
            .unwrap_err();
        assert_eq!(failure.code(), Some("transfer_state_source_changed"));
        assert_eq!(engine.store.state_hash().await.unwrap(), before);
        let input = store::transfers::publication_input(
            &engine.store.pool,
            &transfer.transfer,
            transfer.revision,
            &a.uid,
            "stale-publication",
        )
        .await
        .unwrap();
        let failure = store::transfers::publish_whole_draft_guarded(
            &engine.store.pool,
            input,
            Some(&source.participants[&a.uid].guard),
            Some(&nucleus::transfer::karma::Guard::exact(&source)),
            nucleus::execution::now(),
            Some(a.uid.clone()),
            |hash| Some(a.signer.sign_hash(hash)),
        )
        .await
        .unwrap_err();
        assert!(
            failure
                .to_string()
                .contains("transfer_state_source_changed")
        );
        assert_eq!(engine.store.state_hash().await.unwrap(), before);
    });
}

#[test]
fn activation_checks_its_original_state_in_the_domain_transaction() {
    run(async {
        let (engine, a, b, transfer) = fixture().await;
        change(&engine, &transfer, &a, 2).await;
        change(&engine, &transfer, &b, 2).await;
        let source = store::transfers::karma_snapshot::read(&engine.store.pool, &transfer.transfer)
            .await
            .unwrap();
        assert!(source.ready);
        change(&engine, &transfer, &b, 1).await;
        engine.set_signer(a.signer.clone()).await.unwrap();
        let before = engine.store.state_hash().await.unwrap();
        let failure = store::transfers::activate_occurrences_guarded(
            &engine.store.pool,
            store::transfers::ActivateOccurrencesInput {
                transfer_uid: transfer.transfer,
                expected_revision: transfer.revision,
                idempotency_key: "stale-activation".into(),
                actor_person_uid: a.uid.clone(),
                authorization_intent_uid: None,
                occurrences: vec![store::transfers::OccurrenceActivationInput {
                    promise_uid: "promise-a".into(),
                    opposite_promise_uid: Some("promise-b".into()),
                    giver_person_uid: a.uid.clone(),
                    receiver_person_uid: b.uid,
                }],
            },
            Some(&source.participants[&a.uid].guard),
            Some(&nucleus::transfer::karma::Guard::exact(&source)),
            nucleus::execution::now(),
            |hash| Some(a.signer.sign_hash(hash)),
        )
        .await
        .unwrap_err();
        assert!(
            failure
                .to_string()
                .contains("transfer_state_source_changed")
        );
        assert_eq!(engine.store.state_hash().await.unwrap(), before);
    });
}

#[test]
fn a_guard_does_not_require_participants_hidden_from_its_authorized_view() {
    run(async {
        let (engine, a, b, transfer) = fixture().await;
        let mut source =
            store::transfers::karma_snapshot::read(&engine.store.pool, &transfer.transfer)
                .await
                .unwrap();
        source.participants.retain(|person, _| person == &a.uid);
        change(&engine, &transfer, &b, 1).await;
        engine.set_signer(a.signer.clone()).await.unwrap();
        let action = assign(&transfer, &a, 1, Some(source));
        engine.act(action.clone(), None).await.unwrap();
        let before = engine.store.state_hash().await.unwrap();
        change(&engine, &transfer, &a, 0).await;
        let retreated = engine.store.state_hash().await.unwrap();
        assert_ne!(before, retreated);
        let result = engine.act(action, None).await.unwrap();
        assert!(result.facts.is_empty());
        assert_eq!(engine.store.state_hash().await.unwrap(), retreated);
    });
}
