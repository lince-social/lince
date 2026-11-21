use store::{Store, StoreError, auth};

#[tokio::test]
async fn login_lookups_keep_the_hash_for_verification_and_redact_it_from_debug()
-> Result<(), StoreError> {
    let store = Store::open_memory().await?;
    let pool = &store.pool;
    let role = auth::ensure_role(pool, "redaction-test").await?;
    let username = "fixture-user";
    let hash = "fixture-password-hash";
    let uid = auth::create_person_login(pool, "Fixture User", username, hash, role).await?;

    let by_name = auth::user_by_username(pool, username)
        .await?
        .expect("the login is visible by username");
    let by_uid = auth::user_by_uid(pool, &uid)
        .await?
        .expect("the login is visible by Person UID");
    assert_eq!(by_name, by_uid);
    assert_eq!(by_name.password_hash, hash);
    let debug = format!("{by_name:?}");
    assert!(debug.contains("<redacted>"));
    assert!(!debug.contains(hash));
    Ok(())
}
