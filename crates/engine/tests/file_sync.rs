//! File Sync (blueprint: Sync/CRDT disk-organ projection): head = filename,
//! body = file content, disk wins on conflict, deletion is debounced, and a
//! disk edit reaches a live subscriber through the same Action path an app
//! edit uses.

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

#[tokio::test]
async fn supervisor_starts_and_stops_watch_loops_live_without_reboot() {
    let (e, organ) = cell_with_local_organ().await;
    let e = Arc::new(e);
    let dir = tmp_dir();

    let _supervisor = spawn_supervisor(e.clone());

    // Not configured yet: the seed pass finds nothing to spawn.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(std::fs::read_dir(&dir).unwrap().next().is_none());

    let uid = plain(&e, "Live", "content").await;

    // Enabling via a live SetExtension action (no reboot) must start the
    // watch loop and mirror the existing record to disk.
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

    // Disabling live must stop the watch loop: a disk edit afterward must
    // NOT flow back into the record.
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

/// Selection is `organ_uid` AND whatever the owner configured, in the SAME
/// Protein vocabulary every other filter uses (Ontology §12, C5).
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
                // Stored as text, which is what a text field writes.
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

/// A filter that will not parse is IGNORED rather than failing closed. Failing
/// closed here means mirroring NOTHING, which is a folder that silently
/// empties itself and no error to explain it.
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

// --- the `.lingua` format ------------------------------------------------
//
// A Record's Lingua state above its body, so a folder of files carries what a
// Record IS and what it links to — not only its text. The projection is
// generated from the store; the body half keeps flowing back exactly as it
// does for markdown.

/// Test setup only: the Concepts a projection names have to exist, and how
/// they came to exist is not what these tests are about.
async fn concept(e: &Engine, name: &str) {
    store::concepts::ensure(&e.store.pool, name)
        .await
        .expect("concept");
}

async fn use_lingua(e: &Engine, organ: &str) {
    e.act(
        Action::SetExtension {
            target: organ.to_string(),
            namespace: "lince.file_sync".into(),
            fds: serde_json::json!({
                "enabled": true,
                "path": "/unused-here",
                "formats": ["lingua"],
            }),
        },
        None,
    )
    .await
    .expect("configure");
}

#[tokio::test]
async fn a_lingua_file_carries_identity_concepts_links_and_quantity_above_its_body() {
    let (e, organ) = cell_with_local_organ().await;
    use_lingua(&e, &organ).await;
    let project = plain(&e, "Project A", "the project").await;
    let task = plain(&e, "Write the brief", "Body text.").await;
    for name in ["task", "chapter", "references"] {
        concept(&e, name).await;
    }
    for (predicate, object, quantity) in [
        ("task", None, None),
        ("chapter", None, Some("1")),
        ("references", Some(project.clone()), None),
    ] {
        e.act(
            Action::AssertRecord {
                subject: task.clone(),
                predicate: predicate.into(),
                object,
                quantity: quantity.map(str::to_string),
                unit: None,
            },
            None,
        )
        .await
        .expect("assert");
    }
    e.act(
        Action::SetIdentity {
            subject: task.clone(),
            predicate: Some("task".into()),
        },
        None,
    )
    .await
    .expect("identity");

    let dir = tmp_dir();
    let mut state = FileSyncState::new();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    let content = std::fs::read_to_string(dir.join("Write the brief.lingua")).unwrap();
    assert!(
        content.starts_with("---\nuid: "),
        "machine metadata first: {content}"
    );
    assert!(content.contains("\n@@task\n"), "identity is @@: {content}");
    assert!(
        content.contains("\n@chapter 1\n"),
        "an assertion quantity rides its line: {content}"
    );
    assert!(
        content.contains(&format!("\n@references [[Project A|{project}]]\n")),
        "a link carries the title for the reader and the uid for the machine: {content}"
    );
    assert!(
        content.ends_with("---\n\nBody text."),
        "and the body is below the metadata: {content}"
    );
    assert!(
        !dir.join("Write the brief.md").exists(),
        "one format per folder — never two writable files for one Record"
    );
}

/// The whole point of `@chapter 1`: ordering and hierarchy without anything
/// hardcoding which document comes first. It needed no schema change —
/// `record_assertion` already carries an exact quantity.
#[tokio::test]
async fn chapter_order_survives_a_round_trip_as_an_exact_decimal() {
    let (e, organ) = cell_with_local_organ().await;
    use_lingua(&e, &organ).await;
    concept(&e, "chapter").await;
    let one = plain(&e, "First", "a").await;
    for (subject, chapter) in [(one.clone(), "1"), (plain(&e, "Second", "b").await, "2")] {
        e.act(
            Action::AssertRecord {
                subject,
                predicate: "chapter".into(),
                object: None,
                quantity: Some(chapter.into()),
                unit: None,
            },
            None,
        )
        .await
        .expect("assert");
    }
    let dir = tmp_dir();
    let mut state = FileSyncState::new();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    assert!(
        std::fs::read_to_string(dir.join("First.lingua"))
            .unwrap()
            .contains("@chapter 1"),
    );
    assert!(
        std::fs::read_to_string(dir.join("Second.lingua"))
            .unwrap()
            .contains("@chapter 2"),
    );
    let _ = one;
}

#[tokio::test]
async fn a_body_edit_below_the_metadata_still_flows_back() {
    let (e, organ) = cell_with_local_organ().await;
    use_lingua(&e, &organ).await;
    let uid = plain(&e, "Note", "before").await;
    let dir = tmp_dir();
    let mut state = FileSyncState::new();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    let path = dir.join("Note.lingua");
    let on_disk = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, on_disk.replace("before", "after")).unwrap();
    let report = e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    assert_eq!(report.updated_from_disk, vec![uid.clone()]);
    assert!(report.conflicts.is_empty(), "a body edit is not a conflict");
    let row = store::records::get(&e.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.body, "after");
}

