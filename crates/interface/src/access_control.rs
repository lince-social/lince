mod ui;

use crate::cell_bridge::{CellBridge, CellMessage, ReceiveCell};
use bevy::prelude::*;
use cell::{ClientMessage, ServerMessage};
use serde_json::Value;

#[derive(Resource, Default)]
struct Catalog {
    rows: Vec<Value>,
    subscription: Option<String>,
    refresh: bool,
    ready: bool,
    next: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Users,
    Roles,
}

#[derive(Clone, Copy)]
enum Mutation {
    CreateUser,
    SaveUser,
    DeleteUser,
    Role,
    RenameRole,
    DeleteRole,
    Permission,
    Assign,
}

#[derive(Component)]
pub struct AccessControlSand {
    list: Entity,
    editor: Entity,
    status: Entity,
    tab: Tab,
    page: usize,
    selected: Option<String>,
    pending: Option<(String, Mutation)>,
    dirty: bool,
    reload_editor: bool,
}

pub struct AccessControlPlugin;

impl Plugin for AccessControlPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Catalog>()
            .add_message::<CellMessage>()
            .add_systems(Update, (receive.after(ReceiveCell), maintain).chain())
            .add_systems(
                PostUpdate,
                ui::protect_passwords.before(bevy::text::EditableTextSystems),
            )
            .add_systems(PostUpdate, ui::masks.after(bevy::text::EditableTextSystems));
    }
}

pub(crate) fn populate(world: &mut World, _root: Entity, sand: Entity) -> Entity {
    world.init_resource::<Catalog>();
    ui::populate(world, sand);
    sand
}

fn status(world: &mut World, sand: Entity, message: &str) {
    if let Some(entity) = world.get::<AccessControlSand>(sand).map(|view| view.status)
        && let Some(mut text) = world.get_mut::<Text>(entity)
    {
        text.set_if_neq(Text::new(message));
    }
}

fn send(world: &World, message: ClientMessage) -> Result<(), &'static str> {
    if crate::laboratory::active(world) {
        return Err("Access changes are unavailable in the Laboratory.");
    }
    let bridge = world
        .get_non_send::<CellBridge>()
        .ok_or("Not connected to the local Organ.")?;
    bridge
        .outgoing
        .try_send(message)
        .map_err(|error| match error {
            tokio::sync::mpsc::error::TrySendError::Full(_) => "Connection busy. Try again.",
            tokio::sync::mpsc::error::TrySendError::Closed(_) => {
                "Connection closed. Reopen after reconnecting."
            }
        })
}

fn request(world: &mut World, sand: Entity, action: engine::actions::Action, kind: Mutation) {
    if world
        .get::<AccessControlSand>(sand)
        .is_none_or(|view| view.pending.is_some())
    {
        return;
    }
    let id = next_id(world);
    match send(
        world,
        ClientMessage::Act {
            id: id.clone(),
            action,
        },
    ) {
        Ok(()) => {
            world.get_mut::<AccessControlSand>(sand).unwrap().pending = Some((id, kind));
            ui::clear_password(world, sand);
            status(world, sand, "Saving…");
        }
        Err(error) => status(world, sand, error),
    }
}

fn next_id(world: &mut World) -> String {
    let mut catalog = world.resource_mut::<Catalog>();
    catalog.next += 1;
    format!("interface-access-{}", catalog.next)
}

