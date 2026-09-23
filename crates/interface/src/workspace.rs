use crate::{
    canvas::{CanvasItem, CanvasView},
    container::BoxRoot,
    record_view::ReceiveRecords,
    sand_store::{SandKind, StoredSand},
    sand_text::{self, SavedText},
};
use bevy::{input_focus::InputFocus, math::DVec2, prelude::*};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    io::{self, Write},
    path::PathBuf,
};

mod recovery;
pub(crate) mod storage;

#[derive(Clone, Serialize, Deserialize)]
pub struct Workspace {
    #[serde(default)]
    pub topology: crate::topology::view::View,
    pub id: u64,
    pub name: String,
    pub center: [f64; 2],
    pub zoom: f64,
    #[serde(default)]
    pub colors: crate::canvas_background::CanvasColors,
    #[serde(default)]
    pub color_overrides: [bool; 2],
}

#[derive(Component)]
pub struct Workspaces {
    pub active: u64,
    pub entries: Vec<Workspace>,
    pub error: Option<String>,
    pub(crate) saved_records: HashMap<String, SavedRecord>,
}

impl Default for Workspaces {
    fn default() -> Self {
        Self {
            active: 1,
            entries: vec![Workspace {
                topology: Default::default(),
                id: 1,
                name: "Home".into(),
                center: [0.0; 2],
                zoom: 1.0,
                colors: Default::default(),
                color_overrides: [false; 2],
            }],
            error: None,
            saved_records: HashMap::new(),
        }
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceMember(pub u64);

#[derive(Component)]
pub struct RecordPlacement(pub String);

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SavedRecord {
    #[serde(default)]
    placement: crate::sand_placement::Placement,
    uid: String,
    #[serde(default)]
    pub(crate) tokens: crate::tokens::TokenOverrides,
    workspace: u64,
    position: [f64; 2],
    size: [f32; 2],
}

#[derive(Clone, Serialize, Deserialize)]
struct SavedSand {
    #[serde(default)]
    placement: crate::sand_placement::Placement,
    #[serde(default)]
    tokens: crate::tokens::TokenOverrides,
    kind: SandKind,
    texts: Vec<SavedText>,
    workspace: u64,
    position: [f64; 2],
    size: [f32; 2],
    #[serde(default)]
    timer: Option<crate::work_timer::LocalTimer>,
    #[serde(default)]
    todo: Option<crate::todo::SavedTodo>,
}

#[derive(Serialize, Deserialize)]
struct Document {
    #[serde(skip)]
    recovery: recovery::Report,
    #[serde(default)]
    assertions: Vec<crate::assertion_castle::SavedAssertion>,
    #[serde(default)]
    shaders: Vec<crate::shader_castle::SavedShader>,
    #[serde(default)]
    layouts: Vec<crate::layout::records::Saved>,
    #[serde(default)]
    imports: Vec<crate::topology::assets::SavedAsset>,
    #[serde(default)]
    theme: crate::tokens::ThemeSettings,
    active: u64,
    workspaces: Vec<Workspace>,
    sands: Vec<SavedSand>,
    records: Vec<SavedRecord>,
    #[serde(default)]
    areas: Vec<crate::area::SavedArea>,
    #[serde(default)]
    proteins: Vec<crate::protein_castle::SavedProteinCastle>,
    #[serde(default)]
    karma_castles: Vec<crate::karma_castle::SavedKarmaCastle>,
    #[serde(default)]
    frequency_castles: Vec<crate::frequency_castle::SavedFrequencyCastle>,
    #[serde(default)]
    transfer_castles: Vec<crate::transfer_castle::SavedTransferCastle>,
    #[serde(default)]
    recorders: Vec<crate::recorder_castle::SavedRecorder>,
    #[serde(default)]
    calendars: Vec<crate::calendar::SavedCalendar>,
    #[serde(default)]
    kanbans: Vec<crate::kanban::SavedKanban>,
    #[serde(default)]
    instincts: Vec<crate::instinct::SavedInstinct>,
}

impl Document {
    fn validate(&self) -> bool {
        let ids: HashSet<_> = self.workspaces.iter().map(|space| space.id).collect();
        let area_ids: HashSet<_> = self.areas.iter().map(|saved| &saved.area.id).collect();
        self.theme.validate()
            && self
                .assertions
                .iter()
                .all(|saved| ids.contains(&saved.frame.workspace) && saved.valid())
            && self
                .shaders
                .iter()
                .all(|saved| ids.contains(&saved.0.workspace) && saved.valid())
            && self
                .karma_castles
                .iter()
                .all(|saved| ids.contains(&saved.workspace) && saved.valid())
            && self
                .recorders
                .iter()
                .all(|saved| ids.contains(&saved.workspace) && saved.valid())
            && self
                .frequency_castles
                .iter()
                .all(|saved| ids.contains(&saved.workspace) && saved.valid())
            && self
                .transfer_castles
                .iter()
                .all(|saved| ids.contains(&saved.workspace) && saved.valid())
            && self
                .instincts
                .iter()
                .all(|saved| ids.contains(&saved.workspace) && saved.valid())
            && self
                .kanbans
                .iter()
                .all(|saved| ids.contains(&saved.workspace) && saved.valid())
            && self
                .layouts
                .iter()
                .all(|saved| saved.valid() && ids.contains(&saved.workspace))
            && self
                .calendars
                .iter()
                .all(|saved| ids.contains(&saved.workspace) && saved.valid())
            && self.imports.iter().all(|saved| {
                saved.asset.valid()
                    && saved.placement.valid()
                    && ids.contains(&saved.workspace)
                    && valid_geometry(saved.position, saved.size)
            })
            && self
                .proteins
                .iter()
                .all(|saved| ids.contains(&saved.workspace) && saved.valid())
            && self.areas.len() <= crate::area::MAX_AREAS
            && area_ids.len() == self.areas.len()
            && self.areas.iter().all(|saved| {
                ids.contains(&saved.workspace) && saved.area.validate() && saved.placement.valid()
            })
            && !ids.is_empty()
            && ids.len() == self.workspaces.len()
            && ids.contains(&self.active)
            && self.workspaces.iter().all(Workspace::valid)
            && self
                .sands
                .iter()
                .all(|sand| ids.contains(&sand.workspace) && sand.valid())
            && self
                .records
                .iter()
                .all(|record| ids.contains(&record.workspace) && record.valid())
    }
}

impl Workspace {
    fn valid(&self) -> bool {
        self.topology.valid()
            && !self.name.trim().is_empty()
            && self.name.chars().count() <= 80
            && DVec2::from_array(self.center).is_finite()
            && (CanvasView::MIN_ZOOM..=CanvasView::MAX_ZOOM).contains(&self.zoom)
    }
}

impl SavedSand {
    fn valid(&self) -> bool {
        self.tokens.validate()
            && self.placement.valid()
            && valid_geometry(self.position, self.size)
            && self.texts.len() <= 128
            && self.texts.iter().all(SavedText::validate)
            && self
                .timer
                .as_ref()
                .is_none_or(|timer| self.kind == SandKind::WorkTimer && timer.valid())
            && self
                .todo
                .as_ref()
                .is_none_or(|todo| self.kind == SandKind::Todo && todo.valid())
    }
}

impl SavedRecord {
    fn valid(&self) -> bool {
        self.tokens.validate() && self.placement.valid() && valid_geometry(self.position, self.size)
    }
}

fn valid_geometry(position: [f64; 2], size: [f32; 2]) -> bool {
    DVec2::from_array(position).is_finite()
        && Vec2::from_array(size).is_finite()
        && Vec2::from_array(size).min_element() > 0.0
}

#[derive(Resource)]
pub struct WorkspaceFile {
    path: PathBuf,
    last: Option<Vec<u8>>,
    blocked: bool,
    settings: cell::InterfaceStorage,
    writer: Option<storage::Writer>,
}

impl WorkspaceFile {
    pub(crate) fn directory(&self) -> &std::path::Path {
        self.path.parent().unwrap_or(std::path::Path::new("."))
    }
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            last: None,
            blocked: false,
            settings: Default::default(),
            writer: None,
        }
    }

