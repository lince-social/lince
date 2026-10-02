use engine::{Engine, actions::Action};
use nucleus::social::{Command, PostDraft, PostState, PublicRequest, ServiceSettings, Snippet};
use serde_json::Value;

async fn command(engine: &Engine, request: Command) -> Value {
    engine
        .act(Action::Social { request }, None)
        .await
        .unwrap()
        .data
        .unwrap()
}

#[tokio::test]
async fn a_saturated_public_revision_cache_still_accepts_the_owners_withdrawal() {
    let author = Engine::open_memory().await.unwrap();
    let host = Engine::open_memory().await.unwrap();
    command(
        &host,
        Command::ConfigureServices {
            settings: ServiceSettings {
                directory: true,
                cache_entries: 100,
                storage_bytes: 1024 * 1024,
                ..Default::default()
            },
        },
    )
    .await;
    let source = iroh::SecretKey::from_bytes(&[155; 32]).public().to_string();
    let endpoint = iroh::SecretKey::from_bytes(&[156; 32]).public().to_string();
    let mut record = None;
    let mut saturated = false;
    let mut id = String::new();
    let mut accepted = None;
    for revision in 0..48 {
        let draft = command(
            &author,
            Command::SaveDraft {
                record: record.clone(),
                source: None,
                draft: PostDraft {
                    title: format!("Bicycle help revision {revision}"),
                    text: "🦊".repeat(950),
                    destinations: vec![endpoint.clone()],
                    ..Default::default()
                },
            },
        )
        .await;
        record = Some(draft["record"].as_str().unwrap().to_owned());
        let preview = command(
            &author,
            Command::Preview {
                record: record.clone().unwrap(),
                state: PostState::Active,
            },
        )
        .await;
        let document: Snippet = serde_json::from_value(preview["document"].clone()).unwrap();
        id = document.id.clone();
        command(
            &author,
            Command::Publish {
                record: record.clone().unwrap(),
                preview_hash: preview["preview_hash"].as_str().unwrap().into(),
                document: document.clone(),
            },
        )
        .await;
        let result = host
            .social_public_request(
                &source,
                &endpoint,
                PublicRequest::PublishSnippet {
                    document: document.clone(),
                },
                nucleus::execution::now().timestamp(),
            )
            .await;
        if let Err(error) = result {
            assert!(error.to_string().contains("cache is full"));
            saturated = true;
            break;
        }
        accepted = Some(document);
    }
    assert!(
        saturated,
        "The fixture must saturate the configured public revision byte budget"
    );
    let repeated = host
        .social_public_request(
            &source,
            &endpoint,
            PublicRequest::PublishSnippet {
                document: accepted.unwrap(),
            },
            nucleus::execution::now().timestamp(),
        )
        .await
        .unwrap();
    assert_eq!(repeated["accepted"], true);
    let preview = command(
        &author,
        Command::Preview {
            record: record.unwrap(),
            state: PostState::Withdrawn,
        },
    )
    .await;
    let document: Snippet = serde_json::from_value(preview["document"].clone()).unwrap();
    let mut forged = document.clone();
    forged.signature = "forged ending signature".into();
    assert!(
        host.social_public_request(
            &source,
            &endpoint,
            PublicRequest::PublishSnippet { document: forged },
            nucleus::execution::now().timestamp()
        )
        .await
        .is_err()
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, String>(
            "SELECT state FROM social_document WHERE kind='snippet' AND id=?"
        )
        .bind(&id)
        .fetch_one(&host.store.pool)
        .await
        .unwrap(),
        "active"
    );
    let receipt = host
        .social_public_request(
            &source,
            &endpoint,
            PublicRequest::PublishSnippet { document },
            nucleus::execution::now().timestamp(),
        )
        .await
        .unwrap();
    assert_eq!(receipt["state"], "withdrawn");
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_search WHERE id=?")
            .bind(&id)
            .fetch_one(&host.store.pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_ended_post WHERE id=?")
            .bind(&id)
            .fetch_one(&host.store.pool)
            .await
            .unwrap(),
        1
    );
}
