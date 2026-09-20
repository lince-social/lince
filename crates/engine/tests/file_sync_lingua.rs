use engine::{
    Engine,
    actions::Action,
    file_sync::{FileFormat, FileSyncState},
};
use std::path::{Path, PathBuf};

fn user_file() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testsync/test.lingua")
}

async fn configured(directory: &Path) -> (Engine, String) {
    let engine = Engine::open_memory().await.unwrap();
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    engine
        .act(
            Action::ConfigureFileSync {
                protein: String::new(),
                path: directory.to_string_lossy().into(),
                format: FileFormat::Lingua,
                enabled: true,
            },
            None,
        )
        .await
        .unwrap();
    (engine, organ)
}

fn copy_user_file() -> PathBuf {
    let directory = std::env::temp_dir().join(nucleus::new_uid("lingua-sync-test"));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::copy(user_file(), directory.join("test.lingua")).unwrap();
    directory
}

async fn hello(engine: &Engine) -> store::records::RecordRow {
    store::records::resolve(&engine.store.pool, "hello-my-world")
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn existing_user_file_imports_into_two_independent_instances_without_rewriting() {
    let path = user_file();
    let source = std::fs::read(&path).unwrap();
    let directory = path.parent().unwrap();
    let before: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    let mut uids = Vec::new();
    for _ in 0..2 {
        let (engine, organ) = configured(directory).await;
        let mut state = FileSyncState::new();
        let report = engine
            .file_sync_tick(directory, &organ, &mut state)
            .await
            .unwrap();
        assert!(report.conflicts.is_empty(), "{:?}", report.conflicts);
        assert_eq!(report.created.len(), 1);
        assert!(report.written_to_disk.is_empty());
        let record = hello(&engine).await;
        assert_eq!(record.head, "Hello");
        assert_eq!(record.body, "Oieeee\n");
        assert_eq!(record.quantity.to_string(), "0");
        assert_eq!(record.organ_uid.as_deref(), Some(organ.as_str()));
        let query = serde_json::from_value(serde_json::json!({"source":"record","where":[{"all":[{"slug_eq":"hello-my-world"},{"concept_in":"backlog"},{"concept_in":"task"}]}]})).unwrap();
        let rows = protein::execute(&engine.store, &query).await.unwrap();
        assert_eq!(rows.len(), 1);
        for mut state in [state, FileSyncState::new()] {
            let report = engine
                .file_sync_tick(directory, &organ, &mut state)
                .await
                .unwrap();
            assert!(report.created.is_empty());
            assert!(report.updated_from_disk.is_empty());
            assert!(report.written_to_disk.is_empty());
            assert!(report.conflicts.is_empty(), "{:?}", report.conflicts);
        }
        uids.push(record.uid);
        assert_eq!(std::fs::read(&path).unwrap(), source);
    }
    assert_eq!(uids[0], uids[1]);
    let after: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(before, after);
}

#[tokio::test]
async fn edits_round_trip_through_the_current_grammar_and_reach_queries() {
    let directory = copy_user_file();
    let path = directory.join("test.lingua");
    let (engine, organ) = configured(&directory).await;
    let mut state = FileSyncState::new();
    engine
        .file_sync_tick(&directory, &organ, &mut state)
        .await
        .unwrap();
    let uid = hello(&engine).await.uid;
    let source = std::fs::read_to_string(&path)
        .unwrap()
        .replace("Oieeee", "Edited on disk")
        .replace(": 0,", ": -3.25,")
        .replace("#backlog", "#wip");
    std::fs::write(&path, source).unwrap();
    let report = engine
        .file_sync_tick(&directory, &organ, &mut state)
        .await
        .unwrap();
    assert!(report.conflicts.is_empty(), "{:?}", report.conflicts);
    assert_eq!(report.updated_from_disk, [uid.clone()]);
    assert_eq!(hello(&engine).await.quantity.to_string(), "-3.25");
    assert_eq!(hello(&engine).await.body, "Edited on disk\n");
    engine
        .act(
            Action::EditRecordText {
                target: uid.clone(),
                head: Some("Hello edited".into()),
                body: Some("Edited in Lince\n".into()),
            },
            None,
        )
        .await
        .unwrap();
    let report = engine
        .file_sync_tick(&directory, &organ, &mut state)
        .await
        .unwrap();
    assert!(report.conflicts.is_empty(), "{:?}", report.conflicts);
    assert_eq!(report.written_to_disk, [uid.clone()]);
    let source = std::fs::read_to_string(&path).unwrap();
    let projection = anicca::project(&anicca::parse(&source).unwrap()).unwrap();
    assert_eq!(projection.records[0].uid, uid);
    assert_eq!(
        projection.records[0].slug.as_deref(),
        Some("hello-my-world")
    );
    assert_eq!(projection.records[0].head, "Hello edited");
    assert_eq!(projection.records[0].body, "Edited in Lince\n");
    assert!(
        projection.records[0]
            .assertions
            .iter()
            .any(|a| a.predicate == "wip")
    );
    assert!(
        !projection.records[0]
            .assertions
            .iter()
            .any(|a| a.predicate == "backlog")
    );
    let report = engine
        .file_sync_tick(&directory, &organ, &mut state)
        .await
        .unwrap();
    assert!(report.updated_from_disk.is_empty());
    assert!(report.written_to_disk.is_empty());
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn malformed_file_and_concurrent_edits_are_reported_without_overwriting() {
    let directory = copy_user_file();
    let path = directory.join("test.lingua");
    let (engine, organ) = configured(&directory).await;
    let mut state = FileSyncState::new();
    engine
        .file_sync_tick(&directory, &organ, &mut state)
        .await
        .unwrap();
    let uid = hello(&engine).await.uid;
    let original = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, original.replace(": 0,", ": nope,")).unwrap();
    let report = engine
        .file_sync_tick(&directory, &organ, &mut state)
        .await
        .unwrap();
    assert_eq!(report.conflicts.len(), 1);
    assert_eq!(hello(&engine).await.body, "Oieeee\n");
    let changed = original.replace("Oieeee", "Disk change");
    std::fs::write(&path, &changed).unwrap();
    engine
        .act(
            Action::EditRecordText {
                target: uid,
                head: None,
                body: Some("App change".into()),
            },
            None,
        )
        .await
        .unwrap();
    let report = engine
        .file_sync_tick(&directory, &organ, &mut state)
        .await
        .unwrap();
    assert_eq!(report.conflicts.len(), 1);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), changed);
    assert_eq!(hello(&engine).await.body, "App change");
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn a_missing_file_is_debounced_then_deletes_the_imported_record() {
    let directory = copy_user_file();
    let (engine, organ) = configured(&directory).await;
    let mut state = FileSyncState::new();
    engine
        .file_sync_tick(&directory, &organ, &mut state)
        .await
        .unwrap();
    let uid = hello(&engine).await.uid;
    std::fs::remove_file(directory.join("test.lingua")).unwrap();
    engine
        .file_sync_tick(&directory, &organ, &mut state)
        .await
        .unwrap();
    assert!(
        store::records::get(&engine.store.pool, &uid)
            .await
            .unwrap()
            .is_some()
    );
    assert!(!directory.join("test.lingua").exists());
    let report = engine
        .file_sync_tick(&directory, &organ, &mut state)
        .await
        .unwrap();
    assert_eq!(report.deleted, [uid]);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn a_record_created_in_lince_exports_with_current_syntax() {
    let directory = copy_user_file();
    let (engine, organ) = configured(&directory).await;
    let mut state = FileSyncState::new();
    engine
        .file_sync_tick(&directory, &organ, &mut state)
        .await
        .unwrap();
    let record = hello(&engine).await;
    let uid = engine
        .act(
            Action::CreateRecord {
                slug: Some("another-hello".into()),
                kind: nucleus::RecordKind::Plain,
                head: record.head.clone(),
                body: record.body.clone(),
                quantity: 2.5,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let report = engine
        .file_sync_tick(&directory, &organ, &mut state)
        .await
        .unwrap();
    assert!(report.conflicts.is_empty(), "{:?}", report.conflicts);
    assert_eq!(report.written_to_disk, [uid.clone()]);
    let path = std::fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.file_name().unwrap() != "test.lingua")
        .unwrap();
    let source = std::fs::read_to_string(&path).unwrap();
    let records = anicca::project(&anicca::parse(&source).unwrap())
        .unwrap()
        .records;
    assert_eq!(records[0].uid, uid);
    assert_eq!(records[0].quantity.as_ref().unwrap().0, "2.5");
    assert_eq!(records[0].slug.as_deref(), Some("another-hello"));
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn multiple_records_and_forward_links_use_declared_slugs() {
    let directory = copy_user_file();
    let path = directory.join("test.lingua");
    let original = std::fs::read_to_string(&path).unwrap();
    let source = format!(
        "{}\n{}",
        original.replace("#task", "#task, #related @second-hello"),
        original.replace("@hello-my-world", "@second-hello")
    );
    std::fs::write(&path, &source).unwrap();
    let (engine, organ) = configured(&directory).await;
    let report = engine
        .file_sync_tick(&directory, &organ, &mut FileSyncState::new())
        .await
        .unwrap();
    assert!(report.conflicts.is_empty(), "{:?}", report.conflicts);
    assert_eq!(report.created.len(), 2);
    let first = hello(&engine).await;
    let second = store::records::resolve(&engine.store.pool, "second-hello")
        .await
        .unwrap()
        .unwrap();
    let assertions = store::assertions::for_subjects(&engine.store.pool, &[first.uid])
        .await
        .unwrap();
    assert!(
        assertions
            .iter()
            .any(|a| a.object_uid.as_deref() == Some(&second.uid))
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn slug_identity_and_unit_changes_preserve_the_record_identity() {
    let directory = copy_user_file();
    let path = directory.join("test.lingua");
    let (engine, organ) = configured(&directory).await;
    let mut state = FileSyncState::new();
    engine
        .file_sync_tick(&directory, &organ, &mut state)
        .await
        .unwrap();
    let uid = hello(&engine).await.uid;
    let source = std::fs::read_to_string(&path)
        .unwrap()
        .replace("@hello-my-world: 0", "@renamed-hello: 1.25 @hour")
        .replace("#task", "is #task");
    std::fs::write(&path, source).unwrap();
    let report = engine
        .file_sync_tick(&directory, &organ, &mut state)
        .await
        .unwrap();
    assert!(report.conflicts.is_empty(), "{:?}", report.conflicts);
    assert!(report.created.is_empty());
    let record = store::records::resolve(&engine.store.pool, "renamed-hello")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(record.uid, uid);
    assert_eq!(record.quantity.to_string(), "1.25");
    assert!(record.identity_predicate_uid.is_some());
    assert!(record.unit_uid.is_some());
    let source = std::fs::read_to_string(&path)
        .unwrap()
        .replace("1.25 @hour", "0")
        .replace("is #task", "#task");
    std::fs::write(&path, source).unwrap();
    let report = engine
        .file_sync_tick(&directory, &organ, &mut state)
        .await
        .unwrap();
    assert!(report.conflicts.is_empty(), "{:?}", report.conflicts);
    let record = store::records::get(&engine.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert!(record.unit_uid.is_none());
    assert!(record.identity_predicate_uid.is_none());
    assert!(record.quantity.is_zero());
    let report = engine
        .file_sync_tick(&directory, &organ, &mut state)
        .await
        .unwrap();
    assert!(report.updated_from_disk.is_empty());
    assert!(report.written_to_disk.is_empty());
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn deleting_a_record_in_lince_removes_its_declaration_without_resurrecting_it() {
    let directory = copy_user_file();
    let path = directory.join("test.lingua");
    let (engine, organ) = configured(&directory).await;
    let mut state = FileSyncState::new();
    engine
        .file_sync_tick(&directory, &organ, &mut state)
        .await
        .unwrap();
    let uid = hello(&engine).await.uid;
    engine
        .act(
            Action::DeleteRecord {
                target: uid.clone(),
            },
            None,
        )
        .await
        .unwrap();
    let report = engine
        .file_sync_tick(&directory, &organ, &mut state)
        .await
        .unwrap();
    assert!(report.conflicts.is_empty(), "{:?}", report.conflicts);
    assert!(std::fs::read_to_string(&path).unwrap().is_empty());
    engine
        .file_sync_tick(&directory, &organ, &mut FileSyncState::new())
        .await
        .unwrap();
    assert!(
        store::records::get(&engine.store.pool, &uid)
            .await
            .unwrap()
            .is_none()
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn renaming_the_file_preserves_the_declared_title_and_record() {
    let directory = copy_user_file();
    let (engine, organ) = configured(&directory).await;
    let mut state = FileSyncState::new();
    engine
        .file_sync_tick(&directory, &organ, &mut state)
        .await
        .unwrap();
    let uid = hello(&engine).await.uid;
    std::fs::rename(
        directory.join("test.lingua"),
        directory.join("renamed.lingua"),
    )
    .unwrap();
    for _ in 0..3 {
        let report = engine
            .file_sync_tick(&directory, &organ, &mut state)
            .await
            .unwrap();
        assert!(report.conflicts.is_empty(), "{:?}", report.conflicts);
        assert!(report.created.is_empty());
        assert!(report.deleted.is_empty());
    }
    assert_eq!(hello(&engine).await.uid, uid);
    assert_eq!(hello(&engine).await.head, "Hello");
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn duplicate_files_and_unresolved_references_do_not_create_partial_records() {
    let directory = copy_user_file();
    let path = directory.join("test.lingua");
    let (engine, organ) = configured(&directory).await;
    std::fs::copy(&path, directory.join("duplicate.lingua")).unwrap();
    let mut state = FileSyncState::new();
    let report = engine
        .file_sync_tick(&directory, &organ, &mut state)
        .await
        .unwrap();
    assert_eq!(report.conflicts.len(), 2);
    assert!(report.created.is_empty());
    assert!(
        store::records::resolve(&engine.store.pool, "hello-my-world")
            .await
            .unwrap()
            .is_none()
    );
    std::fs::remove_file(directory.join("duplicate.lingua")).unwrap();
    let source = std::fs::read_to_string(&path)
        .unwrap()
        .replace("#task", "#task, #related @missing");
    std::fs::write(&path, &source).unwrap();
    let report = engine
        .file_sync_tick(&directory, &organ, &mut state)
        .await
        .unwrap();
    assert_eq!(report.conflicts.len(), 1);
    assert!(report.created.is_empty());
    assert!(
        store::records::resolve(&engine.store.pool, "hello-my-world")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn separate_files_can_link_to_each_other_in_the_first_pass() {
    let directory = copy_user_file();
    let path = directory.join("test.lingua");
    let original = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        &path,
        original.replace("#task", "#task, #related @second-hello"),
    )
    .unwrap();
    std::fs::write(
        directory.join("second.lingua"),
        original
            .replace("@hello-my-world", "@second-hello")
            .replace("#task", "#task, #related @hello-my-world"),
    )
    .unwrap();
    let (engine, organ) = configured(&directory).await;
    let mut state = FileSyncState::new();
    let report = engine
        .file_sync_tick(&directory, &organ, &mut state)
        .await
        .unwrap();
    assert!(report.conflicts.is_empty(), "{:?}", report.conflicts);
    assert_eq!(report.created.len(), 2);
    let report = engine
        .file_sync_tick(&directory, &organ, &mut state)
        .await
        .unwrap();
    assert!(report.conflicts.is_empty(), "{:?}", report.conflicts);
    assert!(report.updated_from_disk.is_empty());
    assert!(report.written_to_disk.is_empty());
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn enabling_then_saving_the_user_directory_starts_the_live_watcher() {
    let path = user_file();
    let source = std::fs::read(&path).unwrap();
    let engine = Engine::open_memory().await.unwrap();
    engine
        .act(Action::SetFileSyncEnabled { enabled: true }, None)
        .await
        .unwrap();
    let engine = std::sync::Arc::new(engine);
    let supervisor = engine::file_sync::spawn_supervisor(engine.clone());
    engine
        .act(
            Action::ConfigureFileSync {
                protein: String::new(),
                path: path.parent().unwrap().to_string_lossy().into(),
                format: FileFormat::Lingua,
                enabled: true,
            },
            None,
        )
        .await
        .unwrap();
    let query = serde_json::from_value(serde_json::json!({"source":"record","where":[{"all":[{"slug_eq":"hello-my-world"},{"concept_in":"backlog"},{"concept_in":"task"}]}]})).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            if protein::execute(&engine.store, &query).await.unwrap().len() == 1 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(hello(&engine).await.head, "Hello");
    assert_eq!(hello(&engine).await.body, "Oieeee\n");
    engine
        .act(
            Action::ConfigureFileSync {
                protein: String::new(),
                path: String::new(),
                format: FileFormat::Lingua,
                enabled: false,
            },
            None,
        )
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    supervisor.abort();
    assert_eq!(std::fs::read(path).unwrap(), source);
}
