use engine::{
    Engine,
    pairing::EnrolmentInvite,
    roster::{CellEntry, ROOT_KEY_ID, full_capabilities},
    trust::Signer,
};
use nucleus::social::{
    Command, PostDraft, PostState, PublicRequest, Search, ServiceSettings, Snippet,
};
use serde_json::{Value, json};

async fn command(engine: &Engine, request: Command) -> Value {
    engine
        .social_command(request, None, nucleus::execution::now())
        .await
        .unwrap()
        .data
        .unwrap()
}

async fn publish(
    engine: &Engine,
    record: Option<String>,
    host: &str,
    title: &str,
) -> (String, Snippet) {
    let saved = command(
        engine,
        Command::SaveDraft {
            record,
            source: None,
            draft: PostDraft {
                title: title.into(),
                alias: "Deliberately shared test pseudonym".into(),
                redistribute: true,
                destinations: vec![host.into()],
                ..Default::default()
            },
        },
    )
    .await;
    let record = saved["record"].as_str().unwrap().to_owned();
    let preview = command(
        engine,
        Command::Preview {
            record: record.clone(),
            state: PostState::Active,
        },
    )
    .await;
    let document: Snippet = serde_json::from_value(preview["document"].clone()).unwrap();
    command(
        engine,
        Command::Publish {
            record: record.clone(),
            preview_hash: preview["preview_hash"].as_str().unwrap().into(),
            document: document.clone(),
        },
    )
    .await;
    (record, document)
}

