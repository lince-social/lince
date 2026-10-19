use crate::{Engine, EngineError, signal_streams::invalid};
use serde::{Deserialize, Serialize};

pub const NAMESPACE: &str = "lince.ailuros";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub enabled: bool,
    pub iroh: bool,
    pub https: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            iroh: true,
            https: false,
        }
    }
}

impl Settings {
    pub fn parse(value: serde_json::Value) -> Result<Self, EngineError> {
        let settings: Self = serde_json::from_value(value)
            .map_err(|_| invalid("Invalid hardware receiver settings"))?;
        if settings.enabled && !settings.iroh && !settings.https {
            return Err(invalid("Choose a hardware receiver transport"));
        }
        Ok(settings)
    }
}

impl Engine {
    pub async fn ailuros_receiver_settings(&self) -> Result<Settings, EngineError> {
        match store::cells::config(&self.store.pool, NAMESPACE).await? {
            Some(value) => Settings::parse(value),
            None => Ok(Settings::default()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn receiver_settings_default_disabled_and_refuse_unknown_or_unusable_options() {
        assert!(!Settings::default().enabled);
        assert_eq!(Settings::parse(json!({})).unwrap(), Settings::default());
        assert!(Settings::parse(json!({"enabled":true})).unwrap().iroh);
        for value in [
            json!({"enabled":"true"}),
            json!({"shell":"anything"}),
            json!({"enabled":true,"iroh":false,"https":false}),
            json!(null),
        ] {
            assert!(Settings::parse(value).is_err());
        }
    }
}
