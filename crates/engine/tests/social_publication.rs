use engine::{
    Engine,
    actions::Action,
    social::{document_hash, validate_snippet},
};
use nucleus::social::*;
use serde_json::{Value, json};
use std::sync::Arc;

#[path = "social_publication/profile_devices.rs"]
mod profile_devices;

#[path = "social_publication/ending_recovery.rs"]
mod ending_recovery;

async fn command(engine: &Engine, request: Command) -> Value {
    engine
        .act(Action::Social { request }, None)
        .await
        .unwrap()
        .data
        .unwrap()
}

async fn plain_source(engine: &Engine) -> String {
    store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Plain,
            head: "Bicycle help",
            body: "Original reviewed description",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid
}

#[tokio::test]
async fn post_identity_cannot_be_reassigned_after_cache_expiry() {
    let engine = Engine::open_memory().await.unwrap();
    let uid = save(&engine, draft()).await;
    let (_, doc) = preview(&engine, &uid, PostState::Active).await;
    let impostor = engine::trust::Signer::from_bytes("", "social", [131; 32]);
    let mut changed = doc.clone();
    changed.signing_key = impostor.public_key_b64();
    changed.signature =
        impostor.sign_bytes(&engine::social::signing_bytes("snippet", &changed).unwrap());
    assert!(validate_snippet(&changed, doc.issued_at).is_err());
    changed = doc.clone();
    changed.destinations = vec![iroh::SecretKey::from_bytes(&[132; 32]).public().to_string()];
    assert!(validate_snippet(&changed, doc.issued_at).is_err());
    changed = doc.clone();
    changed.alias = "different alias".into();
    assert!(validate_snippet(&changed, doc.issued_at).is_err());
    validate_snippet(&doc, doc.issued_at).unwrap();
}

