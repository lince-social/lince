use super::*;

impl Host {
    pub(in crate::fiote) async fn check_agent(
        &self,
        record: &str,
        mut config: acp::Config,
    ) -> Result<(), String> {
        self.options_available(record).await?;
        config.require_vault = false;
        let full = {
            let discovery = self.agents.discovery.lock().await;
            discovery.len() >= 16 && !discovery.contains_key(record)
        };
        if full {
            return Err("Close other agent connections with /lock before checking another.".into());
        }
        let mut check = acp::ConnectionCheck::default();
        let result = match config.validate() {
            Ok(()) => self.check_agent_inner(record, config, &mut check).await,
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            check.ready = false;
            check.detail = error;
            if check.agent == "Not checked" {
                check.agent = "Could not start".into();
            }
            if check.session == "Opening" {
                check.session = "Could not open".into();
            }
        }
        self.agents
            .info
            .lock()
            .await
            .entry(record.into())
            .or_insert_with(|| json!({}))["connectionCheck"] =
            serde_json::to_value(check).map_err(|error| error.to_string())?;
        Ok(())
    }

    async fn check_agent_inner(
        &self,
        record: &str,
        config: acp::Config,
        check: &mut acp::ConnectionCheck,
    ) -> Result<(), String> {
        let connection = acp::Connection::open(&config).await?;
        check.agent = "Connected".into();
        let mut info = serde_json::to_value(&connection.info).map_err(|error| error.to_string())?;
        if let Some(providers) = fiote::communication::extensions::provider_catalog(&connection).await? {
            info["providers"] = providers;
            if let Some(provider) = config.options.get("provider").or_else(|| config.session_meta.get("provider")) {
                info["selectedProvider"] = info["providers"].as_array().and_then(|entries| entries.iter().find(|entry| entry["providerId"] == *provider)).cloned().unwrap_or(Value::Null);
            }
        }
        self.agents.clear_options(record).await;
        if let Some((_, previous)) = self
            .agents
            .discovery
            .lock()
            .await
            .insert(record.into(), (config.clone(), connection.clone()))
        {
            previous.close();
        }
        if let Some(previous) = self.agents.info.lock().await.get(record) {
            if previous["selectedProvider"]["providerId"] == info["selectedProvider"]["providerId"]
            {
                for field in ["loginAgent", "loginResult"] {
                    if let Some(value) = previous.get(field) {
                        info[field] = value.clone();
                    }
                }
            }
        }
        self.agents
            .info
            .lock()
            .await
            .insert(record.into(), info.clone());
        let providers = info["providers"].as_array().cloned().unwrap_or_default();
        let session = connection.check(&config, &providers, check).await?;
        if self.load(record)?.and_then(|saved| saved.agent).as_ref() != Some(&config) {
            self.configure_agent(record, config.clone()).await?;
        }
        self.agents.clear_options(record).await;
        self.agents
            .save_options(
                record,
                options::Preview {
                    config: config.clone(),
                    connection: connection.clone(),
                    session,
                },
            )
            .await?;
        Ok(())
    }
}