    pub fn with_settings(path: PathBuf, settings: cell::InterfaceStorage) -> io::Result<Self> {
        settings.validate().map_err(io::Error::other)?;
        Ok(Self {
            settings,
            ..Self::new(path)
        })
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PrepareWorkspaces;

pub struct WorkspacePlugin;
impl Plugin for WorkspacePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<crate::tokens::ThemeSettings>()
            .add_message::<AppExit>()
            .add_systems(
                Update,
                initialize.in_set(PrepareWorkspaces).before(ReceiveRecords),
            )
            .add_systems(Last, persist);
    }
}

fn initialize(world: &mut World) {
    let roots: Vec<_> = world
        .query_filtered::<Entity, (With<BoxRoot>, Without<Workspaces>)>()
        .iter(world)
        .collect();
    for root in roots {
        let document = world
            .get_resource::<WorkspaceFile>()
            .map(|file| read_document(&file.path));
        let mut spaces = Workspaces::default();
        let mut restored = false;
        match document {
            Some(Ok(Some(document))) => {
                let recovered = document.recovery.changed();
                if recovered {
                    let path = world.resource::<WorkspaceFile>().path.clone();
                    let message = match document.recovery.preserve(&path) {
                        Ok(backup) => document.recovery.message(backup.as_deref()),
                        Err(error) => {
                            world.resource_mut::<WorkspaceFile>().blocked = true;
                            let message = format!(
                                "{}. Saving is paused because the original snapshot could not be backed up: {error}",
                                document.recovery.summary()
                            );
                            spaces.error = Some(message.clone());
                            message
                        }
                    };
                    crate::notifications::report(world, "interface::workspaces", &message);
                }
                if !recovered
                    && storage::snapshots(
                        &world
                            .resource::<WorkspaceFile>()
                            .path
                            .with_extension("snapshots"),
                    )
                    .is_ok_and(|files| !files.is_empty())
                {
                    world.resource_mut::<WorkspaceFile>().last =
                        Some(serde_json::to_vec_pretty(&document).unwrap());
                }
                restored = true;
                world
                    .entity_mut(root)
                    .insert(crate::layout::records::SavedLayouts(document.layouts));
                world.insert_resource(document.theme);
                spaces.active = document.active;
                spaces.entries = document.workspaces;
                spaces.saved_records = document
                    .records
                    .into_iter()
                    .map(|record| (record.uid.clone(), record))
                    .collect();
                for saved in document.areas {
                    if let Some(entity) =
                        crate::area::spawn_area(world, root, saved.workspace, saved.area)
                    {
                        saved.placement.restore(world, entity);
                    }
                }
                for saved in document.proteins {
                    saved.restore(world, root);
                }
                for saved in document.shaders {
                    saved.restore(world, root);
                }
                for saved in document.assertions {
                    saved.restore(world, root);
                }
                for saved in document.karma_castles {
                    saved.restore(world, root);
                }
                for saved in document.recorders {
                    saved.restore(world, root);
                }
                for saved in document.transfer_castles {
                    saved.restore(world, root);
                }
                for saved in document.frequency_castles {
                    saved.restore(world, root);
                }
                for saved in document.calendars {
                    saved.restore(world, root);
                }
                for saved in document.kanbans {
                    saved.restore(world, root);
                }
                for saved in document.instincts {
                    saved.restore(world, root);
                }
                for saved in document.imports {
                    let entity = crate::topology::assets::spawn(
                        world,
                        root,
                        saved.workspace,
                        DVec2::from_array(saved.position),
                        saved.asset,
                    );
                    world.get_mut::<CanvasItem>(entity).unwrap().size =
                        Vec2::from_array(saved.size);
                    saved.placement.restore(world, entity);
                }
                for sand in document.sands {
                    let entity = crate::sand_store::spawn_sand(
                        world,
                        root,
                        sand.workspace,
                        SandKind::Square,
                        "",
                        DVec2::from_array(sand.position),
                    );
                    world.get_mut::<CanvasItem>(entity).unwrap().size = Vec2::from_array(sand.size);
                    sand.placement.restore(world, entity);
                    let mut content = None;
                    if sand.kind == SandKind::Operation {
                        content = Some(crate::operation::populate(world, root, entity));
                    }
                    if sand.kind == SandKind::AccessControl {
                        content = Some(crate::access_control::populate(world, root, entity));
                    }
                    if sand.kind == SandKind::Sync {
                        content = Some(crate::sync_castle::populate(world, root, entity));
                    }
                    if sand.kind == SandKind::Freedoom {
                        content = Some(crate::freedoom::populate(world, root, entity));
                    }
                    if sand.kind == SandKind::Terminal {
                        content = Some(crate::terminal::populate(world, root, entity));
                    }
                    if sand.kind == SandKind::Configuration {
                        content = Some(crate::configuration::populate(world, root, entity));
                    }
                    if sand.kind == SandKind::Todo {
                        content = Some(crate::todo::populate(world, root, entity));
                        if let Some(todo) = sand.todo {
                            crate::todo::restore(world, entity, todo);
                        }
                    }
                    for text in sand.texts {
                        let block = sand_text::spawn(world, entity, text);
                        content.get_or_insert(block);
                    }
                    *world.get_mut::<CanvasItem>(entity).unwrap() = CanvasItem {
                        position: DVec2::from_array(sand.position),
                        size: Vec2::from_array(sand.size),
                    };
                    world.entity_mut(entity).insert((
                        sand.tokens,
                        crate::token_style::AppliedSize(Vec2::from_array(sand.size)),
                    ));
                    if let Some(timer) = sand.timer {
                        world.entity_mut(entity).insert(timer);
                    }
                    world.entity_mut(entity).insert(StoredSand {
                        kind: sand.kind,
                        content,
                    });
                }
            }
            Some(Err(error)) => {
                let message =
                    format!("Could not load workspaces: {error}. Saved data has been kept.");
                crate::notifications::report(world, "interface::workspaces", &message);
                spaces.error = Some(message);
                world.resource_mut::<WorkspaceFile>().blocked = true;
            }
            _ => {}
        }
        let active = spaces
            .entries
            .iter()
            .find(|space| space.id == spaces.active)
            .unwrap();
        if restored {
            *world.get_mut::<CanvasView>(root).unwrap() = CanvasView {
                center: DVec2::from_array(active.center),
                zoom: active.zoom,
            };
        }
        let topology = active.topology;
        let seed = !restored
            && spaces.error.is_none()
            && world.get::<crate::instinct::SeedInstinct>(root).is_some();
        world.entity_mut(root).insert((spaces, topology));
        world
            .entity_mut(root)
            .remove::<crate::instinct::SeedInstinct>();
        crate::workspace_config::initialize(world, root);
        if seed {
            crate::instinct::spawn(world, root, 1, DVec2::ZERO, Default::default());
        }
    }
}