fn auth_query() -> protein::Protein {
    protein::Protein {
        source: protein::Source::Auth,
        filter: Vec::new(),
        fields: None,
        include: Default::default(),
        aggregate: None,
        order: Vec::new(),
        limit: None,
    }
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
        let owners: Vec<_> = world
            .query_filtered::<Entity, With<AccessControlSand>>()
            .iter(world)
            .collect();
        for owner in owners {
            world.get_mut::<AccessControlSand>(owner).unwrap().pending = None;
            ui::clear_password(world, owner);
            status(
                world,
                owner,
                "Connection lost. A pending change may have completed; refresh before retrying.",
            );
        }
        let mut catalog = world.resource_mut::<Catalog>();
        catalog.ready = false;
        catalog.refresh = false;
        return;
    }
    if world.resource::<Catalog>().subscription.as_ref() == Some(id) {
        match message {
            ServerMessage::Snapshot { rows, .. } | ServerMessage::Update { rows, .. } => {
                let mut catalog = world.resource_mut::<Catalog>();
                let changed = !catalog.ready || catalog.rows != rows;
                catalog.rows = rows;
                catalog.ready = true;
                if changed {
                    for mut view in world.query::<&mut AccessControlSand>().iter_mut(world) {
                        view.dirty = true;
                    }
                }
            }
            ServerMessage::Error { message, .. } => {
                world.resource_mut::<Catalog>().ready = false;
                let owners: Vec<_> = world
                    .query_filtered::<Entity, With<AccessControlSand>>()
                    .iter(world)
                    .collect();
                for owner in owners {
                    status(world, owner, &message);
                }
            }
            _ => {}
        }
        return;
    }
    let owner = world
        .query::<(Entity, &AccessControlSand)>()
        .iter(world)
        .find(|(_, view)| {
            view.pending
                .as_ref()
                .is_some_and(|(pending, _)| pending == id)
        })
        .map(|(entity, view)| (entity, view.pending.as_ref().unwrap().1));
    let Some((owner, kind)) = owner else {
        return;
    };
    match message {
        ServerMessage::ActionOk {
            created, warnings, ..
        } => {
            world.get_mut::<AccessControlSand>(owner).unwrap().pending = None;
            if matches!(kind, Mutation::CreateUser) {
                if let Some(mut form) = world.get_mut::<ui::UserForm>(owner) {
                    form.uid = created.clone();
                }
                let mut view = world.get_mut::<AccessControlSand>(owner).unwrap();
                view.selected = created;
                view.reload_editor = true;
            } else if matches!(kind, Mutation::Role) {
                let mut view = world.get_mut::<AccessControlSand>(owner).unwrap();
                view.selected = created;
                view.reload_editor = true;
            } else if matches!(kind, Mutation::RenameRole) {
                world
                    .get_mut::<AccessControlSand>(owner)
                    .unwrap()
                    .reload_editor = true;
            } else if matches!(kind, Mutation::DeleteUser | Mutation::DeleteRole) {
                world.get_mut::<AccessControlSand>(owner).unwrap().selected = None;
                ui::editor(world, owner);
            }
            world.resource_mut::<Catalog>().refresh = true;
            world.resource_mut::<Catalog>().ready = false;
            let message = if warnings.is_empty() {
                "Saved.".into()
            } else {
                format!("Saved. {}", warnings.join(" · "))
            };
            status(world, owner, &message);
        }
        ServerMessage::Error { message, .. } => {
            world.get_mut::<AccessControlSand>(owner).unwrap().pending = None;
            status(world, owner, &message);
        }
        _ => {}
    }
}

fn maintain(world: &mut World) {
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<AccessControlSand>>()
        .iter(world)
        .collect();
    if owners.is_empty() {
        if let Some(id) = world.resource::<Catalog>().subscription.clone()
            && send(world, ClientMessage::Unsubscribe { id }).is_ok()
        {
            let mut catalog = world.resource_mut::<Catalog>();
            catalog.subscription = None;
            catalog.rows.clear();
            catalog.ready = false;
        }
        return;
    }
    let needs_query = {
        let catalog = world.resource::<Catalog>();
        catalog.subscription.is_none() || catalog.refresh
    };
    if needs_query && !crate::laboratory::active(world) {
        let id = world
            .resource::<Catalog>()
            .subscription
            .clone()
            .unwrap_or_else(|| next_id(world));
        match send(
            world,
            ClientMessage::Subscribe {
                id: id.clone(),
                protein: auth_query(),
            },
        ) {
            Ok(()) => {
                let mut catalog = world.resource_mut::<Catalog>();
                catalog.subscription = Some(id);
                catalog.refresh = false;
            }
            Err(error) => {
                for owner in &owners {
                    status(world, *owner, error);
                }
            }
        }
    }
    for owner in owners {
        if world.resource::<Catalog>().ready {
            let label = world.get::<AccessControlSand>(owner).unwrap().status;
            if world
                .get::<Text>(label)
                .is_some_and(|text| text.0 == "Loading users and Roles…" || text.0 == "Refreshing…")
            {
                status(world, owner, "Select an entry or create one.");
            }
        }
        if world.get::<AccessControlSand>(owner).unwrap().dirty {
            ui::list(world, owner);
            world.get_mut::<AccessControlSand>(owner).unwrap().dirty = false;
        }
        if world.resource::<Catalog>().ready
            && world.get::<AccessControlSand>(owner).unwrap().reload_editor
        {
            ui::editor(world, owner);
            world
                .get_mut::<AccessControlSand>(owner)
                .unwrap()
                .reload_editor = false;
        }
    }
}
