use crate::{actions::Action, sand_panel as panel};
use bevy::{prelude::*, text::EditableText};
use cell::{ClientMessage, ServerMessage, configuration::Configuration};
use std::collections::VecDeque;

#[derive(Component)]
pub struct ConfigurationSand {
    pages: [Entity; 4],
    status: Entity,
    identity: [Entity; 2],
    toggles: [Entity; 4],
    discovery: [bool; 4],
    discovery_minutes: Entity,
    relays: Entity,
    budget: Entity,
    usage: Entity,
    contacts: Entity,
    selected: Option<String>,
    trust: String,
    trust_label: Entity,
    proximity: Entity,
    snapshot: Option<Configuration>,
    load: Option<tokio::sync::oneshot::Receiver<Result<Configuration, String>>>,
    budget_job:
        Option<tokio::sync::oneshot::Receiver<Result<cell::configuration::Storage, String>>>,
    pending: Option<Pending>,
    requested: bool,
}

struct Pending {
    id: Option<String>,
    actions: VecDeque<engine::actions::Action>,
    completed: usize,
}

pub struct ConfigurationPlugin;
impl Plugin for ConfigurationPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<crate::cell_bridge::CellMessage>()
            .add_systems(Update, update.after(crate::cell_bridge::ReceiveCell));
    }
}

const DISCOVERY: [(&str, &str); 4] = [
    ("local", "Find and advertise Cells on this LAN"),
    ("internet", "Internet reachability"),
    (
        "direct",
        "Allow direct internet connections (reveals this machine’s address)",
    ),
    ("accept_unknown", "Accept unknown conversations"),
];

pub(crate) fn populate(world: &mut World, root: Entity, sand: Entity) -> Entity {
    let body = panel::frame(world, sand, "Configuration");
    let tabs = panel::row(world, body);
    for (index, name) in ["Cell", "Discovery", "Storage", "Contacts"]
        .iter()
        .enumerate()
    {
        panel::button(world, tabs, sand, name, Command::Page(index));
    }
    panel::button(world, tabs, sand, "Refresh", Command::Refresh);
    let status = crate::edit_mode::label(world, body, "Loading configuration…", 12.0);
    let pages = std::array::from_fn(|index| {
        let page = panel::column(world, body);
        let mut node = world.get_mut::<Node>(page).unwrap();
        node.flex_grow = 1.0;
        node.flex_shrink = 1.0;
        node.overflow = Overflow::scroll_y();
        node.display = if index == 0 {
            Display::Flex
        } else {
            Display::None
        };
        crate::scroll_sand::attach(world, page);
        page
    });
    crate::edit_mode::label(
        world,
        pages[0],
        "How this Organ identifies itself locally",
        14.0,
    );
    let identity = [
        panel::field(world, pages[0], "Name", ""),
        panel::field(world, pages[0], "Descriptive address", ""),
    ];
    panel::button(
        world,
        pages[0],
        sand,
        "Save identity",
        Command::SaveIdentity,
    );
    crate::edit_mode::label(
        world,
        pages[1],
        "Where this Cell can be found. These settings do not create trust.",
        14.0,
    );
    let toggles = std::array::from_fn(|index| {
        panel::button(
            world,
            pages[1],
            sand,
            DISCOVERY[index].1,
            Command::Toggle(index),
        )
    });
    let discovery_minutes = panel::field(
        world,
        pages[1],
        "LAN presence in minutes (0 means no time limit)",
        "0",
    );
    let relays = panel::field(
        world,
        pages[1],
        "Relay URLs, separated by commas (blank uses defaults)",
        "",
    );
    panel::button(
        world,
        pages[1],
        sand,
        "Save discovery",
        Command::SaveDiscovery,
    );
    let budget = panel::field(
        world,
        pages[2],
        "Disk budget in MiB (0 means unlimited)",
        "0",
    );
    panel::button(
        world,
        pages[2],
        sand,
        "Save disk budget",
        Command::SaveBudget,
    );
    let usage = crate::edit_mode::label(world, pages[2], "Loading disk usage…", 13.0);
    let sync = panel::column(world, pages[2]);
    world.get_mut::<Node>(sync).unwrap().height = px(540);
    crate::sync_castle::populate(world, root, sync);
    let contacts = panel::column(world, pages[3]);
    crate::edit_mode::label(
        world,
        pages[3],
        "Select a contact above to edit its properties",
        13.0,
    );
    let trust_label = crate::edit_mode::label(world, pages[3], "Trust: no contact selected", 14.0);
    let trust = panel::row(world, pages[3]);
    for value in ["unknown", "known", "blocked"] {
        panel::button(world, trust, sand, value, Command::Trust(value));
    }
    let proximity = panel::field(
        world,
        pages[3],
        "Proximity (non-negative whole number)",
        "0",
    );
    panel::button(world, pages[3], sand, "Save contact", Command::SaveContact);
    world.entity_mut(sand).insert(ConfigurationSand {
        pages,
        status,
        identity,
        toggles,
        discovery: [false, true, false, false],
        discovery_minutes,
        relays,
        budget,
        usage,
        contacts,
        selected: None,
        trust: "unknown".into(),
        trust_label,
        proximity,
        snapshot: None,
        load: None,
        budget_job: None,
        pending: None,
        requested: false,
    });
    sand
}

