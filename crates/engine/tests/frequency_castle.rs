mod support;

use engine::actions::Action;
use nucleus::karma::{Cadence, CadenceStep, FrequencyAst, Slug, TimestampMs};
use serde_json::Value;

fn definition(minutes: u32) -> FrequencyAst {
    nucleus::karma::simple_frequency::frequency_from_cadence(
        Slug::new("castle-frequency").unwrap(),
        format!("Every {minutes} minutes"),
        &Cadence::every(CadenceStep {
            minutes,
            ..Default::default()
        }),
        TimestampMs::parse_canonical("2026-09-20T12:00:00.000Z").unwrap(),
    )
    .unwrap()
}

async fn rows(engine: &engine::Engine) -> Vec<Value> {
    protein::execute(
        &engine.store,
        &serde_json::from_value(serde_json::json!({"source":"frequency"})).unwrap(),
    )
    .await
    .unwrap()
}

async fn create(engine: &engine::Engine) -> String {
    engine
        .act(
            Action::CreateKarmaFrequency {
                request_id: "castle-create".into(),
                frequency: definition(1),
                owner_person_uid: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap()
}

#[tokio::test]
async fn protein_tracks_saved_and_running_revisions_across_full_frequency_crud() {
    let engine = support::engine().await;
    engine
        .install_karma_runtime_config(
            engine::karma_runtime::KarmaDeadlineDirectorConfig::for_host(
                "frequency-castle-test".into(),
            )
            .unwrap(),
        )
        .unwrap();
    let uid = create(&engine).await;
    let first = rows(&engine).await.remove(0);
    assert_eq!(
        first["definition"],
        serde_json::to_value(definition(1)).unwrap()
    );
    assert_eq!(
        nucleus::karma::parse_frequency(first["source"].as_str().unwrap()).unwrap(),
        definition(1)
    );
    assert_eq!(first["status"], "proven");
    let activate = |row: &Value, request: &str| Action::ActivateKarmaFrequency {
        request_id: request.into(),
        frequency_uid: uid.clone(),
        expected_handle_revision: row["handle_revision"].as_u64().unwrap(),
        revision_hash: serde_json::from_value(row["head_revision_hash"].clone()).unwrap(),
        parameter_overrides: Default::default(),
    };
    engine
        .act(activate(&first, "castle-activate"), None)
        .await
        .unwrap();
    let active = rows(&engine).await.remove(0);
    assert_eq!(active["status"], "active");
    assert_eq!(active["active_revision_hash"], active["head_revision_hash"]);
    assert!(active["next_at_ms"].is_i64());
    let revise = |revision| Action::ReviseKarmaFrequency {
        request_id: format!("castle-revise-{revision}"),
        frequency_uid: uid.clone(),
        expected_handle_revision: revision,
        frequency: definition(2),
    };
    assert!(engine.act(revise(1), None).await.is_err());
    engine
        .act(revise(active["handle_revision"].as_u64().unwrap()), None)
        .await
        .unwrap();
    let saved = rows(&engine).await.remove(0);
    assert_eq!(saved["uid"], uid);
    assert_eq!(
        saved["definition"],
        serde_json::to_value(definition(2)).unwrap()
    );
    assert_eq!(
        saved["active_revision_hash"],
        active["active_revision_hash"]
    );
    assert_ne!(saved["active_revision_hash"], saved["head_revision_hash"]);
    engine
        .act(activate(&saved, "castle-apply"), None)
        .await
        .unwrap();
    let applied = rows(&engine).await.remove(0);
    assert_eq!(
        applied["active_revision_hash"],
        applied["head_revision_hash"]
    );
    engine
        .act(
            Action::PauseKarmaFrequency {
                request_id: "castle-pause".into(),
                frequency_uid: uid.clone(),
                expected_handle_revision: applied["handle_revision"].as_u64().unwrap(),
            },
            None,
        )
        .await
        .unwrap();
    let paused = rows(&engine).await.remove(0);
    assert_eq!(paused["status"], "paused");
    assert!(paused["next_at_ms"].is_null());
    assert_eq!(
        paused["last_run_revision_hash"],
        applied["active_revision_hash"]
    );
    engine
        .act(
            Action::ReviseKarmaFrequency {
                request_id: "saved-while-paused".into(),
                frequency_uid: uid.clone(),
                expected_handle_revision: paused["handle_revision"].as_u64().unwrap(),
                frequency: definition(3),
            },
            None,
        )
        .await
        .unwrap();
    let paused = rows(&engine).await.remove(0);
    engine
        .act(
            Action::ActivateKarmaFrequency {
                request_id: "castle-resume".into(),
                frequency_uid: uid.clone(),
                expected_handle_revision: paused["handle_revision"].as_u64().unwrap(),
                revision_hash: serde_json::from_value(paused["last_run_revision_hash"].clone())
                    .unwrap(),
                parameter_overrides: serde_json::from_value(paused["last_run_parameters"].clone())
                    .unwrap(),
            },
            None,
        )
        .await
        .unwrap();
    let resumed = rows(&engine).await.remove(0);
    assert_eq!(resumed["status"], "active");
    assert_eq!(
        resumed["active_revision_hash"],
        applied["active_revision_hash"]
    );
    assert_ne!(
        resumed["head_revision_hash"],
        resumed["active_revision_hash"]
    );
    engine
        .act(Action::DeleteFrequency { frequency: uid }, None)
        .await
        .unwrap();
    assert!(rows(&engine).await.is_empty());
}

#[tokio::test]
async fn referenced_frequency_delete_is_refused_until_rule_is_removed() {
    let engine = support::engine().await;
    let uid = create(&engine).await;
    support::plain(&engine, "balance", 0.0).await;
    let rule = engine
        .act(
            Action::SaveKarmaRule {
                rule: None,
                expected_revision: None,
                fields: ["freq(@castle-frequency)", ">0", "@balance += 1"].map(|source| {
                    nucleus::karma::rule_field::RuleFieldInput::Text {
                        source: source.into(),
                    }
                }),
                request_id: "castle-rule".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let error = engine
        .act(
            Action::DeleteFrequency {
                frequency: uid.clone(),
            },
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), Some("frequency_in_use"));
    assert_eq!(rows(&engine).await.len(), 1);
    engine
        .act(Action::DeleteRecurrence { recurrence: rule }, None)
        .await
        .unwrap();
    engine
        .act(Action::DeleteFrequency { frequency: uid }, None)
        .await
        .unwrap();
    assert!(rows(&engine).await.is_empty());
}

#[tokio::test]
async fn read_only_frequency_viewer_cannot_mutate_or_read_hidden_definitions() {
    let engine = support::engine().await;
    let uid = create(&engine).await;
    let person = support::person(&engine, "frequency-reader").await;
    let role = store::auth::ensure_role(&engine.store.pool, "frequency-reader")
        .await
        .unwrap();
    let permission = store::auth::ensure_permission(&engine.store.pool, "frequency", "read")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, permission)
        .await
        .unwrap();
    store::auth::create_credential(
        &engine.store.pool,
        &person.uid,
        "frequency-reader",
        "hash",
        role,
    )
    .await
    .unwrap();
    let query = serde_json::from_value(serde_json::json!({"source":"frequency"})).unwrap();
    assert!(
        protein::execute_for(&engine.store, &query, Some(&person.uid))
            .await
            .unwrap()
            .is_empty()
    );
    store::visibility::grant(&engine.store.pool, "actor", Some(&person.uid), &uid)
        .await
        .unwrap();
    let visible = protein::execute_for(&engine.store, &query, Some(&person.uid))
        .await
        .unwrap();
    assert_eq!(visible.len(), 1);
    assert_eq!(
        visible[0]["definition"],
        serde_json::to_value(definition(1)).unwrap()
    );
    for action in [
        Action::SaveKarmaFrequency {
            request_id: "unauthorized-save".into(),
            restart: true,
            frequency_uid: Some(uid.clone()),
            expected_handle_revision: Some(1),
            frequency: definition(2),
        },
        Action::ReviseKarmaFrequency {
            request_id: "unauthorized-edit".into(),
            frequency_uid: uid.clone(),
            expected_handle_revision: 1,
            frequency: definition(2),
        },
        Action::DeleteFrequency { frequency: uid },
        Action::CreateKarmaFrequency {
            request_id: "unauthorized-create".into(),
            frequency: definition(2),
            owner_person_uid: None,
        },
    ] {
        assert!(engine.act(action, Some(person.uid.clone())).await.is_err());
    }
    assert_eq!(
        rows(&engine).await[0]["definition"],
        serde_json::to_value(definition(1)).unwrap()
    );
}

fn daily(at: chrono::DateTime<chrono::Utc>) -> FrequencyAst {
    nucleus::karma::simple_frequency::frequency_from_cadence(
        Slug::new("daily").unwrap(),
        "Daily".into(),
        &Cadence::every(CadenceStep {
            days: 1,
            ..Default::default()
        }),
        TimestampMs::from_millis(at.timestamp_millis()).unwrap(),
    )
    .unwrap()
}

async fn daily_rule(engine: &engine::Engine) -> String {
    let target = support::plain(engine, "escovar-dentes", 0.0).await;
    engine
        .act(
            Action::SaveKarmaRule {
                rule: None,
                expected_revision: None,
                fields: [
                    "-1 * freq(@daily) + @escovar-dentes",
                    "!=0",
                    "@escovar-dentes",
                ]
                .map(|source| nucleus::karma::rule_field::RuleFieldInput::Text {
                    source: source.into(),
                }),
                request_id: "daily-rule".into(),
            },
            None,
        )
        .await
        .unwrap();
    target
}

#[tokio::test]
async fn daily_five_minutes_overdue_enacts_once_and_advances_the_real_date() {
    let engine = support::engine().await;
    engine
        .install_karma_runtime_config(
            engine::karma_runtime::KarmaDeadlineDirectorConfig::for_host("daily-test".into())
                .unwrap(),
        )
        .unwrap();
    let now =
        chrono::DateTime::from_timestamp_millis(chrono::Utc::now().timestamp_millis() + 60_000)
            .unwrap();
    let overdue = now - chrono::TimeDelta::minutes(5);
    let save = Action::SaveKarmaFrequency {
        request_id: "save-daily".into(),
        restart: true,
        frequency_uid: None,
        expected_handle_revision: None,
        frequency: daily(overdue),
    };
    let uid = engine
        .act_at(save.clone(), None, now)
        .await
        .unwrap()
        .created
        .unwrap();
    assert_eq!(
        rows(&engine).await[0]["next_at_ms"],
        overdue.timestamp_millis()
    );
    let target = daily_rule(&engine).await;
    engine.advance_karma_time(now).await.unwrap();
    assert_eq!(
        store::facts::level(&engine.store.pool, &target)
            .await
            .unwrap()
            .to_string(),
        "-1"
    );
    let next = overdue + chrono::TimeDelta::days(1);
    let row = rows(&engine).await.remove(0);
    assert_eq!(row["next_at_ms"], next.timestamp_millis());
    assert_eq!(row["active_revision_hash"], row["head_revision_hash"]);
    assert_eq!(
        engine
            .act_at(save, None, now)
            .await
            .unwrap()
            .created
            .unwrap(),
        uid
    );
    engine.advance_karma_time(now).await.unwrap();
    assert_eq!(
        store::facts::level(&engine.store.pool, &target)
            .await
            .unwrap()
            .to_string(),
        "-1"
    );
    let mut renamed = daily(overdue);
    renamed.purpose = "Renamed daily".into();
    engine
        .act_at(
            Action::SaveKarmaFrequency {
                request_id: "rename-daily".into(),
                restart: false,
                frequency_uid: Some(uid),
                expected_handle_revision: row["handle_revision"].as_u64(),
                frequency: renamed,
            },
            None,
            now,
        )
        .await
        .unwrap();
    assert_eq!(
        rows(&engine).await[0]["next_at_ms"],
        next.timestamp_millis()
    );
    engine.advance_karma_time(next).await.unwrap();
    assert_eq!(
        store::facts::level(&engine.store.pool, &target)
            .await
            .unwrap()
            .to_string(),
        "-2"
    );
}

#[tokio::test]
async fn editing_a_running_daily_to_the_past_applies_without_a_separate_activation() {
    let engine = support::engine().await;
    engine
        .install_karma_runtime_config(
            engine::karma_runtime::KarmaDeadlineDirectorConfig::for_host("daily-edit-test".into())
                .unwrap(),
        )
        .unwrap();
    let now =
        chrono::DateTime::from_timestamp_millis(chrono::Utc::now().timestamp_millis() + 60_000)
            .unwrap();
    let uid = engine
        .act_at(
            Action::SaveKarmaFrequency {
                request_id: "daily-future".into(),
                restart: true,
                frequency_uid: None,
                expected_handle_revision: None,
                frequency: daily(now + chrono::TimeDelta::days(1)),
            },
            None,
            now,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let target = daily_rule(&engine).await;
    engine.advance_karma_time(now).await.unwrap();
    assert_eq!(
        store::facts::level(&engine.store.pool, &target)
            .await
            .unwrap()
            .to_string(),
        "0"
    );
    let before = rows(&engine).await.remove(0);
    let overdue = now - chrono::TimeDelta::minutes(5);
    let save = Action::SaveKarmaFrequency {
        request_id: "daily-past".into(),
        restart: true,
        frequency_uid: Some(uid.clone()),
        expected_handle_revision: before["handle_revision"].as_u64(),
        frequency: daily(overdue),
    };
    engine.act_at(save.clone(), None, now).await.unwrap();
    engine.advance_karma_time(now).await.unwrap();
    assert_eq!(
        store::facts::level(&engine.store.pool, &target)
            .await
            .unwrap()
            .to_string(),
        "-1"
    );
    engine.act_at(save, None, now).await.unwrap();
    engine.advance_karma_time(now).await.unwrap();
    let after = rows(&engine).await.remove(0);
    assert_eq!(
        after["next_at_ms"],
        (overdue + chrono::TimeDelta::days(1)).timestamp_millis()
    );
    let stale = Action::SaveKarmaFrequency {
        request_id: "stale-edit".into(),
        restart: true,
        frequency_uid: Some(uid.clone()),
        expected_handle_revision: before["handle_revision"].as_u64(),
        frequency: daily(now),
    };
    assert!(engine.act_at(stale, None, now).await.is_err());
    let mut rejected = daily(now);
    rejected.timer.required_resolution = nucleus::karma::DurationBinding::Literal {
        value: nucleus::karma::DurationMs::new(1),
    };
    assert!(
        engine
            .act_at(
                Action::SaveKarmaFrequency {
                    request_id: "invalid-edit".into(),
                    restart: true,
                    frequency_uid: Some(uid),
                    expected_handle_revision: after["handle_revision"].as_u64(),
                    frequency: rejected
                },
                None,
                now
            )
            .await
            .is_err()
    );
    assert_eq!(rows(&engine).await[0], after);
    assert_eq!(
        store::facts::level(&engine.store.pool, &target)
            .await
            .unwrap()
            .to_string(),
        "-1"
    );
}

#[tokio::test]
async fn saving_a_past_date_wakes_the_live_director_without_replaying_an_applied_beat() {
    let engine = std::sync::Arc::new(support::engine().await);
    let runtime =
        engine::karma_runtime::KarmaDeadlineDirectorConfig::for_host("live-daily-test".into())
            .unwrap();
    engine
        .install_karma_runtime_config(runtime.clone())
        .unwrap();
    let overdue = chrono::DateTime::from_timestamp_millis(chrono::Utc::now().timestamp_millis())
        .unwrap()
        - chrono::TimeDelta::minutes(5);
    let uid = engine
        .act(
            Action::SaveKarmaFrequency {
                request_id: "live-daily".into(),
                frequency_uid: None,
                expected_handle_revision: None,
                frequency: daily(overdue),
                restart: true,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let target = daily_rule(&engine).await;
    let runner = engine.clone().start_karma_deadline_director(runtime);
    let wait_for = |expected: &'static str| {
        let engine = engine.clone();
        let target = target.clone();
        async move {
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                loop {
                    if store::facts::level(&engine.store.pool, &target)
                        .await
                        .unwrap()
                        .to_string()
                        == expected
                    {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await
        }
    };
    let first = wait_for("-1").await;
    if first.is_err() {
        runner.abort();
    }
    first.unwrap();
    let row = rows(&engine).await.remove(0);
    assert_eq!(
        row["next_at_ms"],
        (overdue + chrono::TimeDelta::days(1)).timestamp_millis()
    );
    engine
        .act(
            Action::SaveKarmaFrequency {
                request_id: "live-restart".into(),
                frequency_uid: Some(uid.clone()),
                expected_handle_revision: row["handle_revision"].as_u64(),
                frequency: daily(overdue),
                restart: true,
            },
            None,
        )
        .await
        .unwrap();
    let replayed = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            if rows(&engine).await[0]["next_at_ms"]
                == (overdue + chrono::TimeDelta::days(1)).timestamp_millis()
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await;
    if replayed.is_err() {
        runner.abort();
    }
    replayed.unwrap();
    assert_eq!(
        store::facts::level(&engine.store.pool, &target)
            .await
            .unwrap()
            .to_string(),
        "-1"
    );
    let row = rows(&engine).await.remove(0);
    let overdue = overdue + chrono::TimeDelta::seconds(1);
    engine
        .act(
            Action::SaveKarmaFrequency {
                request_id: "live-new-past-date".into(),
                frequency_uid: Some(uid),
                expected_handle_revision: row["handle_revision"].as_u64(),
                frequency: daily(overdue),
                restart: true,
            },
            None,
        )
        .await
        .unwrap();
    let second = wait_for("-2").await;
    runner.abort();
    let _ = runner.await;
    second.unwrap();
    assert_eq!(
        rows(&engine).await[0]["next_at_ms"],
        (overdue + chrono::TimeDelta::days(1)).timestamp_millis()
    );
}

#[tokio::test]
async fn a_rule_edit_does_not_pick_up_an_older_queued_emission() {
    use store::karma::schedules::{self, CursorCompletion, ScheduleClaim};
    let engine = support::engine().await;
    engine
        .install_karma_runtime_config(
            engine::karma_runtime::KarmaDeadlineDirectorConfig::for_host(
                "queued-daily-test".into(),
            )
            .unwrap(),
        )
        .unwrap();
    let now =
        chrono::DateTime::from_timestamp_millis(chrono::Utc::now().timestamp_millis() + 60_000)
            .unwrap();
    let overdue = now - chrono::TimeDelta::minutes(5);
    engine
        .act_at(
            Action::SaveKarmaFrequency {
                request_id: "queued-daily".into(),
                frequency_uid: None,
                expected_handle_revision: None,
                frequency: daily(overdue),
                restart: true,
            },
            None,
            now,
        )
        .await
        .unwrap();
    let target = daily_rule(&engine).await;
    let cursor = schedules::list_cursors(&engine.store.pool)
        .await
        .unwrap()
        .remove(0);
    let ScheduleClaim::Claimed(lease) = schedules::claim_due(
        &engine.store.pool,
        &cursor.activation_hash,
        cursor.cursor_revision,
        "queued-daily-test",
        now,
        nucleus::karma::DurationMs::new(10_000),
    )
    .await
    .unwrap() else {
        panic!("expected a due beat")
    };
    let CursorCompletion::Completed {
        occurrence: Some(_),
        ..
    } = schedules::complete_elapsed(&engine.store.pool, &lease, now, now)
        .await
        .unwrap()
    else {
        panic!("expected an emission")
    };
    let rule = store::recurrence::all(&engine.store.pool)
        .await
        .unwrap()
        .remove(0);
    engine
        .act_at(
            Action::SaveKarmaRule {
                rule: Some(rule.uid),
                expected_revision: Some(rule.revision),
                fields: [
                    "-2 * freq(@daily) + @escovar-dentes",
                    "!=0",
                    "@escovar-dentes",
                ]
                .map(|source| nucleus::karma::rule_field::RuleFieldInput::Text {
                    source: source.into(),
                }),
                request_id: "edit-after-emission".into(),
            },
            None,
            now + chrono::TimeDelta::seconds(1),
        )
        .await
        .unwrap();
    engine
        .advance_karma_time(now + chrono::TimeDelta::seconds(2))
        .await
        .unwrap();
    assert_eq!(
        store::facts::level(&engine.store.pool, &target)
            .await
            .unwrap()
            .to_string(),
        "0"
    );
    engine
        .advance_karma_time(overdue + chrono::TimeDelta::days(1))
        .await
        .unwrap();
    assert_eq!(
        store::facts::level(&engine.store.pool, &target)
            .await
            .unwrap()
            .to_string(),
        "-2"
    );
}
