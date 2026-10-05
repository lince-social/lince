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

#[derive(Component, Default)]
struct ScopedCatalog(Catalog);

#[derive(Resource, Default)]
struct ScopedSubscriptions(
    std::collections::HashMap<Entity, (String, tokio::sync::mpsc::Sender<ClientMessage>)>,
);

fn catalog(world: &World, owner: Entity) -> &Catalog {
    world
        .get::<ScopedCatalog>(owner)
        .map_or_else(|| world.resource::<Catalog>(), |catalog| &catalog.0)
}

fn refresh_catalog(world: &mut World, owner: Entity) {
    if let Some(mut catalog) = world.get_mut::<ScopedCatalog>(owner) {
        catalog.0.refresh = true;
        catalog.0.ready = false;
    } else {
        world.resource_mut::<Catalog>().refresh = true;
        world.resource_mut::<Catalog>().ready = false;
    }
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
    Standing,
    Role,
    RenameRole,
    DeleteRole,
    Permission,
    Policy,
    Assign,
    Preview,
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
    last_preview: Option<Value>,
}

pub struct AccessControlPlugin;

impl Plugin for AccessControlPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Catalog>()
            .init_resource::<ScopedSubscriptions>()
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
    if crate::practice_cells::source(world, sand).is_some() {
        world.entity_mut(sand).insert(ScopedCatalog::default());
    }
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
    match crate::practice_cells::send(
        world,
        sand,
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
        Err(error) => status(world, sand, &error),
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
    let scoped = world
        .query::<(Entity, &ScopedCatalog)>()
        .iter(world)
        .find(|(_, catalog)| catalog.0.subscription.as_ref() == Some(id))
        .map(|(owner, _)| owner);
    if let Some(owner) = scoped {
        match message {
            ServerMessage::Snapshot { rows, .. } | ServerMessage::Update { rows, .. } => {
                let mut catalog = world.get_mut::<ScopedCatalog>(owner).unwrap();
                let changed = !catalog.0.ready || catalog.0.rows != rows;
                catalog.0.rows = rows;
                catalog.0.ready = true;
                if changed {
                    world.get_mut::<AccessControlSand>(owner).unwrap().dirty = true;
                }
            }
            ServerMessage::Error { message, .. } => {
                world.get_mut::<ScopedCatalog>(owner).unwrap().0.ready = false;
                status(world, owner, &message);
            }
            _ => {}
        }
        return;
    }
    if id == crate::cell_bridge::CONNECTION {
        let owners: Vec<_> = world
            .query_filtered::<Entity, (With<AccessControlSand>, Without<ScopedCatalog>)>()
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
            created,
            warnings,
            data,
            ..
        } => {
            world.get_mut::<AccessControlSand>(owner).unwrap().pending = None;
            if matches!(kind, Mutation::Preview) {
                world
                    .get_mut::<AccessControlSand>(owner)
                    .unwrap()
                    .last_preview = data.clone();
                status(
                    world,
                    owner,
                    &data
                        .and_then(|data| serde_json::to_string_pretty(&data).ok())
                        .unwrap_or_else(|| "Authority preview unavailable".into()),
                );
                return;
            }
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
            } else if matches!(
                kind,
                Mutation::RenameRole | Mutation::Policy | Mutation::Assign
            ) {
                world
                    .get_mut::<AccessControlSand>(owner)
                    .unwrap()
                    .reload_editor = true;
            } else if matches!(kind, Mutation::DeleteRole) {
                world.get_mut::<AccessControlSand>(owner).unwrap().selected = None;
                ui::editor(world, owner);
            }
            refresh_catalog(world, owner);
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
    let scoped: Vec<_> = world
        .query_filtered::<Entity, (With<AccessControlSand>, Without<ScopedCatalog>)>()
        .iter(world)
        .filter(|owner| crate::practice_cells::source(world, *owner).is_some())
        .collect();
    for owner in scoped {
        world.entity_mut(owner).insert(ScopedCatalog::default());
    }
    maintain_scoped(world);
    let owners: Vec<_> = world
        .query_filtered::<Entity, (With<AccessControlSand>, Without<ScopedCatalog>)>()
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
        ui::policy_controls(world, owner);
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

fn maintain_scoped(world: &mut World) {
    let stale: Vec<_> = world
        .resource::<ScopedSubscriptions>()
        .0
        .keys()
        .copied()
        .filter(|owner| world.get::<ScopedCatalog>(*owner).is_none())
        .collect();
    for owner in stale {
        if let Some((id, sender)) = world.resource_mut::<ScopedSubscriptions>().0.remove(&owner) {
            let _ = sender.try_send(ClientMessage::Unsubscribe { id });
        }
    }
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<ScopedCatalog>>()
        .iter(world)
        .collect();
    for owner in owners {
        let state = &world.get::<ScopedCatalog>(owner).unwrap().0;
        if state.subscription.is_none() || state.refresh {
            let id = state
                .subscription
                .clone()
                .unwrap_or_else(|| nucleus::new_uid("practice-access"));
            if let Some(sender) = crate::practice_cells::sender(world, owner)
                && sender
                    .try_send(ClientMessage::Subscribe {
                        id: id.clone(),
                        protein: auth_query(),
                    })
                    .is_ok()
            {
                world
                    .resource_mut::<ScopedSubscriptions>()
                    .0
                    .insert(owner, (id.clone(), sender));
                let mut state = world.get_mut::<ScopedCatalog>(owner).unwrap();
                state.0.subscription = Some(id);
                state.0.refresh = false;
            }
        }
        ui::policy_controls(world, owner);
        if world.get::<AccessControlSand>(owner).unwrap().dirty {
            ui::list(world, owner);
            world.get_mut::<AccessControlSand>(owner).unwrap().dirty = false;
        }
        if catalog(world, owner).ready
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

pub(crate) fn inspect_user(world: &mut World, owner: Entity, uid: &str) -> bool {
    if !catalog(world, owner).ready
        || !catalog(world, owner)
            .rows
            .iter()
            .any(|row| row["kind"] == "user" && row["id"] == uid)
    {
        return false;
    }
    world.get_mut::<AccessControlSand>(owner).unwrap().selected = Some(uid.into());
    ui::editor(world, owner);
    true
}

pub(crate) fn user_visible(world: &World, owner: Entity, uid: &str) -> bool {
    catalog(world, owner).ready
        && world
            .get::<AccessControlSand>(owner)
            .is_some_and(|view| view.selected.as_deref() == Some(uid))
}

pub(crate) fn preview_authority(world: &mut World, owner: Entity, record: &str) -> bool {
    let Some(form) = world.get::<ui::UserForm>(owner) else {
        return false;
    };
    let field = form.preview_record;
    world
        .get_mut::<bevy::text::EditableText>(field)
        .unwrap()
        .editor
        .set_text(record);
    crate::actions::Action::apply(&ui::Command::Preview, world, owner);
    true
}

pub(crate) fn authority_visible(world: &World, owner: Entity) -> bool {
    world.get::<AccessControlSand>(owner).is_some_and(|view| {
        view.pending.is_none()
            && view.last_preview.as_ref().is_some_and(|preview| {
                preview["read"] == true
                    && preview["update_properties"]
                        .as_array()
                        .is_some_and(Vec::is_empty)
            })
    })
}
