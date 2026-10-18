use crate::native::{
    actions::Action,
    canvas_host, sand_panel,
    workspace::{WorkspaceMember, Workspaces},
};
use bevy::prelude::*;
use nucleus::{canvas as canvas_api, physical_workspace as model};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, time::Instant};

pub mod adapter;
mod inference;
mod media;
mod output;
pub use media::{
    Frame, PreparedFrame, capture_once, prepare_frame, submit_frame, submit_prepared_frame,
};
pub use output::open as open_output;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub surface: String,
    pub placement: String,
    pub rectangle: [f64; 4],
    pub canvas_origin: [f64; 2],
    pub canvas_span: [f64; 2],
    pub room: Option<String>,
}

impl Binding {
    fn valid(&self) -> bool {
        model::valid_id(&self.surface)
            && model::valid_id(&self.placement)
            && self
                .rectangle
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            && self.rectangle[2] >= 0.02
            && self.rectangle[3] >= 0.02
            && self.rectangle[0] + self.rectangle[2] <= 1.0
            && self.rectangle[1] + self.rectangle[3] <= 1.0
            && self.canvas_origin.iter().all(|v| v.is_finite())
            && self
                .canvas_span
                .iter()
                .all(|v| v.is_finite() && *v > 0.0 && *v <= 1_000_000.0)
            && self.room.as_ref().is_none_or(|id| model::valid_id(id))
    }

    fn rectangle_at(&self, position: [f64; 2]) -> [f64; 4] {
        [
            self.rectangle[0] + (position[0] - self.canvas_origin[0]) / self.canvas_span[0],
            self.rectangle[1] + (position[1] - self.canvas_origin[1]) / self.canvas_span[1],
            self.rectangle[2],
            self.rectangle[3],
        ]
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Saved {
    pub workspace: u64,
    pub configuration: model::Configuration,
    pub bindings: Vec<Binding>,
}

impl Saved {
    pub fn valid(&self) -> bool {
        self.workspace > 0
            && self.configuration.validate().is_ok()
            && self
                .configuration
                .surfaces
                .iter()
                .all(|s| s.workspace == self.workspace)
            && self.bindings.len() <= 64
            && self.bindings.iter().all(|b| {
                b.valid()
                    && self
                        .configuration
                        .surfaces
                        .iter()
                        .any(|s| s.id == b.surface)
            })
            && self
                .bindings
                .iter()
                .map(|b| (&b.surface, &b.placement))
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == self.bindings.len()
    }
}

struct Drag {
    placement: String,
    position: canvas_api::Pose,
    frame: Option<canvas_api::Pose>,
    room: Option<String>,
    start: [f64; 2],
    revision: u64,
    canvas_span: [f64; 2],
    task: Option<output::TaskSelection>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum InteractionOutcome {
    Placement { receipt: String },
    TaskTransitionRequested { record: String, column: usize },
}

impl InteractionOutcome {
    pub fn receipt(self) -> Option<String> {
        match self {
            Self::Placement { receipt } => Some(receipt),
            Self::TaskTransitionRequested { .. } => None,
        }
    }
}

pub struct Session {
    pub runtime: model::Runtime,
    pub bindings: Vec<Binding>,
    pub status: String,
    clock: Instant,
    drags: BTreeMap<String, Drag>,
    history: Vec<String>,
    routes: BTreeMap<String, inference::Route>,
    media: BTreeMap<String, media::Captured>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            runtime: model::Runtime::default(),
            bindings: Vec::new(),
            status: "Disconnected. No capture or output is enabled.".into(),
            clock: Instant::now(),
            drags: BTreeMap::new(),
            history: Vec::new(),
            routes: BTreeMap::new(),
            media: BTreeMap::new(),
        }
    }
}

impl Session {
    pub fn now_ms(&self) -> u64 {
        self.clock.elapsed().as_millis().min(u64::MAX as u128) as u64
    }
}

#[derive(Component, Default)]
pub struct Installation(pub BTreeMap<u64, Session>);

pub struct PhysicalWorkspacePlugin;

#[derive(Resource, Default)]
struct WakeDeadline(Option<Instant>);

impl Plugin for PhysicalWorkspacePlugin {
    fn build(&self, app: &mut App) {
        output::install(app);
        app.init_resource::<inference::Jobs>();
        app.init_resource::<media::Jobs>();
        app.init_resource::<adapter::Endpoints>();
        app.init_resource::<WakeDeadline>();
        app.add_systems(
            Update,
            tick.after(crate::native::workspace::PrepareWorkspaces)
                .after(crate::native::topology::rooms::synchronize)
                .after(crate::native::shader_effects::UpdateShaderEffects),
        );
    }
}

fn active_scope(world: &World, root: Entity, workspace: u64) -> bool {
    world
        .get::<Workspaces>(root)
        .is_some_and(|s| s.active == workspace)
        && !crate::native::laboratory::suspended(world, root)
        && world
            .get::<crate::native::workspace_sync::HostedReplica>(root)
            .is_none()
}

fn permitted(world: &World, root: Entity, workspace: u64) -> Result<(), String> {
    if world
        .get::<Workspaces>(root)
        .is_none_or(|s| s.active != workspace)
        || !world
            .get::<crate::native::edit_mode::EditMode>(root)
            .is_some_and(|mode| mode.enabled)
        || !active_scope(world, root, workspace)
        || world
            .get::<crate::native::workspace_sync::HostedReplica>(root)
            .is_some()
    {
        return Err("Open the local workspace in Edit mode to change its physical setup.".into());
    }
    Ok(())
}

fn session_mut(
    world: &mut World,
    root: Entity,
    workspace: u64,
) -> Result<Mut<'_, Installation>, String> {
    permitted(world, root, workspace)?;
    if world.get::<Installation>(root).is_none() {
        world.entity_mut(root).insert(Installation::default());
    }
    let mut installation = world.get_mut::<Installation>(root).unwrap();
    if installation.0.len() >= 64 && !installation.0.contains_key(&workspace) {
        return Err("At most sixty-four physical workspace setups can be retained.".into());
    }
    installation.0.entry(workspace).or_default();
    Ok(installation)
}