#[derive(Clone)]
enum Command {
    Page(usize),
    Refresh,
    Toggle(usize),
    SaveIdentity,
    SaveDiscovery,
    SaveBudget,
    Contact(String),
    Trust(&'static str),
    SaveContact,
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(view) = world.get::<ConfigurationSand>(owner) else {
            return;
        };
        let status = view.status;
        if view.pending.is_some() || view.budget_job.is_some() || view.load.is_some() {
            if !matches!(self, Self::Page(_)) {
                panel::status(world, status, "Wait for the current request to finish");
                return;
            }
        }
        let result = match self {
            Self::Page(index) => {
                let pages = view.pages;
                for (i, page) in pages.into_iter().enumerate() {
                    world.get_mut::<Node>(page).unwrap().display = if i == *index {
                        Display::Flex
                    } else {
                        Display::None
                    };
                }
                Ok(())
            }
            Self::Refresh => load(world, owner),
            Self::Toggle(index) => {
                world.get_mut::<ConfigurationSand>(owner).unwrap().discovery[*index] ^= true;
                toggles(world, owner);
                Ok(())
            }
            Self::Trust(trust) => {
                let label = view.trust_label;
                world.get_mut::<ConfigurationSand>(owner).unwrap().trust = (*trust).into();
                panel::status(world, label, format!("Trust: {trust}"));
                Ok(())
            }
            Self::Contact(uid) => {
                select_contact(world, owner, uid);
                Ok(())
            }
            Self::SaveBudget => save_budget(world, owner),
            Self::SaveIdentity | Self::SaveDiscovery | Self::SaveContact => {
                actions(world, owner, self).and_then(|actions| {
                    if crate::laboratory::active(world) {
                        return Err("Changes are unavailable in the Laboratory".into());
                    }
                    world.get_mut::<ConfigurationSand>(owner).unwrap().pending = Some(Pending {
                        id: None,
                        actions: actions.into(),
                        completed: 0,
                    });
                    advance(world, owner)
                })
            }
        };
        if let Err(error) = result {
            panel::status(world, status, error);
        }
    }
}

