use crate::{actions::Action, sand_panel as panel};
use bevy::prelude::*;
use cell::{ClientMessage, ServerMessage};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

mod forms;
mod qr;
mod social;
#[cfg(test)]
mod tests;
mod ui;

use forms::{Field, Kind, form};

#[derive(Component)]
pub struct OrganCastle {
    root: Entity,
    pages: [Entity; 8],
    status: Entity,
    list: Entity,
    detail: Entity,
    nearby: Entity,
    devices: Entity,
    selected: Option<String>,
    rows: HashMap<&'static str, Vec<Value>>,
}

#[derive(Resource, Default)]
struct Requests {
    subscriptions: HashSet<String>,
    senders: HashMap<String, tokio::sync::mpsc::Sender<ClientMessage>>,
    actions: HashMap<String, (Entity, std::time::Instant)>,
}

pub(crate) fn social_delivery_label(stage: &str) -> String {
    social::delivery_label(stage, &Value::Null)
}

pub(crate) fn social_delivery_controls(
    world: &mut World,
    owner: Entity,
    parent: Entity,
    message: &str,
) {
    forms::form(
        world,
        owner,
        parent,
        "Review private delivery",
        json!({"action":"social","request":{"command":"private-delivery-status","message":message}}),
        vec![],
        None,
    );
}

#[derive(Component)]
struct Job(tokio::sync::oneshot::Receiver<Result<Value, String>>);

pub struct OrganCastlePlugin;
impl Plugin for OrganCastlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Requests>()
            .add_message::<crate::cell_bridge::CellMessage>()
            .add_systems(Update, update.after(crate::cell_bridge::ReceiveCell));
    }
}

const TOPICS: [&str; 4] = ["organs", "pairing", "roster", "nearby"];

pub(crate) fn page_visible(world: &World, owner: Entity, index: usize) -> bool {
    world
        .get::<OrganCastle>(owner)
        .and_then(|view| view.pages.get(index))
        .is_some_and(|page| {
            world
                .get::<Node>(*page)
                .is_some_and(|node| node.display != Display::None)
        })
}

pub(crate) fn has_contact(world: &World, owner: Entity, uid: &str) -> bool {
    world
        .get::<OrganCastle>(owner)
        .and_then(|view| view.rows.get("organs"))
        .is_some_and(|rows| {
            rows.iter()
                .any(|row| row["uid"] == uid && row["contact"]["trust"] == "known")
        })
}

pub(crate) fn prepare_pairing(world: &mut World, owner: Entity, invite: &str, name: &str) {
    Command::Page(3).apply(world, owner);
    let form = world
        .query::<(Entity, &forms::Form)>()
        .iter(world)
        .find(|(_, form)| form.owner == owner && form.payload["action"] == "add-known-organ")
        .map(|(entity, _)| entity);
    if let Some(form) = form {
        for (path, value) in [("/invite", invite), ("/name", name)] {
            if let Some(field) = forms::input_text(world, form, path)
                && let Some(mut text) = world.get_mut::<bevy::text::EditableText>(field)
            {
                text.editor.set_text(value);
            }
        }
    }
}

pub(crate) fn submit_pairing(world: &mut World, owner: Entity) {
    let form = world
        .query::<(Entity, &forms::Form)>()
        .iter(world)
        .find(|(_, form)| {
            form.owner == owner
                && form.payload["action"] == "add-known-organ"
                && form.pending.is_none()
        })
        .map(|(entity, _)| entity);
    if let Some(form) = form {
        forms::submit(world, form);
    }
}

pub(crate) fn select_contact(world: &mut World, owner: Entity, uid: &str) -> bool {
    if !world.get::<OrganCastle>(owner).is_some_and(|view| {
        view.rows
            .get("organs")
            .is_some_and(|rows| rows.iter().any(|row| row["uid"] == uid))
    }) {
        return false;
    }
    Command::Page(1).apply(world, owner);
    if world.get::<OrganCastle>(owner).unwrap().selected.as_deref() != Some(uid) {
        Command::Select(uid.into()).apply(world, owner);
    }
    true
}

