mod support;

use engine::{Engine, actions::Action};
use nucleus::karma::rule_field::RuleFieldInput;
use nucleus::transfer::AgreementType;
use store::sqlx::{self, Row};
use support::{DraftOptions, Person, TransferFixture};

fn run(test: impl std::future::Future<Output = ()> + Send + 'static) {
    support::run_async_test("karma-transfer-state", || async move {
        nucleus::execution::Execution::new([36; 32], 1_893_456_000_000)
            .unwrap()
            .scope(test)
            .await;
    });
}

async fn fixture() -> (Engine, Person, Person, TransferFixture) {
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
    (engine, a, b, transfer)
}

fn publish(transfer: &TransferFixture, person: &Person, request: &str) -> Action {
    Action::PublishTransfer {
        transfer: transfer.transfer.clone(),
        expected_revision: transfer.revision,
        request_id: request.into(),
        person: Some(person.uid.clone()),
        expected: None,
        expected_state: None,
    }
}

fn activate(transfer: &TransferFixture, person: &Person, request: &str) -> Action {
    Action::ActivateTransferFulfillment {
        transfer: transfer.transfer.clone(),
        promise: "promise-a".into(),
        fulfillment: "purchase-once".into(),
        expected_revision: transfer.revision,
        request_id: request.into(),
        person: Some(person.uid.clone()),
        expected: None,
        expected_state: None,
    }
}

async fn agree(engine: &Engine, transfer: &TransferFixture, person: &Person) {
    engine.set_signer(person.signer.clone()).await.unwrap();
    engine
        .act(
            Action::AssignTransferAgreementLevel {
                transfer: transfer.transfer.clone(),
                expected_revision: transfer.revision,
                request_id: nucleus::new_uid("agreement"),
                person: Some(person.uid.clone()),
                level: 2,
                expected: None,
                expected_state: None,
            },
            None,
        )
        .await
        .unwrap();
}

#[test]
fn publication_preserves_terms_and_repetition_adds_no_revision() {
    run(async {
        let (engine, a, _, mut transfer) = fixture().await;
        let revisions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM transfer_revision WHERE transfer_uid = ?")
            .bind(&transfer.transfer).fetch_one(&engine.store.pool).await.unwrap();
        let promises = sqlx::query("SELECT uid, record_uid, concept_uid, party_uid, delta, item_json FROM promise WHERE transfer_uid = ? ORDER BY uid")
            .bind(&transfer.transfer).fetch_all(&engine.store.pool).await.unwrap();
        let before: Vec<_> = promises
            .iter()
            .map(|row| {
                (
                    row.get::<String, _>("uid"),
                    row.get::<Option<String>, _>("record_uid"),
                    row.get::<Option<String>, _>("concept_uid"),
                    row.get::<Option<String>, _>("party_uid"),
                    row.get::<f64, _>("delta"),
                    row.get::<Option<String>, _>("item_json"),
                )
            })
            .collect();
        let command = publish(&transfer, &a, "publish-once");
        let result = engine.act(command.clone(), None).await.unwrap();
        assert_eq!(result.data.unwrap()["published"], true);
        let hash = engine.store.state_hash().await.unwrap();
        engine.act(command, None).await.unwrap();
        assert_eq!(engine.store.state_hash().await.unwrap(), hash);
        transfer.revision = support::current_revision(&engine, &transfer.transfer).await;
        engine
            .act(publish(&transfer, &a, "already-public"), None)
            .await
            .unwrap();
        assert_eq!(engine.store.state_hash().await.unwrap(), hash);
        let after = sqlx::query("SELECT uid, record_uid, concept_uid, party_uid, delta, item_json FROM promise WHERE transfer_uid = ? ORDER BY uid")
            .bind(&transfer.transfer).fetch_all(&engine.store.pool).await.unwrap();
        let after: Vec<_> = after
            .iter()
            .map(|row| {
                (
                    row.get::<String, _>("uid"),
                    row.get::<Option<String>, _>("record_uid"),
                    row.get::<Option<String>, _>("concept_uid"),
                    row.get::<Option<String>, _>("party_uid"),
                    row.get::<f64, _>("delta"),
                    row.get::<Option<String>, _>("item_json"),
                )
            })
            .collect();
        assert_eq!(after, before);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM transfer_revision WHERE transfer_uid = ?"
            )
            .bind(&transfer.transfer)
            .fetch_one(&engine.store.pool)
            .await
            .unwrap(),
            revisions + 1
        );
        assert!(sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM visibility_rule WHERE target_uid = ? AND subject_kind = 'public' AND grant_level = 'visible')").bind(&transfer.transfer).fetch_one(&engine.store.pool).await.unwrap());
    });
}

