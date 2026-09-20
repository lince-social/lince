use std::sync::Arc;

use engine::Engine;
use engine::actions::Action;
use engine::file_sync::{FileSyncState, spawn_configured_watchers, spawn_supervisor};
use nucleus::RecordKind;

async fn cell_with_local_organ() -> (Engine, String) {
    let e = Engine::open_memory().await.expect("engine");
    let organ = store::organs::ensure_local(&e.store.pool, "http://cell-a")
        .await
        .unwrap()
        .uid;
    (e, organ)
}

async fn plain(e: &Engine, head: &str, body: &str) -> String {
    e.act(
        Action::CreateRecord {
            slug: None,
            kind: RecordKind::Plain,
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

fn tmp_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(nucleus::new_uid("filesync-dir"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[tokio::test]
async fn mirror_writes_head_as_filename_and_body_as_content() {
    let (e, organ) = cell_with_local_organ().await;
    plain(&e, "My Note", "Hello world").await;
    let dir = tmp_dir();
    let mut state = FileSyncState::new();

    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    let content = std::fs::read_to_string(dir.join("My Note.md")).unwrap();
    assert_eq!(content, "Hello world");
}

#[tokio::test]
async fn disk_body_edit_flows_back_through_the_action_path_and_reaches_a_subscriber() {
    let (e, organ) = cell_with_local_organ().await;
    let uid = plain(&e, "Journal", "first draft").await;
    let dir = tmp_dir();
    let mut state = FileSyncState::new();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    let mut bus = e.subscribe();
    std::fs::write(dir.join("Journal.md"), "edited by hand").unwrap();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    let row = store::records::get(&e.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.body, "edited by hand");

    let fact = tokio::time::timeout(std::time::Duration::from_secs(1), bus.recv())
        .await
        .expect("a fact arrived on the bus")
        .expect("bus not closed");
    assert_eq!(fact.record_uid, uid);
}

#[tokio::test]
async fn a_stray_new_file_becomes_a_record_stamped_to_the_organ() {
    let (e, organ) = cell_with_local_organ().await;
    let dir = tmp_dir();
    std::fs::write(dir.join("Grocery List.md"), "milk\neggs").unwrap();
    let mut state = FileSyncState::new();

    let report = e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();
    assert_eq!(report.created.len(), 1);

    let created = store::records::resolve(&e.store.pool, &report.created[0])
        .await
        .unwrap()
        .expect("created record resolves");
    assert_eq!(created.head, "Grocery List");
    assert_eq!(created.body, "milk\neggs");
    assert_eq!(created.organ_uid.as_deref(), Some(organ.as_str()));
}

#[tokio::test]
async fn file_removal_is_debounced_then_hard_deletes_the_record() {
    let (e, organ) = cell_with_local_organ().await;
    let uid = plain(&e, "Scratch", "temp").await;
    let dir = tmp_dir();
    let mut state = FileSyncState::new();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    std::fs::remove_file(dir.join("Scratch.md")).unwrap();

    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();
    assert!(
        store::records::get(&e.store.pool, &uid)
            .await
            .unwrap()
            .is_some(),
        "record survives a single missing tick"
    );
    assert!(!dir.join("Scratch.md").exists());

    let report = e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();
    assert_eq!(report.deleted, vec![uid.clone()]);
    assert!(
        store::records::get(&e.store.pool, &uid)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn a_record_falling_out_of_organ_selection_loses_its_file() {
    let (e, organ) = cell_with_local_organ().await;
    let other_organ = store::organs::add_contact(
        &e.store.pool,
        "organ_other_uid",
        Some("organ.other"),
        "Other Cell",
        "http://cell-other",
        1,
    )
    .await
    .unwrap();
    let uid = plain(&e, "Reassigned", "content").await;
    let dir = tmp_dir();
    let mut state = FileSyncState::new();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();
    assert!(dir.join("Reassigned.md").exists());

    store::records::set_organ_origin(&e.store.pool, &uid, Some(&other_organ))
        .await
        .unwrap();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    assert!(!dir.join("Reassigned.md").exists());
    assert!(
        store::records::get(&e.store.pool, &uid)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn an_app_side_head_edit_renames_the_file() {
    let (e, organ) = cell_with_local_organ().await;
    let uid = plain(&e, "Old Title", "body").await;
    let dir = tmp_dir();
    let mut state = FileSyncState::new();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();
    assert!(dir.join("Old Title.md").exists());

    e.act(
        Action::EditRecordText {
            target: uid,
            head: Some("New Title".into()),
            body: None,
        },
        None,
    )
    .await
    .unwrap();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    assert!(!dir.join("Old Title.md").exists());
    assert_eq!(
        std::fs::read_to_string(dir.join("New Title.md")).unwrap(),
        "body"
    );
}

#[tokio::test]
async fn same_head_records_get_disambiguated_filenames() {
    let (e, organ) = cell_with_local_organ().await;
    let a = plain(&e, "Untitled", "a").await;
    let b = plain(&e, "Untitled", "b").await;
    let dir = tmp_dir();
    let mut state = FileSyncState::new();

    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    let entries: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        entries.len(),
        2,
        "both records get their own file: {entries:?}"
    );
    assert!(entries.contains(&format!("Untitled -- {a}.md")));
    assert!(entries.contains(&format!("Untitled -- {b}.md")));
}

#[tokio::test]
async fn boot_wiring_spawns_a_watch_loop_only_for_organs_enabled_in_their_extension() {
    let (e, organ) = cell_with_local_organ().await;
    let e = Arc::new(e);
    let dir = tmp_dir();

    let handles = spawn_configured_watchers(e.clone()).await.unwrap();
    assert_eq!(handles.len(), 0);

    store::records::set_extension(
        &e.store.pool,
        &organ,
        "lince.file_sync",
        &serde_json::json!({ "enabled": false, "path": dir.to_string_lossy() }),
    )
    .await
    .unwrap();
    let handles = spawn_configured_watchers(e.clone()).await.unwrap();
    assert_eq!(handles.len(), 0);

    store::records::set_extension(
        &e.store.pool,
        &organ,
        "lince.file_sync",
        &serde_json::json!({ "enabled": true, "path": dir.to_string_lossy() }),
    )
    .await
    .unwrap();
    let handles = spawn_configured_watchers(e.clone()).await.unwrap();
    assert_eq!(handles.len(), 1);
    for handle in handles {
        handle.abort();
    }
}

#[tokio::test]
async fn supervisor_starts_and_stops_watch_loops_live_without_reboot() {
    let (e, organ) = cell_with_local_organ().await;
    let e = Arc::new(e);
    let dir = tmp_dir();

    let _supervisor = spawn_supervisor(e.clone());

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(std::fs::read_dir(&dir).unwrap().next().is_none());

    let uid = plain(&e, "Live", "content").await;

    e.act(
        Action::SetExtension {
            target: organ.clone(),
            namespace: "lince.file_sync".to_string(),
            fds: serde_json::json!({ "enabled": true, "path": dir.to_string_lossy() }),
        },
        None,
    )
    .await
    .unwrap();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if dir.join("Live.md").exists() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "watch loop never started"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(
        std::fs::read_to_string(dir.join("Live.md")).unwrap(),
        "content"
    );

    e.act(
        Action::SetExtension {
            target: organ.clone(),
            namespace: "lince.file_sync".to_string(),
            fds: serde_json::json!({ "enabled": false, "path": dir.to_string_lossy() }),
        },
        None,
    )
    .await
    .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    std::fs::write(dir.join("Live.md"), "changed after disable").unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let row = store::records::get(&e.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.body, "content",
        "disabled watch loop must not apply disk edits"
    );
}

#[tokio::test]
async fn a_configured_filter_narrows_what_reaches_disk() {
    let (e, organ) = cell_with_local_organ().await;
    let kept_uid = plain(&e, "Kept", "in the folder").await;
    let dropped = plain(&e, "Dropped", "not selected").await;
    e.act(
        Action::SetExtension {
            target: organ.clone(),
            namespace: "lince.file_sync".into(),
            fds: serde_json::json!({
                "enabled": true,
                "path": "/unused-here",
                "filter": r#"{"slug_eq":"kept-slug"}"#,
            }),
        },
        None,
    )
    .await
    .expect("configure");
    store::records::set_slug(&e.store.pool, &dropped, Some("other-slug"))
        .await
        .expect("slug");
    store::records::set_slug(&e.store.pool, &kept_uid, Some("kept-slug"))
        .await
        .expect("slug");

    let dir = tmp_dir();
    let mut state = FileSyncState::new();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    assert!(
        dir.join("Kept.md").exists(),
        "the selected record is mirrored"
    );
    assert!(
        !dir.join("Dropped.md").exists(),
        "and one the filter excludes is not — origin alone is no longer the whole selection"
    );
}

#[tokio::test]
async fn an_unreadable_filter_syncs_everything_rather_than_nothing() {
    let (e, organ) = cell_with_local_organ().await;
    plain(&e, "Still here", "body").await;
    e.act(
        Action::SetExtension {
            target: organ.clone(),
            namespace: "lince.file_sync".into(),
            fds: serde_json::json!({
                "enabled": true,
                "path": "/unused-here",
                "filter": "{this is not a predicate",
            }),
        },
        None,
    )
    .await
    .expect("configure");

    let dir = tmp_dir();
    let mut state = FileSyncState::new();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();
    assert!(
        dir.join("Still here.md").exists(),
        "an unreadable filter must not silently empty the folder"
    );

    let config = store::records::get_extension(&e.store.pool, &organ, "lince.file_sync")
        .await
        .unwrap();
    assert_eq!(
        engine::file_sync::unreadable_filter(config.as_ref()).as_deref(),
        Some("{this is not a predicate"),
        "and the surface must be able to say it is being ignored"
    );
}

async fn sync_protein(e: &Engine, name: &str) -> String {
    e.act(Action::SaveProtein {
        slug: "directory-selection".into(), head: "Directory selection".into(),
        ast: serde_json::json!({"source":"record", "where":[{"all":[{"kind_eq":"plain"},{"text_contains":name}]}]}),
    }, None).await.unwrap().created.unwrap()
}

#[tokio::test]
async fn configured_protein_syncs_one_format_and_tracks_saved_query_changes() {
    for format in [
        engine::file_sync::FileFormat::Lingua,
        engine::file_sync::FileFormat::Markdown,
    ] {
        let (e, organ) = cell_with_local_organ().await;
        let first = plain(&e, "Selected first", "Original body").await;
        plain(&e, "Second note", "Second body").await;
        let protein = sync_protein(&e, "Selected").await;
        let dir = tmp_dir();
        e.act(
            Action::ConfigureFileSync {
                protein: protein.clone(),
                path: dir.to_string_lossy().into(),
                format,
                enabled: true,
            },
            None,
        )
        .await
        .unwrap();
        let config = store::records::get_extension(&e.store.pool, &organ, "lince.file_sync")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(config["protein"], protein);
        assert!(config.get("formats").is_none());
        let mut state = FileSyncState::new();
        e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();
        let path = dir.join(format!("Selected first.{}", format.extension()));
        assert!(path.exists());
        assert!(
            !dir.join(format!("Second note.{}", format.extension()))
                .exists()
        );
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        let text = std::fs::read_to_string(&path)
            .unwrap()
            .replace("Original body", "Edited body");
        std::fs::write(&path, text).unwrap();
        e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();
        assert_eq!(
            store::records::get(&e.store.pool, &first)
                .await
                .unwrap()
                .unwrap()
                .body,
            if format == engine::file_sync::FileFormat::Lingua {
                "Edited body\n"
            } else {
                "Edited body"
            }
        );
        sync_protein(&e, "Second").await;
        e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();
        assert!(
            dir.join(format!("Second note.{}", format.extension()))
                .exists()
        );
        e.act(
            Action::ConfigureFileSync {
                protein: String::new(),
                path: String::new(),
                format,
                enabled: false,
            },
            None,
        )
        .await
        .unwrap();
        let config = store::records::get_extension(&e.store.pool, &organ, "lince.file_sync")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(config["enabled"], false);
        assert_eq!(config["protein"], protein);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[tokio::test]
async fn invalid_directory_or_protein_leaves_sync_configuration_unchanged() {
    let (e, organ) = cell_with_local_organ().await;
    let protein = sync_protein(&e, "Selected").await;
    let dir = tmp_dir();
    for (protein, path) in [
        (protein.clone(), "relative/path".to_string()),
        ("missing-protein".into(), dir.to_string_lossy().into()),
    ] {
        assert!(
            e.act(
                Action::ConfigureFileSync {
                    protein,
                    path,
                    format: engine::file_sync::FileFormat::Lingua,
                    enabled: true
                },
                None
            )
            .await
            .is_err()
        );
        assert!(
            store::records::get_extension(&e.store.pool, &organ, "lince.file_sync")
                .await
                .unwrap()
                .is_none()
        );
    }
    e.act(
        Action::ConfigureFileSync {
            protein: protein.clone(),
            path: dir.to_string_lossy().into(),
            format: engine::file_sync::FileFormat::Lingua,
            enabled: true,
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::SetQuantity {
            target: protein,
            value: 0.0,
        },
        None,
    )
    .await
    .unwrap();
    std::fs::write(dir.join("Unimported.lingua"), "Must stay untouched").unwrap();
    assert!(
        e.file_sync_tick(&dir, &organ, &mut FileSyncState::new())
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("Unimported.lingua")).unwrap(),
        "Must stay untouched"
    );
    assert!(
        store::records::resolve(&e.store.pool, "Unimported")
            .await
            .unwrap()
            .is_none()
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn remote_admin_cannot_configure_computer_directory_sync() {
    let (e, organ) = cell_with_local_organ().await;
    e.act(
        Action::CreateRole {
            name: "admin".into(),
        },
        None,
    )
    .await
    .unwrap();
    let actor = e
        .act(
            Action::CreateUser {
                username: "sync-admin".into(),
                name: "Sync admin".into(),
                password: "test-password-long".into(),
                role: "admin".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let result = e
        .act(
            Action::ConfigureFileSync {
                protein: "anything".into(),
                path: "/tmp/directory-sync-forbidden".into(),
                format: engine::file_sync::FileFormat::Lingua,
                enabled: true,
            },
            Some(actor.clone()),
        )
        .await;
    assert!(matches!(
        result,
        Err(engine::error::EngineError::Forbidden(_))
    ));
    assert!(matches!(
        e.act(
            Action::SetFileSyncEnabled { enabled: true },
            Some(actor.clone())
        )
        .await,
        Err(engine::error::EngineError::Forbidden(_))
    ));
    let result = e
        .act(
            Action::SetExtension {
                target: organ.clone(),
                namespace: "lince.file_sync".into(),
                fds: serde_json::json!({"enabled":true,"path":"/tmp/directory-sync-forbidden"}),
            },
            Some(actor),
        )
        .await;
    assert!(matches!(
        result,
        Err(engine::error::EngineError::Forbidden(_))
    ));
    assert!(
        store::records::get_extension(&e.store.pool, &organ, "lince.file_sync")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn protein_directory_sync_honors_order_and_limit() {
    let (e, organ) = cell_with_local_organ().await;
    plain(&e, "First", "One").await;
    plain(&e, "Last", "Two").await;
    let protein = e.act(Action::SaveProtein {
        slug: "limited-sync".into(), head: "Limited sync".into(),
        ast: serde_json::json!({"source":"record","where":[{"all":[{"kind_eq":"plain"}]}],"order":[{"desc":"head"}],"limit":1}),
    }, None).await.unwrap().created.unwrap();
    let dir = tmp_dir();
    e.act(
        Action::ConfigureFileSync {
            protein,
            path: dir.to_string_lossy().into(),
            format: engine::file_sync::FileFormat::Markdown,
            enabled: true,
        },
        None,
    )
    .await
    .unwrap();
    e.file_sync_tick(&dir, &organ, &mut FileSyncState::new())
        .await
        .unwrap();
    assert!(dir.join("Last.md").exists());
    assert!(!dir.join("First.md").exists());
    std::fs::remove_dir_all(dir).unwrap();
}
