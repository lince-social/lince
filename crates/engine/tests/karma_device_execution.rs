use engine::{
    Engine,
    actions::Action,
    roster::{CellEntry, ROOT_KEY_ID},
    trust::Signer,
};
use nucleus::karma::{Cadence, CadenceStep, Consequence};

mod support;

struct Pair {
    laptop: Engine,
    phone: Engine,
    root: Signer,
    laptop_uid: String,
    phone_uid: String,
    trigger: String,
    target: String,
    _directory: tempfile::TempDir,
}

fn entry(uid: &str, key: &Signer, label: &str) -> CellEntry {
    CellEntry {
        cell_uid: uid.into(),
        node_id: format!("node-{label}"),
        label: label.into(),
        operational_key: key.public_key_b64(),
        sealing_key: None,
        front_door: false,
        capabilities: engine::roster::full_capabilities(),
    }
}

async fn pair() -> Pair {
    let laptop = support::engine().await;
    let organ = store::organs::local(&laptop.store.pool)
        .await
        .unwrap()
        .unwrap();
    let laptop_cell = store::cells::ensure_local(&laptop.store.pool, &organ.uid, "Laptop")
        .await
        .unwrap();
    let root = Signer::generate(&organ.uid, ROOT_KEY_ID);
    let laptop_key = Signer::generate(&organ.uid, &engine::roster::cell_key_id(&laptop_cell.uid));
    laptop.set_signer(laptop_key.clone()).await.unwrap();
    laptop.publish_root_key(&root).await.unwrap();
    laptop
        .publish_roster(&root, vec![entry(&laptop_cell.uid, &laptop_key, "laptop")])
        .await
        .unwrap();
    let trigger = support::plain(&laptop, "trigger", 0.0).await;
    let target = support::plain(&laptop, "counter", 0.0).await;
    support::declare_rule(
        &laptop,
        &target,
        Cadence::every_days(1),
        &chrono::Utc::now().to_rfc3339(),
        Some("@trigger"),
        Some("!=0"),
        Some("value"),
        vec![Consequence::AddQuantity {
            delta: Some(nucleus::DecimalValue::parse_inferred("1").unwrap()),
        }],
    )
    .await;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("phone.sqlite");
    laptop.store.snapshot_into(&path).await.unwrap();
    let phone = Engine::open(&format!("sqlite://{}", path.display()))
        .await
        .unwrap();
    store::sqlx::query("UPDATE record SET slug = 'laptop-cell' WHERE uid = ?")
        .bind(&laptop_cell.uid)
        .execute(&phone.store.pool)
        .await
        .unwrap();
    let phone_cell = store::cells::ensure_local(&phone.store.pool, &organ.uid, "Phone")
        .await
        .unwrap();
    let phone_key = Signer::generate(&organ.uid, &engine::roster::cell_key_id(&phone_cell.uid));
    phone.set_signer(phone_key.clone()).await.unwrap();
    let roster = laptop
        .enrol_cell(&root, entry(&phone_cell.uid, &phone_key, "phone"))
        .await
        .unwrap();
    phone.adopt_roster(&roster).await.unwrap();
    Pair {
        laptop,
        phone,
        root,
        laptop_uid: laptop_cell.uid,
        phone_uid: phone_cell.uid,
        trigger,
        target,
        _directory: directory,
    }
}

async fn level(engine: &Engine, uid: &str) -> String {
    store::facts::level(&engine.store.pool, uid)
        .await
        .unwrap()
        .to_string()
}

async fn copy_ops(from: &Engine, to: &Engine) {
    let organ = store::organs::local(&from.store.pool)
        .await
        .unwrap()
        .unwrap();
    let (ops, _) = from.ops_after(0, 10_000).await.unwrap();
    to.import_op_batch(&engine::sync::OpBatch {
        from_organ: organ.uid,
        ops,
    })
    .await
    .unwrap();
    let refused: Vec<(String, String)> = store::sqlx::query_as(
        "SELECT reason, payload FROM sync_quarantine ORDER BY rowid DESC LIMIT 10",
    )
    .fetch_all(&to.store.pool)
    .await
    .unwrap();
    assert!(refused.is_empty(), "Sync refused: {refused:?}");
}