#[test]
fn fulfillment_keys_keep_one_activation_and_current_readiness_is_required() {
    run(async {
        let (engine, a, b, transfer) = fixture().await;
        assert!(
            engine
                .act(activate(&transfer, &a, "not-ready"), None)
                .await
                .is_err()
        );
        agree(&engine, &transfer, &a).await;
        agree(&engine, &transfer, &b).await;
        engine.set_signer(a.signer.clone()).await.unwrap();
        let first = engine
            .act(activate(&transfer, &a, "first-firing"), None)
            .await
            .unwrap();
        let before = engine.store.state_hash().await.unwrap();
        let repeated = engine
            .act(activate(&transfer, &a, "another-firing"), None)
            .await
            .unwrap();
        assert_eq!(first.created, repeated.created);
        assert!(repeated.facts.is_empty());
        assert_eq!(engine.store.state_hash().await.unwrap(), before);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM transfer_occurrence WHERE transfer_uid = ?"
            )
            .bind(&transfer.transfer)
            .fetch_one(&engine.store.pool)
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM transfer_activation_event WHERE transfer_uid = ?"
            )
            .bind(&transfer.transfer)
            .fetch_one(&engine.store.pool)
            .await
            .unwrap(),
            1
        );
        let snapshot = engine.store.logical_snapshot().await.unwrap();
        assert!(
            engine
                .act(publish(&transfer, &a, "active-publication"), None)
                .await
                .is_err()
        );
        let after = engine.store.logical_snapshot().await.unwrap();
        let changes: Vec<_> = snapshot.iter().zip(&after).filter(|(before, after)| before != after).map(|(before, after)| (&before.name, &before.rows, &after.rows)).collect();
        assert_eq!(engine.store.state_hash().await.unwrap(), before, "{changes:?}");
    });
}

#[test]
fn publication_rules_reach_a_fixed_point_through_the_database_adapter() {
    run(async {
        let (engine, _, _, transfer) = fixture().await;
        let rule = engine
            .act(
                Action::SaveKarmaRule {
                    rule: None,
                    expected_revision: None,
                    identity: None,
                    fields: [
                        "1 + 0 * transfer_published(@trade)",
                        "always",
                        "@trade: publish(@a)",
                    ]
                    .map(|source| RuleFieldInput::Text {
                        source: source.into(),
                    }),
                    request_id: "publish-rule".into(),
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        engine
            .act(
                Action::ApplyRecurrenceOccurrence {
                    recurrence: rule,
                    due_at: "2030-01-01T00:00:00Z".into(),
                    amount: None,
                    note: None,
                },
                None,
            )
            .await
            .unwrap();
        for _ in 0..4 {
            let result = engine.run_database_effects().await.unwrap();
            assert!(!result.unsupported);
            assert!(
                result.outcomes.iter().all(|outcome| outcome.ok),
                "{:?}",
                result.outcomes
            );
            if !result.pending {
                break;
            }
        }
        assert!(!engine.run_database_effects().await.unwrap().pending);
        assert_eq!(
            support::current_revision(&engine, &transfer.transfer).await,
            transfer.revision + 1
        );
    });
}
