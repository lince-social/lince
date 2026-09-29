use engine::{
    Engine,
    actions::Action,
    area_transition::{RecordChanges, TransitionPreview},
};
use lince_mobile::{
    navigation::{Navigation, Page},
    record,
    storage::{self, Saved},
};

async fn fixture() -> (Engine, String) {
    let engine = Engine::open_memory().await.unwrap();
    let uid = engine
        .act(
            Action::CreateRecordDraft {
                draft: engine::record_creation::Draft {
                    head: "Task".into(),
                    body: "Original".into(),
                    ..Default::default()
                },
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    (engine, uid)
}

#[test]
fn back_closes_menu_before_leaving_record_and_scope_reset_removes_history() {
    let mut navigation = Navigation::default();
    navigation.open(Page::Kanban);
    navigation.open(Page::Record("r_task".into()));
    navigation.menu_open = true;
    assert!(navigation.back());
    assert_eq!(navigation.current, Page::Record("r_task".into()));
    assert!(navigation.back());
    assert_eq!(navigation.current, Page::Kanban);
    navigation.reset();
    assert_eq!(navigation.current, Page::Records);
    assert!(!navigation.back());
}

#[test]
fn private_drafts_restore_only_for_their_organ() {
    let directory = tempfile::tempdir().unwrap();
    let mut state = Saved {
        organ: "organ-a".into(),
        ..Default::default()
    };
    state.drafts.insert("r_one/body".into(), "ação 👩‍💻".into());
    storage::write(directory.path(), &state).unwrap();
    assert!(
        storage::read(directory.path(), "organ-b")
            .unwrap()
            .is_none()
    );
    let restored = storage::read(directory.path(), "organ-a").unwrap().unwrap();
    assert_eq!(restored.drafts["r_one/body"], "ação 👩‍💻");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(directory.path().join("mobile-drafts.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    std::fs::write(directory.path().join("mobile-drafts.json"), "broken").unwrap();
    assert!(storage::read(directory.path(), "organ-a").is_err());
    assert_eq!(
        std::fs::read_to_string(directory.path().join("mobile-drafts.json")).unwrap(),
        "broken"
    );
}

#[tokio::test]
async fn mobile_record_sorts_keep_equal_titles_in_stable_order() {
    let (engine, first) = fixture().await;
    let second = engine
        .act(
            Action::CreateRecordDraft {
                draft: engine::record_creation::Draft {
                    head: "Task".into(),
                    ..Default::default()
                },
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let mut expected = vec![first.clone(), second.clone()];
    expected.sort();
    for uid in [&first, &second] {
        engine
            .act(record::change(uid, "quantity", "-2", None).unwrap(), None)
            .await
            .unwrap();
    }
    for (label, field) in record::SORTS {
        let mut query = record::query(None, 100, "Task");
        query.order = vec![protein::Order::Asc(field.into())];
        query.filter.push(protein::Predicate::QuantityLt(
            nucleus::DecimalValue::parse_inferred("0").unwrap(),
        ));
        let query = lince_interface::queries::ProteinDraft::from_protein(
            label.into(),
            String::new(),
            query,
        )
        .compile()
        .unwrap();
        let rows = protein::execute(&engine.store, &query).await.unwrap();
        let actual: Vec<_> = rows
            .iter()
            .map(|row| row["uid"].as_str().unwrap())
            .collect();
        if field == "created_at" {
            assert_eq!(actual, [&first, &second], "{label}");
        } else {
            assert_eq!(actual, expected, "{label}");
        }
    }
}

#[tokio::test]
async fn mobile_text_change_merges_concurrent_desktop_typing_and_survives_replay() {
    let (engine, uid) = fixture().await;
    let snapshot = engine.collab_snapshot(&uid).await.unwrap();
    let mobile = record::change(&uid, "body", "Original mobile", Some(&snapshot)).unwrap();
    let desktop = record::change(&uid, "body", "Desktop Original", Some(&snapshot)).unwrap();
    engine.act(desktop, None).await.unwrap();
    engine.act(mobile.clone(), None).await.unwrap();
    engine.act(mobile, None).await.unwrap();
    let rows = protein::execute(&engine.store, &record::query(Some(&uid), 1, ""))
        .await
        .unwrap();
    assert_eq!(rows[0]["body"], "Desktop Original mobile");
    assert_eq!(rows[0]["head"], "Task");
}

#[tokio::test]
async fn subsequent_mobile_saves_do_not_duplicate_the_previous_edit() {
    let (engine, uid) = fixture().await;
    let original = engine.collab_snapshot(&uid).await.unwrap();
    let (first, snapshot) =
        record::prepare_change(&uid, "body", "Original one", Some(&original)).unwrap();
    engine.act(first, None).await.unwrap();
    let remote = record::change(&uid, "body", "Remote Original", Some(&original)).unwrap();
    engine.act(remote, None).await.unwrap();
    let second = record::change(&uid, "body", "Original one two", snapshot.as_deref()).unwrap();
    engine.act(second, None).await.unwrap();
    let rows = protein::execute(&engine.store, &record::query(Some(&uid), 1, ""))
        .await
        .unwrap();
    assert_eq!(rows[0]["body"], "Remote Original one two");
}

#[tokio::test]
async fn interrupted_record_save_replays_the_same_operation_after_restart() {
    let (engine, uid) = fixture().await;
    let original = engine.collab_snapshot(&uid).await.unwrap();
    let (action, snapshot) =
        record::prepare_change(&uid, "body", "Original saved", Some(&original)).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut saved = Saved {
        organ: "organ".into(),
        ..Default::default()
    };
    saved.outbox.insert(
        format!("{uid}/body"),
        record::Prepared {
            action: action.clone(),
            snapshot,
            submitted: "Original saved".into(),
        },
    );
    storage::write(directory.path(), &saved).unwrap();
    engine.act(action, None).await.unwrap();
    let recovered = storage::read(directory.path(), "organ").unwrap().unwrap();
    engine
        .act(
            recovered.outbox[&format!("{uid}/body")].action.clone(),
            None,
        )
        .await
        .unwrap();
    let rows = protein::execute(&engine.store, &record::query(Some(&uid), 1, ""))
        .await
        .unwrap();
    assert_eq!(rows[0]["body"], "Original saved");
}

#[tokio::test]
async fn new_tasks_match_the_kanban_filter_and_start_in_backlog() {
    let engine = Engine::open_memory().await.unwrap();
    let concept = engine
        .act(
            Action::CreateConcept {
                lingua: "g_local".into(),
                name: "task".into(),
                parents: vec![],
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let created = engine
        .act(
            Action::CreateRecordDraft {
                draft: record::new_draft(Some(concept)),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let mut query = record::query(None, 50, "");
    query
        .filter
        .push(protein::Predicate::ConceptIn("task".into()));
    let rows = protein::execute(&engine.store, &query).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["uid"], created);
    assert_eq!(rows[0]["quantity"], "0");
}

#[tokio::test]
async fn quantity_changes_remain_exact_and_protein_search_and_delete_use_existing_backend() {
    let (engine, uid) = fixture().await;
    engine
        .act(
            record::change(&uid, "quantity", "0.123456789012345678", None).unwrap(),
            None,
        )
        .await
        .unwrap();
    let rows = protein::execute(&engine.store, &record::query(None, 50, "Task"))
        .await
        .unwrap();
    let row = rows.iter().find(|row| row["uid"] == uid).unwrap();
    assert_eq!(row["quantity"], "0.123456789012345678");
    assert!(record::change(&uid, "quantity", "NaN", None).is_err());
    assert!(record::change(&uid, "created_at", "anything", None).is_err());
    engine
        .act(
            Action::DeleteRecord {
                target: uid.clone(),
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        protein::execute(&engine.store, &record::query(Some(&uid), 1, ""))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn completing_a_negative_record_removes_it_from_the_protein_view_once() {
    let (engine, uid) = fixture().await;
    engine
        .act(record::change(&uid, "quantity", "-2", None).unwrap(), None)
        .await
        .unwrap();
    let mut query = record::query(None, 50, "Task");
    query.filter.push(protein::Predicate::QuantityLt(
        nucleus::DecimalValue::parse_inferred("0").unwrap(),
    ));
    assert_eq!(
        protein::execute(&engine.store, &query).await.unwrap().len(),
        1
    );
    let completion = record::change(&uid, "quantity", "0", None).unwrap();
    engine.act(completion.clone(), None).await.unwrap();
    engine.act(completion, None).await.unwrap();
    assert!(
        protein::execute(&engine.store, &query)
            .await
            .unwrap()
            .is_empty()
    );
    let unfiltered = protein::execute(&engine.store, &record::query(Some(&uid), 1, ""))
        .await
        .unwrap();
    assert_eq!(unfiltered[0]["quantity"], "0");
}

#[tokio::test]
async fn kanban_preview_cannot_overwrite_a_newer_quantity() {
    let (engine, uid) = fixture().await;
    let result = engine
        .act(
            Action::PreviewAreaTransition {
                target: uid.clone(),
                changes: RecordChanges {
                    quantity: Some("-3".into()),
                    ..Default::default()
                },
                constraints: Default::default(),
            },
            None,
        )
        .await
        .unwrap();
    let preview: TransitionPreview = serde_json::from_value(result.data.unwrap()).unwrap();
    engine
        .act(record::change(&uid, "quantity", "1", None).unwrap(), None)
        .await
        .unwrap();
    assert!(
        engine
            .act(
                Action::ApplyAreaTransition {
                    request_id: nucleus::new_uid("move"),
                    preview
                },
                None
            )
            .await
            .is_err()
    );
    let rows = protein::execute(&engine.store, &record::query(Some(&uid), 1, ""))
        .await
        .unwrap();
    assert_eq!(rows[0]["quantity"], "1");
}
