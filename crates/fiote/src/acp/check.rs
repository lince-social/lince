use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionCheck {
    pub ready: bool,
    pub agent: String,
    pub login: String,
    pub model: String,
    pub session: String,
    pub detail: String,
}

impl Default for ConnectionCheck {
    fn default() -> Self {
        Self {
            ready: false,
            agent: "Not checked".into(),
            login: "Not checked".into(),
            model: "Not checked".into(),
            session: "Not checked".into(),
            detail: String::new(),
        }
    }
}

impl Connection {
    pub async fn check(
        &self,
        config: &Config,
        providers: &[Value],
        check: &mut ConnectionCheck,
    ) -> Result<SessionOptions, String> {
        *check = ConnectionCheck::default();
        check.agent = format!(
            "{} · {} · {}",
            self.info
                .agent_info
                .as_ref()
                .map(|info| format!("{} {}", info.name, info.version))
                .unwrap_or_else(|| "ACP agent".into()),
            launch::resolve(config)?.display(),
            launch::account()
        );
        if !self.info.agent_capabilities.mcp_capabilities.http {
            return Err(
                "This agent cannot connect to Lince tools: HTTP MCP support is required.".into(),
            );
        }
        check.session = "Opening".into();
        let session = self.options(config).await?;
        check.session = "Opened successfully".into();
        let values = session.values();
        let model = values.get("model").and_then(Value::as_str);
        check.model = model.unwrap_or("Agent default").into();
        check.login = "Accepted session; credentials not independently checked".into();
        if self
            .info
            .agent_info
            .as_ref()
            .is_some_and(|agent| agent.name == "goose")
        {
            let provider = values
                .get("provider")
                .or_else(|| config.session_meta.get("provider"))
                .and_then(Value::as_str)
                .ok_or("Choose a provider, sign in, and check the connection again.")?;
            let entry = providers
                .iter()
                .find(|entry| entry["providerId"] == provider);
            if entry.is_some_and(|entry| entry["acp"] == true) {
                let readiness = tokio::time::timeout(
                    Duration::from_secs(75),
                    self.extension_wait(
                        "_goose/unstable/providers/readiness/check",
                        json!({"providerId":provider}),
                    ),
                )
                .await
                .map_err(|_| "The provider session check timed out.".to_string())??;
                if readiness["ready"] != true {
                    return Err(readiness["error"]
                        .as_str()
                        .unwrap_or("The provider could not open a session. Sign in and try again.")
                        .into());
                }
                check.login = "Provider session accepted".into();
            } else {
                let status = self
                    .extension(
                        "_goose/unstable/providers/config/status",
                        json!({"providerIds":[provider]}),
                    )
                    .await?;
                let configured = status["statuses"]
                    .as_array()
                    .and_then(|statuses| {
                        statuses
                            .iter()
                            .find(|status| status["providerId"] == provider)
                    })
                    .and_then(|status| status["isConfigured"].as_bool());
                if configured == Some(false) {
                    check.login = "Sign-in or provider setup required".into();
                    return Err(
                        "Complete this provider's sign-in or required settings, then check again."
                            .into(),
                    );
                }
                check.login = if configured == Some(true) {
                    "Provider settings present"
                } else {
                    "Provider did not report credential status"
                }
                .into();
                match self
                    .extension(
                        "_goose/unstable/providers/supported-models/list",
                        json!({"providerId":provider}),
                    )
                    .await
                {
                    Ok(reply) => {
                        let models = reply["models"]
                            .as_array()
                            .ok_or("The provider returned an invalid model list.")?;
                        if let Some(model) = model {
                            if models
                                .iter()
                                .any(|candidate| candidate.as_str() == Some(model))
                            {
                                check.model = format!("{model} · listed by provider");
                            } else {
                                check.model = format!("{model} · access not confirmed");
                                check.detail = "The selected model was not returned by the provider's model list. It may be an alias; generation was not tested.".into();
                            }
                        }
                        if models.is_empty() {
                            check.detail = "The provider returned no model list. Model access could not be checked.".into();
                        }
                    }
                    Err(error)
                        if error.to_lowercase().contains("not supported")
                            || error.to_lowercase().contains("not implemented") =>
                    {
                        check.detail = "This provider does not offer a model lookup. Model access could not be checked.".into();
                    }
                    Err(error) => return Err(format!("Provider model lookup failed: {error}")),
                }
            }
        }
        check.ready = true;
        Ok(session)
    }
}
