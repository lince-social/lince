use crate::host::performance::{self, Count, Span, Stage};
use crate::native::{
    actions::Action,
    cell_bridge::{CellMessage, ReceiveCell},
    edit_mode::label,
};
use bevy::{prelude::*, text::EditableText};
use cell::{ClientMessage, ServerMessage};
use engine::file_sync::FileFormat;
use serde_json::{Value, json};
use std::collections::HashMap;

mod blobs;
mod directories;
mod offers;
mod plugins;
mod snapshots;

pub(crate) use blobs::{accept_copy, prepare_copy, send_copy, transfer as copy_state};

#[derive(Component)]
struct SyncCastle {
    enabled: bool,
    confirmed_enabled: bool,
    toggle: Entity,
    controls: Entity,
    path: Entity,
    selected: Entity,
    format_label: Entity,
    status: Entity,
    library: Entity,
    library_rows: crate::native::retained::Children<Option<(String, String)>>,
    subscriptions: [String; 2],
    protein: String,
    format: FileFormat,
    config_requested: bool,
    loaded: bool,
    library_requested: bool,
    pending: Option<String>,
    names: HashMap<String, String>,
}

#[derive(Clone, Copy)]
enum Reply {
    Config,
    Library,
    Save,
}

struct Route {
    owner: Entity,
    reply: Reply,
    sender: Option<crate::native::cell_bridge::Sender>,
}

#[derive(Resource, Default)]
struct Requests {
    routes: HashMap<String, Route>,
    next: u64,
}

pub struct SyncCastlePlugin;

impl Plugin for SyncCastlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Requests>()
            .add_systems(Update, (blobs::update, directories::update, offers::update, plugins::update))
            .add_message::<CellMessage>()
            .add_systems(Update, (receive.after(ReceiveCell), maintain).chain());
    }
}

fn status(world: &mut World, owner: Entity, value: &str) {
    if let Some(entity) = world.get::<SyncCastle>(owner).map(|view| view.status) {
        crate::native::sand_panel::status(world, entity, value);
    }
}

fn config_status(config: &Value) -> &'static str {
    if config["enabled"] != true {
        "Stopped"
    } else if config["path"].as_str().is_none_or(|path| path.is_empty()) {
        "Choose a directory and save"
    } else {
        "Running"
    }
}

fn set_enabled(world: &mut World, owner: Entity, enabled: bool) {
    let Some(mut view) = world.get_mut::<SyncCastle>(owner) else {
        return;
    };
    if view.enabled == enabled {
        return;
    }
    view.enabled = enabled;
    let (toggle, controls) = (view.toggle, view.controls);
    if let Some(mut node) = world.get_mut::<Node>(controls) {
        node.display = if enabled {
            Display::Flex
        } else {
            Display::None
        };
    }
    let name = if enabled {
        "File sync: on"
    } else {
        "File sync: off"
    };
    if let Some(label) = world
        .get::<Children>(toggle)
        .and_then(|children| children.first())
        .copied()
        && let Some(mut text) = world.get_mut::<Text>(label)
    {
        text.0 = name.into();
    }
    if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(toggle) {
        node.set_toggled(if enabled {
            accesskit::Toggled::True
        } else {
            accesskit::Toggled::False
        });
    }
}

fn send(
    world: &World,
    owner: Entity,
    message: impl FnOnce() -> ClientMessage,
) -> Result<crate::native::cell_bridge::Sender, &'static str> {
    if crate::native::laboratory::blocked(world, owner) {
        return Err("Directory sync is unavailable in the Laboratory.");
    }
    let sender = crate::native::practice_cells::sender(world, owner).ok_or("No Cell connection")?;
    {
        let permit = sender.try_reserve().map_err(|error| match error {
            tokio::sync::mpsc::error::TrySendError::Full(_) => "Connection busy. Try again.",
            tokio::sync::mpsc::error::TrySendError::Closed(_) => "Connection closed",
        })?;
        permit.send(message());
    }
    Ok(sender)
}

