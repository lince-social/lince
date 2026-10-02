use engine::{Engine, actions::Action, social::Worker};
use nucleus::social::{Command, PostDraft, PostState, PublicRequest, Search, ServiceSettings};
use serde_json::{Value, json};

#[path = "social_operator/health.rs"]
mod health;

async fn command(engine: &Engine, request: Command) -> Value {
    engine
        .act(Action::Social { request }, None)
        .await
        .unwrap()
        .data
        .unwrap()
}

#[tokio::test]
async fn deployment_policy_overrides_native_roles_without_reauthoring_private_settings() {
    let engine = Engine::open_memory().await.unwrap();
    command(
        &engine,
        Command::ConfigureServices {
            settings: ServiceSettings {
                directory: true,
                ..Default::default()
            },
        },
    )
    .await;
    let cell = store::cells::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let original =
        store::records::get_extension(&engine.store.pool, &cell, "lince.social.services")
            .await
            .unwrap();
    engine
        .social_set_deployment_settings(ServiceSettings::default())
        .unwrap();
    assert!(!engine.social_settings().await.unwrap().directory);
    assert!(
        engine
            .act(
                Action::Social {
                    request: Command::ConfigureServices {
                        settings: ServiceSettings {
                            mailbox: true,
                            ..Default::default()
                        }
                    }
                },
                None
            )
            .await
            .is_err()
    );
    assert_eq!(
        store::records::get_extension(&engine.store.pool, &cell, "lince.social.services")
            .await
            .unwrap(),
        original
    );
    assert_eq!(
        command(&engine, Command::Overview).await["services_managed"],
        true
    );
    let endpoint = iroh::SecretKey::from_bytes(&[151; 32]).public().to_string();
    let now = nucleus::execution::now().timestamp();
    let inspected = engine
        .social_public_request(
            "operator-test",
            &endpoint,
            PublicRequest::DescribeService,
            now,
        )
        .await
        .unwrap();
    assert_eq!(inspected["descriptor"]["roles"], json!([]));
    assert!(
        engine
            .social_public_request(
                "operator-test",
                &endpoint,
                PublicRequest::Search {
                    query: Search::default(),
                    known: vec![]
                },
                now
            )
            .await
            .is_err()
    );
    assert!(
        engine
            .social_set_deployment_settings(ServiceSettings {
                relay: true,
                ..Default::default()
            })
            .is_err()
    );
    let reopened = Engine::new(engine.store.clone()).await.unwrap();
    assert!(!reopened.social_services_managed());
    assert!(reopened.social_settings().await.unwrap().directory);
}

async fn post(engine: &Engine, title: &str) -> (String, String) {
    let draft = command(
        engine,
        Command::SaveDraft {
            record: None,
            source: None,
            draft: PostDraft {
                title: title.into(),
                ..Default::default()
            },
        },
    )
    .await;
    let record = draft["record"].as_str().unwrap().to_owned();
    let preview = command(
        engine,
        Command::Preview {
            record: record.clone(),
            state: PostState::Active,
        },
    )
    .await;
    let id = preview["document"]["id"].as_str().unwrap().to_owned();
    command(
        engine,
        Command::Publish {
            record: record.clone(),
            preview_hash: preview["preview_hash"].as_str().unwrap().into(),
            document: serde_json::from_value(preview["document"].clone()).unwrap(),
        },
    )
    .await;
    (record, id)
}

#[tokio::test]
async fn rebuilding_a_damaged_index_revalidates_signatures_and_preserves_ending_floors() {
    let engine = Engine::open_memory().await.unwrap();
    let (_, good) = post(&engine, "Bicycle help").await;
    let (ended_record, ended) = post(&engine, "Bicycle withdrawn").await;
    let (_, invalid) = post(&engine, "Bicycle invalid").await;
    let preview = command(
        &engine,
        Command::Preview {
            record: ended_record.clone(),
            state: PostState::Withdrawn,
        },
    )
    .await;
    command(
        &engine,
        Command::Publish {
            record: ended_record,
            preview_hash: preview["preview_hash"].as_str().unwrap().into(),
            document: serde_json::from_value(preview["document"].clone()).unwrap(),
        },
    )
    .await;
    store::sqlx::query(
        "UPDATE social_document SET body=json_set(body,'$.signature','forged') WHERE id=?",
    )
    .bind(&invalid)
    .execute(&engine.store.pool)
    .await
    .unwrap();
    store::sqlx::query("DELETE FROM social_search")
        .execute(&engine.store.pool)
        .await
        .unwrap();
    let query = Search {
        text: "bicycle".into(),
        ..Default::default()
    };
    assert!(
        store::social::search(
            &engine.store.pool,
            &query,
            nucleus::execution::now().timestamp()
        )
        .await
        .unwrap()
        .is_empty()
    );
    let rebuild = command(&engine, Command::RebuildPublicIndex).await;
    assert_eq!(rebuild["indexed"], 1);
    assert_eq!(rebuild["skipped"], 1);
    let found = store::social::search(
        &engine.store.pool,
        &query,
        nucleus::execution::now().timestamp(),
    )
    .await
    .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0]["document"]["id"], good);
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_ended_post WHERE id=?")
            .bind(&ended)
            .fetch_one(&engine.store.pool)
            .await
            .unwrap(),
        1
    );
    assert!(
        engine
            .act(
                Action::Social {
                    request: Command::RebuildPublicIndex
                },
                Some("unrecognized actor".into())
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn health_reports_bounded_worker_and_queue_metadata_without_query_or_message_contents() {
    let engine = Engine::open_memory().await.unwrap();
    let (_, _) = post(&engine, "Private operator fixture marker").await;
    let now = nucleus::execution::now().timestamp();
    store::sqlx::query("INSERT INTO social_ask_query(id,request,peers,deadline) VALUES(?,?,?,?)")
        .bind(nucleus::new_uid("ask"))
        .bind("{\"private_question\":\"Never expose these search words\"}")
        .bind("[]")
        .bind(now + 30)
        .execute(&engine.store.pool)
        .await
        .unwrap();
    engine.social_worker_started(Worker::Publication);
    engine.social_worker_completed(Worker::Publication, false);
    engine.social_worker_stopped(Worker::Publication);
    engine.social_worker_started(Worker::Publication);
    engine.social_worker_completed(Worker::Publication, true);
    let health = command(&engine, Command::ServiceHealth).await;
    let worker = &health["service_health"]["workers"][0]["state"];
    assert_eq!(worker["running"], true);
    assert_eq!(worker["restarts"], 1);
    assert_eq!(worker["failed_passes"], 1);
    assert_eq!(worker["last_succeeded"], true);
    assert_eq!(health["service_health"]["queues"]["queries"], 1);
    let encoded = serde_json::to_string(&health).unwrap();
    assert!(encoded.len() < 8192);
    assert!(!encoded.contains("Never expose") && !encoded.contains("fixture marker"));
    assert!(
        engine
            .act(
                Action::Social {
                    request: Command::ServiceHealth
                },
                Some("unrecognized actor".into())
            )
            .await
            .is_err()
    );
}