/// Editing the block retracts and asserts on somebody's behalf from a text
/// file, so it applies in full or not at all. `@urgent` is not a Concept
/// here, and a file must never invent meanings — so the OTHER lines in the
/// same block are not applied either, and the file is left exactly as typed.
#[tokio::test]
async fn an_edit_naming_an_unknown_concept_is_refused_and_neither_side_is_touched() {
    let (e, organ) = cell_with_local_organ().await;
    use_lingua(&e, &organ).await;
    let uid = plain(&e, "Note", "body").await;
    concept(&e, "task").await;
    e.act(
        Action::AssertRecord {
            subject: uid.clone(),
            predicate: "task".into(),
            object: None,
            quantity: None,
            unit: None,
        },
        None,
    )
    .await
    .expect("assert");
    let dir = tmp_dir();
    let mut state = FileSyncState::new();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    let path = dir.join("Note.lingua");
    let edited = std::fs::read_to_string(&path)
        .unwrap()
        .replace("@task", "@urgent");
    std::fs::write(&path, &edited).unwrap();
    let report = e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    assert_eq!(report.conflicts.len(), 1, "reported: {report:?}");
    assert!(report.updated_from_disk.is_empty());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        edited,
        "what they typed survives — it is not regenerated over"
    );
    let concepts = store::assertions::concepts_for_record(&e.store.pool, &uid)
        .await
        .unwrap();
    assert_eq!(concepts.len(), 1, "and the Record is unchanged");
}

/// The other half of the format, built 2026-08-16: editing the block of a
/// file Lince already manages is a real retract and assert. Everything the
/// creation path refuses is still refused (see the test above); what changes
/// is that a resolvable edit now lands instead of being reported.
#[tokio::test]
async fn editing_the_block_retracts_and_asserts_for_real() {
    let (e, organ) = cell_with_local_organ().await;
    use_lingua(&e, &organ).await;
    let uid = plain(&e, "Note", "body").await;
    for name in ["task", "urgent"] {
        concept(&e, name).await;
    }
    e.act(
        Action::AssertRecord {
            subject: uid.clone(),
            predicate: "task".into(),
            object: None,
            quantity: None,
            unit: None,
        },
        None,
    )
    .await
    .expect("assert");
    let dir = tmp_dir();
    let mut state = FileSyncState::new();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    let path = dir.join("Note.lingua");
    let edited = std::fs::read_to_string(&path)
        .unwrap()
        .replace("@task", "@urgent");
    std::fs::write(&path, &edited).unwrap();
    let report = e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    assert!(
        report.conflicts.is_empty(),
        "applied, not reported: {report:?}"
    );
    let urgent = store::concepts::resolve(&e.store.pool, "urgent")
        .await
        .unwrap()
        .unwrap();
    let concepts = store::assertions::concepts_for_record(&e.store.pool, &uid)
        .await
        .unwrap();
    assert_eq!(concepts, vec![urgent], "task retracted, urgent asserted");
}