fn begin_save(world: &mut World, owner: Entity, id: String) {
    clear_save(world, owner);
    let Some(mut view) = world.get_mut::<SyncCastle>(owner) else {
        return;
    };
    view.pending = Some(id.clone());
    world.resource_mut::<Requests>().routes.insert(
        id,
        Route {
            owner,
            reply: Reply::Save,
            sender: None,
        },
    );
}

fn clear_save(world: &mut World, owner: Entity) {
    if let Some(id) = world
        .get_mut::<SyncCastle>(owner)
        .and_then(|mut view| view.pending.take())
    {
        world.resource_mut::<Requests>().routes.remove(&id);
    }
}

fn failed(world: &mut World, owner: Entity, message: &str) {
    let Some(view) = world.get::<SyncCastle>(owner) else {
        return;
    };
    let enabled = view.confirmed_enabled;
    clear_save(world, owner);
    set_enabled(world, owner, enabled);
    status(world, owner, message);
}

#[derive(Clone)]
pub(crate) enum Command {
    Select(String, String),
    Format(FileFormat),
    Save,
    Toggle,
    Reload,
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(view) = world.get::<SyncCastle>(owner) else {
            return;
        };
        if view.pending.is_some() {
            return;
        }
        match self {
            Self::Select(uid, name) => {
                let selected = view.selected;
                world
                    .get_mut::<SyncCastle>(owner)
                    .expect("the action's Sync panel is still present")
                    .protein = uid.clone();
                if let Some(mut text) = world.get_mut::<Text>(selected) {
                    text.0 = name.clone();
                }
            }
            Self::Format(format) => {
                let label = view.format_label;
                world
                    .get_mut::<SyncCastle>(owner)
                    .expect("the action's Sync panel is still present")
                    .format = *format;
                if let Some(mut text) = world.get_mut::<Text>(label) {
                    text.0 = format_name(*format).into();
                }
            }
            Self::Reload => {
                let mut view = world
                    .get_mut::<SyncCastle>(owner)
                    .expect("the action's Sync panel is still present");
                view.loaded = false;
                view.config_requested = false;
                view.library_requested = false;
                status(world, owner, "Loading…");
            }
            Self::Save | Self::Toggle => {
                let action = if matches!(self, Self::Toggle) {
                    engine::actions::Action::SetFileSyncEnabled {
                        enabled: !view.enabled,
                    }
                } else {
                    if !view.enabled {
                        status(world, owner, "Enable file sync first");
                        return;
                    }
                    let Some(path) = world.get::<EditableText>(view.path) else {
                        status(world, owner, "The directory field is unavailable");
                        return;
                    };
                    engine::actions::Action::ConfigureFileSync {
                        protein: view.protein.clone(),
                        path: path.value().to_string(),
                        format: view.format,
                        enabled: true,
                    }
                };
                world.resource_mut::<Requests>().next += 1;
                let id = format!("sync-save-{}", world.resource::<Requests>().next);
                match send(world, owner, || ClientMessage::Act {
                    id: id.clone(),
                    action,
                }) {
                    Ok(_) => {
                        begin_save(world, owner, id);
                        status(world, owner, "Saving…");
                    }
                    Err(error) => {
                        failed(world, owner, error);
                    }
                }
            }
        }
    }
}

fn format_name(format: FileFormat) -> &'static str {
    match format {
        FileFormat::Lingua => ".lingua",
        FileFormat::Markdown => "Markdown (.md)",
    }
}

pub(crate) fn configured(world: &World, owner: Entity, enabled: bool) -> bool {
    world.get::<SyncCastle>(owner).is_some_and(|view| {
        view.loaded && view.pending.is_none() && view.confirmed_enabled == enabled
    })
}

pub(crate) fn prepare_directory(
    world: &mut World,
    owner: Entity,
    protein: &str,
    path: &str,
) -> bool {
    let Some(view) = world.get::<SyncCastle>(owner) else {
        return false;
    };
    if !view.loaded || view.pending.is_some() {
        return false;
    }
    let field = view.path;
    let Some(mut field) = world.get_mut::<EditableText>(field) else {
        return false;
    };
    field.editor.set_text(path);
    Command::Select(protein.into(), "Only the practice disk note".into()).apply(world, owner);
    Command::Format(FileFormat::Markdown).apply(world, owner);
    true
}

