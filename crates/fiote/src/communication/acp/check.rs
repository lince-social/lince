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
        check.session = "Opening".into();
        let session = self.options(config).await?;
        check.session = "Opened successfully".into();
        let values = session.values();
        let model = values.get("model").and_then(Value::as_str);
        check.model = model.unwrap_or("Agent default").into();
        check.login = "Accepted session; credentials not independently checked".into();
        check.ready = true;
        Ok(session)
    }
}
