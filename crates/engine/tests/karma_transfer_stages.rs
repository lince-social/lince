mod support;

use engine::{Engine, actions::Action};
use nucleus::karma::rule_field::RuleFieldInput;
use nucleus::transfer::AgreementType;
use support::{DraftOptions, Person, TransferFixture};

const START: i64 = 1_893_456_000_000;
const DAY: i64 = 86_400_000;

fn run(test: impl std::future::Future<Output = ()> + Send + 'static) {
    support::run_async_test("karma-transfer-stage", || async move {
        nucleus::execution::Execution::new([37; 32], START)
            .unwrap()
            .scope(test)
            .await;
    });
}

fn time(offset: i64) {
    nucleus::execution::current()
        .unwrap()
        .set_time(START + offset)
        .unwrap();
}

async fn fixture() -> (Engine, Person, TransferFixture) {
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
    (engine, a, transfer)
}

#[test]
fn agreement_by_an_unread_participant_does_not_cancel_the_own_stage() {
    run(async {
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
        assign(&engine, &transfer, &a, 1).await;
        let parent = save(&engine).await;
        fire(&engine, &parent, 0).await.unwrap();
        drain(&engine).await;
        let schedule = store::karma_schedules::list(&engine.store.pool)
            .await
            .unwrap()
            .pop()
            .unwrap();
        let origin =
            store::karma_stages::for_rule(&engine.store.pool, &schedule.boundaries[0].rule)
                .await
                .unwrap()
                .unwrap();
        assert_eq!(
            origin.evaluation["guards"][&transfer.transfer]["participants"]
                .as_object()
                .unwrap()
                .len(),
            1
        );
        assert!(origin.evaluation["guards"][&transfer.transfer]["ready"].is_null());
        time(DAY);
        store::transfers::assign_agreement(
            &engine.store.pool,
            store::transfers::AgreementTargetInput {
                transition: store::transfers::AgreementTransitionInput {
                    transfer_uid: transfer.transfer.clone(),
                    expected_revision: transfer.revision,
                    idempotency_key: "other-participant-agrees".into(),
                    person_uid: b.uid,
                    to_level: 2,
                    authorization_intent_uid: None,
                },
                expected: None,
                expected_state: None,
            },
            nucleus::execution::now(),
            |hash| Some(b.signer.sign_hash(hash)),
        )
        .await
        .unwrap();
        time(3 * DAY);
        fire(&engine, &schedule.boundaries[0].rule, 3 * DAY)
            .await
            .unwrap();
        drain(&engine).await;
        assert_eq!(level(&engine, &transfer, &a).await, 2);
        assert_eq!(
            store::karma_schedules::get(&engine.store.pool, &schedule.uid)
                .await
                .unwrap()
                .unwrap()
                .boundaries[0]
                .status,
            "retired"
        );
    });
}

async fn assign(engine: &Engine, transfer: &TransferFixture, a: &Person, level: u8) {
    engine
        .act(
            Action::AssignTransferAgreementLevel {
                transfer: transfer.transfer.clone(),
                person: Some(a.uid.clone()),
                expected_revision: transfer.revision,
                request_id: nucleus::new_uid("assign"),
                level,
                expected: None,
                expected_state: None,
            },
            None,
        )
        .await
        .unwrap();
}

