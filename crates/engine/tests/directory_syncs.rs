use engine::{
    Engine,
    actions::Action,
    file_sync::{DirectorySync, FileFormat, Request, spawn_supervisor},
    record_creation::Draft,
};
use serde_json::json;
use std::{path::Path, sync::Arc, time::Duration};

async fn save(engine: &Engine, uid: &str, path: &Path, format: FileFormat, enabled: bool) {
    engine
        .act(
            Action::DirectorySync {
                request: Request::Save {
                    settings: DirectorySync {
                        uid: uid.into(),
                        name: uid.into(),
                        path: path.to_str().unwrap().into(),
                        protein: None,
                        format,
                        enabled,
                    },
                    create: true,
                },
            },
            None,
        )
        .await
        .unwrap();
}

async fn write_record(engine: &Engine, title: &str, origin: Option<&str>) -> String {
    engine
        .act(
            Action::CreateRecordDraft {
                draft: Draft {
                    head: title.into(),
                    body: "Written in Lince.".into(),
                    file_origin: origin.map(str::to_owned),
                    ..Default::default()
                },
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap()
}

async fn wait_for(mut predicate: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(14), async {
        while !predicate() {
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
    })
    .await
    .expect("the background sync reaches the expected state");
}

async fn origins(engine: &Engine, origin: &str) -> Vec<String> {
    let query = serde_json::from_value(json!({
        "source": "record", "where": [{"file_origin_eq": origin}], "fields": ["uid"]
    }))
    .unwrap();
    protein::select_records(&engine.store, &query, None)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.uid)
        .collect()
}

#[tokio::test]
async fn independent_folders_keep_edits_deletes_and_origins_separate() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let folders = tempfile::tempdir().unwrap();
    let personal = folders.path().join("Personal");
    let institute = folders.path().join("Institute");
    let nested = institute.join("subject");
    std::fs::create_dir_all(&nested).unwrap();
    let source = nested.join("Topics.lingua");
    let uid = nucleus::new_uid("r");
    let text = format!("Topics (@topics: 0) {{ {uid}\nWritten on disk.\n}} {uid}\n");
    std::fs::write(&source, &text).unwrap();

    engine
        .act(
            Action::ConfigureFileSync {
                protein: String::new(),
                path: personal.to_str().unwrap().into(),
                format: FileFormat::Lingua,
                enabled: true,
            },
            None,
        )
        .await
        .unwrap();
    save(&engine, "institute", &institute, FileFormat::Lingua, true).await;
    let personal_uid = write_record(&engine, "Personal note", None).await;
    let institute_uid = write_record(&engine, "Institute note", Some("institute")).await;
    let worker = spawn_supervisor(engine.clone());

    let personal_file = personal.join("Personal note.lingua");
    let institute_file = institute.join("Institute note.lingua");
    wait_for(|| personal_file.is_file() && institute_file.is_file()).await;
    assert_eq!(std::fs::read_to_string(&source).unwrap(), text);
    assert!(!personal.join("Institute note.lingua").exists());
    assert!(!institute.join("Personal note.lingua").exists());
    assert!(!personal.join("Topics.lingua").exists());
    let institute_records = origins(&engine, "institute").await;
    assert_eq!(institute_records.len(), 2);
    assert!(institute_records.contains(&uid));
    assert!(institute_records.contains(&institute_uid));
    assert!(origins(&engine, "personal").await.contains(&personal_uid));

    std::fs::write(&source, text.replace("Written on disk.", "Edited on disk.")).unwrap();
    engine
        .act(
            Action::EditRecordText {
                target: institute_uid.clone(),
                head: None,
                body: Some("Edited in Lince.".into()),
            },
            None,
        )
        .await
        .unwrap();
    wait_for(|| {
        std::fs::read_to_string(&institute_file).is_ok_and(|text| text.contains("Edited in Lince."))
    })
    .await;
    assert_eq!(
        store::records::get(&engine.store.pool, &uid)
            .await
            .unwrap()
            .unwrap()
            .body,
        "Edited on disk.\n"
    );

    std::fs::remove_file(&source).unwrap();
    tokio::time::timeout(Duration::from_secs(14), async {
        while store::records::get(&engine.store.pool, &uid)
            .await
            .unwrap()
            .is_some()
        {
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
    })
    .await
    .unwrap();
    engine
        .act(
            Action::DeleteRecord {
                target: institute_uid,
            },
            None,
        )
        .await
        .unwrap();
    wait_for(|| {
        !std::fs::read_to_string(&institute_file).is_ok_and(|text| text.contains("Institute note"))
    })
    .await;

    engine
        .act(
            Action::DirectorySync {
                request: Request::SetEnabled {
                    uid: "institute".into(),
                    enabled: false,
                },
            },
            None,
        )
        .await
        .unwrap();
    write_record(&engine, "Another Institute note", Some("institute")).await;
    write_record(&engine, "Another personal note", None).await;
    wait_for(|| personal.join("Another personal note.lingua").is_file()).await;
    assert!(!institute.join("Another Institute note.lingua").exists());
    assert!(!personal.join("Another Institute note.lingua").exists());

    engine
        .act(
            Action::DirectorySync {
                request: Request::Remove {
                    uid: "institute".into(),
                },
            },
            None,
        )
        .await
        .unwrap();
    assert!(engine.directory_sync_statuses().await.unwrap().is_empty());
    assert_eq!(origins(&engine, "institute").await.len(), 1);
    worker.abort();
}

#[tokio::test]
async fn editing_an_origin_requires_an_existing_sync_and_failed_creation_is_atomic() {
    let engine = Engine::open_memory().await.unwrap();
    let draft = Draft {
        head: "Draft".into(),
        file_origin: Some("missing".into()),
        ..Default::default()
    };
    let uid = draft.uid.clone();
    assert!(
        engine
            .act(Action::CreateRecordDraft { draft }, None)
            .await
            .is_err()
    );
    assert!(
        store::records::get(&engine.store.pool, &uid)
            .await
            .unwrap()
            .is_none()
    );
}
