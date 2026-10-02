use super::*;
use nucleus::social::requests::SESSION_AUTHORITY_NAMESPACE;

#[tokio::test]
async fn signed_owner_retirement_syncs_to_enrolled_devices_and_fresh_requests_recover_authority() {
    retirement_case(false).await;
}

#[tokio::test]
async fn signed_owner_retirement_reaches_a_deleted_context_through_own_sync() {
    retirement_case(true).await;
}

async fn retirement_case(deleted: bool) {
    let clock = nucleus::execution::Execution::new([240; 32], 1_790_899_200_000).unwrap();
    clock.scope(Box::pin(async {
        let directory = tempfile::tempdir().unwrap();
        let (owner, device, root, organ) = devices(directory.path()).await;
        owner.set_sealing_keyring_path(directory.path().join("owner/sealing.json"));
        device.set_sealing_keyring_path(directory.path().join("device/sealing.json"));
        let service = iroh::SecretKey::from_bytes(&[241; 32]).public().to_string();
        let draft = command(&owner, Command::SaveDraft {
            record: None,
            source: None,
            draft: PostDraft { title: "Retirement continuity".into(), ..Default::default() },
        }).await;
        let context = draft["record"].as_str().unwrap().to_owned();
        command(&owner, Command::PrepareReplyKeys { record: context.clone(), services: vec![service.clone()] }).await;
        copy(&owner, &device, &organ).await;
        command(&device, Command::PrepareReplyKeys { record: context.clone(), services: vec![service.clone()] }).await;
        copy(&device, &owner, &organ).await;
        owner.social_refresh_reply_authorizations().await.unwrap();
        copy(&owner, &device, &organ).await;
        device.social_refresh_reply_authorizations().await.unwrap();
        let initial = command(&device, Command::ReplyKeyStatus { record: context.clone() }).await;
        assert_eq!(initial["reply_keys"], "ready");
        let old_route = initial["route"]["route"].clone();
        let peer = Signer::from_bytes("", "social", [243;32]).public_key_b64();
        let started = clock.now().timestamp();
        if deleted {
            store::records::set_extension(&owner.store.pool, &context, nucleus::social::requests::BLOCK_NAMESPACE, &json!({(peer.clone()):{"blocked":true,"window":started}})).await.unwrap();
        }
        command(&owner, Command::ArchivePost { record: context.clone() }).await;
        if deleted {
            store::records::mark_deleted(&owner.store.pool, &context).await.unwrap();
            command(&owner, Command::UnblockParticipant { context:context.clone(),peer:peer.clone() }).await;
            clock.set_time((started+7*86400+2)*1000).unwrap();
        }
        copy(&owner, &device, &organ).await;
        owner.social_refresh_reply_authorizations().await.unwrap();
        device.social_refresh_reply_authorizations().await.unwrap();
        let (state, deadline, error): (String, i64, Option<String>) = store::sqlx::query_as("SELECT state,retire_after,error FROM social_context_retention WHERE context=?").bind(&context).fetch_one(&owner.store.pool).await.unwrap();
        assert_eq!(state, "dormant", "Retirement review: {error:?}");
        let members = owner.roster_of(&organ).await.unwrap().unwrap().roster.cells;
        clock.set_time((deadline - 1) * 1000).unwrap();
        let renewed = owner.publish_roster(&root, members).await.unwrap();
        device.adopt_roster(&renewed).await.unwrap();
        device.social_refresh_reply_authorizations().await.unwrap();
        let account = format!("account:{context}");
        let wallet = format!("authority:{context}");
        assert!(store::social::device_state(&device.store.pool, &account).await.unwrap().is_some());
        clock.set_time(deadline * 1000).unwrap();
        device.social_refresh_reply_authorizations().await.unwrap();
        assert!(store::social::device_state(&device.store.pool, &account).await.unwrap().is_some());
        let owner_wallet = store::social::device_state(&owner.store.pool, &wallet).await.unwrap().unwrap();
        store::sqlx::query("CREATE TRIGGER retirement_failure BEFORE UPDATE ON social_device_state WHEN OLD.kind='authority' BEGIN SELECT RAISE(ABORT,'retirement rollback'); END")
            .execute(&owner.store.pool).await.unwrap();
        owner.social_refresh_reply_authorizations().await.unwrap();
        assert!(store::social::device_state(&owner.store.pool, &account).await.unwrap().is_some());
        assert_eq!(store::social::device_state(&owner.store.pool, &wallet).await.unwrap().unwrap(), owner_wallet);
        assert!(store::records::get_extension(&owner.store.pool, &organ, "lince.social.retirements").await.unwrap().is_none());
        store::sqlx::query("DROP TRIGGER retirement_failure").execute(&owner.store.pool).await.unwrap();
        owner.social_refresh_reply_authorizations().await.unwrap();
        assert!(store::social::device_state(&owner.store.pool, &account).await.unwrap().is_none());
        assert!(store::social::device_state(&device.store.pool, &account).await.unwrap().is_some());
        let proof = store::records::get_extension(&owner.store.pool, &organ, "lince.social.retirements").await.unwrap().unwrap();
        assert_eq!(proof[&context]["request_floor"], deadline);
        let public = ed25519_dalek::VerifyingKey::from_bytes(&B64.decode(proof[&context]["owner_key"].as_str().unwrap()).unwrap().try_into().unwrap()).unwrap();
        let signature = ed25519_dalek::Signature::from_slice(&B64.decode(proof[&context]["signature"].as_str().unwrap()).unwrap()).unwrap();
        assert!(public.verify_strict(&engine::social::signing_bytes("private-key-retirement", &proof[&context]).unwrap(), &signature).is_ok());
        copy(&owner, &device, &organ).await;
        let mut forged = proof.clone();
        forged[&context]["request_floor"] = json!(deadline - 1);
        store::records::set_extension(&device.store.pool, &organ, "lince.social.retirements", &forged).await.unwrap();
        device.social_refresh_reply_authorizations().await.unwrap();
        assert!(store::social::device_state(&device.store.pool, &account).await.unwrap().is_some());
        assert_eq!(store::sqlx::query_scalar::<_, String>("SELECT state FROM social_context_retention WHERE context=?").bind(&context).fetch_one(&device.store.pool).await.unwrap(), "review");
        store::records::set_extension(&device.store.pool, &organ, "lince.social.retirements", &proof).await.unwrap();
        let reopened = Engine::new(device.store.clone()).await.unwrap();
        reopened.set_sealing_keyring_path(directory.path().join("device/sealing.json"));
        let device_signer = device.operational_key_for(&organ).await.unwrap();
        reopened.set_signer(device_signer.clone()).await.unwrap();
        reopened.set_organ_signer(device_signer).await.unwrap();
        reopened.social_refresh_reply_authorizations().await.unwrap();
        assert!(store::social::device_state(&device.store.pool, &account).await.unwrap().is_none());
        assert!(store::social::device_state(&device.store.pool, &wallet).await.unwrap().is_none());
        if deleted {
            assert!(store::sqlx::query_scalar::<_, bool>("SELECT deleted_at IS NOT NULL FROM record WHERE uid=?").bind(&context).fetch_one(&device.store.pool).await.unwrap());
            assert_eq!(store::records::get_extension(&device.store.pool, &context, nucleus::social::requests::BLOCK_NAMESPACE).await.unwrap().unwrap()[&peer]["blocked"],true);
            assert_eq!(store::records::get_extension(&device.store.pool, &organ, "lince.social.retained-blocks").await.unwrap().unwrap()[format!("{context}:{peer}")]["blocked"],false);
            assert_eq!(reopened.social_reconcile_private_admissions().await.unwrap(),0);
            return;
        }
        assert!(store::records::get(&device.store.pool, &context).await.unwrap().is_some());
        let retained_version = store::social::device_state(&owner.store.pool, &wallet).await.unwrap().unwrap().1;
        store::sqlx::query("UPDATE social_device_state SET body=? WHERE id=? AND version=?").bind(&owner_wallet.0).bind(&wallet).bind(retained_version).execute(&owner.store.pool).await.unwrap();
        let before = store::records::get_extension(&owner.store.pool, &context, SESSION_AUTHORITY_NAMESPACE).await.unwrap().unwrap();
        let owner_ready = command(&owner, Command::PrepareReplyKeys { record: context.clone(), services: vec![service.clone()] }).await;
        assert_eq!(owner_ready["reply_keys"], "ready");
        assert_eq!(owner_ready["route"]["control"]["generation"], "2");
        let device_cell = store::cells::local(&device.store.pool).await.unwrap().unwrap().uid;
        let authorized = store::records::get_extension(&owner.store.pool, &context, SESSION_AUTHORITY_NAMESPACE).await.unwrap().unwrap();
        assert_eq!(authorized[format!("request_{device_cell}")], before[format!("request_{device_cell}")]);
        assert_ne!(authorized[format!("authorized_{device_cell}")]["control"], authorized["control"]);
        copy(&owner, &reopened, &organ).await;
        command(&reopened, Command::PrepareReplyKeys { record: context.clone(), services: vec![service] }).await;
        copy(&reopened, &owner, &organ).await;
        owner.social_refresh_reply_authorizations().await.unwrap();
        copy(&owner, &reopened, &organ).await;
        reopened.social_refresh_reply_authorizations().await.unwrap();
        let recovered = command(&reopened, Command::ReplyKeyStatus { record: context.clone() }).await;
        assert_eq!(recovered["reply_keys"], "dormant");
        let recovered = store::records::get_extension(&reopened.store.pool, &context, SESSION_AUTHORITY_NAMESPACE).await.unwrap().unwrap()[format!("authorized_{device_cell}")].clone();
        let route: nucleus::social::requests::CertifiedRoute = serde_json::from_value(recovered.clone()).unwrap();
        engine::social::request_auth::validate_route(&route, clock.now().timestamp()).unwrap();
        assert!(store::social::device_state(&reopened.store.pool, &account).await.unwrap().is_some());
        assert_ne!(recovered["route"]["identity_key"], old_route["identity_key"]);
        assert_ne!(recovered["route"]["mailbox"], old_route["mailbox"]);
        assert_eq!(recovered["control"]["generation"], "2");
    })).await;
}
