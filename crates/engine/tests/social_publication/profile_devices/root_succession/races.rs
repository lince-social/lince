use super::*;
use tokio::sync::Notify;

struct Delayed {
    hosts: Arc<Hosts>,
    held: AtomicBool,
    entered: Notify,
    release: Notify,
}

#[async_trait::async_trait]
impl engine::social::Network for Delayed {
    async fn request(
        &self,
        destination: &str,
        request: PublicRequest,
    ) -> Result<Value, engine::EngineError> {
        let hold = matches!(&request, PublicRequest::PublishProfile { .. })
            && !self.held.swap(true, Ordering::SeqCst);
        let reply = engine::social::Network::request(&*self.hosts, destination, request).await?;
        if hold {
            self.entered.notify_one();
            self.release.notified().await;
        }
        Ok(reply)
    }
}

#[tokio::test]
async fn late_publication_receipt_keeps_evidence_without_sending_the_rest_of_obsolete_work() {
    let clock = nucleus::execution::Execution::new([219; 32], 1_790_899_200_000).unwrap();
    clock.scope(Box::pin(async {
        for change in ["successor", "write-downgrade"] {
            let directory = tempfile::tempdir().unwrap();
            let (owner, _, root, organ) = devices(directory.path()).await;
            let hosts = hosts(&owner).await;
            let initial = command(&owner,Command::SaveProfile {fields:ProfileFields {name:"Reviewed pending profile".into(),..Default::default()},parents:vec![],destinations:hosts.hosts.iter().map(|(node,_)|node.clone()).collect()}).await;
            let initial:Profile=serde_json::from_value(initial["profile"].clone()).unwrap();
            let hash=document_hash("profile",&initial).unwrap();
            let network=Arc::new(Delayed {hosts:hosts.clone(),held:AtomicBool::new(false),entered:Notify::new(),release:Notify::new()});
            owner.attach_social_network(network.clone());
            hosts.offline.store(false,Ordering::SeqCst);
            let (result,()) = tokio::time::timeout(std::time::Duration::from_secs(30), async {
                tokio::join!(owner.social_publish_once(),async {
                    network.entered.notified().await;
                    if change=="successor" {
                        successor(&owner,&root,directory.path(),true,false).await;
                        command(&owner,Command::RotateProfileAuthority).await;
                        assert!(owner.social_renew_profile().await.unwrap());
                    } else {
                        let cell=store::cells::local(&owner.store.pool).await.unwrap().unwrap().uid;
                        let mut members=owner.roster_of(&organ).await.unwrap().unwrap().roster.cells;
                        members.iter_mut().find(|member|member.cell_uid==cell).unwrap().capabilities.clear();
                        owner.publish_roster(&root,members).await.unwrap();
                    }
                    network.release.notify_one();
                })
            }).await.unwrap();
            assert_eq!(result.unwrap(),0,"{change}");
            assert_eq!(hosts.requests.lock().unwrap().iter().filter(|request|matches!(request,PublicRequest::PublishProfile {document} if document.authority.generation==initial.authority.generation)).count(),1,"{change}");
            let receipts:Vec<(String,String)> = store::sqlx::query_as("SELECT state,receipt FROM social_publication_job WHERE hash=? AND receipt IS NOT NULL").bind(&hash).fetch_all(&owner.store.pool).await.unwrap();
            assert_eq!(receipts.len(),1);
            assert_eq!(receipts[0].0,if change=="successor" {"cancelled"} else {"accepted"});
            let receipt:Value=serde_json::from_str(&receipts[0].1).unwrap();
            assert_eq!(receipt["accepted"],true);
            assert_eq!(receipt["hash"],hash);
            assert_eq!(receipt["expires_at"],initial.expires_at);
            assert!(hosts.hosts.iter().any(|(node,_)|receipt["service"]==*node));
            let jobs=store::social::jobs(&owner.store.pool).await.unwrap();
            assert_eq!(jobs.iter().find(|job|job["hash"]==hash && job["receipt"]["accepted"]==true).unwrap()["receipt"],receipt);
            if change=="successor" {
                publish(&owner).await;
                for (_,host) in &hosts.hosts {
                    assert_eq!(store::sqlx::query_scalar::<_,i64>("SELECT generation FROM social_profile_authority WHERE organ=?").bind(&organ).fetch_one(&host.store.pool).await.unwrap(),2);
                }
            } else {
                assert!(owner.social_publish_once().await.is_err());
                assert_eq!(store::sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM social_publication_job WHERE hash=? AND state='pending'").bind(&hash).fetch_one(&owner.store.pool).await.unwrap(),1);
            }
        }
    })).await;
}

#[tokio::test]
async fn revoked_root_pending_identified_post_can_still_be_withdrawn_under_its_stable_id() {
    let clock = nucleus::execution::Execution::new([221; 32], 1_790_899_200_000).unwrap();
    clock.scope(Box::pin(async {
        let directory = tempfile::tempdir().unwrap();
        let (owner, _, root, _) = devices(directory.path()).await;
        let hosts = hosts(&owner).await;
        let destinations:Vec<_>=hosts.hosts.iter().map(|(node,_)|node.clone()).collect();
        command(&owner,Command::SaveProfile {fields:ProfileFields {name:"Identified workshop".into(),..Default::default()},parents:vec![],destinations:destinations.clone()}).await;
        let uid=save(&owner,PostDraft {mode:AuthorMode::Identified,destinations,..draft()}).await;
        let (hash, initial)=preview(&owner,&uid,PostState::Active).await;
        command(&owner,Command::Publish {record:uid.clone(),preview_hash:hash,document:initial.clone()}).await;
        publish(&owner).await;
        let fresh=successor(&owner,&root,directory.path(),true,false).await;
        hosts.offline.store(false,Ordering::SeqCst);
        publish(&owner).await;
        assert!(!hosts.requests.lock().unwrap().iter().any(|request|matches!(request,PublicRequest::PublishSnippet {document} if document.id==initial.id)));
        let (hash, ending)=preview(&owner,&uid,PostState::Withdrawn).await;
        assert_eq!(ending.id,initial.id);
        assert_eq!(ending.signing_key,initial.signing_key);
        assert_eq!(ending.profile.as_ref().unwrap().root_key,fresh.public_key_b64());
        assert_eq!(ending.profile.as_ref().unwrap().generation,"2");
        assert_eq!(ending.state,PostState::Withdrawn);
        let fresh_host=Engine::open_memory().await.unwrap();
        command(&fresh_host,Command::ConfigureServices {settings:ServiceSettings {directory:true,..Default::default()}}).await;
        let receipt=fresh_host.social_public_request(&iroh::SecretKey::from_bytes(&[211;32]).public().to_string(),&ending.destinations[0],PublicRequest::PublishSnippet {document:ending.clone()},clock.now().timestamp()).await.unwrap();
        assert_eq!(receipt["accepted"],true);
        assert_eq!(receipt["hash"],document_hash("snippet",&ending).unwrap());
        assert_eq!(store::sqlx::query_as::<_,(i64,String)>("SELECT generation,editor_key FROM social_profile_authority WHERE organ=?").bind(&ending.profile.as_ref().unwrap().organ).fetch_one(&fresh_host.store.pool).await.unwrap(),(0,String::new()));
        assert_eq!(store::sqlx::query_scalar::<_,String>("SELECT id FROM social_ended_post WHERE id=?").bind(&initial.id).fetch_one(&fresh_host.store.pool).await.unwrap(),initial.id);
        command(&owner,Command::Publish {record:uid,preview_hash:hash,document:ending.clone()}).await;
        publish(&owner).await;
        for (node,host) in &hosts.hosts {
            assert_eq!(store::sqlx::query_scalar::<_,String>("SELECT id FROM social_ended_post WHERE id=?").bind(&initial.id).fetch_one(&host.store.pool).await.unwrap(),initial.id);
            assert_eq!(store::sqlx::query_scalar::<_,String>("SELECT state FROM social_document WHERE id=?").bind(&initial.id).fetch_one(&host.store.pool).await.unwrap(),"withdrawn");
            assert!(host.social_public_request(&iroh::SecretKey::from_bytes(&[211;32]).public().to_string(),node,PublicRequest::PublishSnippet {document:initial.clone()},clock.now().timestamp()).await.is_err());
        }
    })).await;
}

#[tokio::test]
async fn root_refresh_does_not_publish_local_withdrawn_or_conflicting_profile_fields() {
    let clock = nucleus::execution::Execution::new([220; 32], 1_790_899_200_000).unwrap();
    clock
        .scope(Box::pin(async {
            for state in ["local-only", "withdrawn", "conflict"] {
                let directory = tempfile::tempdir().unwrap();
                let (owner, device, root, organ) = devices(directory.path()).await;
                let hosts = hosts(&owner).await;
                let destinations = if state == "local-only" {
                    vec![]
                } else {
                    hosts.hosts.iter().map(|(node, _)| node.clone()).collect()
                };
                let initial = command(
                    &owner,
                    Command::SaveProfile {
                        fields: ProfileFields {
                            name: "Original reviewed profile".into(),
                            ..Default::default()
                        },
                        parents: vec![],
                        destinations: destinations.clone(),
                    },
                )
                .await;
                let parent = initial["hash"].as_str().unwrap().to_owned();
                if state == "withdrawn" {
                    command(
                        &owner,
                        Command::WithdrawProfile {
                            parents: vec![parent],
                        },
                    )
                    .await;
                } else if state == "conflict" {
                    copy(&owner, &device, &organ).await;
                    for (engine, name) in [
                        (&owner, "Owner's offline choice"),
                        (&device, "Other device's offline choice"),
                    ] {
                        command(
                            engine,
                            Command::SaveProfile {
                                fields: ProfileFields {
                                    name: name.into(),
                                    ..Default::default()
                                },
                                parents: vec![parent.clone()],
                                destinations: destinations.clone(),
                            },
                        )
                        .await;
                    }
                    copy(&device, &owner, &organ).await;
                }
                let before =
                    store::records::get_extension(&owner.store.pool, &organ, PROFILE_NAMESPACE)
                        .await
                        .unwrap()
                        .unwrap();
                successor(&owner, &root, directory.path(), true, false).await;
                hosts.offline.store(false, Ordering::SeqCst);
                publish(&owner).await;
                let after =
                    store::records::get_extension(&owner.store.pool, &organ, PROFILE_NAMESPACE)
                        .await
                        .unwrap()
                        .unwrap();
                assert_eq!(after["published"], before["published"], "{state}");
                assert!(
                    !hosts
                        .requests
                        .lock()
                        .unwrap()
                        .iter()
                        .any(|request| matches!(request, PublicRequest::PublishProfile { .. })),
                    "{state}"
                );
                assert_eq!(
                    after
                        .as_object()
                        .unwrap()
                        .keys()
                        .filter(|key| key.starts_with("revision_"))
                        .count(),
                    before
                        .as_object()
                        .unwrap()
                        .keys()
                        .filter(|key| key.starts_with("revision_"))
                        .count()
                );
            }
        }))
        .await;
}
