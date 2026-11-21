use std::collections::BTreeSet;

use serde_json::Value;

use crate::{component::ComponentState, valid_uid};

use super::{
    super::{Component, Descriptor, Setting},
    bounded, workspace_name,
};

const MAX_SETTING_TEXT_BYTES: usize = 65_536;
const MAX_CHOICES: usize = 64;
const MAX_CHOICE_BYTES: usize = 256;
const MAX_COMPONENT_DEPTH: usize = 8;
const MAX_COMPONENT_NODES: usize = 256;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Place,
    Configure,
    Snapshot,
}

impl Setting {
    /// Checks a value against the declared setting type and bounds.
    pub fn accepts(&self, value: &Value) -> bool {
        match self {
            Self::Boolean { .. } => value.is_boolean(),
            Self::Integer { min, max, .. } => {
                value.as_i64().is_some_and(|n| min <= &n && &n <= max)
            }
            Self::Number { min, max, .. } => value
                .as_f64()
                .is_some_and(|n| n.is_finite() && n >= *min && n <= *max),
            Self::Text { max_bytes, .. } => value.as_str().is_some_and(|s| s.len() <= *max_bytes),
            Self::Choice { values, .. } => value
                .as_str()
                .is_some_and(|s| values.iter().any(|v| v == s)),
        }
    }

    pub(in crate::canvas) fn validate(&self) -> bool {
        match self {
            Self::Boolean { .. } => true,
            Self::Integer { default, min, max } => min <= default && default <= max,
            Self::Number { default, min, max } => {
                min.is_finite() && max.is_finite() && min <= default && default <= max
            }
            Self::Text { default, max_bytes } => {
                *max_bytes <= MAX_SETTING_TEXT_BYTES && default.len() <= *max_bytes
            }
            Self::Choice { default, values } => {
                values.len() <= MAX_CHOICES
                    && values
                        .iter()
                        .all(|value| !value.is_empty() && value.len() <= MAX_CHOICE_BYTES)
                    && values.contains(default)
            }
        }
    }
}

impl Descriptor {
    /// Checks the native kind identifier, capability limits, and setting defaults.
    ///
    /// # Errors
    ///
    /// Returns an error if the descriptor cannot be advertised by the Canvas registry.
    pub fn validate(&self) -> Result<(), String> {
        if self.kind.is_empty()
            || self.kind.len() > 80
            || !self
                .kind
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
            || self.name.trim().is_empty()
            || self.name.len() > 256
            || self.description.len() > 4096
            || self.max_bindings > 32
            || self.settings.len() > 64
            || self
                .settings
                .iter()
                .any(|(key, setting)| key.is_empty() || key.len() > 80 || !setting.validate())
            || self
                .unavailable_reason
                .as_ref()
                .is_some_and(|s| s.len() > 1024)
        {
            return Err("Invalid native component descriptor.".into());
        }
        Ok(())
    }
}

impl Component {
    /// Validates a component for placement or configuration in the current registry.
    ///
    /// # Errors
    ///
    /// Returns an error for an unsupported operation, an invalid component or an exceeded limit.
    pub fn validate(&self, registry: &[Descriptor], configuring: bool) -> Result<(), String> {
        let mode = if configuring {
            Mode::Configure
        } else {
            Mode::Place
        };
        self.validate_tree(registry, mode, 0, &mut 0)?;
        bounded(self)
    }

    /// Validates stored content even if the current registry disallows new placements.
    ///
    /// # Errors
    ///
    /// Returns an error for missing descriptors, invalid content, or exceeded limits.
    pub fn validate_snapshot(&self, registry: &[Descriptor]) -> Result<(), String> {
        self.validate_tree(registry, Mode::Snapshot, 0, &mut 0)?;
        bounded(self)
    }

