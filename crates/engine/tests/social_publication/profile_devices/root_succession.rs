use super::*;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

#[path = "root_succession/races.rs"]
mod races;

struct Hosts {
    hosts: Vec<(String, Arc<Engine>)>,
    offline: AtomicBool,
    requests: Mutex<Vec<PublicRequest>>,
}

#[async_trait::async_trait]
impl engine::social::Network for Hosts {
    async fn request(
        &self,
        destination: &str,
        request: PublicRequest,
    ) -> Result<Value, engine::EngineError> {
        if self.offline.load(Ordering::SeqCst) {
            return Err(engine::EngineError::Consequence("Offline host".into()));
        }
        self.requests.lock().unwrap().push(request.clone());
        let (_, host) = self
            .hosts
            .iter()
            .find(|(node, _)| node == destination)
            .unwrap();
        host.social_public_request(
            &iroh::SecretKey::from_bytes(&[211; 32]).public().to_string(),
            destination,
            request,
            nucleus::execution::now().timestamp(),
        )
        .await
    }
}

async fn hosts(owner: &Engine) -> Arc<Hosts> {
    let mut hosts = Vec::new();
    for seed in [212, 213] {
        let host = Arc::new(Engine::open_memory().await.unwrap());
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
        hosts.push((
            iroh::SecretKey::from_bytes(&[seed; 32])
                .public()
                .to_string(),
            host,
        ));
    }
    let hosts = Arc::new(Hosts {
        hosts,
        offline: AtomicBool::new(true),
        requests: Mutex::new(Vec::new()),
    });
    owner.attach_social_network(hosts.clone());
    hosts
}

async fn successor(
    owner: &Engine,
    root: &Signer,
    directory: &std::path::Path,
    revoke: bool,
    remove_device: bool,
) -> Signer {
    let fresh = Signer::from_bytes(&root.actor_uid, ROOT_KEY_ID, [214; 32]);
    owner
        .sign_succession(root, &fresh.public_key_b64())
        .await
        .unwrap();
    std::fs::write(directory.join("root.key"), fresh.secret_bytes()).unwrap();
    let mut members = owner
        .roster_of(&root.actor_uid)
        .await
        .unwrap()
        .unwrap()
        .roster
        .cells;
    if remove_device {
        let cell = store::cells::local(&owner.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        members.retain(|member| member.cell_uid == cell);
    }
    owner.publish_roster(&fresh, members).await.unwrap();
    if revoke {
        let (key, signature) = owner.revocation_certificate(root);
        owner
            .adopt_revocation(&root.actor_uid, &key, &signature)
            .await
            .unwrap();
        assert!(
            !owner
                .key_chains(&root.actor_uid, &root.public_key_b64())
                .await
                .unwrap()
        );
    }
    assert!(
        owner
            .key_chains(&root.actor_uid, &fresh.public_key_b64())
            .await
            .unwrap()
    );
    fresh
}

async fn publish(owner: &Engine) {
    for _ in 0..4 {
        store::sqlx::query(
            "UPDATE social_publication_job SET next_attempt=0 WHERE state='pending'",
        )
        .execute(&owner.store.pool)
        .await
        .unwrap();
        owner.social_publish_once().await.unwrap();
    }
}

#[tokio::test]
async fn pending_profile_refreshes_under_trusted_successor_without_old_authority_dispatch() {
    let clock = nucleus::execution::Execution::new([215; 32], 1_790_899_200_000).unwrap();
    clock.scope(Box::pin(async {
        for revoke in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let (owner, _, root, organ) = devices(directory.path()).await;
            let hosts = hosts(&owner).await;
            let destinations: Vec<String> = hosts.hosts.iter().map(|(node, _)| node.clone()).collect();
            let fields = ProfileFields { name:"Reviewed bicycle workshop".into(), description:"No automatic activity or presence claim".into(), ..Default::default() };
            let initial = command(&owner, Command::SaveProfile { fields:fields.clone(), parents:vec![], destinations:destinations.clone() }).await;
            let initial: Profile = serde_json::from_value(initial["profile"].clone()).unwrap();
            for (node, host) in &hosts.hosts {
                host.social_public_request(&iroh::SecretKey::from_bytes(&[211;32]).public().to_string(),node,PublicRequest::PublishProfile {document:initial.clone()},clock.now().timestamp()).await.unwrap();
            }
            publish(&owner).await;
            assert!(initial.expires_at > clock.now().timestamp() + 86400);
            let fresh = successor(&owner, &root, directory.path(), revoke, false).await;
            hosts.offline.store(false, Ordering::SeqCst);
            publish(&owner).await;
            let state = store::records::get_extension(&owner.store.pool, &organ, PROFILE_NAMESPACE).await.unwrap().unwrap();
            let refreshed: Profile = serde_json::from_value(state["published"].clone()).unwrap();
            assert_eq!(refreshed.authority.organ, organ);
            assert_eq!(refreshed.authority.root_key, fresh.public_key_b64());
            assert!(refreshed.authority.generation.parse::<i64>().unwrap() > initial.authority.generation.parse::<i64>().unwrap());
            assert_eq!(refreshed.fields, fields);
            assert_eq!(refreshed.destinations, destinations);
            assert_eq!(refreshed.authority.successions.len(), 1);
            for request in hosts.requests.lock().unwrap().iter() {
                match request {
                    PublicRequest::PublishProfile {document} => assert_eq!(document.authority.root_key, fresh.public_key_b64()),
                    PublicRequest::PublishAuthority {document} => assert_eq!(document.authority.root_key, fresh.public_key_b64()),
                    _ => {}
                }
            }
            for (node, host) in &hosts.hosts {
                let body: String = store::sqlx::query_scalar("SELECT body FROM social_document WHERE kind='profile' AND id=? AND state='active'").bind(&organ).fetch_one(&host.store.pool).await.unwrap();
                assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), serde_json::to_value(&refreshed).unwrap());
                assert!(host.social_public_request(&iroh::SecretKey::from_bytes(&[211;32]).public().to_string(), node, PublicRequest::PublishProfile {document:initial.clone()}, clock.now().timestamp()).await.is_err());
            }
        }
    })).await;
}

