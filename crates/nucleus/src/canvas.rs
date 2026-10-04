use crate::component::ComponentState;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_BYTES: usize = 256 * 1024;
pub const MAX_PLACEMENTS: usize = 256;
pub const MAX_STATE_PLACEMENTS: usize = 16_384;

mod state;
pub use state::State;

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

impl Setting {
    pub fn accepts(&self, value: &serde_json::Value) -> bool {
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

    fn validate(&self) -> bool {
        let default = match self {
            Self::Boolean { default } => serde_json::json!(default),
            Self::Integer { default, min, max } if min <= max => serde_json::json!(default),
            Self::Number { default, min, max }
                if min.is_finite() && max.is_finite() && min <= max =>
            {
                serde_json::json!(default)
            }
            Self::Text { default, max_bytes } if *max_bytes <= 65_536 => serde_json::json!(default),
            Self::Choice { default, values }
                if !values.is_empty()
                    && values.len() <= 64
                    && values.iter().all(|v| !v.is_empty() && v.len() <= 256) =>
            {
                serde_json::json!(default)
            }
            _ => return false,
        };
        self.accepts(&default)
    }
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

impl Descriptor {
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "source", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Component {
    Builtin {
        state: ComponentState,
    },
    Native {
        kind: String,
        settings: BTreeMap<String, serde_json::Value>,
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
    pub origin: Option<crate::component::composition::Origin>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Part {
    pub id: String,
    pub geometry: Geometry,
    pub component: Component,
    pub events: Vec<crate::component::EventBinding>,
}

impl Component {
    pub fn validate(&self, registry: &[Descriptor], configuring: bool) -> Result<(), String> {
        self.validate_tree(registry, Some(configuring), 0, &mut 0)?;
        bounded(self)
    }

    pub fn validate_snapshot(&self, registry: &[Descriptor]) -> Result<(), String> {
        self.validate_tree(registry, None, 0, &mut 0)?;
        bounded(self)
    }

    fn validate_tree(
        &self,
        registry: &[Descriptor],
        operation: Option<bool>,
        depth: usize,
        total: &mut usize,
    ) -> Result<(), String> {
        *total += 1;
        if depth >= 8 || *total > 256 {
            return Err("Compose at most 256 parts within eight levels.".into());
        }
        match self {
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
                if operation.is_some_and(|configuring| {
                    !descriptor.placeable
                        || configuring && !descriptor.configurable
                        || depth > 0 && !descriptor.composable
                }) {
                    return Err(descriptor
                        .unavailable_reason
                        .clone()
                        .unwrap_or("This component does not support this operation.".into()));
                }
                if bindings.len() > descriptor.max_bindings
                    || bindings.iter().any(|uid| !crate::valid_uid(uid, "r"))
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
                        !crate::valid_uid(&origin.agent, "r")
                            || !crate::valid_uid(&origin.thread, "r")
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
                                || !event.action.is_object()
                                || !event.action["action"].is_string()
                        })
                    {
                        return Err("Invalid composition part identity or event binding.".into());
                    }
                    part.geometry.validate()?;
                    part.component
                        .validate_tree(registry, operation, depth + 1, total)?;
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
                if depth + 1 >= 8 || *total > 256 {
                    return Err("Compose at most 256 parts within eight levels.".into());
                }
                Self::validate_builtin_limits(&part.component, depth + 1, total)?;
            }
        }
        Ok(())
    }

    pub fn records(&self) -> Vec<&str> {
        match self {
            Self::Builtin { state } => state.records(),
            Self::Native { bindings, .. } => bindings.iter().map(String::as_str).collect(),
            Self::Composition { composition } => composition
                .parts
                .iter()
                .flat_map(|part| part.component.records())
                .collect(),
        }
    }

    pub fn origin(&self) -> Option<&crate::component::composition::Origin> {
        match self {
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
    pub const FORMAT: &str = "lince.canvas_component";
    pub fn decode(body: &str) -> Result<Self, String> {
        if body.len() > MAX_BYTES {
            return Err("The canvas component definition exceeds 256 KiB.".into());
        }
        let document: Self = serde_json::from_str(body).map_err(|e| e.to_string())?;
        if document.format != Self::FORMAT {
            return Err("Unknown canvas component definition format.".into());
        }
        workspace_name(&document.name)?;
        Ok(document)
    }
    pub fn encode(name: String, component: Component) -> Result<String, String> {
        workspace_name(&name)?;
        let document = Self {
            format: Self::FORMAT.into(),
            name,
            component,
        };
        bounded(&document)?;
        serde_json::to_string(&document).map_err(|e| e.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Geometry {
    pub position: [f64; 2],
    pub size: [f64; 2],
}

impl Geometry {
    pub fn validate(&self) -> Result<(), String> {
        if self
            .position
            .iter()
            .any(|n| !n.is_finite() || n.abs() > 1_000_000.0)
            || self
                .size
                .iter()
                .any(|n| !n.is_finite() || *n <= 0.0 || *n > 100_000.0)
        {
            return Err("Use finite, bounded canvas positions and positive sizes.".into());
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

impl View {
    fn validate(&self) -> bool {
        self.position.iter().all(|n| n.is_finite())
            && self.yaw.is_finite()
            && self.pitch.is_finite()
            && self.pitch.abs() < std::f64::consts::FRAC_PI_2
            && self.plane.is_finite()
            && self.selection_depth.is_finite()
            && self.selection_depth > 0.0
    }
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
    pub order: i32,
    pub pinned: Option<Pinned>,
    pub event_boundary: Vec<String>,
    pub spatial: Spatial,
    pub attachment: Option<Pose>,
    pub group_pose: Option<Pose>,
}

impl Metadata {
    fn validate(&self) -> bool {
        let rotation = |q: &[f64; 4]| {
            q.iter().all(|n| n.is_finite())
                && (q.iter().map(|n| n * n).sum::<f64>() - 1.0).abs() < 1e-6
        };
        let pose = |p: &Pose| p.position.iter().all(|n| n.is_finite()) && rotation(&p.rotation);
        self.pinned.as_ref().is_none_or(|p| {
            p.anchor.iter().all(|n| n.is_finite()) && (0.1..=3.0).contains(&p.scale)
        }) && self.event_boundary.len() <= 64
            && self
                .event_boundary
                .iter()
                .all(|s| !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control))
            && self.spatial.elevation.is_finite()
            && rotation(&self.spatial.rotation)
            && self
                .spatial
                .depth
                .is_none_or(|n| n.is_finite() && n > 0.0 && n <= 100_000.0)
            && self.attachment.as_ref().is_none_or(pose)
            && self.group_pose.as_ref().is_none_or(pose)
    }
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

impl Snapshot {
    pub fn validate(&self) -> Result<(), String> {
        self.validate_with_limits(MAX_PLACEMENTS, MAX_BYTES)
    }
    fn validate_full(&self) -> Result<(), String> {
        self.validate_with_limits(MAX_STATE_PLACEMENTS, 16 * 1024 * 1024)
    }
    fn validate_with_limits(&self, placements: usize, bytes: usize) -> Result<(), String> {
        if self.workspaces.is_empty()
            || self.workspaces.len() > 64
            || self.placements.len() > placements
        {
            return Err("Canvas snapshot exceeds workspace/placement bounds.".into());
        }
        let mut spaces = BTreeSet::new();
        for space in &self.workspaces {
            workspace_name(&space.name)?;
            if space.id == 0
                || !spaces.insert(space.id)
                || space.center.iter().any(|n| !n.is_finite())
                || !space.zoom.is_finite()
                || space.zoom <= 0.0
                || space.view.as_ref().is_some_and(|view| !view.validate())
            {
                return Err("Invalid workspace identity or view.".into());
            }
        }
        if !spaces.contains(&self.active_workspace) {
            return Err("The active workspace is missing.".into());
        }
        let mut ids = BTreeSet::new();
        for placement in &self.placements {
            placement_id(&placement.id)?;
            placement.geometry.validate()?;
            if !placement.metadata.validate() {
                return Err("Invalid canvas placement metadata.".into());
            }
            if !ids.insert(&placement.id)
                || !spaces.contains(&placement.workspace)
                || placement.group.as_ref().is_some_and(|g| g.len() > 80)
            {
                return Err("Invalid or duplicate canvas placement.".into());
            }
        }
        if serde_json::to_vec(self).map_err(|e| e.to_string())?.len() > bytes {
            return Err("Canvas snapshot exceeds its byte limit.".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Mutation {
    Add {
        placement: String,
        workspace: u64,
        component: Component,
        geometry: Geometry,
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Request {
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

impl Request {
    pub fn is_mutation(&self) -> bool {
        matches!(self, Self::Mutate { .. })
    }

    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Inspect { limit, offset, .. }
                if *limit == 0 || *limit > MAX_PLACEMENTS || *offset > 1_000_000 =>
            {
                return Err("Inspect 1–256 placements at a bounded offset.".into());
            }
            Self::Mutate {
                request_id,
                mutation,
                ..
            } => {
                request_identifier(request_id)?;
                match mutation {
                    Mutation::Add {
                        placement,
                        geometry,
                        workspace,
                        ..
                    } => {
                        placement_id(placement)?;
                        geometry.validate()?;
                        workspace_id(*workspace)?;
                    }
                    Mutation::Configure { placement, .. } | Mutation::Remove { placement } => {
                        placement_id(placement)?
                    }
                    Mutation::Move {
                        placement,
                        workspace,
                        position,
                    } => {
                        placement_id(placement)?;
                        workspace_id(*workspace)?;
                        Geometry {
                            position: *position,
                            size: [1.0, 1.0],
                        }
                        .validate()?;
                    }
                    Mutation::Resize { placement, size } => {
                        placement_id(placement)?;
                        Geometry {
                            position: [0.0, 0.0],
                            size: *size,
                        }
                        .validate()?;
                    }
                    Mutation::CreateWorkspace { name } => workspace_name(name)?,
                    Mutation::RenameWorkspace { workspace, name } => {
                        workspace_id(*workspace)?;
                        workspace_name(name)?;
                    }
                    Mutation::SwitchWorkspace { workspace }
                    | Mutation::RemoveWorkspace { workspace } => workspace_id(*workspace)?,
                    Mutation::Undo { receipt } => request_identifier(receipt)?,
                }
            }
            Self::Receipt { request_id } => request_identifier(request_id)?,
            _ => {}
        }
        bounded(self)
    }
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
pub enum Response {
    Snapshot { snapshot: Snapshot },
    Registry { components: Vec<Descriptor> },
    Receipt { receipt: Receipt },
}

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

pub fn bounded(value: &(impl Serialize + ?Sized)) -> Result<(), String> {
    if serde_json::to_vec(value).map_err(|e| e.to_string())?.len() > MAX_BYTES {
        Err("Canvas data exceeds 256 KiB.".into())
    } else {
        Ok(())
    }
}

pub fn request_identifier(id: &str) -> Result<(), String> {
    if crate::valid_uid(id, "request") {
        Ok(())
    } else {
        Err("Use a stable canvas request UID.".into())
    }
}

pub fn placement_id(id: &str) -> Result<(), String> {
    if crate::valid_uid(id, "placement") {
        Ok(())
    } else {
        Err("Use a stable canvas placement UID.".into())
    }
}

fn workspace_id(id: u64) -> Result<(), String> {
    if id == 0 {
        Err("Choose a nonzero workspace ID.".into())
    } else {
        Ok(())
    }
}
fn workspace_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        Err("Use a workspace name of 1–80 characters.".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixed_compositions_share_one_part_and_depth_budget() {
        let builtin = |parts: Vec<crate::component::Part>| ComponentState::Composition {
            composition: crate::component::Composition {
                name: "Builtin".into(),
                parts,
                origin: None,
            },
        };
        let part = |id: usize, component| crate::component::Part {
            id: format!("part-{id}"),
            settings: BTreeMap::new(),
            position: [0, 0],
            size: [100, 100],
            component,
            events: vec![],
        };
        let wrap = |component| Component::Composition {
            composition: Composition {
                name: "Wrapper".into(),
                origin: None,
                parts: vec![Part {
                    id: "content".into(),
                    geometry: Geometry {
                        position: [0.0, 0.0],
                        size: [100.0, 100.0],
                    },
                    component,
                    events: vec![],
                }],
            },
        };
        let text = ComponentState::Text {
            text: "hello".into(),
        };
        let full = |count| Component::Builtin {
            state: builtin((0..count).map(|id| part(id, text.clone())).collect()),
        };
        assert!(wrap(full(254)).validate(&[], false).is_ok());
        assert!(wrap(full(255)).validate(&[], false).is_err());
        let mut nested = text;
        for _ in 0..6 {
            nested = builtin(vec![part(0, nested)]);
        }
        let nested = Component::Builtin { state: nested };
        assert!(wrap(nested.clone()).validate(&[], false).is_ok());
        assert!(wrap(wrap(nested)).validate(&[], false).is_err());
    }

    #[test]
    fn canvas_contract_rejects_unsafe_geometry_and_requests() {
        assert!(
            Geometry {
                position: [f64::NAN, 0.0],
                size: [1.0, 1.0]
            }
            .validate()
            .is_err()
        );
        assert!(
            Geometry {
                position: [0.0, 0.0],
                size: [0.0, 1.0]
            }
            .validate()
            .is_err()
        );
        assert!(
            Request::Inspect {
                workspace: None,
                selected_only: false,
                offset: 0,
                limit: 257
            }
            .validate()
            .is_err()
        );
        assert!(
            Request::Mutate {
                request_id: "invented".into(),
                expected_revision: 0,
                mutation: Mutation::Remove {
                    placement: crate::new_uid("placement")
                }
            }
            .validate()
            .is_err()
        );
    }
    #[test]
    fn schema_rejects_unknown_settings_and_missing_kinds() {
        let descriptor = Descriptor {
            kind: "clock".into(),
            name: "Clock".into(),
            description: String::new(),
            settings: BTreeMap::from([("running".into(), Setting::Boolean { default: false })]),
            max_bindings: 0,
            placeable: true,
            configurable: true,
            composable: false,
            unavailable_reason: None,
        };
        let mut component = Component::Native {
            kind: "clock".into(),
            settings: BTreeMap::from([("running".into(), serde_json::json!(true))]),
            bindings: vec![],
        };
        assert!(
            component
                .validate(std::slice::from_ref(&descriptor), false)
                .is_ok()
        );
        if let Component::Native { settings, .. } = &mut component {
            settings.insert("shell".into(), serde_json::json!("anything"));
        }
        assert!(component.validate(&[descriptor], false).is_err());
    }
}
