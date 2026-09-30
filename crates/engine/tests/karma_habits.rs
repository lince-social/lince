use std::future::Future;

use chrono::{DateTime, Utc};
use engine::{
    Engine,
    actions::Action,
    karma_habits::{Imported, Input, Kind, Preview},
};

fn run(test: impl Future<Output = ()> + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    nucleus::execution::Execution::new([6; 32], 1_893_456_000_000)
                        .unwrap()
                        .scope(test)
                        .await;
                });
        })
        .unwrap()
        .join()
        .unwrap();
}

async fn engine() -> Engine {
    let engine = Engine::open_memory().await.unwrap();
    engine
        .install_karma_runtime_config(
            engine::karma_runtime::KarmaDeadlineDirectorConfig::for_host("habit-test".into())
                .unwrap(),
        )
        .unwrap();
    engine
}

fn now() -> DateTime<Utc> {
    nucleus::execution::now()
}

async fn preview(engine: &Engine, input: Input) -> Preview {
    serde_json::from_value(
        engine
            .act(Action::PreviewKarmaHabit { input }, None)
            .await
            .unwrap()
            .data
            .unwrap(),
    )
    .unwrap()
}

fn import_action(input: Input, preview: &Preview, request: &str) -> Action {
    Action::ImportKarmaHabit {
        input,
        expected_preview: preview.fingerprint.clone(),
        request_id: request.into(),
    }
}

async fn import(engine: &Engine, input: Input, preview: &Preview, request: &str) -> Imported {
    serde_json::from_value(
        engine
            .act(import_action(input, preview, request), None)
            .await
            .unwrap()
            .data
            .unwrap(),
    )
    .unwrap()
}

async fn quantity(engine: &Engine, record: &str) -> nucleus::DecimalValue {
    store::records::get(&engine.store.pool, record)
        .await
        .unwrap()
        .unwrap()
        .quantity
}

