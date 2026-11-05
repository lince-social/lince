use std::sync::Arc;

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use engine::{
    Engine,
    actions::Action,
    record_change::{Mutation, Request},
    record_edit::Write,
};
use loro::{ExportMode, LoroDoc};
use store::record_edit_drafts::{self as drafts, Metadata};

fn request(uid: &str, mutation: Mutation) -> Arc<Request> {
    Arc::new(Request {
        id: nucleus::new_uid("op"),
        record_uid: uid.into(),
        mutation,
    })
}

fn write(
    metadata: Option<Metadata>,
    checkpoint: Option<String>,
    pending: Vec<Arc<Request>>,
    updates: Vec<Vec<u8>>,
    atoms: usize,
) -> Write {
    Write {
        generation: metadata.as_ref().map_or_else(
            || nucleus::new_uid("ed"),
            |metadata| metadata.stamp.generation.clone(),
        ),
        expected: metadata,
        checkpoint: checkpoint.map(Arc::new),
        pending,
        updates,
        atoms,
    }
}

async fn record(engine: &Engine) -> String {
    engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "Recovery".into(),
                body: "Before".into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap()
}

#[tokio::test]
async fn file_backed_unsent_text_recovers_and_canonical_replay_is_exactly_once() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        directory.path().join("recovery.sqlite").display()
    );
    let engine = Engine::open(&url).await.unwrap();
    let uid = record(&engine).await;
    let checkpoint = engine.collab_snapshot(&uid).await.unwrap();
    let document = LoroDoc::new();
    document.import(&B64.decode(&checkpoint).unwrap()).unwrap();
    document.set_peer_id(99).unwrap();
    let before = document.oplog_vv();
    document.get_text("body").insert(0, "é新 ").unwrap();
    document.commit();
    let change = request(
        &uid,
        Mutation::Text {
            update_base64: engine::collab::encode_update(&document, &before).unwrap(),
        },
    );
    let metadata = engine
        .save_record_edit_draft(
            "local\nowner",
            &uid,
            write(
                None,
                Some(checkpoint),
                vec![change.clone()],
                vec![document.export(ExportMode::updates(&before)).unwrap()],
                3,
            ),
        )
        .await
        .unwrap();
    assert_eq!(engine.doc_text(&uid).await.unwrap().1, "Before");
    engine.store.pool.close().await;
    drop(engine);
    let engine = Engine::open(&url).await.unwrap();
    let loaded = engine
        .load_record_edit_draft("local\nowner", &uid)
        .await
        .unwrap();
    assert_eq!(loaded.metadata.unwrap(), metadata);
    let recovered = loaded.recovered.unwrap();
    let restored = LoroDoc::new();
    restored.import(&recovered.snapshot).unwrap();
    assert_eq!(restored.get_text("body").to_string(), "é新 Before");
    assert_eq!(recovered.pending[0].id, change.id);
    assert!(
        engine
            .load_record_edit_draft("local\nanother-owner", &uid)
            .await
            .unwrap()
            .recovered
            .is_none()
    );
    engine
        .change_record(recovered.pending[0].as_ref().clone(), None)
        .await
        .unwrap();
    let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact WHERE record_uid=?")
        .bind(&uid)
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    engine
        .change_record(recovered.pending[0].as_ref().clone(), None)
        .await
        .unwrap();
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM fact WHERE record_uid=?")
            .bind(&uid)
            .fetch_one(&engine.store.pool)
            .await
            .unwrap(),
        count
    );
    let acknowledged = engine
        .save_record_edit_draft(
            "local\nowner",
            &uid,
            write(Some(metadata), None, vec![], vec![], 0),
        )
        .await
        .unwrap();
    assert!(
        engine
            .load_record_edit_draft("local\nowner", &uid)
            .await
            .unwrap()
            .recovered
            .is_none()
    );
    assert_eq!(engine.doc_text(&uid).await.unwrap().1, "é新 Before");
    assert!(
        engine
            .retire_record_edit_draft("local\nowner", &uid, &acknowledged.stamp)
            .await
            .unwrap()
    );
    engine.store.pool.close().await;
    let reopened = Engine::open(&url).await.unwrap();
    assert_eq!(reopened.doc_text(&uid).await.unwrap().1, "é新 Before");
    reopened.store.pool.close().await;
}