#[tokio::test]
async fn anonymous_pending_post_rebinds_revoked_root_without_changing_public_identity_or_deadline()
{
    let clock = nucleus::execution::Execution::new([216; 32], 1_790_899_200_000).unwrap();
    clock.scope(Box::pin(async {
        let directory = tempfile::tempdir().unwrap();
        let (owner, _, root, _) = devices(directory.path()).await;
        let hosts = hosts(&owner).await;
        let input = PostDraft { destinations:hosts.hosts.iter().map(|(node,_)|node.clone()).collect(), ..draft() };
        let uid = save(&owner, input).await;
        let (hash, initial) = preview(&owner, &uid, PostState::Active).await;
        command(&owner, Command::Publish {record:uid.clone(),preview_hash:hash,document:initial.clone()}).await;
        publish(&owner).await;
        let before = store::records::get_extension(&owner.store.pool, &uid, PUBLICATION_NAMESPACE).await.unwrap().unwrap();
        let wallet: (String,i64) = store::sqlx::query_as("SELECT body,version FROM social_device_state WHERE id=?").bind(format!("posting:{uid}")).fetch_one(&owner.store.pool).await.unwrap();
        let fresh = successor(&owner, &root, directory.path(), true, false).await;
        hosts.offline.store(false, Ordering::SeqCst);
        publish(&owner).await;
        let after = store::records::get_extension(&owner.store.pool, &uid, PUBLICATION_NAMESPACE).await.unwrap().unwrap();
        assert_eq!(after["anonymous_binding"]["root_key"], fresh.public_key_b64());
        assert_eq!(after["anonymous_binding"]["owner_key"], before["anonymous_binding"]["owner_key"]);
        assert_eq!(after["anonymous_authority"], before["anonymous_authority"]);
        assert_eq!(after["published"], before["published"]);
        assert_eq!(store::sqlx::query_as::<_,(String,i64)>("SELECT body,version FROM social_device_state WHERE id=?").bind(format!("posting:{uid}")).fetch_one(&owner.store.pool).await.unwrap(),wallet);
        for (_,host) in &hosts.hosts {
            let body:String = store::sqlx::query_scalar("SELECT body FROM social_document WHERE kind='snippet' AND id=? AND state='active'").bind(&initial.id).fetch_one(&host.store.pool).await.unwrap();
            assert_eq!(serde_json::from_str::<Value>(&body).unwrap(),serde_json::to_value(&initial).unwrap());
            assert!(!body.contains(&root.public_key_b64()));
            assert!(!body.contains(&fresh.public_key_b64()));
        }
    })).await;
}