/// The rule that is easy to get wrong: `assertions::assert` is idempotent on
/// `(subject, predicate, object)` and returns the existing row UNTOUCHED. So
/// a changed number has to be a retract AND an assert, or `@chapter 1`
/// silently survives being edited to `@chapter 3` while the tick reports
/// success.
#[tokio::test]
async fn changing_a_number_in_the_block_actually_changes_it() {
    let (e, organ) = cell_with_local_organ().await;
    use_lingua(&e, &organ).await;
    let uid = plain(&e, "Note", "body").await;
    concept(&e, "chapter").await;
    e.act(
        Action::AssertRecord {
            subject: uid.clone(),
            predicate: "chapter".into(),
            object: None,
            quantity: Some("1".into()),
            unit: None,
        },
        None,
    )
    .await
    .expect("assert");
    let dir = tmp_dir();
    let mut state = FileSyncState::new();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    let path = dir.join("Note.lingua");
    let edited = std::fs::read_to_string(&path)
        .unwrap()
        .replace("@chapter 1", "@chapter 3");
    std::fs::write(&path, &edited).unwrap();
    let report = e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();
    assert!(report.conflicts.is_empty(), "applied: {report:?}");

    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("@chapter 3"), "the store now says 3: {text}");
    assert!(!text.contains("@chapter 1"), "and not 1 as well: {text}");
}

/// The whole point of the write-back: an agent given a task edits one line in
/// a file and is really holding it, so another agent reading the folder can
/// see the work is taken without any coordination between them.
#[tokio::test]
async fn an_agent_assigns_itself_a_task_by_editing_the_file() {
    let (e, organ) = cell_with_local_organ().await;
    use_lingua(&e, &organ).await;
    let task = plain(&e, "Write the metadata block back", "").await;
    let claude = plain(&e, "claude", "an agent").await;
    for name in ["assigned-to", "wip"] {
        concept(&e, name).await;
    }
    let dir = tmp_dir();
    let mut state = FileSyncState::new();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    let path = dir.join("Write the metadata block back.lingua");
    let text = std::fs::read_to_string(&path).unwrap();
    let edited = text.replace(
        "---\n\n",
        &format!("@assigned-to [[claude|{claude}]]\n@wip\n---\n\n"),
    );
    assert_ne!(edited, text, "the prelude fence was found: {text}");
    std::fs::write(&path, &edited).unwrap();
    let report = e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    assert!(report.conflicts.is_empty(), "applied: {report:?}");
    let concepts = store::assertions::concepts_for_record(&e.store.pool, &task)
        .await
        .unwrap();
    let wip = store::concepts::resolve(&e.store.pool, "wip")
        .await
        .unwrap()
        .unwrap();
    assert!(concepts.contains(&wip), "concepts: {concepts:?}");
    let assertions = store::assertions::for_subjects(&e.store.pool, &[task.clone()])
        .await
        .unwrap();
    assert!(
        assertions
            .iter()
            .any(|a| a.predicate == "assigned-to"
                && a.object_uid.as_deref() == Some(claude.as_str())),
        "the link landed on the right Record: {assertions:?}"
    );
}