#[tokio::test]
async fn a_long_offline_text_backlog_recovers_and_can_acknowledge_its_durable_prefix() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        directory.path().join("offline-text.sqlite").display()
    );
    let engine = Engine::open(&url).await.unwrap();
    let uid = record(&engine).await;
    let checkpoint = engine.collab_snapshot(&uid).await.unwrap();
    let document = LoroDoc::new();
    document.import(&B64.decode(&checkpoint).unwrap()).unwrap();
    let base = document.oplog_vv();
    let mut pending = Vec::new();
    for _ in 0..1025 {
        let before = document.oplog_vv();
        document.get_text("body").insert(0, "é新").unwrap();
        document.commit();
        pending.push(request(
            &uid,
            Mutation::Text {
                update_base64: engine::collab::encode_update(&document, &before).unwrap(),
            },
        ));
    }
    let ids: Vec<_> = pending.iter().map(|request| request.id.clone()).collect();
    let metadata = engine
        .save_record_edit_draft(
            "local",
            &uid,
            write(
                None,
                Some(checkpoint),
                pending,
                vec![document.export(ExportMode::updates(&base)).unwrap()],
                2050,
            ),
        )
        .await
        .unwrap();
    engine.store.pool.close().await;
    drop(engine);
    let engine = Engine::open(&url).await.unwrap();
    let recovered = engine
        .load_record_edit_draft("local", &uid)
        .await
        .unwrap()
        .recovered
        .unwrap();
    assert_eq!(
        recovered
            .pending
            .iter()
            .map(|request| request.id.clone())
            .collect::<Vec<_>>(),
        ids
    );
    let replica = LoroDoc::new();
    replica.import(&recovered.snapshot).unwrap();
    assert_eq!(
        replica.get_text("body").to_string(),
        format!("{}Before", "é新".repeat(1025))
    );
    engine
        .change_record(recovered.pending[0].as_ref().clone(), None)
        .await
        .unwrap();
    assert_eq!(engine.doc_text(&uid).await.unwrap().1, "é新Before");
    let remaining: Vec<_> = recovered.pending.into_iter().skip(1).collect();
    engine
        .save_record_edit_draft(
            "local",
            &uid,
            write(Some(metadata), None, remaining, Vec::new(), 0),
        )
        .await
        .unwrap();
    engine.store.pool.close().await;
    drop(engine);
    let engine = Engine::open(&url).await.unwrap();
    let recovered = engine
        .load_record_edit_draft("local", &uid)
        .await
        .unwrap()
        .recovered
        .unwrap();
    assert_eq!(recovered.pending.len(), 1024);
    assert_eq!(recovered.pending[0].id, ids[1]);
    assert_eq!(engine.doc_text(&uid).await.unwrap().1, "é新Before");
    engine.store.pool.close().await;
}

#[tokio::test]
async fn failed_scalar_intent_is_recoverable_and_can_be_replaced_without_a_text_snapshot() {
    let engine = Engine::open_memory().await.unwrap();
    let uid = record(&engine).await;
    let snapshot = B64.encode(LoroDoc::new().export(ExportMode::Snapshot).unwrap());
    let refused = request(
        &uid,
        Mutation::Quantity {
            value: "not a quantity".into(),
        },
    );
    let metadata = engine
        .save_record_edit_draft(
            "local",
            &uid,
            write(None, Some(snapshot), vec![refused.clone()], vec![], 0),
        )
        .await
        .unwrap();
    assert!(
        engine
            .change_record(refused.as_ref().clone(), None)
            .await
            .is_err()
    );
    assert_eq!(
        engine
            .load_record_edit_draft("local", &uid)
            .await
            .unwrap()
            .recovered
            .unwrap()
            .pending[0]
            .id,
        refused.id
    );
    let corrected = request(
        &uid,
        Mutation::Quantity {
            value: "3.75".into(),
        },
    );
    let next = engine
        .save_record_edit_draft(
            "local",
            &uid,
            write(Some(metadata), None, vec![corrected.clone()], vec![], 0),
        )
        .await
        .unwrap();
    assert_eq!(next.update_count, 0);
    let loaded = engine
        .load_record_edit_draft("local", &uid)
        .await
        .unwrap()
        .recovered
        .unwrap();
    assert_eq!(loaded.pending.len(), 1);
    assert_eq!(loaded.pending[0].id, corrected.id);
    engine
        .change_record(loaded.pending[0].as_ref().clone(), None)
        .await
        .unwrap();
}