pub(crate) fn save_directory(world: &mut World, owner: Entity) {
    if !configured(world, owner, true) {
        if world
            .get::<SyncCastle>(owner)
            .is_some_and(|view| view.loaded && !view.enabled && view.pending.is_none())
        {
            Command::Toggle.apply(world, owner);
        }
    } else {
        Command::Save.apply(world, owner);
    }
}

pub(crate) fn stop_directory(world: &mut World, owner: Entity) {
    if configured(world, owner, true) {
        Command::Toggle.apply(world, owner);
    }
}

fn button(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    name: &str,
    command: Command,
) -> Entity {
    crate::native::information::action_button(
        world,
        parent,
        owner,
        name,
        crate::native::actions![command],
    )
}

pub fn open_source(world: &mut World, root: Entity, uri: &str) -> Entity {
    let workspace = world
        .get::<crate::native::workspace::Workspaces>(root)
        .map_or(1, |spaces| spaces.active);
    let sand = crate::native::sand_store::spawn_sand(
        world,
        root,
        workspace,
        crate::native::sand_store::SandKind::Sync,
        "",
        bevy::math::DVec2::ZERO,
    );
    let mut pending = world
        .get::<Children>(sand)
        .map(|children| children.iter().collect::<Vec<_>>())
        .unwrap_or_default();
    while let Some(child) = pending.pop() {
        if world
            .get::<crate::native::source_view::SourceView>(child)
            .is_some()
        {
            crate::native::source_view::set_source(world, child, uri);
            return child;
        }
        if let Some(children) = world.get::<Children>(child) {
            pending.extend(children.iter());
        }
    }
    let owner = crate::native::source_view::exchange(world, sand, uri);
    crate::native::source_view::set_source(world, owner, uri);
    owner
}

pub(crate) fn populate(world: &mut World, root: Entity, sand: Entity) -> Entity {
    world.init_resource::<Requests>();
    world.entity_mut(sand).insert((
        crate::native::castle::Castle,
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(8),
            padding: UiRect::all(px(12)),
            overflow: Overflow::clip(),
            ..default()
        },
        crate::native::token_style::background(crate::native::tokens::Token::Surface),
    ));
    let content = crate::native::scroll_sand::fill(world, sand);
    world
        .get_mut::<Node>(content)
        .expect("the scroll content has a layout node")
        .row_gap = px(8);
    label(world, content, "Sync", 22.0);
    label(world, content, "Personal folder", 18.0);
    let toggle = crate::native::information::action_button(
        world,
        content,
        sand,
        "File sync: off",
        crate::native::actions![Command::Toggle],
    );
    let mut node = world
        .get_mut::<bevy::a11y::AccessibilityNode>(toggle)
        .expect("the toggle constructor adds its accessibility node");
    node.set_role(accesskit::Role::Switch);
    node.set_label("File sync");
    node.set_toggled(accesskit::Toggled::False);
    let controls = world
        .spawn((
            Node {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                ..default()
            },
            ChildOf(content),
        ))
        .id();
    let selected = label(world, controls, "All local Records", 16.0);
    world.entity_mut(selected).insert(crate::native::icons::Tooltip("Sync all local Records, or choose a saved Protein without aggregation to limit which Records are written to the directory.".into()));
    let library = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(controls),
        ))
        .id();
    label(world, controls, "Directory", 14.0);
    let input = crate::native::sand::single_line_editor(
        "",
        world.resource::<crate::native::theme::Typography>(),
        0,
        4096,
    );
    let path = world
        .spawn((
            input,
            crate::native::icons::Tooltip(
                "Absolute directory on this computer. Files and Records sync both ways.".into(),
            ),
            ChildOf(controls),
        ))
        .id();
    if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(path) {
        node.set_label("Sync directory");
    }
    let format_label = label(world, controls, format_name(FileFormat::Lingua), 14.0);
    button(
        world,
        controls,
        sand,
        ".lingua",
        Command::Format(FileFormat::Lingua),
    );
    button(
        world,
        controls,
        sand,
        "Markdown (.md)",
        Command::Format(FileFormat::Markdown),
    );
    button(world, controls, sand, "Save directory", Command::Save);
    button(world, content, sand, "Reload settings", Command::Reload);
    let status = label(world, content, "Loading…", 14.0);
    let subscriptions = [subscription(sand, false), subscription(sand, true)];
    for (id, reply) in subscriptions.iter().zip([Reply::Config, Reply::Library]) {
        world.resource_mut::<Requests>().routes.insert(
            id.clone(),
            Route {
                owner: sand,
                reply,
                sender: None,
            },
        );
    }
    world.entity_mut(sand).insert(SyncCastle {
        enabled: false,
        confirmed_enabled: false,
        toggle,
        controls,
        path,
        selected,
        format_label,
        status,
        library,
        library_rows: Default::default(),
        subscriptions,
        protein: String::new(),
        format: FileFormat::Lingua,
        config_requested: false,
        loaded: false,
        library_requested: false,
        pending: None,
        names: Default::default(),
    });
    directories::populate(world, content, sand);
    crate::native::source_view::exchange(world, content, "");
    blobs::populate(world, content);
    offers::populate(world, content);
    plugins::populate(world, content);
    crate::native::workspace_sync::populate(world, root, content);
    crate::native::information::sync::panel(world, root, content);
    sand
}

