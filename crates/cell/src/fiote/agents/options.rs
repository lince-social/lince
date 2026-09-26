use super::*;

pub(super) struct Preview {
    config: acp::Config,
    pub connection: Arc<acp::Connection>,
    session: acp::SessionOptions,
}

impl Agents {
    pub(super) async fn clear_options(&self, record: &str) {
        if let Some(preview) = self.options.lock().await.remove(record) {
            preview.connection.close();
        }
        if let Some(info) = self
            .info
            .lock()
            .await
            .get_mut(record)
            .and_then(Value::as_object_mut)
        {
            info.remove("configOptions");
        }
    }

    async fn save_options(&self, record: &str, preview: Preview) -> Result<(), String> {
        let options =
            serde_json::to_value(&preview.session.options).map_err(|error| error.to_string())?;
        self.options.lock().await.insert(record.into(), preview);
        self.info
            .lock()
            .await
            .entry(record.into())
            .or_insert_with(|| json!({}))["configOptions"] = options;
        Ok(())
    }
}

impl Host {
    async fn options_available(&self, record: &str) -> Result<(), String> {
        self.record(record).await?;
        if self
            .running
            .lock()
            .await
            .values()
            .any(|run| run.record == record)
        {
            return Err("Stop this Fiote before changing its settings.".into());
        }
        if self
            .agents
            .info
            .lock()
            .await
            .get(record)
            .is_some_and(|info| info["loginPending"] == true)
        {
            return Err("Finish the current agent login first.".into());
        }
        Ok(())
    }

    pub(in crate::fiote) async fn agent_options(
        &self,
        record: &str,
        mut config: acp::Config,
    ) -> Result<(), String> {
        self.options_available(record).await?;
        config.validate()?;
        config.require_vault = false;
        {
            let options = self.agents.options.lock().await;
            if options.len() >= 16 && !options.contains_key(record) {
                return Err(
                    "Use /lock to close agent connections before loading more settings.".into(),
                );
            }
        }
        let connection = acp::Connection::open(&config).await?;
        let session = match connection.options(&config).await {
            Ok(session) => session,
            Err(error) => {
                connection.close();
                return Err(error);
            }
        };
        if let Err(error) = self.configure_agent(record, config.clone()).await {
            connection.close();
            return Err(error);
        }
        self.agents
            .save_options(
                record,
                Preview {
                    config,
                    connection,
                    session,
                },
            )
            .await
    }

    pub(in crate::fiote) async fn set_agent_option(
        &self,
        record: &str,
        option: &str,
        value: &Value,
    ) -> Result<(), String> {
        self.options_available(record).await?;
        let mut preview = self
            .agents
            .options
            .lock()
            .await
            .remove(record)
            .ok_or("Load this Fiote's choices before changing a setting.")?;
        let saved = self
            .load(record)?
            .and_then(|configuration| configuration.agent);
        if saved.as_ref() != Some(&preview.config) {
            preview.connection.close();
            return Err("Fiote settings changed. Load its choices again.".into());
        }
        if let Err(error) = preview
            .connection
            .set_option(&mut preview.session, option, value)
            .await
        {
            self.agents.clear_options(record).await;
            preview.connection.close();
            return Err(error);
        }
        let current = preview.session.values();
        preview.config.options.insert(option.into(), value.clone());
        preview.config.options.retain(|id, value| {
            if let Some(actual) = current.get(id) {
                *value = actual.clone();
                true
            } else {
                false
            }
        });
        if option == "provider"
            && preview
                .connection
                .info
                .agent_info
                .as_ref()
                .is_some_and(|info| info.name == "goose")
        {
            if let Some(provider) = current.get(option) {
                preview
                    .config
                    .session_meta
                    .insert("provider".into(), provider.clone());
            }
        }
        if let Err(error) = self.configure_agent(record, preview.config.clone()).await {
            preview.connection.close();
            return Err(error);
        }
        self.agents.save_options(record, preview).await
    }
}
