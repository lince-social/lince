use engine::{Engine, actions::Action};
use nucleus::{
    RecordKind,
    karma::{ReferenceKind, TypedUid},
    plugins::{Binding, Command, Direction},
    projection::{Context, Scheduled, Window},
    schedule::TimeRange,
    simulation::{Cause, Quantity, RuleOccurrence},
};
use serde_json::{Value, json};
use std::path::Path;

async fn engine(directory: &Path) -> Engine {
    let installed = tokio::process::Command::new(env!("CARGO_BIN_EXE_lince-plugin-files"))
        .arg("--install")
        .arg(directory)
        .output()
        .await
        .unwrap();
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );
    let engine = Engine::open_memory().await.unwrap();
    engine
        .load_plugins(directory.join("plugins"))
        .await
        .unwrap();
    let package = engine.plugin_catalog().await.unwrap().packages.remove(0);
    engine
        .plugin_command(
            Command::Enable {
                plugin: package.manifest.id,
                digest: package.digest,
                enabled: true,
            },
            None,
        )
        .await
        .unwrap();
    engine
}

async fn record(engine: &Engine, kind: RecordKind, head: &str, body: &str) -> String {
    engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind,
                head: head.into(),
                body: body.into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap()
}

async fn protein(engine: &Engine, query: Value) -> String {
    let uid = record(engine, RecordKind::Protein, "Export selection", "").await;
    store::records::set_extension(&engine.store.pool, &uid, "lince.protein", &query)
        .await
        .unwrap();
    uid
}

fn binding(directory: &Path, format: &str, protein: &str) -> Binding {
    Binding {
        uid: nucleus::new_uid("r"),
        provider: format!("lince.files/{format}"),
        name: format.into(),
        settings: json!({"directory":directory,"filename":if format=="ics" {"schedule.ics"}else{"records.md"},"title":"Personal"}),
        protein_uid: Some(protein.into()),
        incoming_filter: json!([]),
        direction: Direction::Outgoing,
        interval_seconds: 60,
        enabled: true,
        revision: 0,
    }
}