fn subscription(owner: Entity, library: bool) -> String {
    format!(
        "sync-castle-{}-{}",
        owner.to_bits(),
        if library { "proteins" } else { "config" }
    )
}

fn query(library: bool) -> protein::Protein {
    serde_json::from_value(if library {
        json!({"source":"record", "where":[{"kind_eq":"protein"},{"quantity_gt":"0"}], "fields":["uid","head"], "order":[{"asc":"head"}]})
    } else {
        json!({"source":"record", "where":[{"slug_eq":"local-organ"}], "fields":["uid","extension"], "include":{"extension":{"namespace":"lince.file_sync"}}})
    }).expect("the literal Sync query matches the Protein schema")
}

fn maintain(world: &mut World) {
    world.resource_scope(|world, mut requests: Mut<Requests>| {
        requests.routes.retain(|id, route| {
            if world.get::<SyncCastle>(route.owner).is_some() {
                return true;
            }
            route.sender.as_ref().is_some_and(|sender| {
                !sender.is_closed()
                    && sender
                        .try_send(ClientMessage::Unsubscribe { id: id.clone() })
                        .is_err()
            })
        });
    });
    let requests: Vec<_> = world
        .query::<(Entity, &SyncCastle)>()
        .iter(world)
        .flat_map(|(owner, view)| {
            [
                (false, view.config_requested),
                (true, view.library_requested),
            ]
            .into_iter()
            .filter_map(move |(library, requested)| {
                (!requested).then(|| {
                    (
                        owner,
                        library,
                        view.subscriptions[usize::from(library)].clone(),
                    )
                })
            })
        })
        .collect();
    for (owner, library, id) in requests {
        match send(world, owner, || ClientMessage::Subscribe {
            id: id.clone(),
            protein: query(library),
        }) {
            Ok(sender) => {
                world
                    .resource_mut::<Requests>()
                    .routes
                    .get_mut(&id)
                    .expect("a Sync panel registers both subscription routes")
                    .sender = Some(sender);
                let mut view = world
                    .get_mut::<SyncCastle>(owner)
                    .expect("the subscription was prepared for this panel");
                if library {
                    view.library_requested = true;
                } else {
                    view.config_requested = true;
                }
            }
            Err(error) => status(world, owner, error),
        }
    }
}

fn reply_id(message: &ServerMessage) -> Option<&str> {
    match message {
        ServerMessage::Snapshot { id, .. }
        | ServerMessage::Update { id, .. }
        | ServerMessage::ActionOk { id, .. }
        | ServerMessage::Error { id, .. } => Some(id),
        _ => None,
    }
}

