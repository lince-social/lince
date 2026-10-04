use super::{
    acp,
    adapters::{AuthKind, Catalog},
    connection::{Capabilities, Check, Connection, Profile, Support},
    provider::GenaiProvider,
};
use crate::config::Secret;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Stage {
    #[default]
    Configuration,
    Executable,
    Handshake,
    Authentication,
    Session,
    ModelDiscovery,
}

pub async fn probe(profile: &Profile, catalog: &Catalog, credential: Option<&Secret>) -> Check {
    let mut check = Check {
        profile: profile.id.clone(),
        ready: false,
        detail: String::new(),
        capabilities: Capabilities::default(),
        settings: Value::Null,
        stage: Stage::Configuration,
    };
    let mut configured = profile.clone();
    if let Err(detail) = configured.validate() {
        check.detail = detail;
        return check;
    }
    let profile = &configured;
    let result = match &profile.connection {
        Connection::Harness { config } => {
            check.stage = Stage::Executable;
            match acp::resolve(config) {
                Err(error) => Err(error),
                Ok(_) => {
                    check.stage = Stage::Handshake;
                    match acp::Connection::open(config).await {
                        Err(error) => Err(error),
                        Ok(agent) => {
                            check.capabilities = Capabilities::harness(&agent);
                            check.stage = Stage::Session;
                            let mut status = acp::ConnectionCheck::default();
                            let result = agent.check(config, &[], &mut status).await;
                            agent.close();
                            result.map(|options| {
                                check.capabilities.settings = (!options.options.is_empty()).into();
                                check.settings = json!({"session":options.session,"options":options.options});
                                "ACP session opened; inference and account model entitlement have not been tested.".to_string()
                            })
                        }
                    }
                }
            }
        }
        Connection::Model { settings } => {
            check.capabilities = Capabilities::model();
            let mut settings = settings.clone();
            match catalog
                .validate(&mut settings)
                .and_then(|_| catalog.method(&settings).map(|method| method.kind))
            {
                Err(error) => Err(error),
                Ok(kind) => {
                    check.stage = Stage::Authentication;
                    if kind != AuthKind::None && credential.is_none() {
                        Err(match kind {
                            AuthKind::Browser => "Complete this connection's browser login and unlock its credential vault.".into(),
                            _ => "Unlock the vault and save an API key for this connection.".into(),
                        })
                    } else if catalog.driver(&settings.provider).is_some() {
                        check.capabilities.images = Support::Unsupported;
                        check.capabilities.embedded_content = Support::Unsupported;
                        Ok("Configuration and credential availability checked; inference and account entitlement remain untested. This subscription wrapper supports text and tools, not attachments.".into())
                    } else {
                        check.stage = Stage::ModelDiscovery;
                        match GenaiProvider::new(&settings, &credential.cloned().unwrap_or_default()) {
                            Err(error) => Err(error),
                            Ok(provider) => provider.check_models().await.map(|models| {
                                let selected = models.contains(&settings.model);
                                check.settings = json!({"models":models});
                                if selected { "Endpoint listed the chosen model; inference, tool use and modalities remain untested." } else { "Endpoint model discovery succeeded; selected alias/model entitlement and inference remain unverified." }.into()
                            }),
                        }
                    }
                }
            }
        }
        Connection::External => {
            check.capabilities = Capabilities::external();
            Ok("Open a scoped MCP connection from a conversation. The external client owns chat; automatic activation requires a supported controllable connection.".into())
        }
    };
    match result {
        Ok(detail) => {
            check.ready = true;
            check.detail = detail;
        }
        Err(detail) => check.detail = detail,
    }
    check
}
