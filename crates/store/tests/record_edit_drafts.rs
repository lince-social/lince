use store::record_edit_drafts::{self as drafts, PendingRequest, Write};

fn initial(uid: &str) -> Write {
    Write {
        expected: None,
        generation: nucleus::new_uid("ed"),
        checkpoint: Some("checkpoint".into()),
        pending: vec!["first".into()],
        requests: vec![PendingRequest {
            uid: "first".into(),
            payload: serde_json::json!({"id":"first","record_uid":uid,"value":"é新"}).to_string(),
        }],
        updates: vec![b"first delta".to_vec()],
    }
}

#[tokio::test]
async fn file_backed_append_acknowledgement_and_reopen_keep_payloads_and_history() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        directory.path().join("recovery.sqlite").display()
    );
    let store = store::Store::open_durable(&url).await.unwrap();
    let uid = nucleus::new_uid("r");
    let source = "local\nowner";
    let write = initial(&uid);
    let first = drafts::save(&store.pool, source, &uid, &write, None)
        .await
        .unwrap();
    assert_eq!(
        drafts::save(&store.pool, source, &uid, &write, None)
            .await
            .unwrap(),
        first
    );
    let update = Write {
        expected: Some(first.stamp.clone()),
        generation: first.stamp.generation.clone(),
        checkpoint: None,
        pending: vec!["first".into(), "second".into()],
        requests: vec![PendingRequest {
            uid: "second".into(),
            payload: "{\"id\":\"second\"}".into(),
        }],
        updates: vec![b"second delta".to_vec()],
    };
    let second = drafts::save(&store.pool, source, &uid, &update, None)
        .await
        .unwrap();
    let acknowledgement = Write {
        expected: Some(second.stamp.clone()),
        generation: second.stamp.generation.clone(),
        checkpoint: None,
        pending: vec!["second".into()],
        requests: vec![],
        updates: vec![],
    };
    let acknowledged = drafts::save(&store.pool, source, &uid, &acknowledgement, None)
        .await
        .unwrap();
    assert_eq!(acknowledged.update_count, 2);
    assert_eq!(
        acknowledged.checkpoint_bytes,
        write.checkpoint.as_ref().unwrap().len()
    );
    store.pool.close().await;
    let reopened = store::Store::open_existing_durable(&url).await.unwrap();
    let restored = drafts::replay(&reopened.pool, source, &uid, &acknowledged.stamp)
        .await
        .unwrap();
    assert_eq!(restored.checkpoint, "checkpoint");
    assert_eq!(
        restored.updates,
        vec![b"first delta".to_vec(), b"second delta".to_vec()]
    );
    assert_eq!(restored.requests.len(), 1);
    assert_eq!(restored.requests[0].uid, "second");
    assert!(
        drafts::metadata(&reopened.pool, "local\nanother-owner", &uid)
            .await
            .unwrap()
            .is_none()
    );
    reopened.pool.close().await;
}

#[tokio::test]
async fn a_long_offline_backlog_survives_reopen_and_acknowledgements_make_progress() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        directory.path().join("offline-backlog.sqlite").display()
    );
    let store = store::Store::open_durable(&url).await.unwrap();
    let uid = nucleus::new_uid("r");
    let ids: Vec<_> = (0..1025).map(|_| nucleus::new_uid("op")).collect();
    let requests: Vec<_> = ids
        .iter()
        .enumerate()
        .map(|(index, id)| PendingRequest {
            uid: id.clone(),
            payload: serde_json::json!({"id":id,"record_uid":uid,"value":format!("é新{index}")})
                .to_string(),
        })
        .collect();
    let write = Write {
        expected: None,
        generation: nucleus::new_uid("ed"),
        checkpoint: Some("offline checkpoint".into()),
        pending: ids.clone(),
        requests: requests.clone(),
        updates: vec![b"offline text updates".to_vec()],
    };
    let metadata = drafts::save(&store.pool, "local", &uid, &write, None)
        .await
        .unwrap();
    store.pool.close().await;
    let store = store::Store::open_existing_durable(&url).await.unwrap();
    let restored = drafts::replay(&store.pool, "local", &uid, &metadata.stamp)
        .await
        .unwrap();
    assert_eq!(restored.metadata.pending, ids);
    assert!(
        restored
            .requests
            .iter()
            .zip(&requests)
            .all(|(actual, expected)| actual.uid == expected.uid
                && actual.payload == expected.payload)
    );
    let acknowledgement = Write {
        expected: Some(metadata.stamp.clone()),
        generation: metadata.stamp.generation.clone(),
        checkpoint: None,
        pending: ids[1..].to_vec(),
        requests: Vec::new(),
        updates: Vec::new(),
    };
    let acknowledged = drafts::save(&store.pool, "local", &uid, &acknowledgement, None)
        .await
        .unwrap();
    assert!(acknowledged.request_bytes < metadata.request_bytes);
    assert_eq!(acknowledged.update_count, metadata.update_count);
    store.pool.close().await;
    let store = store::Store::open_existing_durable(&url).await.unwrap();
    let restored = drafts::replay(&store.pool, "local", &uid, &acknowledged.stamp)
        .await
        .unwrap();
    assert_eq!(restored.metadata.pending, ids[1..]);
    assert_eq!(restored.requests.len(), 1024);
    assert_eq!(restored.requests[0].uid, ids[1]);
    assert_eq!(restored.checkpoint, "offline checkpoint");
    store.pool.close().await;
}

