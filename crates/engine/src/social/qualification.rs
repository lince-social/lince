use super::*;
use std::{sync::Arc, time::Duration};
use store::sqlx::Connection;

#[path = "qualification/resource_tiers.rs"]
mod resource_tiers;

pub(super) fn publication(index: u32, endpoint: &str) -> (Snippet, Signer) {
    let now = nucleus::execution::now().timestamp();
    let mut owner_bytes = [222; 32];
    owner_bytes[..4].copy_from_slice(&index.to_le_bytes());
    let mut editor_bytes = [223; 32];
    editor_bytes[..4].copy_from_slice(&index.to_le_bytes());
    let owner = Signer::from_bytes("", "social", owner_bytes);
    let editor = Signer::from_bytes("", "social", editor_bytes);
    let mut authority = PostingAuthority {
        owner_key: owner.public_key_b64(),
        editor_key: editor.public_key_b64(),
        generation: "1".into(),
        issued_at: now,
        expires_at: now + MAX_LIFETIME,
        signature: String::new(),
    };
    authority.signature =
        owner.sign_bytes(&signing_bytes("posting-authority", &authority).unwrap());
    let nonce = B64.encode(index.to_le_bytes().repeat(4));
    let destinations = vec![endpoint.to_owned()];
    let mut document = Snippet {
        protocol: "lince.snippet.1".into(),
        id: post_id::post_id(
            &owner.public_key_b64(),
            &nonce,
            AuthorMode::Anonymous,
            "",
            None,
            &destinations,
        )
        .unwrap(),
        nonce,
        revision: "1".into(),
        parent: None,
        resolves: vec![],
        created_at: now,
        issued_at: now,
        expires_at: now + MAX_LIFETIME,
        mode: AuthorMode::Anonymous,
        signing_key: editor.public_key_b64(),
        profile: None,
        anonymous: Some(authority),
        alias: String::new(),
        title: format!("Bicycle help {index}"),
        text: "x".repeat(1000),
        direction: PostDraft::default().direction,
        quantity: None,
        unit: None,
        concept: None,
        language: String::new(),
        area: String::new(),
        availability: String::new(),
        state: PostState::Active,
        redistribute: true,
        destinations,
        reply: None,
        signature: String::new(),
    };
    document.signature = editor.sign_bytes(&signing_bytes("snippet", &document).unwrap());
    validate_snippet(&document, now).unwrap();
    (document, editor)
}

async fn host(path: &std::path::Path) -> Arc<Engine> {
    let engine = Arc::new(Engine::open(path.to_str().unwrap()).await.unwrap());
    engine
        .social_command(
            Command::ConfigureServices {
                settings: ServiceSettings {
                    directory: true,
                    cache_entries: 100,
                    storage_bytes: 1024 * 1024,
                    ..Default::default()
                },
            },
            None,
            nucleus::execution::now(),
        )
        .await
        .unwrap();
    engine
}

#[tokio::test]
async fn sqlite_full_refuses_publication_without_partial_authority_and_retries_after_space_returns()
{
    let directory = tempfile::tempdir().unwrap();
    let engine = host(&directory.path().join("full.sqlite")).await;
    store::sqlx::query("CREATE TABLE fault_filler (payload BLOB NOT NULL)")
        .execute(&engine.store.pool)
        .await
        .unwrap();
    let pages: i64 = store::sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    let mut connections = Vec::new();
    for _ in 0..4 {
        connections.push(engine.store.pool.acquire().await.unwrap());
    }
    for connection in &mut connections {
        store::sqlx::query(&format!("PRAGMA max_page_count={}", pages + 8))
            .execute(&mut **connection)
            .await
            .unwrap();
    }
    drop(connections);
    loop {
        if let Err(error) = store::sqlx::query("INSERT INTO fault_filler VALUES (zeroblob(3000))")
            .execute(&engine.store.pool)
            .await
        {
            assert!(error.to_string().contains("full"), "{error}");
            break;
        }
    }
    let endpoint = iroh::SecretKey::from_bytes(&[224; 32]).public().to_string();
    let (mut document, editor) = publication(1, &endpoint);
    document.text = "🦊".repeat(1000);
    document.signature = editor.sign_bytes(&signing_bytes("snippet", &document).unwrap());
    validate_snippet(&document, nucleus::execution::now().timestamp()).unwrap();
    let request = PublicRequest::PublishSnippet {
        document: document.clone(),
    };
    let error = engine
        .social_public_request(
            &endpoint,
            &endpoint,
            request.clone(),
            nucleus::execution::now().timestamp(),
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("full"), "{error}");
    for table in ["social_document", "social_posting_authority"] {
        assert_eq!(
            store::sqlx::query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(&engine.store.pool)
                .await
                .unwrap(),
            0
        );
    }
    store::sqlx::query("DELETE FROM fault_filler")
        .execute(&engine.store.pool)
        .await
        .unwrap();
    engine
        .social_public_request(
            &endpoint,
            &endpoint,
            request,
            nucleus::execution::now().timestamp(),
        )
        .await
        .unwrap();
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_document WHERE id=?")
            .bind(&document.id)
            .fetch_one(&engine.store.pool)
            .await
            .unwrap(),
        1
    );
    engine.store.pool.close().await;
}