fn read_document(path: &std::path::Path) -> io::Result<Option<Document>> {
    storage::load(path)
}

fn read_snapshot(path: &std::path::Path) -> io::Result<Option<Document>> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let document: Document = serde_json::from_slice(&bytes)?;
    if !document.validate() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid workspace data",
        ));
    }
    Ok(Some(document))
}

pub(crate) fn regroup_saved(
    world: &mut World,
    root: Entity,
    groups: &[crate::canvas_selection::SandGroup],
    target: Option<crate::canvas_selection::SandGroup>,
) {
    if let Some(mut spaces) = world.get_mut::<Workspaces>(root) {
        let active = spaces.active;
        for record in spaces.saved_records.values_mut() {
            if record.workspace == active
                && record
                    .placement
                    .group
                    .is_some_and(|group| groups.contains(&group))
            {
                record.placement.group = target;
            }
        }
    }
}

pub fn switch(world: &mut World, root: Entity, target: u64) -> bool {
    let Some(spaces) = world.get::<Workspaces>(root) else {
        return false;
    };
    if spaces.active == target || !spaces.entries.iter().any(|entry| entry.id == target) {
        return false;
    }
    crate::canvas_selection::clear(world, root);
    let camera = *world.get::<CanvasView>(root).unwrap();
    let topology = world
        .get::<crate::topology::view::View>(root)
        .copied()
        .unwrap_or_default();
    let mut spaces = world.get_mut::<Workspaces>(root).unwrap();
    let active = spaces.active;
    let previous = spaces
        .entries
        .iter_mut()
        .find(|entry| entry.id == active)
        .unwrap();
    previous.center = camera.center.to_array();
    previous.zoom = camera.zoom;
    previous.topology = topology;
    let next = spaces
        .entries
        .iter()
        .find(|entry| entry.id == target)
        .unwrap();
    let camera = CanvasView {
        center: DVec2::from_array(next.center),
        zoom: next.zoom,
    };
    let topology = next.topology;
    spaces.active = target;
    world.entity_mut(root).insert(topology);
    *world.get_mut::<CanvasView>(root).unwrap() = camera;
    world.resource_mut::<InputFocus>().clear();
    crate::workspace_config::reload(world, root, target);
    true
}

pub fn create(world: &mut World, root: Entity) {
    let spaces = world.get::<Workspaces>(root).unwrap();
    let Some(id) = spaces
        .entries
        .iter()
        .map(|entry| entry.id)
        .max()
        .unwrap_or(0)
        .checked_add(1)
    else {
        return;
    };
    let Some(id) = crate::workspace_config::next_id(world, root, id) else {
        return;
    };
    let mut spaces = world.get_mut::<Workspaces>(root).unwrap();
    spaces.entries.push(Workspace {
        topology: Default::default(),
        id,
        name: format!("Workspace {id}"),
        center: [0.0; 2],
        zoom: 1.0,
        colors: Default::default(),
        color_overrides: [false; 2],
    });
    switch(world, root, id);
}

pub fn rename(world: &mut World, root: Entity, name: &str) -> bool {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return false;
    }
    let mut spaces = world.get_mut::<Workspaces>(root).unwrap();
    let active = spaces.active;
    spaces
        .entries
        .iter_mut()
        .find(|entry| entry.id == active)
        .unwrap()
        .name = name.into();
    true
}

pub fn remove(world: &mut World, root: Entity, removed: u64) -> bool {
    let spaces = world.get::<Workspaces>(root).unwrap();
    if spaces.entries.len() <= 1 || !spaces.entries.iter().any(|entry| entry.id == removed) {
        return false;
    }
    let next = if spaces.active == removed {
        spaces
            .entries
            .iter()
            .find(|entry| entry.id != removed)
            .map(|entry| entry.id)
            .unwrap()
    } else {
        spaces.active
    };
    if spaces.active == removed {
        switch(world, root, next);
    }
    let mut spaces = world.get_mut::<Workspaces>(root).unwrap();
    spaces.entries.retain(|entry| entry.id != removed);
    for record in spaces.saved_records.values_mut() {
        if record.workspace == removed {
            record.workspace = next;
        }
    }
    let mut items = world.query::<(&ChildOf, &mut WorkspaceMember)>();
    for (parent, mut member) in items.iter_mut(world) {
        if parent.parent() == root && member.0 == removed {
            member.0 = next;
        }
    }
    true
}

pub fn remove_active(world: &mut World, root: Entity) -> bool {
    let removed = world.get::<Workspaces>(root).unwrap().active;
    remove(world, root, removed)
}

pub(crate) fn place_record(world: &mut World, root: Entity, card: Entity, uid: &str) {
    let placement = world.get::<Workspaces>(root).map(|spaces| {
        let saved = spaces.saved_records.get(uid).cloned();
        (spaces.entries[0].id, saved)
    });
    let (mut workspace, saved) = placement.unwrap_or((1, None));
    if let Some(saved) = saved {
        saved.placement.restore(world, card);
        workspace = saved.workspace;
        world.entity_mut(card).insert((
            saved.tokens,
            crate::token_style::AppliedSize(Vec2::from_array(saved.size)),
        ));
        *world.get_mut::<CanvasItem>(card).unwrap() = CanvasItem {
            position: DVec2::from_array(saved.position),
            size: Vec2::from_array(saved.size),
        };
    }
    world
        .entity_mut(card)
        .insert((WorkspaceMember(workspace), RecordPlacement(uid.into())));
}

fn snapshot(world: &mut World, root: Entity) -> Document {
    let spaces = world.get::<Workspaces>(root).unwrap();
    let active = spaces.active;
    let mut workspaces = spaces.entries.clone();
    let camera = world.get::<CanvasView>(root).unwrap();
    let current = workspaces
        .iter_mut()
        .find(|entry| entry.id == active)
        .unwrap();
    current.center = camera.center.to_array();
    current.zoom = camera.zoom;
    current.topology = world
        .get::<crate::topology::view::View>(root)
        .copied()
        .unwrap_or_default();
    let mut records = spaces.saved_records.clone();
    let mut sands = Vec::new();
    let mut items = world.query::<(
        Entity,
        &ChildOf,
        &CanvasItem,
        &WorkspaceMember,
        Option<&StoredSand>,
        Option<&RecordPlacement>,
    )>();
    for (entity, parent, item, member, sand, record) in items.iter(world) {
        if parent.parent() != root {
            continue;
        }
        if let Some(record) = record {
            records.insert(
                record.0.clone(),
                SavedRecord {
                    placement: crate::sand_placement::Placement::capture(world, entity),
                    uid: record.0.clone(),
                    tokens: crate::token_style::overrides(world, entity),
                    workspace: member.0,
                    position: item.position.to_array(),
                    size: item.size.to_array(),
                },
            );
        }
        if let Some(sand) = sand {
            sands.push(SavedSand {
                placement: crate::sand_placement::Placement::capture(world, entity),
                tokens: crate::token_style::overrides(world, entity),
                kind: sand.kind,
                texts: sand_text::snapshot(world, entity),
                timer: world.get::<crate::work_timer::LocalTimer>(entity).cloned(),
                todo: crate::todo::snapshot(world, entity),
                workspace: member.0,
                position: item.position.to_array(),
                size: item.size.to_array(),
            });
        }
    }
    let mut records: Vec<_> = records.into_values().collect();
    records.sort_by(|a, b| a.uid.cmp(&b.uid));
    let mut areas: Vec<_> = world
        .query_filtered::<(
            Entity,
            &crate::area::InfluenceArea,
            &WorkspaceMember,
            &ChildOf,
        ), Without<crate::protein_area::grouping::GeneratedGroup>>()
        .iter(world)
        .filter(|(_, _, _, parent)| parent.parent() == root)
        .map(|(entity, area, member, _)| crate::area::SavedArea {
            placement: crate::sand_placement::Placement::capture(world, entity),
            workspace: member.0,
            area: crate::record_presentation::capture(world, entity, area.clone()),
        })
        .collect();
    areas.sort_by(|a, b| a.area.id.cmp(&b.area.id));
    Document {
        recovery: Default::default(),
        layouts: crate::layout::records::snapshot(world, root),
        imports: crate::topology::assets::snapshot(world, root),
        theme: world.resource::<crate::tokens::ThemeSettings>().clone(),
        active,
        workspaces,
        sands,
        records,
        areas,
        proteins: crate::protein_castle::snapshot(world, root),
        shaders: crate::shader_castle::snapshot(world, root),
        assertions: crate::assertion_castle::snapshot(world, root),
        karma_castles: crate::karma_castle::snapshot(world, root),
        frequency_castles: crate::frequency_castle::snapshot(world, root),
        transfer_castles: crate::transfer_castle::snapshot(world, root),
        recorders: crate::recorder_castle::snapshot(world, root),
        calendars: crate::calendar::snapshot(world, root),
        kanbans: crate::kanban::snapshot(world, root),
        instincts: crate::instinct::snapshot(world, root),
    }
}

