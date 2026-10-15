use engine::{Engine, actions::Action};
use nucleus::knowledge::Command;
use serde_json::Value;
use serde_json::json;

async fn read_from(engine: &Engine, source: &str, item: &str) -> Value {
    let id = nucleus::new_uid("kr");
    command(
        engine,
        Command::Read {
            request_id: id.clone(),
            source: source.into(),
            item: item.into(),
        },
    )
    .await;
    let result = complete(engine, &id).await;
    assert_eq!(result["status"], "complete", "{result}");
    result["result"]["item"].clone()
}

async fn organ(engine: &Engine) -> String {
    store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid
}

async fn save(engine: &Engine, source: &str, item: &str) -> String {
    let preview = command(
        engine,
        Command::PreviewSave {
            source: source.into(),
            item: item.into(),
            organ: organ(engine).await,
            selection: Default::default(),
        },
    )
    .await;
    command(
        engine,
        Command::Save {
            preview: preview["preview_uid"].as_str().unwrap().into(),
        },
    )
    .await["record_uid"]
        .as_str()
        .unwrap()
        .into()
}

#[tokio::test]
async fn export_import_are_offline_atomic_and_keep_required_data() {
    let engine = Engine::open_memory().await.unwrap();
    read_from(&engine, "test", "reading").await;
    read_from(&engine, "test", "reference").await;
    let uid = save(&engine, "test", "reading").await;
    engine
        .act(
            Action::EditRecordText {
                target: uid.clone(),
                head: None,
                body: Some("Private note".into()),
            },
            None,
        )
        .await
        .unwrap();
    let package = command(&engine, Command::Export { record: uid }).await["package"].clone();
    assert!(!package.to_string().contains("Private note"));
    let receiver = Engine::open_memory().await.unwrap();
    let destination = organ(&receiver).await;
    let count = store::records::list_all(&receiver.store.pool)
        .await
        .unwrap()
        .len();
    let mut broken = package.clone();
    let root = broken["root"].clone();
    broken["records"]
        .as_array_mut()
        .unwrap()
        .retain(|record| record["uid"] == root);
    assert!(
        receiver
            .act(
                Action::Knowledge {
                    request: Command::Import {
                        package: broken.to_string(),
                        organ: destination.clone()
                    }
                },
                None
            )
            .await
            .is_err()
    );
    assert_eq!(
        store::records::list_all(&receiver.store.pool)
            .await
            .unwrap()
            .len(),
        count
    );
    let request = Command::Import {
        package: package.to_string(),
        organ: destination,
    };
    let imported = command(&receiver, request.clone()).await;
    assert_eq!(imported["saved_records"].as_array().unwrap().len(), 2);
    assert_eq!(command(&receiver, request).await, imported);
    let uid = imported["record_uid"].as_str().unwrap();
    let snapshot = command(&receiver, Command::InspectSaved { record: uid.into() }).await;
    assert_eq!(snapshot["item"]["claims"][0]["value"]["amount"], "21.5");
    assert_eq!(snapshot["record"]["body"], "");
    assert_eq!(command(&receiver, Command::Sources).await["cache_bytes"], 0);
}