#[tokio::test]
async fn anonymous_editor_removal_suppresses_old_pending_posts_and_propagates_new_floor() {
    let clock = nucleus::execution::Execution::new([217; 32], 1_790_899_200_000).unwrap();
    clock.scope(Box::pin(async {
        let directory = tempfile::tempdir().unwrap();
        let (owner, _, root, _) = devices(directory.path()).await;
        let hosts = hosts(&owner).await;
        let uid = save(&owner, PostDraft {destinations:hosts.hosts.iter().map(|(node,_)|node.clone()).collect(), ..draft()}).await;
        let (hash, initial) = preview(&owner, &uid, PostState::Active).await;
        command(&owner, Command::Publish {record:uid.clone(),preview_hash:hash,document:initial.clone()}).await;
        publish(&owner).await;
        successor(&owner, &root, directory.path(), true, true).await;
        hosts.offline.store(false,Ordering::SeqCst);
        publish(&owner).await;
        let state = store::records::get_extension(&owner.store.pool, &uid, PUBLICATION_NAMESPACE).await.unwrap().unwrap();
        assert_eq!(state["anonymous_authority"]["owner_key"],initial.anonymous.as_ref().unwrap().owner_key);
        assert_eq!(state["anonymous_authority"]["generation"],"2");
        assert_eq!(state["published"],serde_json::to_value(&initial).unwrap());
        assert!(!hosts.requests.lock().unwrap().iter().any(|request|matches!(request,PublicRequest::PublishSnippet {document} if document.id==initial.id)));
        for (node, host) in &hosts.hosts {
            assert_eq!(store::sqlx::query_scalar::<_,i64>("SELECT generation FROM social_posting_authority WHERE owner=?").bind(&initial.anonymous.as_ref().unwrap().owner_key).fetch_one(&host.store.pool).await.unwrap(),2);
            assert!(host.social_public_request(&iroh::SecretKey::from_bytes(&[211;32]).public().to_string(),node,PublicRequest::PublishSnippet {document:initial.clone()},clock.now().timestamp()).await.is_err());
        }
    })).await;
}

#[tokio::test]
async fn forged_anonymous_historical_binding_cannot_be_resigned_by_the_successor() {
    let clock = nucleus::execution::Execution::new([218; 32], 1_790_899_200_000).unwrap();
    clock
        .scope(Box::pin(async {
            let directory = tempfile::tempdir().unwrap();
            let (owner, _, root, _) = devices(directory.path()).await;
            let hosts = hosts(&owner).await;
            let uid = save(
                &owner,
                PostDraft {
                    destinations: hosts.hosts.iter().map(|(node, _)| node.clone()).collect(),
                    ..draft()
                },
            )
            .await;
            preview(&owner, &uid, PostState::Active).await;
            let mut state =
                store::records::get_extension(&owner.store.pool, &uid, PUBLICATION_NAMESPACE)
                    .await
                    .unwrap()
                    .unwrap();
            state["anonymous_binding"]["signature"] = json!("forged binding");
            store::records::set_extension(&owner.store.pool, &uid, PUBLICATION_NAMESPACE, &state)
                .await
                .unwrap();
            let wallet: (String, i64) =
                store::sqlx::query_as("SELECT body,version FROM social_device_state WHERE id=?")
                    .bind(format!("posting:{uid}"))
                    .fetch_one(&owner.store.pool)
                    .await
                    .unwrap();
            successor(&owner, &root, directory.path(), true, false).await;
            assert!(
                owner
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
            assert_eq!(
                store::records::get_extension(&owner.store.pool, &uid, PUBLICATION_NAMESPACE)
                    .await
                    .unwrap()
                    .unwrap(),
                state
            );
            assert_eq!(
                store::sqlx::query_as::<_, (String, i64)>(
                    "SELECT body,version FROM social_device_state WHERE id=?"
                )
                .bind(format!("posting:{uid}"))
                .fetch_one(&owner.store.pool)
                .await
                .unwrap(),
                wallet
            );
        }))
        .await;
}
