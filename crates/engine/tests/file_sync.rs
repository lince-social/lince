//! File Sync (blueprint: Sync/CRDT disk-organ projection): head = filename,
//! body = file content, disk wins on conflict, deletion is debounced, and a
//! disk edit reaches a live subscriber through the same Action path an app
//! edit uses.

use std::sync::Arc;

use engine::Engine;
use engine::actions::Action;
use engine::file_sync::{FileSyncState, spawn_configured_watchers};
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

    // The edit went through the real Action write path (`EditRecordText`),
    // so it fired an annotation fact on the fact_bus — the exact mechanism
    // that pushes live Protein subscriptions ("resend to sand").
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

    // First miss: not deleted yet (guards against an editor's atomic save).
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();
    assert!(
        store::records::get(&e.store.pool, &uid)
            .await
            .unwrap()
            .is_some(),
        "record survives a single missing tick"
    );
    // A mirror pass during the debounce window must NOT recreate the file.
    assert!(!dir.join("Scratch.md").exists());

    // Second consecutive miss: now it hard-deletes.
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
    // The record itself is untouched — only its file-sync projection left.
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

    // No `lince.file_sync` extension yet: nothing to spawn.
    let handles = spawn_configured_watchers(e.clone()).await.unwrap();
    assert_eq!(handles.len(), 0);

    // Present but disabled: still nothing.
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

    // Enabled with a path: one loop spawned for this organ.
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