fn receive(world: &mut World, mut cursor: Local<bevy::ecs::message::MessageCursor<CellMessage>>) {
    let requests = world.resource::<Requests>();
    let messages: Vec<_> = cursor
        .read(world.resource::<Messages<CellMessage>>())
        .filter(|message| {
            reply_id(&message.0).is_some_and(|id| requests.routes.contains_key(id))
                || matches!(message.0.as_ref(), ServerMessage::Error { id, .. }
                    if id == crate::native::cell_bridge::CONNECTION)
        })
        .map(|message| message.0.clone())
        .collect();
    for message in messages.iter().map(|message| message.as_ref()) {
        receive_message(world, message);
    }
}

fn library(world: &mut World, owner: Entity, rows: &[Value]) {
    let _span = Span::new(Stage::NativeSyncLibrary);
    let Some(view) = world.get::<SyncCastle>(owner) else {
        return;
    };
    let (parent, selected) = (view.library, view.selected);
    let mut names = HashMap::with_capacity(rows.len());
    let mut choices = Vec::with_capacity(rows.len() + 1);
    choices.push((None, ("", "All local Records")));
    let mut selected_name = None;
    for row in rows {
        let Some(uid) = row["uid"].as_str() else {
            continue;
        };
        let head = row["head"].as_str();
        let name = head.unwrap_or(uid);
        if let Some(head) = head {
            names.insert(uid.to_owned(), head.to_owned());
        }
        if uid == view.protein {
            selected_name = Some(name);
        }
        choices.push((Some((uid.to_owned(), name.to_owned())), (uid, name)));
    }
    if let Some(name) = selected_name {
        crate::native::sand_panel::status(world, selected, name);
    }
    let Some(mut view) = world.get_mut::<SyncCastle>(owner) else {
        return;
    };
    view.names = names;
    let mut children = std::mem::take(&mut view.library_rows);
    let (mut created, mut retained) = (0, 0);
    children.reconcile(
        world,
        parent,
        choices,
        |world, parent, &(uid, name)| {
            created += 1;
            button(
                world,
                parent,
                owner,
                name,
                Command::Select(uid.into(), name.into()),
            )
        },
        |_, _, _| retained += 1,
    );
    performance::add(Count::SyncProteinChoicesCreated, created);
    performance::add(Count::SyncProteinChoicesRetained, retained);
    if let Some(mut view) = world.get_mut::<SyncCastle>(owner) {
        view.library_rows = children;
    }
}

fn receive_message(world: &mut World, message: impl std::borrow::Borrow<ServerMessage>) {
    let message = message.borrow();
    if let ServerMessage::Error { id, message, .. } = message
        && id == crate::native::cell_bridge::CONNECTION
    {
        let owners: Vec<_> = world
            .query_filtered::<Entity, With<SyncCastle>>()
            .iter(world)
            .collect();
        for owner in owners {
            failed(world, owner, message);
        }
        return;
    }
    let Some(route) = reply_id(message).and_then(|id| world.resource::<Requests>().routes.get(id))
    else {
        return;
    };
    let (owner, reply) = (route.owner, route.reply);
    let Some(view) = world.get::<SyncCastle>(owner) else {
        return;
    };
    performance::count(Count::SyncCastleRepliesRouted);
    match (reply, message) {
        (
            Reply::Library,
            ServerMessage::Snapshot { rows, .. } | ServerMessage::Update { rows, .. },
        ) => {
            library(world, owner, rows);
        }
        (
            Reply::Config,
            ServerMessage::Update { rows, .. } | ServerMessage::Snapshot { rows, .. },
        ) if view.loaded => {
            if view.pending.is_none() {
                let config = rows
                    .first()
                    .map(|row| &row["extension"])
                    .unwrap_or(&Value::Null);
                world
                    .get_mut::<SyncCastle>(owner)
                    .expect("the configuration reply belongs to a mounted panel")
                    .confirmed_enabled = config["enabled"] == true;
                set_enabled(world, owner, config["enabled"] == true);
                status(world, owner, config_status(config));
            }
        }
        (Reply::Config, ServerMessage::Snapshot { rows, .. }) => {
            if view.pending.is_some() {
                return;
            }
            let config = rows
                .first()
                .map(|row| &row["extension"])
                .unwrap_or(&Value::Null);
            let path = view.path;
            let selected = view.selected;
            let format_label = view.format_label;
            let protein = config["protein"].as_str().unwrap_or_default().to_string();
            let name = view
                .names
                .get(&protein)
                .cloned()
                .unwrap_or_else(|| protein.clone());
            let format =
                serde_json::from_value(config["format"].clone()).unwrap_or(FileFormat::Lingua);
            if let Some(mut field) = world.get_mut::<EditableText>(path) {
                field
                    .editor
                    .set_text(config["path"].as_str().unwrap_or_default());
            }
            if let Some(mut label) = world.get_mut::<Text>(selected) {
                label.0 = if protein.is_empty() {
                    "All local Records".into()
                } else {
                    name
                };
            }
            if let Some(mut label) = world.get_mut::<Text>(format_label) {
                label.0 = format_name(format).into();
            }
            let mut view = world
                .get_mut::<SyncCastle>(owner)
                .expect("the configuration reply belongs to a mounted panel");
            view.loaded = true;
            view.protein = protein;
            view.format = format;
            view.confirmed_enabled = config["enabled"] == true;
            set_enabled(world, owner, config["enabled"] == true);
            status(world, owner, config_status(config));
        }
        (Reply::Save, ServerMessage::ActionOk { id, .. }) if view.pending.as_ref() == Some(id) => {
            clear_save(world, owner);
            let mut view = world
                .get_mut::<SyncCastle>(owner)
                .expect("the save reply belongs to a mounted panel");
            view.config_requested = false;
            status(world, owner, "Saved");
        }
        (_, ServerMessage::Error { id, message, .. })
            if !matches!(reply, Reply::Save) || view.pending.as_ref() == Some(id) =>
        {
            failed(world, owner, message);
        }
        _ => {}
    }
}