pub fn configure(world: &mut World, root: Entity, saved: Saved) -> Result<(), String> {
    if !saved.valid() {
        return Err("Invalid physical workspace configuration.".into());
    }
    let mut installation = session_mut(world, root, saved.workspace)?;
    let session = installation.0.get_mut(&saved.workspace).unwrap();
    session.runtime.configure(saved.configuration)?;
    session.bindings = saved.bindings;
    session.drags.clear();
    session.media.clear();
    session.status =
        "Configuration saved. Connect devices deliberately; output starts blanked.".into();
    Ok(())
}

pub fn snapshot(world: &World, root: Entity) -> Vec<Saved> {
    let ids = workspace_ids(world, root);
    world
        .get::<Installation>(root)
        .map(|installation| {
            installation
                .0
                .iter()
                .filter(|(workspace, _)| ids.contains(workspace))
                .map(|(workspace, session)| Saved {
                    workspace: *workspace,
                    configuration: session.runtime.configuration.clone(),
                    bindings: session.bindings.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn workspace_ids(world: &World, root: Entity) -> std::collections::BTreeSet<u64> {
    world
        .get::<Workspaces>(root)
        .map(|spaces| {
            spaces
                .entries
                .iter()
                .map(|w| w.id)
                .chain(spaces.trash.iter().map(|w| w.workspace.id))
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn restore(world: &mut World, root: Entity, saved: Vec<Saved>) {
    let mut installation = Installation::default();
    for saved in saved.into_iter().filter(Saved::valid).take(64) {
        let session = Session {
            runtime: model::Runtime::new(saved.configuration).unwrap(),
            bindings: saved.bindings,
            ..Session::default()
        };
        installation.0.insert(saved.workspace, session);
    }
    world.entity_mut(root).insert(installation);
}

pub fn connect(
    world: &mut World,
    root: Entity,
    workspace: u64,
    device: &str,
    session: &str,
) -> Result<(), String> {
    session_mut(world, root, workspace)?
        .0
        .get_mut(&workspace)
        .unwrap()
        .runtime
        .connect(device, session)
}

pub fn capture(
    world: &mut World,
    root: Entity,
    workspace: u64,
    camera: &str,
    enabled: bool,
) -> Result<(), String> {
    let mut installation = session_mut(world, root, workspace)?;
    let session = installation.0.get_mut(&workspace).unwrap();
    session.runtime.capture(camera, enabled)?;
    if !enabled {
        session.media.clear();
        session.drags.clear();
    }
    Ok(())
}

pub fn disconnect(
    world: &mut World,
    root: Entity,
    workspace: u64,
    device: &str,
) -> Result<(), String> {
    let mut installation = session_mut(world, root, workspace)?;
    let session = installation.0.get_mut(&workspace).unwrap();
    session.runtime.disconnect(device);
    session
        .media
        .retain(|id, _| session.runtime.observations.contains_key(id));
    session
        .drags
        .retain(|id, _| session.runtime.pointers.contains_key(id));
    Ok(())
}

pub fn observe(
    world: &mut World,
    root: Entity,
    workspace: u64,
    evidence: model::Evidence,
) -> Result<(), String> {
    let mut installation = session_mut(world, root, workspace)?;
    let session = installation.0.get_mut(&workspace).unwrap();
    session
        .runtime
        .observe(workspace, evidence, session.now_ms())
}

pub fn blank(world: &mut World, root: Entity) {
    if let Some(mut installation) = world.get_mut::<Installation>(root) {
        for session in installation.0.values_mut() {
            session.runtime.blank(true);
            session.drags.clear();
        }
    }
    output::black(world, root);
}

pub fn close_outputs(world: &mut World, root: Entity) {
    blank(world, root);
    output::close(world, root);
}

fn resolve(world: &mut World, root: Entity, workspace: u64, id: &str) -> Option<Entity> {
    world
        .query::<(Entity, &canvas_host::Identity, &ChildOf, &WorkspaceMember)>()
        .iter(world)
        .find(|(_, identity, parent, member)| {
            identity.0 == id && parent.parent() == root && member.0 == workspace
        })
        .map(|(entity, _, _, _)| entity)
}

struct Coordinates {
    local: canvas_api::Pose,
    frame: Option<canvas_api::Pose>,
    room: Option<String>,
}

fn coordinates(world: &mut World, root: Entity, workspace: u64, id: &str) -> Option<Coordinates> {
    let owner = resolve(world, root, workspace, id)?;
    let pose = crate::native::topology::rooms::world_pose(world, owner)?;
    let owner_frame = crate::native::topology::rooms::frame(world, owner);
    let room_id = owner_frame.and_then(|frame| {
        world
            .get::<crate::native::topology::rooms::Member>(frame)
            .map(|member| member.0.room.clone())
            .or_else(|| {
                world
                    .get::<crate::native::topology::rooms::Room>(frame)
                    .and_then(|_| {
                        world
                            .get::<canvas_host::Identity>(frame)
                            .map(|id| id.0.clone())
                    })
            })
    });
    if let Some(room_id) = room_id {
        let room_entity = resolve(world, root, workspace, &room_id)?;
        let frame = crate::native::topology::rooms::world_pose(world, room_entity)?;
        Some(Coordinates {
            local: canvas_api::room::relative(&frame, &pose),
            frame: Some(frame),
            room: Some(room_id),
        })
    } else {
        Some(Coordinates {
            local: pose,
            frame: None,
            room: None,
        })
    }
}

pub fn bind_selection(
    world: &mut World,
    root: Entity,
    workspace: u64,
    surface: &str,
) -> Result<(), String> {
    permitted(world, root, workspace)?;
    let snapshot = canvas_host::capture(world, root)?;
    let selected: Vec<_> = snapshot
        .placements
        .iter()
        .filter(|p| p.selected && p.workspace == workspace)
        .collect();
    if selected.is_empty() || selected.len() > 16 {
        return Err(
            "Select one to sixteen placed Sands or Records before assigning a surface.".into(),
        );
    }
    let points: BTreeMap<_, _> = selected
        .iter()
        .filter_map(|p| {
            coordinates(world, root, workspace, &p.id).map(|point| (p.id.clone(), point))
        })
        .collect();
    if points.len() != selected.len() {
        return Err("Wait for all selected placement coordinates.".into());
    }
    let mut installation = session_mut(world, root, workspace)?;
    let session = installation.0.get_mut(&workspace).unwrap();
    session.runtime.surface(surface, workspace)?;
    let mut bindings: Vec<_> = session
        .bindings
        .iter()
        .filter(|b| b.surface != surface)
        .cloned()
        .collect();
    let count = selected.len();
    for (index, placement) in selected.into_iter().enumerate() {
        bindings.push(Binding {
            surface: surface.into(),
            placement: placement.id.clone(),
            rectangle: [
                0.05,
                0.05 + index as f64 * 0.9 / count as f64,
                0.9,
                0.85 / count as f64,
            ],
            canvas_origin: [
                points[&placement.id].local.position[0],
                points[&placement.id].local.position[2],
            ],
            room: points[&placement.id].room.clone(),
            canvas_span: [
                placement.geometry.size[0] / 0.9,
                placement.geometry.size[1] / (0.85 / count as f64),
            ],
        });
    }
    if bindings.len() > 64 {
        return Err("At most sixty-four placements can be assigned.".into());
    }
    session.bindings = bindings;
    session.status = "Only the selected placements are assigned to this surface.".into();
    Ok(())
}

pub fn interact(
    world: &mut World,
    root: Entity,
    workspace: u64,
    interaction: model::Interaction,
) -> Result<Option<InteractionOutcome>, String> {
    permitted(world, root, workspace)?;
    if output::calibrating(world, root, workspace, &interaction.evidence.surface) {
        return Err("Close the calibration pattern before interacting with content.".into());
    }
    if matches!(
        interaction.phase,
        model::Phase::Update | model::Phase::Cancel
    ) {
        let mut installation = session_mut(world, root, workspace)?;
        let session = installation.0.get_mut(&workspace).unwrap();
        let pointer = interaction.pointer.clone();
        let cancel = interaction.phase == model::Phase::Cancel;
        let now = session.now_ms();
        let result = session.runtime.interact(workspace, interaction, now);
        if cancel || result.is_err() {
            session.drags.remove(&pointer);
        }
        return result.map(|_| None);
    }
    let snapshot = canvas_host::capture(world, root)?;
    let task = if interaction.phase == model::Phase::Begin {
        let local = world
            .get::<Installation>(root)
            .and_then(|i| i.0.get(&workspace))
            .and_then(|s| {
                s.runtime
                    .surface(&interaction.evidence.surface, workspace)
                    .ok()
            })
            .and_then(|s| s.camera_quad.map(interaction.camera_normalized, true).ok());
        local.and_then(|local| {
            output::pick_task(world, root, workspace, &interaction.evidence.surface, local)
        })
    } else {
        None
    };
    let bound: Vec<_> = world
        .get::<Installation>(root)
        .and_then(|i| i.0.get(&workspace))
        .map(|s| s.bindings.iter().map(|b| b.placement.clone()).collect())
        .unwrap_or_default();
    let points: BTreeMap<_, _> = bound
        .into_iter()
        .filter_map(|id| coordinates(world, root, workspace, &id).map(|point| (id, point)))
        .collect();
    let pointer = interaction.pointer.clone();
    let phase = interaction.phase;
    let mut installation = session_mut(world, root, workspace)?;
    let session = installation.0.get_mut(&workspace).unwrap();
    let now = session.now_ms();
    let commit = match session.runtime.interact(workspace, interaction, now) {
        Ok(commit) => commit,
        Err(error) => {
            session.drags.remove(&pointer);
            return Err(error);
        }
    };
    if phase == model::Phase::Begin {
        if let Some(task) = task {
            session.drags.insert(
                pointer,
                Drag {
                    placement: task.board.clone(),
                    position: canvas_api::room::identity_pose(),
                    frame: None,
                    room: None,
                    start: [0.0; 2],
                    revision: snapshot.revision,
                    canvas_span: [1.0; 2],
                    task: Some(task),
                },
            );
            return Ok(None);
        }
        let point = &session.runtime.pointers[&pointer];
        let binding = session
            .bindings
            .iter()
            .rev()
            .find(|binding| {
                let Some(placement) = snapshot
                    .placements
                    .iter()
                    .find(|p| p.id == binding.placement && p.workspace == workspace)
                else {
                    return false;
                };
                let Some(source) = points
                    .get(&placement.id)
                    .filter(|point| point.room == binding.room)
                else {
                    return false;
                };
                let [x, y, w, h] =
                    binding.rectangle_at([source.local.position[0], source.local.position[2]]);
                binding.surface == point.surface
                    && point.local[0] >= x
                    && point.local[0] <= x + w
                    && point.local[1] >= y
                    && point.local[1] <= y + h
            })
            .ok_or("Select an assigned placement on the surface.")?;
        let placement = snapshot
            .placements
            .iter()
            .find(|p| p.id == binding.placement && p.workspace == workspace)
            .ok_or("The placement is unavailable.")?;
        if placement.metadata.room.is_some() {
            session.runtime.pointers.remove(&pointer);
            return Err("Move the virtual room from the canvas; physical projection keeps its own alignment.".into());
        }
        let source = &points[&placement.id];
        session.drags.insert(
            pointer,
            Drag {
                placement: placement.id.clone(),
                position: source.local.clone(),
                frame: source.frame.clone(),
                room: source.room.clone(),
                start: point.local,
                revision: snapshot.revision,
                canvas_span: binding.canvas_span,
                task: None,
            },
        );
        return Ok(None);
    }
    if phase == model::Phase::Cancel {
        session.drags.remove(&pointer);
        return Ok(None);
    }
    let Some(commit) = commit else {
        return Ok(None);
    };
    let drag = session
        .drags
        .remove(&pointer)
        .ok_or("Selection was canceled.")?;
    if snapshot.revision != drag.revision {
        return Err("The canvas changed during selection; select again.".into());
    }
    if let Some(task) = drag.task {
        drop(installation);
        let column = output::pick_column(
            world,
            root,
            workspace,
            &commit.surface,
            &task.board,
            commit.local,
        )
        .ok_or("Release over a column of the same board.")?;
        let board =
            resolve(world, root, workspace, &task.board).ok_or("The board is unavailable.")?;
        output::validate_task(world, root, workspace, &task)?;
        crate::native::kanban::move_card(world, board, &task.record, column)?;
        return Ok(Some(InteractionOutcome::TaskTransitionRequested {
            record: task.record,
            column,
        }));
    }
    let delta = [
        commit.local[0] - drag.start[0],
        commit.local[1] - drag.start[1],
    ];
    let mut point = drag.position.clone();
    point.position[0] += delta[0] * drag.canvas_span[0];
    point.position[2] += delta[1] * drag.canvas_span[1];
    let transformed = drag
        .frame
        .as_ref()
        .map(|frame| canvas_api::room::compose(frame, &point))
        .unwrap_or_else(|| point.clone());
    let mut metadata = snapshot
        .placements
        .iter()
        .find(|p| p.id == drag.placement)
        .ok_or("The placement disappeared.")?
        .metadata
        .clone();
    drop(installation);
    let mutation = if let Some(member) = &mut metadata.room_member {
        let parent = resolve(
            world,
            root,
            workspace,
            member.parent.as_deref().unwrap_or(&member.room),
        )
        .and_then(|parent| crate::native::topology::rooms::world_pose(world, parent))
        .ok_or("The parent frame is unavailable.")?;
        member.local = canvas_api::room::relative(&parent, &transformed);
        canvas_api::Mutation::SetMetadata {
            placement: drag.placement.clone(),
            metadata,
        }
    } else if let Some(room) = drag.room {
        canvas_api::Mutation::SetRoomMembers {
            members: BTreeMap::from([(
                drag.placement.clone(),
                Some(canvas_api::room::Member {
                    room,
                    parent: None,
                    local: point,
                    role: canvas_api::room::Role::Content,
                }),
            )]),
        }
    } else {
        canvas_api::Mutation::Move {
            placement: drag.placement.clone(),
            workspace,
            position: [transformed.position[0], transformed.position[2]],
        }
    };
    let request = canvas_api::Request::Mutate {
        request_id: nucleus::new_uid("request"),
        expected_revision: drag.revision,
        mutation,
    };
    let response = canvas_host::handle(world, root, &request)?;
    let canvas_api::Response::Receipt { receipt } = response else {
        return Err("Movement did not produce a receipt.".into());
    };
    let id = receipt.request_id;
    let mut installation = world.get_mut::<Installation>(root).unwrap();
    let session = installation.0.get_mut(&workspace).unwrap();
    if session.history.len() == 32 {
        session.history.remove(0);
    }
    session.history.push(id.clone());
    Ok(Some(InteractionOutcome::Placement { receipt: id }))
}

#[derive(Component)]
struct Panel {
    root: Entity,
    workspace: u64,
    surface: Entity,
    configuration: Entity,
    status: Entity,
    thread: Entity,
    prompt: Entity,
    camera_index: Entity,
    projector_corners: Entity,
    camera_corners: Entity,
    dimensions: Entity,
}

pub fn undo(world: &mut World, root: Entity, workspace: u64) -> Result<(), String> {
    let mut installation = session_mut(world, root, workspace)?;
    let id = installation
        .0
        .get_mut(&workspace)
        .unwrap()
        .history
        .last()
        .cloned()
        .ok_or("No physical movement to undo.")?;
    drop(installation);
    let revision = canvas_host::capture(world, root)?.revision;
    canvas_host::handle(
        world,
        root,
        &canvas_api::Request::Mutate {
            request_id: nucleus::new_uid("request"),
            expected_revision: revision,
            mutation: canvas_api::Mutation::Undo { receipt: id },
        },
    )?;
    world
        .get_mut::<Installation>(root)
        .unwrap()
        .0
        .get_mut(&workspace)
        .unwrap()
        .history
        .pop();
    Ok(())
}

#[derive(Component)]
struct Entry(Entity);

#[derive(Component)]
struct Tasks {
    root: Entity,
    workspace: u64,
    signature: String,
}

#[derive(Clone)]
struct RequestControl {
    root: Entity,
    workspace: u64,
    id: String,
    dismiss: bool,
}

impl Action for RequestControl {
    fn apply(&self, world: &mut World, _: Entity) {
        let Ok(mut installation) = session_mut(world, self.root, self.workspace) else {
            return;
        };
        let session = installation.0.get_mut(&self.workspace).unwrap();
        let result = if self.dismiss {
            session.runtime.dismiss(&self.id)
        } else {
            session.runtime.cancel(&self.id)
        };
        if result.is_ok() && self.dismiss {
            session.routes.remove(&self.id);
        }
        if let Err(error) = result {
            session.status = error;
        }
    }
}

#[derive(Clone, Copy)]
enum Control {
    Open,
    Apply,
    Replay,
    Assign,
    Output,
    Blank,
    Close,
    Stop,
    Undo,
    Cancel,
    Request,
    RequestFrame,
    Capture,
    Refresh,
    Calibration,
    ResetCalibration,
    Calibrate,
    Adapter,
    CopyAdapter,
    StopAdapter,
}

pub fn populate_controls(world: &mut World, root: Entity, parent: Entity) {
    world.entity_mut(parent).insert(Entry(root));
    sand_panel::button(world, parent, parent, "Physical workspace", Control::Open);
}

fn panel(world: &mut World, root: Entity, parent: Entity, workspace: u64) {
    let body = sand_panel::column(world, parent);
    crate::native::edit_mode::label(
        world,
        body,
        "Physical workspace — cameras and surface projection",
        18.0,
    );
    crate::native::edit_mode::label(
        world,
        body,
        "Only explicitly assigned placements are displayed. Replay is synthetic input; no camera opens automatically.",
        13.0,
    );
    let surface = sand_panel::field(world, body, "Surface ID", "desk");
    let saved = snapshot(world, root)
        .into_iter()
        .find(|s| s.workspace == workspace)
        .unwrap_or(Saved {
            workspace,
            configuration: Default::default(),
            bindings: Vec::new(),
        });
    let configuration = sand_panel::field(
        world,
        body,
        "Installation configuration (JSON)",
        &serde_json::to_string(&saved).unwrap(),
    );
    if let Some(mut input) = world.get_mut::<bevy::text::EditableText>(configuration) {
        input.max_characters = Some(65_536);
    }
    let thread = sand_panel::field(world, body, "Existing Fiote thread Record ID", "");
    let prompt = sand_panel::field(world, body, "Request for the selected Record", "");
    let camera_index = sand_panel::field(world, body, "Local camera index", "0");
    let projector_corners = sand_panel::field(
        world,
        body,
        "Projector corners: top-left, top-right, bottom-right, bottom-left (normalized x,y)",
        "0,0 1,0 1,1 0,1",
    );
    let camera_corners = sand_panel::field(
        world,
        body,
        "Camera corners in the same order (normalized x,y)",
        "0,0 1,0 1,1 0,1",
    );
    let dimensions = sand_panel::field(world, body, "Surface width,height in meters", "1.2,0.8");
    if let Some(selected) = saved.configuration.surfaces.first() {
        if let Some(mut input) = world.get_mut::<bevy::text::EditableText>(surface) {
            input.editor.set_text(&selected.id);
        }
        fill_calibration_fields(
            world,
            projector_corners,
            camera_corners,
            dimensions,
            selected,
        );
    }
    crate::native::edit_mode::label(
        world,
        body,
        "Capture blanks output and opens the selected camera for one frame. Sending a frame to Fiote saves that image in the conversation and may send it to your configured model provider. Frames expire after five seconds unless sent.",
        13.0,
    );
    for (caption, action) in [
        ("Apply configuration", Control::Apply),
        ("Reload setup fields", Control::Refresh),
        ("Load replay setup", Control::Replay),
        ("Assign selected placements", Control::Assign),
        ("Open surface output", Control::Output),
        ("Show calibration pattern", Control::Calibration),
        ("Reset surface calibration", Control::ResetCalibration),
        ("Apply surface calibration", Control::Calibrate),
        ("Blank output", Control::Blank),
        ("Close output windows", Control::Close),
        ("Stop all capture", Control::Stop),
        ("Undo physical movement", Control::Undo),
        ("Cancel pending requests", Control::Cancel),
        ("Request Fiote answer", Control::Request),
        ("Request Fiote with latest frame", Control::RequestFrame),
        ("Blank output and capture one frame", Control::Capture),
        ("Enable local hardware adapter", Control::Adapter),
        ("Enable and copy adapter pairing", Control::CopyAdapter),
        ("Disable local hardware adapters", Control::StopAdapter),
    ] {
        sand_panel::button(world, body, body, caption, action);
    }
    let status = crate::native::edit_mode::label(world, body, "Disconnected", 13.0);
    world.entity_mut(body).insert(Panel {
        root,
        workspace,
        surface,
        configuration,
        status,
        thread,
        prompt,
        camera_index,
        projector_corners,
        camera_corners,
        dimensions,
    });
    let tasks = sand_panel::column(world, body);
    world.entity_mut(tasks).insert(Tasks {
        root,
        workspace,
        signature: String::new(),
    });
    sand_panel::credits(world, body, body, CREDITS);
}

const CREDITS: &[crate::native::credits::Attribution] = &[
    crate::native::credits::Attribution {
        name: "nokhwa",
        author: "l1npengtul and nokhwa contributors",
        license: include_str!("../../licenses/native/nokhwa-Apache-2.0.txt"),
    },
    crate::native::credits::Attribution {
        name: "image",
        author: "The image-rs developers",
        license: include_str!("../../licenses/native/image-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "base64",
        author: "The base64 contributors",
        license: include_str!("../../licenses/native/base64-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "Bevy",
        author: "Bevy contributors",
        license: crate::native::credits::BEVY_LICENSE,
    },
    crate::native::credits::Attribution {
        name: "glam",
        author: "Cameron Hart and contributors",
        license: include_str!("../../licenses/native/glam-MIT.txt"),
    },
];

fn parse_numbers<const N: usize>(value: &str) -> Result<[f64; N], String> {
    value
        .split([',', ' ', '\t'])
        .filter(|v| !v.is_empty())
        .map(|v| {
            v.parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or("Enter finite numbers separated by commas or spaces.".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?
        .try_into()
        .map_err(|_| format!("Enter exactly {N} numbers."))
}

fn fill_calibration_fields(
    world: &mut World,
    projector: Entity,
    camera: Entity,
    dimensions: Entity,
    surface: &model::Surface,
) {
    for (field, value) in [
        (
            projector,
            surface
                .projector_quad
                .0
                .iter()
                .map(|p| format!("{},{}", p[0], p[1]))
                .collect::<Vec<_>>()
                .join(" "),
        ),
        (
            camera,
            surface
                .camera_quad
                .0
                .iter()
                .map(|p| format!("{},{}", p[0], p[1]))
                .collect::<Vec<_>>()
                .join(" "),
        ),
        (
            dimensions,
            format!("{},{}", surface.size_meters[0], surface.size_meters[1]),
        ),
    ] {
        if let Some(mut input) = world.get_mut::<bevy::text::EditableText>(field) {
            input.editor.set_text(&value);
        }
    }
}

pub fn replay_configuration(workspace: u64) -> Saved {
    Saved {
        workspace,
        bindings: Vec::new(),
        configuration: model::Configuration {
            devices: vec![
                model::Device {
                    id: "replay-camera".into(),
                    name: "Synthetic observation input".into(),
                    kind: model::DeviceKind::Camera,
                    resolution: [1280, 720],
                },
                model::Device {
                    id: "preview-projector".into(),
                    name: "Surface output preview".into(),
                    kind: model::DeviceKind::Projector,
                    resolution: [1280, 720],
                },
            ],
            surfaces: vec![model::Surface {
                id: "desk".into(),
                name: "Desk surface preview".into(),
                workspace,
                projector: "preview-projector".into(),
                camera: Some("replay-camera".into()),
                physical_pose_meters: nucleus::canvas::room::identity_pose(),
                size_meters: [1.2, 0.8],
                calibration_revision: 1,
                projector_quad: model::Quad::default(),
                camera_quad: model::Quad::default(),
            }],
        },
    }
}

impl Action for Control {
    fn apply(&self, world: &mut World, owner: Entity) {
        if matches!(self, Self::Open) {
            let Some(root) = world.get::<Entry>(owner).map(|entry| entry.0) else {
                return;
            };
            let workspace = world.get::<Workspaces>(root).map(|s| s.active).unwrap_or(0);
            if permitted(world, root, workspace).is_ok() {
                let existing = world
                    .query::<(Entity, &Panel)>()
                    .iter(world)
                    .find(|(_, p)| p.root == root && p.workspace == workspace)
                    .map(|(entity, _)| entity);
                if existing.is_none() {
                    panel(world, root, owner, workspace);
                }
            }
            return;
        }
        let Some(context) = world.get::<Panel>(owner) else {
            return;
        };
        let (root, workspace, surface, configuration) = (
            context.root,
            context.workspace,
            context.surface,
            context.configuration,
        );
        let result = (|| -> Result<(), String> {
            if matches!(self, Self::Blank) {
                blank(world, root);
                return Ok(());
            }
            if matches!(self, Self::Close) {
                close_outputs(world, root);
                return Ok(());
            }
            permitted(world, root, workspace)?;
            let selected = sand_panel::value(world, surface)?;
            match self {
                Self::Apply => {
                    let input = sand_panel::value(world, configuration)?;
                    if input.len() > 65_536 {
                        return Err("Configuration exceeds 64 KiB.".into());
                    }
                    let saved: Saved = serde_json::from_str(&input).map_err(|e| e.to_string())?;
                    if saved.workspace != workspace {
                        return Err("Keep the setup in its current workspace.".into());
                    }
                    configure(world, root, saved)
                }
                Self::Replay => {
                    let mut saved = replay_configuration(workspace);
                    if let Some(old) = world
                        .get::<Installation>(root)
                        .and_then(|i| i.0.get(&workspace))
                    {
                        saved.configuration.surfaces[0].calibration_revision = old
                            .runtime
                            .configuration
                            .surfaces
                            .iter()
                            .map(|s| s.calibration_revision)
                            .max()
                            .unwrap_or(0)
                            .saturating_add(1);
                    }
                    configure(world, root, saved)?;
                    connect(
                        world,
                        root,
                        workspace,
                        "replay-camera",
                        &nucleus::new_uid("camera"),
                    )?;
                    connect(
                        world,
                        root,
                        workspace,
                        "preview-projector",
                        &nucleus::new_uid("projector"),
                    )?;
                    capture(world, root, workspace, "replay-camera", true)?;
                    let saved = snapshot(world, root)
                        .into_iter()
                        .find(|s| s.workspace == workspace)
                        .unwrap();
                    if let Some(mut input) =
                        world.get_mut::<bevy::text::EditableText>(configuration)
                    {
                        input
                            .editor
                            .set_text(&serde_json::to_string(&saved).unwrap());
                    }
                    Ok(())
                }
                Self::Refresh => {
                    let saved = snapshot(world, root)
                        .into_iter()
                        .find(|s| s.workspace == workspace)
                        .ok_or("No saved setup.")?;
                    if let Some(mut input) =
                        world.get_mut::<bevy::text::EditableText>(configuration)
                    {
                        input
                            .editor
                            .set_text(&serde_json::to_string(&saved).unwrap());
                    }
                    if let Some(surface) = saved
                        .configuration
                        .surfaces
                        .iter()
                        .find(|s| s.id == selected.trim())
                    {
                        let panel = world.get::<Panel>(owner).unwrap();
                        let fields = (
                            panel.projector_corners,
                            panel.camera_corners,
                            panel.dimensions,
                        );
                        fill_calibration_fields(world, fields.0, fields.1, fields.2, surface);
                    }
                    Ok(())
                }
                Self::Assign => bind_selection(world, root, workspace, selected.trim()),
                Self::Output | Self::Calibration => {
                    let projector = world
                        .get::<Installation>(root)
                        .and_then(|i| i.0.get(&workspace))
                        .ok_or("Configure a surface first.")?
                        .runtime
                        .surface(selected.trim(), workspace)?
                        .projector
                        .clone();
                    let connected = world.get::<Installation>(root).unwrap().0[&workspace]
                        .runtime
                        .connections
                        .contains_key(&projector);
                    if !connected {
                        connect(
                            world,
                            root,
                            workspace,
                            &projector,
                            &nucleus::new_uid("output"),
                        )?;
                    }
                    if matches!(self, Self::Calibration) {
                        output::calibration(world, root, workspace, selected.trim())
                    } else {
                        output::open(world, root, workspace, selected.trim())
                    }
                }
                Self::Calibrate => {
                    let panel = world.get::<Panel>(owner).unwrap();
                    let projector =
                        parse_numbers::<8>(&sand_panel::value(world, panel.projector_corners)?)?;
                    let camera =
                        parse_numbers::<8>(&sand_panel::value(world, panel.camera_corners)?)?;
                    let dimensions =
                        parse_numbers::<2>(&sand_panel::value(world, panel.dimensions)?)?;
                    let mut saved = snapshot(world, root)
                        .into_iter()
                        .find(|s| s.workspace == workspace)
                        .ok_or("Configure a surface first.")?;
                    let surface = saved
                        .configuration
                        .surfaces
                        .iter_mut()
                        .find(|s| s.id == selected.trim())
                        .ok_or("Choose an existing surface.")?;
                    surface.projector_quad = model::Quad(std::array::from_fn(|i| {
                        [projector[i * 2], projector[i * 2 + 1]]
                    }));
                    surface.camera_quad =
                        model::Quad(std::array::from_fn(|i| [camera[i * 2], camera[i * 2 + 1]]));
                    surface.size_meters = dimensions;
                    surface.calibration_revision = surface
                        .calibration_revision
                        .checked_add(1)
                        .ok_or("Calibration revision exhausted.")?;
                    configure(world, root, saved)
                }
                Self::ResetCalibration => {
                    let mut saved = snapshot(world, root)
                        .into_iter()
                        .find(|s| s.workspace == workspace)
                        .ok_or("Configure a surface first.")?;
                    let surface = saved
                        .configuration
                        .surfaces
                        .iter_mut()
                        .find(|s| s.id == selected.trim())
                        .ok_or("Choose an existing surface.")?;
                    surface.projector_quad = model::Quad::default();
                    surface.camera_quad = model::Quad::default();
                    surface.calibration_revision = surface
                        .calibration_revision
                        .checked_add(1)
                        .ok_or("Calibration revision exhausted.")?;
                    configure(world, root, saved)
                }
                Self::Stop => {
                    let mut installation = session_mut(world, root, workspace)?;
                    let session = installation.0.get_mut(&workspace).unwrap();
                    let cameras: Vec<_> = session
                        .runtime
                        .configuration
                        .devices
                        .iter()
                        .filter(|d| d.kind == model::DeviceKind::Camera)
                        .map(|d| d.id.clone())
                        .collect();
                    for camera in cameras {
                        let _ = session.runtime.capture(&camera, false);
                    }
                    session.drags.clear();
                    session.media.clear();
                    Ok(())
                }
                Self::Cancel => {
                    session_mut(world, root, workspace)?
                        .0
                        .get_mut(&workspace)
                        .unwrap()
                        .runtime
                        .cancel_all();
                    Ok(())
                }
                Self::Request | Self::RequestFrame => {
                    let (thread, prompt) = {
                        let panel = world.get::<Panel>(owner).unwrap();
                        (panel.thread, panel.prompt)
                    };
                    let thread = sand_panel::value(world, thread)?;
                    let prompt = sand_panel::value(world, prompt)?;
                    inference::enqueue(
                        world,
                        root,
                        workspace,
                        selected.trim(),
                        thread.trim(),
                        &prompt,
                        matches!(self, Self::RequestFrame),
                    )
                    .map(|_| ())
                }
                Self::Capture => {
                    let input = world.get::<Panel>(owner).unwrap().camera_index;
                    let index = sand_panel::value(world, input)?
                        .parse::<u32>()
                        .map_err(|_| "Enter a numeric camera index.")?;
                    capture_once(world, root, workspace, selected.trim(), index)
                }
                Self::Adapter | Self::CopyAdapter => {
                    let descriptor = adapter::start(world, root, workspace, selected.trim())?;
                    let value =
                        serde_json::to_string_pretty(&descriptor).map_err(|e| e.to_string())?;
                    if matches!(self, Self::CopyAdapter) {
                        world
                            .get_resource_mut::<bevy::clipboard::Clipboard>()
                            .ok_or("Clipboard is unavailable; use the displayed pairing details.")?
                            .set_text(&value)
                            .map_err(|error| error.to_string())?;
                    }
                    session_mut(world, root, workspace)?
                        .0
                        .get_mut(&workspace)
                        .unwrap()
                        .status = format!("Local adapter pairing details (keep private):\n{value}");
                    return Ok(());
                }
                Self::StopAdapter => {
                    adapter::stop(world, root, workspace);
                    Ok(())
                }
                Self::Undo => undo(world, root, workspace),
                Self::Blank | Self::Close | Self::Open => Ok(()),
            }
        })();
        if let Some(mut installation) = world.get_mut::<Installation>(root) {
            if let Some(session) = installation.0.get_mut(&workspace) {
                session
                    .routes
                    .retain(|id, _| session.runtime.requests.contains_key(id));
                for (id, route) in &mut session.routes {
                    if session
                        .runtime
                        .requests
                        .get(id)
                        .is_some_and(|r| r.terminal())
                    {
                        route.release_media();
                    }
                }
                if !matches!(self, Self::Adapter | Self::CopyAdapter) || result.is_err() {
                    session.status = result.err().unwrap_or_else(|| "Action completed.".into());
                }
            }
        }
    }
}

fn tick(world: &mut World) {
    let roots: Vec<_> = world
        .query_filtered::<Entity, With<Installation>>()
        .iter(world)
        .collect();
    for root in roots {
        let active = world.get::<Workspaces>(root).map(|s| s.active);
        let ids = workspace_ids(world, root);
        let authorized = active.is_some_and(|workspace| active_scope(world, root, workspace));
        let mut installation = world.get_mut::<Installation>(root).unwrap();
        installation
            .0
            .retain(|workspace, _| ids.contains(workspace));
        for (workspace, session) in &mut installation.0 {
            let _ = session.runtime.advance(session.now_ms());
            session
                .drags
                .retain(|pointer, _| session.runtime.pointers.contains_key(pointer));
            if active != Some(*workspace) || !authorized {
                session.runtime.blank(true);
                session.runtime.cancel_all();
                let devices: Vec<_> = session.runtime.connections.keys().cloned().collect();
                for device in devices {
                    session.runtime.disconnect(&device);
                }
                session.drags.clear();
            }
        }
    }
    media::tick(world);
    adapter::tick(world);
    inference::tick(world);
    output::tick(world);
    let active = world
        .query::<&Installation>()
        .iter(world)
        .any(|i| i.0.values().any(|s| !s.runtime.pointers.is_empty()));
    let due = world
        .get_resource::<WakeDeadline>()
        .is_none_or(|d| d.0.is_none_or(|time| time.elapsed().as_millis() >= 100));
    if active && due {
        if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
            wake.after(std::time::Duration::from_millis(100));
        }
        if let Some(mut deadline) = world.get_resource_mut::<WakeDeadline>() {
            deadline.0 = Some(Instant::now());
        }
    }
    let task_lists: Vec<_> = world
        .query::<(Entity, &Tasks)>()
        .iter(world)
        .map(|(entity, t)| (entity, t.root, t.workspace, t.signature.clone()))
        .collect();
    for (entity, root, workspace, previous) in task_lists {
        let tasks: Vec<_> = world
            .get::<Installation>(root)
            .and_then(|i| i.0.get(&workspace))
            .map(|s| {
                s.runtime
                    .requests
                    .values()
                    .map(|r| {
                        (
                            r.id.clone(),
                            r.prompt.chars().take(72).collect::<String>(),
                            r.status,
                            r.terminal(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let signature = format!("{tasks:?}");
        if signature == previous {
            continue;
        }
        sand_panel::clear(world, entity);
        for (id, prompt, status, terminal) in tasks {
            crate::native::edit_mode::label(world, entity, &format!("{status:?}: {prompt}"), 13.0);
            sand_panel::button(
                world,
                entity,
                entity,
                if terminal {
                    "Dismiss finished request"
                } else {
                    "Cancel this request"
                },
                RequestControl {
                    root,
                    workspace,
                    id,
                    dismiss: terminal,
                },
            );
        }
        world.get_mut::<Tasks>(entity).unwrap().signature = signature;
    }
    let panels: Vec<_> = world
        .query::<(Entity, &Panel)>()
        .iter(world)
        .map(|(entity, p)| (entity, p.root, p.workspace, p.status))
        .collect();
    for (entity, root, workspace, status) in panels {
        let Some(session) = world
            .get::<Installation>(root)
            .and_then(|i| i.0.get(&workspace))
        else {
            continue;
        };
        let summary = format!(
            "{}\n{} device sessions · {} observations · {} pointers · {} requests\nOutput {}. Camera input {}. One-shot captures pending: {}.",
            session.status,
            session.runtime.connections.len(),
            session.runtime.observations.len(),
            session.runtime.pointers.len(),
            session.runtime.requests.len(),
            if session.runtime.blanked {
                "blanked"
            } else {
                "enabled"
            },
            if session
                .runtime
                .connections
                .values()
                .any(|c| c.capture_enabled)
            {
                "enabled"
            } else {
                "off"
            },
            media::pending(world, root, workspace)
        );
        if let Some(mut text) = world.get_mut::<Text>(status) {
            if text.0 != summary {
                text.0 = summary;
            }
        }
        if world
            .get::<Workspaces>(root)
            .is_none_or(|s| s.active != workspace)
        {
            if let Some(mut node) = world.get_mut::<Node>(entity) {
                node.display = Display::None;
            }
        }
    }
}

#[cfg(test)]
mod tests;