pub(crate) fn feed_enabled(world: &World, owner: Entity, uid: &str) -> bool {
    world
        .get::<OrganCastle>(owner)
        .and_then(|view| view.rows.get("organs"))
        .is_some_and(|rows| {
            rows.iter()
                .any(|row| row["uid"] == uid && row["contact"]["sync_out"] == true)
        })
}

pub(crate) fn submit_form(
    world: &mut World,
    owner: Entity,
    action: &str,
    fields: &[(&str, Value)],
) -> bool {
    let form = world
        .query::<(Entity, &forms::Form)>()
        .iter(world)
        .find(|(_, form)| {
            form.owner == owner && form.payload["action"] == action && form.pending.is_none()
        })
        .map(|(entity, _)| entity);
    let Some(form) = form else { return false };
    forms::prepare_values(world, form, fields);
    forms::submit(world, form);
    true
}

pub(crate) fn form_result(world: &mut World, owner: Entity, action: &str) -> Option<Value> {
    world
        .query::<&forms::Form>()
        .iter(world)
        .find(|form| form.owner == owner && form.payload["action"] == action)
        .and_then(|form| form.result.clone())
}

pub(crate) fn social_form(
    world: &mut World,
    owner: Entity,
    command: &str,
    fields: &[(&str, Value)],
    submit: bool,
) -> bool {
    let form = world
        .query::<(Entity, &forms::Form)>()
        .iter(world)
        .find(|(_, form)| {
            form.owner == owner
                && form.payload["action"] == "social"
                && form.payload["request"]["command"] == command
                && form.pending.is_none()
        })
        .map(|(entity, _)| entity);
    let Some(form) = form else { return false };
    forms::prepare_values(world, form, fields);
    if submit {
        forms::submit(world, form);
    }
    true
}

pub(crate) fn social_result(world: &mut World, owner: Entity, command: &str) -> Option<Value> {
    world
        .query::<&forms::Form>()
        .iter(world)
        .find(|form| {
            form.owner == owner
                && form.payload["action"] == "social"
                && form.payload["request"]["command"] == command
        })
        .and_then(|form| form.result.clone())
}

pub(crate) fn roster_visible(world: &World, owner: Entity) -> bool {
    page_visible(world, owner, 4)
        && world
            .get::<OrganCastle>(owner)
            .and_then(|view| view.rows.get("roster"))
            .is_some_and(|rows| {
                rows.iter().any(|row| {
                    row["slug"] == "local-organ"
                        && row["extension"]["cells"]
                            .as_array()
                            .is_some_and(|cells| !cells.is_empty())
                })
            })
}

pub(crate) fn populate(world: &mut World, root: Entity, sand: Entity) -> Entity {
    let body = panel::frame(world, sand, "Organ");
    let tabs = panel::row(world, body);
    for (index, title) in [
        "Local settings",
        "Organs",
        "Nearby",
        "Add contact",
        "My devices",
        "Mail",
        "Discovery",
        "Public profile",
    ]
    .iter()
    .enumerate()
    {
        panel::button(world, tabs, sand, title, Command::Page(index));
    }
    panel::button(world, tabs, sand, "Reconnect", Command::Reconnect);
    let status = label(world, body, "Connecting to the local Organ…");
    let pages = std::array::from_fn(|index| {
        let page = panel::column(world, body);
        let mut node = world.get_mut::<Node>(page).unwrap();
        node.flex_grow = 1.0;
        node.flex_shrink = 1.0;
        node.overflow = Overflow::scroll_y();
        node.display = if index == 1 {
            Display::Flex
        } else {
            Display::None
        };
        crate::scroll_sand::attach(world, page);
        page
    });
    crate::configuration::populate(world, root, pages[0]);
    let list = panel::column(world, pages[1]);
    panel::button(
        world,
        pages[1],
        sand,
        "Reload selected Organ",
        Command::Reload,
    );
    let detail = panel::column(world, pages[1]);
    let nearby = panel::column(world, pages[2]);
    ui::registration(world, sand, pages[3]);
    let devices = panel::column(world, pages[4]);
    ui::device_controls(world, sand, pages[4]);
    ui::mail(world, sand, pages[5]);
    social::populate(world, sand, pages[6]);
    social::profile(world, sand, pages[7], &json!({}));
    panel::credits(world, tabs, body, qr::CREDITS);
    panel::credits(world, tabs, body, social::CREDITS);
    world.entity_mut(sand).insert(OrganCastle {
        root,
        pages,
        status,
        list,
        detail,
        nearby,
        devices,
        selected: None,
        rows: HashMap::new(),
    });
    sand
}

