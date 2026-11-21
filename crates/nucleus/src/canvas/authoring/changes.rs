use std::collections::BTreeSet;

use serde_json::Value;

use super::{
    super::{Descriptor, Geometry, bounded},
    Change, Edits, TokenValue, part_id,
};

impl Edits {
    /// Checks the editing layer's size, ownership, layout, behaviors, and links.
    ///
    /// # Errors
    ///
    /// Returns the first invalid entry, bound, ownership cycle, or serialization failure.
    pub fn validate(&self, _registry: &[Descriptor]) -> Result<(), String> {
        if let Some(size) = self.size {
            Geometry {
                position: [0.0; 2],
                size,
            }
            .validate()?;
        }
        if self.defaults.len() > 512 {
            return Err("Factory defaults exceed 512 edited parts.".into());
        }
        for (id, defaults) in &self.defaults {
            part_id(id)?;
            defaults.style.validate()?;
            if let Some(geometry) = &defaults.geometry {
                geometry.validate()?;
            }
            if defaults.tokens.len() > 64
                || defaults.token_uses.len() > 16
                || defaults.order > 2048
                || defaults
                    .text
                    .as_ref()
                    .is_some_and(|text| text.len() > 65536)
            {
                return Err("Invalid authored defaults.".into());
            }
        }
        if self.parts.len() + self.added.len() > 512
            || self.behaviors.len() > 128
            || self.links.len() > 128
        {
            return Err(
                "An editing layer supports 512 part entries and 128 behaviors or links.".into(),
            );
        }
        for (id, edit) in &self.parts {
            part_id(id)?;
            edit.style.validate()?;
            if let Some(geometry) = &edit.geometry {
                geometry.validate()?;
            }
            if let Some(parent) = &edit.parent {
                part_id(parent)?;
            }
            if let Some(parent) = &edit.layout_parent {
                part_id(parent)?;
                if parent == id || edit.layout_detached {
                    return Err(
                        "Layout attachment needs a distinct parent and cannot also detach.".into(),
                    );
                }
            }
            if edit.order.is_some_and(|order| order > 512)
                || edit.tokens.len() > 64
                || edit.token_uses.len() > 16
            {
                return Err("Part edits exceed their bounds.".into());
            }
            if edit.settings.len() > 64 || edit.text.as_ref().is_some_and(|text| text.len() > 65536)
            {
                return Err("Part text or settings exceed their bounds.".into());
            }
            for (token, value) in &edit.tokens {
                part_id(token)?;
                if matches!(value, TokenValue::Number(value) if !value.is_finite()) {
                    return Err("Token values must be finite.".into());
                }
            }
            for (usage, token) in &edit.token_uses {
                if !matches!(
                    usage.as_str(),
                    "background"
                        | "border"
                        | "text"
                        | "outline"
                        | "cursor"
                        | "font-size"
                        | "spacing"
                        | "padding"
                        | "icon-size"
                        | "icon-padding"
                ) {
                    return Err("Unsupported token use.".into());
                }
                part_id(token)?;
            }
        }
        for (id, part) in &self.added {
            if !id.starts_with("user/") {
                return Err("New part identities start with user/.".into());
            }
            part_id(id)?;
            part_id(&part.parent)?;
            Geometry {
                position: [0.0; 2],
                size: part.size,
            }
            .validate()?;
        }
        let parent_of = |id: &str| {
            self.parts
                .get(id)
                .and_then(|edit| edit.parent.as_deref())
                .or_else(|| self.added.get(id).map(|part| part.parent.as_str()))
        };
        for id in self.parts.keys().chain(self.added.keys()) {
            let mut seen = BTreeSet::from([id.as_str()]);
            let mut parent = parent_of(id);
            while let Some(current) = parent {
                if !seen.insert(current) || seen.len() > 32 {
                    return Err("Part ownership cannot contain cycles or exceed 32 levels.".into());
                }
                parent = parent_of(current);
            }
        }
        for (id, behavior) in &self.behaviors {
            part_id(id)?;
            part_id(&behavior.part)?;
            event_name(&behavior.event)?;
            if !behavior.action.get("action").is_some_and(Value::is_string) {
                return Err("A behavior uses an ordinary registered Lince Action.".into());
            }
        }
        for (id, link) in &self.links {
            part_id(id)?;
            part_id(&link.from)?;
            part_id(&link.to)?;
            event_name(&link.event)?;
            event_name(&link.receive)?;
        }
        if self.groups.len() > 64 {
            return Err("An item supports at most 64 transform groups.".into());
        }
        let mut members = BTreeSet::new();
        for (id, group) in &self.groups {
            if id.len() != 32
                || !id.bytes().all(|byte| byte.is_ascii_hexdigit())
                || group.is_empty()
                || group.len() > 256
            {
                return Err(
                    "A transform group has a hexadecimal identity and 1–256 members.".into(),
                );
            }
            for member in group {
                part_id(member)?;
                if !members.insert(member) {
                    return Err("A part belongs to at most one authored transform group.".into());
                }
            }
        }
        bounded(self)
    }