fn actions(
    world: &World,
    owner: Entity,
    command: &Command,
) -> Result<Vec<engine::actions::Action>, String> {
    use engine::actions::Action;
    let view = world
        .get::<ConfigurationSand>(owner)
        .ok_or("Configuration is closed")?;
    let snapshot = view
        .snapshot
        .as_ref()
        .ok_or("Load the configuration first")?;
    match command {
        Command::SaveIdentity => {
            let name = panel::value(world, view.identity[0])?.trim().to_owned();
            let address = panel::value(world, view.identity[1])?.trim().to_owned();
            if name.is_empty() || name.chars().count() > 160 || address.chars().count() > 2048 {
                return Err(
                    "Use a name of 1–160 characters and an address of at most 2048 characters"
                        .into(),
                );
            }
            Ok(vec![Action::EditRecordText {
                target: snapshot.organ_uid.clone(),
                head: Some(name),
                body: Some(address),
            }])
        }
        Command::SaveDiscovery => {
            let mut fields = snapshot
                .discovery
                .as_object()
                .cloned()
                .ok_or("Invalid discovery settings; refresh before changing them")?;
            for (index, (key, _)) in DISCOVERY.iter().enumerate() {
                fields.insert((*key).into(), view.discovery[index].into());
            }
            let minutes = panel::value(world, view.discovery_minutes)?
                .trim()
                .parse::<u32>()
                .map_err(|_| "LAN duration must be a non-negative whole number of minutes")?;
            if minutes > 525600 {
                return Err("Choose a LAN duration of at most one year".into());
            }
            fields.remove("local_until");
            if view.discovery[0] && minutes > 0 {
                fields.insert(
                    "local_until".into(),
                    (chrono::Utc::now() + chrono::Duration::minutes(i64::from(minutes)))
                        .to_rfc3339()
                        .into(),
                );
            }
            let relays = panel::value(world, view.relays)?;
            let relays: Vec<_> = relays
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .collect();
            for relay in &relays {
                let url = reqwest::Url::parse(relay).map_err(|_| "Enter complete relay URLs")?;
                if !matches!(url.scheme(), "https" | "http") || url.host_str().is_none() {
                    return Err("Relay URLs must use http or https and include a host".into());
                }
            }
            fields.insert("relays".into(), serde_json::json!(relays));
            Ok(vec![Action::SetCellConfig {
                namespace: "lince.discovery".into(),
                fds: fields.into(),
            }])
        }
        Command::SaveContact => {
            let target = view
                .selected
                .as_ref()
                .filter(|uid| snapshot.contacts.iter().any(|contact| &contact.uid == *uid))
                .ok_or("Choose a contact first")?;
            if !["unknown", "known", "blocked"].contains(&view.trust.as_str()) {
                return Err("Choose a valid trust value".into());
            }
            let proximity = panel::value(world, view.proximity)?
                .trim()
                .parse::<u32>()
                .map_err(|_| "Proximity must be a non-negative whole number")?;
            Ok(vec![
                Action::SetContactTrust {
                    target: target.clone(),
                    trust: view.trust.clone(),
                },
                Action::SetContactProximity {
                    target: target.clone(),
                    proximity,
                },
            ])
        }
        _ => Err("This control does not save a property".into()),
    }
}

fn advance(world: &mut World, owner: Entity) -> Result<(), String> {
    let view = world.get::<ConfigurationSand>(owner).unwrap();
    let Some(pending) = view.pending.as_ref() else {
        return Ok(());
    };
    if pending.id.is_some() {
        return Ok(());
    }
    let Some(action) = pending.actions.front().cloned() else {
        let status = view.status;
        world.get_mut::<ConfigurationSand>(owner).unwrap().pending = None;
        panel::status(
            world,
            status,
            "Saved. Refresh to read the current settings.",
        );
        return Ok(());
    };
    let status = view.status;
    let id = nucleus::new_uid("config");
    panel::send(
        world,
        ClientMessage::Act {
            id: id.clone(),
            action,
        },
    )?;
    let mut view = world.get_mut::<ConfigurationSand>(owner).unwrap();
    let pending = view.pending.as_mut().unwrap();
    pending.id = Some(id);
    pending.actions.pop_front();
    panel::status(world, status, "Saving…");
    Ok(())
}

fn load(world: &mut World, owner: Entity) -> Result<(), String> {
    if crate::laboratory::active(world) {
        return Err("Configuration is unavailable in the Laboratory".into());
    }
    let runtime = world
        .get_resource::<crate::app::CellHandle>()
        .ok_or("The local Cell is unavailable")?
        .0
        .clone();
    let handle =
        tokio::runtime::Handle::try_current().map_err(|_| "The local runtime is unavailable")?;
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    let (tx, rx) = tokio::sync::oneshot::channel();
    handle.spawn(async move {
        let result = runtime.configuration().await.map_err(|e| e.to_string());
        let _ = tx.send(result);
        if let Some(wake) = wake {
            wake.ring();
        }
    });
    let mut view = world.get_mut::<ConfigurationSand>(owner).unwrap();
    view.requested = true;
    view.load = Some(rx);
    Ok(())
}

fn budget_bytes(text: &str) -> Result<i64, String> {
    text.trim()
        .parse::<i64>()
        .ok()
        .filter(|value| *value >= 0)
        .and_then(|value| value.checked_mul(1024 * 1024))
        .ok_or_else(|| "Enter a non-negative whole number of MiB within the supported range".into())
}

