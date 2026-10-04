use super::acp;
use serde_json::{Value, json};

pub fn provider_setup_available(info: &Value) -> bool {
    info["agentInfo"]["name"] == "goose"
}

pub fn preserves_provider_metadata(connection: &acp::Connection) -> bool {
    connection
        .info
        .agent_info
        .as_ref()
        .is_some_and(|info| info.name == "goose")
}

pub fn validate_standard_auth(connection: &acp::Connection) -> Result<(), String> {
    if preserves_provider_metadata(connection) {
        Err("Choose a provider from this harness's connections; its generic authentication method does not perform provider login.".into())
    } else {
        Ok(())
    }
}

pub async fn provider_catalog(connection: &acp::Connection) -> Result<Option<Value>, String> {
    if !preserves_provider_metadata(connection) {
        return Ok(None);
    }
    let catalog = connection
        .extension("_goose/unstable/providers/setup/catalog/list", json!({}))
        .await?;
    if catalog.to_string().len() > 262_144 {
        return Err("The agent provider catalog exceeds its size limit.".into());
    }
    Ok(Some(catalog["providers"].clone()))
}

pub async fn save_provider_fields(
    connection: &acp::Connection,
    provider: &str,
    fields: Vec<Value>,
) -> Result<Value, String> {
    if !preserves_provider_metadata(connection) {
        return Err("This harness does not expose provider setup fields.".into());
    }
    connection
        .extension(
            "_goose/unstable/providers/config/save",
            json!({"providerId":provider,"fields":fields}),
        )
        .await
}

pub async fn authenticate_provider(
    connection: &acp::Connection,
    provider: &str,
) -> Result<Value, String> {
    if !preserves_provider_metadata(connection) {
        return Err("This harness does not expose provider authentication.".into());
    }
    connection
        .extension_wait(
            "_goose/unstable/providers/config/authenticate",
            json!({"providerId":provider}),
        )
        .await
}