/// A file dropped into the folder by hand DOES get its metadata read — there
/// is nothing to retract and no existing state to clobber, which is what makes
/// creation the safe half.
#[tokio::test]
async fn a_new_hand_written_file_gets_its_concepts_and_links_applied() {
    let (e, organ) = cell_with_local_organ().await;
    use_lingua(&e, &organ).await;
    let project = plain(&e, "Project A", "the project").await;
    let dir = tmp_dir();
    let mut state = FileSyncState::new();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();
    // The Concepts have to exist already — see the next test.
    for name in ["task", "chapter", "references"] {
        concept(&e, name).await;
    }

    std::fs::write(
        dir.join("Hand written.lingua"),
        format!(
            "---\n@@task\n@chapter 3\n@references [[Project A|{project}]]\n---\n\nTyped by hand.\n"
        ),
    )
    .unwrap();
    let report = e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    assert_eq!(report.created.len(), 1, "created: {report:?}");
    let uid = report.created[0].clone();
    let row = store::records::get(&e.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.body, "Typed by hand.\n");
    assert!(
        row.identity_predicate_uid.is_some(),
        "@@ became the identity, not another tag"
    );
    let concepts = store::assertions::concepts_for_record(&e.store.pool, &uid)
        .await
        .unwrap();
    assert_eq!(concepts.len(), 3, "three assertions applied");
}

/// The property that makes a folder of `.lingua` files a shippable BUNDLE: it
/// is valid on its own, before anything in it has been adopted.
///
/// A link is `[[Title|uid]]` and a link without a uid is refused, so files that
/// reference each other have to agree on their uids in advance — which means a
/// hand-written file has to be allowed to carry the uid of the Record it is
/// going to become. Importing with no links and adding them in a second pass
/// was the alternative, and it means the folder is never self-contained.
#[tokio::test]
async fn hand_written_files_cross_link_by_uid_before_either_record_exists() {
    let (e, organ) = cell_with_local_organ().await;
    use_lingua(&e, &organ).await;
    for name in ["idea", "see-also"] {
        concept(&e, name).await;
    }
    let dir = tmp_dir();
    let mut state = FileSyncState::new();

    let needs = nucleus::new_uid("r");
    let apples = nucleus::new_uid("r");
    std::fs::write(
        dir.join("Needs.lingua"),
        format!(
            "---\nuid: {needs}\n@@idea\n@see-also [[Apples|{apples}]]\n---\n\nA need is negative.\n"
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("Apples.lingua"),
        format!("---\nuid: {apples}\n@@idea\n@see-also [[Needs|{needs}]]\n---\n\nOne apple, three shapes.\n"),
    )
    .unwrap();

    let report = e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();
    assert!(report.conflicts.is_empty(), "adopted: {report:?}");
    assert_eq!(report.created.len(), 2, "created: {report:?}");

    for uid in [&needs, &apples] {
        let row = store::records::get(&e.store.pool, uid).await.unwrap();
        assert!(row.is_some(), "{uid} took the uid the file gave it");
    }
    let assertions = store::assertions::for_subjects(&e.store.pool, &[needs.clone()])
        .await
        .unwrap();
    assert!(
        assertions
            .iter()
            .any(|a| a.predicate == "see-also" && a.object_uid.as_deref() == Some(apples.as_str())),
        "the link resolved both ways: {assertions:?}"
    );
}

/// A uid names ONE thing. Adopting or overwriting on a collision is the single
/// failure that cannot be undone afterwards, so it refuses instead.
#[tokio::test]
async fn a_file_claiming_a_taken_uid_is_refused_rather_than_merged() {
    let (e, organ) = cell_with_local_organ().await;
    use_lingua(&e, &organ).await;
    let existing = plain(&e, "Already here", "the original body").await;
    let dir = tmp_dir();
    let mut state = FileSyncState::new();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    std::fs::write(
        dir.join("Impostor.lingua"),
        format!("---\nuid: {existing}\n---\n\nsomething else entirely\n"),
    )
    .unwrap();
    let report = e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    assert_eq!(report.conflicts.len(), 1, "refused: {report:?}");
    assert!(report.created.is_empty());
    let row = store::records::get(&e.store.pool, &existing)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.body, "the original body", "the original is untouched");
    assert_eq!(row.head, "Already here");
}