fn save_budget(world: &mut World, owner: Entity) -> Result<(), String> {
    if crate::laboratory::active(world) {
        return Err("Changes are unavailable in the Laboratory".into());
    }
    let view = world.get::<ConfigurationSand>(owner).unwrap();
    let bytes = budget_bytes(&panel::value(world, view.budget)?)?;
    let runtime = world
        .get_resource::<crate::app::CellHandle>()
        .ok_or("The local Cell is unavailable")?
        .0
        .clone();
    let handle =
        tokio::runtime::Handle::try_current().map_err(|_| "The local runtime is unavailable")?;
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    let (tx, rx) = tokio::sync::oneshot::channel();
    handle.spawn(async move {
        let _ = tx.send(
            runtime
                .set_storage_budget(bytes)
                .await
                .map_err(|e| e.to_string()),
        );
        if let Some(wake) = wake {
            wake.ring();
        }
    });
    let mut view = world.get_mut::<ConfigurationSand>(owner).unwrap();
    view.budget_job = Some(rx);
    let status = view.status;
    panel::status(world, status, "Saving disk budget…");
    Ok(())
}

fn toggles(world: &mut World, owner: Entity) {
    let view = world.get::<ConfigurationSand>(owner).unwrap();
    let values = view.discovery;
    let buttons = view.toggles;
    for (index, button) in buttons.into_iter().enumerate() {
        let label = world
            .get::<Children>(button)
            .and_then(|children| children.first())
            .copied();
        if let Some(label) = label {
            panel::status(
                world,
                label,
                format!(
                    "{}: {}",
                    DISCOVERY[index].1,
                    if values[index] { "on" } else { "off" }
                ),
            );
        }
    }
}

fn set_text(world: &mut World, field: Entity, value: &str) {
    if let Some(mut input) = world.get_mut::<EditableText>(field) {
        input.editor.set_text(value);
    }
}

fn select_contact(world: &mut World, owner: Entity, uid: &str) {
    let view = world.get::<ConfigurationSand>(owner).unwrap();
    let Some(contact) = view
        .snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.contacts.iter().find(|contact| contact.uid == uid))
        .cloned()
    else {
        return;
    };
    let (label, field) = (view.trust_label, view.proximity);
    let mut view = world.get_mut::<ConfigurationSand>(owner).unwrap();
    view.selected = Some(uid.into());
    view.trust = contact.trust.clone();
    set_text(world, field, &contact.proximity.to_string());
    panel::status(
        world,
        label,
        format!("{} · Trust: {}", contact.name, contact.trust),
    );
}

fn usage(world: &mut World, owner: Entity, storage: &cell::configuration::Storage) {
    let view = world.get::<ConfigurationSand>(owner).unwrap();
    let (budget, label) = (view.budget, view.usage);
    set_text(
        world,
        budget,
        &(storage.budget_bytes / (1024 * 1024)).to_string(),
    );
    let total = storage
        .on_disk_bytes
        .map_or("Total directory size unavailable".into(), |bytes| {
            format!("{:.1} MiB on disk in total", bytes as f64 / 1048576.0)
        });
    panel::status(
        world,
        label,
        format!(
            "{total}\nDatabase: {:.1} MiB (kept)\nQuarantine payloads: {:.1} MiB · share: {}\nMedia share: 70% · consumer not built yet\nFacade share: 25% · consumer not built yet\nThe budget applies to managed data; kept files can exceed it.",
            storage.database_bytes as f64 / 1048576.0,
            storage.quarantine_bytes as f64 / 1048576.0,
            if storage.budget_bytes == 0 {
                "unlimited".into()
            } else {
                format!(
                    "{:.1} MiB",
                    (storage.budget_bytes / 100 * 5) as f64 / 1048576.0
                )
            }
        ),
    );
}