async fn cache(engine: &Engine, document: &Snippet) {
    engine::social::validate_snippet(document, nucleus::execution::now().timestamp()).unwrap();
    let hash = engine::social::document_hash("snippet", document).unwrap();
    let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
    store::social::put_snippet_on(
        &mut tx,
        document,
        &hash,
        "selected directory",
        nucleus::execution::now().timestamp(),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

async fn copy(source: &Engine, target: &Engine, organ: &str) {
    for _ in 0..4 {
        let vector = store::sync_ops::version_vector_for_organ(&target.store.pool, organ)
            .await
            .unwrap();
        let page = source.export_sync_page(organ, &vector, 2000).await.unwrap();
        if page.batch.ops.is_empty() {
            break;
        }
        target.receive_sync_batch(organ, &page.batch).await.unwrap();
    }
}

#[tokio::test]
async fn merged_mute_overflow_remains_paged_and_clearable_without_dropping_other_targets() {
    let engine = Engine::open_memory().await.unwrap();
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let mut map = serde_json::Map::new();
    for _ in 0..257 {
        let target = nucleus::new_uid("post");
        let key = format!(
            "mute_{}",
            nucleus::fact::sha256_hex(format!("post\n{target}").as_bytes())
        );
        map.insert(
            key,
            json!({"kind":"post","target":target,"title":"Hidden public post"}),
        );
    }
    store::records::set_extension(
        &engine.store.pool,
        &organ,
        "lince.social.mutes",
        &Value::Object(map.clone()),
    )
    .await
    .unwrap();
    let page = command(&engine, Command::Mutes { after: None }).await;
    assert_eq!(page["total"], 257);
    assert_eq!(page["over_limit"], true);
    assert_eq!(page["mutes"].as_object().unwrap().len(), 32);
    let after = page["next_after"].as_str().unwrap().to_owned();
    let next = command(
        &engine,
        Command::Mutes {
            after: Some(after.clone()),
        },
    )
    .await;
    assert!(
        next["mutes"]
            .as_object()
            .unwrap()
            .keys()
            .all(|key| key > &after)
    );
    let key = page["mutes"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    let cleared = command(&engine, Command::Unmute { key: key.clone() }).await;
    assert_eq!(cleared["over_limit"], false);
    map.remove(&key);
    let retained = store::records::get_extension(&engine.store.pool, &organ, "lince.social.mutes")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retained, Value::Object(map));
}

#[tokio::test]
async fn private_author_hiding_and_unmute_reuse_enrolled_device_sync_without_foreign_disclosure() {
    let source = Engine::open_memory().await.unwrap();
    let second = Engine::open_memory().await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    source.set_sealing_keyring_path(directory.path().join("owner/sealing.json"));
    let path = directory.path().join("root.key");
    std::fs::write(&path, [174; 32]).unwrap();
    source.set_root_key_path(path);
    let organ = store::organs::local(&source.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let root = Signer::from_bytes(&organ, ROOT_KEY_ID, [174; 32]);
    let mut cells = Vec::new();
    let mut keys = Vec::new();
    for (engine, label) in [(&source, "Owner"), (&second, "Other device")] {
        let cell = store::cells::local(&engine.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let key = engine.operational_key_for(&organ).await.unwrap();
        cells.push(CellEntry {
            cell_uid: cell,
            node_id: label.into(),
            label: label.into(),
            operational_key: key.public_key_b64(),
            sealing_key: None,
            front_door: false,
            capabilities: full_capabilities(),
        });
        keys.push(key);
    }
    source.set_organ_signer(keys[0].clone()).await.unwrap();
    source.set_signer(keys[0].clone()).await.unwrap();
    source.publish_root_key(&root).await.unwrap();
    let roster = source.publish_roster(&root, cells).await.unwrap();
    second
        .join_organ(
            &EnrolmentInvite {
                node_id: "Owner".into(),
                organ_uid: organ.clone(),
                root_key: root.public_key_b64(),
                token: "private-mutes".into(),
                addrs: vec![],
            },
            &roster,
            keys[1].clone(),
        )
        .await
        .unwrap();
    second.set_signer(keys[1].clone()).await.unwrap();
    let host = iroh::SecretKey::from_bytes(&[175; 32]).public().to_string();
    let (_, first) = publish(&source, None, &host, "Bicycle tools").await;
    let (_, other) = publish(&source, None, &host, "Bicycle lessons").await;
    assert_eq!(
        first.anonymous.as_ref().unwrap().owner_key,
        other.anonymous.as_ref().unwrap().owner_key
    );
    let hidden = command(
        &source,
        Command::MutePost {
            post: first.id.clone(),
            whole_author: true,
        },
    )
    .await;
    assert_eq!(hidden["mutes"].as_object().unwrap().len(), 1);
    assert!(!hidden.to_string().contains(&organ));
    let key = hidden["mutes"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    let page = command(
        &source,
        Command::Search {
            query: Search::default(),
            services: vec![],
        },
    )
    .await;
    assert!(page["results"].as_array().unwrap().is_empty());
    assert!(page["next_after"].is_string());
    copy(&source, &second, &organ).await;
    cache(&second, &first).await;
    cache(&second, &other).await;
    assert_eq!(
        command(&second, Command::Mutes { after: None }).await["mutes"],
        hidden["mutes"]
    );
    assert!(
        command(
            &second,
            Command::Search {
                query: Search::default(),
                services: vec![]
            }
        )
        .await["results"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let contact = store::organs::add_contact(
        &source.store.pool,
        &nucleus::new_uid("r"),
        None,
        "Foreign contact",
        "",
        1,
    )
    .await
    .unwrap();
    store::organs::set_trust(&source.store.pool, &contact, "known")
        .await
        .unwrap();
    let export = source.export_sync_page(&contact, &[], 2000).await.unwrap();
    assert!(
        !export
            .batch
            .ops
            .iter()
            .any(|op| op.field.starts_with("lince.social.mutes"))
    );
    command(&source, Command::Unmute { key }).await;
    copy(&source, &second, &organ).await;
    assert!(
        command(&second, Command::Mutes { after: None }).await["mutes"]
            .as_object()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        command(
            &second,
            Command::Search {
                query: Search::default(),
                services: vec![]
            }
        )
        .await["results"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn host_removal_survives_new_revisions_and_reopen_while_withdrawals_still_refresh_known_results()
 {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        directory.path().join("host.sqlite").display()
    );
    let host = Engine::open(&url).await.unwrap();
    command(
        &host,
        Command::ConfigureServices {
            settings: ServiceSettings {
                directory: true,
                townsquare: true,
                ..Default::default()
            },
        },
    )
    .await;
    let author = Engine::open_memory().await.unwrap();
    let source = iroh::SecretKey::from_bytes(&[176; 32]).public().to_string();
    let endpoint = iroh::SecretKey::from_bytes(&[177; 32]).public().to_string();
    let (record, first) = publish(&author, None, &endpoint, "Bicycle tools").await;
    host.social_public_request(
        &source,
        &endpoint,
        PublicRequest::PublishSnippet {
            document: first.clone(),
        },
        nucleus::execution::now().timestamp(),
    )
    .await
    .unwrap();
    command(
        &host,
        Command::RemoveListing {
            post: first.id.clone(),
            reason: "Host policy".into(),
        },
    )
    .await;
    let (_, next) = publish(
        &author,
        Some(record.clone()),
        &endpoint,
        "Revised bicycle tools",
    )
    .await;
    host.social_public_request(
        &source,
        &endpoint,
        PublicRequest::PublishSnippet {
            document: next.clone(),
        },
        nucleus::execution::now().timestamp(),
    )
    .await
    .unwrap();
    let known = format!(
        "{}:{}",
        first.id,
        engine::social::document_hash("snippet", &first).unwrap()
    );
    let response = host
        .social_public_request(
            &source,
            &endpoint,
            PublicRequest::Search {
                query: Search::default(),
                known: vec![known.clone()],
            },
            nucleus::execution::now().timestamp(),
        )
        .await
        .unwrap();
    assert!(response["results"].as_array().unwrap().is_empty());
    assert!(response["updates"].as_array().unwrap().is_empty());
    assert_eq!(response["posting_authorities"].as_array().unwrap().len(), 1);
    drop(host);
    let host = Engine::open(&url).await.unwrap();
    assert_eq!(
        command(&host, Command::RemovedListings { after: None }).await["removed_listings"][0]["post"],
        first.id
    );
    command(&host, Command::RebuildPublicIndex).await;
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_search")
            .fetch_one(&host.store.pool)
            .await
            .unwrap(),
        0
    );
    command(
        &host,
        Command::RestoreListing {
            post: first.id.clone(),
        },
    )
    .await;
    assert_eq!(
        command(
            &host,
            Command::Search {
                query: Search::default(),
                services: vec![]
            }
        )
        .await["results"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    command(
        &host,
        Command::RemoveListing {
            post: first.id.clone(),
            reason: "Host policy".into(),
        },
    )
    .await;
    let preview = command(
        &author,
        Command::Preview {
            record: record.clone(),
            state: PostState::Withdrawn,
        },
    )
    .await;
    let withdrawn: Snippet = serde_json::from_value(preview["document"].clone()).unwrap();
    host.social_public_request(
        &source,
        &endpoint,
        PublicRequest::PublishSnippet {
            document: withdrawn,
        },
        nucleus::execution::now().timestamp(),
    )
    .await
    .unwrap();
    let response = host
        .social_public_request(
            &source,
            &endpoint,
            PublicRequest::Search {
                query: Search::default(),
                known: vec![known],
            },
            nucleus::execution::now().timestamp(),
        )
        .await
        .unwrap();
    assert_eq!(response["updates"][0]["document"]["state"], "withdrawn");
    assert!(
        host.social_command(
            Command::RestoreListing {
                post: first.id.clone()
            },
            None,
            nucleus::execution::now()
        )
        .await
        .is_err()
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_listing_removal")
            .fetch_one(&host.store.pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_ended_post")
            .fetch_one(&host.store.pool)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn expired_restoration_rolls_back_and_private_controls_require_settings_permission() {
    let engine = Engine::open_memory().await.unwrap();
    let host = iroh::SecretKey::from_bytes(&[178; 32]).public().to_string();
    let (_, document) = publish(&engine, None, &host, "Bicycle tools").await;
    command(
        &engine,
        Command::RemoveListing {
            post: document.id.clone(),
            reason: "Review".into(),
        },
    )
    .await;
    let future =
        nucleus::execution::Execution::new([179; 32], (document.expires_at + 1) * 1000).unwrap();
    assert!(
        future
            .scope(engine.social_command(
                Command::RestoreListing {
                    post: document.id.clone()
                },
                None,
                chrono::DateTime::from_timestamp(document.expires_at + 1, 0).unwrap()
            ))
            .await
            .is_err()
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_listing_removal")
            .fetch_one(&engine.store.pool)
            .await
            .unwrap(),
        1
    );
    let role = store::auth::ensure_role(&engine.store.pool, "public-viewer")
        .await
        .unwrap();
    let permission = store::auth::ensure_permission(&engine.store.pool, "view", "stream")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, permission)
        .await
        .unwrap();
    let actor = store::auth::create_person_login(
        &engine.store.pool,
        "Viewer",
        "public-viewer",
        "hash",
        role,
    )
    .await
    .unwrap();
    for request in [
        Command::Mutes { after: None },
        Command::RemovedListings { after: None },
        Command::MutePost {
            post: document.id.clone(),
            whole_author: false,
        },
        Command::RemoveListing {
            post: document.id.clone(),
            reason: "Unauthorized".into(),
        },
    ] {
        assert!(
            engine
                .social_command(request, Some(&actor), nucleus::execution::now())
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn hidden_post_filters_saved_contact_answers_without_removing_signed_cache_or_ending_floors()
{
    let engine = Engine::open_memory().await.unwrap();
    let host = iroh::SecretKey::from_bytes(&[180; 32]).public().to_string();
    let (_, document) = publish(&engine, None, &host, "Bicycle tools").await;
    let id = nucleus::new_uid("ask");
    let now = nucleus::execution::now().timestamp();
    let request = nucleus::social::ask::Request {
        id: id.clone(),
        query: Search::default(),
        issued_at: now,
        deadline: now + 30,
        work: 12,
        bytes: 192 * 1024,
        results: 50,
        depth: 2,
    };
    let results = json!([{"document":document,"source":"Saved contact answer"}]);
    store::sqlx::query("INSERT INTO social_ask_query(id,request,peers,deadline,state,results) VALUES(?,?,'[]',?,'completed',?)")
        .bind(&id).bind(serde_json::to_string(&request).unwrap()).bind(now+30).bind(results.to_string()).execute(&engine.store.pool).await.unwrap();
    assert_eq!(
        command(&engine, Command::AskResults { id: id.clone() }).await["results"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let muted = command(
        &engine,
        Command::MutePost {
            post: document.id.clone(),
            whole_author: false,
        },
    )
    .await;
    assert!(
        command(&engine, Command::AskResults { id: id.clone() }).await["results"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM social_document WHERE kind='snippet'"
        )
        .fetch_one(&engine.store.pool)
        .await
        .unwrap(),
        1
    );
    let key = muted["mutes"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    command(&engine, Command::Unmute { key }).await;
    assert_eq!(
        command(&engine, Command::AskResults { id }).await["results"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
