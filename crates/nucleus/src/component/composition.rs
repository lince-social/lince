use super::ComponentState;
use serde::{Deserialize, Serialize};

pub const FORMAT: &str = "lince.custom_component";
pub const MAX_BYTES: usize = 256 * 1024;

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Immunity {
    #[default]
    None,
    External,
    Internal,
    All,
    Containment,
    Isolation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Part {
    pub id: String,
    #[serde(default)]
    pub position: [i32; 2],
    pub size: [u32; 2],
    pub component: ComponentState,
    #[serde(default)]
    pub events: Vec<EventBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EventBinding {
    pub event: String,
    pub action: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Composition {
    pub name: String,
    pub parts: Vec<Part>,
    #[serde(default)]
    pub origin: Option<Origin>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Origin {
    pub agent: String,
    pub thread: String,
}

impl Composition {
    pub fn validate(&self) -> Result<(), String> {
        if serde_json::to_vec(self).map_err(|e| e.to_string())?.len() > MAX_BYTES {
            return Err("Composition exceeds 256 KiB".into());
        }
        self.validate_tree(0, &mut 0)
    }

    fn validate_tree(&self, depth: usize, total: &mut usize) -> Result<(), String> {
        if self.origin.as_ref().is_some_and(|origin| {
            !crate::valid_uid(&origin.agent, "r") || !crate::valid_uid(&origin.thread, "r")
        }) {
            return Err("Composition origin requires stable Agent and conversation UIDs".into());
        }
        if depth >= 8
            || self.name.trim().is_empty()
            || self.name.chars().count() > 80
            || self.name.chars().any(char::is_control)
            || self.parts.is_empty()
        {
            return Err(
                "Use a name of 1–80 characters, nonempty parts and at most 8 nested compositions"
                    .into(),
            );
        }
        let mut ids = std::collections::BTreeSet::new();
        for part in &self.parts {
            if part.events.len() > 32
                || part.events.iter().any(|binding| {
                    binding.event.trim().is_empty()
                        || binding.event.len() > 128
                        || binding.event.chars().any(char::is_control)
                        || !binding.action.is_object()
                        || binding.action["action"].as_str().is_none()
                })
            {
                return Err(
                    "A part supports at most 32 named event bindings to ordinary Actions".into(),
                );
            }
            *total += 1;
            if *total > 256
                || part.id.is_empty()
                || part.id.len() > 80
                || part.id.chars().any(char::is_control)
                || !ids.insert(&part.id)
                || part.position.iter().any(|v| v.unsigned_abs() > 100_000)
                || part.size.iter().any(|v| !(1..=10_000).contains(v))
            {
                return Err("Invalid or duplicate part ID, bounds, or more than 256 parts".into());
            }
            match &part.component {
                ComponentState::Composition { composition } => {
                    composition.validate_tree(depth + 1, total)?
                }
                component => component.validate()?,
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub format: String,
    pub composition: Composition,
}

impl Document {
    pub fn decode(body: &str) -> Result<Self, String> {
        if body.len() > MAX_BYTES {
            return Err("Composition exceeds 256 KiB".into());
        }
        let document: Self = serde_json::from_str(body).map_err(|e| e.to_string())?;
        if document.format != FORMAT {
            return Err("Invalid composition format".into());
        }
        document.composition.validate()?;
        Ok(document)
    }

    pub fn encode(composition: Composition) -> Result<String, String> {
        composition.validate()?;
        let body = serde_json::to_string(&Self {
            format: FORMAT.into(),
            composition,
        })
        .map_err(|e| e.to_string())?;
        if body.len() > MAX_BYTES {
            return Err("Composition exceeds 256 KiB".into());
        }
        Ok(body)
    }
}