fn label(world: &mut World, parent: Entity, text: &str) -> Entity {
    crate::edit_mode::label(world, parent, text, 14.0)
}

fn notice(world: &mut World, parent: Entity, text: &str) {
    let notice = label(world, parent, text);
    crate::accessibility::status(world, notice);
}

fn report(world: &mut World, output: Entity, text: &str) {
    if world.get_entity(output).is_ok() {
        panel::clear(world, output);
        notice(world, output, text);
    }
}

#[derive(Clone)]
pub(crate) enum Command {
    Page(usize),
    Select(String),
    Reload,
    Reconnect,
    Open(String, bool),
    Logout(String),
    Workspaces(String),
}
impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(view) = world.get::<OrganCastle>(owner) else {
            return;
        };
        match self {
            Self::Logout(organ) => crate::protein_area::logout_organ(world, organ),
            Self::Workspaces(organ) => {
                let root = view.root;
                crate::workspace_sync::open_organ(world, root, organ);
            }
            Self::Page(index) => {
                let pages = view.pages;
                for (i, page) in pages.into_iter().enumerate() {
                    world.get_mut::<Node>(page).unwrap().display = if i == *index {
                        Display::Flex
                    } else {
                        Display::None
                    };
                }
            }
            Self::Select(uid) => {
                world.get_mut::<OrganCastle>(owner).unwrap().selected = Some(uid.clone());
                ui::detail(world, owner);
            }
            Self::Reload => ui::detail(world, owner),
            Self::Reconnect => {
                world.init_resource::<Requests>();
                for topic in TOPICS {
                    world
                        .resource_mut::<Requests>()
                        .subscriptions
                        .remove(&subscription(owner, topic));
                }
            }
            Self::Open(uid, remote) => {
                let root = view.root;
                if *remote {
                    open_live(world, root, uid);
                } else {
                    let source = crate::practice_cells::source(world, owner).map_or(
                        crate::protein_area::Source::Local,
                        crate::protein_area::Source::Organ,
                    );
                    crate::full_record::open(world, root, uid, source);
                }
            }
        }
    }
}

fn open_live(world: &mut World, root: Entity, uid: &str) -> Option<Entity> {
    let workspace = world.get::<crate::workspace::Workspaces>(root)?.active;
    let position = world
        .get::<crate::canvas::CanvasView>(root)
        .map_or(bevy::math::DVec2::ZERO, |view| view.center);
    let mut area = crate::area::InfluenceArea::new(
        crate::area::AreaShape::Polygon(vec![
            [-0.5, -0.5],
            [0.5, -0.5],
            [0.5, 0.5],
            [-0.5, 0.5],
            [-0.5, -0.5],
        ]),
        position,
        bevy::math::DVec2::new(640.0, 640.0),
    );
    area.name = "Live Organ".into();
    let mut config = crate::protein_area::Config::records();
    config.source = crate::protein_area::Source::Organ(uid.into());
    config.draft.name = "Live Organ Records".into();
    config.draft.query = json!({"source":"record","where":[],"order":[{"asc":"head"}],"limit":100});
    area.protein = Some(config);
    crate::area::spawn_area(world, root, workspace, area)
}

fn subscription(owner: Entity, topic: &str) -> String {
    format!("organ-castle-{}-{topic}", owner.to_bits())
}