#[tokio::test]
async fn reviews_preserve_notes_allow_selected_changes_and_create_independent_copies() {
    use nucleus::knowledge::UpdateField;
    let engine = Engine::open_memory().await.unwrap();
    let mut archive = engine::knowledge::provider::test_source().archive;
    archive.source.id = "local-fixture".into();
    for item in &mut archive.items {
        item.source = archive.source.id.clone();
        for claim in &mut item.claims {
            claim.source = item.source.clone();
        }
        for reference in &mut item.dependencies {
            reference.source = item.source.clone();
        }
    }
    let source = command(
        &engine,
        Command::InstallArchive {
            package: serde_json::to_string(&archive).unwrap(),
        },
    )
    .await["source_uid"]
        .as_str()
        .unwrap()
        .to_owned();
    read_from(&engine, &source, "reading").await;
    read_from(&engine, &source, "reference").await;
    let uid = save(&engine, &source, "reading").await;
    engine
        .act(
            Action::EditRecordText {
                target: uid.clone(),
                head: Some("My title".into()),
                body: Some("My note".into()),
            },
            None,
        )
        .await
        .unwrap();
    archive.items[0].revision = "2".into();
    archive.items[0].title = "New source title".into();
    archive.items[0].content = "New source body".into();
    archive.items[0].raw = "revision 2 raw source".into();
    command(
        &engine,
        Command::ReplaceArchive {
            source: source.clone(),
            package: serde_json::to_string(&archive).unwrap(),
        },
    )
    .await;
    read_from(&engine, &source, "reading").await;
    let review = command(
        &engine,
        Command::ReviewUpdate {
            record: uid.clone(),
        },
    )
    .await;
    assert_eq!(review["review"]["local_changed"]["title"], true);
    assert_eq!(review["review"]["local_changed"]["annotations"], true);
    let apply = Command::ApplyUpdate {
        review: review["review_uid"].as_str().unwrap().into(),
        fields: vec![UpdateField::Content],
        fork: false,
    };
    command(&engine, apply.clone()).await;
    assert_eq!(command(&engine, apply).await["record_uid"], uid);
    let saved = command(
        &engine,
        Command::InspectSaved {
            record: uid.clone(),
        },
    )
    .await;
    assert_eq!(saved["record"]["head"], "My title");
    assert_eq!(saved["record"]["body"], "My note");
    assert_eq!(saved["item"]["content"], "New source body");
    let review = command(
        &engine,
        Command::ReviewUpdate {
            record: uid.clone(),
        },
    )
    .await;
    let fork = command(
        &engine,
        Command::ApplyUpdate {
            review: review["review_uid"].as_str().unwrap().into(),
            fields: vec![UpdateField::Title],
            fork: true,
        },
    )
    .await;
    assert_ne!(fork["record_uid"], uid);
    let package = command(
        &engine,
        Command::Export {
            record: fork["record_uid"].as_str().unwrap().into(),
        },
    )
    .await["package"]
        .to_string();
    let receiver = Engine::open_memory().await.unwrap();
    let import = Command::Import {
        package,
        organ: organ(&receiver).await,
    };
    let copy = command(&receiver, import.clone()).await;
    assert_eq!(command(&receiver, import).await, copy);
    assert_eq!(
        command(
            &engine,
            Command::InspectSaved {
                record: fork["record_uid"].as_str().unwrap().into()
            }
        )
        .await["record"]["body"],
        "My note"
    );
    let review = command(
        &engine,
        Command::ReviewUpdate {
            record: uid.clone(),
        },
    )
    .await;
    engine
        .act(
            Action::EditRecordText {
                target: uid.clone(),
                head: None,
                body: Some("Changed after review".into()),
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        engine
            .act(
                Action::Knowledge {
                    request: Command::ApplyUpdate {
                        review: review["review_uid"].as_str().unwrap().into(),
                        fields: vec![UpdateField::Title],
                        fork: false
                    }
                },
                None
            )
            .await
            .is_err()
    );
    command(&engine, Command::RemoveSource { source }).await;
    assert_eq!(
        command(&engine, Command::InspectSaved { record: uid }).await["item"]["revision"],
        "2"
    );
}

#[tokio::test]
async fn archive_replacement_cancels_old_reads_without_losing_a_working_source_on_failure() {
    let engine = Engine::open_memory().await.unwrap();
    let mut archive = engine::knowledge::provider::test_source().archive;
    let source = command(
        &engine,
        Command::InstallArchive {
            package: serde_json::to_string(&archive).unwrap(),
        },
    )
    .await["source_uid"]
        .as_str()
        .unwrap()
        .to_owned();
    let old = nucleus::new_uid("kr");
    command(
        &engine,
        Command::Read {
            request_id: old.clone(),
            source: source.clone(),
            item: "reading".into(),
        },
    )
    .await;
    archive.items[0].revision = "2".into();
    archive.items[0].content = "A revised source reading".into();
    command(
        &engine,
        Command::ReplaceArchive {
            source: source.clone(),
            package: serde_json::to_string(&archive).unwrap(),
        },
    )
    .await;
    assert_eq!(
        command(&engine, Command::Poll { request_id: old }).await["status"],
        "cancelled"
    );
    tokio::task::yield_now().await;
    assert!(
        engine
            .act(
                Action::Knowledge {
                    request: Command::ReadCached {
                        source: source.clone(),
                        item: "reading".into()
                    }
                },
                None
            )
            .await
            .is_err()
    );
    assert_eq!(
        read_from(&engine, &source, "reading").await["revision"],
        "2"
    );
    archive.items[0].content = "Changed without a new revision".into();
    assert!(
        engine
            .act(
                Action::Knowledge {
                    request: Command::ReplaceArchive {
                        source: source.clone(),
                        package: serde_json::to_string(&archive).unwrap()
                    }
                },
                None
            )
            .await
            .is_err()
    );
    assert_eq!(
        command(
            &engine,
            Command::ReadCached {
                source: source.clone(),
                item: "reading".into()
            }
        )
        .await["item"]["content"],
        "A revised source reading"
    );
    assert_eq!(
        read_from(&engine, &source, "reading").await["content"],
        "A revised source reading"
    );
}

#[tokio::test]
async fn a_full_mapping_collection_can_reconfirm_an_existing_meaning_but_cannot_grow() {
    let engine = Engine::open_memory().await.unwrap();
    read_from(&engine, "test", "reading").await;
    let concept = engine
        .act(
            Action::Vocabulary {
                request: nucleus::vocabulary::Command::CreateConcept {
                    vocabulary: "g_local".into(),
                    name: "Measured temperature".into(),
                    language: "en".into(),
                    definition: "Temperature with its measurement conditions".into(),
                    derived_from: None,
                },
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let request = Command::MapMeaning {
        source: "test".into(),
        external: "measurement".into(),
        kind: nucleus::knowledge::MeaningKind::Predicate,
        concept: concept.clone(),
    };
    command(&engine, request.clone()).await;
    store::sqlx::query("WITH RECURSIVE sequence(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM sequence WHERE n<4095) INSERT INTO knowledge_mapping(owner_key,source_uid,external_id,kind,concept_uid,definition_revision,external_revision,external_hash) SELECT 'local','test','other-'||n,'predicate',?,1,'1','' FROM sequence").bind(&concept).execute(&engine.store.pool).await.unwrap();
    assert_eq!(
        command(&engine, request).await["mapping"]["concept_uid"],
        concept
    );
    assert!(
        engine
            .act(
                Action::Knowledge {
                    request: Command::MapMeaning {
                        source: "test".into(),
                        external: "celsius".into(),
                        kind: nucleus::knowledge::MeaningKind::Unit,
                        concept
                    }
                },
                None
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn save_and_source_data_are_private_and_permissions_are_checked_on_retries() {
    let engine = Engine::open_memory().await.unwrap();
    let person = store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Person,
            head: "Reader",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid;
    assert!(
        engine
            .act(
                Action::Knowledge {
                    request: Command::Sources
                },
                Some(person.clone())
            )
            .await
            .is_err()
    );
    let role = store::auth::ensure_role(&engine.store.pool, "Knowledge readers")
        .await
        .unwrap();
    let permission = store::auth::ensure_permission(&engine.store.pool, "record", "read")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, permission)
        .await
        .unwrap();
    engine
        .act(
            Action::AssignRoles {
                person: person.clone(),
                roles: vec!["Knowledge readers".into()],
                expected_revision: 0,
            },
            None,
        )
        .await
        .unwrap();
    read_from(&engine, "test", "reading").await;
    assert!(
        engine
            .act(
                Action::Knowledge {
                    request: Command::ReadCached {
                        source: "test".into(),
                        item: "reading".into()
                    }
                },
                Some(person.clone())
            )
            .await
            .is_err()
    );
    assert!(
        engine
            .act(
                Action::Knowledge {
                    request: Command::PreviewSave {
                        source: "test".into(),
                        item: "reading".into(),
                        organ: organ(&engine).await,
                        selection: Default::default()
                    }
                },
                Some(person.clone())
            )
            .await
            .is_err()
    );
    let id = nucleus::new_uid("kr");
    engine
        .act(
            Action::Knowledge {
                request: Command::Read {
                    request_id: id.clone(),
                    source: "test".into(),
                    item: "reading".into(),
                },
            },
            Some(person.clone()),
        )
        .await
        .unwrap();
    assert!(
        engine
            .act(
                Action::Knowledge {
                    request: Command::Poll {
                        request_id: id.clone()
                    }
                },
                None
            )
            .await
            .is_err()
    );
    store::sqlx::query("DELETE FROM role_permission WHERE role_id=?")
        .bind(role)
        .execute(&engine.store.pool)
        .await
        .unwrap();
    assert!(
        engine
            .act(
                Action::Knowledge {
                    request: Command::Poll { request_id: id }
                },
                Some(person.clone())
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn asset_integrity_limits_and_transaction_rollback_protect_saved_content() {
    let engine = Engine::open_memory().await.unwrap();
    let mut item = engine::knowledge::provider::test_source()
        .archive
        .items
        .remove(1);
    item.assets.push(nucleus::knowledge::Asset {
        id: "figure".into(),
        uri: "urn:figure".into(),
        media_type: "image/png".into(),
        license: item.license.clone(),
        credits: vec![],
        sha384: Some("bad".into()),
        bytes: Some("aGVsbG8=".into()),
        required: true,
    });
    assert!(store::knowledge::validate_item(&item).is_err());
    item.assets[0].sha384 = Some(store::knowledge::fingerprint(b"hello"));
    store::knowledge::validate_item(&item).unwrap();
    let uid = nucleus::new_uid("r");
    let destination = organ(&engine).await;
    let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
    store::knowledge::create_reference_on(&mut tx, &uid, &destination, &item)
        .await
        .unwrap();
    store::knowledge::save_payload_on(&mut tx, &uid, &item, &json!({"format":1,"item":item}))
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    assert!(
        store::records::get(&engine.store.pool, &uid)
            .await
            .unwrap()
            .is_none()
    );
    let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
    for n in 0..12 {
        let mut large = item.clone();
        large.id = format!("large-{n}");
        large.content = "a".repeat(3 * 1024 * 1024);
        store::knowledge::cache_on(&mut tx, "local", &large)
            .await
            .unwrap();
    }
    tx.commit().await.unwrap();
    assert!(
        command(&engine, Command::Sources).await["cache_bytes"]
            .as_u64()
            .unwrap()
            <= nucleus::knowledge::CACHE_LIMIT as u64
    );
    assert!(
        command(
            &engine,
            Command::ReadCached {
                source: "test".into(),
                item: "large-11".into()
            }
        )
        .await["offline"]
            .as_bool()
            .unwrap()
    );
}

#[tokio::test]
async fn synced_knowledge_chunks_detect_missing_parts_and_restore_durable_reference_identity() {
    use store::sqlx::Row;
    let sender = Engine::open_memory().await.unwrap();
    read_from(&sender, "test", "reading").await;
    read_from(&sender, "test", "reference").await;
    let uid = save(&sender, "test", "reading").await;
    let extensions =
        store::sqlx::query("SELECT namespace,fds FROM record_extension WHERE record_uid=?")
            .bind(&uid)
            .fetch_all(&sender.store.pool)
            .await
            .unwrap();
    let receiver = Engine::open_memory().await.unwrap();
    let destination = organ(&receiver).await;
    store::records::create_with_uid(
        &receiver.store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Plain,
            head: "Replica",
            body: "",
            quantity: store::exact::zero(),
        },
        &uid,
    )
    .await
    .unwrap();
    let mut ordered = extensions.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|row| {
        !(row.get::<String, _>("namespace") == store::knowledge::KNOWLEDGE_NAMESPACE)
    });
    for (index, row) in ordered.iter().enumerate() {
        let namespace: String = row.get("namespace");
        let value: Value = serde_json::from_str(&row.get::<String, _>("fds")).unwrap();
        store::sync_apply::set_extension_whole(
            &receiver.store.pool,
            &uid,
            &namespace,
            &value,
            store::sync_apply::Stamp {
                tbl: "record_extension",
                uid: &uid,
                field: &namespace,
                hlc: i64::MAX,
            },
        )
        .await
        .unwrap();
        if index == 0 {
            let mut tx = store::write_tx(&receiver.store.pool).await.unwrap();
            assert!(store::knowledge::payload_on(&mut tx, &uid).await.is_err());
        }
    }
    let restored = command(
        &receiver,
        Command::InspectSaved {
            record: uid.clone(),
        },
    )
    .await;
    assert_eq!(restored["item"]["claims"][0]["value"]["amount"], "21.5");
    let mut tx = store::write_tx(&receiver.store.pool).await.unwrap();
    assert_eq!(
        store::knowledge::reference_uid_on(&mut tx, &destination, "test", "reading")
            .await
            .unwrap(),
        Some(uid)
    );
    let operations = store::sync_ops::after(&sender.store.pool, &organ(&sender).await, 0, 10000)
        .await
        .unwrap();
    assert!(
        operations
            .iter()
            .any(|operation| operation.tbl == "record_extension"
                && operation.field.contains("lince.knowledge"))
    );
}

#[tokio::test]
async fn interrupted_previews_and_save_receipts_survive_database_reopening() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        directory.path().join("knowledge.db").display()
    );
    let engine = Engine::open(&url).await.unwrap();
    read_from(&engine, "test", "reading").await;
    read_from(&engine, "test", "reference").await;
    let preview = command(
        &engine,
        Command::PreviewSave {
            source: "test".into(),
            item: "reading".into(),
            organ: organ(&engine).await,
            selection: Default::default(),
        },
    )
    .await;
    let request = Command::Save {
        preview: preview["preview_uid"].as_str().unwrap().into(),
    };
    engine.store.pool.close().await;
    drop(engine);
    let engine = Engine::open(&url).await.unwrap();
    let saved = command(&engine, request.clone()).await;
    engine.store.pool.close().await;
    drop(engine);
    let engine = Engine::open(&url).await.unwrap();
    assert_eq!(command(&engine, request).await, saved);
    assert_eq!(
        command(
            &engine,
            Command::InspectSaved {
                record: saved["record_uid"].as_str().unwrap().into()
            }
        )
        .await["item"]["revision"],
        "1"
    );
}

#[tokio::test]
async fn clearing_temporary_content_discards_pending_reviews_and_preserves_applied_receipts() {
    let engine = Engine::open_memory().await.unwrap();
    read_from(&engine, "test", "reference").await;
    let preview = command(
        &engine,
        Command::PreviewSave {
            source: "test".into(),
            item: "reference".into(),
            organ: organ(&engine).await,
            selection: Default::default(),
        },
    )
    .await;
    command(
        &engine,
        Command::ClearCache {
            source: Some("test".into()),
        },
    )
    .await;
    assert!(
        engine
            .act(
                Action::Knowledge {
                    request: Command::Save {
                        preview: preview["preview_uid"].as_str().unwrap().into()
                    }
                },
                None
            )
            .await
            .is_err()
    );
    let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
    assert!(
        store::knowledge::check_review_capacity_on(
            &mut tx,
            "local",
            nucleus::knowledge::CACHE_LIMIT
        )
        .await
        .is_err()
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn importing_plain_knowledge_uses_destination_permissions_without_meaning_admin_rights() {
    let engine = Engine::open_memory().await.unwrap();
    read_from(&engine, "test", "reference").await;
    let uid = save(&engine, "test", "reference").await;
    let package = command(&engine, Command::Export { record: uid }).await["package"].to_string();
    let receiver = Engine::open_memory().await.unwrap();
    let person = store::records::create(
        &receiver.store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Person,
            head: "Knowledge writer",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid;
    let role = store::auth::ensure_role(&receiver.store.pool, "Knowledge writers")
        .await
        .unwrap();
    for (namespace, verb) in [
        ("record", "read"),
        ("record", "create"),
        ("record", "update"),
        ("organ", "update"),
    ] {
        let permission = store::auth::ensure_permission(&receiver.store.pool, namespace, verb)
            .await
            .unwrap();
        store::auth::grant(&receiver.store.pool, role, permission)
            .await
            .unwrap();
    }
    receiver
        .act(
            Action::AssignRoles {
                person: person.clone(),
                roles: vec!["Knowledge writers".into()],
                expected_revision: 0,
            },
            None,
        )
        .await
        .unwrap();
    let result = receiver
        .act(
            Action::Knowledge {
                request: Command::Import {
                    package,
                    organ: organ(&receiver).await,
                },
            },
            Some(person),
        )
        .await
        .unwrap();
    assert!(result.data.unwrap()["record_uid"].is_string());
}

async fn command(engine: &Engine, request: Command) -> Value {
    engine
        .act(Action::Knowledge { request }, None)
        .await
        .unwrap()
        .data
        .unwrap()
}

async fn complete(engine: &Engine, id: &str) -> Value {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let data = command(
                engine,
                Command::Poll {
                    request_id: id.into(),
                },
            )
            .await;
            if data["complete"] == true {
                return data;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn test_source_search_and_read_preserve_claims_and_isolate_disposable_cache() {
    let engine = Engine::open_memory().await.unwrap();
    assert_eq!(
        command(&engine, Command::Sources).await["sources"][0]["id"],
        "test"
    );
    let search = nucleus::new_uid("kr");
    command(
        &engine,
        Command::Search {
            request_id: search.clone(),
            sources: vec!["test".into()],
            text: "Temperature".into(),
            offset: 0,
        },
    )
    .await;
    let result = complete(&engine, &search).await;
    assert_eq!(result["status"], "complete");
    assert_eq!(result["result"]["sources"][0]["items"][0]["id"], "reading");
    let read = nucleus::new_uid("kr");
    command(
        &engine,
        Command::Read {
            request_id: read.clone(),
            source: "test".into(),
            item: "reading".into(),
        },
    )
    .await;
    let result = complete(&engine, &read).await;
    assert_eq!(
        result["result"]["item"]["claims"][0]["value"]["amount"],
        "21.5"
    );
    assert!(result["result"]["item"]["claims"][0]["conditions"].is_object());
    assert_eq!(
        result["result"]["item"]["claims"][0]["references"][0]["id"],
        "reference"
    );
    let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
    assert!(
        store::knowledge::cached_on(&mut tx, "another-person", "test", "reading")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        store::knowledge::cached_on(&mut tx, "local", "test", "reading")
            .await
            .unwrap()
            .is_some()
    );
    tx.commit().await.unwrap();
    command(&engine, Command::ClearCache { source: None }).await;
    assert_eq!(command(&engine, Command::Sources).await["cache_bytes"], 0);
}

#[tokio::test]
async fn cancellations_limits_and_source_access_checks_do_not_save_organ_records() {
    let engine = Engine::open_memory().await.unwrap();
    let records = store::records::list_all(&engine.store.pool)
        .await
        .unwrap()
        .len();
    let id = nucleus::new_uid("kr");
    command(
        &engine,
        Command::Read {
            request_id: id.clone(),
            source: "test".into(),
            item: "reading".into(),
        },
    )
    .await;
    command(
        &engine,
        Command::Cancel {
            request_id: id.clone(),
        },
    )
    .await;
    let state = complete(&engine, &id).await;
    assert!(state["status"] == "cancelled" || state["status"] == "complete");
    assert!(
        engine
            .act(
                Action::Knowledge {
                    request: Command::Search {
                        request_id: nucleus::new_uid("kr"),
                        sources: vec!["test".into()],
                        text: "x".repeat(257),
                        offset: 0
                    }
                },
                None
            )
            .await
            .is_err()
    );
    assert!(
        engine
            .act(
                Action::Knowledge {
                    request: Command::Read {
                        request_id: nucleus::new_uid("kr"),
                        source: "private-source".into(),
                        item: "anything".into()
                    }
                },
                None
            )
            .await
            .is_err()
    );
    assert_eq!(
        store::records::list_all(&engine.store.pool)
            .await
            .unwrap()
            .len(),
        records
    );
}

#[tokio::test]
async fn offline_archive_installation_is_idempotent_and_private() {
    let engine = Engine::open_memory().await.unwrap();
    let archive = engine::knowledge::provider::test_source().archive;
    let package = serde_json::to_string(&archive).unwrap();
    let first = command(
        &engine,
        Command::InstallArchive {
            package: package.clone(),
        },
    )
    .await["source_uid"]
        .as_str()
        .unwrap()
        .to_owned();
    let second = command(&engine, Command::InstallArchive { package }).await["source_uid"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(first, second);
    assert!(
        store::knowledge::archive(&engine.store.pool, "another-person", &first)
            .await
            .unwrap()
            .is_none()
    );
    let id = nucleus::new_uid("kr");
    command(
        &engine,
        Command::Read {
            request_id: id.clone(),
            source: first.clone(),
            item: "reading".into(),
        },
    )
    .await;
    let result = complete(&engine, &id).await;
    assert_eq!(result["result"]["item"]["source"], first);
    assert_eq!(result["result"]["item"]["claims"][0]["source"], first);
}

async fn read(engine: &Engine, item: &str) {
    let id = nucleus::new_uid("kr");
    command(
        engine,
        Command::Read {
            request_id: id.clone(),
            source: "test".into(),
            item: item.into(),
        },
    )
    .await;
    assert_eq!(complete(engine, &id).await["status"], "complete");
}

#[tokio::test]
async fn saving_is_atomic_reuses_references_and_keeps_meanings_and_annotations_offline() {
    let engine = Engine::open_memory().await.unwrap();
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    read(&engine, "reading").await;
    let count = store::records::list_all(&engine.store.pool)
        .await
        .unwrap()
        .len();
    let preview = command(
        &engine,
        Command::PreviewSave {
            source: "test".into(),
            item: "reading".into(),
            organ: organ.clone(),
            selection: Default::default(),
        },
    )
    .await;
    assert_eq!(preview["preview"]["can_save"], false);
    assert!(
        engine
            .act(
                Action::Knowledge {
                    request: Command::Save {
                        preview: preview["preview_uid"].as_str().unwrap().into()
                    }
                },
                None
            )
            .await
            .is_err()
    );
    assert_eq!(
        store::records::list_all(&engine.store.pool)
            .await
            .unwrap()
            .len(),
        count
    );
    let reference = command(
        &engine,
        Command::Reference {
            source: "test".into(),
            item: "reading".into(),
            organ: organ.clone(),
        },
    )
    .await["record_uid"]
        .as_str()
        .unwrap()
        .to_owned();
    engine
        .act(
            Action::EditRecordText {
                target: reference.clone(),
                head: None,
                body: Some("My annotation".into()),
            },
            None,
        )
        .await
        .unwrap();
    let concept = engine
        .act(
            Action::Vocabulary {
                request: nucleus::vocabulary::Command::CreateConcept {
                    vocabulary: "g_local".into(),
                    name: "measured temperature".into(),
                    language: "en".into(),
                    definition: "A temperature measurement with its unit, time and reference"
                        .into(),
                    derived_from: None,
                },
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    command(
        &engine,
        Command::MapMeaning {
            source: "test".into(),
            external: "measurement".into(),
            kind: nucleus::knowledge::MeaningKind::Predicate,
            concept: concept.clone(),
        },
    )
    .await;
    read(&engine, "reference").await;
    let preview = command(
        &engine,
        Command::PreviewSave {
            source: "test".into(),
            item: "reading".into(),
            organ,
            selection: Default::default(),
        },
    )
    .await;
    assert_eq!(preview["preview"]["can_save"], true);
    let preview_uid = preview["preview_uid"].as_str().unwrap().to_owned();
    let saved = command(
        &engine,
        Command::Save {
            preview: preview_uid.clone(),
        },
    )
    .await;
    assert_eq!(saved["record_uid"], reference);
    assert_eq!(saved["saved_records"].as_array().unwrap().len(), 2);
    command(&engine, Command::ClearCache { source: None }).await;
    engine
        .act(
            Action::Vocabulary {
                request: nucleus::vocabulary::Command::ReviseDefinition {
                    concept: concept.clone(),
                    expected_revision: 1,
                    definition: "An updated future definition".into(),
                },
            },
            None,
        )
        .await
        .unwrap();
    let offline = command(
        &engine,
        Command::InspectSaved {
            record: reference.clone(),
        },
    )
    .await;
    assert_eq!(offline["item"]["claims"][0]["value"]["amount"], "21.5");
    assert_eq!(offline["mappings"][0]["concept_uid"], concept);
    assert_eq!(offline["mappings"][0]["definition_revision"], 1);
    assert_eq!(offline["record"]["body"], "My annotation");
    assert_eq!(offline["offline"], true);
    assert_eq!(
        command(
            &engine,
            Command::Save {
                preview: preview_uid
            }
        )
        .await,
        saved
    );
}