#[tokio::test]
async fn stale_writes_payload_reuse_and_cleanup_cannot_destroy_newer_intent() {
    let store = store::Store::open_memory().await.unwrap();
    let uid = nucleus::new_uid("r");
    let write = initial(&uid);
    let first = drafts::save(&store.pool, "local", &uid, &write, None)
        .await
        .unwrap();
    let mut changed = initial(&uid);
    changed.expected = Some(first.stamp.clone());
    changed.generation = first.stamp.generation.clone();
    changed.checkpoint = None;
    changed.requests[0].payload = "{\"changed\":true}".into();
    assert!(
        drafts::save(&store.pool, "local", &uid, &changed, None)
            .await
            .is_err()
    );
    assert_eq!(
        drafts::metadata(&store.pool, "local", &uid)
            .await
            .unwrap()
            .unwrap(),
        first
    );
    let next = Write {
        expected: Some(first.stamp.clone()),
        generation: first.stamp.generation.clone(),
        checkpoint: None,
        pending: vec!["first".into()],
        requests: vec![],
        updates: vec![b"next".to_vec()],
    };
    let second = drafts::save(&store.pool, "local", &uid, &next, None)
        .await
        .unwrap();
    let stale = Write {
        updates: vec![b"stale".to_vec()],
        ..next
    };
    assert!(
        drafts::save(&store.pool, "local", &uid, &stale, None)
            .await
            .is_err()
    );
    assert!(
        !drafts::retire(&store.pool, "local", &uid, &first.stamp)
            .await
            .unwrap()
    );
    assert!(
        !drafts::retire(&store.pool, "local", &uid, &second.stamp)
            .await
            .unwrap()
    );
    assert_eq!(
        drafts::metadata(&store.pool, "local", &uid)
            .await
            .unwrap()
            .unwrap(),
        second
    );
}

