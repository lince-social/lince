use crate::{
    acp,
    config::{Secret, Settings},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Support {
    Supported,
    Unsupported,
    #[default]
    Unknown,
}

impl From<bool> for Support {
    fn from(value: bool) -> Self {
        if value {
            Self::Supported
        } else {
            Self::Unsupported
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Capabilities {
    pub text: Support,
    pub images: Support,
    pub audio: Support,
    pub video: Support,
    pub embedded_content: Support,
    pub tools: Support,
    pub cancellation: Support,
    pub resume: Support,
    pub settings: Support,
    pub automatic_activation: Support,
}

impl Capabilities {
    pub fn harness(info: &acp::Connection) -> Self {
        let caps = &info.info.agent_capabilities;
        Self {
            text: Support::Supported,
            images: caps.prompt_capabilities.image.into(),
            audio: caps.prompt_capabilities.audio.into(),
            embedded_content: caps.prompt_capabilities.embedded_context.into(),
            tools: if caps.mcp_capabilities.http || crate::adapters::bundled_executable().is_some()
            {
                Support::Supported
            } else {
                Support::Unknown
            },
            cancellation: Support::Supported,
            resume: caps.load_session.into(),
            automatic_activation: Support::Supported,
            ..Default::default()
        }
    }

    pub fn model() -> Self {
        Self {
            text: Support::Supported,
            audio: Support::Unsupported,
            video: Support::Unsupported,
            cancellation: Support::Supported,
            automatic_activation: Support::Supported,
            ..Default::default()
        }
    }

    pub fn external() -> Self {
        Self {
            tools: Support::Supported,
            automatic_activation: Support::Unsupported,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "route", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Connection {
    Harness { config: acp::Config },
    Model { settings: Settings },
    External,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub connection: Connection,
}

impl Profile {
    pub fn validate_metadata(&mut self) -> Result<(), String> {
        if self.id.is_empty()
            || self.id.len() > 80
            || !self
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(
                "Use a connection ID of 1–80 letters, digits, hyphens or underscores.".into(),
            );
        }
        self.name = self.name.trim().into();
        if self.name.is_empty()
            || self.name.chars().count() > 80
            || self.name.chars().any(char::is_control)
        {
            return Err("Use a connection name of 1–80 characters.".into());
        }
        if serde_json::to_vec(self).map_err(|e| e.to_string())?.len() > 32_768 {
            return Err("A connection profile exceeds 32 KiB.".into());
        }
        Ok(())
    }

    pub fn validate(&mut self) -> Result<(), String> {
        self.validate_metadata()?;
        match &mut self.connection {
            Connection::Harness { config } => config.validate()?,
            Connection::Model { settings } => settings.validate()?,
            Connection::External => {}
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profiles {
    pub selected: Option<String>,
    pub entries: Vec<Profile>,
}

impl Profiles {
    pub fn validate(&mut self) -> Result<(), String> {
        if self.entries.len() > 16 {
            return Err("Save at most 16 connections per Fiote.".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for profile in &mut self.entries {
            profile.validate_metadata()?;
            if !ids.insert(&profile.id) {
                return Err("Connection IDs must be unique.".into());
            }
        }
        if self.selected.as_ref().is_some_and(|id| !ids.contains(id)) {
            return Err("The selected connection no longer exists.".into());
        }
        if serde_json::to_vec(self).map_err(|e| e.to_string())?.len() > 524_288 {
            return Err("Saved connections exceed 512 KiB.".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Request {
    Inspect,
    Discover {
        refresh_registry: bool,
    },
    Deselect,
    Save {
        profile: Profile,
    },
    Remove {
        id: String,
    },
    Select {
        id: String,
        api_key: Option<Secret>,
        password: Option<Secret>,
    },
    Check {
        id: String,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Status {
    pub profiles: Profiles,
    pub check: Option<Check>,
    #[serde(default)]
    pub discovery: Option<super::discovery::Discovery>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Check {
    pub profile: String,
    pub ready: bool,
    pub detail: String,
    pub capabilities: Capabilities,
    pub settings: Value,
    #[serde(default)]
    pub stage: super::check::Stage,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_reject_duplicate_missing_and_unsafe_identifiers() {
        let profile = Profile {
            id: "local".into(),
            name: "Local".into(),
            connection: Connection::External,
        };
        let mut profiles = Profiles {
            selected: Some("missing".into()),
            entries: vec![profile.clone()],
        };
        assert!(profiles.validate().is_err());
        profiles.selected = Some("local".into());
        profiles.entries.push(profile);
        assert!(profiles.validate().is_err());
        profiles.entries.pop();
        profiles.entries[0].id = "../other".into();
        assert!(profiles.validate().is_err());
    }

    #[test]
    fn unknown_capabilities_and_external_wakeups_are_not_promised() {
        assert_eq!(Capabilities::model().images, Support::Unknown);
        assert_eq!(
            Capabilities::external().automatic_activation,
            Support::Unsupported
        );
    }
}