fn persist(world: &mut World) {
    if crate::laboratory::active(world) {
        if world.resource::<Messages<AppExit>>().is_empty() {
            return;
        }
        crate::laboratory::close(world);
    }
    if world
        .get_resource::<WorkspaceFile>()
        .is_none_or(|file| file.blocked)
    {
        return;
    }
    let Some(root) = world
        .query_filtered::<Entity, (With<Workspaces>, Without<crate::laboratory::LaboratoryRoot>)>()
        .iter(world)
        .next()
    else {
        return;
    };
    let quitting = !world.resource::<Messages<AppExit>>().is_empty();
    if world.resource::<WorkspaceFile>().writer.is_none() {
        let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
        let mut file = world.resource_mut::<WorkspaceFile>();
        match storage::Writer::start(file.path.clone(), file.last.take(), file.settings, wake) {
            Ok(writer) => file.writer = Some(writer),
            Err(error) => {
                report(
                    world,
                    root,
                    Some(format!("Could not start workspace saving: {error}")),
                );
                if quitting {
                    world.resource_mut::<Messages<AppExit>>().clear();
                }
                return;
            }
        }
    }
    let writer = world.resource::<WorkspaceFile>().writer.as_ref().unwrap();
    if let Some(result) = writer.result() {
        report(world, root, result.err());
    }
    let writer = world.resource::<WorkspaceFile>().writer.as_ref().unwrap();
    if !quitting && !writer.due() {
        return;
    }
    let document = snapshot(world, root);
    if !document.validate() {
        report(
            world,
            root,
            Some("Could not save workspaces: invalid scene data".into()),
        );
        if quitting {
            world.resource_mut::<Messages<AppExit>>().clear();
        }
        return;
    }
    let result = world
        .resource::<WorkspaceFile>()
        .writer
        .as_ref()
        .unwrap()
        .save(document, quitting);
    if let Err(error) = result {
        report(world, root, Some(error));
        if quitting {
            world.resource_mut::<Messages<AppExit>>().clear();
        }
    }
}

fn report(world: &mut World, root: Entity, error: Option<String>) {
    let mut spaces = world.get_mut::<Workspaces>(root).unwrap();
    if spaces.error != error {
        spaces.error = error.clone();
        if let Some(error) = error {
            crate::notifications::report(world, "interface::workspaces", &error);
        }
        if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
            wake.ring();
        }
    }
}

fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = path.with_extension("json.tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    }
    let mut file = options.open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    std::fs::rename(temporary, path)?;
    sync_parent(path)
}

fn sync_parent(path: &std::path::Path) -> io::Result<()> {
    sync_directory(
        path.parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(std::path::Path::new(".")),
    )
}

fn sync_directory(_path: &std::path::Path) -> io::Result<()> {
    #[cfg(unix)]
    std::fs::File::open(_path)?.sync_all()?;
    Ok(())
}

pub(crate) mod tests {
    use super::*;
    use crate::theme::Typography;
    use bevy::text::EditableText;

    fn fixture(path: Option<PathBuf>) -> (App, Entity) {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>()
            .init_resource::<Typography>()
            .init_resource::<InputFocus>()
            .add_plugins(WorkspacePlugin);
        if let Some(path) = path {
            app.insert_resource(WorkspaceFile::new(path));
        }
        let root = app.world_mut().spawn(BoxRoot).id();
        app.update();
        flush(&mut app);
        (app, root)
    }

    fn flush(app: &mut App) {
        app.world_mut().write_message(AppExit::Success);
        app.update();
        app.world_mut().resource_mut::<Messages<AppExit>>().clear();
    }

    #[cfg_attr(test, test)]
    fn instinct_seeds_once_and_saved_or_deleted_readers_stay_that_way() {
        fn seeded(path: PathBuf) -> (App, Entity) {
            let mut app = App::new();
            crate::laboratory::isolate(app.world_mut());
            app.init_resource::<Assets<Font>>()
                .init_resource::<Typography>()
                .init_resource::<InputFocus>()
                .add_plugins(WorkspacePlugin)
                .insert_resource(WorkspaceFile::new(path));
            let root = app
                .world_mut()
                .spawn((BoxRoot, crate::instinct::SeedInstinct))
                .id();
            app.update();
            flush(&mut app);
            (app, root)
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interface.json");
        let (mut app, root) = seeded(path.clone());
        let reader = app
            .world_mut()
            .query_filtered::<Entity, With<crate::instinct::Instinct>>()
            .single(app.world())
            .unwrap();
        assert_eq!(app.world().get::<WorkspaceMember>(reader).unwrap().0, 1);
        assert_eq!(app.world().get::<ChildOf>(reader).unwrap().parent(), root);
        app.world_mut()
            .get_mut::<crate::instinct::Instinct>(reader)
            .unwrap()
            .page = Some("tool".into());
        flush(&mut app);
        drop(app);
        let (mut app, _) = seeded(path.clone());
        let (reader, instinct) = app
            .world_mut()
            .query::<(Entity, &crate::instinct::Instinct)>()
            .single(app.world())
            .unwrap();
        assert_eq!(instinct.page.as_deref(), Some("tool"));
        app.world_mut().despawn(reader);
        flush(&mut app);
        drop(app);
        let (mut app, _) = seeded(path.clone());
        assert_eq!(
            app.world_mut()
                .query::<&crate::instinct::Instinct>()
                .iter(app.world())
                .count(),
            0
        );
        drop(app);
        let path = directory.path().join("broken.json");
        std::fs::write(&path, b"broken").unwrap();
        let (mut app, root) = seeded(path);
        assert!(app.world().get::<Workspaces>(root).unwrap().error.is_some());
        assert_eq!(
            app.world_mut()
                .query::<&crate::instinct::Instinct>()
                .iter(app.world())
                .count(),
            0
        );
    }

    #[test]
    fn standalone_time_log_and_running_stopwatch_survive_restart() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interface.json");
        let (mut app, root) = fixture(Some(path.clone()));
        let sand = crate::sand_store::spawn_sand(
            app.world_mut(),
            root,
            1,
            SandKind::WorkTimer,
            "",
            DVec2::ZERO,
        );
        let timer: crate::work_timer::LocalTimer = serde_json::from_value(serde_json::json!({"logs":[
            {"id":"work.log:first", "start":"2026-09-19T10:00:00Z", "end":"2026-09-19T10:05:00Z"},
            {"id":"work.log:running", "start":"2026-09-19T11:00:00Z", "end":null}
        ]})).unwrap();
        app.world_mut().entity_mut(sand).insert(timer.clone());
        flush(&mut app);
        drop(app);
        let (mut app, _) = fixture(Some(path));
        let restored = app
            .world_mut()
            .query::<&crate::work_timer::LocalTimer>()
            .single(app.world())
            .unwrap();
        assert_eq!(restored, &timer);
        assert!(restored.valid());
    }