#[tokio::test]
async fn periodic_checkpoints_keep_all_text_and_the_original_pending_request() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        directory
            .path()
            .join("checkpoint-recovery.sqlite")
            .display()
    );
    let engine = Engine::open(&url).await.unwrap();
    let uid = record(&engine).await;
    let document = LoroDoc::new();
    document.get_text("body").insert(0, "base").unwrap();
    document.commit();
    let pending = request(&uid, Mutation::Quantity { value: "1".into() });
    let checkpoint = B64.encode(document.export(ExportMode::Snapshot).unwrap());
    let mut metadata = engine
        .save_record_edit_draft(
            "local",
            &uid,
            write(None, Some(checkpoint), vec![pending.clone()], vec![], 0),
        )
        .await
        .unwrap();
    for _ in 0..128 {
        let before = document.oplog_vv();
        document.get_text("body").insert(0, "x").unwrap();
        document.commit();
        metadata = engine
            .save_record_edit_draft(
                "local",
                &uid,
                write(
                    Some(metadata),
                    None,
                    vec![pending.clone()],
                    vec![document.export(ExportMode::updates(&before)).unwrap()],
                    1,
                ),
            )
            .await
            .unwrap();
    }
    assert_eq!(metadata.update_count, 0);
    engine.store.pool.close().await;
    drop(engine);
    let engine = Engine::open(&url).await.unwrap();
    let restored = engine
        .load_record_edit_draft("local", &uid)
        .await
        .unwrap()
        .recovered
        .unwrap();
    assert_eq!(restored.pending[0].id, pending.id);
    let replica = LoroDoc::new();
    replica.import(&restored.snapshot).unwrap();
    assert_eq!(
        replica.get_text("body").to_string(),
        format!("{}base", "x".repeat(128))
    );
}

#[tokio::test]
async fn unresolved_remote_updates_survive_recovery_and_checkpoint_only_after_dependencies_arrive()
{
    let engine = Engine::open_memory().await.unwrap();
    let uid = record(&engine).await;
    let origin = LoroDoc::new();
    origin.get_text("body").insert(0, "base").unwrap();
    origin.commit();
    let checkpoint = B64.encode(origin.export(ExportMode::Snapshot).unwrap());
    let before_a = origin.oplog_vv();
    origin.get_text("body").insert(0, "A").unwrap();
    origin.commit();
    let parent = origin.export(ExportMode::updates(&before_a)).unwrap();
    let before_b = origin.oplog_vv();
    origin
        .get_text("body")
        .insert(0, &"b".repeat(70000))
        .unwrap();
    origin.commit();
    let child = origin.export(ExportMode::updates(&before_b)).unwrap();
    let pending = request(&uid, Mutation::Quantity { value: "1".into() });
    let metadata = engine
        .save_record_edit_draft(
            "local",
            &uid,
            write(
                None,
                Some(checkpoint),
                vec![pending.clone()],
                vec![child],
                70000,
            ),
        )
        .await
        .unwrap();
    assert_eq!(metadata.update_count, 1);
    let recovered = engine
        .load_record_edit_draft("local", &uid)
        .await
        .unwrap()
        .recovered
        .unwrap();
    assert!(!recovered.pending_updates.is_empty());
    let replica = LoroDoc::new();
    replica.import(&recovered.snapshot).unwrap();
    replica.import_batch(&recovered.pending_updates).unwrap();
    assert_eq!(replica.get_text("body").to_string(), "base");
    replica.import(&parent).unwrap();
    assert_eq!(
        replica.get_text("body").to_string(),
        format!("{}Abase", "b".repeat(70000))
    );
    let complete = engine
        .save_record_edit_draft(
            "local",
            &uid,
            write(
                Some(metadata),
                None,
                vec![pending.clone()],
                vec![parent],
                70001,
            ),
        )
        .await
        .unwrap();
    assert_eq!(complete.update_count, 0);
    let recovered = engine
        .load_record_edit_draft("local", &uid)
        .await
        .unwrap()
        .recovered
        .unwrap();
    assert!(recovered.pending_updates.is_empty());
    let replica = LoroDoc::new();
    replica.import(&recovered.snapshot).unwrap();
    assert_eq!(
        replica.get_text("body").to_string(),
        format!("{}Abase", "b".repeat(70000))
    );
}

