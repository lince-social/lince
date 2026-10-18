use fiote::config::{Secret, Settings};
use std::path::PathBuf;

pub async fn existing_connection() -> (Settings, Secret) {
    let directory = PathBuf::from(
        std::env::var_os("LINCE_FIOTE_TEST_DATA").expect("Choose Lince data explicitly"),
    );
    let password_path = PathBuf::from(
        std::env::var_os("LINCE_FIOTE_TEST_PASSWORD_FILE")
            .expect("Choose an owner-readable vault-password file"),
    );
    let record = std::env::var("LINCE_FIOTE_TEST_RECORD")
        .expect("Choose an existing native Fiote explicitly");
    assert!(nucleus::valid_uid(&record, "r"));
    let metadata = std::fs::metadata(&password_path).unwrap();
    assert!(metadata.is_file() && metadata.len() <= 4096);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(metadata.permissions().mode() & 0o077, 0);
    }
    let mut password = Secret(std::fs::read_to_string(password_path).unwrap());
    let length = password.0.trim_end_matches(['\r', '\n']).len();
    password.0.truncate(length);
    let configuration: serde_json::Value = serde_json::from_slice(
        &std::fs::read(directory.join("fiote").join(format!("{record}.json"))).unwrap(),
    )
    .unwrap();
    assert!(
        configuration
            .get("agent")
            .is_none_or(serde_json::Value::is_null)
    );
    let mut settings: Settings = serde_json::from_value(configuration["settings"].clone()).unwrap();
    if let Ok(model) = std::env::var("LINCE_FIOTE_TEST_MODEL") {
        settings.model = model;
    }
    settings.reasoning = Some("low".into());
    settings.fast = false;
    let pool = store::sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            store::sqlx::sqlite::SqliteConnectOptions::new()
                .filename(directory.join("lince.db"))
                .read_only(true)
                .create_if_missing(false),
        )
        .await
        .unwrap();
    let uid: String = store::sqlx::query_scalar(
        "SELECT uid FROM record WHERE slug = 'fiote-vault' AND deleted_at IS NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let envelope = store::records::get_extension(&pool, &uid, "lince.vault.providers")
        .await
        .unwrap()
        .unwrap();
    let plaintext = Secret(
        utils::vault::unlock(&uid, &password.0, envelope["ciphertext"].as_str().unwrap()).unwrap(),
    );
    let mut entries: std::collections::BTreeMap<String, Secret> =
        serde_json::from_str(&plaintext.0).unwrap();
    let mut key = entries
        .remove(&format!(
            "{}:{}:{}:{}",
            settings.provider.0, settings.auth_method, settings.endpoint, settings.account
        ))
        .expect("The selected native connection has no saved credential");
    if fiote::communication::native::has_native_login(&settings) {
        let mut account = fiote::communication::auth::Account::decode(&key).unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        println!(
            "native_test_connection_enabled={} access_valid_for_seconds={}",
            account.enabled(),
            account.expires_at.saturating_sub(now)
        );
        assert!(
            account.enabled() && account.expires_at > now + 300,
            "Renew the connection through normal Lince before this access-token-only test"
        );
        account.refresh_token = Secret::default();
        key = account.encode().unwrap();
    }
    pool.close().await;
    (settings, key)
}