fn query(topic: &str) -> protein::Protein {
    serde_json::from_value(match topic {
        "nearby" => json!({"source":"nearby"}),
        "organs" => json!({"source":"record","where":[{"kind_eq":"organ"}],"include":{"contact":true,"conversations":true,"extension":{"namespace":"lince.file_sync"}},"order":[{"asc":"head"}]}),
        _ => json!({"source":"record","where":[{"kind_eq":"organ"}],"include":{"extension":{"namespace":format!("lince.{topic}")}}}),
    }).unwrap()
}

fn dispatch(world: &mut World, entity: Entity, payload: Value) -> Result<(), String> {
    if crate::laboratory::active(world) {
        return Err("Changes are unavailable in the Laboratory".into());
    }
    let form = world
        .get::<forms::Form>(entity)
        .ok_or("This form is closed")?;
    let output = form.output;
    if form.pending.is_some() {
        return Err("Wait for this request to finish".into());
    }
    match payload["action"].as_str() {
        Some("scan-file") => return qr::scan_file(world, entity, &payload),
        Some("scan-camera") => return qr::scan_camera(world, entity, &payload),
        Some("nearby-pair" | "nearby-chat") => return nearby_job(world, entity, payload),
        _ => {}
    }
    let action: engine::actions::Action =
        serde_json::from_value(payload).map_err(|e| e.to_string())?;
    let id = nucleus::new_uid("organ");
    crate::practice_cells::send(
        world,
        form.owner,
        ClientMessage::Act {
            id: id.clone(),
            action,
        },
    )?;
    world.get_mut::<forms::Form>(entity).unwrap().pending = Some(id.clone());
    world.init_resource::<Requests>();
    world
        .resource_mut::<Requests>()
        .actions
        .insert(id, (entity, std::time::Instant::now()));
    report(world, output, "Waiting for the Organ…");
    Ok(())
}

