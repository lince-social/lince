//! Models for Canvas components, workspaces, and authoring requests.
//!
//! [Component](crate::canvas::Component) describes a placement's content.
//! [Snapshot](crate::canvas::Snapshot) carries a workspace view, and
//! [Request](crate::canvas::Request) and [Response](crate::canvas::Response)
//! form the authoring protocol.

use std::{collections::BTreeMap, ops::ControlFlow};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, from_str, to_string};

use crate::{
    component::{ComponentState, EventBinding, composition::Origin},
    terrain::SpatialBinding,
};

/// The byte limit for a Canvas document, request, or snapshot page.
pub const MAX_BYTES: usize = 256 * 1024;
/// The placement limit for a snapshot page or batch operation.
pub const MAX_PLACEMENTS: usize = 256;
/// The placement limit for a complete stored Canvas.
pub const MAX_STATE_PLACEMENTS: usize = 16_384;

pub mod appearance;
pub mod authoring;
pub mod room;
pub mod scene;
mod state;
mod validation;

#[cfg(test)]
mod tests;

pub use state::State;
pub use validation::{bounded, placement_id, request_identifier, validate_registry};

use validation::workspace_name;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Setting {
    Boolean {
        default: bool,
    },
    Integer {
        default: i64,
        min: i64,
        max: i64,
    },
    Number {
        default: f64,
        min: f64,
        max: f64,
    },
    Text {
        default: String,
        max_bytes: usize,
    },
    Choice {
        default: String,
        values: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Descriptor {
    pub kind: String,
    pub name: String,
    pub description: String,
    pub settings: BTreeMap<String, Setting>,
    pub max_bindings: usize,
    pub placeable: bool,
    pub configurable: bool,
    pub composable: bool,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "source", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Component {
    Authored {
        base: Box<Component>,
        edits: authoring::Edits,
    },
    Builtin {
        state: ComponentState,
    },
    Native {
        kind: String,
        settings: BTreeMap<String, Value>,
        bindings: Vec<String>,
    },
    Composition {
        composition: Composition,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Composition {
    pub name: String,
    pub parts: Vec<Part>,
    pub origin: Option<Origin>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Part {
    pub id: String,
    pub geometry: Geometry,
    pub component: Component,
    pub events: Vec<EventBinding>,
}

impl Component {
    /// Collects Record references in traversal order.
    pub fn records(&self) -> Vec<&str> {
        let mut records = Vec::new();
        let _: ControlFlow<()> = self.visit_records(&mut |record| {
            records.push(record);
            ControlFlow::Continue(())
        });
        records
    }

    /// Visits Record references without collecting them and propagates an early exit.
    pub fn visit_records<'a, B>(
        &'a self,
        visit: &mut impl FnMut(&'a str) -> ControlFlow<B>,
    ) -> ControlFlow<B> {
        match self {
            Self::Authored { base, edits } => {
                base.visit_records(visit)?;
                for component in edits
                    .added
                    .values()
                    .filter_map(|part| part.component.as_ref())
                {
                    component.visit_records(visit)?;
                }
            }
            Self::Builtin { state } => state.visit_records(visit)?,
            Self::Native { bindings, .. } => {
                for record in bindings {
                    visit(record)?;
                }
            }
            Self::Composition { composition } => {
                for part in &composition.parts {
                    part.component.visit_records(visit)?;
                }
            }
        }
        ControlFlow::Continue(())
    }

    /// Borrows the authored origin if the component is a composition.
    pub fn origin(&self) -> Option<&Origin> {
        match self {
            Self::Authored { base, .. } => base.origin(),
            Self::Builtin {
                state: ComponentState::Composition { composition },
            } => composition.origin.as_ref(),
            Self::Composition { composition } => composition.origin.as_ref(),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub format: String,
    pub name: String,
    pub component: Component,
}

impl Document {
    /// Format identifier for a standalone Canvas component.
    pub const FORMAT: &str = "lince.canvas_component";
    /// Decodes a component document and validates its envelope.
    ///
    /// # Errors
    ///
    /// Returns an error for oversized or malformed JSON, an unknown format, or an invalid name.
    pub fn decode(body: &str) -> Result<Self, String> {
        if body.len() > MAX_BYTES {
            return Err("The canvas component definition exceeds 256 KiB.".into());
        }
        let document: Self = from_str(body).map_err(|error| error.to_string())?;
        if document.format != Self::FORMAT {
            return Err("Unknown canvas component definition format.".into());
        }
        workspace_name(&document.name)?;
        Ok(document)
    }
    /// Encodes a component as a standalone Canvas document.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid name or a document that cannot be encoded within the limit.
    pub fn encode(name: String, component: Component) -> Result<String, String> {
        workspace_name(&name)?;
        let document = Self {
            format: Self::FORMAT.into(),
            name,
            component,
        };
        bounded(&document)?;
        to_string(&document).map_err(|error| error.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Geometry {
    pub position: [f64; 2],
    pub size: [f64; 2],
}

impl Geometry {
    /// Checks that positions are finite and dimensions are positive and bounded.
    ///
    /// # Errors
    ///
    /// Returns an error if either position or size is invalid.
    pub fn validate(&self) -> Result<(), String> {
        if self.position.iter().any(|n| !n.is_finite())
            || self
                .size
                .iter()
                .any(|n| !n.is_finite() || *n <= 0.0 || *n > 100_000.0)
        {
            return Err(
                "Use finite canvas positions and positive sizes of at most 100,000.".into(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct View {
    pub spatial: bool,
    pub position: [f64; 3],
    pub yaw: f64,
    pub pitch: f64,
    pub plane: f64,
    pub selection_depth: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Pinned {
    pub anchor: [f64; 2],
    pub scale: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Pose {
    pub position: [f64; 3],
    pub rotation: [f64; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Spatial {
    pub elevation: f64,
    pub rotation: [f64; 4],
    pub depth: Option<f64>,
    pub world_pinned: bool,
}

impl Default for Spatial {
    fn default() -> Self {
        Self {
            elevation: 0.0,
            rotation: [0.0, 0.0, 0.0, 1.0],
            depth: None,
            world_pinned: false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    #[serde(default)]
    pub room: Option<room::Room>,
    #[serde(default)]
    pub room_member: Option<room::Member>,
    #[serde(default)]
    pub material: Option<appearance::Material>,
    #[serde(default)]
    pub light: Option<appearance::Light>,
    #[serde(default)]
    pub binding: Option<SpatialBinding>,
    #[serde(default)]
    pub follow_slope: bool,
    pub order: i32,
    pub pinned: Option<Pinned>,
    pub event_boundary: Vec<String>,
    pub spatial: Spatial,
    pub attachment: Option<Pose>,
    pub group_pose: Option<Pose>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    pub id: u64,
    pub name: String,
    pub center: [f64; 2],
    pub zoom: f64,
    pub view: Option<View>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    pub id: String,
    pub workspace: u64,
    pub component: Component,
    pub geometry: Geometry,
    pub selected: bool,
    pub group: Option<String>,
    pub metadata: Metadata,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub revision: u64,
    pub active_workspace: u64,
    pub workspaces: Vec<Workspace>,
    pub placements: Vec<Placement>,
    pub next_offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
#[expect(
    clippy::large_enum_variant,
    reason = "AddWithMetadata retains its public inline component and metadata"
)]
pub enum Mutation {
    SetRoomMembers {
        members: BTreeMap<String, Option<room::Member>>,
    },
    SetMetadata {
        placement: String,
        metadata: Metadata,
    },
    Rebind {
        placement: String,
        records: BTreeMap<String, String>,
    },
    Patch {
        placement: String,
        changes: Vec<authoring::Change>,
    },
    Duplicate {
        placement: String,
        copy: String,
        workspace: u64,
        position: [f64; 2],
    },
    Group {
        placements: Vec<String>,
        group: Option<String>,
    },
    Add {
        placement: String,
        workspace: u64,
        component: Component,
        geometry: Geometry,
    },
    AddWithMetadata {
        placement: String,
        workspace: u64,
        component: Component,
        geometry: Geometry,
        metadata: Metadata,
    },
    Configure {
        placement: String,
        component: Component,
    },
    Move {
        placement: String,
        workspace: u64,
        position: [f64; 2],
    },
    Resize {
        placement: String,
        size: [f64; 2],
    },
    Remove {
        placement: String,
    },
    CreateWorkspace {
        name: String,
    },
    RenameWorkspace {
        workspace: u64,
        name: String,
    },
    SwitchWorkspace {
        workspace: u64,
    },
    RemoveWorkspace {
        workspace: u64,
    },
    Undo {
        receipt: String,
    },
    UndoChanges {
        receipt: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
#[expect(
    clippy::large_enum_variant,
    reason = "Mutate retains its public inline Mutation payload"
)]
pub enum Request {
    InspectElement {
        placement: String,
        offset: usize,
        limit: usize,
    },
    Export {
        placement: String,
        name: String,
    },
    Inspect {
        workspace: Option<u64>,
        selected_only: bool,
        offset: usize,
        limit: usize,
    },
    Registry,
    Mutate {
        request_id: String,
        expected_revision: u64,
        mutation: Mutation,
    },
    Receipt {
        request_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Persistence {
    Applied,
    Saved,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub request_id: String,
    pub revision: u64,
    pub persistence: Persistence,
    pub affected_placements: Vec<String>,
    pub affected_count: usize,
    pub workspace: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "result", rename_all = "kebab-case", deny_unknown_fields)]
#[expect(
    clippy::large_enum_variant,
    reason = "Element retains its public inline authoring payload"
)]
pub enum Response {
    Element {
        element: authoring::Element,
    },
    Export {
        document: Document,
        hash: String,
        revision: u64,
    },
    Snapshot {
        snapshot: Snapshot,
    },
    Registry {
        components: Vec<Descriptor>,
    },
    Receipt {
        receipt: Receipt,
    },
}