fn loaded(world: &mut World, owner: Entity, snapshot: Configuration) {
    let view = world.get::<ConfigurationSand>(owner).unwrap();
    let (identity, contacts, selected, status) = (
        view.identity,
        view.contacts,
        view.selected.clone(),
        view.status,
    );
    set_text(world, identity[0], &snapshot.name);
    set_text(world, identity[1], &snapshot.address);
    let view = world.get::<ConfigurationSand>(owner).unwrap();
    let (minutes_field, relays_field) = (view.discovery_minutes, view.relays);
    let minutes = snapshot.discovery["local_until"]
        .as_str()
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|expiry| {
            (expiry.with_timezone(&chrono::Utc) - chrono::Utc::now())
                .num_minutes()
                .max(0)
        })
        .unwrap_or(0);
    set_text(world, minutes_field, &minutes.to_string());
    let relays = snapshot.discovery["relays"]
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    set_text(world, relays_field, &relays);
    world.get_mut::<ConfigurationSand>(owner).unwrap().discovery = std::array::from_fn(|index| {
        snapshot.discovery[DISCOVERY[index].0]
            .as_bool()
            .unwrap_or(index == 1)
    });
    toggles(world, owner);
    usage(world, owner, &snapshot.storage);
    panel::clear(world, contacts);
    for contact in &snapshot.contacts {
        panel::button(
            world,
            contacts,
            owner,
            &contact.name,
            Command::Contact(contact.uid.clone()),
        );
    }
    if snapshot.contacts.is_empty() {
        crate::edit_mode::label(world, contacts, "No contacts yet", 14.0);
    }
    let selected = selected
        .filter(|uid| snapshot.contacts.iter().any(|contact| &contact.uid == uid))
        .or_else(|| snapshot.contacts.first().map(|contact| contact.uid.clone()));
    world.get_mut::<ConfigurationSand>(owner).unwrap().snapshot = Some(snapshot);
    world.get_mut::<ConfigurationSand>(owner).unwrap().selected = selected.clone();
    if let Some(uid) = selected {
        select_contact(world, owner, &uid);
    }
    panel::status(world, status, "Loaded. Changes are saved when submitted.");
}