#[tokio::test]
async fn cross_record_payloads_are_refused_without_creating_a_journal() {
    let engine = Engine::open_memory().await.unwrap();
    let uid = record(&engine).await;
    let another = nucleus::new_uid("r");
    let pending = request(&another, Mutation::Quantity { value: "1".into() });
    let checkpoint = B64.encode(LoroDoc::new().export(ExportMode::Snapshot).unwrap());
    assert!(
        engine
            .save_record_edit_draft(
                "local",
                &uid,
                write(None, Some(checkpoint), vec![pending], vec![], 0)
            )
            .await
            .is_err()
    );
    assert!(
        drafts::metadata(&engine.store.pool, "local", &uid)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn a_large_file_backed_unsent_edit_recovers_from_its_checkpoint_and_small_delta() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        directory.path().join("large-recovery.sqlite").display()
    );
    let engine = Engine::open(&url).await.unwrap();
    let uid = record(&engine).await;
    let mut seed = 0x517cc1b727220a95_u64;
    let body: String = (0..1048000)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            char::from(b'a' + (seed % 26) as u8)
        })
        .collect();
    engine
        .write_record_text(&uid, None, Some(""))
        .await
        .unwrap();
    engine
        .write_record_text(&uid, None, Some(&body))
        .await
        .unwrap();
    let checkpoint = engine.collab_snapshot(&uid).await.unwrap();
    let document = LoroDoc::new();
    document.import(&B64.decode(&checkpoint).unwrap()).unwrap();
    let before = document.oplog_vv();
    document.get_text("body").insert(0, "é新").unwrap();
    document.commit();
    let change = request(
        &uid,
        Mutation::Text {
            update_base64: engine::collab::encode_update(&document, &before).unwrap(),
        },
    );
    let delta = document.export(ExportMode::updates(&before)).unwrap();
    assert!(delta.len() < 4096);
    engine
        .save_record_edit_draft(
            "local",
            &uid,
            write(None, Some(checkpoint), vec![change.clone()], vec![delta], 2),
        )
        .await
        .unwrap();
    engine.store.pool.close().await;
    drop(engine);
    let engine = Engine::open(&url).await.unwrap();
    let recovered = engine
        .load_record_edit_draft("local", &uid)
        .await
        .unwrap()
        .recovered
        .unwrap();
    assert_eq!(recovered.pending[0].id, change.id);
    let replica = LoroDoc::new();
    replica.import(&recovered.snapshot).unwrap();
    assert_eq!(replica.get_text("body").to_string(), format!("é新{body}"));
    assert!(recovered.pending_updates.is_empty());
    engine.store.pool.close().await;
}

struct CrashChild(std::process::Child);

impl Drop for CrashChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test]
#[ignore = "private child invoked by the recovery crash-boundary test"]
async fn recovery_crash_child() {
    let url = std::env::var("LINCE_RECOVERY_CRASH_DATABASE").unwrap();
    let phase = std::env::var("LINCE_RECOVERY_CRASH_PHASE").unwrap();
    let ready = std::path::PathBuf::from(std::env::var_os("LINCE_RECOVERY_CRASH_READY").unwrap());
    let engine = Engine::open(&url).await.unwrap();
    let uid = record(&engine).await;
    let checkpoint = engine.collab_snapshot(&uid).await.unwrap();
    let document = LoroDoc::new();
    document.import(&B64.decode(&checkpoint).unwrap()).unwrap();
    let before = document.oplog_vv();
    let inserted = if phase == "compaction" {
        "x".repeat(65536)
    } else {
        "é新 ".to_owned()
    };
    document.get_text("body").insert(0, &inserted).unwrap();
    document.commit();
    let change = request(
        &uid,
        Mutation::Text {
            update_base64: engine::collab::encode_update(&document, &before).unwrap(),
        },
    );
    let mut metadata = engine
        .save_record_edit_draft(
            "local\ncrash",
            &uid,
            write(
                None,
                Some(checkpoint),
                vec![change.clone()],
                vec![document.export(ExportMode::updates(&before)).unwrap()],
                inserted.chars().count(),
            ),
        )
        .await
        .unwrap();
    if phase == "compaction" {
        assert_eq!(metadata.update_count, 0);
    }
    if phase == "canonical" || phase == "acknowledgement" {
        engine
            .change_record(change.as_ref().clone(), None)
            .await
            .unwrap();
    }
    if phase == "acknowledgement" {
        metadata = engine
            .save_record_edit_draft(
                "local\ncrash",
                &uid,
                write(Some(metadata), None, Vec::new(), Vec::new(), 0),
            )
            .await
            .unwrap();
    }
    let state = serde_json::json!({"uid":uid,"request":change.id,"pending":metadata.pending});
    let temporary = ready.with_extension("tmp");
    std::fs::write(&temporary, serde_json::to_vec(&state).unwrap()).unwrap();
    std::fs::rename(temporary, ready).unwrap();
    loop {
        std::thread::park();
    }
}

