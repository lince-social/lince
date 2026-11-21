// non-lean-yet
// Scope: struct SessionOptions.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::unwrap_used] Propagate or branch on recoverable failures. Prove or document
//   retained invariants. Sites 151.
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 93, 94, 121, 140.
// - [Clippy: clippy::map_err_ignore] Preserve relevant causes and context at the boundary. Sites
//   99, 128.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 9, 72, 86, 110.
// - [Clippy: clippy::wildcard_imports] Use explicit imports while preserving framework and test
//   conventions. Sites 1.
// - [rustc: missing_debug_implementations] type does not implement `std::fmt::Debug`; consider
//   adding `#[derive(Debug)]` or a manual implementation. Sites 3.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 3, 4,
//   5, 9, 54, 72, 86, 110.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites option_value:
//   String (9), options: String (86), set_option: String (110), validate_directories: String (72).

use super::*;

pub struct SessionOptions {
    pub session: String,
    pub options: Vec<SessionConfigOption>,
}

impl SessionOptions {
    pub fn option_value(
        &self,
        id: &str,
        value: &Value,
    ) -> Result<SessionConfigOptionValue, String> {
        let option = self
            .options
            .iter()
            .find(|option| option.id.to_string() == id)
            .ok_or("This agent does not offer that setting. Load its choices again.")?;
        let value = match &option.kind {
            SessionConfigKind::Boolean(_) => SessionConfigOptionValue::boolean(
                value
                    .as_bool()
                    .ok_or("Choose on or off for this setting.")?,
            ),
            SessionConfigKind::Select(select) => {
                let value = value
                    .as_str()
                    .ok_or("Choose a value offered by the agent.")?;
                let contains = |choices: &[SessionConfigSelectOption]| {
                    choices
                        .iter()
                        .any(|choice| choice.value.to_string() == value)
                };
                let allowed = match &select.options {
                    SessionConfigSelectOptions::Ungrouped(choices) => contains(choices),
                    SessionConfigSelectOptions::Grouped(groups) => {
                        groups.iter().any(|group| contains(&group.options))
                    }
                    _ => false,
                };
                if !allowed {
                    return Err(format!(
                        "The agent does not offer {value} for {}. Load its choices again.",
                        option.name
                    ));
                }
                SessionConfigOptionValue::value_id(value.to_string())
            }
            _ => return Err("This agent setting uses an unsupported control.".into()),
        };
        Ok(value)
    }

    pub fn values(&self) -> BTreeMap<String, Value> {
        self.options
            .iter()
            .filter_map(|option| {
                let value = match &option.kind {
                    SessionConfigKind::Select(select) => {
                        Value::from(select.current_value.to_string())
                    }
                    SessionConfigKind::Boolean(toggle) => Value::from(toggle.current_value),
                    _ => return None,
                };
                Some((option.id.to_string(), value))
            })
            .collect()
    }
}

impl Connection {
    pub fn validate_directories(&self, config: &Config) -> Result<(), String> {
        if !config.additional_directories.is_empty()
            && self
                .info
                .agent_capabilities
                .session_capabilities
                .additional_directories
                .is_none()
        {
            return Err("This agent does not advertise additional directory support.".into());
        }
        Ok(())
    }

    pub async fn options(&self, config: &Config) -> Result<SessionOptions, String> {
        self.validate_directories(config)?;
        let response = tokio::time::timeout(
            Duration::from_secs(30),
            self.peer
                .send_request(
                    NewSessionRequest::new(&config.directory)
                        .additional_directories(config.additional_directories.clone())
                        .meta(config.session_meta.clone()),
                )
                .block_task(),
        )
        .await
        .map_err(|_| "The agent settings request timed out.".to_string())?
        .map_err(failure)?;
        let mut options = SessionOptions {
            session: response.session_id.to_string(),
            options: response.config_options.unwrap_or_default(),
        };
        self.apply_options(&mut options, &config.options).await?;
        self.remember_options(&options).await;
        Ok(options)
    }

    pub async fn set_option(
        &self,
        session: &mut SessionOptions,
        id: &str,
        value: &Value,
    ) -> Result<(), String> {
        let value = session.option_value(id, value)?;
        let response = tokio::time::timeout(
            Duration::from_secs(30),
            self.peer
                .send_request(SetSessionConfigOptionRequest::new(
                    session.session.clone(),
                    id.to_string(),
                    value,
                ))
                .block_task(),
        )
        .await
        .map_err(|_| "The agent settings request timed out.".to_string())?
        .map_err(failure)?;
        session.options = response.config_options;
        self.remember_options(session).await;
        Ok(())
    }

    pub(super) async fn apply_options(
        &self,
        session: &mut SessionOptions,
        values: &BTreeMap<String, Value>,
    ) -> Result<(), String> {
        let mut remaining = values.clone();
        while !remaining.is_empty() {
            let id = session.options.iter()
                .filter(|option| remaining.contains_key(&option.id.to_string()))
                .min_by_key(|option| {
                    if option.id.to_string() == "provider" { 0 }
                    else if option.category == Some(SessionConfigOptionCategory::Model) || option.id.to_string() == "model" { 1 }
                    else { 2 }
                })
                .map(|option| option.id.to_string())
                .ok_or("A saved agent setting is no longer available. Reset choices in Fiote settings.")?;
            let value = remaining
                .remove(&id)
                .expect("the selected option is in the remaining settings");
            if session.values().get(&id) != Some(&value) {
                self.set_option(session, &id, &value).await?;
            }
        }
        Ok(())
    }
}