    /// Applies one authoring operation to the layer.
    ///
    /// # Errors
    ///
    /// Returns an error if the target or the references required by the operation are invalid.
    pub fn apply(&mut self, change: &Change) -> Result<(), String> {
        match change {
            Change::SetPart { part, edit } => {
                self.parts.insert(part.clone(), edit.clone());
            }
            Change::ResetPart { part } => {
                self.parts.remove(part);
            }
            Change::InsertPart { part, definition } => {
                if self.added.contains_key(part) {
                    return Err("This authored part already exists.".into());
                }
                self.added.insert(part.clone(), definition.clone());
            }
            Change::ConfigurePart { part, component } => {
                self.added.get_mut(part).ok_or("Configure an authored part's factory; use its registered settings for renderer-owned parts.")?.component = component.clone();
            }
            Change::RemovePart { part, cascade } => {
                if part == "root" {
                    return Err("Remove the placement to delete its root.".into());
                }
                let mut removed = BTreeSet::from([part.clone()]);
                loop {
                    let count = removed.len();
                    for (id, definition) in &self.added {
                        let parent = self
                            .parts
                            .get(id)
                            .and_then(|edit| edit.parent.as_ref())
                            .unwrap_or(&definition.parent);
                        if removed.contains(parent) && !removed.contains(id) {
                            removed.insert(id.clone());
                        }
                    }
                    if count == removed.len() {
                        break;
                    }
                }
                let references = self
                    .links
                    .values()
                    .any(|link| removed.contains(&link.from) || removed.contains(&link.to))
                    || self
                        .behaviors
                        .values()
                        .any(|behavior| removed.contains(&behavior.part))
                    || self
                        .groups
                        .values()
                        .any(|group| group.iter().any(|member| removed.contains(member)))
                    || self.parts.iter().any(|(id, edit)| {
                        !removed.contains(id)
                            && edit
                                .parent
                                .as_ref()
                                .is_some_and(|parent| removed.contains(parent))
                    });
                if !cascade && (removed.len() > 1 || references) {
                    return Err("This part has children or incoming references; use an explicit cascade or detach them first.".into());
                }
                if references {
                    self.links.retain(|_, link| {
                        !removed.contains(&link.from) && !removed.contains(&link.to)
                    });
                    self.behaviors
                        .retain(|_, behavior| !removed.contains(&behavior.part));
                    for group in self.groups.values_mut() {
                        group.retain(|member| !removed.contains(member));
                    }
                    self.groups.retain(|_, members| !members.is_empty());
                    for edit in self.parts.values_mut() {
                        if edit
                            .parent
                            .as_ref()
                            .is_some_and(|parent| removed.contains(parent))
                        {
                            edit.parent = None;
                        }
                    }
                }
                for id in removed {
                    self.parts.remove(&id);
                    if self.added.remove(&id).is_none() {
                        self.parts.entry(id).or_default().removed = true;
                    } else {
                        self.defaults.retain(|part, _| {
                            part != &id
                                && !part
                                    .strip_prefix(id.as_str())
                                    .is_some_and(|suffix| suffix.starts_with('/'))
                        });
                    }
                }
            }
            Change::SetBehavior { id, behavior } => {
                self.behaviors.insert(id.clone(), behavior.clone());
            }
            Change::RemoveBehavior { id } => {
                if self.behaviors.remove(id).is_none() {
                    return Err("This behavior does not exist.".into());
                }
            }
            Change::SetLink { id, link } => {
                self.links.insert(id.clone(), link.clone());
            }
            Change::RemoveLink { id } => {
                if self.links.remove(id).is_none() {
                    return Err("This link does not exist.".into());
                }
            }
            Change::SetGroup { id, members } => {
                self.groups.insert(id.clone(), members.clone());
            }
            Change::RemoveGroup { id } => {
                if self.groups.remove(id).is_none() {
                    return Err("This transform group does not exist.".into());
                }
            }
        }
        Ok(())
    }
}

fn event_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 128 || name.chars().any(char::is_control) {
        Err("Use an event name of 1–128 bytes.".into())
    } else {
        Ok(())
    }
}

impl Change {
    /// Validates the target identifier and encoded size for this change.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid identifier or an oversized change.
    pub fn validate(&self) -> Result<(), String> {
        let id = match self {
            Self::SetPart { part, .. }
            | Self::ResetPart { part }
            | Self::InsertPart { part, .. }
            | Self::ConfigurePart { part, .. }
            | Self::RemovePart { part, .. } => part,
            Self::SetBehavior { id, .. }
            | Self::RemoveBehavior { id }
            | Self::SetLink { id, .. }
            | Self::RemoveLink { id }
            | Self::SetGroup { id, .. }
            | Self::RemoveGroup { id } => id,
        };
        part_id(id)?;
        bounded(self)
    }
}