fn nearby_job(world: &mut World, entity: Entity, payload: Value) -> Result<(), String> {
    let form = world
        .get::<forms::Form>(entity)
        .ok_or("This form is closed")?;
    let (owner, output) = (form.owner, form.output);
    if payload["action"] == "nearby-chat" {
        let existing = world
            .get::<OrganCastle>(owner)
            .and_then(|view| view.rows.get("organs"))
            .and_then(|rows| {
                rows.iter()
                    .find(|row| row["contact"]["node_id"] == payload["node_id"])
            })
            .and_then(|row| row["conversations"].as_array())
            .and_then(|rows| rows.first())
            .and_then(|row| row["uid"].as_str())
            .map(str::to_owned);
        if let Some(uid) = existing {
            Command::Open(uid, false).apply(world, owner);
            report(world, output, "Opened the existing conversation.");
            return Ok(());
        }
    }
    let runtime = crate::practice_cells::runtime(world, owner).ok_or("The Cell is unavailable")?;
    let name = payload["name"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string();
    if name.is_empty() {
        return Err("Enter a name or conversation title".into());
    }
    let node_id = payload["node_id"]
        .as_str()
        .ok_or("Missing NodeId")?
        .to_string();
    let pairing = payload["action"] == "nearby-pair";
    let handle = tokio::runtime::Handle::try_current().map_err(|_| "Runtime is unavailable")?;
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    let (tx, rx) = tokio::sync::oneshot::channel();
    handle.spawn(async move {
        let result = tokio::time::timeout(std::time::Duration::from_secs(30), async {
            if pairing {
                runtime.pair_nearby(&node_id, &name).await.map(|uid| json!({"contact":uid})).map_err(|e| e.to_string())
            } else {
                runtime.chat_nearby(&node_id, &name).await.map(|(conversation, thread)| json!({"conversation":conversation,"thread":thread})).map_err(|e| e.to_string())
            }
        }).await.unwrap_or_else(|_| Err("The other Organ did not answer in time".into()));
        let _ = tx.send(result);
        if let Some(wake) = wake { wake.ring(); }
    });
    begin_job(world, entity, rx);
    Ok(())
}

fn begin_job(
    world: &mut World,
    entity: Entity,
    rx: tokio::sync::oneshot::Receiver<Result<Value, String>>,
) {
    world.entity_mut(entity).insert(Job(rx));
    let mut form = world.get_mut::<forms::Form>(entity).unwrap();
    form.pending = Some("job".into());
    let output = form.output;
    report(world, output, "Working…");
}

fn finish(world: &mut World, entity: Entity, result: Result<Value, String>) {
    let Some(mut form) = world.get_mut::<forms::Form>(entity) else {
        return;
    };
    form.pending = None;
    form.result = result.as_ref().ok().cloned();
    let (output, owner) = (form.output, form.owner);
    panel::clear(world, output);
    match result {
        Ok(value) => {
            if value["karma"].is_object() {
                let karma = &value["karma"];
                label(
                    world,
                    output,
                    &format!(
                        "Karma permission: {} · local state: {} · {}",
                        if karma["permitted"] == true {
                            "enabled"
                        } else {
                            "disabled"
                        },
                        if karma["local_running"] == true {
                            "running"
                        } else {
                            "stopped"
                        },
                        if karma["executing"] == true {
                            "may execute"
                        } else {
                            "will not execute"
                        }
                    ),
                );
                if let Some(reason) = karma["reason"].as_str() {
                    label(world, output, reason);
                }
            }
            if world
                .get::<forms::Form>(entity)
                .is_some_and(|form| form.payload["action"] == "social")
            {
                social::result(world, owner, output, &value);
                return;
            }
            if let Some(target) = world.get::<qr::ScanTarget>(entity).map(|target| target.0) {
                if let Some(text) = value["scanned"].as_str() {
                    if let Some(mut input) = world.get_mut::<bevy::text::EditableText>(target) {
                        input.editor.set_text(text);
                    }
                    label(world, output, "Scanned. Check the code, then submit it.");
                    return;
                }
            }
            label(
                world,
                output,
                "Done. Reload the selected Organ to read its current settings.",
            );
            ui::result(world, owner, output, &value);
        }
        Err(error) => {
            report(world, output, &error);
        }
    }
}

fn receive(world: &mut World, message: &ServerMessage) {
    match message {
        ServerMessage::ActionOk {
            id,
            created,
            data,
            warnings,
            ..
        } => {
            if let Some((entity, _)) = world.resource_mut::<Requests>().actions.remove(id) {
                let mut value = data.clone().unwrap_or_else(|| json!({}));
                if let Some(created) = created {
                    if let Ok(object) = serde_json::from_str::<Value>(created) {
                        value["created"] = object;
                    } else {
                        value["created"] = json!(created);
                    }
                }
                value["warnings"] = json!(warnings);
                finish(world, entity, Ok(value));
            }
        }
        ServerMessage::Error { id, message, .. } => {
            if id == crate::cell_bridge::CONNECTION {
                let actions = std::mem::take(&mut world.resource_mut::<Requests>().actions);
                for (_, (entity, _)) in actions {
                    finish(world, entity, Err(message.clone()));
                }
                world.resource_mut::<Requests>().subscriptions.clear();
            } else if let Some((entity, _)) = world.resource_mut::<Requests>().actions.remove(id) {
                finish(world, entity, Err(message.clone()));
            }
            let owners: Vec<_> = world
                .query::<(Entity, &OrganCastle)>()
                .iter(world)
                .map(|(owner, view)| (owner, view.status))
                .collect();
            for (owner, status) in owners {
                if id == crate::cell_bridge::CONNECTION
                    || TOPICS.iter().any(|topic| *id == subscription(owner, topic))
                {
                    panel::status(world, status, message);
                }
            }
        }
        ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows } => {
            let owners: Vec<_> = world
                .query_filtered::<Entity, With<OrganCastle>>()
                .iter(world)
                .collect();
            for owner in owners {
                for topic in TOPICS {
                    if *id != subscription(owner, topic) {
                        continue;
                    }
                    let mut view = world.get_mut::<OrganCastle>(owner).unwrap();
                    let changed = view.rows.get(topic) != Some(rows);
                    let local_roster_changed = view.rows.get(topic).is_none_or(|previous| {
                        previous
                            .iter()
                            .find(|row| row["slug"] == "local-organ")
                            .map(|row| &row["extension"])
                            != rows
                                .iter()
                                .find(|row| row["slug"] == "local-organ")
                                .map(|row| &row["extension"])
                    });
                    view.rows.insert(topic, rows.clone());
                    let status = view.status;
                    panel::status(
                        world,
                        status,
                        "Connected · drafts change only when you select or reload an Organ",
                    );
                    if !changed {
                        continue;
                    }
                    match topic {
                        "organs" => ui::organ_list(world, owner),
                        "nearby" => ui::nearby(world, owner),
                        "roster" => {
                            if local_roster_changed {
                                ui::devices(world, owner);
                            }
                            ui::pairings(world, owner);
                        }
                        "pairing" => ui::pairings(world, owner),
                        _ => {}
                    }
                }
            }
        }
        _ => {}
    }
}