async fn save(engine: &Engine, binding: Binding) {
    let preview = engine
        .plugin_command(
            Command::Preview {
                binding: binding.clone(),
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    engine
        .plugin_command(
            Command::SaveBinding {
                binding,
                preview: preview["preview"].as_str().map(str::to_owned),
            },
            None,
        )
        .await
        .unwrap();
}

async fn cache(engine: &Engine, window: Window, uid: &str, incomplete: bool) {
    let config =
        engine::karma_runtime::KarmaDeadlineDirectorConfig::for_host("projection".into()).unwrap();
    store::projection::set_runtime(
        &engine.store.pool,
        config.projection_identity().unwrap().as_str(),
    )
    .await
    .unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    let scheduled: Vec<_> = (1..=3)
        .map(|day| Scheduled {
            id: format!("daily-shower-{day}"),
            record: TypedUid::new(ReferenceKind::Record, uid).unwrap(),
            time: TimeRange {
                from_ms: now + day * 86_400_000,
                until_ms: Some(now + day * 86_400_000 + 600_000),
            },
            quantity: Quantity {
                value: nucleus::DecimalValue::parse_inferred("-1").unwrap(),
                unit: None,
            },
            cause: Cause::Rule {
                occurrence: RuleOccurrence {
                    rule_uid: nucleus::new_uid("rec"),
                    revision: 1,
                    event_id: format!("day-{day}"),
                    frequency: None,
                    intended_at_ms: Some(now + day * 86_400_000),
                },
                consequence: 0,
            },
            head: "Shower".into(),
            slug: None,
            record_kind: "plain".into(),
            preview: false,
        })
        .collect();
    let revision = store::projection::revision(&engine.store.pool)
        .await
        .unwrap();
    assert!(
        store::projection::publish_schedule(
            &engine.store.pool,
            &Context {
                window,
                actor: None
            },
            revision,
            now,
            now + 3_600_000,
            if incomplete {
                Some(&nucleus::projection::Incomplete::Budget {})
            } else {
                None
            },
            &[],
            &scheduled
        )
        .await
        .unwrap()
    );
}

#[tokio::test]
async fn calendar_and_markdown_have_independent_destinations_and_restore_external_edits() {
    let directory = tempfile::tempdir().unwrap();
    let engine = engine(directory.path()).await;
    let shower = record(&engine, RecordKind::Plain, "Shower", "Private note").await;
    let notes = protein(
        &engine,
        json!({"source":"record","where":[{"uid_eq":shower}]}),
    )
    .await;
    let query: protein::Protein = serde_json::from_value(json!({"source":"schedule","where":[{"rolling_window":{"past_days":0,"future_days":7,"timezone":"UTC"}},{"uid_eq":shower}]})).unwrap();
    let window = protein::calendar::context(&query, None).unwrap().window;
    let schedule = protein(&engine, serde_json::to_value(query).unwrap()).await;
    let narrow = protein(&engine,json!({"source":"schedule","where":[{"rolling_window":{"past_days":0,"future_days":2,"timezone":"UTC"}},{"uid_eq":shower}]})).await;
    cache(&engine, window.clone(), &shower, false).await;
    let calendar = binding(&directory.path().join("calendar"), "ics", &schedule);
    let markdown = binding(&directory.path().join("notes"), "markdown", &notes);
    let short_calendar = binding(&directory.path().join("short-calendar"), "ics", &narrow);
    save(&engine, calendar.clone()).await;
    save(&engine, markdown.clone()).await;
    save(&engine, short_calendar.clone()).await;
    let narrower_query: protein::Protein = serde_json::from_value(json!({"source":"schedule","where":[{"rolling_window":{"past_days":0,"future_days":2,"timezone":"UTC"}},{"uid_eq":shower}]})).unwrap();
    let narrower_rows = protein::execute(&engine.store, &narrower_query)
        .await
        .unwrap();
    assert_eq!(
        narrower_rows
            .iter()
            .filter(|row| row["kind"] == "schedule-entry")
            .count(),
        1
    );
    assert_eq!(engine.run_plugin_binding(&calendar.uid).await.unwrap(), 3);
    assert_eq!(engine.run_plugin_binding(&markdown.uid).await.unwrap(), 1);
    let revision = store::projection::revision(&engine.store.pool)
        .await
        .unwrap();
    assert_eq!(
        engine
            .run_plugin_binding(&short_calendar.uid)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        store::projection::revision(&engine.store.pool)
            .await
            .unwrap(),
        revision
    );
    let file = directory.path().join("calendar/schedule.ics");
    let original = std::fs::read(&file).unwrap();
    assert_eq!(
        String::from_utf8_lossy(&original)
            .matches("BEGIN:VEVENT")
            .count(),
        3
    );
    assert!(
        std::fs::read_to_string(directory.path().join("notes/records.md"))
            .unwrap()
            .contains("Private note")
    );
    std::fs::write(&file, "external edit").unwrap();
    engine.run_plugin_binding(&calendar.uid).await.unwrap();
    assert_eq!(std::fs::read(&file).unwrap(), original);
    let duplicate = binding(&directory.path().join("calendar"), "ics", &schedule);
    assert!(
        engine
            .plugin_command(Command::Preview { binding: duplicate }, None)
            .await
            .is_err()
    );
    cache(&engine, window.clone(), &shower, true).await;
    assert!(engine.run_plugin_binding(&calendar.uid).await.is_err());
    assert_eq!(std::fs::read(&file).unwrap(), original);
    store::records::set_extension(&engine.store.pool,&schedule,"lince.protein",&json!({"source":"schedule","where":[{"rolling_window":{"past_days":0,"future_days":7,"timezone":"UTC"}},{"text_contains":"no matching record"}]})).await.unwrap();
    cache(&engine, window, &shower, false).await;
    assert_eq!(engine.run_plugin_binding(&calendar.uid).await.unwrap(), 0);
    assert!(
        !std::fs::read_to_string(&file)
            .unwrap()
            .contains("BEGIN:VEVENT")
    );
    assert_eq!(
        std::fs::read_to_string(directory.path().join("short-calendar/schedule.ics"))
            .unwrap()
            .matches("BEGIN:VEVENT")
            .count(),
        1
    );
    engine
        .plugin_command(
            Command::Stop {
                binding: calendar.uid.clone(),
            },
            None,
        )
        .await
        .unwrap();
    assert!(engine.run_plugin_binding(&calendar.uid).await.is_err());
}

#[tokio::test]
async fn export_rejects_wrong_sources_relative_paths_and_stale_previews() {
    let directory = tempfile::tempdir().unwrap();
    let engine = engine(directory.path()).await;
    let uid = record(&engine, RecordKind::Plain, "One", "Body").await;
    let selection = protein(&engine, json!({"source":"record","where":[{"uid_eq":uid}]})).await;
    let wrong = binding(directory.path(), "ics", &selection);
    assert!(
        engine
            .plugin_command(Command::Preview { binding: wrong }, None)
            .await
            .is_err()
    );
    let mut valid = binding(directory.path(), "markdown", &selection);
    let preview = engine
        .plugin_command(
            Command::Preview {
                binding: valid.clone(),
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    valid.settings["filename"] = json!("different.md");
    assert!(
        engine
            .plugin_command(
                Command::SaveBinding {
                    binding: valid.clone(),
                    preview: preview["preview"].as_str().map(str::to_owned)
                },
                None
            )
            .await
            .is_err()
    );
    valid.settings["directory"] = json!("relative/path");
    assert!(
        engine
            .plugin_command(
                Command::Preview {
                    binding: valid.clone()
                },
                None
            )
            .await
            .is_err()
    );
    valid.settings["directory"] = json!(directory.path());
    valid.settings["filename"] = json!("../escape.md");
    assert!(
        engine
            .plugin_command(Command::Preview { binding: valid }, None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn headless_schedule_export_requests_simulation_without_a_time_castle() {
    let directory = tempfile::tempdir().unwrap();
    let engine = std::sync::Arc::new(engine(directory.path()).await);
    let uid = record(&engine, RecordKind::Plain, "Shower", "").await;
    let at = chrono::Utc::now()
        .date_naive()
        .succ_opt()
        .unwrap()
        .and_hms_opt(9, 0, 0)
        .unwrap()
        .and_utc();
    engine
        .act(
            Action::SetExtension {
                target: uid.clone(),
                namespace: "work".into(),
                fds: json!({"estimate_min":10}),
            },
            None,
        )
        .await
        .unwrap();
    engine
        .act(
            Action::CreateRecurrence {
                target: uid.clone(),
                consequences: vec![nucleus::karma::Consequence::SetQuantity {
                    value: Some(nucleus::DecimalValue::parse_inferred("-1").unwrap()),
                }],
                condition: None,
                gate: None,
                carry: None,
                note: None,
                cadence: nucleus::karma::Cadence::every_days(1),
                anchor_at: Some(at.to_rfc3339()),
                request_id: None,
            },
            None,
        )
        .await
        .unwrap();
    let selection = protein(&engine,json!({"source":"schedule","where":[{"rolling_window":{"past_days":0,"future_days":4,"timezone":"UTC"}},{"uid_eq":uid}]})).await;
    let mut export = binding(directory.path(), "ics", &selection);
    export.interval_seconds = 86400;
    let preview = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            match engine
                .plugin_command(
                    Command::Preview {
                        binding: export.clone(),
                    },
                    None,
                )
                .await
            {
                Ok(result) => break result.data.unwrap(),
                Err(error) => {
                    assert!(error.to_string().contains("updating"), "{error}");
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(preview["count"], 3);
    save(&engine, export.clone()).await;
    store::sqlx::query("UPDATE projection_source SET revision=revision+1 WHERE id=1")
        .execute(&engine.store.pool)
        .await
        .unwrap();
    let worker = engine.clone().start_plugin_sync();
    let ready = tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            if directory.path().join("schedule.ics").is_file() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    })
    .await;
    worker.abort();
    ready.unwrap();
    let calendar = std::fs::read_to_string(directory.path().join("schedule.ics")).unwrap();
    assert_eq!(calendar.matches("SUMMARY:Shower").count(), 3);
    assert_eq!(calendar.matches("STATUS:TENTATIVE").count(), 3);
    assert_eq!(
        store::records::get(&engine.store.pool, &uid)
            .await
            .unwrap()
            .unwrap()
            .quantity
            .to_string(),
        "0"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn export_does_not_follow_an_existing_file_symlink() {
    let directory = tempfile::tempdir().unwrap();
    let engine = engine(directory.path()).await;
    let uid = record(&engine, RecordKind::Plain, "One", "Body").await;
    let selection = protein(&engine, json!({"source":"record","where":[{"uid_eq":uid}]})).await;
    let export = binding(&directory.path().join("out"), "markdown", &selection);
    save(&engine, export.clone()).await;
    std::fs::create_dir_all(directory.path().join("out")).unwrap();
    let untouched = directory.path().join("untouched.md");
    std::fs::write(&untouched, "original").unwrap();
    std::os::unix::fs::symlink(&untouched, directory.path().join("out/records.md")).unwrap();
    assert!(engine.run_plugin_binding(&export.uid).await.is_err());
    assert_eq!(std::fs::read_to_string(untouched).unwrap(), "original");
}

#[tokio::test]
async fn moving_schedule_previews_keep_the_policy_while_times_follow_the_cursor() {
    let directory = tempfile::tempdir().unwrap();
    let engine = engine(directory.path()).await;
    let uid = record(&engine, RecordKind::Plain, "Shower", "").await;
    engine
        .act(
            Action::AddQuantity {
                target: uid.clone(),
                delta: -1.0,
            },
            None,
        )
        .await
        .unwrap();
    engine
        .act(
            Action::SetExtension {
                target: uid.clone(),
                namespace: "work".into(),
                fds: json!({"estimate_min":10}),
            },
            None,
        )
        .await
        .unwrap();
    let selection = protein(&engine,json!({"source":"schedule","where":[{"rolling_window":{"past_days":0,"future_days":2,"timezone":"UTC"}},{"uid_eq":uid}],"order":[{"desc":"#task"}]})).await;
    let export = binding(directory.path(), "ics", &selection);
    let before = chrono::Utc::now().timestamp_millis();
    let first = tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            match engine
                .plugin_command(
                    Command::Preview {
                        binding: export.clone(),
                    },
                    None,
                )
                .await
            {
                Ok(result) => break result.data.unwrap(),
                Err(error) => {
                    assert!(error.to_string().contains("updating"), "{error}");
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
            }
        }
    })
    .await
    .unwrap();
    let start = first["records"][0]["time"]["from_ms"].as_i64().unwrap();
    assert!(start >= before);
    assert_eq!(
        first["records"][0]["time"]["until_ms"].as_i64().unwrap() - start,
        600_000
    );
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    let second = engine
        .plugin_command(
            Command::Preview {
                binding: export.clone(),
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    assert_eq!(first["preview"], second["preview"]);
    assert!(second["records"][0]["time"]["from_ms"].as_i64().unwrap() > start);
    engine
        .plugin_command(
            Command::SaveBinding {
                binding: export.clone(),
                preview: first["preview"].as_str().map(str::to_owned),
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(engine.run_plugin_binding(&export.uid).await.unwrap(), 1);
}