#[allow(
    clippy::unwrap_used,
    reason = "Sync Laboratory fixtures require every expected state and control to be present"
)]
pub(crate) mod tests {
    use super::*;

    fn fixture() -> (World, Entity) {
        let mut world = World::new();
        crate::native::laboratory::isolate(&mut world);
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::native::theme::Typography>();
        let root = world.spawn_empty().id();
        let castle = crate::native::sand_store::spawn_sand(
            &mut world,
            root,
            1,
            crate::native::sand_store::SandKind::Sync,
            "",
            bevy::math::DVec2::ZERO,
        );
        (world, castle)
    }

    #[cfg_attr(test, test)]
    fn directory_draft_survives_updates_and_failed_save() {
        let (mut world, castle) = fixture();
        assert!(world.get::<crate::native::castle::Castle>(castle).is_some());
        receive_message(
            &mut world,
            ServerMessage::Snapshot {
                id: subscription(castle, false),
                rows: vec![
                    json!({"extension":{"enabled":true,"path":"/saved","protein":"protein-a","format":"markdown"}}),
                ].into(),
            },
        );
        let path = world.get::<SyncCastle>(castle).unwrap().path;
        assert_eq!(
            world.get::<EditableText>(path).unwrap().value().to_string(),
            "/saved"
        );
        assert_eq!(
            world.get::<SyncCastle>(castle).unwrap().format,
            FileFormat::Markdown
        );
        world
            .get_mut::<EditableText>(path)
            .unwrap()
            .editor
            .set_text("/draft");
        Command::Format(FileFormat::Lingua).apply(&mut world, castle);
        Command::Select("protein-b".into(), "Notes".into()).apply(&mut world, castle);
        for _ in 0..3 {
            receive_message(
                &mut world,
                ServerMessage::Snapshot {
                    id: subscription(castle, false),
                    rows: vec![
                        json!({"extension":{"enabled":true,"path":"/saved","protein":"protein-a","format":"markdown"}}),
                    ].into(),
                },
            );
        }
        receive_message(
            &mut world,
            ServerMessage::Update {
                id: subscription(castle, false),
                rows: vec![
                    json!({"extension":{"enabled":false,"path":"/elsewhere","protein":"protein-a","format":"markdown"}}),
                ].into(),
            },
        );
        receive_message(
            &mut world,
            ServerMessage::Update {
                id: subscription(castle, true),
                rows: vec![json!({"uid":"protein-b","head":"Renamed notes"})].into(),
            },
        );
        begin_save(&mut world, castle, "save-test".into());
        receive_message(
            &mut world,
            ServerMessage::Error {
                id: "save-test".into(),
                message: "Directory is not writable".into(),
                code: None,
            },
        );
        let view = world.get::<SyncCastle>(castle).unwrap();
        assert_eq!(view.protein, "protein-b");
        assert_eq!(view.format, FileFormat::Lingua);
        assert!(view.pending.is_none());
        assert_eq!(world.get::<Text>(view.selected).unwrap().0, "Renamed notes");
        assert_eq!(
            world.get::<Text>(view.status).unwrap().0,
            "Directory is not writable"
        );
        assert_eq!(
            world.get::<EditableText>(path).unwrap().value().to_string(),
            "/draft"
        );
        assert!(crate::native::sand_text::snapshot(&world, castle).is_empty());
        set_enabled(&mut world, castle, true);
        Command::Save.apply(&mut world, castle);
        let view = world.get::<SyncCastle>(castle).unwrap();
        assert!(view.pending.is_none());
        assert!(
            world
                .get::<Text>(view.status)
                .unwrap()
                .0
                .contains("No Cell connection")
        );
    }

