use super::*;
use nucleus::social::requests::{BLOCK_NAMESPACE, SESSION_AUTHORITY_NAMESPACE};

#[tokio::test]
async fn inactive_admission_and_unblock_tombstones_survive_stale_enrolled_device_uploads() {
    let clock = nucleus::execution::Execution::new([244; 32], 1_790_899_200_000).unwrap();
    clock.scope(Box::pin(async {
        let directory = tempfile::tempdir().unwrap();
        let (owner, device, root, organ) = devices(directory.path()).await;
        owner.set_sealing_keyring_path(directory.path().join("owner/sealing.json"));
        device.set_sealing_keyring_path(directory.path().join("device/sealing.json"));
        let service = iroh::SecretKey::from_bytes(&[245; 32]).public().to_string();
        let context = command(&owner, Command::SaveDraft { record:None,source:None,draft:PostDraft { title:"Inactive admission continuity".into(),..Default::default() } }).await["record"].as_str().unwrap().to_owned();
        command(&owner, Command::PrepareReplyKeys { record:context.clone(),services:vec![service.clone()] }).await;
        copy(&owner, &device, &organ).await;
        command(&device, Command::PrepareReplyKeys { record:context.clone(),services:vec![service] }).await;
        copy(&device, &owner, &organ).await;
        owner.social_refresh_reply_authorizations().await.unwrap();
        copy(&owner, &device, &organ).await;
        device.social_refresh_reply_authorizations().await.unwrap();
        let now = clock.now().timestamp();
        let peer = Signer::from_bytes("","social",[246;32]).public_key_b64();
        store::records::set_extension(&owner.store.pool, &context, BLOCK_NAMESPACE, &json!({peer.clone():{"blocked":true,"window":now}})).await.unwrap();
        copy(&owner, &device, &organ).await;
        clock.set_time((now+1)*1000).unwrap();
        store::records::set_extension(&device.store.pool, &context, BLOCK_NAMESPACE, &json!({peer.clone():{"blocked":true,"window":now+1}})).await.unwrap();
        clock.set_time((now+2)*1000).unwrap();
        command(&owner, Command::UnblockParticipant { context:context.clone(),peer:peer.clone() }).await;
        command(&owner, Command::ArchivePost { record:context.clone() }).await;
        let cell = store::cells::local(&owner.store.pool).await.unwrap().unwrap().uid;
        let namespace = format!("lince.social.admission-{cell}");
        let admission = store::records::get_extension(&owner.store.pool, &context, &namespace).await.unwrap().unwrap();
        assert!(admission[&peer]["signature"].as_str().is_some());
        for index in 0..10 {
            store::records::set_extension(&owner.store.pool,&context,&format!("lince.social.admission-z{index:02}"),&admission).await.unwrap();
        }
        copy(&owner, &device, &organ).await;
        clock.set_time((now+7*86400+3)*1000).unwrap();
        owner.social_refresh_reply_authorizations().await.unwrap();
        let deadline = command(&owner, Command::ReplyKeyStatus { record:context.clone() }).await["retire_after"].as_i64().unwrap();
        clock.set_time((deadline-1)*1000).unwrap();
        let members = owner.roster_of(&organ).await.unwrap().unwrap().roster.cells;
        device.adopt_roster(&owner.publish_roster(&root, members).await.unwrap()).await.unwrap();
        owner.social_refresh_reply_authorizations().await.unwrap();
        assert!(store::records::get_extension(&owner.store.pool, &context, &namespace).await.unwrap().is_some());
        clock.set_time(deadline*1000).unwrap();
        owner.social_refresh_reply_authorizations().await.unwrap();
        assert!(store::records::get_extension(&owner.store.pool, &context, &namespace).await.unwrap().is_none());
        assert!(store::records::get_extension(&owner.store.pool, &context, BLOCK_NAMESPACE).await.unwrap().is_none());
        assert_eq!(store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM record_extension WHERE record_uid=? AND namespace LIKE 'lince.social.admission-%'").bind(&context).fetch_one(&owner.store.pool).await.unwrap(),3);
        let reopened = Engine::new(owner.store.clone()).await.unwrap();
        reopened.set_sealing_keyring_path(directory.path().join("owner/sealing.json"));
        reopened.set_root_key_path(directory.path().join("root.key"));
        let signer = owner.operational_key_for(&organ).await.unwrap();
        reopened.set_signer(signer.clone()).await.unwrap();
        reopened.set_organ_signer(signer).await.unwrap();
        reopened.social_refresh_reply_authorizations().await.unwrap();
        assert_eq!(store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM record_extension WHERE record_uid=? AND namespace LIKE 'lince.social.admission-%'").bind(&context).fetch_one(&owner.store.pool).await.unwrap(),0);
        copy(&device, &owner, &organ).await;
        assert!(store::records::get_extension(&owner.store.pool, &context, BLOCK_NAMESPACE).await.unwrap().is_none());
        copy(&owner, &device, &organ).await;
        device.social_refresh_reply_authorizations().await.unwrap();
        assert!(store::records::get_extension(&device.store.pool, &context, &namespace).await.unwrap().unwrap_or_else(||json!({})).as_object().unwrap().is_empty());
        assert!(store::records::get_extension(&device.store.pool, &context, BLOCK_NAMESPACE).await.unwrap().unwrap_or_else(||json!({})).as_object().unwrap().is_empty());
        assert!(store::records::get_extension(&device.store.pool, &context, SESSION_AUTHORITY_NAMESPACE).await.unwrap().is_some());
        assert_eq!(owner.social_reconcile_private_admissions().await.unwrap(),0);
    })).await;
}
