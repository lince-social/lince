use fiote::config::{Secret, Settings};
use std::path::PathBuf;

pub async fn existing_connection() -> (Settings, Secret) {
    let directory = PathBuf::from(
        std::env::var_os("LINCE_FIOTE_TEST_DATA").expect("Choose Lince data explicitly"),
    );
    let record = std::env::var("LINCE_FIOTE_TEST_RECORD")
        .expect("Choose an existing native Fiote explicitly");
    assert!(nucleus::valid_uid(&record, "r"));
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
    let mut key = fiote::credentials::Store::new(directory.join("fiote/credentials"))
        .key(&fiote::credentials::slot(&settings)).await.unwrap()
        .expect("Reconnect the explicitly selected provider before this live test");
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
    (settings, key)
}