    #[cfg_attr(test, test)]
    fn saved_settings_refresh_preserves_new_typing_until_explicit_reload() {
        let (mut world, castle) = fixture();
        let snapshot = || {
            ServerMessage::Snapshot {
            id: subscription(castle, false),
            rows: vec![
                json!({"extension":{"enabled":true,"path":"/saved","protein":"protein-a","format":"markdown"}}),
            ].into(),
        }
        };
        receive_message(&mut world, snapshot());
        let path = world.get::<SyncCastle>(castle).unwrap().path;
        begin_save(&mut world, castle, "save-test".into());
        world
            .get_mut::<EditableText>(path)
            .unwrap()
            .editor
            .set_text("/next-draft");
        receive_message(&mut world, snapshot());
        receive_message(
            &mut world,
            ServerMessage::ActionOk {
                id: "save-test".into(),
                created: None,
                facts: 1,
                warnings: vec![],
                data: None,
            },
        );
        receive_message(&mut world, snapshot());
        assert_eq!(
            world.get::<EditableText>(path).unwrap().value().to_string(),
            "/next-draft"
        );
        assert!(world.get::<SyncCastle>(castle).unwrap().pending.is_none());
        Command::Reload.apply(&mut world, castle);
        receive_message(&mut world, snapshot());
        assert_eq!(
            world.get::<EditableText>(path).unwrap().value().to_string(),
            "/saved"
        );
        assert_eq!(
            world.get::<SyncCastle>(castle).unwrap().format,
            FileFormat::Markdown
        );
        assert_eq!(
            world.get::<SyncCastle>(castle).unwrap().protein,
            "protein-a"
        );
    }

