use engine::{Engine, actions::Action};
use nucleus::command::Command;
use nucleus::karma::{Cadence, Consequence};

mod support;

async fn command(engine: &Engine, script: &str) -> String {
    engine
        .act(
            Action::SaveKarmaCommand {
                target: None,
                expected_revision: None,
                slug: "reader".into(),
                head: "Reader".into(),
                configuration: Command::Shell {
                    script: script.into(),
                },
                host: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap()
}

async fn rule(engine: &Engine, condition: &str) -> (String, String, String) {
    let trigger = support::plain(engine, "trigger", 0.0).await;
    let target = support::plain(engine, "target", 0.0).await;
    let rule = support::declare_rule(
        engine,
        &target,
        Cadence::every_days(1),
        &chrono::Utc::now().to_rfc3339(),
        Some(condition),
        Some("!=0"),
        Some("value"),
        vec![Consequence::SetQuantity { value: None }],
    )
    .await;
    (trigger, target, rule)
}

async fn value(engine: &Engine, target: &str) -> String {
    store::facts::level(&engine.store.pool, target)
        .await
        .unwrap()
        .to_string()
}

async fn diagnostics(engine: &Engine) -> String {
    let rules: Vec<(String, Option<String>)> =
        store::sqlx::query_as("SELECT status, reason FROM karma_rule_application")
            .fetch_all(&engine.store.pool)
            .await
            .unwrap();
    let commands: Vec<(String, Option<String>)> =
        store::sqlx::query_as("SELECT status, error FROM karma_command_invocation")
            .fetch_all(&engine.store.pool)
            .await
            .unwrap();
    format!("Rules: {rules:?}; commands: {commands:?}")
}

#[tokio::test]
async fn fresh_queries_resume_the_original_event_and_reuse_repeated_references() {
    let engine = support::karma::engine().await;
    let reader = command(&engine, "printf ' -2.5 '").await;
    let (trigger, target, _) = rule(
        &engine,
        "@trigger * (query_command(@reader) + query_command(@reader))",
    )
    .await;
    engine.append_user(&trigger, 1.0).await.unwrap();
    assert_eq!(value(&engine, &target).await, "0");
    engine.run_due_effects().await.unwrap();
    assert_eq!(
        value(&engine, &target).await,
        "-5.0",
        "{}",
        diagnostics(&engine).await
    );
    let count: i64 = store::sqlx::query_scalar("SELECT count(*) FROM karma_command_invocation")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    engine.run_due_effects().await.unwrap();
    assert_eq!(value(&engine, &target).await, "-5.0");
    let result = engine
        .act(
            Action::PreviewKarmaReading {
                source: "signal(@reader)".into(),
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(result.data.unwrap()["value"], "-2.5");
    assert_eq!(value(&engine, &reader).await, "-2.5");
}

#[tokio::test]
async fn logical_guards_do_not_execute_commands_and_editor_inspection_does_not_run_them() {
    let engine = support::karma::engine().await;
    command(&engine, "exit 7").await;
    let (trigger, target, _) = rule(&engine, "(@trigger < 0) && query_command(@reader)").await;
    engine.append_user(&trigger, 1.0).await.unwrap();
    assert!(engine.run_due_effects().await.unwrap().is_empty());
    assert_eq!(value(&engine, &target).await, "0");
    assert!(
        engine
            .act(
                Action::PreviewKarmaReading {
                    source: "query_command(@reader)".into()
                },
                None
            )
            .await
            .is_err()
    );
    assert!(
        engine
            .act(
                Action::PreviewKarmaReading {
                    source: "signal(@reader)".into()
                },
                None
            )
            .await
            .is_err()
    );
    let count: i64 = store::sqlx::query_scalar("SELECT count(*) FROM karma_command_invocation")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn malformed_numeric_output_is_not_zero_and_does_not_replace_a_good_sample() {
    let engine = support::karma::engine().await;
    let reader = command(&engine, "printf 12").await;
    engine
        .act(
            Action::RunKarmaCommand {
                command: reader.clone(),
                request_id: "sample-good".into(),
                numeric: true,
            },
            None,
        )
        .await
        .unwrap();
    engine.run_due_effects().await.unwrap();
    engine
        .act(
            Action::SaveKarmaCommand {
                target: Some(reader.clone()),
                expected_revision: Some(1),
                slug: "reader".into(),
                head: "Reader".into(),
                configuration: Command::Shell {
                    script: "printf '12 apples'".into(),
                },
                host: None,
            },
            None,
        )
        .await
        .unwrap();
    let (trigger, target, rule_uid) = rule(&engine, "@trigger * query_command(@reader)").await;
    engine.append_user(&trigger, 1.0).await.unwrap();
    let outcomes = engine.run_due_effects().await.unwrap();
    assert!(!outcomes.is_empty(), "{}", diagnostics(&engine).await);
    assert!(!outcomes[0].ok);
    assert_eq!(value(&engine, &target).await, "0");
    assert_eq!(value(&engine, &reader).await, "12");
    let status: String = store::sqlx::query_scalar(
        "SELECT status FROM karma_rule_application WHERE rule_uid = ? ORDER BY rowid DESC LIMIT 1",
    )
    .bind(rule_uid)
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    assert_eq!(status, "failed");
    let rows = engine
        .act(Action::InspectKarmaCommands, None)
        .await
        .unwrap()
        .data
        .unwrap();
    assert_eq!(rows["commands"][0]["sample"], "12");
    assert_eq!(rows["commands"][0]["history"][0]["status"], "failed");
}

#[tokio::test]
async fn pause_revision_delete_and_wrong_targets_refuse_queued_queries() {
    for change in ["pause", "revise", "delete"] {
        let engine = support::karma::engine().await;
        let reader = command(&engine, "printf 9").await;
        let (trigger, target, rule_uid) = rule(&engine, "@trigger * query_command(@reader)").await;
        engine.append_user(&trigger, 1.0).await.unwrap();
        match change {
            "pause" => {
                engine
                    .act(
                        Action::SetRecurrencePaused {
                            recurrence: rule_uid,
                            expected_revision: 1,
                            request_id: "pause".into(),
                            paused: true,
                        },
                        None,
                    )
                    .await
                    .unwrap();
            }
            "revise" => {
                engine
                    .act(
                        Action::SaveKarmaCommand {
                            target: Some(reader),
                            expected_revision: Some(1),
                            slug: "reader".into(),
                            head: "Reader".into(),
                            configuration: Command::Shell {
                                script: "printf 10".into(),
                            },
                            host: None,
                        },
                        None,
                    )
                    .await
                    .unwrap();
            }
            _ => {
                engine
                    .act(Action::DeleteRecord { target: reader }, None)
                    .await
                    .unwrap();
            }
        }
        engine.run_due_effects().await.unwrap();
        assert_eq!(value(&engine, &target).await, "0");
    }
    let engine = support::karma::engine().await;
    let plain = support::plain(&engine, "ordinary", 0.0).await;
    assert!(
        engine
            .act(
                Action::RunKarmaCommand {
                    command: plain,
                    request_id: "wrong-target".into(),
                    numeric: true
                },
                None
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn waiting_query_does_not_hold_the_rule_lock_and_rechecks_pause_after_execution() {
    let engine = std::sync::Arc::new(support::karma::engine().await);
    command(&engine, "sleep 0.2; printf 9").await;
    let (trigger, target, rule_uid) = rule(&engine, "@trigger * query_command(@reader)").await;
    engine.append_user(&trigger, 1.0).await.unwrap();
    let worker = {
        let engine = engine.clone();
        tokio::spawn(async move { engine.run_due_effects().await.unwrap() })
    };
    for _ in 0..100 {
        let running: bool = store::sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM karma_command_invocation WHERE status = 'running')",
        )
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
        if running {
            break;
        }
        tokio::task::yield_now().await;
    }
    tokio::time::timeout(
        std::time::Duration::from_millis(100),
        engine.act(
            Action::SetRecurrencePaused {
                recurrence: rule_uid,
                expected_revision: 1,
                request_id: "pause".into(),
                paused: true,
            },
            None,
        ),
    )
    .await
    .unwrap()
    .unwrap();
    worker.await.unwrap();
    assert_eq!(value(&engine, &target).await, "0");
}

#[tokio::test]
async fn manual_retry_reuses_its_result_and_equal_samples_remain_distinct() {
    let engine = support::karma::engine().await;
    let reader = command(&engine, "printf 0").await;
    for request in ["one", "one", "two"] {
        engine
            .act(
                Action::RunKarmaCommand {
                    command: reader.clone(),
                    request_id: request.into(),
                    numeric: true,
                },
                None,
            )
            .await
            .unwrap();
        engine.run_due_effects().await.unwrap();
    }
    let count: i64 = store::sqlx::query_scalar("SELECT count(*) FROM karma_command_invocation")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    assert_eq!(count, 2);
    let samples: i64 = store::sqlx::query_scalar("SELECT count(*) FROM fact WHERE record_uid = ? AND json_extract(payload, '$.command_invocation') IS NOT NULL").bind(&reader).fetch_one(&engine.store.pool).await.unwrap();
    assert_eq!(samples, 2, "{}", diagnostics(&engine).await);
    assert!(
        engine
            .act(
                Action::RunKarmaCommand {
                    command: reader,
                    request_id: "one".into(),
                    numeric: false
                },
                None
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn command_authority_is_separate_from_record_visibility() {
    let engine = support::karma::engine().await;
    let reader = command(&engine, "printf 1").await;
    let person = support::person(&engine, "reader-person").await;
    let role = store::auth::ensure_role(&engine.store.pool, "reader-role")
        .await
        .unwrap();
    let permission = store::auth::ensure_permission(&engine.store.pool, "record", "read")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, permission)
        .await
        .unwrap();
    store::auth::create_credential(
        &engine.store.pool,
        &person.uid,
        "reader-person",
        "hash",
        role,
    )
    .await
    .unwrap();
    store::visibility::grant(&engine.store.pool, "actor", Some(&person.uid), &reader)
        .await
        .unwrap();
    assert!(
        engine
            .act(
                Action::RunKarmaCommand {
                    command: reader.clone(),
                    request_id: "denied".into(),
                    numeric: true
                },
                Some(person.uid.clone())
            )
            .await
            .unwrap_err()
            .to_string()
            .contains("organ:update")
    );
    assert!(engine.act(Action::SetExtension { target: reader.clone(), namespace: "lince.command".into(), fds: serde_json::json!({"configuration":{"kind":"shell","script":"printf changed"}}) }, None).await.is_err());
    assert!(
        store::misc::due_effects(&engine.store.pool)
            .await
            .unwrap()
            .is_empty()
    );
    let permission = store::auth::ensure_permission(&engine.store.pool, "organ", "update")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, permission)
        .await
        .unwrap();
    engine
        .act(
            Action::RunKarmaCommand {
                command: reader.clone(),
                request_id: "authorized-queued".into(),
                numeric: true,
            },
            Some(person.uid),
        )
        .await
        .unwrap();
    store::auth::revoke(&engine.store.pool, role, permission)
        .await
        .unwrap();
    let outcomes = engine.run_due_effects().await.unwrap();
    assert!(outcomes.iter().all(|outcome| !outcome.ok));
    let sample: bool = store::sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM karma_signal_sample WHERE signal_uid = ?)",
    )
    .bind(reader)
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    assert!(!sample);
}

#[tokio::test]
async fn controlled_query_responses_never_launch_the_configured_process() {
    let engine = support::karma::engine().await;
    let reader = command(&engine, "exit 99").await;
    engine
        .supply_command_responses(vec![nucleus::command::CommandResponse {
            command: reader,
            ok: true,
            stdout: "3.75".into(),
            stderr: "controlled".into(),
        }])
        .unwrap();
    let (trigger, target, _) = rule(&engine, "@trigger * query_command(@reader)").await;
    let execution =
        nucleus::execution::Execution::new([7; 32], chrono::Utc::now().timestamp_millis()).unwrap();
    execution
        .scope(engine.append_user(&trigger, 1.0))
        .await
        .unwrap();
    assert_eq!(
        value(&engine, &target).await,
        "3.75",
        "{}",
        diagnostics(&engine).await
    );
    assert!(
        store::misc::due_effects(&engine.store.pool)
            .await
            .unwrap()
            .is_empty()
    );
    execution
        .scope(engine.append_user(&trigger, 1.0))
        .await
        .unwrap();
    assert_eq!(value(&engine, &target).await, "3.75");
    let error: String = store::sqlx::query_scalar("SELECT reason FROM karma_rule_application WHERE status = 'failed' ORDER BY rowid DESC LIMIT 1").fetch_one(&engine.store.pool).await.unwrap();
    assert!(error.contains("controlled response"));
}

#[tokio::test]
async fn interrupted_started_queries_are_indeterminate_and_do_not_execute_again() {
    let engine = std::sync::Arc::new(support::karma::engine().await);
    command(&engine, "printf 5").await;
    let (trigger, target, _) = rule(&engine, "@trigger * query_command(@reader)").await;
    engine.append_user(&trigger, 1.0).await.unwrap();
    store::sqlx::query("UPDATE karma_command_invocation SET status = 'running'")
        .execute(&engine.store.pool)
        .await
        .unwrap();
    store::sqlx::query("UPDATE effect_queue SET status = 'running'")
        .execute(&engine.store.pool)
        .await
        .unwrap();
    let worker = engine.clone().start_effect_worker();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let indeterminate: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM karma_command_invocation WHERE status = 'indeterminate')").fetch_one(&engine.store.pool).await.unwrap();
            if indeterminate { break; }
            tokio::task::yield_now().await;
        }
    }).await.unwrap();
    worker.abort();
    assert_eq!(value(&engine, &target).await, "0");
    assert!(engine.run_due_effects().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_slow_command_worker_does_not_block_later_ordinary_effects() {
    let engine = std::sync::Arc::new(support::karma::engine().await);
    let reader = command(&engine, "sleep 5; printf 4").await;
    let target = support::plain(&engine, "ordinary", 0.0).await;
    for index in 0..65 {
        engine
            .act(
                Action::RunKarmaCommand {
                    command: reader.clone(),
                    request_id: format!("slow-{index}"),
                    numeric: true,
                },
                None,
            )
            .await
            .unwrap();
    }
    store::misc::queue_effect(
        &engine.store.pool,
        "action",
        &serde_json::json!({"action":{"action":"set-quantity-exact","target":target,"amount":"7"}}),
        Some(&target),
    )
    .await
    .unwrap();
    let worker = engine.clone().start_effect_worker();
    let completed = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if value(&engine, &target).await == "7" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await;
    worker.abort();
    let _ = worker.await;
    completed.unwrap();
}

#[tokio::test]
async fn consequences_from_the_same_occurrence_keep_their_order_across_workers() {
    let engine = std::sync::Arc::new(support::karma::engine().await);
    let reader = command(&engine, "sleep 0.5; printf done").await;
    let trigger = support::plain(&engine, "trigger", 0.0).await;
    let target = support::plain(&engine, "target", 0.0).await;
    support::declare_rule(
        &engine,
        &target,
        Cadence::every_days(1),
        &chrono::Utc::now().to_rfc3339(),
        Some("@trigger"),
        Some("!=0"),
        Some("value"),
        vec![
            Consequence::InvokeCommand { command: reader },
            Consequence::Notify {
                message: Some("After the command".into()),
            },
        ],
    )
    .await;
    engine.append_user(&trigger, 1.0).await.unwrap();
    let worker = engine.clone().start_effect_worker();
    let result = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let running: bool = store::sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM karma_command_invocation WHERE status = 'running')",
            )
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
            if running {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        let notified: i64 = store::sqlx::query_scalar(
            "SELECT count(*) FROM effect_queue WHERE kind = 'notify' AND status = 'done'",
        )
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
        assert_eq!(notified, 0);
        loop {
            let notified: i64 = store::sqlx::query_scalar(
                "SELECT count(*) FROM effect_queue WHERE kind = 'notify' AND status = 'done'",
            )
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
            if notified == 1 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        let completed: bool = store::sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM karma_command_invocation WHERE status = 'completed')",
        )
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
        assert!(completed);
    })
    .await;
    worker.abort();
    let _ = worker.await;
    result.unwrap();
}