/// A uid that is not shaped like one INSERTS perfectly happily — nothing
/// downstream parses a uid — and the damage shows up much later as a Record
/// that does not sort or compare like any other. So the shape is checked on
/// the way in, where the file that caused it can still be named.
#[tokio::test]
async fn a_malformed_uid_is_refused_at_the_door() {
    let (e, organ) = cell_with_local_organ().await;
    use_lingua(&e, &organ).await;
    let dir = tmp_dir();
    let mut state = FileSyncState::new();

    std::fs::write(dir.join("Bad.lingua"), "---\nuid: not-a-uid\n---\n\nbody\n").unwrap();
    let report = e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    assert_eq!(report.conflicts.len(), 1, "refused: {report:?}");
    assert!(report.created.is_empty(), "and no Record was made");
    assert!(
        store::records::get(&e.store.pool, "not-a-uid")
            .await
            .unwrap()
            .is_none(),
        "least of all under that uid"
    );
}

/// A file must never invent meanings or retarget a Record by name. An unknown
/// Concept refuses the whole file BEFORE the Record exists — validating
/// afterwards would leave a bare Record behind that nobody goes looking for.
#[tokio::test]
async fn an_unknown_concept_refuses_the_file_and_creates_nothing() {
    let (e, organ) = cell_with_local_organ().await;
    use_lingua(&e, &organ).await;
    let dir = tmp_dir();
    let mut state = FileSyncState::new();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    std::fs::write(dir.join("Bad.lingua"), "---\n@nonesuch\n---\n\nBody.\n").unwrap();
    let report = e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    assert!(report.created.is_empty(), "nothing created: {report:?}");
    assert_eq!(report.conflicts.len(), 1);
    assert!(
        report.conflicts[0].reason.contains("nonesuch"),
        "and it says which one: {:?}",
        report.conflicts[0]
    );
    assert!(
        dir.join("Bad.lingua").exists(),
        "the file is left for the person to fix, not deleted"
    );
}

/// Switching format leaves the previous format's files alone. They are a
/// person's text, and changing a setting is not consent to delete it.
#[tokio::test]
async fn files_from_a_previous_format_are_reported_rather_than_swept() {
    let (e, organ) = cell_with_local_organ().await;
    plain(&e, "Older note", "written as markdown").await;
    let dir = tmp_dir();
    let mut state = FileSyncState::new();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();
    assert!(dir.join("Older note.md").exists());

    use_lingua(&e, &organ).await;
    let mut fresh = FileSyncState::new();
    let report = e.file_sync_tick(&dir, &organ, &mut fresh).await.unwrap();

    assert!(dir.join("Older note.md").exists(), "not deleted");
    assert!(dir.join("Older note.lingua").exists(), "and re-mirrored");
    assert!(
        report
            .conflicts
            .iter()
            .any(|c| c.path.ends_with("Older note.md")),
        "the leftover is reported: {report:?}"
    );
}

/// Both shapes for one Record, and each format owns what it can express.
/// A Markdown file holds only a body, so a Markdown edit can only mean a body
/// edit; the `.lingua` beside it carries everything Markdown cannot say.
async fn use_both(e: &Engine, organ: &str) {
    e.act(
        Action::SetExtension {
            target: organ.to_string(),
            namespace: "lince.file_sync".into(),
            fds: serde_json::json!({
                "enabled": true,
                "path": "/unused-here",
                // Precedence order: lingua wins a contested body.
                "formats": ["lingua", "markdown"],
            }),
        },
        None,
    )
    .await
    .expect("configure");
}

#[tokio::test]
async fn one_record_can_be_mirrored_in_both_formats_at_once() {
    let (e, organ) = cell_with_local_organ().await;
    use_both(&e, &organ).await;
    concept(&e, "task").await;
    let uid = plain(&e, "Note", "Body text.").await;
    e.act(
        Action::AssertRecord {
            subject: uid.clone(),
            predicate: "task".into(),
            object: None,
            quantity: None,
            unit: None,
        },
        None,
    )
    .await
    .expect("assert");

    let dir = tmp_dir();
    let mut state = FileSyncState::new();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    assert_eq!(
        std::fs::read_to_string(dir.join("Note.md")).unwrap(),
        "Body text.",
        "markdown carries the body and nothing else"
    );
    let lingua = std::fs::read_to_string(dir.join("Note.lingua")).unwrap();
    assert!(
        lingua.contains("\n@task\n"),
        "and lingua carries the rest: {lingua}"
    );
    assert!(lingua.ends_with("---\n\nBody text."));
}