#[tokio::test]
async fn malformed_frames_spend_budget_before_parsing_and_directory_search_is_available() {
    let host = Engine::open_memory().await.unwrap();
    command(
        &host,
        Command::ConfigureServices {
            settings: ServiceSettings {
                directory: true,
                ..Default::default()
            },
        },
    )
    .await;
    let now = nucleus::execution::now().timestamp();
    let peer = iroh::SecretKey::from_bytes(&[133; 32]).public().to_string();
    let node = iroh::SecretKey::from_bytes(&[134; 32]).public().to_string();
    assert!(
        host.social_public_frame(&peer, &node, b"{bad", now)
            .await
            .is_err()
    );
    let spent: i64 = store::sqlx::query_scalar(
        "SELECT work FROM social_service_budget WHERE source=? AND direction='in'",
    )
    .bind(&peer)
    .fetch_one(&host.store.pool)
    .await
    .unwrap();
    assert_eq!(spent, 1);
    host.social_public_request(
        &peer,
        &node,
        PublicRequest::Search {
            query: Search {
                text: "bicycle".into(),
                ..Default::default()
            },
            known: vec![],
        },
        now,
    )
    .await
    .unwrap();
    assert!(
        host.social_public_request(
            &peer,
            &node,
            PublicRequest::Search {
                query: Search::default(),
                known: vec![]
            },
            now
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn source_control_characters_are_sanitized_and_private_labels_are_refused() {
    let engine = Engine::open_memory().await.unwrap();
    let source = plain_source(&engine).await;
    store::records::set_text(
        &engine.store.pool,
        &source,
        Some("Bicycle\r\nhelp"),
        Some("Review\0 this\ttext"),
    )
    .await
    .unwrap();
    let projected = command(&engine, Command::PrepareFromRecord { source }).await;
    let input: PostDraft = serde_json::from_value(projected["draft"].clone()).unwrap();
    assert_eq!(input.title, "Bicycle\nhelp");
    assert_eq!(input.text, "Review this\ttext");
    input.validate().unwrap();
    for prefix in ["r_", "c_", "p_", "pl_"] {
        let mut changed = input.clone();
        changed.concept = Some(format!("{prefix}private"));
        assert!(changed.validate().is_err());
    }
}

#[tokio::test]
async fn rotating_public_editor_preserves_identity_and_allows_old_post_withdrawal() {
    let engine = Engine::open_memory().await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    let root_path = directory.path().join("root.key");
    std::fs::write(&root_path, [135; 32]).unwrap();
    engine.set_root_key_path(root_path);
    let first = command(
        &engine,
        Command::SaveProfile {
            fields: ProfileFields {
                name: "Workshop".into(),
                ..Default::default()
            },
            parents: vec![],
            destinations: vec![],
        },
    )
    .await;
    let mut input = draft();
    input.mode = AuthorMode::Identified;
    let uid = save(&engine, input).await;
    let (hash, doc) = preview(&engine, &uid, PostState::Active).await;
    command(
        &engine,
        Command::Publish {
            record: uid.clone(),
            preview_hash: hash,
            document: doc.clone(),
        },
    )
    .await;
    command(&engine, Command::RotateProfileAuthority).await;
    assert!(
        engine
            .act(
                Action::Social {
                    request: Command::Preview {
                        record: uid.clone(),
                        state: PostState::Active
                    }
                },
                None
            )
            .await
            .is_err()
    );
    let (hash, ending) = preview(&engine, &uid, PostState::Withdrawn).await;
    assert_eq!(ending.signing_key, doc.signing_key);
    assert_eq!(ending.id, doc.id);
    command(
        &engine,
        Command::Publish {
            record: uid,
            preview_hash: hash,
            document: ending,
        },
    )
    .await;
    let second = command(
        &engine,
        Command::SaveProfile {
            fields: ProfileFields {
                name: "Updated workshop".into(),
                ..Default::default()
            },
            parents: vec![first["hash"].as_str().unwrap().into()],
            destinations: vec![],
        },
    )
    .await;
    assert_eq!(
        first["profile"]["authority"]["organ"],
        second["profile"]["authority"]["organ"]
    );
    assert_ne!(
        first["profile"]["authority"]["editor_key"],
        second["profile"]["authority"]["editor_key"]
    );
    assert_eq!(second["profile"]["authority"]["generation"], "2");
    let old: Profile = serde_json::from_value(first["profile"].clone()).unwrap();
    let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
    assert!(
        store::social::put_profile_on(&mut tx, &old, first["hash"].as_str().unwrap(), "replay")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn profile_identity_pin_and_revision_floor_survive_display_cache_pruning() {
    let engine = Engine::open_memory().await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("root.key");
    std::fs::write(&path, [136; 32]).unwrap();
    engine.set_root_key_path(path);
    let first = command(
        &engine,
        Command::SaveProfile {
            fields: ProfileFields {
                name: "Workshop".into(),
                ..Default::default()
            },
            parents: vec![],
            destinations: vec![],
        },
    )
    .await;
    let second = command(
        &engine,
        Command::SaveProfile {
            fields: ProfileFields {
                name: "Updated".into(),
                ..Default::default()
            },
            parents: vec![first["hash"].as_str().unwrap().into()],
            destinations: vec![],
        },
    )
    .await;
    let second: Profile = serde_json::from_value(second["profile"].clone()).unwrap();
    store::social::prune(&engine.store.pool, second.expires_at + 601)
        .await
        .unwrap();
    let old: Profile = serde_json::from_value(first["profile"].clone()).unwrap();
    let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
    assert!(
        !store::social::put_profile_on(&mut tx, &old, first["hash"].as_str().unwrap(), "replay")
            .await
            .unwrap()
    );
    let impostor = engine::trust::Signer::from_bytes("", "root", [137; 32]);
    let mut changed = second.clone();
    changed.authority.root_key = impostor.public_key_b64();
    changed.authority.signature = impostor.sign_bytes(
        &engine::social::signing_bytes("profile-authority", &changed.authority).unwrap(),
    );
    assert!(
        store::social::anchor_profile_authority_on(&mut tx, &changed.authority, false)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn a_profile_host_can_catch_up_after_missing_intermediate_edits() {
    let owner = Engine::open_memory().await.unwrap();
    let host = Engine::open_memory().await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("root.key");
    std::fs::write(&path, [143; 32]).unwrap();
    owner.set_root_key_path(path);
    command(
        &host,
        Command::ConfigureServices {
            settings: ServiceSettings {
                directory: true,
                ..Default::default()
            },
        },
    )
    .await;
    let node = iroh::SecretKey::from_bytes(&[144; 32]).public().to_string();
    let peer = iroh::SecretKey::from_bytes(&[145; 32]).public().to_string();
    let mut previous = command(
        &owner,
        Command::SaveProfile {
            fields: ProfileFields {
                name: "First".into(),
                ..Default::default()
            },
            parents: vec![],
            destinations: vec![node.clone()],
        },
    )
    .await;
    let first_hash = previous["hash"].as_str().unwrap().to_owned();
    host.social_public_request(
        &peer,
        &node,
        PublicRequest::PublishProfile {
            document: serde_json::from_value(previous["profile"].clone()).unwrap(),
        },
        nucleus::execution::now().timestamp(),
    )
    .await
    .unwrap();
    for index in 0..4 {
        previous = command(
            &owner,
            Command::SaveProfile {
                fields: ProfileFields {
                    name: format!("Edit {index}"),
                    ..Default::default()
                },
                parents: vec![previous["hash"].as_str().unwrap().to_owned()],
                destinations: vec![node.clone()],
            },
        )
        .await;
    }
    let latest: Profile = serde_json::from_value(previous["profile"].clone()).unwrap();
    assert!(latest.parents.contains(&first_hash));
    let receipt = host
        .social_public_request(
            &peer,
            &node,
            PublicRequest::PublishProfile { document: latest },
            nucleus::execution::now().timestamp(),
        )
        .await
        .unwrap();
    assert_eq!(receipt["accepted"], true);
    assert_eq!(receipt["hash"], previous["hash"]);
    assert_eq!(receipt["state"], "active");
}

#[tokio::test]
async fn source_changes_prepare_a_review_candidate_without_republishing() {
    let engine = Engine::open_memory().await.unwrap();
    let source = plain_source(&engine).await;
    let prepared = command(
        &engine,
        Command::PrepareFromRecord {
            source: source.clone(),
        },
    )
    .await;
    let mut input: PostDraft = serde_json::from_value(prepared["draft"].clone()).unwrap();
    input.alias = "Workshop".into();
    input.language = "pt".into();
    let saved = command(
        &engine,
        Command::SaveDraft {
            record: None,
            source: Some(source.clone()),
            draft: input.clone(),
        },
    )
    .await;
    let uid = saved["record"].as_str().unwrap().to_owned();
    let (hash, published) = preview(&engine, &uid, PostState::Active).await;
    command(
        &engine,
        Command::Publish {
            record: uid.clone(),
            preview_hash: hash,
            document: published.clone(),
        },
    )
    .await;
    store::records::set_text(
        &engine.store.pool,
        &source,
        Some("Updated bicycle need"),
        Some("New private source description"),
    )
    .await
    .unwrap();
    let overview = command(&engine, Command::Overview).await;
    let post = overview["posts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|post| post["record"] == uid)
        .unwrap();
    assert_eq!(post["draft"], json!(input));
    assert_eq!(post["published"], json!(published));
    assert_eq!(post["source_draft"]["title"], "Updated bicycle need");
    assert_eq!(post["source_draft"]["alias"], "Workshop");
    assert_eq!(post["source_draft"]["language"], "pt");
    assert!(overview["jobs"].as_array().unwrap().is_empty());
    let candidate = serde_json::from_value(post["source_draft"].clone()).unwrap();
    command(
        &engine,
        Command::SaveDraft {
            record: Some(uid.clone()),
            source: Some(source),
            draft: candidate,
        },
    )
    .await;
    let overview = command(&engine, Command::Overview).await;
    assert!(overview["posts"][0]["source_draft"].is_null());
    assert_eq!(overview["posts"][0]["published"], json!(published));
}

#[tokio::test]
async fn closing_a_source_promise_keeps_existing_post_edits_and_withdrawals_available() {
    let engine = Engine::open_memory().await.unwrap();
    let source = plain_source(&engine).await;
    let owner = store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Person,
            head: "Source owner",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap();
    let promise = store::misc::insert_promise(
        &engine.store.pool,
        store::misc::NewPromise {
            record_uid: Some(source),
            state: Some(nucleus::PromiseState::Open),
            party_uid: Some(owner.uid),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let prepared = command(
        &engine,
        Command::PrepareFromRecord {
            source: promise.clone(),
        },
    )
    .await;
    let mut input: PostDraft = serde_json::from_value(prepared["draft"].clone()).unwrap();
    let saved = command(
        &engine,
        Command::SaveDraft {
            record: None,
            source: Some(promise.clone()),
            draft: input.clone(),
        },
    )
    .await;
    let uid = saved["record"].as_str().unwrap().to_owned();
    let (hash, doc) = preview(&engine, &uid, PostState::Active).await;
    command(
        &engine,
        Command::Publish {
            record: uid.clone(),
            preview_hash: hash,
            document: doc,
        },
    )
    .await;
    store::misc::set_promise_state(
        &engine.store.pool,
        &promise,
        nucleus::PromiseState::Withdrawn,
    )
    .await
    .unwrap();
    assert!(
        engine
            .act(
                Action::Social {
                    request: Command::PrepareFromRecord {
                        source: promise.clone()
                    }
                },
                None
            )
            .await
            .is_err()
    );
    input.text = "Deliberately reviewed after the source ended".into();
    command(
        &engine,
        Command::SaveDraft {
            record: Some(uid.clone()),
            source: Some(promise),
            draft: input,
        },
    )
    .await;
    let overview = command(&engine, Command::Overview).await;
    assert!(overview["posts"][0]["source_error"].is_string());
    let (hash, doc) = preview(&engine, &uid, PostState::Withdrawn).await;
    command(
        &engine,
        Command::Publish {
            record: uid,
            preview_hash: hash,
            document: doc,
        },
    )
    .await;
}

#[tokio::test]
async fn archive_requires_an_ending_and_preserves_the_public_ending_floor() {
    let engine = Engine::open_memory().await.unwrap();
    let uid = save(&engine, draft()).await;
    assert!(
        engine
            .act(
                Action::DeleteRecord {
                    target: uid.clone()
                },
                None
            )
            .await
            .is_err()
    );
    let (hash, active) = preview(&engine, &uid, PostState::Active).await;
    command(
        &engine,
        Command::Publish {
            record: uid.clone(),
            preview_hash: hash,
            document: active.clone(),
        },
    )
    .await;
    assert!(
        engine
            .act(
                Action::Social {
                    request: Command::ArchivePost {
                        record: uid.clone()
                    }
                },
                None
            )
            .await
            .is_err()
    );
    let (hash, ended) = preview(&engine, &uid, PostState::Withdrawn).await;
    command(
        &engine,
        Command::Publish {
            record: uid.clone(),
            preview_hash: hash,
            document: ended.clone(),
        },
    )
    .await;
    command(&engine, Command::ArchivePost { record: uid }).await;
    assert!(
        command(&engine, Command::Overview).await["posts"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    store::social::prune(&engine.store.pool, ended.expires_at + 601)
        .await
        .unwrap();
    let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
    assert!(
        !store::social::put_snippet_on(
            &mut tx,
            &active,
            &document_hash("snippet", &active).unwrap(),
            "replay",
            ended.expires_at + 601
        )
        .await
        .unwrap()
    );
    let retained: i64 =
        store::sqlx::query_scalar("SELECT COUNT(*) FROM social_document WHERE id=?")
            .bind(&active.id)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(retained, 0);
}

struct LocalSocialHost {
    engine: Arc<Engine>,
    node: String,
}

#[tokio::test]
async fn first_public_profile_cannot_replace_an_already_trusted_organ_identity() {
    let owner = Engine::open_memory().await.unwrap();
    let host = Engine::open_memory().await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    let root_path = directory.path().join("root.key");
    std::fs::write(&root_path, [230; 32]).unwrap();
    owner.set_root_key_path(root_path);
    let node = iroh::SecretKey::from_bytes(&[231; 32]).public().to_string();
    let source = iroh::SecretKey::from_bytes(&[232; 32]).public().to_string();
    command(
        &host,
        Command::ConfigureServices {
            settings: ServiceSettings {
                directory: true,
                ..Default::default()
            },
        },
    )
    .await;
    let saved = command(
        &owner,
        Command::SaveProfile {
            fields: ProfileFields {
                name: "Trusted workshop".into(),
                ..Default::default()
            },
            parents: vec![],
            destinations: vec![node.clone()],
        },
    )
    .await;
    let profile: Profile = serde_json::from_value(saved["profile"].clone()).unwrap();
    store::organs::add_contact(
        &host.store.pool,
        &profile.authority.organ,
        None,
        "Trusted Organ",
        "",
        1,
    )
    .await
    .unwrap();
    let root = engine::trust::Signer::from_bytes(
        &profile.authority.organ,
        engine::roster::ROOT_KEY_ID,
        [230; 32],
    );
    host.publish_root_key(&root).await.unwrap();
    let impostor = engine::trust::Signer::from_bytes(
        &profile.authority.organ,
        engine::roster::ROOT_KEY_ID,
        [233; 32],
    );
    let editor =
        engine::trust::Signer::from_bytes(&profile.authority.organ, "social-profile", [234; 32]);
    let mut changed = profile.clone();
    changed.authority.root_key = impostor.public_key_b64();
    changed.authority.editor_key = editor.public_key_b64();
    changed.authority.signature = impostor.sign_bytes(
        &engine::social::signing_bytes("profile-authority", &changed.authority).unwrap(),
    );
    changed.signature =
        editor.sign_bytes(&engine::social::signing_bytes("profile", &changed).unwrap());
    assert!(
        host.social_public_request(
            &source,
            &node,
            PublicRequest::PublishProfile { document: changed },
            profile.issued_at
        )
        .await
        .is_err()
    );
    let pins: i64 =
        store::sqlx::query_scalar("SELECT COUNT(*) FROM social_profile_authority WHERE organ=?")
            .bind(&profile.authority.organ)
            .fetch_one(&host.store.pool)
            .await
            .unwrap();
    assert_eq!(pins, 0);
    host.social_public_request(
        &source,
        &node,
        PublicRequest::PublishProfile {
            document: profile.clone(),
        },
        profile.issued_at,
    )
    .await
    .unwrap();
}

struct SelectedMailboxHosts {
    hosts: Vec<(String, Arc<Engine>)>,
    second_online: std::sync::atomic::AtomicBool,
}

#[async_trait::async_trait]
impl engine::social::Network for SelectedMailboxHosts {
    async fn request(
        &self,
        destination: &str,
        request: PublicRequest,
    ) -> Result<Value, engine::EngineError> {
        let (index, (_, host)) = self
            .hosts
            .iter()
            .enumerate()
            .find(|(_, (id, _))| id == destination)
            .unwrap();
        if index == 1 && !self.second_online.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(engine::EngineError::Consequence(
                "Offline selected host".into(),
            ));
        }
        let source = iroh::SecretKey::from_bytes(&[220; 32]).public().to_string();
        host.social_public_request(
            &source,
            destination,
            request,
            nucleus::execution::now().timestamp(),
        )
        .await
    }
}

#[tokio::test]
async fn selected_mailboxes_retry_registration_and_ending_on_a_combined_directory_host() {
    let owner = Engine::open_memory().await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    owner.set_sealing_keyring_path(directory.path().join("sealing.json"));
    let root_path = directory.path().join("root.key");
    std::fs::write(&root_path, [221; 32]).unwrap();
    owner.set_root_key_path(root_path);
    let organ = store::organs::local(&owner.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let cell = store::cells::local(&owner.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let signer =
        engine::trust::Signer::from_bytes(&organ, &engine::roster::cell_key_id(&cell), [222; 32]);
    owner.set_signer(signer.clone()).await.unwrap();
    owner.set_organ_signer(signer).await.unwrap();
    let mut hosts = Vec::new();
    for seed in [223, 224] {
        let host = Arc::new(Engine::open_memory().await.unwrap());
        command(
            &host,
            Command::ConfigureServices {
                settings: ServiceSettings {
                    directory: true,
                    mailbox: true,
                    ..Default::default()
                },
            },
        )
        .await;
        hosts.push((
            iroh::SecretKey::from_bytes(&[seed; 32])
                .public()
                .to_string(),
            host,
        ));
    }
    let services: Vec<String> = hosts.iter().map(|(id, _)| id.clone()).collect();
    let network = Arc::new(SelectedMailboxHosts {
        hosts,
        second_online: std::sync::atomic::AtomicBool::new(false),
    });
    owner.attach_social_network(network.clone());
    let mut input = draft();
    input.destinations = services.clone();
    let record = save(&owner, input).await;
    let prepared = command(
        &owner,
        Command::PrepareReplyKeys {
            record: record.clone(),
            services,
        },
    )
    .await;
    assert_eq!(prepared["reply_keys"], "ready");
    let (_, post) = preview(&owner, &record, PostState::Active).await;
    assert!(post.reply.is_some());
    command(
        &owner,
        Command::Publish {
            record: record.clone(),
            preview_hash: document_hash("snippet", &post).unwrap(),
            document: post.clone(),
        },
    )
    .await;
    owner.social_publish_once().await.unwrap();
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_reply_route WHERE post=?")
            .bind(&post.id)
            .fetch_one(&network.hosts[0].1.store.pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_reply_route")
            .fetch_one(&network.hosts[1].1.store.pool)
            .await
            .unwrap(),
        0
    );
    network
        .second_online
        .store(true, std::sync::atomic::Ordering::SeqCst);
    store::sqlx::query("UPDATE social_publication_job SET next_attempt=0 WHERE state='pending'")
        .execute(&owner.store.pool)
        .await
        .unwrap();
    owner.social_publish_once().await.unwrap();
    let (hash, ending) = preview(&owner, &record, PostState::Withdrawn).await;
    assert!(ending.reply.is_none());
    command(
        &owner,
        Command::Publish {
            record,
            preview_hash: hash,
            document: ending.clone(),
        },
    )
    .await;
    owner.social_publish_once().await.unwrap();
    for (_, host) in &network.hosts {
        let state: String =
            store::sqlx::query_scalar("SELECT state FROM social_reply_route WHERE post=?")
                .bind(&post.id)
                .fetch_one(&host.store.pool)
                .await
                .unwrap();
        assert_eq!(state, "closed");
        assert!(
            host.social_public_request(
                &network.hosts[0].0,
                &network.hosts[0].0,
                PublicRequest::RegisterReplyRoute {
                    document: post.reply.as_ref().unwrap().clone(),
                    post: Some(Box::new(post.clone()))
                },
                ending.issued_at
            )
            .await
            .is_err()
        );
    }
}

#[tokio::test]
async fn identified_ending_recovers_from_revoked_maximum_revision_without_restoring_editor() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let owner = Engine::open_memory().await.unwrap();
    let host = Engine::open_memory().await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    let root_path = directory.path().join("root.key");
    std::fs::write(&root_path, [211; 32]).unwrap();
    owner.set_root_key_path(root_path);
    command(
        &owner,
        Command::SaveProfile {
            fields: ProfileFields {
                name: "Workshop".into(),
                ..Default::default()
            },
            parents: vec![],
            destinations: vec![],
        },
    )
    .await;
    let node = iroh::SecretKey::from_bytes(&[212; 32]).public().to_string();
    let source = iroh::SecretKey::from_bytes(&[213; 32]).public().to_string();
    command(
        &host,
        Command::ConfigureServices {
            settings: ServiceSettings {
                directory: true,
                ..Default::default()
            },
        },
    )
    .await;
    let mut input = draft();
    input.mode = AuthorMode::Identified;
    input.destinations = vec![node.clone()];
    let record = save(&owner, input).await;
    let (hash, original) = preview(&owner, &record, PostState::Active).await;
    command(
        &owner,
        Command::Publish {
            record: record.clone(),
            preview_hash: hash,
            document: original.clone(),
        },
    )
    .await;
    let state = store::records::get_extension(&owner.store.pool, &record, PUBLICATION_NAMESPACE)
        .await
        .unwrap()
        .unwrap();
    let bytes: [u8; 32] = STANDARD
        .decode(state["posting_authority"]["secret"].as_str().unwrap())
        .unwrap()
        .try_into()
        .unwrap();
    let revoked = engine::trust::Signer::from_bytes("", "social", bytes);
    let mut abusive = original.clone();
    abusive.revision = i64::MAX.to_string();
    abusive.signature =
        revoked.sign_bytes(&engine::social::signing_bytes("snippet", &abusive).unwrap());
    host.social_public_request(
        &source,
        &node,
        PublicRequest::PublishSnippet {
            document: abusive.clone(),
        },
        original.issued_at,
    )
    .await
    .unwrap();
    command(&owner, Command::RotateProfileAuthority).await;
    let (_, ending) = preview(&owner, &record, PostState::Withdrawn).await;
    assert_eq!(ending.id, original.id);
    assert_eq!(ending.signing_key, original.signing_key);
    assert_eq!(ending.profile.as_ref().unwrap().generation, "2");
    host.social_public_request(
        &source,
        &node,
        PublicRequest::PublishSnippet {
            document: ending.clone(),
        },
        ending.issued_at,
    )
    .await
    .unwrap();
    let organ = store::organs::local(&owner.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let authority = store::records::get_extension(&owner.store.pool, &organ, PRIVATE_NAMESPACE)
        .await
        .unwrap()
        .unwrap()["profile_signer"]["authority"]
        .clone();
    let active: Delegation = serde_json::from_value(authority).unwrap();
    let mut tx = store::write_tx(&host.store.pool).await.unwrap();
    store::social::anchor_profile_authority_on(&mut tx, &active, false)
        .await
        .unwrap();
    store::social::anchor_profile_authority_on(&mut tx, ending.profile.as_ref().unwrap(), true)
        .await
        .unwrap();
    let held: (i64, String) = store::sqlx::query_as(
        "SELECT generation,editor_key FROM social_profile_authority WHERE organ=?",
    )
    .bind(&organ)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(held, (2, active.editor_key));
    tx.commit().await.unwrap();
    assert!(
        host.social_public_request(
            &source,
            &node,
            PublicRequest::PublishSnippet { document: abusive },
            ending.issued_at
        )
        .await
        .is_err()
    );
    let mut revived = ending.clone();
    revived.state = PostState::Active;
    revived.signature =
        revoked.sign_bytes(&engine::social::signing_bytes("snippet", &revived).unwrap());
    assert!(validate_snippet(&revived, revived.issued_at).is_err());
}

#[tokio::test]
async fn public_authority_change_reaches_selected_hosts_and_stops_revoked_edits() {
    let sender = Engine::open_memory().await.unwrap();
    let host = Arc::new(Engine::open_memory().await.unwrap());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("root.key");
    std::fs::write(&path, [138; 32]).unwrap();
    sender.set_root_key_path(path);
    let node = iroh::SecretKey::from_bytes(&[139; 32]).public().to_string();
    command(
        &host,
        Command::ConfigureServices {
            settings: ServiceSettings {
                directory: true,
                ..Default::default()
            },
        },
    )
    .await;
    let network = Arc::new(LocalSocialHost {
        engine: host.clone(),
        node: node.clone(),
    });
    sender.attach_social_network(network.clone());
    let first = command(
        &sender,
        Command::SaveProfile {
            fields: ProfileFields {
                name: "Workshop".into(),
                ..Default::default()
            },
            parents: vec![],
            destinations: vec![node.clone()],
        },
    )
    .await;
    assert_eq!(sender.social_publish_once().await.unwrap(), 1);
    command(&sender, Command::RotateProfileAuthority).await;
    assert_eq!(sender.social_publish_once().await.unwrap(), 2);
    let profile: String = store::sqlx::query_scalar(
        "SELECT body FROM social_document WHERE kind='profile' AND state='active'",
    )
    .fetch_one(&host.store.pool)
    .await
    .unwrap();
    let profile: Profile = serde_json::from_str(&profile).unwrap();
    assert_eq!(profile.authority.generation, "2");
    assert_eq!(profile.fields.name, "Workshop");
    let old: Profile = serde_json::from_value(first["profile"].clone()).unwrap();
    let peer = iroh::SecretKey::from_bytes(&[140; 32]).public().to_string();
    assert!(
        host.social_public_request(
            &peer,
            &node,
            PublicRequest::PublishProfile { document: old },
            nucleus::execution::now().timestamp()
        )
        .await
        .is_err()
    );
    command(
        &sender,
        Command::SaveProfile {
            fields: ProfileFields {
                name: "Updated".into(),
                ..Default::default()
            },
            parents: vec![document_hash("profile", &profile).unwrap()],
            destinations: vec![node.clone()],
        },
    )
    .await;
    assert_eq!(sender.social_publish_once().await.unwrap(), 1);
    let hosted = host
        .social_public_request(
            &peer,
            &node,
            PublicRequest::FetchProfile {
                organ: first["profile"]["authority"]["organ"]
                    .as_str()
                    .unwrap()
                    .into(),
            },
            nucleus::execution::now().timestamp(),
        )
        .await
        .unwrap();
    assert_eq!(hosted["profile"]["fields"]["name"], "Updated");
    assert_eq!(hosted["profile"]["authority"]["generation"], "2");
}

#[tokio::test]
async fn cached_identified_posts_are_suppressed_when_search_learns_a_new_editing_generation() {
    let sender = Engine::open_memory().await.unwrap();
    let viewer = Engine::open_memory().await.unwrap();
    let host = Arc::new(Engine::open_memory().await.unwrap());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("root.key");
    std::fs::write(&path, [181; 32]).unwrap();
    sender.set_root_key_path(path);
    let node = iroh::SecretKey::from_bytes(&[182; 32]).public().to_string();
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
    let network = Arc::new(LocalSocialHost {
        engine: host.clone(),
        node: node.clone(),
    });
    sender.attach_social_network(network.clone());
    viewer.attach_social_network(network.clone());
    command(
        &sender,
        Command::SaveProfile {
            fields: ProfileFields {
                name: "Workshop".into(),
                ..Default::default()
            },
            parents: vec![],
            destinations: vec![node.clone()],
        },
    )
    .await;
    let mut input = draft();
    input.mode = AuthorMode::Identified;
    input.destinations = vec![node.clone()];
    let uid = save(&sender, input).await;
    let (hash, document) = preview(&sender, &uid, PostState::Active).await;
    command(
        &sender,
        Command::Publish {
            record: uid,
            preview_hash: hash,
            document,
        },
    )
    .await;
    assert_eq!(sender.social_publish_once().await.unwrap(), 2);
    let search = || Command::Search {
        query: Search::default(),
        services: vec![node.clone()],
    };
    assert_eq!(
        command(&viewer, search()).await["results"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    command(&sender, Command::RotateProfileAuthority).await;
    assert_eq!(sender.social_publish_once().await.unwrap(), 2);
    let refreshed = command(&viewer, search()).await;
    assert!(refreshed["failures"].as_array().unwrap().is_empty());
    assert!(refreshed["results"].as_array().unwrap().is_empty());
    assert!(
        store::social::search(
            &host.store.pool,
            &Search::default(),
            nucleus::execution::now().timestamp()
        )
        .await
        .unwrap()
        .is_empty()
    );
}

#[tokio::test]
async fn cached_anonymous_posts_follow_owner_revocations_without_revealing_the_organ() {
    let sender = Engine::open_memory().await.unwrap();
    let viewer = Engine::open_memory().await.unwrap();
    let host = Arc::new(Engine::open_memory().await.unwrap());
    let node = iroh::SecretKey::from_bytes(&[199; 32]).public().to_string();
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
    let network = Arc::new(LocalSocialHost {
        engine: host.clone(),
        node: node.clone(),
    });
    sender.attach_social_network(network.clone());
    viewer.attach_social_network(network.clone());
    let mut input = draft();
    input.destinations = vec![node.clone()];
    let uid = save(&sender, input).await;
    let (hash, doc) = preview(&sender, &uid, PostState::Active).await;
    command(
        &sender,
        Command::Publish {
            record: uid,
            preview_hash: hash,
            document: doc,
        },
    )
    .await;
    assert_eq!(sender.social_publish_once().await.unwrap(), 1);
    let search = || Command::Search {
        query: Search::default(),
        services: vec![node.clone()],
    };
    assert_eq!(
        command(&viewer, search()).await["results"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let organ = store::organs::local(&sender.store.pool)
        .await
        .unwrap()
        .unwrap();
    store::records::set_extension(
        &sender.store.pool,
        &organ.uid,
        PRIVATE_NAMESPACE,
        &json!({"posting_revocation_version":1}),
    )
    .await
    .unwrap();
    assert_eq!(sender.social_publish_once().await.unwrap(), 1);
    let refreshed = command(&viewer, search()).await;
    assert!(refreshed["failures"].as_array().unwrap().is_empty());
    assert!(refreshed["results"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn anonymous_owner_rotation_recovers_from_a_revoked_editors_maximum_revision() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let owner = Engine::open_memory().await.unwrap();
    let host = Engine::open_memory().await.unwrap();
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
    let node = iroh::SecretKey::from_bytes(&[197; 32]).public().to_string();
    let source = iroh::SecretKey::from_bytes(&[198; 32]).public().to_string();
    let mut input = draft();
    input.destinations = vec![node.clone()];
    let uid = save(&owner, input).await;
    let (hash, original) = preview(&owner, &uid, PostState::Active).await;
    command(
        &owner,
        Command::Publish {
            record: uid.clone(),
            preview_hash: hash,
            document: original.clone(),
        },
    )
    .await;
    let state = store::records::get_extension(&owner.store.pool, &uid, PUBLICATION_NAMESPACE)
        .await
        .unwrap()
        .unwrap();
    let secret: [u8; 32] = STANDARD
        .decode(state["secret"].as_str().unwrap())
        .unwrap()
        .try_into()
        .unwrap();
    let stolen = engine::trust::Signer::from_bytes("", "social", secret);
    let mut abusive = original.clone();
    abusive.revision = i64::MAX.to_string();
    abusive.signature =
        stolen.sign_bytes(&engine::social::signing_bytes("snippet", &abusive).unwrap());
    host.social_public_request(
        &source,
        &node,
        PublicRequest::PublishSnippet {
            document: abusive.clone(),
        },
        original.issued_at,
    )
    .await
    .unwrap();
    let organ = store::organs::local(&owner.store.pool)
        .await
        .unwrap()
        .unwrap();
    store::records::set_extension(
        &owner.store.pool,
        &organ.uid,
        PRIVATE_NAMESPACE,
        &json!({"posting_revocation_version":1}),
    )
    .await
    .unwrap();
    let (_, replacement) = preview(&owner, &uid, PostState::Withdrawn).await;
    assert_eq!(replacement.id, original.id);
    assert_eq!(replacement.revision, "1");
    assert_eq!(replacement.anonymous.as_ref().unwrap().generation, "2");
    assert_ne!(replacement.signing_key, original.signing_key);
    assert_eq!(
        replacement.anonymous.as_ref().unwrap().owner_key,
        original.anonymous.as_ref().unwrap().owner_key
    );
    let receipt = host
        .social_public_request(
            &source,
            &node,
            PublicRequest::PublishSnippet {
                document: replacement.clone(),
            },
            replacement.issued_at,
        )
        .await
        .unwrap();
    assert_eq!(receipt["accepted"], true);
    assert!(
        host.social_public_request(
            &source,
            &node,
            PublicRequest::PublishSnippet { document: abusive },
            replacement.issued_at
        )
        .await
        .is_err()
    );
    let public = serde_json::to_string(&replacement).unwrap();
    assert!(!public.contains(&organ.uid));
    assert!(!public.contains(&uid));
    assert!(!public.contains(state["secret"].as_str().unwrap()));
    let floor: (i64, i64) =
        store::sqlx::query_as("SELECT generation,revision FROM social_ended_post WHERE id=?")
            .bind(&replacement.id)
            .fetch_one(&host.store.pool)
            .await
            .unwrap();
    assert_eq!(floor, (2, 1));
}

#[tokio::test]
async fn profile_edits_survive_missing_and_expired_owner_authority_before_publication() {
    let clock = nucleus::execution::Execution::new([205; 32], 1_800_000_000_000).unwrap();
    clock
        .scope(async {
            let engine = Engine::open_memory().await.unwrap();
            let local_organ = store::organs::local(&engine.store.pool)
                .await
                .unwrap()
                .unwrap();
            let operational = engine.operational_key_for(&local_organ.uid).await.unwrap();
            engine.set_organ_signer(operational.clone()).await.unwrap();
            engine.set_signer(operational).await.unwrap();
            let host = Arc::new(Engine::open_memory().await.unwrap());
            let node = iroh::SecretKey::from_bytes(&[206; 32]).public().to_string();
            command(
                &host,
                Command::ConfigureServices {
                    settings: ServiceSettings {
                        directory: true,
                        ..Default::default()
                    },
                },
            )
            .await;
            let network = Arc::new(LocalSocialHost {
                engine: host.clone(),
                node: node.clone(),
            });
            engine.attach_social_network(network.clone());
            let fields = ProfileFields {
                name: "Offline workshop".into(),
                ..Default::default()
            };
            let pending = command(
                &engine,
                Command::SaveProfile {
                    fields: fields.clone(),
                    parents: vec![],
                    destinations: vec![node.clone()],
                },
            )
            .await;
            assert_eq!(pending["profile_draft"]["fields"], json!(fields));
            assert_eq!(
                store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_publication_job")
                    .fetch_one(&engine.store.pool)
                    .await
                    .unwrap(),
                0
            );
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("root.key");
            std::fs::write(&path, [207; 32]).unwrap();
            engine.set_root_key_path(path.clone());
            assert_eq!(engine.social_publish_once().await.unwrap(), 1);
            let organ = store::organs::local(&engine.store.pool)
                .await
                .unwrap()
                .unwrap();
            let state =
                store::records::get_extension(&engine.store.pool, &organ.uid, PROFILE_NAMESPACE)
                    .await
                    .unwrap()
                    .unwrap();
            let published: Profile = serde_json::from_value(state["published"].clone()).unwrap();
            assert_eq!(published.fields, fields);
            assert!(
                !state
                    .as_object()
                    .unwrap()
                    .keys()
                    .any(|key| key.starts_with("pending_profile_"))
            );
            engine.set_root_key_path(directory.path().join("offline-root.key"));
            clock
                .set_time(clock.now().timestamp_millis() + 8 * 86400 * 1000)
                .unwrap();
            let updated = ProfileFields {
                name: "Updated while owner offline".into(),
                ..Default::default()
            };
            let pending = command(
                &engine,
                Command::SaveProfile {
                    fields: updated.clone(),
                    parents: vec![document_hash("profile", &published).unwrap()],
                    destinations: vec![node],
                },
            )
            .await;
            assert_eq!(pending["profile_draft"]["fields"], json!(updated));
            let overview = command(&engine, Command::Overview).await;
            assert_eq!(overview["profile"]["editor"]["fields"], json!(updated));
            assert_eq!(overview["profile"]["published"]["fields"], json!(fields));
            engine.set_root_key_path(path);
            assert_eq!(engine.social_publish_once().await.unwrap(), 1);
            let state =
                store::records::get_extension(&engine.store.pool, &organ.uid, PROFILE_NAMESPACE)
                    .await
                    .unwrap()
                    .unwrap();
            assert_eq!(state["published"]["fields"], json!(updated));
            assert!(
                !state
                    .as_object()
                    .unwrap()
                    .keys()
                    .any(|key| key.starts_with("pending_profile_"))
            );
        })
        .await;
}

#[tokio::test]
async fn enabled_profile_renews_without_changing_fields_or_reviving_a_withdrawal() {
    let clock = nucleus::execution::Execution::new([194; 32], 1_800_000_000_000).unwrap();
    clock
        .scope(async {
            let engine = Engine::open_memory().await.unwrap();
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("root.key");
            std::fs::write(&path, [195; 32]).unwrap();
            engine.set_root_key_path(path);
            let hosts = vec![iroh::SecretKey::from_bytes(&[196; 32]).public().to_string()];
            let fields = ProfileFields {
                name: "Bicycle workshop".into(),
                ..Default::default()
            };
            let original = command(
                &engine,
                Command::SaveProfile {
                    fields: fields.clone(),
                    parents: vec![],
                    destinations: hosts.clone(),
                },
            )
            .await;
            assert!(!engine.social_renew_profile().await.unwrap());
            clock
                .set_time(clock.now().timestamp_millis() + 6 * 86400 * 1000 + 1000)
                .unwrap();
            assert!(engine.social_renew_profile().await.unwrap());
            assert!(!engine.social_renew_profile().await.unwrap());
            let organ = store::organs::local(&engine.store.pool)
                .await
                .unwrap()
                .unwrap();
            let state =
                store::records::get_extension(&engine.store.pool, &organ.uid, PROFILE_NAMESPACE)
                    .await
                    .unwrap()
                    .unwrap();
            let renewed: Profile = serde_json::from_value(state["published"].clone()).unwrap();
            assert_eq!(renewed.fields, fields);
            assert_eq!(renewed.destinations, hosts);
            assert_eq!(renewed.authority.organ, organ.uid);
            assert_eq!(renewed.revision, "2");
            assert_eq!(
                renewed.authority.generation,
                original["profile"]["authority"]["generation"]
            );
            assert_eq!(renewed.expires_at, clock.now().timestamp() + MAX_LIFETIME);
            command(
                &engine,
                Command::WithdrawProfile {
                    parents: vec![document_hash("profile", &renewed).unwrap()],
                },
            )
            .await;
            clock
                .set_time(clock.now().timestamp_millis() + 8 * 86400 * 1000)
                .unwrap();
            assert!(!engine.social_renew_profile().await.unwrap());
        })
        .await;
}

#[tokio::test]
async fn profile_image_upload_normalizes_bytes_without_opening_a_server_path() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let engine = Engine::open_memory().await.unwrap();
    let image = image::DynamicImage::new_rgb8(40, 20);
    let mut source = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut source, image::ImageFormat::Png)
        .unwrap();
    let result = command(
        &engine,
        Command::ImportProfileImageData {
            encoded: STANDARD.encode(source.into_inner()),
        },
    )
    .await;
    let bytes: Vec<u8> =
        store::sqlx::query_scalar("SELECT bytes FROM social_public_asset WHERE hash=?")
            .bind(result["asset_hash"].as_str().unwrap())
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
    assert_eq!(
        image::guess_format(&bytes).unwrap(),
        image::ImageFormat::Jpeg
    );
    assert_eq!(result["width"], 40);
    assert_eq!(result["height"], 20);
    assert!(
        engine
            .act(
                Action::Social {
                    request: Command::ImportProfileImageData {
                        encoded: STANDARD.encode(vec![0; 4 * 1024 * 1024 + 1])
                    }
                },
                None
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn my_posts_pages_bound_output_without_losing_retained_drafts() {
    let engine = Engine::open_memory().await.unwrap();
    for index in 0..10 {
        let mut input = draft();
        input.title = format!("Need {index}");
        save(&engine, input).await;
    }
    let first = command(&engine, Command::Overview).await;
    assert_eq!(first["posts"].as_array().unwrap().len(), 8);
    let after = first["next_posts_after"].as_str().unwrap().to_owned();
    let second = command(&engine, Command::PostPage { after }).await;
    assert_eq!(second["posts"].as_array().unwrap().len(), 2);
    assert!(second["next_posts_after"].is_null());
    let mut records: Vec<_> = first["posts"]
        .as_array()
        .unwrap()
        .iter()
        .chain(second["posts"].as_array().unwrap().iter())
        .map(|post| post["record"].as_str().unwrap())
        .collect();
    records.sort();
    records.dedup();
    assert_eq!(records.len(), 10);
}

#[async_trait::async_trait]
impl engine::social::Network for LocalSocialHost {
    async fn request(
        &self,
        destination: &str,
        request: PublicRequest,
    ) -> Result<Value, engine::EngineError> {
        assert_eq!(destination, self.node);
        let source = iroh::SecretKey::from_bytes(&[121; 32]).public().to_string();
        self.engine
            .social_public_request(
                &source,
                &self.node,
                request,
                nucleus::execution::now().timestamp(),
            )
            .await
    }
}

#[tokio::test]
async fn public_images_are_normalized_hosted_and_loaded_only_when_requested() {
    let sender = Engine::open_memory().await.unwrap();
    let host = Arc::new(Engine::open_memory().await.unwrap());
    let directory = tempfile::tempdir().unwrap();
    let root_path = directory.path().join("root.key");
    std::fs::write(&root_path, [122; 32]).unwrap();
    sender.set_root_key_path(root_path);
    let path = directory.path().join("chosen.png");
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(64, 64, image::Rgb([7, 14, 28])))
        .save(&path)
        .unwrap();
    let imported = command(
        &sender,
        Command::ImportProfileImage {
            path: path.to_string_lossy().into_owned(),
        },
    )
    .await;
    let hash = imported["asset_hash"].as_str().unwrap().to_owned();
    let bytes: Vec<u8> =
        store::sqlx::query_scalar("SELECT bytes FROM social_public_asset WHERE hash=?")
            .bind(&hash)
            .fetch_one(&sender.store.pool)
            .await
            .unwrap();
    assert_eq!(
        image::guess_format(&bytes).unwrap(),
        image::ImageFormat::Jpeg
    );
    assert_eq!(nucleus::fact::sha256_hex(&bytes), hash);
    let node = iroh::SecretKey::from_bytes(&[123; 32]).public().to_string();
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
    let network = Arc::new(LocalSocialHost {
        engine: host.clone(),
        node: node.clone(),
    });
    sender.attach_social_network(network.clone());
    let saved = command(
        &sender,
        Command::SaveProfile {
            fields: ProfileFields {
                name: "Workshop".into(),
                avatar: Some(hash.clone()),
                ..Default::default()
            },
            parents: vec![],
            destinations: vec![node.clone()],
        },
    )
    .await;
    assert_eq!(sender.social_publish_once().await.unwrap(), 2);
    let viewer = Engine::open_memory().await.unwrap();
    viewer.attach_social_network(network.clone());
    let organ = saved["profile"]["authority"]["organ"]
        .as_str()
        .unwrap()
        .to_owned();
    command(
        &viewer,
        Command::FetchProfile {
            organ: organ.clone(),
            services: vec![node.clone()],
        },
    )
    .await;
    let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM social_public_asset")
        .fetch_one(&viewer.store.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let loaded = command(
        &viewer,
        Command::FetchProfileImage {
            organ,
            hash: hash.clone(),
            services: vec![node],
        },
    )
    .await;
    assert_eq!(loaded["image_hash"], hash);
    let cached: Vec<u8> =
        store::sqlx::query_scalar("SELECT bytes FROM social_public_asset WHERE hash=?")
            .bind(&hash)
            .fetch_one(&viewer.store.pool)
            .await
            .unwrap();
    assert_eq!(cached, bytes);
}

#[tokio::test]
async fn public_profile_images_reject_unknown_hashes_and_oversized_dimensions() {
    let engine = Engine::open_memory().await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    let root_path = directory.path().join("root.key");
    std::fs::write(&root_path, [124; 32]).unwrap();
    engine.set_root_key_path(root_path);
    assert!(
        engine
            .act(
                Action::Social {
                    request: Command::SaveProfile {
                        fields: ProfileFields {
                            name: "Workshop".into(),
                            avatar: Some("ab".repeat(32)),
                            ..Default::default()
                        },
                        parents: vec![],
                        destinations: vec![],
                    }
                },
                None
            )
            .await
            .is_err()
    );
    let path = directory.path().join("too-wide.png");
    image::DynamicImage::ImageRgb8(image::RgbImage::new(2049, 1))
        .save(&path)
        .unwrap();
    assert!(
        engine
            .act(
                Action::Social {
                    request: Command::ImportProfileImage {
                        path: path.to_string_lossy().into_owned()
                    }
                },
                None
            )
            .await
            .is_err()
    );
    let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM social_public_asset")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

fn draft() -> PostDraft {
    PostDraft {
        title: "Bicycle help".into(),
        text: "I can repair bicycles this weekend".into(),
        direction: Direction::Contribution,
        language: "en".into(),
        ..Default::default()
    }
}

async fn save(engine: &Engine, input: PostDraft) -> String {
    let data = command(
        engine,
        Command::SaveDraft {
            record: None,
            source: None,
            draft: input,
        },
    )
    .await;
    data["record"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn discovery_pages_preserve_filters_services_and_reject_private_record_cursors() {
    let engine = Engine::open_memory().await.unwrap();
    let node = iroh::SecretKey::from_bytes(&[141; 32]).public().to_string();
    let now = nucleus::execution::now().timestamp();
    for index in 0..55 {
        let record = save(
            &engine,
            PostDraft {
                title: format!("cursorfixture contribution {index}"),
                destinations: vec![node.clone()],
                ..Default::default()
            },
        )
        .await;
        let (_, document) = preview(&engine, &record, PostState::Active).await;
        let hash = engine::social::document_hash("snippet", &document).unwrap();
        let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
        store::social::put_snippet_on(&mut tx, &document, &hash, "cursor-fixture", now)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }
    let query = nucleus::social::Search {
        text: "cursorfixture".into(),
        ..Default::default()
    };
    let first = command(
        &engine,
        Command::Search {
            query: query.clone(),
            services: vec![],
        },
    )
    .await;
    assert_eq!(first["results"].as_array().unwrap().len(), 50);
    assert_eq!(first["query"]["text"], query.text);
    assert_eq!(first["services"], json!([]));
    let mut next = query.clone();
    next.after = Some(first["next_after"].as_str().unwrap().into());
    let second = command(
        &engine,
        Command::Search {
            query: next.clone(),
            services: vec![],
        },
    )
    .await;
    assert_eq!(second["results"].as_array().unwrap().len(), 5);
    let ids: std::collections::BTreeSet<&str> = first["results"]
        .as_array()
        .unwrap()
        .iter()
        .chain(second["results"].as_array().unwrap())
        .map(|row| row["document"]["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids.len(), 55);
    next.after = Some(second["next_after"].as_str().unwrap().into());
    let empty = command(
        &engine,
        Command::Search {
            query: next,
            services: vec![],
        },
    )
    .await;
    assert!(empty["results"].as_array().unwrap().is_empty());
    assert!(empty["next_after"].is_null());
    let mut invalid = query;
    invalid.after = Some(nucleus::new_uid("r"));
    assert!(
        engine
            .social_command(
                Command::Search {
                    query: invalid.clone(),
                    services: vec![]
                },
                None,
                nucleus::execution::now()
            )
            .await
            .is_err()
    );
    command(
        &engine,
        Command::ConfigureServices {
            settings: ServiceSettings {
                townsquare: true,
                ..Default::default()
            },
        },
    )
    .await;
    assert!(
        engine
            .social_public_request(
                &node,
                &node,
                PublicRequest::Search {
                    query: invalid,
                    known: vec![]
                },
                now
            )
            .await
            .is_err()
    );
}

async fn preview(engine: &Engine, record: &str, state: PostState) -> (String, Snippet) {
    let data = command(
        engine,
        Command::Preview {
            record: record.into(),
            state,
        },
    )
    .await;
    (
        data["preview_hash"].as_str().unwrap().into(),
        serde_json::from_value(data["document"].clone()).unwrap(),
    )
}

#[tokio::test]
async fn selected_hosts_publish_retry_search_and_refuse_stale_receipts_over_wire() {
    let sender = Arc::new(Engine::open_memory().await.unwrap());
    let mut hosts = Vec::new();
    let mut wires = Vec::new();
    let mut servers = Vec::new();
    for seed in [81, 82] {
        let host = Arc::new(Engine::open_memory().await.unwrap());
        if seed == 81 {
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
        }
        let wire = Arc::new(
            engine::wire::Wire::bind_with_discovery(
                host.clone(),
                iroh::SecretKey::from_bytes(&[seed; 32]),
                engine::wire::Reach::Local,
                None,
                false,
            )
            .await
            .unwrap(),
        );
        let server = wire.clone();
        servers.push(tokio::spawn(async move { server.serve().await }));
        hosts.push(host);
        wires.push(wire);
    }
    let network = Arc::new(
        engine::wire::Wire::bind_with_discovery(
            sender.clone(),
            iroh::SecretKey::from_bytes(&[83; 32]),
            engine::wire::Reach::Local,
            None,
            false,
        )
        .await
        .unwrap(),
    );
    for wire in &wires {
        let port = wire
            .endpoint()
            .bound_sockets()
            .into_iter()
            .next()
            .unwrap()
            .port();
        network.remember_addr(
            iroh::EndpointAddr::new(wire.node_id()).with_ip_addr(([127, 0, 0, 1], port).into()),
        );
    }
    sender.attach_social_network(network.clone());
    let destinations: Vec<String> = wires
        .iter()
        .map(|wire| wire.node_id().to_string())
        .collect();
    let mut input = draft();
    input.destinations = destinations.clone();
    let uid = save(&sender, input).await;
    let (hash, original) = preview(&sender, &uid, PostState::Active).await;
    command(
        &sender,
        Command::Publish {
            record: uid.clone(),
            preview_hash: hash,
            document: original.clone(),
        },
    )
    .await;
    assert_eq!(sender.social_publish_once().await.unwrap(), 1);
    let jobs = store::social::jobs(&sender.store.pool).await.unwrap();
    assert_eq!(
        jobs.iter().filter(|job| job["state"] == "accepted").count(),
        1
    );
    assert_eq!(
        jobs.iter().filter(|job| job["state"] == "pending").count(),
        1
    );
    command(
        &hosts[1],
        Command::ConfigureServices {
            settings: ServiceSettings {
                directory: true,
                townsquare: true,
                ..Default::default()
            },
        },
    )
    .await;
    store::sqlx::query("UPDATE social_publication_job SET next_attempt=0 WHERE state='pending'")
        .execute(&sender.store.pool)
        .await
        .unwrap();
    assert_eq!(sender.social_publish_once().await.unwrap(), 1);
    let results = command(
        &sender,
        Command::Search {
            query: Search {
                text: "bicycle".into(),
                ..Default::default()
            },
            services: destinations,
        },
    )
    .await;
    assert_eq!(results["results"].as_array().unwrap().len(), 1);
    assert!(results["failures"].as_array().unwrap().is_empty());
    let (hash, withdrawn) = preview(&sender, &uid, PostState::Withdrawn).await;
    command(
        &sender,
        Command::Publish {
            record: uid,
            preview_hash: hash,
            document: withdrawn.clone(),
        },
    )
    .await;
    assert_eq!(sender.social_publish_once().await.unwrap(), 2);
    let stale = engine::social::Network::request(
        network.as_ref(),
        &wires[0].node_id().to_string(),
        PublicRequest::PublishSnippet { document: original },
    )
    .await
    .unwrap();
    assert_eq!(stale["accepted"], false);
    assert_eq!(stale["hash"], document_hash("snippet", &withdrawn).unwrap());
    for wire in &wires {
        wire.shutdown().await;
    }
    network.shutdown().await;
    for server in servers {
        server.abort();
    }
}

#[tokio::test]
async fn anonymous_public_projection_keeps_identity_and_source_private() {
    let engine = Engine::open_memory().await.unwrap();
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap();
    let source = store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Plain,
            head: "Private source",
            body: "Private details",
            quantity: store::exact::one(),
        },
    )
    .await
    .unwrap();
    let data = command(
        &engine,
        Command::SaveDraft {
            record: None,
            source: Some(source.uid.clone()),
            draft: draft(),
        },
    )
    .await;
    let uid = data["record"].as_str().unwrap();
    let (_, doc) = preview(&engine, uid, PostState::Active).await;
    let bytes = serde_json::to_string(&doc).unwrap();
    for private in [&organ.uid, &source.uid, uid, "Private details"] {
        assert!(!bytes.contains(private));
    }
    assert!(doc.profile.is_none());
    assert_eq!(doc.mode, AuthorMode::Anonymous);
    assert!(doc.reply.is_none());
    validate_snippet(&doc, nucleus::execution::now().timestamp()).unwrap();
}

#[tokio::test]
async fn anonymous_posts_are_separate_and_alias_reuse_is_deliberate() {
    let engine = Engine::open_memory().await.unwrap();
    let a = save(&engine, draft()).await;
    let b = save(&engine, draft()).await;
    let (_, a) = preview(&engine, &a, PostState::Active).await;
    let (_, b) = preview(&engine, &b, PostState::Active).await;
    assert_ne!(a.signing_key, b.signing_key);
    assert_ne!(a.id, b.id);
    let mut input = draft();
    input.alias = "repair-friend".into();
    let c = save(&engine, input.clone()).await;
    let d = save(&engine, input).await;
    assert_eq!(
        preview(&engine, &c, PostState::Active).await.1.signing_key,
        preview(&engine, &d, PostState::Active).await.1.signing_key
    );
}

#[tokio::test]
async fn generic_queries_cannot_read_social_posting_secrets() {
    let engine = Engine::open_memory().await.unwrap();
    let uid = save(&engine, draft()).await;
    let query: protein::Protein = serde_json::from_value(json!({"source":"record","where":[{"uid_eq":uid}],"include":{"extension":{"namespace":PUBLICATION_NAMESPACE}}})).unwrap();
    assert!(protein::execute(&engine.store, &query).await.is_err());
    let overview = command(&engine, Command::Overview).await;
    assert!(
        !serde_json::to_string(&overview)
            .unwrap()
            .contains("\"secret\"")
    );
    assert_eq!(overview["posts"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn enabling_a_public_service_does_not_export_local_only_posts_or_profiles() {
    let engine = Engine::open_memory().await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("root.key");
    std::fs::write(&path, [87; 32]).unwrap();
    engine.set_root_key_path(path);
    let profile = command(
        &engine,
        Command::SaveProfile {
            fields: ProfileFields {
                name: "Local workshop".into(),
                ..Default::default()
            },
            parents: vec![],
            destinations: vec![],
        },
    )
    .await;
    let mut input = draft();
    input.redistribute = true;
    let uid = save(&engine, input).await;
    let (hash, doc) = preview(&engine, &uid, PostState::Active).await;
    command(
        &engine,
        Command::Publish {
            record: uid,
            preview_hash: hash,
            document: doc.clone(),
        },
    )
    .await;
    command(
        &engine,
        Command::ConfigureServices {
            settings: ServiceSettings {
                directory: true,
                townsquare: true,
                ..Default::default()
            },
        },
    )
    .await;
    let source = iroh::SecretKey::from_bytes(&[85; 32]).public().to_string();
    let host = iroh::SecretKey::from_bytes(&[86; 32]).public().to_string();
    let now = nucleus::execution::now().timestamp();
    let public = engine
        .social_public_request(
            &source,
            &host,
            PublicRequest::Search {
                query: Search::default(),
                known: vec![format!("{}:{}", doc.id, "0".repeat(64))],
            },
            now,
        )
        .await
        .unwrap();
    assert!(public["results"].as_array().unwrap().is_empty());
    assert!(public["updates"].as_array().unwrap().is_empty());
    let public = engine
        .social_public_request(
            &source,
            &host,
            PublicRequest::FetchProfile {
                organ: profile["profile"]["authority"]["organ"]
                    .as_str()
                    .unwrap()
                    .into(),
            },
            now,
        )
        .await
        .unwrap();
    assert!(public["profile"].is_null());
    assert_eq!(
        command(
            &engine,
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
}

#[tokio::test]
async fn exact_preview_is_required_and_publication_retry_is_idempotent() {
    let engine = Engine::open_memory().await.unwrap();
    let uid = save(&engine, draft()).await;
    let (hash, doc) = preview(&engine, &uid, PostState::Active).await;
    let mut tampered = doc.clone();
    tampered.text = "Rewritten".into();
    assert!(
        engine
            .act(
                Action::Social {
                    request: Command::Publish {
                        record: uid.clone(),
                        preview_hash: hash.clone(),
                        document: tampered
                    }
                },
                None
            )
            .await
            .is_err()
    );
    let request = Command::Publish {
        record: uid.clone(),
        preview_hash: hash,
        document: doc.clone(),
    };
    let first = engine
        .act(
            Action::Social {
                request: request.clone(),
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(first.facts.len(), 1);
    let retry = engine.act(Action::Social { request }, None).await.unwrap();
    assert!(retry.facts.is_empty());
    assert_eq!(
        command(
            &engine,
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
    assert_eq!(doc.revision, "1");
}

#[tokio::test]
async fn changing_a_draft_invalidates_its_old_preview() {
    let engine = Engine::open_memory().await.unwrap();
    let uid = save(&engine, draft()).await;
    let (hash, doc) = preview(&engine, &uid, PostState::Active).await;
    let mut changed = draft();
    changed.text = "Another offer".into();
    command(
        &engine,
        Command::SaveDraft {
            record: Some(uid.clone()),
            source: None,
            draft: changed,
        },
    )
    .await;
    assert!(
        engine
            .act(
                Action::Social {
                    request: Command::Publish {
                        record: uid,
                        preview_hash: hash,
                        document: doc
                    }
                },
                None
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn pause_renew_and_withdraw_preserve_public_identity_and_suppress_stale_results() {
    let engine = Engine::open_memory().await.unwrap();
    let uid = save(&engine, draft()).await;
    let (hash, active) = preview(&engine, &uid, PostState::Active).await;
    command(
        &engine,
        Command::Publish {
            record: uid.clone(),
            preview_hash: hash,
            document: active.clone(),
        },
    )
    .await;
    let (hash, paused) = preview(&engine, &uid, PostState::Paused).await;
    assert_eq!(
        paused.parent,
        Some(document_hash("snippet", &active).unwrap())
    );
    command(
        &engine,
        Command::Publish {
            record: uid.clone(),
            preview_hash: hash,
            document: paused,
        },
    )
    .await;
    assert!(
        command(
            &engine,
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
    let (hash, renewed) = preview(&engine, &uid, PostState::Active).await;
    assert_eq!(renewed.id, active.id);
    assert_eq!(renewed.revision, "3");
    command(
        &engine,
        Command::Publish {
            record: uid.clone(),
            preview_hash: hash,
            document: renewed,
        },
    )
    .await;
    let (hash, withdrawn) = preview(&engine, &uid, PostState::Withdrawn).await;
    command(
        &engine,
        Command::Publish {
            record: uid,
            preview_hash: hash,
            document: withdrawn,
        },
    )
    .await;
    let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
    assert!(
        !store::social::put_snippet_on(
            &mut tx,
            &active,
            &document_hash("snippet", &active).unwrap(),
            "stale-server",
            nucleus::execution::now().timestamp()
        )
        .await
        .unwrap()
    );
    tx.commit().await.unwrap();
    assert!(
        command(
            &engine,
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
}

#[tokio::test]
async fn exact_quantity_and_safe_plain_text_search() {
    let engine = Engine::open_memory().await.unwrap();
    let mut input = draft();
    input.quantity = Some("1.250".into());
    input.unit = Some("hour".into());
    let uid = save(&engine, input).await;
    let (hash, doc) = preview(&engine, &uid, PostState::Active).await;
    assert_eq!(doc.quantity, Some("1.250".into()));
    command(
        &engine,
        Command::Publish {
            record: uid,
            preview_hash: hash,
            document: doc,
        },
    )
    .await;
    let data = command(
        &engine,
        Command::Search {
            query: Search {
                text: "bicycle".into(),
                ..Default::default()
            },
            services: vec![],
        },
    )
    .await;
    assert_eq!(data["results"].as_array().unwrap().len(), 1);
    command(
        &engine,
        Command::Search {
            query: Search {
                text: "\" OR * NOT (".into(),
                ..Default::default()
            },
            services: vec![],
        },
    )
    .await;
}

#[tokio::test]
async fn generic_extensions_cannot_bypass_publication_authority() {
    let engine = Engine::open_memory().await.unwrap();
    let uid = save(&engine, draft()).await;
    assert!(
        engine
            .act(
                Action::SetExtension {
                    target: uid,
                    namespace: PUBLICATION_NAMESPACE.into(),
                    fds: json!({"secret":"attacker"})
                },
                None
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn publication_state_uses_existing_sync_but_stays_out_of_foreign_exports() {
    let engine = Engine::open_memory().await.unwrap();
    let uid = save(&engine, draft()).await;
    let local = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap();
    let operational = engine.operational_key_for(&local.uid).await.unwrap();
    engine.set_organ_signer(operational.clone()).await.unwrap();
    engine.set_signer(operational).await.unwrap();
    let own = engine
        .export_sync_page(&local.uid, &[], 2000)
        .await
        .unwrap();
    assert!(
        own.batch
            .ops
            .iter()
            .any(|op| op.uid == uid && nucleus::social::private_sync_field(&op.tbl, &op.field))
    );
    let contact = store::organs::add_contact(
        &engine.store.pool,
        &nucleus::new_uid("r"),
        None,
        "Another Organ",
        "",
        1,
    )
    .await
    .unwrap();
    store::organs::set_trust(&engine.store.pool, &contact, "known")
        .await
        .unwrap();
    store::organs::set_sync_policy(&engine.store.pool, &contact, true, true)
        .await
        .unwrap();
    let foreign = engine.export_sync_page(&contact, &[], 2000).await.unwrap();
    assert!(
        foreign
            .batch
            .ops
            .iter()
            .any(|op| op.uid == uid && op.tbl == "record")
    );
    assert!(
        !foreign
            .batch
            .ops
            .iter()
            .any(|op| nucleus::social::private_sync_field(&op.tbl, &op.field))
    );
}

#[test]
fn signing_bytes_have_a_fixed_unicode_and_integer_vector() {
    let first = json!({"z":null,"signature":"ignored","amount":"1.250","n":1,"a":"😀"});
    let second = json!({"a":"😀","n":1,"amount":"1.250","z":null,"signature":"different"});
    let bytes = engine::social::signing_bytes("snippet", &first).unwrap();
    assert_eq!(
        bytes,
        "lince/social/snippet/1\n{\"a\":\"😀\",\"amount\":\"1.250\",\"n\":1,\"z\":null}".as_bytes()
    );
    assert_eq!(
        bytes,
        engine::social::signing_bytes("snippet", &second).unwrap()
    );
    assert_ne!(
        bytes,
        engine::social::signing_bytes("profile", &first).unwrap()
    );
    assert!(engine::social::signing_bytes("snippet", &json!({"n":0.5})).is_err());
}

#[tokio::test]
async fn saving_a_draft_cannot_claim_an_arbitrary_record() {
    let engine = Engine::open_memory().await.unwrap();
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap();
    assert!(
        engine
            .act(
                Action::Social {
                    request: Command::SaveDraft {
                        record: Some(organ.uid),
                        source: None,
                        draft: draft()
                    }
                },
                None
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn identified_post_reuses_a_narrow_profile_authority_without_revealing_anonymous_posts() {
    let engine = Engine::open_memory().await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    engine.set_root_key_path(directory.path().join("root.key"));
    std::fs::write(directory.path().join("root.key"), [19u8; 32]).unwrap();
    let anonymous = save(&engine, draft()).await;
    let (_, before) = preview(&engine, &anonymous, PostState::Active).await;
    let fields = ProfileFields {
        name: "Repair workshop".into(),
        description: "Bicycle repair".into(),
        ..Default::default()
    };
    let first = command(
        &engine,
        Command::SaveProfile {
            fields: fields.clone(),
            parents: vec![],
            destinations: vec![],
        },
    )
    .await;
    let profile: Profile = serde_json::from_value(first["profile"].clone()).unwrap();
    let mut input = draft();
    input.mode = AuthorMode::Identified;
    let identified = save(&engine, input).await;
    let (_, post) = preview(&engine, &identified, PostState::Active).await;
    assert_eq!(
        post.profile.as_ref().unwrap().organ,
        profile.authority.organ
    );
    assert_eq!(post.signing_key, profile.authority.editor_key);
    assert_ne!(post.signing_key, before.signing_key);
    let mut changed = fields;
    changed.name = "New public name".into();
    let next = command(
        &engine,
        Command::SaveProfile {
            fields: changed,
            parents: vec![first["hash"].as_str().unwrap().into()],
            destinations: vec![],
        },
    )
    .await;
    assert_eq!(
        next["profile"]["authority"]["organ"],
        first["profile"]["authority"]["organ"]
    );
    assert_eq!(
        preview(&engine, &anonymous, PostState::Active)
            .await
            .1
            .signing_key,
        before.signing_key
    );
}