fn update(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::cell_bridge::CellMessage>>,
) {
    let messages: Vec<_> = cursor
        .read(world.resource::<Messages<crate::cell_bridge::CellMessage>>())
        .map(|message| message.0.clone())
        .collect();
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<ConfigurationSand>>()
        .iter(world)
        .collect();
    for owner in owners {
        let status = world.get::<ConfigurationSand>(owner).unwrap().status;
        if !world.get::<ConfigurationSand>(owner).unwrap().requested
            && world.contains_resource::<crate::app::CellHandle>()
        {
            if let Err(error) = load(world, owner) {
                panel::status(world, status, error);
            }
        }
        let result = world
            .get_mut::<ConfigurationSand>(owner)
            .unwrap()
            .load
            .as_mut()
            .and_then(|receiver| match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    Some(Err("Configuration request stopped".into()))
                }
                Err(_) => None,
            });
        if let Some(result) = result {
            world.get_mut::<ConfigurationSand>(owner).unwrap().load = None;
            match result {
                Ok(snapshot) => loaded(world, owner, snapshot),
                Err(error) => panel::status(world, status, error),
            }
        }
        let result = world
            .get_mut::<ConfigurationSand>(owner)
            .unwrap()
            .budget_job
            .as_mut()
            .and_then(|receiver| match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => Some(Err(
                    "Storage request stopped; refresh before retrying".into(),
                )),
                Err(_) => None,
            });
        if let Some(result) = result {
            world
                .get_mut::<ConfigurationSand>(owner)
                .unwrap()
                .budget_job = None;
            match result {
                Ok(storage) => {
                    usage(world, owner, &storage);
                    panel::status(world, status, "Disk budget saved");
                }
                Err(error) => panel::status(world, status, error),
            }
        }
        for message in &messages {
            let view = world.get::<ConfigurationSand>(owner).unwrap();
            let Some(pending) = view.pending.as_ref() else {
                continue;
            };
            match message {
                ServerMessage::ActionOk { id, .. } if pending.id.as_ref() == Some(id) => {
                    let mut view = world.get_mut::<ConfigurationSand>(owner).unwrap();
                    let pending = view.pending.as_mut().unwrap();
                    pending.id = None;
                    pending.completed += 1;
                }
                ServerMessage::Error { id, message, .. }
                    if pending.id.as_ref() == Some(id) || id == crate::cell_bridge::CONNECTION =>
                {
                    let completed = pending.completed;
                    world.get_mut::<ConfigurationSand>(owner).unwrap().pending = None;
                    panel::status(
                        world,
                        status,
                        format!("{message} ({completed} changes saved). Refresh before retrying."),
                    );
                }
                _ => {}
            }
        }
        if let Err(error) = advance(world, owner) {
            panel::status(world, status, error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sand_panel::tests::{app, connect, settle};

    #[test]
    fn storage_budget_rejects_negative_fractional_and_overflow_values() {
        assert_eq!(budget_bytes("0").unwrap(), 0);
        assert_eq!(budget_bytes("2048").unwrap(), 2 * 1024 * 1024 * 1024);
        for text in ["-1", "1.5", "NaN", "9223372036854775807", ""] {
            assert!(budget_bytes(text).is_err());
        }
    }

    #[tokio::test]
    async fn configuration_saves_identity_cell_discovery_and_disk_budget() {
        let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
        let mut app = app();
        let runtime = connect(&mut app, engine);
        app.add_plugins(ConfigurationPlugin);
        let owner = app.world_mut().spawn(Node::default()).id();
        populate(app.world_mut(), owner, owner);
        settle(&mut app, |world| {
            world
                .get::<ConfigurationSand>(owner)
                .unwrap()
                .snapshot
                .is_some()
        })
        .await;
        let identity = app
            .world()
            .get::<ConfigurationSand>(owner)
            .unwrap()
            .identity;
        set_text(app.world_mut(), identity[0], "Local test name");
        set_text(app.world_mut(), identity[1], "https://example.test");
        Command::SaveIdentity.apply(app.world_mut(), owner);
        settle(&mut app, |world| {
            world
                .get::<ConfigurationSand>(owner)
                .unwrap()
                .pending
                .is_none()
        })
        .await;
        assert_eq!(
            runtime.configuration().await.unwrap().name,
            "Local test name"
        );
        assert_eq!(
            runtime.configuration().await.unwrap().address,
            "https://example.test"
        );
        {
            let mut view = app.world_mut().get_mut::<ConfigurationSand>(owner).unwrap();
            view.snapshot.as_mut().unwrap().discovery = serde_json::json!({"local":false,"internet":false,"future_property":"keep","local_until":"2020-01-01T00:00:00Z"});
            view.discovery = [true, false, false, false];
        }
        Command::SaveDiscovery.apply(app.world_mut(), owner);
        settle(&mut app, |world| {
            world
                .get::<ConfigurationSand>(owner)
                .unwrap()
                .pending
                .is_none()
        })
        .await;
        let discovery = runtime.configuration().await.unwrap().discovery;
        assert_eq!(discovery["local"], true);
        assert_eq!(discovery["future_property"], "keep");
        assert!(discovery.get("local_until").is_none());
        let view = app.world().get::<ConfigurationSand>(owner).unwrap();
        let (minutes, relays) = (view.discovery_minutes, view.relays);
        set_text(app.world_mut(), minutes, "30");
        set_text(app.world_mut(), relays, "https://relay.example.test");
        let queued = actions(app.world(), owner, &Command::SaveDiscovery).unwrap();
        let engine::actions::Action::SetCellConfig { fds, .. } = &queued[0] else {
            panic!("Expected Cell configuration")
        };
        assert_eq!(
            fds["relays"],
            serde_json::json!(["https://relay.example.test"])
        );
        let expiry =
            chrono::DateTime::parse_from_rfc3339(fds["local_until"].as_str().unwrap()).unwrap();
        assert!(
            (29..=30)
                .contains(&(expiry.with_timezone(&chrono::Utc) - chrono::Utc::now()).num_minutes())
        );
        set_text(app.world_mut(), relays, "file:///private");
        assert!(actions(app.world(), owner, &Command::SaveDiscovery).is_err());
        set_text(app.world_mut(), relays, "");
        set_text(app.world_mut(), minutes, "-1");
        assert!(actions(app.world(), owner, &Command::SaveDiscovery).is_err());
        let budget = app.world().get::<ConfigurationSand>(owner).unwrap().budget;
        set_text(app.world_mut(), budget, "64");
        Command::SaveBudget.apply(app.world_mut(), owner);
        settle(&mut app, |world| {
            world
                .get::<ConfigurationSand>(owner)
                .unwrap()
                .budget_job
                .is_none()
        })
        .await;
        assert_eq!(
            runtime.storage_usage().await.unwrap().budget_bytes,
            64 * 1024 * 1024
        );
        assert!(runtime.set_storage_budget(-1).await.is_err());
        assert_eq!(
            runtime.storage_usage().await.unwrap().budget_bytes,
            64 * 1024 * 1024
        );
    }
}