fn update(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::cell_bridge::CellMessage>>,
) {
    social::poll_media(world);
    let messages: Vec<_> = cursor
        .read(world.resource::<Messages<crate::cell_bridge::CellMessage>>())
        .map(|m| m.0.clone())
        .collect();
    for message in messages {
        receive(world, &message);
    }
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<OrganCastle>>()
        .iter(world)
        .collect();
    let active: HashSet<_> = owners
        .iter()
        .flat_map(|owner| TOPICS.map(|topic| subscription(*owner, topic)))
        .collect();
    let stale: Vec<_> = world
        .resource::<Requests>()
        .subscriptions
        .difference(&active)
        .cloned()
        .collect();
    for id in stale {
        if world
            .resource::<Requests>()
            .senders
            .get(&id)
            .is_none_or(|sender| {
                sender.is_closed()
                    || sender
                        .try_send(ClientMessage::Unsubscribe { id: id.clone() })
                        .is_ok()
            })
        {
            world.resource_mut::<Requests>().subscriptions.remove(&id);
            world.resource_mut::<Requests>().senders.remove(&id);
        }
    }
    for owner in owners {
        for topic in TOPICS {
            let id = subscription(owner, topic);
            if !world.resource::<Requests>().subscriptions.contains(&id)
                && !crate::laboratory::active(world)
            {
                if crate::practice_cells::send(
                    world,
                    owner,
                    ClientMessage::Subscribe {
                        id: id.clone(),
                        protein: query(topic),
                    },
                )
                .is_ok()
                {
                    if let Some(sender) = crate::practice_cells::sender(world, owner) {
                        world
                            .resource_mut::<Requests>()
                            .senders
                            .insert(id.clone(), sender);
                    }
                    world.resource_mut::<Requests>().subscriptions.insert(id);
                }
            }
        }
    }
    let expired: Vec<_> = world
        .resource::<Requests>()
        .actions
        .iter()
        .filter(|(_, (entity, time))| {
            world.get::<forms::Form>(*entity).is_none() || time.elapsed().as_secs() >= 60
        })
        .map(|(id, _)| id.clone())
        .collect();
    for id in expired {
        if let Some((entity, _)) = world.resource_mut::<Requests>().actions.remove(&id) {
            finish(
                world,
                entity,
                Err("No reply yet. Check the current state before retrying this action.".into()),
            );
        }
    }
    let jobs: Vec<_> = world
        .query::<(Entity, &mut Job)>()
        .iter_mut(world)
        .filter_map(|(entity, mut job)| match job.0.try_recv() {
            Ok(result) => Some((entity, result)),
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                Some((entity, Err("The request stopped before replying".into())))
            }
            Err(_) => None,
        })
        .collect();
    for (entity, result) in jobs {
        world.entity_mut(entity).remove::<Job>();
        finish(world, entity, result);
    }
}
