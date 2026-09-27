#[cfg(test)]
mod tests;
mod ui;

use super::CustomCastle;
use crate::{actions::Action, cell_bridge::CellMessage, sand_panel as panel};
use bevy::prelude::*;
use cell::{ClientMessage, ServerMessage};
use engine::actions::Action as Backend;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
pub(super) use ui::show;

use engine::custom_component::{FORMAT, MAX_BYTES};

#[derive(Serialize, Deserialize)]
struct Document {
    format: String,
    castle: CustomCastle,
}

fn encode(castle: CustomCastle) -> Result<String, String> {
    if !castle.valid() {
        return Err("Invalid custom component".into());
    }
    let body = serde_json::to_string(&Document {
        format: FORMAT.into(),
        castle,
    })
    .map_err(|error| error.to_string())?;
    if body.len() > MAX_BYTES {
        return Err("An Organ component can contain at most 256 KiB".into());
    }
    Ok(body)
}

fn decode(row: &Value) -> Result<CustomCastle, String> {
    let body = row["body"]
        .as_str()
        .ok_or("Component contents are unavailable")?;
    if body.len() > MAX_BYTES || row["kind"] != "sand" {
        return Err("Invalid custom component".into());
    }
    let mut document: Document =
        serde_json::from_str(body).map_err(|_| "Invalid custom component")?;
    document.castle.name = row["head"].as_str().unwrap_or_default().into();
    if document.format != FORMAT || !document.castle.valid() {
        return Err("Invalid custom component".into());
    }
    Ok(document.castle)
}

fn query(target: Option<&str>) -> protein::Protein {
    serde_json::from_value(match target {
        Some(uid) => json!({"source":"record", "where":[{"uid_eq":uid},{"kind_eq":"sand"}], "fields":["uid","head","body","kind"], "limit":1}),
        None => json!({"source":"record", "where":[{"kind_eq":"sand"},{"text_contains":FORMAT}], "fields":["uid","head"], "order":[{"asc":"head"}], "limit":null}),
    }).unwrap()
}

#[derive(Component)]
struct Library {
    organ: Option<String>,
    remote: Option<crate::protein_area::Remote>,
    connected: bool,
    login: bool,
    subscribed: bool,
    contacts_requested: bool,
    contacts_id: String,
    list_id: String,
    detail_id: Option<String>,
    contacts: Vec<Value>,
    entries: Vec<Value>,
    selected: Option<String>,
    castle: Option<CustomCastle>,
    name: String,
    pending: Option<String>,
    deleting: bool,
    status: String,
    dirty: bool,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            organ: None,
            remote: None,
            connected: true,
            login: false,
            subscribed: false,
            contacts_requested: false,
            contacts_id: nucleus::new_uid("component-organs"),
            list_id: nucleus::new_uid("components"),
            detail_id: None,
            contacts: Vec::new(),
            entries: Vec::new(),
            selected: None,
            castle: None,
            name: String::new(),
            pending: None,
            deleting: false,
            status: "Loading components…".into(),
            dirty: true,
        }
    }
}

pub(crate) struct LibraryPlugin;
impl Plugin for LibraryPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CellMessage>()
            .add_systems(Update, update.after(crate::cell_bridge::ReceiveCell));
    }
}

#[derive(Clone)]
enum Command {
    Organ(Option<String>),
    Select(String),
    Refresh,
    Save(Entity),
    Replace(Entity),
    Rename(Entity),
    Delete,
    Add,
    Login(Entity, Entity),
}

fn send(world: &World, root: Entity, message: ClientMessage) -> Result<(), String> {
    if crate::laboratory::active(world) {
        return Err("Changes are unavailable in the Laboratory".into());
    }
    let library = world
        .get::<Library>(root)
        .ok_or("Component library is closed")?;
    if library.organ.is_some() {
        library
            .remote
            .as_ref()
            .ok_or("Connect to the Organ first")?
            .outgoing
            .try_send(message)
            .map_err(|error| match error {
                tokio::sync::mpsc::error::TrySendError::Full(_) => {
                    "Connection busy; try again".into()
                }
                tokio::sync::mpsc::error::TrySendError::Closed(_) => {
                    "Connection closed; refresh to reconnect".into()
                }
            })
    } else {
        panel::send(world, message)
    }
}

fn select(world: &mut World, root: Entity, uid: String) -> Result<(), String> {
    let id = nucleus::new_uid("component-detail");
    send(
        world,
        root,
        ClientMessage::Subscribe {
            id: id.clone(),
            protein: query(Some(&uid)),
        },
    )?;
    let previous = world
        .get_mut::<Library>(root)
        .unwrap()
        .detail_id
        .replace(id);
    if let Some(id) = previous {
        let _ = send(world, root, ClientMessage::Unsubscribe { id });
    }
    let mut library = world.get_mut::<Library>(root).unwrap();
    library.selected = Some(uid);
    library.castle = None;
    library.status = "Loading component…".into();
    library.dirty = true;
    Ok(())
}