    #[test]
    fn migrated_sands_and_todo_preferences_survive_workspace_restart() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interface.json");
        let (mut app, root) = fixture(Some(path.clone()));
        for (index, kind) in [
            SandKind::Freedoom,
            SandKind::Terminal,
            SandKind::Configuration,
            SandKind::Todo,
        ]
        .into_iter()
        .enumerate()
        {
            let sand = crate::sand_store::spawn_sand(
                app.world_mut(),
                root,
                1,
                kind,
                "",
                DVec2::new(index as f64 * 600.0, 40.0),
            );
            if kind == SandKind::Todo {
                let saved = serde_json::from_value(serde_json::json!({"protein":"weekend", "show_ids":true, "draft":"Unfinished task"})).unwrap();
                crate::todo::restore(app.world_mut(), sand, saved);
            }
        }
        flush(&mut app);
        drop(app);
        let (mut app, _) = fixture(Some(path));
        let sands: Vec<_> = app
            .world_mut()
            .query::<(Entity, &StoredSand)>()
            .iter(app.world())
            .map(|(entity, sand)| (entity, sand.kind))
            .collect();
        assert_eq!(sands.len(), 4);
        for kind in [
            SandKind::Freedoom,
            SandKind::Terminal,
            SandKind::Configuration,
            SandKind::Todo,
        ] {
            let owner = sands.iter().find(|(_, found)| *found == kind).unwrap().0;
            let populated = match kind {
                SandKind::Freedoom => app
                    .world()
                    .get::<crate::freedoom::FreedoomSand>(owner)
                    .is_some(),
                SandKind::Terminal => app
                    .world()
                    .get::<crate::terminal::TerminalSand>(owner)
                    .is_some(),
                SandKind::Configuration => app
                    .world()
                    .get::<crate::configuration::ConfigurationSand>(owner)
                    .is_some(),
                SandKind::Todo => app.world().get::<crate::todo::TodoSand>(owner).is_some(),
                _ => unreachable!(),
            };
            assert!(populated);
            assert!(app.world().get::<CanvasItem>(owner).unwrap().size.x >= 500.0);
            if kind == SandKind::Todo {
                let saved = crate::todo::snapshot(app.world(), owner).unwrap();
                assert_eq!(
                    serde_json::to_value(saved).unwrap(),
                    serde_json::json!({"protein":"weekend", "show_ids":true, "draft":"Unfinished task"})
                );
            }
        }
    }

    #[test]
    fn topology_restart_preserves_imports_depth_and_mixed_group_transforms() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interface.json");
        let (mut app, root) = fixture(Some(path.clone()));
        let sand = crate::sand_store::spawn_sand(
            app.world_mut(),
            root,
            1,
            SandKind::Square,
            "",
            DVec2::new(30.0, 40.0),
        );
        app.world_mut()
            .entity_mut(sand)
            .insert(crate::topology::Spatial {
                depth: Some(27.0),
                ..default()
            });
        let asset = crate::topology::assets::ImportedAsset {
            id: "a".repeat(32),
            file: "source.glb".into(),
            name: "Model".into(),
            scale: 100.0,
        };
        let imported = crate::topology::assets::spawn(
            app.world_mut(),
            root,
            1,
            DVec2::new(200.0, 300.0),
            asset.clone(),
        );
        crate::topology::assets::spawn(app.world_mut(), root, 1, DVec2::new(-200.0, -300.0), asset);
        let mut influence = crate::area::InfluenceArea::new(
            crate::area::AreaShape::Square,
            DVec2::new(80.0, 90.0),
            DVec2::splat(50.0),
        );
        influence.depth = 19.0;
        let area = crate::area::spawn_area(app.world_mut(), root, 1, influence).unwrap();
        for entity in [sand, imported, area] {
            app.world_mut()
                .entity_mut(entity)
                .insert(crate::canvas_selection::SandGroup([3; 16]));
        }
        crate::topology::groups::attach(app.world_mut(), &[sand, imported, area]);
        crate::topology::groups::transform(
            app.world_mut(),
            imported,
            bevy::math::DVec3::new(500.0, 70.0, -300.0),
            bevy::math::DQuat::from_rotation_y(0.7),
        );
        let before = snapshot(app.world_mut(), root);
        flush(&mut app);
        let (mut restored, root) = fixture(Some(path));
        let after = snapshot(restored.world_mut(), root);
        assert_eq!(after.imports.len(), 2);
        assert_eq!(after.areas[0].area.depth, 19.0);
        assert_eq!(after.sands[0].placement.spatial.depth, Some(27.0));
        let placements = |document: &Document| {
            let mut placements: Vec<_> = document
                .imports
                .iter()
                .map(|asset| serde_json::to_string(&asset.placement).unwrap())
                .collect();
            placements.sort();
            placements
        };
        assert_eq!(placements(&before), placements(&after));
        assert_eq!(
            serde_json::to_value(&before.areas[0].placement).unwrap(),
            serde_json::to_value(&after.areas[0].placement).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&before.sands[0].placement).unwrap(),
            serde_json::to_value(&after.sands[0].placement).unwrap()
        );
    }

    #[cfg_attr(test, test)]
    fn sand_groups_survive_workspace_restart_and_saved_record_regrouping() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interface.json");
        let (mut app, root) = fixture(Some(path.clone()));
        let group = crate::canvas_selection::SandGroup([9; 16]);
        for x in [-100.0, 100.0] {
            let sand = crate::sand_store::spawn_sand(
                app.world_mut(),
                root,
                1,
                SandKind::Square,
                "",
                DVec2::new(x, 0.0),
            );
            app.world_mut().entity_mut(sand).insert(group);
        }
        flush(&mut app);
        drop(app);
        let (mut app, root) = fixture(Some(path));
        let groups: Vec<_> = app
            .world_mut()
            .query::<&crate::canvas_selection::SandGroup>()
            .iter(app.world())
            .copied()
            .collect();
        assert_eq!(groups, vec![group, group]);
        let saved = SavedRecord {
            placement: crate::sand_placement::Placement {
                group: Some(group),
                ..default()
            },
            uid: "test".into(),
            tokens: default(),
            workspace: 1,
            position: [0.0; 2],
            size: [100.0; 2],
        };
        app.world_mut()
            .get_mut::<Workspaces>(root)
            .unwrap()
            .saved_records
            .insert("test".into(), saved);
        let next = crate::canvas_selection::SandGroup([10; 16]);
        regroup_saved(app.world_mut(), root, &[group], Some(next));
        assert_eq!(
            app.world().get::<Workspaces>(root).unwrap().saved_records["test"]
                .placement
                .group,
            Some(next)
        );
        regroup_saved(app.world_mut(), root, &[next], None);
        assert_eq!(
            app.world().get::<Workspaces>(root).unwrap().saved_records["test"]
                .placement
                .group,
            None
        );
    }

    #[cfg_attr(test, test)]
    fn areas_restore_identity_shape_properties_and_workspace_and_reject_invalid_snapshots() {
        use crate::area::{Direction, InfluenceArea, Property, PropertyRule, spawn_area};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interface.json");
        let (mut app, root) = fixture(Some(path.clone()));
        create(app.world_mut(), root);
        let workspace = app.world().get::<Workspaces>(root).unwrap().active;
        let mut area =
            InfluenceArea::drawn(&[DVec2::ZERO, DVec2::new(200.0, 0.0), DVec2::new(0.0, 100.0)])
                .unwrap();
        area.direction = Direction::Repel;
        area.strength = 42.5;
        area.reach = crate::area::Reach {
            mode: crate::area::ReachMode::Unlimited,
            shape: crate::area::ReachShape::Square,
            radius: 75.5,
        };
        area.rules = vec![PropertyRule {
            property: Property::Quantity,
            value: "-3".into(),
        }];
        spawn_area(app.world_mut(), root, workspace, area.clone()).unwrap();
        flush(&mut app);
        drop(app);
        let (mut app, root) = fixture(Some(path));
        let (restored, member) = app
            .world_mut()
            .query::<(&InfluenceArea, &WorkspaceMember)>()
            .single(app.world())
            .unwrap();
        assert_eq!(restored, &area);
        assert_eq!(member.0, workspace);
        let mut document = snapshot(app.world_mut(), root);
        assert!(document.validate());
        document.areas.push(document.areas[0].clone());
        assert!(!document.validate());
        document.areas.pop();
        document.areas[0].area.strength = -1.0;
        assert!(!document.validate());
        document.areas[0].area.strength = 42.5;
        document.areas[0].workspace = u64::MAX;
        assert!(!document.validate());
    }

    #[cfg_attr(test, test)]
    fn resized_bounds_survive_restart_without_changing_text_areas() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interface.json");
        let (mut app, root) = fixture(Some(path.clone()));
        let sand = crate::sand_store::spawn_sand(
            app.world_mut(),
            root,
            1,
            SandKind::EditableText,
            "Keep the complete text",
            DVec2::ZERO,
        );
        let position = DVec2::new(-70.0, 80.0);
        let size = Vec2::new(80.0, 60.0);
        *app.world_mut().get_mut::<CanvasItem>(sand).unwrap() = CanvasItem { position, size };
        app.world_mut().entity_mut(sand).insert((
            crate::sand_placement::Pinned {
                anchor: [0.2, 0.7],
                scale: 1.5,
            },
            ZIndex(17),
        ));
        flush(&mut app);
        drop(app);
        let (mut app, _) = fixture(Some(path));
        let (item, stored, pinned, order) = app
            .world_mut()
            .query::<(
                &CanvasItem,
                &StoredSand,
                &crate::sand_placement::Pinned,
                &ZIndex,
            )>()
            .single(app.world())
            .unwrap();
        assert_eq!(item.position, position);
        assert_eq!(item.size, size);
        assert_eq!(pinned.anchor, [0.2, 0.7]);
        assert_eq!(pinned.scale, 1.5);
        assert_eq!(order.0, 17);
        let text = stored.content.unwrap();
        assert_eq!(
            sand_text::value(app.world(), text),
            "Keep the complete text"
        );
        assert_eq!(
            app.world()
                .get::<crate::sand_text::SandText>(text)
                .unwrap()
                .size,
            [216.0, 152.0]
        );
    }

    #[cfg_attr(test, test)]
    fn protein_castles_restore_workspace_geometry_and_unfinished_queries() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interface.json");
        let (mut app, root) = fixture(Some(path.clone()));
        create(app.world_mut(), root);
        let workspace = app.world().get::<Workspaces>(root).unwrap().active;
        let mut draft = crate::protein_castle::ProteinDraft::default();
        draft.name = "My unfinished query".into();
        draft.query["limit"] = serde_json::json!("12x");
        let position = DVec2::new(-850.0, 300.0);
        let castle =
            crate::protein_castle::spawn(app.world_mut(), root, workspace, position, draft.clone());
        app.world_mut().get_mut::<CanvasItem>(castle).unwrap().size = Vec2::new(780.0, 620.0);
        flush(&mut app);
        drop(app);
        let (mut app, _) = fixture(Some(path));
        let (restored, item, member) = app
            .world_mut()
            .query::<(
                &crate::protein_castle::ProteinCastle,
                &CanvasItem,
                &WorkspaceMember,
            )>()
            .single(app.world())
            .unwrap();
        assert_eq!(restored.draft, draft);
        assert_eq!(item.position, position);
        assert_eq!(item.size, Vec2::new(780.0, 620.0));
        assert_eq!(member.0, workspace);
    }

    #[cfg_attr(test, test)]
    fn switching_restores_each_camera_and_keeps_live_drafts() {
        let (mut app, root) = fixture(None);
        let world = app.world_mut();
        let sand = crate::sand_store::spawn_sand(
            world,
            root,
            1,
            SandKind::EditableText,
            "keep my draft",
            DVec2::splat(1e12),
        );
        let editor = world.get::<StoredSand>(sand).unwrap().content.unwrap();
        world.get_mut::<CanvasView>(root).unwrap().center = DVec2::new(1e12, -1e12);
        world.get_mut::<CanvasView>(root).unwrap().zoom = 2.0;
        create(world, root);
        assert_eq!(world.get::<Workspaces>(root).unwrap().active, 2);
        assert_eq!(world.get::<CanvasView>(root).unwrap().center, DVec2::ZERO);
        world.get_mut::<CanvasView>(root).unwrap().zoom = 0.5;
        assert!(switch(world, root, 1));
        assert_eq!(
            world.get::<CanvasView>(root).unwrap().center,
            DVec2::new(1e12, -1e12)
        );
        assert_eq!(world.get::<CanvasView>(root).unwrap().zoom, 2.0);
        assert_eq!(
            world
                .get::<EditableText>(editor)
                .unwrap()
                .value()
                .to_string(),
            "keep my draft"
        );
        assert!(!switch(world, root, 777));
        assert!(switch(world, root, 2));
        assert_eq!(world.get::<CanvasView>(root).unwrap().zoom, 0.5);
        assert!(!rename(world, root, "  "));
        assert!(!rename(world, root, &"a".repeat(81)));
        assert!(rename(world, root, "  Writing  "));
        assert_eq!(
            world.get::<Workspaces>(root).unwrap().entries[1].name,
            "Writing"
        );
    }

    #[cfg_attr(test, test)]
    fn removing_workspace_moves_sands_without_deleting_record_drafts_or_other_boxes() {
        let (mut app, root) = fixture(None);
        let world = app.world_mut();
        let sand = crate::sand_store::spawn_sand(
            world,
            root,
            1,
            SandKind::EditableText,
            "retained",
            DVec2::ZERO,
        );
        let other_root = world.spawn_empty().id();
        let other = world.spawn((WorkspaceMember(1), ChildOf(other_root))).id();
        create(world, root);
        switch(world, root, 1);
        assert!(remove_active(world, root));
        assert_eq!(world.get::<WorkspaceMember>(sand).unwrap().0, 2);
        assert_eq!(world.get::<WorkspaceMember>(other).unwrap().0, 1);
        assert!(!remove_active(world, root));
        assert!(world.get_entity(sand).is_ok());
    }

    #[cfg_attr(test, test)]
    fn restart_restores_notes_workspaces_cameras_and_record_layouts_without_idle_writes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interface.json");
        let (mut app, root) = fixture(Some(path.clone()));
        let world = app.world_mut();
        create(world, root);
        rename(world, root, "Writing");
        let colors = crate::canvas_background::CanvasColors {
            background: [16, 32, 48],
            grid: [64, 80, 96],
        };
        world.get_mut::<Workspaces>(root).unwrap().entries[1].colors = colors;
        let sand = crate::sand_store::spawn_sand(
            world,
            root,
            2,
            SandKind::EditableText,
            "note",
            DVec2::new(1e12 + 0.25, -1e12),
        );
        let editor = world.get::<StoredSand>(sand).unwrap().content.unwrap();
        world
            .get_mut::<EditableText>(editor)
            .unwrap()
            .editor
            .set_text("saved as I type");
        world.get_mut::<CanvasView>(root).unwrap().center = DVec2::new(1e12, -1e12);
        world.get_mut::<CanvasView>(root).unwrap().zoom = 0.5;
        let record = world
            .spawn((
                CanvasItem {
                    position: DVec2::new(30.0, 50.0),
                    size: Vec2::new(120.0, 240.0),
                },
                ChildOf(root),
            ))
            .id();
        place_record(world, root, record, "uid-a");
        world.get_mut::<WorkspaceMember>(record).unwrap().0 = 2;
        flush(&mut app);
        let latest = storage::snapshots(&path.with_extension("snapshots"))
            .unwrap()
            .pop()
            .unwrap();
        let modified = std::fs::metadata(&latest).unwrap().modified().unwrap();
        for _ in 0..30 {
            app.update();
        }
        assert_eq!(
            modified,
            std::fs::metadata(&latest).unwrap().modified().unwrap()
        );
        let (mut loaded, root) = fixture(Some(path));
        let world = loaded.world_mut();
        assert_eq!(world.get::<Workspaces>(root).unwrap().active, 2);
        assert_eq!(
            world.get::<Workspaces>(root).unwrap().entries[1].name,
            "Writing"
        );
        assert_eq!(world.get::<CanvasView>(root).unwrap().zoom, 0.5);
        assert_eq!(
            world.get::<Workspaces>(root).unwrap().entries[1].colors,
            colors
        );
        let (sand, item) = world
            .query::<(&StoredSand, &CanvasItem)>()
            .single(world)
            .unwrap();
        assert_eq!(item.position, DVec2::new(1e12 + 0.25, -1e12));
        assert_eq!(
            world
                .get::<EditableText>(sand.content.unwrap())
                .unwrap()
                .value()
                .to_string(),
            "saved as I type"
        );
        let record = world
            .spawn(CanvasItem {
                position: DVec2::ZERO,
                size: Vec2::ONE,
            })
            .id();
        place_record(world, root, record, "uid-a");
        assert_eq!(world.get::<WorkspaceMember>(record).unwrap().0, 2);
        assert_eq!(
            world.get::<CanvasItem>(record).unwrap().position,
            DVec2::new(30.0, 50.0)
        );
    }

    #[cfg_attr(test, test)]
    fn token_layers_and_dragged_sizes_survive_restart() {
        use crate::{
            token_style::{self, TokenStylePlugin},
            tokens::{
                ColorScheme, SandStyleKind, ThemeSettings, Token, TokenOverrides, TokenValue,
            },
        };
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interface.json");
        let (mut app, root) = fixture(Some(path.clone()));
        app.add_plugins(TokenStylePlugin);
        let sand = crate::sand_store::spawn_sand(
            app.world_mut(),
            root,
            1,
            SandKind::EditableText,
            "My text",
            DVec2::ZERO,
        );
        let text = app
            .world()
            .get::<StoredSand>(sand)
            .unwrap()
            .content
            .unwrap();
        app.update();
        app.world_mut().get_mut::<CanvasItem>(sand).unwrap().size.x = 500.0;
        let mut settings = app.world_mut().resource_mut::<ThemeSettings>();
        settings.scheme = ColorScheme::Light;
        settings.global.set(Token::Width, TokenValue::Number(400.0));
        settings
            .global
            .set(Token::CanvasPattern, TokenValue::Number(35.0));
        settings
            .global
            .set(Token::Spacing, TokenValue::Number(12.0));
        settings
            .global
            .set(Token::FontSize, TokenValue::Number(18.0));
        settings
            .kinds
            .entry(SandStyleKind::EditableText)
            .or_default()
            .set(Token::Roundness, TokenValue::Number(12.0));
        let mut own = TokenOverrides::default();
        own.set(Token::SandInk, TokenValue::Color([1, 2, 3, 255]));
        token_style::set_overrides(app.world_mut(), text, own);
        app.update();
        flush(&mut app);
        drop(app);
        let (mut app, _) = fixture(Some(path));
        app.add_plugins(TokenStylePlugin);
        app.update();
        assert_eq!(
            app.world().resource::<ThemeSettings>().scheme,
            ColorScheme::Light
        );
        let (entity, item, stored) = app
            .world_mut()
            .query::<(Entity, &CanvasItem, &StoredSand)>()
            .single(app.world())
            .unwrap();
        assert_eq!(
            app.world().resource::<ThemeSettings>().global.0[&Token::CanvasPattern],
            TokenValue::Number(35.0)
        );
        assert_eq!(
            app.world().resource::<ThemeSettings>().global.0[&Token::Spacing],
            TokenValue::Number(12.0)
        );
        assert_eq!(
            app.world().resource::<ThemeSettings>().global.0[&Token::FontSize],
            TokenValue::Number(18.0)
        );
        assert_eq!(item.size.x, 500.0);
        assert_eq!(
            token_style::resolve(app.world(), entity, Token::Roundness).0,
            TokenValue::Number(12.0)
        );
        let text = stored.content.unwrap();
        assert_eq!(
            app.world().get::<TextColor>(text).unwrap().0,
            Color::srgb_u8(1, 2, 3)
        );
        app.world_mut()
            .resource_mut::<ThemeSettings>()
            .global
            .set(Token::Width, TokenValue::Number(600.0));
        app.update();
        assert_eq!(app.world().get::<CanvasItem>(entity).unwrap().size.x, 500.0);
    }

    #[cfg_attr(test, test)]
    fn malformed_saved_state_is_reported_and_never_overwritten() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interface.json");
        for bytes in [
            b"invalid json".as_slice(),
            br#"{"active":1,"workspaces":[],"sands":[],"records":[]}"#.as_slice(),
        ] {
            std::fs::write(&path, bytes).unwrap();
            let (mut app, root) = fixture(Some(path.clone()));
            create(app.world_mut(), root);
            app.update();
            assert!(app.world().get::<Workspaces>(root).unwrap().error.is_some());
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
    }

    #[cfg_attr(test, test)]
    fn an_existing_scene_is_imported_and_new_snapshots_become_authoritative() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interface.json");
        let (mut source, root) = fixture(None);
        rename(source.world_mut(), root, "Existing workspace");
        let document = snapshot(source.world_mut(), root);
        let original = serde_json::to_vec_pretty(&document).unwrap();
        write_atomic(&path, &original).unwrap();
        let (mut app, root) = fixture(Some(path.clone()));
        assert_eq!(
            app.world().get::<Workspaces>(root).unwrap().entries[0].name,
            "Existing workspace"
        );
        rename(app.world_mut(), root, "Snapshot workspace");
        flush(&mut app);
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert_eq!(
            read_document(&path).unwrap().unwrap().workspaces[0].name,
            "Snapshot workspace"
        );
        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            read_document(&path).unwrap().unwrap().workspaces[0].name,
            "Snapshot workspace"
        );
    }

    #[cfg_attr(test, test)]
    fn failed_saves_keep_live_notes_and_recover_when_storage_returns() {
        let directory = tempfile::tempdir().unwrap();
        let parent = directory.path().join("missing");
        let path = parent.join("interface.json");
        let (mut app, root) = fixture(Some(path.clone()));
        let sand = crate::sand_store::spawn_sand(
            app.world_mut(),
            root,
            1,
            SandKind::EditableText,
            "keep this",
            DVec2::ZERO,
        );
        app.update();
        assert!(app.world().get::<Workspaces>(root).unwrap().error.is_some());
        assert!(app.world().get_entity(sand).is_ok());
        std::fs::create_dir(&parent).unwrap();
        flush(&mut app);
        app.update();
        assert!(app.world().get::<Workspaces>(root).unwrap().error.is_none());
        assert_eq!(
            read_document(&path).unwrap().unwrap().sands[0].texts[0].text,
            "keep this"
        );
    }

    #[cfg_attr(test, test)]
    fn editing_stays_in_memory_between_saves_and_quit_flushes_the_latest_scene() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interface.json");
        let (mut app, root) = fixture(Some(path.clone()));
        let before = storage::snapshots(&path.with_extension("snapshots")).unwrap();
        for step in 1..=100 {
            app.world_mut().get_mut::<CanvasView>(root).unwrap().center = DVec2::splat(step as f64);
            app.update();
        }
        assert_eq!(
            storage::snapshots(&path.with_extension("snapshots")).unwrap(),
            before
        );
        app.world_mut().write_message(AppExit::Success);
        app.update();
        assert_eq!(app.should_exit(), Some(AppExit::Success));
        assert_eq!(
            read_document(&path).unwrap().unwrap().workspaces[0].center,
            [100.0; 2]
        );
    }

    #[cfg_attr(test, test)]
    fn quit_is_cancelled_when_pending_changes_cannot_be_saved() {
        let directory = tempfile::tempdir().unwrap();
        let parent = directory.path().join("missing");
        let path = parent.join("interface.json");
        let (mut app, root) = fixture(Some(path.clone()));
        rename(app.world_mut(), root, "Keep this workspace");
        app.world_mut().write_message(AppExit::Success);
        app.update();
        assert_eq!(app.should_exit(), None);
        assert!(app.world().get::<Workspaces>(root).unwrap().error.is_some());
        std::fs::create_dir(&parent).unwrap();
        app.world_mut().write_message(AppExit::Success);
        app.update();
        assert_eq!(app.should_exit(), Some(AppExit::Success));
        assert_eq!(
            read_document(&path).unwrap().unwrap().workspaces[0].name,
            "Keep this workspace"
        );
    }

    #[cfg(unix)]
    #[cfg_attr(test, test)]
    fn saves_are_private_and_do_not_follow_a_temporary_file_symlink() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interface.json");
        write_atomic(&path, b"previous").unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let target = directory.path().join("untouched");
        std::fs::write(&target, b"untouched").unwrap();
        symlink(&target, path.with_extension("json.tmp")).unwrap();
        assert!(write_atomic(&path, b"replacement").is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"untouched");
        assert_eq!(std::fs::read(&path).unwrap(), b"previous");
    }

    #[cfg_attr(test, test)]
    fn protein_group_settings_are_saved_without_generated_areas() {
        use crate::protein_area::{Config, GroupAxis, grouping::GeneratedGroup};
        let (mut app, root) = fixture(None);
        let mut area = crate::area::InfluenceArea::new(
            crate::area::AreaShape::Square,
            DVec2::ZERO,
            DVec2::splat(800.0),
        );
        let mut config = Config::default();
        config.grouping.vertical = Some(GroupAxis::new("assignees"));
        area.protein = Some(config.clone());
        area.filter = Some(config.clone());
        area.sorting = Some(crate::area_effects::Sorting::default());
        area.immunity = crate::area_effects::Immunity::External;
        area.scale = 0.5;
        area.force_mode = crate::area_effects::ForceMode::Newtonian;
        area.depth = 27.0;
        area.target = crate::area::AttractionTarget::Point([950.0, -30.0]);
        let owner = crate::area::spawn_area(app.world_mut(), root, 1, area.clone()).unwrap();
        area.protein = None;
        let generated = crate::area::spawn_area(app.world_mut(), root, 1, area).unwrap();
        app.world_mut()
            .entity_mut(generated)
            .insert(GeneratedGroup {
                owner,
                horizontal: false,
                index: 0,
                targets: Default::default(),
            });
        let saved = snapshot(app.world_mut(), root);
        assert_eq!(saved.areas.len(), 1);
        assert_eq!(saved.areas[0].area.protein.as_ref(), Some(&config));
        let restored: Document =
            serde_json::from_value(serde_json::to_value(saved).unwrap()).unwrap();
        assert!(restored.validate());
        assert_eq!(restored.areas[0].area.protein.as_ref(), Some(&config));
        assert_eq!(restored.areas[0].area.filter.as_ref(), Some(&config));
        assert_eq!(
            restored.areas[0].area.sorting,
            Some(crate::area_effects::Sorting::default())
        );
        assert_eq!(
            restored.areas[0].area.immunity,
            crate::area_effects::Immunity::External
        );
        assert_eq!(restored.areas[0].area.scale, 0.5);
        assert_eq!(restored.areas[0].area.depth, 27.0);
        assert_eq!(
            restored.areas[0].area.target,
            crate::area::AttractionTarget::Point([950.0, -30.0])
        );
        assert_eq!(
            restored.areas[0].area.force_mode,
            crate::area_effects::ForceMode::Newtonian
        );
    }

    crate::laboratory_cases! {
        instinct_seeds_once_and_saved_or_deleted_readers_stay_that_way,
        protein_group_settings_are_saved_without_generated_areas,
        sand_groups_survive_workspace_restart_and_saved_record_regrouping,
        areas_restore_identity_shape_properties_and_workspace_and_reject_invalid_snapshots,
        resized_bounds_survive_restart_without_changing_text_areas,
        protein_castles_restore_workspace_geometry_and_unfinished_queries,
        switching_restores_each_camera_and_keeps_live_drafts,
        removing_workspace_moves_sands_without_deleting_record_drafts_or_other_boxes,
        restart_restores_notes_workspaces_cameras_and_record_layouts_without_idle_writes,
        token_layers_and_dragged_sizes_survive_restart,
        malformed_saved_state_is_reported_and_never_overwritten,
        an_existing_scene_is_imported_and_new_snapshots_become_authoritative,
        failed_saves_keep_live_notes_and_recover_when_storage_returns,
        editing_stays_in_memory_between_saves_and_quit_flushes_the_latest_scene,
        quit_is_cancelled_when_pending_changes_cannot_be_saved,
        #[cfg(unix)]
        saves_are_private_and_do_not_follow_a_temporary_file_symlink,
    }
}
