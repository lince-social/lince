mod model;
pub(crate) mod tests;
mod ui;

use crate::{
    cell_bridge::{CellBridge, CellMessage, ReceiveCell},
    workspace::WorkspaceMember,
};
use bevy::{math::DVec2, prelude::*};
use cell::{ClientMessage, ServerMessage};
pub use model::ProteinDraft;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap, VecDeque};

#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct ProteinCastle {
    pub draft: ProteinDraft,
}

pub(crate) fn refresh_editor(world: &mut World, entity: Entity) {
    ui::editor(world, entity);
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SavedProteinCastle {
    pub workspace: u64,
    draft: ProteinDraft,
    position: [f64; 2],
    size: [f32; 2],
    placement: crate::sand_placement::Placement,
    tokens: crate::tokens::TokenOverrides,
}

impl SavedProteinCastle {
    pub(crate) fn valid(&self) -> bool {
        self.draft.valid_storage()
            && DVec2::from_array(self.position).is_finite()
            && Vec2::from_array(self.size).is_finite()
            && Vec2::from_array(self.size).min_element() > 0.0
            && self.placement.valid()
            && self.tokens.validate()
    }

    pub(crate) fn restore(self, world: &mut World, root: Entity) -> Entity {
        let entity = spawn(
            world,
            root,
            self.workspace,
            DVec2::from_array(self.position),
            self.draft,
        );
        world
            .get_mut::<crate::canvas::CanvasItem>(entity)
            .unwrap()
            .size = Vec2::from_array(self.size);
        self.placement.restore(world, entity);
        world.entity_mut(entity).insert((
            self.tokens,
            crate::token_style::AppliedSize(Vec2::from_array(self.size)),
        ));
        entity
    }
}

pub(crate) fn snapshot(world: &mut World, root: Entity) -> Vec<SavedProteinCastle> {
    world
        .query::<(
            Entity,
            &ChildOf,
            &WorkspaceMember,
            &crate::canvas::CanvasItem,
            &ProteinCastle,
        )>()
        .iter(world)
        .filter(|(entity, parent, _, _, _)| {
            parent.parent() == root
                && world
                    .get::<crate::protein_area::QueryEditor>(*entity)
                    .is_none()
        })
        .map(|(entity, _, member, item, castle)| SavedProteinCastle {
            workspace: member.0,
            draft: castle.draft.clone(),
            position: item.position.to_array(),
            size: item.size.to_array(),
            placement: crate::sand_placement::Placement::capture(world, entity),
            tokens: crate::token_style::overrides(world, entity),
        })
        .collect()
}

#[derive(Component, Default)]
pub struct ProteinResults {
    pub query: Option<protein::Protein>,
    pub rows: Vec<Value>,
    pub columns: Vec<String>,
    pub revision: u64,
    pub current: bool,
    pub error: Option<String>,
}

#[derive(Component)]
struct View {
    editor: Entity,
    output: Entity,
    status: String,
    page: usize,
    library: Vec<Value>,
    library_open: bool,
    saving: bool,
    subscription: Option<String>,
    output_dirty: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RequestKind {
    Results,
    Library,
    Save,
}

#[derive(Resource, Default)]
struct Requests {
    next: u64,
    owners: HashMap<String, (Entity, RequestKind)>,
    outgoing: VecDeque<ClientMessage>,
}

pub struct ProteinCastlePlugin;

impl Plugin for ProteinCastlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Requests>()
            .add_message::<CellMessage>()
            .add_systems(Update, (receive.after(ReceiveCell), maintain).chain())
            .add_systems(
                PostUpdate,
                ui::inputs
                    .after(bevy::text::EditableTextSystems)
                    .before(crate::actions::ApplyActions),
            );
    }
}