async fn save(engine: &Engine) -> String {
    engine
        .act(
            Action::SaveKarmaRule {
                rule: None,
                expected_revision: None,
                identity: None,
                fields: [
                    "agreement_level(@trade, @a) == 1",
                    "!= 0",
                    "@trade: agreement(@a, 2, 3d)",
                ]
                .map(|source| RuleFieldInput::Text {
                    source: source.into(),
                }),
                request_id: nucleus::new_uid("save"),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap()
}

async fn fire(engine: &Engine, rule: &str, due: i64) -> Result<(), engine::EngineError> {
    engine
        .act(
            Action::ApplyRecurrenceOccurrence {
                recurrence: rule.into(),
                due_at: chrono::DateTime::from_timestamp_millis(START + due)
                    .unwrap()
                    .to_rfc3339(),
                amount: None,
                note: None,
            },
            None,
        )
        .await
        .map(|_| ())
}

async fn drain(engine: &Engine) {
    for _ in 0..8 {
        let result = engine.run_database_effects().await.unwrap();
        assert!(!result.unsupported);
        assert!(
            result.outcomes.iter().all(|outcome| outcome.ok),
            "{:?}",
            result.outcomes
        );
        if !result.pending {
            return;
        }
    }
    panic!("Stages did not settle");
}

async fn level(engine: &Engine, transfer: &TransferFixture, a: &Person) -> i64 {
    store::sqlx::query_scalar("SELECT a.level FROM transfer_agreement a JOIN transfer_party p ON p.uid = a.party_uid WHERE a.transfer_uid = ? AND p.actor_uid = ? AND a.revision = ?")
        .bind(&transfer.transfer).bind(&a.uid).bind(transfer.revision as i64).fetch_one(&engine.store.pool).await.unwrap()
}

#[test]
fn stages_reuse_one_schedule_and_keep_the_actual_change_delay_after_downtime() {
    run(async {
        let (engine, a, transfer) = fixture().await;
        time(10 * DAY);
        assign(&engine, &transfer, &a, 1).await;
        let parent = save(&engine).await;
        fire(&engine, &parent, 10 * DAY).await.unwrap();
        drain(&engine).await;
        let schedule = store::karma_schedules::list(&engine.store.pool)
            .await
            .unwrap()
            .pop()
            .unwrap();
        let child = &schedule.boundaries[0];
        assert_eq!(child.intended_at_ms, START + 13 * DAY);
        let origin = store::karma_stages::for_rule(&engine.store.pool, &child.rule)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(origin.due_at_ms, START + 13 * DAY);
        time(10 * DAY + 1_000);
        engine
            .act(
                Action::SetSlug {
                    target: a.uid.clone(),
                    slug: Some("renamed-a".into()),
                },
                None,
            )
            .await
            .unwrap();
        drain(&engine).await;
        assert_eq!(
            store::karma_schedules::list(&engine.store.pool)
                .await
                .unwrap()
                .len(),
            1
        );
        time(12 * DAY);
        fire(&engine, &child.rule, 13 * DAY).await.unwrap();
        drain(&engine).await;
        assert_eq!(level(&engine, &transfer, &a).await, 1);
        time(14 * DAY);
        fire(&engine, &child.rule, 13 * DAY).await.unwrap();
        drain(&engine).await;
        assert_eq!(level(&engine, &transfer, &a).await, 2);
        assert_eq!(
            store::karma_schedules::get(&engine.store.pool, &schedule.uid)
                .await
                .unwrap()
                .unwrap()
                .boundaries[0]
                .status,
            "retired"
        );
    });
}

#[test]
fn retreat_and_return_refuse_the_original_stage_guard() {
    run(async {
        let (engine, a, transfer) = fixture().await;
        assign(&engine, &transfer, &a, 1).await;
        let parent = save(&engine).await;
        fire(&engine, &parent, 0).await.unwrap();
        drain(&engine).await;
        let old = store::karma_schedules::list(&engine.store.pool)
            .await
            .unwrap()
            .pop()
            .unwrap();
        time(DAY);
        assign(&engine, &transfer, &a, 0).await;
        assign(&engine, &transfer, &a, 1).await;
        drain(&engine).await;
        time(3 * DAY);
        assert!(
            fire(&engine, &old.boundaries[0].rule, 3 * DAY)
                .await
                .is_err()
        );
        assert_eq!(level(&engine, &transfer, &a).await, 1);
        assert!(
            store::karma_schedules::get(&engine.store.pool, &old.uid)
                .await
                .unwrap()
                .unwrap()
                .cancelled
        );
        assert_eq!(
            store::karma_schedules::list(&engine.store.pool)
                .await
                .unwrap()
                .len(),
            2
        );
    });
}

#[test]
fn pausing_resuming_editing_and_deleting_the_parent_cannot_revive_old_stages() {
    run(async {
        let (engine, a, transfer) = fixture().await;
        assign(&engine, &transfer, &a, 1).await;
        let parent = save(&engine).await;
        fire(&engine, &parent, 0).await.unwrap();
        drain(&engine).await;
        let old = store::karma_schedules::list(&engine.store.pool)
            .await
            .unwrap()
            .pop()
            .unwrap();
        engine
            .act(
                Action::SetRecurrencePaused {
                    recurrence: parent.clone(),
                    expected_revision: 1,
                    request_id: "pause".into(),
                    paused: true,
                },
                None,
            )
            .await
            .unwrap();
        assert!(
            store::karma_schedules::get(&engine.store.pool, &old.uid)
                .await
                .unwrap()
                .unwrap()
                .cancelled
        );
        engine
            .act(
                Action::SetRecurrencePaused {
                    recurrence: parent.clone(),
                    expected_revision: 2,
                    request_id: "resume".into(),
                    paused: false,
                },
                None,
            )
            .await
            .unwrap();
        fire(&engine, &parent, 0).await.unwrap();
        drain(&engine).await;
        let schedule = store::karma_schedules::list(&engine.store.pool)
            .await
            .unwrap()
            .into_iter()
            .find(|value| !value.cancelled)
            .unwrap();
        engine
            .act(
                Action::SaveKarmaRule {
                    rule: Some(parent.clone()),
                    expected_revision: Some(3),
                    identity: None,
                    fields: [
                        "agreement_level(@trade, @a) == 1",
                        "!= 0",
                        "@trade: agreement(@a, 2, 5d)",
                    ]
                    .map(|source| RuleFieldInput::Text {
                        source: source.into(),
                    }),
                    request_id: "edit-stage".into(),
                },
                None,
            )
            .await
            .unwrap();
        assert!(
            store::karma_schedules::get(&engine.store.pool, &schedule.uid)
                .await
                .unwrap()
                .unwrap()
                .cancelled
        );
        fire(&engine, &parent, 0).await.unwrap();
        drain(&engine).await;
        engine
            .act(Action::DeleteRecurrence { recurrence: parent }, None)
            .await
            .unwrap();
        assert!(
            store::karma_schedules::list(&engine.store.pool)
                .await
                .unwrap()
                .iter()
                .all(|value| value.cancelled)
        );
        time(10 * DAY);
        fire(&engine, &old.boundaries[0].rule, 3 * DAY)
            .await
            .unwrap();
        assert_eq!(level(&engine, &transfer, &a).await, 1);
    });
}
