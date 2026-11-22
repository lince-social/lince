//! Local, single-root model composition. Placement coordinates are exact metres in the root frame.
use crate::{
    id::{new_uid, valid_uid},
    karma::{CanonicalHash, DecimalValue, canonical_hash},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::Arc};

pub const MAX_PARTS: usize = 64;
pub const MAX_SLOTS: usize = 128;

macro_rules! identity {
    ($name:ident, $prefix:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);
        impl $name {
            fn new() -> Self {
                Self(new_uid($prefix))
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
            pub fn valid(&self) -> bool {
                valid_uid(&self.0, $prefix)
            }
        }
    };
}
identity!(DocumentId, "doc");
identity!(RootId, "root");
identity!(PartId, "part");
identity!(SlotId, "slot");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Handle {
    pub root: RootId,
    pub slot: SlotId,
}

/// Local component index within a pinned definition. A plain part exposes its origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElementId(pub u32);
pub const PART_ORIGIN: ElementId = ElementId(0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Part {
    pub id: PartId,
    pub name: String,
}

impl Part {
    pub fn revision(&self) -> Result<CanonicalHash, String> {
        canonical_hash("lince.workspace.definition.v1", self).map_err(|e| e.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slot {
    pub id: SlotId,
    pub part: PartId,
    pub revision: CanonicalHash,
    pub metres: [DecimalValue; 3],
    pub record: Option<String>,
}

/// A scene root uses one existing Room frame; the handle identifies its direct member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanvasBinding {
    pub handle: Handle,
    pub room: String,
    pub placement: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scene {
    pub document: DocumentId,
    pub root: RootId,
    pub canvas: Option<CanvasBinding>,
    pub parts: Arc<Vec<Part>>,
    pub slots: Arc<Vec<Slot>>,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            document: DocumentId::new(),
            root: RootId::new(),
            canvas: None,
            parts: Arc::default(),
            slots: Arc::default(),
        }
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name.trim() == name
        && !name.chars().any(char::is_control)
}

fn normalized(value: DecimalValue) -> DecimalValue {
    let mut scale = value.scale();
    let mut mantissa = value.mantissa();
    while scale > 0 && mantissa % 10 == 0 {
        scale -= 1;
        mantissa /= 10;
    }
    DecimalValue::from_mantissa(scale, mantissa).expect("reducing a valid decimal scale")
}

impl Scene {
    pub fn validate(&self) -> Result<(), String> {
        if !self.document.valid()
            || !self.root.valid()
            || self.parts.len() > MAX_PARTS
            || self.slots.len() > MAX_SLOTS
        {
            return Err("Invalid scene identity or part/placement count.".into());
        }
        if let Some(binding) = &self.canvas
            && (!valid_uid(&binding.room, "placement")
                || !valid_uid(&binding.placement, "placement")
                || binding.room == binding.placement
                || self.slot(&binding.handle).is_none())
        {
            return Err("Invalid Room, placement or occurrence in canvas binding.".into());
        }
        let mut revisions = std::collections::BTreeMap::new();
        for part in self.parts.iter() {
            if !part.id.valid()
                || !valid_name(&part.name)
                || revisions.insert(&part.id, part.revision()?).is_some()
            {
                return Err("Invalid or duplicate part definition.".into());
            }
        }
        let mut ids = BTreeSet::new();
        for slot in self.slots.iter() {
            if slot.metres != slot.metres.map(normalized)
                || !slot.id.valid()
                || !ids.insert(&slot.id)
                || revisions.get(&slot.part) != Some(&slot.revision)
                || slot.record.as_ref().is_some_and(|r| !valid_uid(r, "r"))
            {
                return Err(
                    "Invalid placement identity, Record binding or pinned definition.".into(),
                );
            }
        }
        Ok(())
    }

    pub fn add_part(&mut self, name: &str) -> Result<Handle, String> {
        if !valid_name(name) || self.parts.len() >= MAX_PARTS {
            return Err("Use a part name of 1–128 bytes and at most 64 definitions.".into());
        }
        let part = Part {
            id: PartId::new(),
            name: name.into(),
        };
        let handle = self.insert_slot(&part)?;
        Arc::make_mut(&mut self.parts).push(part);
        Ok(handle)
    }

    fn insert_slot(&mut self, part: &Part) -> Result<Handle, String> {
        if self.slots.len() >= MAX_SLOTS {
            return Err("The scene already has 128 placements.".into());
        }
        let slot = Slot {
            id: SlotId::new(),
            part: part.id.clone(),
            revision: part.revision()?,
            metres: [DecimalValue::from_integer(0); 3],
            record: None,
        };
        let handle = self.handle(&slot);
        Arc::make_mut(&mut self.slots).push(slot);
        Ok(handle)
    }

    pub fn handle(&self, slot: &Slot) -> Handle {
        Handle {
            root: self.root.clone(),
            slot: slot.id.clone(),
        }
    }

    pub fn slot(&self, handle: &Handle) -> Option<&Slot> {
        if self.root != handle.root {
            return None;
        }
        self.slots.iter().find(|s| s.id == handle.slot)
    }

    pub fn place_again(&mut self, handle: &Handle) -> Result<Handle, String> {
        let slot = self
            .slot(handle)
            .ok_or("Select a placement in this scene.")?;
        let part = self
            .parts
            .iter()
            .find(|p| p.id == slot.part)
            .ok_or("Missing definition.")?
            .clone();
        self.insert_slot(&part)
    }

    pub fn edit(
        &mut self,
        handle: &Handle,
        metres: [DecimalValue; 3],
        record: Option<String>,
    ) -> Result<(), String> {
        if record.as_ref().is_some_and(|r| !valid_uid(r, "r")) {
            return Err("Enter a valid Record UID or leave it empty.".into());
        }
        if handle.root != self.root {
            return Err("The selected placement is unavailable.".into());
        }
        let slot = Arc::make_mut(&mut self.slots)
            .iter_mut()
            .find(|s| s.id == handle.slot)
            .ok_or("The selected placement is unavailable.")?;
        slot.metres = metres.map(normalized);
        slot.record = record;
        Ok(())
    }
}
