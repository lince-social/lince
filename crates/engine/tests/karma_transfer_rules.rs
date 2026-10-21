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
    let (engine, a, _, transfer) = fixture_parties().await;
    (engine, a, transfer)
}

async fn fixture_parties() -> (Engine, Person, Person, TransferFixture) {
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

async fn create_weekly(engine: &Engine) {
    engine
        .act(
            Action::CreateFrequency {
                slug: "weekly".into(),
                head: Some("Weekly".into()),
                every: nucleus::karma::CadenceStep {
                    weeks: 1,
                    ..Default::default()
                },
                anchor_at: Some("2030-01-01T00:00:00Z".into()),
                request_id: Some("create-weekly".into()),
            },
            None,
        )
        .await
        .unwrap();
}

async fn weekly(engine: &Engine, consequence: &str) -> String {
    create_weekly(engine).await;
    save(engine, "freq(@weekly)", "> 0", consequence).await
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

#[test]
fn need_reopening_uses_children_and_never_duplicates_unresolved_demand() {
    run(async {
        let engine = support::karma::engine().await;
        let owner = support::person(&engine, "receiver").await;
        let quantity = support::plain(&engine, "pantry", 8.0).await;
        engine.set_signer(owner.signer.clone()).await.unwrap();
        let template = engine.act(serde_json::from_value(serde_json::json!({
            "action":"create-transfer-draft","request_id":"need-template","creator":owner.uid,
            "head":"Recurrent food Need","source":quantity,"visibility":"hidden",
            "promises":[{"open":true,"delta":-8.0,"item":{"title":"Food contribution"}}]
        })).unwrap(), None).await.unwrap().created.unwrap();
        let rule = save(
            &engine,
            &format!("@{quantity}"),
            "< 7",
            &format!("@{template}: reopen(@{}, 14)", owner.uid),
        )
        .await;
        let count = || {
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM transfer WHERE parent_uid=?")
                .bind(&template)
                .fetch_one(&engine.store.pool)
        };
        fire(&engine, &rule).await.unwrap();
        drain(&engine).await;
        assert_eq!(count().await.unwrap(), 0);
        for (amount, expected) in [(7.0, 0), (6.0, 1)] {
            engine
                .act(
                    Action::SetQuantity {
                        target: quantity.clone(),
                        value: amount,
                    },
                    None,
                )
                .await
                .unwrap();
            drain(&engine).await;
            assert_eq!(count().await.unwrap(), expected);
        }
        fire(&engine, &rule).await.unwrap();
        drain(&engine).await;
        assert_eq!(count().await.unwrap(), 1);
        let delta: f64 = sqlx::query_scalar("SELECT p.delta FROM promise p JOIN transfer t ON t.record_uid=p.transfer_uid WHERE t.parent_uid=?").bind(&template).fetch_one(&engine.store.pool).await.unwrap();
        assert_eq!(delta, -8.0);
        let before = store::transfers::revision_fact(&engine.store.pool, &template, 1)
            .await
            .unwrap()
            .unwrap();
        let child: String =
            sqlx::query_scalar("SELECT record_uid FROM transfer WHERE parent_uid=?")
                .bind(&template)
                .fetch_one(&engine.store.pool)
                .await
                .unwrap();
        engine
            .act(
                Action::ReviseTransferDraft {
                    transfer: child.clone(),
                    expected_revision: 1,
                    request_id: "withdraw-reopened-need".into(),
                    draft: serde_json::from_value(serde_json::json!({
                        "creator":owner.uid,"head":"Closed food Need",
                        "parent":template,"source":quantity,"visibility":"hidden",
                        "promises":[]
                    }))
                    .unwrap(),
                },
                None,
            )
            .await
            .unwrap();
        assert!(
            store::location::transfer_ended(&engine.store.pool, &child)
                .await
                .unwrap()
        );
        engine
            .act(
                Action::SetQuantity {
                    target: quantity,
                    value: 5.0,
                },
                None,
            )
            .await
            .unwrap();
        drain(&engine).await;
        assert_eq!(count().await.unwrap(), 2);
        let original = store::transfers::revision_fact(&engine.store.pool, &template, 1)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(original.hash, before.hash);
        assert_eq!(original.payload, before.payload);
        assert_eq!(original.signature, before.signature);
        let revision = store::transfers::get(&engine.store.pool, &template)
            .await
            .unwrap()
            .unwrap()
            .revision as u64;
        let current = store::transfers::revision_fact(&engine.store.pool, &template, revision)
            .await
            .unwrap()
            .unwrap();
        let previous: serde_json::Value =
            serde_json::from_str(before.payload.as_deref().unwrap()).unwrap();
        let current: serde_json::Value =
            serde_json::from_str(current.payload.as_deref().unwrap()).unwrap();
        for field in ["transfer", "promises", "parties", "invitations"] {
            assert_eq!(current["terms"][field], previous["terms"][field], "{field}");
        }
    });
}

#[test]
fn scheduled_proposals_replay_once_and_do_not_invent_counterparty_agreement() {
    run(async {
        let (engine, a, transfer) = fixture().await;
        let rule = weekly(&engine, &format!("@trade: repeat(@{})", a.uid)).await;
        fire(&engine, &rule).await.unwrap();
        drain(&engine).await;
        fire(&engine, &rule).await.unwrap();
        drain(&engine).await;
        let children: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM transfer WHERE parent_uid=?")
            .bind(&transfer.transfer)
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
        assert_eq!(children, 1);
        let confirmed: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM transfer_occurrence o JOIN transfer t ON t.record_uid=o.transfer_uid WHERE t.parent_uid=?").bind(&transfer.transfer).fetch_one(&engine.store.pool).await.unwrap();
        assert_eq!(confirmed, 0);
        save(
            &engine,
            "@item",
            "> 0",
            &format!("@trade: repeat(@{})", a.uid),
        )
        .await;
        engine
            .act(
                Action::SetQuantity {
                    target: "item".into(),
                    value: 9.0,
                },
                None,
            )
            .await
            .unwrap();
        drain(&engine).await;
        let children: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM transfer WHERE parent_uid=?")
            .bind(&transfer.transfer)
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
        assert_eq!(children, 1);
        engine
            .act(
                Action::ApplyRecurrenceOccurrence {
                    recurrence: rule,
                    due_at: "2030-01-08T00:00:00Z".into(),
                    amount: None,
                    note: None,
                },
                None,
            )
            .await
            .unwrap();
        drain(&engine).await;
        let children: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM transfer WHERE parent_uid=?")
            .bind(&transfer.transfer)
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
        assert_eq!(children, 2);
    });
}

#[test]
fn competing_executors_restart_and_pause_preserve_one_scheduled_effect() {
    run(async {
        let (engine, a, transfer) = fixture().await;
        let other = Engine::new(engine.store.clone()).await.unwrap();
        other.set_signer(a.signer.clone()).await.unwrap();
        let rule = weekly(&engine, &format!("@trade: repeat(@{})", a.uid)).await;
        let (a_result, b_result) = tokio::join!(fire(&engine, &rule), fire(&other, &rule));
        assert!(a_result.is_ok() || b_result.is_ok());
        let _ = tokio::join!(engine.run_database_effects(), other.run_database_effects());
        drain(&engine).await;
        let count = || {
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM transfer WHERE parent_uid=?")
                .bind(&transfer.transfer)
                .fetch_one(&engine.store.pool)
        };
        assert_eq!(count().await.unwrap(), 1);
        drop(other);
        let restarted = Engine::new(engine.store.clone()).await.unwrap();
        restarted.set_signer(a.signer.clone()).await.unwrap();
        fire(&restarted, &rule).await.unwrap();
        drain(&restarted).await;
        assert_eq!(count().await.unwrap(), 1);
        let revision = store::recurrence::get(&engine.store.pool, &rule)
            .await
            .unwrap()
            .unwrap()
            .revision;
        engine
            .act(
                Action::SetRecurrencePaused {
                    recurrence: rule.clone(),
                    expected_revision: revision,
                    request_id: "pause-repeated-transfer".into(),
                    paused: true,
                },
                None,
            )
            .await
            .unwrap();
        let _ = restarted
            .act(
                Action::ApplyRecurrenceOccurrence {
                    recurrence: rule,
                    due_at: "2030-01-08T00:00:00Z".into(),
                    amount: None,
                    note: None,
                },
                None,
            )
            .await;
        let _ = restarted.run_database_effects().await;
        assert_eq!(count().await.unwrap(), 1);
    });
}

#[test]
fn recurring_activation_waits_for_current_counterparty_agreement_and_never_reports_receipt() {
    run(async {
        let (engine, a, b, transfer) = fixture_parties().await;
        let rule = weekly(
            &engine,
            &format!("@trade: repeat(@{}, \"activate\")", a.uid),
        )
        .await;
        fire(&engine, &rule).await.unwrap();
        drain(&engine).await;
        let child: String =
            sqlx::query_scalar("SELECT record_uid FROM transfer WHERE parent_uid=?")
                .bind(&transfer.transfer)
                .fetch_one(&engine.store.pool)
                .await
                .unwrap();
        let count = || {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM transfer_occurrence WHERE transfer_uid=?",
            )
            .bind(&child)
            .fetch_one(&engine.store.pool)
        };
        assert_eq!(count().await.unwrap(), 0);
        let invitation = store::transfers::invitations_for_transfer(&engine.store.pool, &child)
            .await
            .unwrap()
            .into_iter()
            .find(|invitation| invitation.addressed_person_uid == b.uid)
            .unwrap();
        engine.set_signer(b.signer.clone()).await.unwrap();
        engine
            .act(
                Action::AcceptTransferInvitation {
                    transfer: Some(child.clone()),
                    invitation: invitation.uid,
                    person: Some(b.uid.clone()),
                    expected_revision: 2,
                    request_id: "accept-recurring-child".into(),
                },
                None,
            )
            .await
            .unwrap();
        let revision = store::transfers::get(&engine.store.pool, &child)
            .await
            .unwrap()
            .unwrap()
            .revision as u64;
        let child_fixture = TransferFixture {
            transfer: child.clone(),
            revision,
        };
        assign(&engine, &child_fixture, &b, 2).await;
        engine.set_signer(a.signer.clone()).await.unwrap();
        drain(&engine).await;
        assert_eq!(count().await.unwrap(), 0);
        assign(&engine, &child_fixture, &a, 2).await;
        drain(&engine).await;
        assert_eq!(count().await.unwrap(), 1);
        let claims:i64=sqlx::query_scalar("SELECT COUNT(*) FROM transfer_occurrence_claim_event WHERE occurrence_uid IN (SELECT uid FROM transfer_occurrence WHERE transfer_uid=?)").bind(&child).fetch_one(&engine.store.pool).await.unwrap();
        assert_eq!(claims, 0);
    });
}

#[test]
fn revoking_required_permissions_before_a_recurrent_effect_prevents_the_proposal() {
    run(async {
        let (engine, a, transfer) = fixture().await;
        let role = store::auth::ensure_role(&engine.store.pool, "recurrent-controller")
            .await
            .unwrap();
        let mut creation = None;
        for (resource, operations) in [
            ("record", vec!["read", "create", "update"]),
            ("transfer", vec!["read", "create", "update"]),
            ("frequency", vec!["read", "create", "update"]),
        ] {
            for operation in operations {
                let permission =
                    store::auth::ensure_permission(&engine.store.pool, resource, operation)
                        .await
                        .unwrap();
                store::auth::grant(&engine.store.pool, role, permission)
                    .await
                    .unwrap();
                if resource == "transfer" && operation == "create" {
                    creation = Some(permission);
                }
            }
        }
        store::auth::create_credential(
            &engine.store.pool,
            &a.uid,
            "recurrent-controller",
            "fixture",
            role,
        )
        .await
        .unwrap();
        create_weekly(&engine).await;
        let rule = engine
            .act(
                Action::SaveKarmaRule {
                    rule: None,
                    expected_revision: None,
                    identity: None,
                    fields: [
                        "freq(@weekly)".into(),
                        "> 0".into(),
                        format!("@trade: repeat(@{})", a.uid),
                    ]
                    .map(|source| RuleFieldInput::Text { source }),
                    request_id: "save-guarded-repeat".into(),
                },
                Some(a.uid.clone()),
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
                Some(a.uid.clone()),
            )
            .await
            .unwrap();
        store::auth::revoke(&engine.store.pool, role, creation.unwrap())
            .await
            .unwrap();
        let outcomes = engine.run_database_effects().await.unwrap();
        assert!(outcomes.outcomes.iter().any(|outcome| !outcome.ok));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM transfer WHERE parent_uid=?")
            .bind(&transfer.transfer)
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    });
}