    #[cfg_attr(test, test)]
    fn sync_castle_restores_after_workspace_restart() {
        fn open(path: std::path::PathBuf) -> (App, Entity) {
            let mut app = App::new();
            crate::native::laboratory::isolate(app.world_mut());
            app.add_plugins(MinimalPlugins)
                .init_resource::<Assets<Font>>()
                .init_resource::<crate::native::theme::Typography>()
                .init_resource::<bevy::input_focus::InputFocus>()
                .insert_resource(crate::native::workspace::WorkspaceFile::new(path))
                .add_plugins((crate::native::workspace::WorkspacePlugin, SyncCastlePlugin));
            let root = app
                .world_mut()
                .spawn(crate::native::container::BoxRoot)
                .id();
            app.update();
            (app, root)
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interface.json");
        let (mut app, root) = open(path.clone());
        crate::native::sand_store::spawn_sand(
            app.world_mut(),
            root,
            1,
            crate::native::sand_store::SandKind::Sync,
            "",
            bevy::math::DVec2::new(20.0, 30.0),
        );
        app.world_mut().write_message(AppExit::Success);
        app.update();
        drop(app);
        let (mut app, _) = open(path);
        let (stored, item) = app
            .world_mut()
            .query_filtered::<(
                &crate::native::sand_store::StoredSand,
                &crate::native::canvas::CanvasItem,
            ), With<SyncCastle>>()
            .single(app.world())
            .unwrap();
        assert_eq!(stored.kind, crate::native::sand_store::SandKind::Sync);
        assert_eq!(item.position, bevy::math::DVec2::new(20.0, 30.0));
    }

    #[cfg_attr(test, tokio::test)]
    async fn castle_saves_and_stops_directory_sync_through_the_cell() {
        let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
        let protein = engine
            .act(
                engine::actions::Action::SaveProtein {
                    slug: "notes".into(),
                    head: "Notes".into(),
                    ast: json!({"source":"record","where":[{"all":[{"kind_eq":"plain"}]}],"limit":100}),
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let runtime = cell::CellRuntime {
            commands: Default::default(),
            store: engine.store.clone(),
            engine: engine.clone(),
            lanes: std::sync::Arc::new(cell::LaneHub::new()),
            wire: Default::default(),
            fiote: None,
            speech: None,
            information: None,
        };
        let mut app = App::new();
        crate::native::laboratory::isolate(app.world_mut());
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Font>>()
            .init_resource::<crate::native::theme::Typography>()
            .insert_resource(crate::native::app::CellHandle(runtime))
            .insert_resource(crate::native::wake::WakeSignal::new(|| {}))
            .add_plugins((
                SyncCastlePlugin,
                crate::native::cell_bridge::CellBridgePlugin,
            ));
        let root = app.world_mut().spawn_empty().id();
        let castle = crate::native::sand_store::spawn_sand(
            app.world_mut(),
            root,
            1,
            crate::native::sand_store::SandKind::Sync,
            "",
            bevy::math::DVec2::ZERO,
        );
        async fn wait_status(app: &mut App, castle: Entity, expected: &str) {
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                loop {
                    app.update();
                    let view = app.world().get::<SyncCastle>(castle).unwrap();
                    if app.world().get::<Text>(view.status).unwrap().0 == expected {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
        }
        wait_status(&mut app, castle, "Stopped").await;
        let controls = app.world().get::<SyncCastle>(castle).unwrap().controls;
        assert_eq!(
            app.world().get::<Node>(controls).unwrap().display,
            Display::None
        );
        Command::Toggle.apply(app.world_mut(), castle);
        wait_status(&mut app, castle, "Choose a directory and save").await;
        assert_eq!(
            app.world().get::<Node>(controls).unwrap().display,
            Display::Flex
        );
        let rows = protein::execute(&engine.store, &query(false))
            .await
            .unwrap();
        assert_eq!(rows[0]["extension"]["enabled"], true);
        Command::Select(protein.clone(), "Notes".into()).apply(app.world_mut(), castle);
        let directory = tempfile::tempdir().unwrap();
        let path = app.world().get::<SyncCastle>(castle).unwrap().path;
        app.world_mut()
            .get_mut::<EditableText>(path)
            .unwrap()
            .editor
            .set_text(&directory.path().to_string_lossy());
        Command::Format(FileFormat::Markdown).apply(app.world_mut(), castle);
        Command::Save.apply(app.world_mut(), castle);
        wait_status(&mut app, castle, "Running").await;
        let rows = protein::execute(&engine.store, &query(false))
            .await
            .unwrap();
        assert_eq!(rows[0]["extension"]["protein"], protein);
        assert_eq!(rows[0]["extension"]["format"], "markdown");
        assert!(rows[0]["extension"].get("formats").is_none());
        Command::Toggle.apply(app.world_mut(), castle);
        wait_status(&mut app, castle, "Stopped").await;
        let rows = protein::execute(&engine.store, &query(false))
            .await
            .unwrap();
        assert_eq!(rows[0]["extension"]["enabled"], false);
        assert_eq!(
            app.world().get::<Node>(controls).unwrap().display,
            Display::None
        );
    }

    crate::native::laboratory_cases! {
        directory_draft_survives_updates_and_failed_save,
        saved_settings_refresh_preserves_new_typing_until_explicit_reload,
        sync_castle_restores_after_workspace_restart,
        async castle_saves_and_stops_directory_sync_through_the_cell,
    }
}