#[tokio::test]
async fn compaction_and_failed_append_are_atomic_without_publishing_record_changes() {
    let store = store::Store::open_memory().await.unwrap();
    let uid = nucleus::new_uid("r");
    let commit: i64 = store::sqlx::query_scalar("SELECT value FROM commit_sequence WHERE id = 1")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    let write = initial(&uid);
    let first = drafts::save(&store.pool, "local", &uid, &write, None)
        .await
        .unwrap();
    store::sqlx::query("CREATE TRIGGER fail_recovery_update BEFORE INSERT ON record_edit_update BEGIN SELECT RAISE(ABORT,'held recovery update'); END").execute(&store.pool).await.unwrap();
    let next = Write {
        expected: Some(first.stamp.clone()),
        generation: first.stamp.generation.clone(),
        checkpoint: None,
        pending: vec![],
        requests: vec![],
        updates: vec![b"next".to_vec()],
    };
    assert!(
        drafts::save(&store.pool, "local", &uid, &next, None)
            .await
            .is_err()
    );
    assert_eq!(
        drafts::metadata(&store.pool, "local", &uid)
            .await
            .unwrap()
            .unwrap(),
        first
    );
    store::sqlx::query("DROP TRIGGER fail_recovery_update")
        .execute(&store.pool)
        .await
        .unwrap();
    let compacted = drafts::save(&store.pool, "local", &uid, &next, Some("compacted"))
        .await
        .unwrap();
    assert!(compacted.pending.is_empty());
    assert_eq!(compacted.update_count, 0);
    assert_eq!(compacted.request_bytes, 0);
    let replay = drafts::replay(&store.pool, "local", &uid, &compacted.stamp)
        .await
        .unwrap();
    assert_eq!(replay.checkpoint, "compacted");
    assert!(replay.updates.is_empty() && replay.requests.is_empty());
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT value FROM commit_sequence WHERE id = 1")
            .fetch_one(&store.pool)
            .await
            .unwrap(),
        commit
    );
    assert!(
        !drafts::retire(&store.pool, "local", &uid, &first.stamp)
            .await
            .unwrap()
    );
    assert!(
        drafts::retire(&store.pool, "local", &uid, &compacted.stamp)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn a_new_empty_session_rotates_the_generation_and_old_retirement_is_harmless() {
    let store = store::Store::open_memory().await.unwrap();
    let uid = nucleus::new_uid("r");
    let write = initial(&uid);
    let first = drafts::save(&store.pool, "local", &uid, &write, None)
        .await
        .unwrap();
    let clear = Write {
        expected: Some(first.stamp.clone()),
        generation: first.stamp.generation.clone(),
        checkpoint: None,
        pending: vec![],
        requests: vec![],
        updates: vec![],
    };
    let empty = drafts::save(&store.pool, "local", &uid, &clear, None)
        .await
        .unwrap();
    let mut replacement = initial(&uid);
    replacement.expected = Some(empty.stamp.clone());
    let current = drafts::save(&store.pool, "local", &uid, &replacement, None)
        .await
        .unwrap();
    assert_ne!(current.stamp.generation, empty.stamp.generation);
    assert!(
        !drafts::retire(&store.pool, "local", &uid, &empty.stamp)
            .await
            .unwrap()
    );
    assert_eq!(
        drafts::metadata(&store.pool, "local", &uid)
            .await
            .unwrap()
            .unwrap(),
        current
    );
    let blocked = Write {
        expected: Some(current.stamp.clone()),
        generation: nucleus::new_uid("ed"),
        ..replacement
    };
    assert!(
        drafts::save(&store.pool, "local", &uid, &blocked, None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn malformed_or_unbounded_journals_are_refused_without_modifying_the_saved_draft() {
    let store = store::Store::open_memory().await.unwrap();
    let uid = nucleus::new_uid("r");
    let write = initial(&uid);
    let first = drafts::save(&store.pool, "local", &uid, &write, None)
        .await
        .unwrap();
    for pending in [
        vec!["missing".into()],
        vec!["first".into(), "first".into()],
        vec!["first".into(); 513],
    ] {
        let invalid = Write {
            expected: Some(first.stamp.clone()),
            generation: first.stamp.generation.clone(),
            checkpoint: None,
            pending,
            requests: vec![],
            updates: vec![],
        };
        assert!(
            drafts::save(&store.pool, "local", &uid, &invalid, None)
                .await
                .is_err()
        );
        assert_eq!(
            drafts::metadata(&store.pool, "local", &uid)
                .await
                .unwrap()
                .unwrap(),
            first
        );
    }
}

#[tokio::test]
async fn metadata_updates_do_not_rewrite_large_checkpoint_pages() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("recovery-pages.sqlite");
    let store = store::Store::open_durable(&format!("sqlite://{}", path.display()))
        .await
        .unwrap();
    let uid = nucleus::new_uid("r");
    let mut write = initial(&uid);
    write.checkpoint = Some("x".repeat(2 * 1024 * 1024));
    let first = drafts::save(&store.pool, "local", &uid, &write, None)
        .await
        .unwrap();
    store::sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&store.pool)
        .await
        .unwrap();
    let ack = Write {
        expected: Some(first.stamp.clone()),
        generation: first.stamp.generation,
        checkpoint: None,
        pending: vec![],
        requests: vec![],
        updates: vec![],
    };
    let metadata = drafts::save(&store.pool, "local", &uid, &ack, None)
        .await
        .unwrap();
    let wal = std::fs::metadata(path.with_file_name("recovery-pages.sqlite-wal"))
        .unwrap()
        .len();
    let page_size: i64 = store::sqlx::query_scalar("PRAGMA page_size")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    assert!(wal > 0 && wal <= page_size as u64 * 16, "{wal}");
    let saved = drafts::replay(&store.pool, "local", &uid, &metadata.stamp)
        .await
        .unwrap();
    assert_eq!(saved.checkpoint, write.checkpoint.unwrap());
    store.pool.close().await;
}

#[tokio::test]
async fn migrations_preserve_a_snapshot_draft_and_ordered_unicode_requests() {
    let pool = store::sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    store::sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&pool)
        .await
        .unwrap();
    store::sqlx::query("CREATE TABLE record_edit_draft(source TEXT NOT NULL,record_uid TEXT NOT NULL,state TEXT NOT NULL,updated_at TEXT NOT NULL,PRIMARY KEY(source,record_uid))")
        .execute(&pool).await.unwrap();
    let uid = nucleus::new_uid("r");
    let requests = serde_json::json!([
        {"id":"second","record_uid":uid,"value":"é新"},
        {"id":"first","record_uid":uid,"value":"👩‍💻"}
    ]);
    let state = serde_json::json!({"snapshot":"AAAA","pending":requests});
    store::sqlx::query("INSERT INTO record_edit_draft VALUES(?,?,?,?)")
        .bind("local\nowner")
        .bind(&uid)
        .bind(state.to_string())
        .bind("2026-11-05T00:00:00Z")
        .execute(&pool)
        .await
        .unwrap();
    for migration in [
        include_str!("../migrations/0042_incremental_record_edit_recovery.sql"),
        include_str!("../migrations/0043_separate_record_edit_checkpoints.sql"),
    ] {
        store::sqlx::raw_sql(migration)
            .execute(&pool)
            .await
            .unwrap();
    }
    let metadata = drafts::metadata(&pool, "local\nowner", &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(metadata.pending, ["second", "first"]);
    let restored = drafts::replay(&pool, "local\nowner", &uid, &metadata.stamp)
        .await
        .unwrap();
    assert_eq!(restored.checkpoint, "AAAA");
    for (index, request) in restored.requests.iter().enumerate() {
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&request.payload).unwrap(),
            requests[index]
        );
    }
    assert!(
        store::sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&pool)
            .await
            .unwrap()
            .is_empty()
    );
    pool.close().await;
}