fn connect(world: &mut World, root: Entity, organ: Option<String>) -> Result<(), String> {
    let library = world.get::<Library>(root).unwrap();
    for id in std::iter::once(&library.list_id).chain(library.detail_id.iter()) {
        let _ = send(world, root, ClientMessage::Unsubscribe { id: id.clone() });
    }
    let mut library = world.get_mut::<Library>(root).unwrap();
    library.remote = None;
    library.organ = organ.clone();
    library.connected = organ.is_none();
    library.login = false;
    library.subscribed = false;
    library.list_id = nucleus::new_uid("components");
    library.detail_id = None;
    library.selected = None;
    library.castle = None;
    library.entries.clear();
    library.name.clear();
    library.status = "Connecting…".into();
    library.dirty = true;
    if let Some(organ) = organ {
        let remote = crate::protein_area::connect_organ(world, &organ)?;
        world.get_mut::<Library>(root).unwrap().remote = Some(remote);
    }
    Ok(())
}

fn mutation(world: &World, root: Entity, command: &Command) -> Result<Backend, String> {
    let library = world
        .get::<Library>(root)
        .ok_or("Component library is closed")?;
    if !library.connected || !library.subscribed {
        return Err("Connect to the Organ first".into());
    }
    let target = || {
        library
            .selected
            .clone()
            .filter(|_| library.castle.is_some())
            .ok_or_else(|| "Select a component first".to_string())
    };
    let name = |field| {
        let name = panel::value(world, field)?.trim().to_owned();
        if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
            Err("Use a component name of 1–80 characters".to_string())
        } else {
            Ok(name)
        }
    };
    Ok(match command {
        Command::Save(field) => {
            let head = name(*field)?;
            let body = encode(CustomCastle::capture(world, root, &head)?)?;
            Backend::CreateCustomComponent { head, body }
        }
        Command::Replace(field) => {
            let target = target()?;
            let head = name(*field)?;
            let body = encode(CustomCastle::capture(world, root, &head)?)?;
            Backend::EditRecordText {
                target,
                head: Some(head),
                body: Some(body),
            }
        }
        Command::Rename(field) => Backend::EditRecordText {
            target: target()?,
            head: Some(name(*field)?),
            body: None,
        },
        Command::Delete => Backend::DeleteRecord { target: target()? },
        _ => return Err("Choose a component operation".into()),
    })
}

impl Action for Command {
    fn apply(&self, world: &mut World, root: Entity) {
        if !world
            .get::<crate::edit_mode::EditMode>(root)
            .is_some_and(|mode| mode.enabled)
        {
            return;
        }
        let Some(library) = world.get::<Library>(root) else {
            return;
        };
        if library.pending.is_some() {
            return;
        }
        let result = (|| {
            match self {
                Self::Organ(organ) => connect(world, root, organ.clone())?,
                Self::Refresh => {
                    let organ = world.get::<Library>(root).unwrap().organ.clone();
                    connect(world, root, organ)?;
                }
                Self::Select(uid) => select(world, root, uid.clone())?,
                Self::Add => {
                    let castle = world
                        .get::<Library>(root)
                        .unwrap()
                        .castle
                        .clone()
                        .ok_or("Select a component first")?;
                    castle.spawn(world, root)?;
                    world.get_mut::<Library>(root).unwrap().status =
                        format!("Added {} to your canvas", castle.name);
                }
                Self::Login(username, password) => {
                    let username = panel::value(world, *username)?;
                    let secret = panel::value(world, *password)?;
                    world
                        .get_mut::<bevy::text::EditableText>(*password)
                        .unwrap()
                        .editor
                        .set_text("");
                    send(
                        world,
                        root,
                        ClientMessage::LiveLogin {
                            username,
                            password: secret,
                        },
                    )?;
                    let mut library = world.get_mut::<Library>(root).unwrap();
                    library.login = false;
                    library.status = "Logging in…".into();
                }
                _ => {
                    let action = mutation(world, root, self)?;
                    let id = nucleus::new_uid("component-action");
                    send(
                        world,
                        root,
                        ClientMessage::Act {
                            id: id.clone(),
                            action,
                        },
                    )?;
                    let mut library = world.get_mut::<Library>(root).unwrap();
                    library.pending = Some(id);
                    library.deleting = matches!(self, Self::Delete);
                    library.status = "Saving…".into();
                }
            }
            Ok::<_, String>(())
        })();
        let mut library = world.get_mut::<Library>(root).unwrap();
        if let Err(error) = result {
            library.status = error;
        }
        library.dirty = true;
        ui::refresh(world, root);
    }
}