#[tokio::test]
async fn an_edit_to_either_file_reaches_the_record() {
    let (e, organ) = cell_with_local_organ().await;
    use_both(&e, &organ).await;
    let uid = plain(&e, "Note", "before").await;
    let dir = tmp_dir();
    let mut state = FileSyncState::new();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    std::fs::write(dir.join("Note.md"), "from markdown").unwrap();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();
    let row = store::records::get(&e.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.body, "from markdown");
    assert!(
        std::fs::read_to_string(dir.join("Note.lingua"))
            .unwrap()
            .ends_with("from markdown"),
        "and the other projection catches up"
    );

    let lingua = std::fs::read_to_string(dir.join("Note.lingua")).unwrap();
    std::fs::write(
        dir.join("Note.lingua"),
        lingua.replace("from markdown", "from lingua"),
    )
    .unwrap();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();
    let row = store::records::get(&e.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.body, "from lingua");
    assert_eq!(
        std::fs::read_to_string(dir.join("Note.md")).unwrap(),
        "from lingua"
    );
}

/// Both edited between two ticks, disagreeing. The configured ORDER decides,
/// not whichever the filesystem happened to hand back first.
#[tokio::test]
async fn the_first_listed_format_wins_a_contested_body() {
    let (e, organ) = cell_with_local_organ().await;
    use_both(&e, &organ).await;
    let uid = plain(&e, "Note", "before").await;
    let dir = tmp_dir();
    let mut state = FileSyncState::new();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    let lingua = std::fs::read_to_string(dir.join("Note.lingua")).unwrap();
    std::fs::write(dir.join("Note.md"), "markdown says this").unwrap();
    std::fs::write(
        dir.join("Note.lingua"),
        lingua.replace("before", "lingua says this"),
    )
    .unwrap();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    let row = store::records::get(&e.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.body, "lingua says this", "lingua is listed first");
}

/// Deleting ONE projection is not deleting the Record. Tidying a folder must
/// not destroy data; the mirror half simply puts the file back.
#[tokio::test]
async fn removing_one_format_s_file_does_not_delete_the_record() {
    let (e, organ) = cell_with_local_organ().await;
    use_both(&e, &organ).await;
    let uid = plain(&e, "Note", "body").await;
    let dir = tmp_dir();
    let mut state = FileSyncState::new();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    std::fs::remove_file(dir.join("Note.md")).unwrap();
    for _ in 0..3 {
        e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();
    }

    assert!(
        store::records::get(&e.store.pool, &uid)
            .await
            .unwrap()
            .is_some(),
        "the record survives — the .lingua file is still there"
    );
    assert!(
        dir.join("Note.md").exists(),
        "and the projection comes back"
    );
}

#[tokio::test]
async fn removing_every_file_still_deletes_the_record() {
    let (e, organ) = cell_with_local_organ().await;
    use_both(&e, &organ).await;
    let uid = plain(&e, "Note", "body").await;
    let dir = tmp_dir();
    let mut state = FileSyncState::new();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    std::fs::remove_file(dir.join("Note.md")).unwrap();
    std::fs::remove_file(dir.join("Note.lingua")).unwrap();
    for _ in 0..3 {
        e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();
    }

    assert!(
        store::records::get(&e.store.pool, &uid)
            .await
            .unwrap()
            .is_none(),
        "every projection gone means the Record was deleted"
    );
}

