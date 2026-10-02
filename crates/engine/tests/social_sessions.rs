use vodozemac::olm::{Account, AccountPickle, OlmMessage, Session, SessionConfig, SessionPickle};

#[tokio::test]
async fn session_reset_rollback_preserves_decryptable_accounts_and_authority_wallet() {
    use engine::social::session::{AccountState, new_account, open_local, seal_local};
    let engine = engine::Engine::open_memory().await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    engine.set_sealing_keyring_path(directory.path().join("sealing.json"));
    let key = engine.social_storage_key().await.unwrap();
    let wallet_key = engine.social_authority_storage_key().await.unwrap();
    let account = new_account(
        &key,
        vec![],
        nucleus::execution::now().timestamp() + 32 * 86400,
    )
    .unwrap();
    let identity = account.route.identity_key.clone();
    let body = seal_local("account:test", &account, &key).unwrap();
    let wallet = seal_local(
        "wallet:test",
        &serde_json::json!({"owner":"private owner"}),
        &wallet_key,
    )
    .unwrap();
    for (id, kind, held) in [
        ("account:test", "account", &body),
        ("wallet:test", "authority", &wallet),
    ] {
        store::sqlx::query("INSERT INTO social_device_state(id,kind,context,body,updated_at) VALUES(?,?,'test',?,1)")
            .bind(id).bind(kind).bind(held).execute(&engine.store.pool).await.unwrap();
    }
    store::sqlx::query("INSERT INTO social_private_outbox(id,context,body,hash,expires_at,state) VALUES('reset-envelope','test','{}','test-hash',?,'pending')")
        .bind(account.expires_at).execute(&engine.store.pool).await.unwrap();
    store::sqlx::query("INSERT INTO social_private_destination(envelope,service,state) VALUES('reset-envelope','selected host','pending')")
        .execute(&engine.store.pool).await.unwrap();
    store::sqlx::query("CREATE TRIGGER fail_session_reset BEFORE DELETE ON social_device_state WHEN OLD.kind='account' BEGIN SELECT RAISE(ABORT,'injected database failure'); END")
        .execute(&engine.store.pool).await.unwrap();
    assert!(engine.social_reset_private_sessions().await.is_err());
    let held: String =
        store::sqlx::query_scalar("SELECT body FROM social_device_state WHERE id='account:test'")
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
    let current_key = engine.social_storage_key().await.unwrap();
    let retained: AccountState = open_local("account:test", &held, &current_key).unwrap();
    assert_eq!(
        retained
            .account(&current_key)
            .unwrap()
            .curve25519_key()
            .to_base64(),
        identity
    );
    assert_eq!(current_key, key);
    let queued: String = store::sqlx::query_scalar(
        "SELECT state FROM social_private_outbox WHERE id='reset-envelope'",
    )
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    assert_eq!(queued, "pending");
    assert_eq!(
        engine.social_authority_storage_key().await.unwrap(),
        wallet_key
    );
    store::sqlx::query("DROP TRIGGER fail_session_reset")
        .execute(&engine.store.pool)
        .await
        .unwrap();
    engine.social_reset_private_sessions().await.unwrap();
    let accounts: i64 = store::sqlx::query_scalar(
        "SELECT COUNT(*) FROM social_device_state WHERE kind IN ('account','session')",
    )
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    assert_eq!(accounts, 0);
    let held: String = store::sqlx::query_scalar(
        "SELECT state FROM social_private_outbox WHERE id='reset-envelope'",
    )
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    let destination: String = store::sqlx::query_scalar(
        "SELECT state FROM social_private_destination WHERE envelope='reset-envelope'",
    )
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    assert_eq!(held, "held");
    assert_eq!(destination, "cancelled");
    let retained_wallet: String =
        store::sqlx::query_scalar("SELECT body FROM social_device_state WHERE id='wallet:test'")
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
    assert_eq!(retained_wallet, wallet);
    let decoded: serde_json::Value =
        open_local("wallet:test", &retained_wallet, &wallet_key).unwrap();
    assert_eq!(decoded["owner"], "private owner");
    let fresh = new_account(
        &engine.social_storage_key().await.unwrap(),
        vec![],
        account.expires_at,
    )
    .unwrap();
    assert_ne!(fresh.route.identity_key, identity);
    assert_ne!(fresh.route.signing_key, account.route.signing_key);
    assert_ne!(fresh.route.pickup_key, account.route.pickup_key);
}

#[test]
fn offline_fallback_supports_independent_initiations_and_encrypted_restart_state() {
    let key = [17; 32];
    let mut receiver = Account::new();
    receiver.generate_fallback_key();
    let fallback = receiver.fallback_key().into_values().next().unwrap();
    let identity = receiver.curve25519_key();
    receiver.mark_keys_as_published();
    let held = receiver.pickle().encrypt(&key);
    let mut receiver = Account::from_pickle(AccountPickle::from_encrypted(&held, &key).unwrap());
    for index in 0..4 {
        let sender = Account::new();
        let mut outbound = sender
            .create_outbound_session(SessionConfig::version_1(), identity, fallback)
            .unwrap();
        let plaintext = format!("Private introduction {index}");
        let message = outbound.encrypt(&plaintext).unwrap();
        let message: OlmMessage =
            serde_json::from_slice(&serde_json::to_vec(&message).unwrap()).unwrap();
        let OlmMessage::PreKey(message) = message else {
            panic!("An initial offline message must be a prekey message")
        };
        let wrong_identity = Account::new().curve25519_key();
        assert!(
            receiver
                .create_inbound_session(SessionConfig::version_1(), wrong_identity, &message)
                .is_err()
        );
        let incoming = receiver
            .create_inbound_session(
                SessionConfig::version_1(),
                sender.curve25519_key(),
                &message,
            )
            .unwrap();
        assert_eq!(incoming.plaintext, plaintext.as_bytes());
        let mut inbound = incoming.session;
        assert_eq!(inbound.session_id(), outbound.session_id());
        let reply = inbound.encrypt("A private provisional reply").unwrap();
        assert_eq!(
            outbound.decrypt(&reply).unwrap(),
            b"A private provisional reply"
        );
        let messages: Vec<_> = ["one", "two", "three"]
            .into_iter()
            .map(|text| outbound.encrypt(text).unwrap())
            .collect();
        assert_eq!(inbound.decrypt(&messages[2]).unwrap(), b"three");
        assert_eq!(inbound.decrypt(&messages[0]).unwrap(), b"one");
        assert_eq!(inbound.decrypt(&messages[1]).unwrap(), b"two");
        assert!(inbound.decrypt(&messages[2]).is_err());
        let held = inbound.pickle().encrypt(&key);
        assert!(SessionPickle::from_encrypted(&held, &[18; 32]).is_err());
        let mut restarted =
            Session::from_pickle(SessionPickle::from_encrypted(&held, &key).unwrap());
        let next = outbound.encrypt("After restarting this device").unwrap();
        assert_eq!(
            restarted.decrypt(&next).unwrap(),
            b"After restarting this device"
        );
        let held = receiver.pickle().encrypt(&key);
        receiver = Account::from_pickle(AccountPickle::from_encrypted(&held, &key).unwrap());
    }
}