fn receive(world: &mut World, root: Entity, message: &ServerMessage) {
    let mut library = world.get_mut::<Library>(root).unwrap();
    match message {
        ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows }
            if id == &library.contacts_id =>
        {
            library.contacts = rows.clone();
        }
        ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows }
            if id == &library.list_id =>
        {
            library.entries = rows.clone();
            if library
                .selected
                .as_ref()
                .is_some_and(|uid| !rows.iter().any(|row| row["uid"].as_str() == Some(uid)))
            {
                library.selected = None;
                library.castle = None;
            }
            if library.pending.is_none() && library.selected.is_none() {
                library.status = "Components loaded".into();
            }
        }
        ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows }
            if Some(id) == library.detail_id.as_ref() =>
        {
            match rows
                .first()
                .ok_or("Component is no longer available".to_string())
                .and_then(decode)
            {
                Ok(castle) => {
                    library.name = castle.name.clone();
                    library.castle = Some(castle);
                    library.status = "Component ready".into();
                }
                Err(error) => {
                    library.castle = None;
                    library.status = error;
                }
            }
        }
        ServerMessage::SessionAuthenticated { .. } if library.organ.is_some() => {
            library.connected = true;
            library.login = false;
        }
        ServerMessage::LiveHello {
            login_required: true,
        } if library.organ.is_some() => {
            library.login = true;
            library.status = "Log in to this Organ to see its components".into();
        }
        ServerMessage::ActionOk {
            id,
            created,
            warnings,
            ..
        } if library.pending.as_ref() == Some(id) => {
            library.pending = None;
            library.subscribed = false;
            library.status = if warnings.is_empty() {
                "Saved".into()
            } else {
                warnings.join(" · ")
            };
            let deleted = library.deleting;
            let target = if deleted {
                library.selected = None;
                library.castle = None;
                library.name.clear();
                library.status = "Component deleted".into();
                None
            } else {
                created.clone().or_else(|| library.selected.clone())
            };
            let unsubscribe = deleted.then(|| library.detail_id.take()).flatten();
            library.dirty = true;
            drop(library);
            if let Some(id) = unsubscribe {
                let _ = send(world, root, ClientMessage::Unsubscribe { id });
            }
            if let Some(uid) = target {
                let _ = select(world, root, uid);
            }
            return;
        }
        ServerMessage::Error { id, message, .. }
            if id == &library.list_id
                || Some(id) == library.detail_id.as_ref()
                || Some(id) == library.pending.as_ref()
                || id == "connection"
                || id == crate::cell_bridge::CONNECTION =>
        {
            library.status = message.clone();
            if Some(id) == library.pending.as_ref()
                || id == "connection"
                || id == crate::cell_bridge::CONNECTION
            {
                library.pending = None;
            }
            if Some(id) == library.detail_id.as_ref() {
                library.castle = None;
            }
            if id == "connection" || id == crate::cell_bridge::CONNECTION {
                library.connected = false;
                library.login = false;
                library.entries.clear();
                library.castle = None;
            }
        }
        _ => return,
    }
    library.dirty = true;
}

fn update(world: &mut World, mut cursor: Local<bevy::ecs::message::MessageCursor<CellMessage>>) {
    let messages: Vec<_> = cursor
        .read(world.resource::<Messages<CellMessage>>())
        .map(|message| message.0.clone())
        .collect();
    let roots: Vec<_> = world
        .query_filtered::<Entity, With<Library>>()
        .iter(world)
        .collect();
    for root in roots {
        for message in &messages {
            let library = world.get::<Library>(root).unwrap();
            if library.organ.is_none()
                || matches!(message, ServerMessage::Snapshot { id, .. } | ServerMessage::Update { id, .. } if id == &library.contacts_id)
            {
                receive(world, root, message);
            }
        }
        let mut incoming = Vec::new();
        if let Some(remote) = &mut world.get_mut::<Library>(root).unwrap().remote {
            for _ in 0..64 {
                match remote.incoming.try_recv() {
                    Ok(message) => incoming.push(message),
                    Err(_) => break,
                }
            }
        }
        for message in incoming {
            receive(world, root, &message);
        }
        let library = world.get::<Library>(root).unwrap();
        if !library.contacts_requested && !crate::laboratory::active(world) {
            let request = ClientMessage::Subscribe {
                id: library.contacts_id.clone(),
                protein: serde_json::from_value(json!({"source":"record","where":[{"kind_eq":"organ"}],"fields":["uid","head","slug"],"order":[{"asc":"head"}],"limit":null})).unwrap(),
            };
            if panel::send(world, request).is_ok() {
                world.get_mut::<Library>(root).unwrap().contacts_requested = true;
            }
        }
        let library = world.get::<Library>(root).unwrap();
        if library.connected && !library.subscribed && !crate::laboratory::active(world) {
            let request = ClientMessage::Subscribe {
                id: library.list_id.clone(),
                protein: query(None),
            };
            match send(world, root, request) {
                Ok(()) => {
                    world.get_mut::<Library>(root).unwrap().subscribed = true;
                }
                Err(error) => {
                    let mut library = world.get_mut::<Library>(root).unwrap();
                    if library.status != error {
                        library.status = error;
                        library.dirty = true;
                    }
                }
            }
        }
        ui::refresh(world, root);
    }
    ui::password_masks(world);
}
