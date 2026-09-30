mod support;

use engine::{
    Engine,
    actions::{Action, ActionOutcome},
};
use nucleus::transfer::{AgreementGuard, AgreementType};
use store::sqlx::{self, Row};
use support::{DraftOptions, Person, TransferFixture};

fn run(test: impl std::future::Future<Output = ()> + Send + 'static) {
    support::run_async_test("karma-transfer-target", || async move {
        nucleus::execution::Execution::new([34; 32], 1_893_456_000_000)
            .unwrap()
            .scope(test)
            .await;
    });
}

async fn fixture() -> (Engine, Person, Person, TransferFixture) {
    let engine = support::engine().await;
    let a = support::person(&engine, "a").await;
    let b = support::person(&engine, "b").await;
    let record = support::plain(&engine, "item", 10.0).await;
    let transfer = support::create_transfer(
        &engine,
        &a,
        &[b.clone()],
        vec![
            support::promise("promise-a", &record, &a, -1.0),
            support::promise("promise-b", &record, &b, 1.0),
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

fn action(
    transfer: &TransferFixture,
    person: &Person,
    request: &str,
    level: u8,
    expected: Option<AgreementGuard>,
) -> Action {
    Action::AssignTransferAgreementLevel {
        transfer: transfer.transfer.clone(),
        expected_revision: transfer.revision,
        request_id: request.into(),
        person: Some(person.uid.clone()),
        level,
        expected,
        expected_state: None,
    }
}

async fn guard(engine: &Engine, transfer: &TransferFixture, person: &Person) -> AgreementGuard {
    let row = sqlx::query("SELECT a.level, a.last_event_uid FROM transfer_agreement a JOIN transfer_party p ON p.uid = a.party_uid WHERE a.transfer_uid = ? AND p.actor_uid = ? AND a.revision = ?")
        .bind(&transfer.transfer).bind(&person.uid).bind(transfer.revision as i64)
        .fetch_optional(&engine.store.pool).await.unwrap();
    AgreementGuard {
        level: row
            .as_ref()
            .map_or(0, |row| row.get::<i64, _>("level") as u8),
        change_uid: row.and_then(|row| row.get("last_event_uid")),
    }
}

fn changes(outcome: &ActionOutcome) -> &Vec<serde_json::Value> {
    outcome.data.as_ref().unwrap()["changes"]
        .as_array()
        .unwrap()
}

#[test]
fn direct_target_preserves_steps_and_same_target_is_a_noop() {
    run(async {
        let (engine, a, _, transfer) = fixture().await;
        let command = action(&transfer, &a, "set-maximum", 2, None);
        let outcome = engine.act(command.clone(), None).await.unwrap();
        assert_eq!(changes(&outcome).len(), 2);
        assert_eq!(changes(&outcome)[0]["from_level"], 0);
        assert_eq!(changes(&outcome)[1]["to_level"], 2);
        assert_eq!(outcome.facts.len(), 2);
        let source = guard(&engine, &transfer, &a).await;
        let noop = engine
            .act(
                action(&transfer, &a, "maximum-again", 2, Some(source.clone())),
                None,
            )
            .await
            .unwrap();
        assert!(changes(&noop).is_empty());
        assert!(noop.facts.is_empty());
        assert_eq!(guard(&engine, &transfer, &a).await, source);
        let before = engine.store.state_hash().await.unwrap();
        let replayed = engine.act(command, None).await.unwrap();
        assert_eq!(outcome.data, replayed.data);
        assert!(replayed.facts.is_empty());
        assert_eq!(engine.store.state_hash().await.unwrap(), before);
        let retreat = engine
            .act(action(&transfer, &a, "retreat", 0, None), None)
            .await
            .unwrap();
        assert_eq!(changes(&retreat).len(), 2);
        assert_eq!(guard(&engine, &transfer, &a).await.level, 0);
    });
}

#[test]
fn a_refused_second_step_rolls_back_the_entire_assignment() {
    run(async {
        let (engine, a, _, transfer) = fixture().await;
        sqlx::query("CREATE TRIGGER refuse_maximum BEFORE INSERT ON transfer_agreement_event WHEN NEW.to_level = 2 BEGIN SELECT RAISE(ABORT, 'test refusal'); END")
            .execute(&engine.store.pool).await.unwrap();
        let before = engine.store.state_hash().await.unwrap();
        assert!(
            engine
                .act(action(&transfer, &a, "atomic", 2, None), None)
                .await
                .is_err()
        );
        assert_eq!(engine.store.state_hash().await.unwrap(), before);
        assert_eq!(guard(&engine, &transfer, &a).await.level, 0);
        sqlx::query("DROP TRIGGER refuse_maximum")
            .execute(&engine.store.pool)
            .await
            .unwrap();
        assert_eq!(
            changes(
                &engine
                    .act(action(&transfer, &a, "atomic", 2, None), None)
                    .await
                    .unwrap()
            )
            .len(),
            2
        );
    });
}

#[test]
fn returning_to_the_same_level_does_not_restore_an_old_guard() {
    run(async {
        let (engine, a, _, transfer) = fixture().await;
        let old = guard(&engine, &transfer, &a).await;
        engine
            .act(action(&transfer, &a, "up", 1, None), None)
            .await
            .unwrap();
        engine
            .act(action(&transfer, &a, "down", 0, None), None)
            .await
            .unwrap();
        let fresh = guard(&engine, &transfer, &a).await;
        assert_eq!(old.level, fresh.level);
        assert_ne!(old.change_uid, fresh.change_uid);
        let before = engine.store.state_hash().await.unwrap();
        let refusal = engine
            .act(action(&transfer, &a, "stale", 2, Some(old)), None)
            .await
            .unwrap_err();
        assert_eq!(refusal.code(), Some("transfer_agreement_source_changed"));
        assert_eq!(engine.store.state_hash().await.unwrap(), before);
        engine
            .act(action(&transfer, &a, "fresh", 2, Some(fresh)), None)
            .await
            .unwrap();
    });
}

#[test]
fn request_conflicts_and_another_persons_agreement_are_refused() {
    run(async {
        let (engine, a, b, transfer) = fixture().await;
        engine
            .act(action(&transfer, &a, "target", 1, None), None)
            .await
            .unwrap();
        let before = engine.store.state_hash().await.unwrap();
        assert!(
            engine
                .act(action(&transfer, &a, "target", 2, None), None)
                .await
                .is_err()
        );
        assert!(
            engine
                .act(action(&transfer, &b, "wrong-author", 0, None), None)
                .await
                .is_err()
        );
        assert!(
            engine
                .act(action(&transfer, &a, "create:trade", 2, None), None)
                .await
                .is_err()
        );
        assert!(
            engine
                .act(
                    Action::SetTransferAgreementLevel {
                        transfer: transfer.transfer.clone(),
                        expected_revision: transfer.revision,
                        request_id: "target".into(),
                        person: Some(a.uid.clone()),
                        level: 2,
                    },
                    None
                )
                .await
                .is_err()
        );
        assert_eq!(engine.store.state_hash().await.unwrap(), before);
    });
}

async fn reading(engine: &Engine, source: &str, actor: Option<&str>) -> nucleus::DecimalValue {
    let outcome = engine
        .act(
            Action::PreviewKarmaReading {
                source: source.into(),
            },
            actor.map(str::to_owned),
        )
        .await
        .unwrap();
    nucleus::DecimalValue::parse_inferred(outcome.data.unwrap()["value"].as_str().unwrap()).unwrap()
}

fn number(value: &str) -> nucleus::DecimalValue {
    nucleus::DecimalValue::parse_inferred(value).unwrap()
}

#[test]
fn readings_use_transfer_state_and_the_actual_change_time() {
    run(async {
        let (engine, a, b, transfer) = fixture().await;
        let before = engine.store.state_hash().await.unwrap();
        assert_eq!(
            reading(&engine, "agreement_level(@trade, @a)", None).await,
            number("0")
        );
        assert_eq!(
            reading(&engine, "transfer_revision(@trade)", None).await,
            number(&transfer.revision.to_string())
        );
        assert_eq!(
            reading(
                &engine,
                "transfer_active(@trade) + transfer_published(@trade) + transfer_ready(@trade)",
                None
            )
            .await,
            number("0")
        );
        assert_eq!(engine.store.state_hash().await.unwrap(), before);
        engine
            .act(action(&transfer, &a, "checked", 1, None), None)
            .await
            .unwrap();
        assert_eq!(
            reading(&engine, "agreement_changed_at(@trade, @a)", None).await,
            number("1893456000000")
        );
        nucleus::execution::current()
            .unwrap()
            .set_time(1_893_456_001_500)
            .unwrap();
        let age = reading(&engine, "agreement_age(@trade, @a)", None).await;
        assert_eq!(age.to_f64(), 1.5);
        engine
            .act(action(&transfer, &a, "still-checked", 1, None), None)
            .await
            .unwrap();
        assert_eq!(
            reading(&engine, "agreement_changed_at(@trade, @a)", None).await,
            number("1893456000000")
        );
        assert_eq!(
            reading(
                &engine,
                "1 * (agreement_level(@trade, @a) == 0) + 2 * (agreement_level(@trade, @a) == 1)",
                None
            )
            .await,
            number("2")
        );
        engine
            .act(
                Action::SetQuantityExact {
                    target: a.uid.clone(),
                    amount: "99".into(),
                },
                None,
            )
            .await
            .unwrap();
        assert_eq!(
            reading(&engine, "agreement_level(@trade, @a)", None).await,
            number("1")
        );
        engine
            .act(action(&transfer, &a, "a-agreed", 2, None), None)
            .await
            .unwrap();
        engine.set_signer(b.signer.clone()).await.unwrap();
        engine
            .act(action(&transfer, &b, "b-agreed", 2, None), None)
            .await
            .unwrap();
        assert_eq!(
            reading(&engine, "transfer_ready(@trade)", None).await,
            number("1")
        );
        engine.set_signer(a.signer.clone()).await.unwrap();
        engine
            .act(
                Action::ActivateTransferOccurrence {
                    transfer: transfer.transfer.clone(),
                    promise: "promise-a".into(),
                    expected_revision: transfer.revision,
                    request_id: "active-fulfillment".into(),
                    person: Some(a.uid.clone()),
                },
                None,
            )
            .await
            .unwrap();
        assert_eq!(
            reading(&engine, "transfer_active(@trade)", None).await,
            number("1")
        );
        sqlx::query("UPDATE record SET quantity_mantissa = 0, quantity_scale = 0 WHERE uid = ?")
            .bind(&transfer.transfer).execute(&engine.store.pool).await.unwrap();
        assert_eq!(reading(&engine, "transfer_active(@trade)", None).await, number("1"));
        assert_eq!(
            store::records::get(&engine.store.pool, &transfer.transfer)
                .await
                .unwrap()
                .unwrap()
                .quantity,
            number("0")
        );
    });
}

#[test]
fn agreement_changes_wake_bound_rules_after_names_change() {
    run(async {
        let (engine, a, b, mut transfer) = fixture().await;
        let result = support::plain(&engine, "result", 0.0).await;
        let uid = engine
            .act(
                Action::SaveKarmaRule {
                    rule: None,
                    expected_revision: None,
                    identity: None,
                    fields: ["agreement_level(@trade, @a) * 2", "always", "@result"].map(
                        |source| nucleus::karma::rule_field::RuleFieldInput::Text {
                            source: source.into(),
                        },
                    ),
                    request_id: "save-agreement-reading".into(),
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let rule = store::recurrence::get(&engine.store.pool, &uid)
            .await
            .unwrap()
            .unwrap();
        let bindings = &rule.condition.as_ref().unwrap().bindings;
        assert!(bindings.iter().any(|binding| binding.target.kind()
            == nucleus::karma::ReferenceKind::Transfer
            && binding.target.as_str() == transfer.transfer));
        assert!(bindings.iter().any(|binding| binding.target.kind()
            == nucleus::karma::ReferenceKind::Person
            && binding.target.as_str() == a.uid));
        engine.act(Action::CounterofferTransfer {
            transfer: transfer.transfer.clone(),
            expected_revision: transfer.revision,
            request_id: "rename-transfer".into(),
            person: Some(a.uid.clone()),
            draft: engine::actions::TransferDraftRevisionInput {
                creator: a.uid.clone(),
                slug: Some("renamed-trade".into()),
                head: "Renamed Trade".into(),
                agreement: AgreementType::Full,
                agreement_pct: None,
                satiation: engine::actions::TransferSatiation::None,
                parent: None,
                source: None,
                visibility: engine::actions::TransferVisibility::Hidden,
                max_proximity: None,
                reserve_default: engine::actions::TransferReservePoint::Active,
                require_confirmation: true,
                default_place: None,
                invitees: Vec::new(),
                promises: vec![
                    support::promise("promise-a", "item", &a, -1.0),
                    support::promise("promise-b", "item", &b, 1.0),
                ],
                dependencies: Vec::new(),
            },
        }, None).await.unwrap();
        transfer.revision = support::current_revision(&engine, &transfer.transfer).await;
        engine.act(Action::SetSlug { target: a.uid.clone(), slug: Some("renamed-a".into()) }, None).await.unwrap();
        support::person(&engine, "a").await;
        support::plain(&engine, "trade", 77.0).await;
        engine
            .act(action(&transfer, &a, "state-changed", 1, None), None)
            .await
            .unwrap();
        assert_eq!(
            store::records::get(&engine.store.pool, &result)
                .await
                .unwrap()
                .unwrap()
                .quantity,
            number("2")
        );
        engine
            .act(action(&transfer, &a, "state-changed-again", 2, None), None)
            .await
            .unwrap();
        assert_eq!(
            store::records::get(&engine.store.pool, &result)
                .await
                .unwrap()
                .unwrap()
                .quantity,
            number("4")
        );
    });
}

#[test]
fn missing_participants_and_revoked_read_permission_are_errors() {
    run(async {
        let (engine, _, b, _) = fixture().await;
        let role = store::auth::ensure_role(&engine.store.pool, "agreement-reader")
            .await
            .unwrap();
        for resource in ["record", "transfer"] {
            let permission = store::auth::ensure_permission(&engine.store.pool, resource, "read")
                .await
                .unwrap();
            store::auth::grant(&engine.store.pool, role, permission)
                .await
                .unwrap();
        }
        store::auth::create_credential(&engine.store.pool, &b.uid, "b", "test-hash", role)
            .await
            .unwrap();
        assert_eq!(
            reading(&engine, "agreement_level(@trade, @b)", Some(&b.uid)).await,
            number("0")
        );
        support::person(&engine, "outsider").await;
        let before = engine.store.state_hash().await.unwrap();
        let missing = engine
            .act(
                Action::PreviewKarmaReading {
                    source: "agreement_level(@trade, @outsider)".into(),
                },
                Some(b.uid.clone()),
            )
            .await
            .unwrap_err();
        assert!(missing.to_string().contains("unavailable or hidden"));
        assert_eq!(engine.store.state_hash().await.unwrap(), before);
        let permission = store::auth::ensure_permission(&engine.store.pool, "transfer", "read")
            .await
            .unwrap();
        store::auth::revoke(&engine.store.pool, role, permission)
            .await
            .unwrap();
        let before = engine.store.state_hash().await.unwrap();
        assert!(
            engine
                .act(
                    Action::PreviewKarmaReading {
                        source: "agreement_level(@trade, @b)".into()
                    },
                    Some(b.uid.clone())
                )
                .await
                .is_err()
        );
        assert_eq!(engine.store.state_hash().await.unwrap(), before);
        for source in [
            "agreement_level(@trade)",
            "agreement_level(@trade, @b, @outsider)",
            "transfer_active(@trade, @b)",
        ] {
            assert!(nucleus::karma::Condition::parse(source).is_err());
        }
    });
}