#[test]
fn the_daily_import_fires_only_at_the_upcoming_boundary_and_completion_sets_zero() {
    run(async {
        let engine = engine().await;
        let input = Input::default();
        let before = engine.store.state_hash().await.unwrap();
        let candidate = preview(&engine, input.clone()).await;
        assert_eq!(engine.store.state_hash().await.unwrap(), before);
        assert_eq!(
            candidate.first_at_ms,
            now().timestamp_millis() + 18 * 3_600_000
        );
        assert!(candidate.objects.iter().all(|object| !object.exists));
        let action = import_action(input.clone(), &candidate, "daily");
        let first = engine.act(action.clone(), None).await.unwrap();
        let imported: Imported = serde_json::from_value(first.data.clone().unwrap()).unwrap();
        let room = &imported.objects[0].uid;
        assert_eq!(imported.objects[2].name, "Cleaning Room daily reset");
        assert_eq!(imported.objects[2].slug, "cleaning-room.daily-reset");
        assert_eq!(quantity(&engine, room).await, store::exact::zero());
        let fingerprint = engine.store.state_hash().await.unwrap();
        assert_eq!(engine.act(action, None).await.unwrap().data, first.data);
        assert_eq!(engine.store.state_hash().await.unwrap(), fingerprint);
        engine
            .advance_karma_time(DateTime::from_timestamp_millis(candidate.first_at_ms - 1).unwrap())
            .await
            .unwrap();
        assert!(quantity(&engine, room).await.is_zero());
        engine
            .advance_karma_time(DateTime::from_timestamp_millis(candidate.first_at_ms).unwrap())
            .await
            .unwrap();
        assert_eq!(quantity(&engine, room).await, store::exact::integer(-1));
        engine
            .advance_karma_time(
                DateTime::from_timestamp_millis(candidate.first_at_ms + 86_400_000).unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(quantity(&engine, room).await, store::exact::integer(-1));
        engine
            .act(
                Action::SetQuantityExact {
                    target: room.clone(),
                    amount: "0".into(),
                },
                None,
            )
            .await
            .unwrap();
        assert!(quantity(&engine, room).await.is_zero());
        engine
            .advance_karma_time(
                DateTime::from_timestamp_millis(candidate.first_at_ms + 2 * 86_400_000).unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(quantity(&engine, room).await, store::exact::integer(-1));
        assert_eq!(
            store::recurrence::all(&engine.store.pool)
                .await
                .unwrap()
                .len(),
            1
        );
    });
}

#[test]
fn repeat_import_preserves_edits_pauses_and_reused_old_names() {
    run(async {
        let engine = engine().await;
        let input = Input::default();
        let candidate = preview(&engine, input.clone()).await;
        let imported = import(&engine, input.clone(), &candidate, "initial").await;
        let room = &imported.objects[0].uid;
        engine
            .act(
                Action::SetQuantityExact {
                    target: room.clone(),
                    amount: "7".into(),
                },
                None,
            )
            .await
            .unwrap();
        engine
            .act(
                Action::SetSlug {
                    target: room.clone(),
                    slug: Some("my-room".into()),
                },
                None,
            )
            .await
            .unwrap();
        engine
            .act(
                Action::EditRecordText {
                    target: room.clone(),
                    head: Some("My room".into()),
                    body: Some("Keep this text".into()),
                },
                None,
            )
            .await
            .unwrap();
        engine
            .act(
                Action::CreateRecord {
                    slug: Some("cleaning-room".into()),
                    head: "Different room".into(),
                    body: String::new(),
                    kind: nucleus::RecordKind::Plain,
                    quantity: 0.0,
                },
                None,
            )
            .await
            .unwrap();
        let rule = store::recurrence::get(&engine.store.pool, &imported.objects[2].uid)
            .await
            .unwrap()
            .unwrap();
        engine
            .act(
                Action::SetRecurrencePaused {
                    recurrence: rule.uid.clone(),
                    expected_revision: rule.revision,
                    paused: true,
                    request_id: "pause-rule".into(),
                },
                None,
            )
            .await
            .unwrap();
        let frequency =
            store::karma::frequencies::get_handle(&engine.store.pool, &imported.objects[1].uid)
                .await
                .unwrap()
                .unwrap();
        engine
            .act(
                Action::PauseKarmaFrequency {
                    request_id: "pause-frequency".into(),
                    frequency_uid: frequency.record_uid.clone(),
                    expected_handle_revision: frequency.handle_revision,
                },
                None,
            )
            .await
            .unwrap();
        let rule_before = store::recurrence::get(&engine.store.pool, &rule.uid)
            .await
            .unwrap();
        let frequency_before =
            store::karma::frequencies::get_handle(&engine.store.pool, &frequency.record_uid)
                .await
                .unwrap();
        let candidate = preview(
            &engine,
            Input {
                time: "09:00".into(),
                ..input.clone()
            },
        )
        .await;
        assert!(candidate.imported);
        assert!(candidate.conflicts.is_empty());
        assert_eq!(candidate.input, input);
        import(
            &engine,
            Input {
                time: "09:00".into(),
                ..input
            },
            &candidate,
            "repeat",
        )
        .await;
        assert_eq!(quantity(&engine, room).await, store::exact::integer(7));
        assert_eq!(
            store::records::get(&engine.store.pool, room)
                .await
                .unwrap()
                .unwrap()
                .body,
            "Keep this text"
        );
        assert_eq!(
            store::recurrence::get(&engine.store.pool, &rule.uid)
                .await
                .unwrap(),
            rule_before
        );
        assert_eq!(
            store::karma::frequencies::get_handle(&engine.store.pool, &frequency.record_uid)
                .await
                .unwrap(),
            frequency_before
        );
    });
}

#[test]
fn conflicts_stale_previews_and_deleted_objects_never_create_or_replace_objects() {
    run(async {
        let engine = engine().await;
        let input = Input::default();
        let candidate = preview(&engine, input.clone()).await;
        engine
            .act(
                Action::CreateRecord {
                    slug: Some("cleaning-room".into()),
                    head: "Existing".into(),
                    body: String::new(),
                    kind: nucleus::RecordKind::Plain,
                    quantity: 4.0,
                },
                None,
            )
            .await
            .unwrap();
        let before = engine.store.state_hash().await.unwrap();
        assert!(
            engine
                .act(import_action(input.clone(), &candidate, "conflict"), None)
                .await
                .is_err()
        );
        assert_eq!(engine.store.state_hash().await.unwrap(), before);
        let conflict = preview(&engine, input.clone()).await;
        assert_eq!(conflict.conflicts, vec!["@cleaning-room is already in use"]);
        engine
            .act(
                Action::DeleteRecord {
                    target: "cleaning-room".into(),
                },
                None,
            )
            .await
            .unwrap();
        let candidate = preview(&engine, input.clone()).await;
        let mut stale = candidate.clone();
        stale.fingerprint = "different".into();
        let before = engine.store.state_hash().await.unwrap();
        assert!(
            engine
                .act(import_action(input.clone(), &stale, "stale"), None)
                .await
                .is_err()
        );
        assert_eq!(engine.store.state_hash().await.unwrap(), before);
        let imported = import(&engine, input.clone(), &candidate, "good").await;
        engine
            .act(
                Action::DeleteRecurrence {
                    recurrence: imported.objects[2].uid.clone(),
                },
                None,
            )
            .await
            .unwrap();
        let candidate = preview(&engine, input.clone()).await;
        assert!(
            candidate
                .conflicts
                .iter()
                .any(|conflict| conflict.contains("was deleted"))
        );
        let before = engine.store.state_hash().await.unwrap();
        assert!(
            engine
                .act(import_action(input, &candidate, "deleted"), None)
                .await
                .is_err()
        );
        assert_eq!(engine.store.state_hash().await.unwrap(), before);
    });
}

#[test]
fn a_partial_import_retries_the_original_definition_without_resetting_the_created_record() {
    run(async {
        let engine = engine().await;
        let input = Input::default();
        let candidate = preview(&engine, input.clone()).await;
        let action = import_action(input.clone(), &candidate, "retry");
        store::sqlx::query("CREATE TRIGGER fixture_habit_failure BEFORE INSERT ON karma_frequency BEGIN SELECT RAISE(ABORT, 'fixture import failure'); END").execute(&engine.store.pool).await.unwrap();
        assert!(engine.act(action.clone(), None).await.is_err());
        assert!(
            store::records::get(&engine.store.pool, &candidate.objects[0].uid)
                .await
                .unwrap()
                .is_some()
        );
        let markers: Vec<(String, i64)> =
            store::sqlx::query_as("SELECT kind, created FROM karma_habit_object ORDER BY kind")
                .fetch_all(&engine.store.pool)
                .await
                .unwrap();
        assert_eq!(
            markers,
            vec![
                ("frequency".into(), 0),
                ("record".into(), 1),
                ("rule".into(), 0)
            ]
        );
        engine
            .act(
                Action::SetQuantityExact {
                    target: candidate.objects[0].uid.clone(),
                    amount: "-5".into(),
                },
                None,
            )
            .await
            .unwrap();
        store::sqlx::query("DROP TRIGGER fixture_habit_failure")
            .execute(&engine.store.pool)
            .await
            .unwrap();
        let later = preview(
            &engine,
            Input {
                time: "09:00".into(),
                ..input
            },
        )
        .await;
        assert_eq!(later.first_at_ms, candidate.first_at_ms);
        assert_eq!(later.input.time, "18:00");
        assert!(engine.act(action, None).await.unwrap().data.is_some());
        assert_eq!(
            quantity(&engine, &candidate.objects[0].uid).await,
            store::exact::integer(-5)
        );
        assert!(preview(&engine, Input::default()).await.imported);
    });
}

#[test]
fn import_uses_the_callers_permissions_and_retains_their_authority_on_the_objects() {
    run(async {
        let engine = engine().await;
        let person = engine
            .act(
                Action::CreateRecord {
                    slug: Some("person".into()),
                    kind: nucleus::RecordKind::Person,
                    head: "Person".into(),
                    body: String::new(),
                    quantity: 1.0,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let role = store::auth::ensure_role(&engine.store.pool, "habit-importer")
            .await
            .unwrap();
        for (subject, action) in [("record", "read"), ("frequency", "read")] {
            let permission = store::auth::ensure_permission(&engine.store.pool, subject, action)
                .await
                .unwrap();
            store::auth::grant(&engine.store.pool, role, permission)
                .await
                .unwrap();
        }
        store::auth::create_credential(&engine.store.pool, &person, "person", "hash", role)
            .await
            .unwrap();
        let input = Input::default();
        let candidate: Preview = serde_json::from_value(
            engine
                .act(
                    Action::PreviewKarmaHabit {
                        input: input.clone(),
                    },
                    Some(person.clone()),
                )
                .await
                .unwrap()
                .data
                .unwrap(),
        )
        .unwrap();
        let action = import_action(input, &candidate, "actor");
        let before = engine.store.state_hash().await.unwrap();
        assert!(matches!(
            engine.act(action.clone(), Some(person.clone())).await,
            Err(engine::EngineError::Forbidden(_))
        ));
        assert_eq!(engine.store.state_hash().await.unwrap(), before);
        for (subject, action) in [
            ("record", "create"),
            ("record", "update"),
            ("frequency", "create"),
            ("frequency", "update"),
            ("karma", "update"),
        ] {
            let permission = store::auth::ensure_permission(&engine.store.pool, subject, action)
                .await
                .unwrap();
            store::auth::grant(&engine.store.pool, role, permission)
                .await
                .unwrap();
        }
        let imported: Imported = serde_json::from_value(
            engine
                .act(action, Some(person.clone()))
                .await
                .unwrap()
                .data
                .unwrap(),
        )
        .unwrap();
        let rule = store::recurrence::get(
            &engine.store.pool,
            &imported
                .objects
                .iter()
                .find(|object| object.kind == Kind::Rule)
                .unwrap()
                .uid,
        )
        .await
        .unwrap()
        .unwrap();
        let frequency =
            store::karma::frequencies::get_handle(&engine.store.pool, &imported.objects[1].uid)
                .await
                .unwrap()
                .unwrap();
        assert_eq!(rule.actor_uid.as_deref(), Some(person.as_str()));
        assert_eq!(frequency.owner_person_uid.as_deref(), Some(person.as_str()));
    });
}

#[test]
fn local_time_resolves_the_brazilian_offset_and_a_dst_gap_and_fold() {
    run(async {
        let engine = engine().await;
        let mut input = Input {
            time: "09:00".into(),
            timezone: nucleus::karma::TimeZoneId::new("America/Sao_Paulo").unwrap(),
            ..Default::default()
        };
        assert_eq!(
            preview(&engine, input.clone()).await.first_at_ms,
            now().timestamp_millis() + 12 * 3_600_000
        );
        let execution = nucleus::execution::current().unwrap();
        execution
            .set_time(
                DateTime::parse_from_rfc3339("2025-03-09T05:00:00Z")
                    .unwrap()
                    .timestamp_millis(),
            )
            .unwrap();
        input.time = "02:30".into();
        input.timezone = nucleus::karma::TimeZoneId::new("America/New_York").unwrap();
        assert_eq!(
            preview(&engine, input.clone()).await.first_at_ms,
            DateTime::parse_from_rfc3339("2025-03-09T07:00:00Z")
                .unwrap()
                .timestamp_millis()
        );
        input.gap = nucleus::karma::GapPolicy::Pause;
        let before = engine.store.state_hash().await.unwrap();
        assert!(
            engine
                .act(
                    Action::PreviewKarmaHabit {
                        input: input.clone()
                    },
                    None
                )
                .await
                .unwrap_err()
                .to_string()
                .contains("clock gap")
        );
        assert_eq!(engine.store.state_hash().await.unwrap(), before);
        input.gap = nucleus::karma::GapPolicy::Skip;
        assert_eq!(
            preview(&engine, input.clone()).await.first_at_ms,
            DateTime::parse_from_rfc3339("2025-03-10T06:30:00Z")
                .unwrap()
                .timestamp_millis()
        );
        execution
            .set_time(
                DateTime::parse_from_rfc3339("2025-11-02T05:45:00Z")
                    .unwrap()
                    .timestamp_millis(),
            )
            .unwrap();
        input.time = "01:30".into();
        input.fold = nucleus::karma::FoldPolicy::Second;
        assert_eq!(
            preview(&engine, input.clone()).await.first_at_ms,
            DateTime::parse_from_rfc3339("2025-11-02T06:30:00Z")
                .unwrap()
                .timestamp_millis()
        );
        input.fold = nucleus::karma::FoldPolicy::Pause;
        assert!(
            engine
                .act(Action::PreviewKarmaHabit { input }, None)
                .await
                .unwrap_err()
                .to_string()
                .contains("repeated")
        );
    });
}