#[tokio::test]
async fn default_phone_syncs_and_edits_but_never_executes_reactive_rules() {
    let pair = pair().await;
    assert!(
        pair.laptop
            .karma_device_execution()
            .await
            .unwrap()
            .executing
    );
    assert!(!pair.phone.karma_device_execution().await.unwrap().permitted);
    for amount in [-1.0, -2.0, 0.0, 1.0].into_iter().cycle().take(20) {
        pair.laptop
            .act(
                Action::SetQuantity {
                    target: pair.trigger.clone(),
                    value: amount,
                },
                None,
            )
            .await
            .unwrap();
        copy_ops(&pair.laptop, &pair.phone).await;
        assert_eq!(
            level(&pair.laptop, &pair.trigger).await,
            level(&pair.phone, &pair.trigger).await
        );
        let phone_applied: i64 = store::sqlx::query_scalar(
            "SELECT count(*) FROM karma_rule_application WHERE status = 'applied'",
        )
        .fetch_one(&pair.phone.store.pool)
        .await
        .unwrap();
        assert_eq!(phone_applied, 0);
    }
    pair.phone
        .act(
            Action::SetQuantity {
                target: pair.trigger.clone(),
                value: 0.0,
            },
            None,
        )
        .await
        .unwrap();
    copy_ops(&pair.phone, &pair.laptop).await;
    assert_eq!(level(&pair.laptop, &pair.trigger).await, "0");
    assert!(!pair.phone.karma_device_execution().await.unwrap().permitted);
    let command = pair
        .phone
        .act(
            Action::SaveKarmaCommand {
                target: None,
                expected_revision: None,
                slug: "phone-reader".into(),
                head: "Phone reader".into(),
                configuration: nucleus::command::Command::Shell {
                    script: "printf 7".into(),
                },
                host: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    assert!(
        pair.phone
            .act(
                Action::RunKarmaCommand {
                    command: command.clone(),
                    request_id: "disabled-manual-run".into(),
                    numeric: true,
                },
                None,
            )
            .await
            .is_err()
    );
    copy_ops(&pair.phone, &pair.laptop).await;
    let definitions = pair
        .laptop
        .act(Action::InspectKarmaCommands, None)
        .await
        .unwrap()
        .data
        .unwrap();
    let definition = definitions["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|definition| definition["uid"] == command)
        .unwrap();
    assert_eq!(definition["configuration"]["script"], "printf 7");
    assert!(definition["history"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn only_one_executor_is_selected_unless_additional_execution_is_explicit() {
    let pair = pair().await;
    let roster = pair
        .laptop
        .roster_of(&pair.root.actor_uid)
        .await
        .unwrap()
        .unwrap();
    assert!(
        pair.laptop
            .choose_karma_executor(
                &pair.root,
                &pair.phone_uid,
                true,
                false,
                roster.roster.version - 1
            )
            .await
            .is_err()
    );
    let changed = pair
        .laptop
        .choose_karma_executor(
            &pair.root,
            &pair.phone_uid,
            true,
            false,
            roster.roster.version,
        )
        .await
        .unwrap();
    pair.phone.adopt_roster(&changed).await.unwrap();
    assert!(
        !pair
            .laptop
            .karma_device_execution()
            .await
            .unwrap()
            .permitted
    );
    assert!(pair.phone.karma_device_execution().await.unwrap().permitted);
    let changed = pair
        .laptop
        .choose_karma_executor(
            &pair.root,
            &pair.laptop_uid,
            true,
            true,
            changed.roster.version,
        )
        .await
        .unwrap();
    pair.phone.adopt_roster(&changed).await.unwrap();
    assert_eq!(
        pair.laptop
            .karma_device_execution()
            .await
            .unwrap()
            .executors
            .len(),
        2
    );
    let changed = pair
        .laptop
        .enrol_cell(
            &pair.root,
            entry(
                &pair.phone_uid,
                &Signer::generate(&pair.root.actor_uid, "new-key"),
                "renamed-phone",
            ),
        )
        .await
        .unwrap();
    assert!(
        changed
            .roster
            .cells
            .iter()
            .find(|cell| cell.cell_uid == pair.phone_uid)
            .unwrap()
            .may(engine::roster::CAP_KARMA)
    );
}

#[tokio::test]
async fn scheduled_and_manual_occurrences_require_device_permission() {
    let pair = pair().await;
    let now = chrono::Utc::now();
    let due = now + chrono::TimeDelta::seconds(1);
    let task = support::plain(&pair.laptop, "task", 0.0).await;
    let rule = support::declare_rule(
        &pair.laptop,
        &task,
        Cadence::every(CadenceStep {
            seconds: 60,
            ..Default::default()
        }),
        &due.to_rfc3339(),
        None,
        None,
        None,
        vec![Consequence::SetQuantity {
            value: Some(nucleus::DecimalValue::parse_inferred("-1").unwrap()),
        }],
    )
    .await;
    pair.laptop.advance_karma_time(now).await.unwrap();
    copy_ops(&pair.laptop, &pair.phone).await;
    assert!(
        pair.phone
            .act(
                Action::ApplyRecurrenceOccurrence {
                    recurrence: rule,
                    due_at: due.to_rfc3339(),
                    amount: None,
                    note: None
                },
                None
            )
            .await
            .is_err()
    );
    pair.phone.advance_karma_time(due).await.unwrap();
    assert_eq!(level(&pair.phone, &task).await, "0");
    pair.laptop.advance_karma_time(due).await.unwrap();
    assert_eq!(level(&pair.laptop, &task).await, "-1");
    copy_ops(&pair.laptop, &pair.phone).await;
    assert_eq!(level(&pair.phone, &task).await, "-1");
    pair.phone
        .act(
            Action::SetQuantity {
                target: task.clone(),
                value: 0.0,
            },
            None,
        )
        .await
        .unwrap();
    copy_ops(&pair.phone, &pair.laptop).await;
    assert_eq!(level(&pair.laptop, &task).await, "0");
    pair.laptop
        .advance_karma_time(due + chrono::TimeDelta::seconds(60))
        .await
        .unwrap();
    copy_ops(&pair.laptop, &pair.phone).await;
    assert_eq!(level(&pair.phone, &task).await, "-1");
}

#[tokio::test]
async fn local_stop_restart_removal_and_expiry_preserve_permission_boundaries() {
    let pair = pair().await;
    pair.laptop
        .act(
            Action::SetCellConfig {
                namespace: "lince.karma-runtime".into(),
                fds: serde_json::json!({"running":false}),
            },
            None,
        )
        .await
        .unwrap();
    let status = pair.laptop.karma_device_execution().await.unwrap();
    assert!(status.permitted);
    assert!(!status.local_running);
    assert!(!status.executing);
    pair.laptop.append_user(&pair.trigger, 1.0).await.unwrap();
    assert_eq!(level(&pair.laptop, &pair.target).await, "0");
    let path = pair._directory.path().join("restart.sqlite");
    pair.laptop.store.snapshot_into(&path).await.unwrap();
    let restarted = Engine::open(&format!("sqlite://{}", path.display()))
        .await
        .unwrap();
    assert!(
        !restarted
            .karma_device_execution()
            .await
            .unwrap()
            .local_running
    );
    pair.laptop
        .act(
            Action::SetCellConfig {
                namespace: "lince.karma-runtime".into(),
                fds: serde_json::json!({"running":true}),
            },
            None,
        )
        .await
        .unwrap();
    let future = nucleus::execution::Execution::new(
        [11; 32],
        (chrono::Utc::now() + chrono::TimeDelta::days(31)).timestamp_millis(),
    )
    .unwrap();
    assert!(
        !future
            .scope(pair.laptop.karma_device_execution())
            .await
            .unwrap()
            .permitted
    );
    assert!(
        future
            .scope(pair.laptop.append_user(&pair.trigger, 1.0))
            .await
            .is_err()
    );
    assert_eq!(level(&pair.laptop, &pair.target).await, "0");
    pair.laptop
        .revoke_cell(&pair.root, &pair.laptop_uid)
        .await
        .unwrap();
    assert!(
        !pair
            .laptop
            .karma_device_execution()
            .await
            .unwrap()
            .permitted
    );
    assert!(!pair.phone.karma_device_execution().await.unwrap().permitted);
}

#[tokio::test]
async fn queued_commands_stop_after_a_roster_switch_or_target_executor_change() {
    for switch_roster in [false, true] {
        let pair = pair().await;
        support::declare_rule(
            &pair.laptop,
            &pair.target,
            Cadence::every_days(1),
            &chrono::Utc::now().to_rfc3339(),
            Some("@trigger"),
            Some("!=0"),
            Some("value"),
            vec![Consequence::RunCommand {
                command: "printf should-not-run".into(),
            }],
        )
        .await;
        pair.laptop.append_user(&pair.trigger, 1.0).await.unwrap();
        if switch_roster {
            let version = pair
                .laptop
                .roster_of(&pair.root.actor_uid)
                .await
                .unwrap()
                .unwrap()
                .roster
                .version;
            pair.laptop
                .choose_karma_executor(&pair.root, &pair.phone_uid, true, false, version)
                .await
                .unwrap();
        } else {
            pair.laptop
                .act(
                    Action::DesignateKarmaExecutor {
                        target: pair.target.clone(),
                        cell_uid: Some(pair.phone_uid.clone()),
                    },
                    None,
                )
                .await
                .unwrap();
        }
        let outcomes = pair.laptop.run_due_effects().await.unwrap();
        assert!(!outcomes.is_empty());
        assert!(outcomes.iter().all(|outcome| !outcome.ok));
        let started: i64 = store::sqlx::query_scalar(
            "SELECT count(*) FROM karma_command_invocation WHERE status = 'completed'",
        )
        .fetch_one(&pair.laptop.store.pool)
        .await
        .unwrap();
        assert_eq!(started, 0);
    }
}

#[tokio::test]
async fn saved_numeric_samples_remain_readable_after_sync_to_a_nonexecuting_cell() {
    let pair = pair().await;
    let command = pair
        .laptop
        .act(
            Action::SaveKarmaCommand {
                target: None,
                expected_revision: None,
                slug: "reader".into(),
                head: "Reader".into(),
                configuration: nucleus::command::Command::Shell {
                    script: "printf 2.50".into(),
                },
                host: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    pair.laptop
        .act(
            Action::RunKarmaCommand {
                command,
                request_id: "sample".into(),
                numeric: true,
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        pair.laptop
            .run_due_effects()
            .await
            .unwrap()
            .iter()
            .all(|outcome| outcome.ok)
    );
    copy_ops(&pair.laptop, &pair.phone).await;
    let result = pair
        .phone
        .act(
            Action::PreviewKarmaReading {
                source: "signal(@reader)".into(),
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    assert_eq!(result["value"], "2.50");
    assert!(!pair.phone.karma_device_execution().await.unwrap().executing);
}

#[tokio::test]
async fn disconnected_quantity_assignments_converge_and_replay_never_refires_a_rule() {
    let pair = pair().await;
    pair.laptop
        .act(
            Action::SetQuantity {
                target: pair.trigger.clone(),
                value: -1.0,
            },
            None,
        )
        .await
        .unwrap();
    copy_ops(&pair.laptop, &pair.phone).await;
    pair.phone
        .act(
            Action::SetQuantity {
                target: pair.trigger.clone(),
                value: 0.0,
            },
            None,
        )
        .await
        .unwrap();
    pair.laptop
        .act(
            Action::SetQuantity {
                target: pair.trigger.clone(),
                value: -2.0,
            },
            None,
        )
        .await
        .unwrap();
    copy_ops(&pair.phone, &pair.laptop).await;
    copy_ops(&pair.laptop, &pair.phone).await;
    assert_eq!(level(&pair.laptop, &pair.trigger).await, "-2");
    assert_eq!(level(&pair.phone, &pair.trigger).await, "-2");
    let applications: i64 = store::sqlx::query_scalar(
        "SELECT count(*) FROM karma_rule_application WHERE status = 'applied'",
    )
    .fetch_one(&pair.laptop.store.pool)
    .await
    .unwrap();
    let final_value = level(&pair.laptop, &pair.target).await;
    copy_ops(&pair.phone, &pair.laptop).await;
    copy_ops(&pair.laptop, &pair.phone).await;
    assert_eq!(level(&pair.laptop, &pair.target).await, final_value);
    let replayed: i64 = store::sqlx::query_scalar(
        "SELECT count(*) FROM karma_rule_application WHERE status = 'applied'",
    )
    .fetch_one(&pair.laptop.store.pool)
    .await
    .unwrap();
    assert_eq!(replayed, applications);
    let phone: i64 = store::sqlx::query_scalar(
        "SELECT count(*) FROM karma_rule_application WHERE status = 'applied'",
    )
    .fetch_one(&pair.phone.store.pool)
    .await
    .unwrap();
    assert_eq!(phone, 0);
}
