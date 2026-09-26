use super::*;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionState {
    pub options: Vec<SessionConfigOption>,
    pub commands: Vec<AvailableCommand>,
    pub plan: Option<Plan>,
    pub usage: Option<UsageUpdate>,
    #[serde(default)]
    pub usage_updated_ms: u64,
}

impl SessionState {
    pub(super) fn update(&mut self, update: &SessionUpdate) {
        match update {
            SessionUpdate::ConfigOptionUpdate(update) => {
                self.options = update.config_options.clone();
            }
            SessionUpdate::AvailableCommandsUpdate(update) => {
                self.commands = update.available_commands.clone();
            }
            SessionUpdate::Plan(plan) => self.plan = Some(plan.clone()),
            SessionUpdate::UsageUpdate(usage) => {
                if self.usage.as_ref() != Some(usage) {
                    self.usage = Some(usage.clone());
                    self.usage_updated_ms = nucleus::operation::now_ms();
                }
            }
            _ => {}
        }
    }
}

impl Connection {
    pub async fn state(&self, session: &str) -> SessionState {
        self.states
            .lock()
            .await
            .get(session)
            .cloned()
            .unwrap_or_default()
    }

    pub(super) async fn remember_options(&self, session: &SessionOptions) {
        let mut states = self.states.lock().await;
        if states.len() < 16 || states.contains_key(&session.session) {
            states.entry(session.session.clone()).or_default().options = session.options.clone();
        }
    }

    pub async fn change_option(
        &self,
        session: &str,
        id: &str,
        value: &Value,
    ) -> Result<(), String> {
        let mut options = SessionOptions {
            session: session.into(),
            options: self.state(session).await.options,
        };
        self.set_option(&mut options, id, value).await
    }
}

impl SessionState {
    pub fn usage_report(&self, id: &str, source: &str) -> Option<nucleus::operation::Usage> {
        let usage = self.usage.as_ref()?;
        Some(nucleus::operation::Usage {
            id: id.into(),
            source: source.into(),
            scope: "cumulative session".into(),
            updated_ms: self.usage_updated_ms,
            context_used: Some(usage.used),
            context_capacity: Some(usage.size),
            cost: usage
                .cost
                .as_ref()
                .filter(|cost| cost.amount.is_finite() && cost.amount >= 0.0)
                .map(|cost| nucleus::operation::Cost {
                    amount: cost.amount,
                    currency: cost.currency.clone(),
                    estimated: false,
                }),
            ..Default::default()
        })
    }
}
