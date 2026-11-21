// non-lean-yet
// Scope: struct ConnectionCheck.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::map_unwrap_or] called `map(<f>).unwrap_or_else(<g>)` on an `Option` value.
//   Sites 35.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 27.
// - [Clippy: clippy::wildcard_imports] Use explicit imports while preserving framework and test
//   conventions. Sites 1.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 4, 5,
//   6, 7, 8, 9, 10, 27.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites check: String
//   (27).

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