pub fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    draft: ProteinDraft,
) -> Entity {
    world.init_resource::<Requests>();
    let entity = world
        .spawn((
            crate::castle::Castle,
            crate::sand::Square,
            crate::sand::InBox(root),
            ChildOf(root),
            WorkspaceMember(workspace),
            crate::sand_store::SandCredits(crate::credits::ATTRIBUTIONS),
            crate::canvas::CanvasItem {
                position,
                size: Vec2::new(720.0, 680.0),
            },
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(12)),
                row_gap: px(8),
                overflow: Overflow::clip(),
                ..default()
            },
            crate::token_style::background(crate::tokens::Token::Surface),
            ProteinCastle { draft },
            ProteinResults::default(),
        ))
        .id();
    let header = ui::row(world, entity);
    crate::edit_mode::label(world, header, "Protein", 22.0);
    ui::icon(
        world,
        header,
        entity,
        ui::Command::Run,
        crate::icons::Icon::Play,
        "Run this query and keep results live",
    );
    ui::icon(
        world,
        header,
        entity,
        ui::Command::Stop,
        crate::icons::Icon::Stop,
        "Stop receiving results",
    );
    ui::icon(
        world,
        header,
        entity,
        ui::Command::Library,
        crate::icons::Icon::Store,
        "Open saved Proteins",
    );
    ui::icon(
        world,
        header,
        entity,
        ui::Command::Save,
        crate::icons::Icon::Save,
        "Save this query as a named Protein. An existing slug updates that Protein.",
    );
    ui::icon(
        world,
        header,
        entity,
        ui::Command::Delete,
        crate::icons::Icon::Close,
        "Remove this Castle from the workspace",
    );
    let body = world
        .spawn((
            Node {
                flex_grow: 1.0,
                min_height: px(0),
                column_gap: px(16),
                ..default()
            },
            ChildOf(entity),
        ))
        .id();
    let editor = ui::scroll(world, body, 55.0);
    let output = ui::scroll(world, body, 45.0);
    world.entity_mut(entity).insert(View {
        editor,
        output,
        status: "Draft".into(),
        page: 0,
        library: Vec::new(),
        library_open: false,
        saving: false,
        subscription: None,
        output_dirty: true,
    });
    ui::editor(world, entity);
    entity
}

pub(crate) fn store_entries(world: &mut World, root: Entity, parent: Entity) {
    crate::sand_store::castle_entry(
        world,
        root,
        parent,
        "Protein Castle",
        "Build a query and browse live results.",
        ui::Command::Create,
        |world, root| spawn(world, root, 1, DVec2::ZERO, ProteinDraft::default()),
    );
}

fn request(
    world: &mut World,
    owner: Entity,
    kind: RequestKind,
    message: impl FnOnce(String) -> ClientMessage,
) -> String {
    let mut requests = world.resource_mut::<Requests>();
    requests.next += 1;
    let id = format!("interface-protein-{}", requests.next);
    requests.owners.insert(id.clone(), (owner, kind));
    requests.outgoing.push_back(message(id.clone()));
    id
}

fn cancel(world: &mut World, owner: Entity, kind: Option<RequestKind>) {
    let mut requests = world.resource_mut::<Requests>();
    let ids: Vec<_> = requests
        .owners
        .iter()
        .filter(|(_, (entity, k))| *entity == owner && kind.is_none_or(|kind| kind == *k))
        .map(|(id, (_, kind))| (id.clone(), *kind))
        .collect();
    for (id, kind) in ids {
        requests.owners.remove(&id);
        requests.outgoing.retain(|message| match message {
            ClientMessage::Subscribe { id: pending, .. }
            | ClientMessage::Act { id: pending, .. } => pending != &id,
            _ => true,
        });
        if kind != RequestKind::Save {
            requests
                .outgoing
                .push_back(ClientMessage::Unsubscribe { id });
        }
    }
}

pub(crate) fn remove(world: &mut World, owner: Entity) {
    cancel(world, owner, None);
    world.despawn(owner);
}

pub(crate) fn status(world: &mut World, owner: Entity, message: impl Into<String>) {
    if let Some(mut view) = world.get_mut::<View>(owner) {
        view.status = message.into();
        view.output_dirty = true;
    }
}

