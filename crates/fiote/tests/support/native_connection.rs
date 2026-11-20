use fiote::config::{Secret, Settings};
use std::{io, path::PathBuf};

pub async fn existing_connection() -> io::Result<(Settings, Secret)> {
    let directory = PathBuf::from(
        std::env::var_os("LINCE_FIOTE_TEST_DATA")
            .ok_or_else(|| io::Error::other("Choose Lince data explicitly"))?,
    );
    let record = std::env::var("LINCE_FIOTE_TEST_RECORD")
        .map_err(|_| io::Error::other("Choose an existing native Fiote explicitly"))?;
    if !nucleus::valid_uid(&record, "r") {
        return Err(io::Error::other("Choose a valid native Fiote Record UID"));
    }
    let path = directory.join("fiote").join(format!("{record}.json"));
    let configuration: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)
        .map_err(|_| io::Error::other("Cannot read the selected Fiote configuration"))?;
    if configuration
        .get("agent")
        .is_some_and(|agent| !agent.is_null())
    {
        return Err(io::Error::other("Choose a native Fiote without an agent"));
    }
    let mut settings: Settings = serde_json::from_value(configuration["settings"].clone())
        .map_err(|_| io::Error::other("The selected Fiote settings are invalid"))?;
    if let Ok(model) = std::env::var("LINCE_FIOTE_TEST_MODEL") {
        settings.model = model;
    }
    settings.reasoning = Some("low".into());
    settings.fast = false;
    let mut key = fiote::credentials::Store::new(directory.join("fiote/credentials"))
        .key(&fiote::credentials::slot(&settings))
        .await
        .map_err(io::Error::other)?
        .ok_or_else(|| io::Error::other("Reconnect the selected provider before this live test"))?;
    if fiote::communication::native::has_native_login(&settings) {
        let mut account =
            fiote::communication::auth::Account::decode(&key).map_err(io::Error::other)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_secs();
        println!(
            "native_test_connection_enabled={} access_valid_for_seconds={}",
            account.enabled(),
            account.expires_at.saturating_sub(now)
        );
        if !account.enabled() || account.expires_at <= now.saturating_add(300) {
            return Err(io::Error::other(
                "Renew the connection through normal Lince before this access-token-only test",
            ));
        }
        account.refresh_token = Secret::default();
        key = account.encode().map_err(io::Error::other)?;
    }
    Ok((settings, key))
}