    fn validate_tree(
        &self,
        registry: &[Descriptor],
        mode: Mode,
        depth: usize,
        total: &mut usize,
    ) -> Result<(), String> {
        *total += 1;
        if depth >= MAX_COMPONENT_DEPTH || *total > MAX_COMPONENT_NODES {
            return Err("Compose at most 256 parts within eight levels.".into());
        }
        match self {
            Self::Authored { base, edits } => {
                if matches!(base.as_ref(), Self::Authored { .. }) {
                    return Err("An authored item has one editing layer.".into());
                }
                edits.validate(registry)?;
                base.validate_tree(registry, mode, depth, total)?;
                for part in edits.added.values() {
                    if let Some(component) = &part.component {
                        component.validate_tree(registry, Mode::Place, depth + 1, total)?;
                    }
                }
                Ok(())
            }
            Self::Builtin { state } => {
                state.validate()?;
                Self::validate_builtin_limits(state, depth, total)
            }
            Self::Native {
                kind,
                settings,
                bindings,
            } => {
                let descriptor = registry
                    .iter()
                    .find(|d| &d.kind == kind)
                    .ok_or("Discover a registered native component before using it.")?;
                if mode != Mode::Snapshot
                    && (!descriptor.placeable
                        || mode == Mode::Configure && !descriptor.configurable
                        || depth > 0 && !descriptor.composable)
                {
                    return Err(descriptor
                        .unavailable_reason
                        .as_deref()
                        .unwrap_or("This component does not support this operation.")
                        .into());
                }
                if bindings.len() > descriptor.max_bindings
                    || bindings.iter().any(|uid| !valid_uid(uid, "r"))
                {
                    return Err("Invalid component Record bindings.".into());
                }
                if settings.iter().any(|(key, value)| {
                    descriptor
                        .settings
                        .get(key)
                        .is_none_or(|setting| !setting.accepts(value))
                }) {
                    return Err("The component settings do not match its advertised schema.".into());
                }
                Ok(())
            }
            Self::Composition { composition } => {
                workspace_name(&composition.name)?;
                if composition.parts.is_empty()
                    || composition.origin.as_ref().is_some_and(|origin| {
                        !valid_uid(&origin.agent, "r") || !valid_uid(&origin.thread, "r")
                    })
                {
                    return Err("A composition needs valid parts and origin.".into());
                }
                let mut ids = BTreeSet::new();
                for part in &composition.parts {
                    if part.id.is_empty()
                        || part.id.len() > 80
                        || part.id.chars().any(char::is_control)
                        || !ids.insert(&part.id)
                        || part.events.len() > 32
                        || part.events.iter().any(|event| {
                            event.event.is_empty()
                                || event.event.len() > 128
                                || event.event.chars().any(char::is_control)
                                || !event.action.get("action").is_some_and(Value::is_string)
                        })
                    {
                        return Err("Invalid composition part identity or event binding.".into());
                    }
                    part.geometry.validate()?;
                    part.component
                        .validate_tree(registry, mode, depth + 1, total)?;
                }
                Ok(())
            }
        }
    }

    fn validate_builtin_limits(
        state: &ComponentState,
        depth: usize,
        total: &mut usize,
    ) -> Result<(), String> {
        if let ComponentState::Composition { composition } = state {
            for part in &composition.parts {
                *total += 1;
                if depth + 1 >= MAX_COMPONENT_DEPTH || *total > MAX_COMPONENT_NODES {
                    return Err("Compose at most 256 parts within eight levels.".into());
                }
                Self::validate_builtin_limits(&part.component, depth + 1, total)?;
            }
        }
        Ok(())
    }
}

/// Checks registry limits, unique native kinds, and all descriptors.
///
/// # Errors
///
/// Returns an error if the registry has too many entries, duplicate kinds, or invalid descriptors.
pub fn validate_registry(components: &[Descriptor]) -> Result<(), String> {
    if components.len() > 128 {
        return Err("The component registry exceeds 128 entries.".into());
    }
    let mut kinds = BTreeSet::new();
    for component in components {
        component.validate()?;
        if !kinds.insert(&component.kind) {
            return Err("Component kinds must be unique.".into());
        }
    }
    bounded(components)
}