fn run(world: &mut World, owner: Entity) {
    if crate::protein_area::run_editor(world, owner) {
        return;
    }
    let Some(castle) = world.get::<ProteinCastle>(owner) else {
        return;
    };
    let query = match castle.draft.compile() {
        Ok(query) => query,
        Err(error) => {
            status(world, owner, error);
            return;
        }
    };
    if world.get_non_send::<CellBridge>().is_none() {
        status(world, owner, "No Cell connection");
        return;
    }
    cancel(world, owner, Some(RequestKind::Results));
    world.get_mut::<ProteinResults>(owner).unwrap().current = false;
    world.get_mut::<ProteinResults>(owner).unwrap().query = Some(query.clone());
    let id = request(world, owner, RequestKind::Results, |id| {
        ClientMessage::Subscribe { id, protein: query }
    });
    world.get_mut::<View>(owner).unwrap().subscription = Some(id);
    status(world, owner, "Loading…");
}

fn library(world: &mut World, owner: Entity) {
    if world.get::<View>(owner).is_none() {
        return;
    }
    if world.get::<View>(owner).unwrap().library_open {
        world.get_mut::<View>(owner).unwrap().library_open = false;
        cancel(world, owner, Some(RequestKind::Library));
        ui::editor(world, owner);
        return;
    }
    if world.get_non_send::<CellBridge>().is_none() {
        status(world, owner, "No Cell connection");
        return;
    }
    world.get_mut::<View>(owner).unwrap().library_open = true;
    let protein = serde_json::from_value(serde_json::json!({"source":"record", "where":[{"kind_eq":"protein"},{"quantity_gt":"0"}], "fields":["uid","slug","head","extension"], "include":{"extension":{"namespace":"lince.protein"}}, "order":[{"asc":"head"}]})).unwrap();
    request(world, owner, RequestKind::Library, |id| {
        ClientMessage::Subscribe { id, protein }
    });
    ui::editor(world, owner);
    status(world, owner, "Loading saved Proteins…");
}

fn save(world: &mut World, owner: Entity) {
    let Some(castle) = world.get::<ProteinCastle>(owner) else {
        return;
    };
    if world.get::<View>(owner).is_some_and(|view| view.saving) {
        return;
    }
    let draft = castle.draft.clone();
    let query = match draft.compile() {
        Ok(query) => query,
        Err(error) => {
            status(world, owner, error);
            return;
        }
    };
    if draft.name.trim().is_empty() || draft.slug.trim().is_empty() {
        status(world, owner, "Name and slug are required to save a Protein");
        return;
    }
    if world.get_non_send::<CellBridge>().is_none() {
        status(world, owner, "No Cell connection");
        return;
    }
    request(world, owner, RequestKind::Save, |id| ClientMessage::Act {
        id,
        action: engine::actions::Action::SaveProtein {
            slug: draft.slug.trim().into(),
            head: draft.name.trim().into(),
            ast: serde_json::to_value(query).unwrap(),
        },
    });
    world.get_mut::<View>(owner).unwrap().saving = true;
    status(world, owner, "Saving…");
}

fn receive(world: &mut World, mut cursor: Local<bevy::ecs::message::MessageCursor<CellMessage>>) {
    let messages: Vec<_> = cursor
        .read(world.resource::<Messages<CellMessage>>())
        .map(|m| m.0.clone())
        .collect();
    for message in messages {
        receive_message(world, message);
    }
}