/// The level written in a file becomes TRUE in the database.
///
/// Not by assignment — the level is a fold of Ledger Facts — but by appending
/// the exact difference, so the number a person typed is what the Record
/// holds and the Ledger still explains how it got there.
#[tokio::test]
async fn a_quantity_written_in_a_lingua_file_becomes_true_in_the_database() {
    let (e, organ) = cell_with_local_organ().await;
    use_lingua(&e, &organ).await;
    concept(&e, "hour").await;
    let uid = plain(&e, "Timesheet", "hours worked").await;
    e.act(
        Action::SetQuantityExact {
            target: uid.clone(),
            amount: "2".into(),
        },
        None,
    )
    .await
    .expect("seed");

    let dir = tmp_dir();
    let mut state = FileSyncState::new();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();
    let path = dir.join("Timesheet.lingua");
    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert!(on_disk.contains("quantity: 2"), "rendered: {on_disk}");

    // Write a new level, exactly as a person or an agent would.
    std::fs::write(
        &path,
        on_disk.replace("quantity: 2", "quantity: 12.50 @hour"),
    )
    .unwrap();
    let report = e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    assert!(report.conflicts.is_empty(), "not a conflict: {report:?}");
    let row = store::records::get(&e.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.quantity.to_string(),
        "12.50",
        "the file's number is the Record's level, exactly"
    );
    assert!(row.unit_uid.is_some(), "and the unit came with it");

    // The Ledger still explains it: a fold, never an assignment.
    let facts = store::facts::for_record(&e.store.pool, &uid, 100)
        .await
        .expect("facts");
    let deltas: Vec<String> = facts.iter().map(|f| f.delta.to_string()).collect();
    assert!(
        deltas.contains(&"10.50".to_string()),
        "appended the DIFFERENCE from 2, not the number itself: {deltas:?}"
    );
    assert!(
        !deltas.contains(&"12.50".to_string()),
        "a level is a fold of Facts, never an assignment: {deltas:?}"
    );
}

/// A level that is not a number leaves the Record's level alone and says so,
/// rather than resolving to zero.
#[tokio::test]
async fn an_unreadable_quantity_is_reported_and_changes_nothing() {
    let (e, organ) = cell_with_local_organ().await;
    use_lingua(&e, &organ).await;
    let uid = plain(&e, "Thing", "body").await;
    e.act(
        Action::SetQuantityExact {
            target: uid.clone(),
            amount: "5".into(),
        },
        None,
    )
    .await
    .expect("seed");
    let dir = tmp_dir();
    let mut state = FileSyncState::new();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    let path = dir.join("Thing.lingua");
    let on_disk = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, on_disk.replace("quantity: 5", "quantity: lots")).unwrap();
    let report = e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    assert_eq!(report.conflicts.len(), 1, "reported: {report:?}");
    let row = store::records::get(&e.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.quantity.to_string(), "5", "and the level is untouched");
}

/// **A folder outlives the process that wrote it.**
///
/// The path<->uid memory lives in `FileSyncState`, which starts empty on every
/// boot, so after a restart every file in the folder is a path this Cell has
/// never seen — while the Records they belong to are still right there. Read
/// as new, each one asked to create a Record under a uid that already exists
/// and was refused, which left the folder stuck in a loop nobody could see:
/// one conflict per file per tick, no edit imported, and the mirror half
/// skipping the very files it had written. Editing a note while the Cell was
/// closed then did nothing at all — the symptom that found this.
#[tokio::test]
async fn a_restart_picks_the_folder_back_up_instead_of_refusing_every_file() {
    let (e, organ) = cell_with_local_organ().await;
    use_lingua(&e, &organ).await;
    let uid = plain(&e, "Proximity", "the original body").await;
    let dir = tmp_dir();

    let mut state = FileSyncState::new();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();
    let path = dir.join("Proximity.lingua");
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains("the original body"));

    // The Cell stops, someone edits the note in their editor, the Cell comes
    // back: a new process, a new state, the same folder.
    std::fs::write(
        &path,
        written.replace("the original body", "edited while closed"),
    )
    .unwrap();
    let mut after_restart = FileSyncState::new();
    let report = e
        .file_sync_tick(&dir, &organ, &mut after_restart)
        .await
        .unwrap();

    assert!(
        report.conflicts.is_empty(),
        "the folder is picked back up: {:?}",
        report.conflicts
    );
    assert!(
        report.created.is_empty(),
        "a file already adopted is not adopted twice"
    );
    let row = store::records::get(&e.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.body, "edited while closed",
        "disk wins, as it does while running"
    );
    assert_eq!(
        store::records::list_all(&e.store.pool).await.unwrap().len(),
        // The Organ and the Cell are Records too, and neither is mirrored.
        3,
        "no duplicate Record was created for a file that already had one",
    );
}

