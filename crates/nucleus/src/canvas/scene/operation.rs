//! Commands on published local scenes. The host supplies the Actor, never the request body.
use super::{DocumentId, Handle, Scene};
use crate::{
    canvas::request_identifier,
    component::composition::Origin,
    karma::{CanonicalHash, DecimalValue},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Operation {
    PlaceCopy {
        source: Handle,
    },
    Move {
        target: Handle,
        metres: [DecimalValue; 3],
    },
    Undo {
        request_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Edit {
    pub request_id: String,
    pub document: DocumentId,
    pub expected: CanonicalHash,
    pub operation: Operation,
}

impl Edit {
    pub fn validate(&self) -> Result<(), String> {
        request_identifier(&self.request_id)?;
        if !self.document.valid() {
            return Err("Invalid scene document identity.".into());
        }
        match &self.operation {
            Operation::PlaceCopy { source: h } | Operation::Move { target: h, .. }
                if !h.root.valid() || !h.slot.valid() =>
            {
                Err("Invalid scene placement handle.".into())
            }
            Operation::Undo { request_id } => request_identifier(request_id),
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Request {
    Inspect,
    Submit {
        edit: Edit,
    },
    Receipt {
        document: DocumentId,
        request_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Effect {
    Created,
    Moved,
    Removed,
}

impl Effect {
    pub fn inverse(&self) -> Self {
        match self {
            Self::Created => Self::Removed,
            Self::Removed => Self::Created,
            Self::Moved => Self::Moved,
        }
    }
}

/// Captures one invocation on an immutable placement. Two runs on the same source still
/// have different identities. Changing a selection or cancelling retires the live identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Generation {
    pub id: String,
    pub document: DocumentId,
    pub revision: CanonicalHash,
    pub handle: Handle,
}

impl Generation {
    pub fn capture(document: DocumentId, revision: CanonicalHash, handle: Handle) -> Self {
        Self {
            id: crate::new_uid("job"),
            document,
            revision,
            handle,
        }
    }

    pub fn current(
        &self,
        live: Option<&str>,
        document: &DocumentId,
        revision: &CanonicalHash,
        handle: Option<&Handle>,
    ) -> bool {
        live == Some(self.id.as_str())
            && &self.document == document
            && &self.revision == revision
            && handle == Some(&self.handle)
    }
}

/// The given revision is the generation of the newly published source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub edit: Edit,
    /// None identifies the local operator. Fiote provenance comes from its authenticated tool session.
    pub fiote: Option<Origin>,
    pub revision: CanonicalHash,
    pub handle: Handle,
    pub effect: Effect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    pub handle: Handle,
    pub name: String,
    pub metres: [DecimalValue; 3],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Availability {
    pub place_copy: bool,
    pub move_placement: bool,
    pub undo_request: Option<String>,
}

impl Availability {
    pub fn for_scene(
        scene: &Scene,
        revision: &CanonicalHash,
        editable: bool,
        receipt: Option<&Receipt>,
    ) -> Self {
        let has_placement = editable && !scene.slots.is_empty();
        Self {
            place_copy: has_placement && scene.slots.len() < super::MAX_SLOTS,
            move_placement: has_placement,
            undo_request: receipt
                .filter(|r| editable && r.revision == *revision)
                .map(|r| r.edit.request_id.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Inspection {
    pub document: DocumentId,
    pub revision: CanonicalHash,
    pub editable: bool,
    pub available: Availability,
    pub placements: Vec<Placement>,
}

impl Inspection {
    pub fn new(
        scene: &Scene,
        revision: CanonicalHash,
        editable: bool,
        last: Option<&Receipt>,
    ) -> Self {
        let placements = scene
            .slots
            .iter()
            .map(|slot| Placement {
                handle: scene.handle(slot),
                name: scene
                    .definition(&slot.pin(&scene.document))
                    .expect("validated scene definition")
                    .name
                    .clone(),
                metres: slot.metres,
            })
            .collect();
        Self {
            document: scene.document.clone(),
            available: Availability::for_scene(scene, &revision, editable, last),
            revision,
            editable,
            placements,
        }
    }
}