fn receive_message(world: &mut World, message: ServerMessage) {
    let id = match &message {
        ServerMessage::Snapshot { id, .. }
        | ServerMessage::Update { id, .. }
        | ServerMessage::ActionOk { id, .. }
        | ServerMessage::Error { id, .. } => id,
        _ => return,
    };
    if id == crate::cell_bridge::CONNECTION {
        disconnected(world);
        return;
    }
    let Some((owner, kind)) = world.resource::<Requests>().owners.get(id).copied() else {
        return;
    };
    if world.get::<View>(owner).is_none() {
        return;
    }
    match message {
        ServerMessage::Snapshot { rows, .. } | ServerMessage::Update { rows, .. } => match kind {
            RequestKind::Results => {
                let columns: BTreeSet<_> = rows
                    .iter()
                    .filter_map(Value::as_object)
                    .flat_map(|row| row.keys().cloned())
                    .collect();
                let draft = world
                    .get::<ProteinCastle>(owner)
                    .unwrap()
                    .draft
                    .compile()
                    .ok()
                    .map(|query| serde_json::to_value(query).unwrap());
                let mut result = world.get_mut::<ProteinResults>(owner).unwrap();
                result.current = result
                    .query
                    .as_ref()
                    .is_some_and(|query| Some(serde_json::to_value(query).unwrap()) == draft);
                result.rows = rows;
                result.columns = columns.into_iter().collect();
                result.revision += 1;
                result.error = None;
                let current = result.current;
                let saved = world
                    .get::<View>(owner)
                    .filter(|view| {
                        view.saving || view.status == "Saved" || view.status.starts_with("Saved · ")
                    })
                    .map(|view| view.status.clone());
                status(
                    world,
                    owner,
                    if current {
                        saved.as_deref().unwrap_or("Live")
                    } else {
                        "Draft changed · showing previous query"
                    },
                );
            }
            RequestKind::Library => {
                world.get_mut::<View>(owner).unwrap().library = rows;
                ui::editor(world, owner);
                status(world, owner, "Saved Proteins");
            }
            RequestKind::Save => {}
        },
        ServerMessage::ActionOk { id, warnings, .. } if kind == RequestKind::Save => {
            world.resource_mut::<Requests>().owners.remove(&id);
            world.get_mut::<View>(owner).unwrap().saving = false;
            status(
                world,
                owner,
                if warnings.is_empty() {
                    "Saved".into()
                } else {
                    format!("Saved · {}", warnings.join("; "))
                },
            );
        }
        ServerMessage::Error { id, message, .. } => {
            if kind == RequestKind::Save {
                world.resource_mut::<Requests>().owners.remove(&id);
                world.get_mut::<View>(owner).unwrap().saving = false;
            }
            if kind == RequestKind::Results {
                let mut results = world.get_mut::<ProteinResults>(owner).unwrap();
                results.current = false;
                results.error = Some(message.clone());
            }
            status(world, owner, message);
        }
        _ => {}
    }
}

fn disconnected(world: &mut World) {
    let owners: Vec<_> = world
        .resource::<Requests>()
        .owners
        .values()
        .map(|(owner, _)| *owner)
        .collect();
    world.resource_mut::<Requests>().owners.clear();
    world.resource_mut::<Requests>().outgoing.clear();
    for owner in owners {
        if let Some(mut view) = world.get_mut::<View>(owner) {
            view.saving = false;
            view.subscription = None;
        }
        if let Some(mut result) = world.get_mut::<ProteinResults>(owner) {
            result.current = false;
            result.error = Some("Connection closed".into());
        }
        status(world, owner, "Connection closed · results may be stale");
    }
}

fn maintain(world: &mut World) {
    let stale: Vec<_> = world
        .resource::<Requests>()
        .owners
        .values()
        .filter(|(owner, _)| world.get::<ProteinCastle>(*owner).is_none())
        .map(|(owner, _)| *owner)
        .collect();
    for owner in stale {
        cancel(world, owner, None);
    }
    if !crate::laboratory::active(world) {
        while let Some(message) = world.resource_mut::<Requests>().outgoing.pop_front() {
            let Some(bridge) = world.get_non_send::<CellBridge>() else {
                disconnected(world);
                break;
            };
            match bridge.outgoing.try_send(message) {
                Ok(()) => {}
                Err(tokio::sync::mpsc::error::TrySendError::Full(message)) => {
                    world
                        .resource_mut::<Requests>()
                        .outgoing
                        .push_front(message);
                    if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
                        wake.ring();
                    }
                    break;
                }
                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                    disconnected(world);
                    break;
                }
            }
        }
    }
    let dirty: Vec<_> = world
        .query::<(Entity, &View)>()
        .iter(world)
        .filter(|(_, view)| view.output_dirty)
        .map(|(entity, _)| entity)
        .collect();
    for entity in dirty {
        ui::output(world, entity);
    }
}
