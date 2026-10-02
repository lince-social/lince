use super::*;
use base64::Engine as _;

#[tokio::test]
async fn offline_mailbox_first_registration_after_expiry_retains_the_exact_ending() {
    let clock = nucleus::execution::Execution::new([215; 32], 1_790_899_200_000).unwrap();
    clock.scope(Box::pin(async {
        let owner = Engine::open_memory().await.unwrap();
        let directory = tempfile::tempdir().unwrap();
        owner.set_sealing_keyring_path(directory.path().join("sealing.json"));
        let root = directory.path().join("root.key");
        std::fs::write(&root, [216;32]).unwrap();
        owner.set_root_key_path(root);
        let organ = store::organs::local(&owner.store.pool).await.unwrap().unwrap().uid;
        let signer = owner.operational_key_for(&organ).await.unwrap();
        owner.set_signer(signer.clone()).await.unwrap();
        owner.set_organ_signer(signer).await.unwrap();
        let mut hosts = Vec::new();
        for seed in [217,218] {
            let host = Arc::new(Engine::open_memory().await.unwrap());
            command(&host,Command::ConfigureServices { settings:ServiceSettings { directory:seed==217,mailbox:true,..Default::default() } }).await;
            hosts.push((iroh::SecretKey::from_bytes(&[seed;32]).public().to_string(),host));
        }
        let services:Vec<String> = hosts.iter().map(|(id,_)|id.clone()).collect();
        let network = Arc::new(SelectedMailboxHosts { hosts,second_online:std::sync::atomic::AtomicBool::new(false) });
        owner.attach_social_network(network.clone());
        let mut input = draft();
        input.destinations = vec![services[0].clone()];
        let record = save(&owner,input).await;
        command(&owner,Command::PrepareReplyKeys { record:record.clone(),services:services.clone() }).await;
        let (hash,original) = preview(&owner,&record,PostState::Active).await;
        command(&owner,Command::Publish { record:record.clone(),preview_hash:hash,document:original.clone() }).await;
        for _ in 0..4 { owner.social_publish_once().await.unwrap(); }
        let missed = &network.hosts[1].1;
        assert_eq!(store::sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM social_reply_route").fetch_one(&missed.store.pool).await.unwrap(),0);
        clock.set_time((original.expires_at+1)*1000).unwrap();
        let (hash,ending) = preview(&owner,&record,PostState::Withdrawn).await;
        command(&owner,Command::Publish { record:record.clone(),preview_hash:hash.clone(),document:ending.clone() }).await;
        for _ in 0..4 { owner.social_publish_once().await.unwrap(); }
        command(&owner,Command::PrepareReplyKeys { record:record.clone(),services }).await;
        network.second_online.store(true,std::sync::atomic::Ordering::SeqCst);
        for _ in 0..8 {
            store::sqlx::query("UPDATE social_publication_job SET next_attempt=0 WHERE state='pending'").execute(&owner.store.pool).await.unwrap();
            owner.social_publish_once().await.unwrap();
        }
        let ending_work:String = store::sqlx::query_scalar("SELECT state FROM social_publication_job WHERE kind='reply-ending' AND destination=? AND expires_at=?").bind(&network.hosts[1].0).bind(ending.expires_at).fetch_one(&owner.store.pool).await.unwrap();
        assert_eq!(ending_work,"accepted");
        assert_eq!(store::sqlx::query_scalar::<_,String>("SELECT hash FROM social_ended_post WHERE id=?").bind(&original.id).fetch_one(&missed.store.pool).await.unwrap(),hash);
        let states:Vec<String> = store::sqlx::query_scalar("SELECT state FROM social_reply_route").fetch_all(&missed.store.pool).await.unwrap();
        assert!(!states.is_empty());
        assert!(states.iter().all(|state|state=="closed"));
        let status = missed.social_public_request(&network.hosts[0].0,&network.hosts[1].0,PublicRequest::EndReplyPost { document:Box::new(ending.clone()) },ending.issued_at).await.unwrap();
        assert_eq!(status["hash"],hash);
        let state = store::records::get_extension(&owner.store.pool,&record,PUBLICATION_NAMESPACE).await.unwrap().unwrap();
        let secret = base64::engine::general_purpose::STANDARD.decode(state["secret"].as_str().unwrap()).unwrap();
        let editor = engine::trust::Signer::from_bytes("","social",secret.try_into().unwrap());
        let mut replay = ending.clone();
        replay.state = PostState::Active;
        replay.revision = "999".into();
        replay.signature = editor.sign_bytes(&engine::social::signing_bytes("snippet",&replay).unwrap());
        validate_snippet(&replay,ending.issued_at).unwrap();
        let mut forged = ending.clone();
        forged.text.push_str("tampered");
        for document in [replay.clone(),forged] {
            assert!(missed.social_public_request(&network.hosts[0].0,&network.hosts[1].0,PublicRequest::EndReplyPost { document:Box::new(document) },ending.issued_at).await.is_err());
        }
        let full = Engine::open_memory().await.unwrap();
        command(&full,Command::ConfigureServices { settings:ServiceSettings { mailbox:true,cache_entries:32,..Default::default() } }).await;
        store::sqlx::query("WITH RECURSIVE n(i) AS (VALUES(1) UNION ALL SELECT i+1 FROM n WHERE i<32) INSERT INTO social_ended_post(id,authority,revision,hash) SELECT 'filled-ending-'||i,'capacity-fixture',1,'capacity-fixture' FROM n").execute(&full.store.pool).await.unwrap();
        let refused = full.social_public_request(&network.hosts[0].0,&network.hosts[1].0,PublicRequest::EndReplyPost { document:Box::new(ending.clone()) },ending.issued_at).await.unwrap_err();
        assert!(refused.to_string().contains("cache is full"));
        assert_eq!(store::sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM social_ended_post WHERE id=?").bind(&original.id).fetch_one(&full.store.pool).await.unwrap(),0);
        assert_eq!(store::sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM social_posting_authority").fetch_one(&full.store.pool).await.unwrap(),0);
        let path = directory.path().join("missed-mailbox.db");
        missed.store.snapshot_into(&path).await.unwrap();
        let reopened = Engine::open(&format!("sqlite://{}?mode=rwc",path.display())).await.unwrap();
        assert_eq!(store::sqlx::query_scalar::<_,String>("SELECT hash FROM social_ended_post WHERE id=?").bind(&original.id).fetch_one(&reopened.store.pool).await.unwrap(),hash);
        let mut tx = store::write_tx(&reopened.store.pool).await.unwrap();
        assert!(store::social::put_snippet_on(&mut tx,&replay,&document_hash("snippet",&replay).unwrap(),&network.hosts[0].0,ending.issued_at).await.is_err());
        tx.rollback().await.unwrap();
        clock.set_time((ending.expires_at+601)*1000).unwrap();
        store::social::prune(&reopened.store.pool,nucleus::execution::now().timestamp()).await.unwrap();
        assert_eq!(store::sqlx::query_scalar::<_,String>("SELECT hash FROM social_ended_post WHERE id=?").bind(&original.id).fetch_one(&reopened.store.pool).await.unwrap(),hash);
    })).await;
}
