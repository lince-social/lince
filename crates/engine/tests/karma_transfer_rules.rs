mod support;

use engine::{Engine, actions::Action};
use nucleus::karma::rule_field::RuleFieldInput;
use nucleus::transfer::AgreementType;
use store::sqlx;
use support::{DraftOptions, Person, TransferFixture};

fn run(test: impl std::future::Future<Output = ()> + Send + 'static) {
    support::run_async_test("karma-transfer-rule", || async move {
        nucleus::execution::Execution::new([35; 32], 1_893_456_000_000)
            .unwrap()
            .scope(test)
            .await;
    });
}

async fn fixture() -> (Engine, Person, TransferFixture) {
    let engine = support::engine().await;
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

async fn save(engine: &Engine, condition: &str, gate: &str, consequence: &str) -> String {
    engine
        .act(
            Action::SaveKarmaRule {
                rule: None,
                expected_revision: None,
                identity: None,
                fields: [condition, gate, consequence].map(|source| RuleFieldInput::Text {
                    source: source.into(),
                }),
                request_id: nucleus::new_uid("request"),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap()
}

async fn fire(engine: &Engine, rule: &str) -> Result<(), engine::EngineError> {
    engine
        .act(
            Action::ApplyRecurrenceOccurrence {
                recurrence: rule.into(),
                due_at: "2030-01-01T00:00:00Z".into(),
                amount: None,
                note: None,
            },
            None,
        )
        .await
        .map(|_| ())
}

async fn level(engine: &Engine, transfer: &TransferFixture, person: &Person) -> u8 {
    sqlx::query_scalar::<_, i64>("SELECT COALESCE((SELECT a.level FROM transfer_agreement a JOIN transfer_party p ON p.uid = a.party_uid WHERE a.transfer_uid = ? AND a.revision = ? AND p.actor_uid = ?), 0)")
        .bind(&transfer.transfer).bind(transfer.revision as i64).bind(&person.uid)
        .fetch_one(&engine.store.pool).await.unwrap() as u8
}

async fn assign(engine: &Engine, transfer: &TransferFixture, person: &Person, target: u8) {
    engine
        .act(
            Action::AssignTransferAgreementLevel {
                transfer: transfer.transfer.clone(),
                expected_revision: transfer.revision,
                request_id: nucleus::new_uid("request"),
                person: Some(person.uid.clone()),
                level: target,
                expected: None,
                expected_state: None,
            },
            None,
        )
        .await
        .unwrap();
}

async fn drain(engine: &Engine) {
    for _ in 0..10 {
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
    panic!("Transfer effects did not settle");
}

#[test]
fn condition_math_advances_the_existing_transfer_and_stops_at_maximum() {
    run(async {
        let (engine, a, transfer) = fixture().await;
        let rule = save(
            &engine,
            "1 * (agreement_level(@trade, @a) == 0) + 2 * (agreement_level(@trade, @a) == 1)",
            "> 0",
            "@trade: agreement(@a)",
        )
        .await;
        fire(&engine, &rule).await.unwrap();
        drain(&engine).await;
        assert_eq!(level(&engine, &transfer, &a).await, 2);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM transfer")
                .fetch_one(&engine.store.pool)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM transfer_agreement_event WHERE transfer_uid = ?"
            )
            .bind(&transfer.transfer)
            .fetch_one(&engine.store.pool)
            .await
            .unwrap(),
            2
        );
        let saved = store::recurrence::get(&engine.store.pool, &rule)
            .await
            .unwrap()
            .unwrap();
        assert!(
            saved
                .condition
                .unwrap()
                .bindings
                .iter()
                .any(|binding| binding.reading == "consequence.person.0"
                    && binding.target.as_str() == a.uid)
        );
        let history = engine
            .act(Action::InspectKarmaRuleHistory { rule, limit: 10 }, None)
            .await
            .unwrap()
            .data
            .unwrap();
        assert!(
            history["applications"]
                .as_array()
                .unwrap()
                .iter()
                .any(|application| application["effects"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|effect| effect["status"] == "done"
                        && effect["reason"] == "Transfer consequence committed"))
        );
    });
}

#[test]
fn fixed_zero_is_a_retreat_and_fractional_results_are_refused() {
    run(async {
        let (engine, a, transfer) = fixture().await;
        assign(&engine, &transfer, &a, 1).await;
        let rule = save(
            &engine,
            "agreement_level(@trade, @a) == 1",
            "!= 0",
            "@trade: agreement(@a, 0)",
        )
        .await;
        fire(&engine, &rule).await.unwrap();
        drain(&engine).await;
        assert_eq!(level(&engine, &transfer, &a).await, 0);
        let invalid = save(
            &engine,
            "1.5 + 0 * agreement_level(@trade, @a)",
            "always",
            "@trade: agreement(@a)",
        )
        .await;
        let before = engine.store.state_hash().await.unwrap();
        let error = fire(&engine, &invalid).await.unwrap_err();
        assert!(error.to_string().contains("whole number 0, 1 or 2"));
        assert_eq!(level(&engine, &transfer, &a).await, 0);
        assert_ne!(engine.store.state_hash().await.unwrap(), before);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM effect_queue WHERE payload LIKE ?")
                .bind(format!("%{invalid}%"))
                .fetch_one(&engine.store.pool)
                .await
                .unwrap(),
            0
        );
    });
}

#[test]
fn queued_changes_refuse_a_retreat_and_parent_pause_invalidates_work() {
    run(async {
        let (engine, a, transfer) = fixture().await;
        assign(&engine, &transfer, &a, 1).await;
        let rule = save(
            &engine,
            "agreement_level(@trade, @a) == 1",
            "!= 0",
            "@trade: agreement(@a, 2)",
        )
        .await;
        fire(&engine, &rule).await.unwrap();
        assign(&engine, &transfer, &a, 0).await;
        let result = engine.run_database_effects().await.unwrap();
        assert!(
            result
                .outcomes
                .iter()
                .any(|outcome| !outcome.ok && outcome.result.contains("changed after evaluation"))
        );
        assert_eq!(level(&engine, &transfer, &a).await, 0);
        let paused = save(
            &engine,
            "1 + 0 * agreement_level(@trade, @a)",
            "always",
            "@trade: agreement(@a, 2)",
        )
        .await;
        fire(&engine, &paused).await.unwrap();
        engine
            .act(
                Action::SetRecurrencePaused {
                    recurrence: paused.clone(),
                    expected_revision: 1,
                    request_id: "pause-target".into(),
                    paused: true,
                },
                None,
            )
            .await
            .unwrap();
        engine
            .act(
                Action::SetRecurrencePaused {
                    recurrence: paused,
                    expected_revision: 2,
                    request_id: "resume-target".into(),
                    paused: false,
                },
                None,
            )
            .await
            .unwrap();
        let result = engine.run_database_effects().await.unwrap();
        assert!(result.outcomes.iter().any(|outcome| !outcome.ok));
        assert_eq!(level(&engine, &transfer, &a).await, 0);
    });
}

#[test]
fn repeated_targets_are_noops_and_editor_person_bindings_survive_name_reuse() {
    run(async {
        let (engine, a, transfer) = fixture().await;
        let condition = "agreement_level(@trade, @a) >= 0";
        let consequence = "@trade: agreement(@a, 2)";
        let rule = save(&engine, condition, "!= 0", consequence).await;
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
        support::person(&engine, "a").await;
        engine.set_signer(a.signer.clone()).await.unwrap();
        engine
            .act(
                Action::SaveKarmaRule {
                    rule: Some(rule.clone()),
                    expected_revision: Some(1),
                    identity: None,
                    fields: [condition, "> 0", consequence].map(|source| RuleFieldInput::Text {
                        source: source.into(),
                    }),
                    request_id: "edit-after-name-reuse".into(),
                },
                None,
            )
            .await
            .unwrap();
        let invalidated = engine.run_database_effects().await.unwrap();
        assert!(
            invalidated
                .outcomes
                .iter()
                .all(|outcome| !outcome.ok && outcome.result.contains("revised"))
        );
        assert_eq!(level(&engine, &transfer, &a).await, 0);
        fire(&engine, &rule).await.unwrap();
        drain(&engine).await;
        assert_eq!(level(&engine, &transfer, &a).await, 2);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM transfer_agreement_event WHERE transfer_uid = ?"
            )
            .bind(&transfer.transfer)
            .fetch_one(&engine.store.pool)
            .await
            .unwrap(),
            2
        );
        let first_effect: String = sqlx::query_scalar("SELECT uid FROM effect_queue WHERE json_extract(payload, '$.rule') = ? AND json_extract(payload, '$.revision') = 2 AND status = 'done' ORDER BY rowid LIMIT 1").bind(&rule).fetch_one(&engine.store.pool).await.unwrap();
        assign(&engine, &transfer, &a, 0).await;
        sqlx::query("UPDATE effect_queue SET status = 'cancelled' WHERE uid != ? AND json_extract(payload, '$.rule') = ? AND status = 'queued'")
            .bind(&first_effect).bind(&rule).execute(&engine.store.pool).await.unwrap();
        sqlx::query("UPDATE effect_queue SET status = 'queued' WHERE uid = ?")
            .bind(&first_effect)
            .execute(&engine.store.pool)
            .await
            .unwrap();
        let result = engine.run_database_effects().await.unwrap();
        assert!(
            result
                .outcomes
                .iter()
                .find(|outcome| outcome.uid == first_effect)
                .unwrap()
                .ok
        );
        assert_eq!(level(&engine, &transfer, &a).await, 0);
        assert!(!result.pending);
    });
}