/// **A filter says what to mirror. It never says what to delete.**
///
/// With a narrowing filter configured, dropping a plain file into the folder
/// adopted it as a Record carrying no concepts — which the filter did not
/// select, so the mirror did not want its file and the sweep deleted it on the
/// same tick that had just read it. A note written by hand disappeared, and
/// the Record left behind had no file to find it by. Now the file is kept and
/// the reason is reported.
#[tokio::test]
async fn a_file_the_filter_excludes_is_kept_and_reported_never_swept() {
    let (e, organ) = cell_with_local_organ().await;
    concept(&e, "instinct").await;
    let dir = tmp_dir();
    e.act(
        Action::SetExtension {
            target: organ.clone(),
            namespace: "lince.file_sync".into(),
            fds: serde_json::json!({
                "enabled": true,
                "path": dir.display().to_string(),
                "formats": ["lingua"],
                "filter": "{\"concept_in\":\"instinct\"}",
            }),
        },
        None,
    )
    .await
    .expect("configure");

    let path = dir.join("Thoughts.lingua");
    std::fs::write(&path, "a thought nobody has filed yet").unwrap();
    let mut state = FileSyncState::new();
    let report = e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    assert!(path.exists(), "the note somebody wrote is still on disk");
    assert_eq!(report.created.len(), 1, "and it was adopted as a Record");
    assert!(
        report
            .conflicts
            .iter()
            .any(|c| c.reason.contains("not selected by this folder's filter")),
        "the folder says why it is not syncing: {:?}",
        report.conflicts,
    );

    // And again: a second tick must not read the kept file as new and adopt
    // the same text a second time.
    let report = e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();
    assert!(path.exists());
    assert!(
        report.created.is_empty(),
        "no duplicate Record on the next tick"
    );
    let thoughts: Vec<_> = store::records::list_all(&e.store.pool)
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.head == "Thoughts")
        .collect();
    assert_eq!(thoughts.len(), 1, "one file, one Record");
}

/// **The uid is the identity; the filename is a projection of the head.**
///
/// Renaming a file therefore renames the Record. It used to read as one Record
/// going missing and another asking for a uid already taken — a refused
/// adoption, and then a debounced delete of the Record the renamed file was
/// still describing. Whether the rename happened with the Cell running or while
/// it was closed makes no difference to what the folder means.
#[tokio::test]
async fn renaming_a_file_renames_the_record_it_carries_the_uid_of() {
    let (e, organ) = cell_with_local_organ().await;
    use_lingua(&e, &organ).await;
    let uid = plain(&e, "Later, still Rule", "what a rule will grow into").await;
    let dir = tmp_dir();
    let mut state = FileSyncState::new();
    e.file_sync_tick(&dir, &organ, &mut state).await.unwrap();

    let old = dir.join("Later, still Rule.lingua");
    let text = std::fs::read_to_string(&old).unwrap();
    std::fs::rename(&old, dir.join("Rule.lingua")).unwrap();

    // Closed at the time, so nothing is remembered — the hard case.
    let mut after_restart = FileSyncState::new();
    let report = e
        .file_sync_tick(&dir, &organ, &mut after_restart)
        .await
        .unwrap();
    assert!(report.conflicts.is_empty(), "{:?}", report.conflicts);
    assert!(report.created.is_empty(), "a rename creates nothing");
    let row = store::records::get(&e.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.head, "Rule", "the Record took the new name");
    assert_eq!(
        row.body, "what a rule will grow into",
        "and kept everything else"
    );

    // Two more ticks: the debounce that used to fire on the vanished path must
    // not delete the Record, and the file keeps its new name.
    for _ in 0..2 {
        e.file_sync_tick(&dir, &organ, &mut after_restart)
            .await
            .unwrap();
    }
    assert!(dir.join("Rule.lingua").exists(), "the renamed file stays");
    assert!(!old.exists(), "and the old name is not resurrected");
    let row = store::records::get(&e.store.pool, &uid)
        .await
        .unwrap()
        .expect("the Record survived the rename");
    assert_eq!(row.head, "Rule");
    assert_eq!(
        std::fs::read_to_string(dir.join("Rule.lingua")).unwrap(),
        text
    );
}