#[tokio::test]
async fn sqlite_busy_refuses_intake_then_accepts_the_same_signed_document_after_unlock() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("busy.sqlite");
    let engine = host(&path).await;
    let mut connections = Vec::new();
    for _ in 0..4 {
        connections.push(engine.store.pool.acquire().await.unwrap());
    }
    for connection in &mut connections {
        store::sqlx::query("PRAGMA busy_timeout=100")
            .execute(&mut **connection)
            .await
            .unwrap();
    }
    drop(connections);
    let mut lock = store::sqlx::SqliteConnection::connect(&format!("sqlite://{}", path.display()))
        .await
        .unwrap();
    store::sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut lock)
        .await
        .unwrap();
    let endpoint = iroh::SecretKey::from_bytes(&[225; 32]).public().to_string();
    let (document, _) = publication(2, &endpoint);
    let request = PublicRequest::PublishSnippet {
        document: document.clone(),
    };
    let result = tokio::time::timeout(
        Duration::from_secs(15),
        engine.social_public_request(
            &endpoint,
            &endpoint,
            request.clone(),
            nucleus::execution::now().timestamp(),
        ),
    )
    .await
    .unwrap();
    let error = result.unwrap_err();
    assert!(
        error.to_string().contains("locked") || error.to_string().contains("busy"),
        "{error}"
    );
    store::sqlx::query("ROLLBACK")
        .execute(&mut lock)
        .await
        .unwrap();
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_document")
            .fetch_one(&engine.store.pool)
            .await
            .unwrap(),
        0
    );
    engine
        .social_public_request(
            &endpoint,
            &endpoint,
            request,
            nucleus::execution::now().timestamp(),
        )
        .await
        .unwrap();
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_document WHERE id=?")
            .bind(&document.id)
            .fetch_one(&engine.store.pool)
            .await
            .unwrap(),
        1
    );
    lock.close().await.unwrap();
    engine.store.pool.close().await;
}

#[tokio::test]
async fn five_hundred_virtual_sources_cannot_overbook_the_cache_or_prevent_withdrawal() {
    let started = std::time::Instant::now();
    let directory = tempfile::tempdir().unwrap();
    let engine = host(&directory.path().join("sources.sqlite")).await;
    let endpoint = iroh::SecretKey::from_bytes(&[226; 32]).public().to_string();
    let mut accepted = Vec::new();
    for wave in 0..32u32 {
        let mut workers = tokio::task::JoinSet::new();
        for offset in 0..16 {
            let index = wave * 16 + offset;
            let engine = engine.clone();
            let endpoint = endpoint.clone();
            let (document, editor) = publication(index, &endpoint);
            let mut node = [227; 32];
            node[..4].copy_from_slice(&index.to_le_bytes());
            let source = iroh::SecretKey::from_bytes(&node).public().to_string();
            workers.spawn(async move {
                let result = engine
                    .social_public_request(
                        &source,
                        &endpoint,
                        PublicRequest::PublishSnippet {
                            document: document.clone(),
                        },
                        nucleus::execution::now().timestamp(),
                    )
                    .await;
                (result, document, editor)
            });
        }
        while let Some(result) = workers.join_next().await {
            let (result, document, editor) = result.unwrap();
            match result {
                Ok(_) => accepted.push((document, editor)),
                Err(error) => assert!(
                    error.to_string().contains("full") || error.to_string().contains("limited"),
                    "{error}"
                ),
            }
        }
    }
    let admitted = accepted.len();
    assert!(admitted > 0 && admitted <= 100);
    for table in ["social_document", "social_posting_authority"] {
        assert_eq!(
            store::sqlx::query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(&engine.store.pool)
                .await
                .unwrap(),
            admitted as i64
        );
    }
    let (old, editor) = accepted.remove(0);
    let mut ending = old.clone();
    ending.revision = "2".into();
    ending.parent = Some(document_hash("snippet", &old).unwrap());
    ending.state = PostState::Withdrawn;
    ending.signature = editor.sign_bytes(&signing_bytes("snippet", &ending).unwrap());
    engine
        .social_public_request(
            &endpoint,
            &endpoint,
            PublicRequest::PublishSnippet { document: ending },
            nucleus::execution::now().timestamp(),
        )
        .await
        .unwrap();
    let stale = engine
        .social_public_request(
            &endpoint,
            &endpoint,
            PublicRequest::PublishSnippet {
                document: old.clone(),
            },
            nucleus::execution::now().timestamp(),
        )
        .await
        .unwrap();
    assert_eq!(stale["accepted"], false);
    assert_eq!(stale["state"], "withdrawn");
    assert_eq!(stale["revision"], "2");
    let results = store::social::search(
        &engine.store.pool,
        &Search {
            text: old.title.clone(),
            ..Default::default()
        },
        nucleus::execution::now().timestamp(),
    )
    .await
    .unwrap();
    assert!(results.iter().all(|row| row["document"]["id"] != old.id));
    let pages: i64 = store::sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    let page_size: i64 = store::sqlx::query_scalar("PRAGMA page_size")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    let wal = std::fs::metadata(directory.path().join("sources.sqlite-wal"))
        .map_or(0, |metadata| metadata.len());
    println!(
        "512 virtual sources/owners, 16 concurrent requests, {admitted} admitted posts within a 100-entry/1-MiB configuration, {} database bytes, {wal} WAL bytes, {:?} elapsed",
        pages * page_size,
        started.elapsed()
    );
    engine.store.pool.close().await;
}