#[tokio::test]
async fn unclean_exit_at_each_durable_boundary_preserves_text_and_exactly_once_replay() {
    for phase in ["append", "canonical", "acknowledgement", "compaction"] {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("crash.sqlite");
        let url = format!("sqlite://{}", database.display());
        let ready = directory.path().join("ready.json");
        let output = directory.path().join("child.log");
        let log = std::fs::File::create(&output).unwrap();
        let mut child = CrashChild(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "recovery_crash_child",
                    "--exact",
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env("LINCE_RECOVERY_CRASH_DATABASE", &url)
                .env("LINCE_RECOVERY_CRASH_PHASE", phase)
                .env("LINCE_RECOVERY_CRASH_READY", &ready)
                .stdout(log.try_clone().unwrap())
                .stderr(log)
                .spawn()
                .unwrap(),
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(45);
        while !ready.exists() {
            assert!(
                child.0.try_wait().unwrap().is_none() && std::time::Instant::now() < deadline,
                "{phase}: {}",
                std::fs::read_to_string(&output).unwrap()
            );
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let state: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&ready).unwrap()).unwrap();
        let uid = state["uid"].as_str().unwrap();
        let change_id = state["request"].as_str().unwrap();
        assert!(
            std::fs::metadata(database.with_file_name("crash.sqlite-wal"))
                .unwrap()
                .len()
                > 32
        );
        child.0.kill().unwrap();
        assert!(!child.0.wait().unwrap().success());
        let engine = Engine::open(&url).await.unwrap();
        let expected = if phase == "compaction" {
            format!("{}Before", "x".repeat(65536))
        } else {
            "é新 Before".to_owned()
        };
        let loaded = engine
            .load_record_edit_draft("local\ncrash", uid)
            .await
            .unwrap();
        if phase == "acknowledgement" {
            assert!(loaded.recovered.is_none());
            assert_eq!(engine.doc_text(uid).await.unwrap().1, expected);
        } else {
            let recovered = loaded.recovered.unwrap();
            assert_eq!(recovered.pending.len(), 1);
            assert_eq!(recovered.pending[0].id, change_id);
            let replica = LoroDoc::new();
            replica.import(&recovered.snapshot).unwrap();
            assert_eq!(replica.get_text("body").to_string(), expected);
            engine
                .change_record(recovered.pending[0].as_ref().clone(), None)
                .await
                .unwrap();
            let count: i64 =
                store::sqlx::query_scalar("SELECT COUNT(*) FROM fact WHERE record_uid=?")
                    .bind(uid)
                    .fetch_one(&engine.store.pool)
                    .await
                    .unwrap();
            engine
                .change_record(recovered.pending[0].as_ref().clone(), None)
                .await
                .unwrap();
            assert_eq!(
                store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM fact WHERE record_uid=?")
                    .bind(uid)
                    .fetch_one(&engine.store.pool)
                    .await
                    .unwrap(),
                count
            );
        }
        assert_eq!(engine.doc_text(uid).await.unwrap().1, expected);
        engine.store.pool.close().await;
        let reopened = Engine::open(&url).await.unwrap();
        assert_eq!(reopened.doc_text(uid).await.unwrap().1, expected);
        reopened.store.pool.close().await;
    }
}
