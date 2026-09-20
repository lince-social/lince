use crate::{
    actions::Action,
    cell_bridge::{CellBridge, CellMessage, ReceiveCell},
    edit_mode::label,
};
use bevy::{prelude::*, text::EditableText};
use cell::{ClientMessage, ServerMessage};
use engine::file_sync::FileFormat;
use serde_json::{Value, json};
use std::collections::HashSet;

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
    protein: String,
    format: FileFormat,
    config_requested: bool,
    library_requested: bool,
    pending: Option<String>,
    names: std::collections::HashMap<String, String>,
}

#[derive(Resource, Default)]
struct Requests {
    subscriptions: HashSet<String>,
    next: u64,
}

pub struct SyncCastlePlugin;

impl Plugin for SyncCastlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Requests>()
            .add_message::<CellMessage>()
            .add_systems(Update, (receive.after(ReceiveCell), maintain).chain());
    }
}

fn status(world: &mut World, owner: Entity, value: &str) {
    let entity = world.get::<SyncCastle>(owner).unwrap().status;
    world.get_mut::<Text>(entity).unwrap().0 = value.into();
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
    let mut view = world.get_mut::<SyncCastle>(owner).unwrap();
    view.enabled = enabled;
    let (toggle, controls) = (view.toggle, view.controls);
    world.get_mut::<Node>(controls).unwrap().display = if enabled {
        Display::Flex
    } else {
        Display::None
    };
    let name = if enabled {
        "File sync: on"
    } else {
        "File sync: off"
    };
    let label = world.get::<Children>(toggle).unwrap()[0];
    world.get_mut::<Text>(label).unwrap().0 = name.into();
    let mut node = world
        .get_mut::<bevy::a11y::AccessibilityNode>(toggle)
        .unwrap();
    node.set_role(accesskit::Role::Switch);
    node.set_label("File sync");
    node.set_toggled(if enabled {
        accesskit::Toggled::True
    } else {
        accesskit::Toggled::False
    });
}

fn send(world: &World, message: ClientMessage) -> Result<(), &'static str> {
    if crate::laboratory::active(world) {
        return Err("Directory sync is unavailable in the Laboratory.");
    }
    world
        .get_non_send::<CellBridge>()
        .ok_or("No Cell connection")?
        .outgoing
        .try_send(message)
        .map_err(|error| match error {
            tokio::sync::mpsc::error::TrySendError::Full(_) => "Connection busy. Try again.",
            tokio::sync::mpsc::error::TrySendError::Closed(_) => "Connection closed",
        })
}

#[derive(Clone)]
enum Command {
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
                world.get_mut::<SyncCastle>(owner).unwrap().protein = uid.clone();
                world.get_mut::<Text>(selected).unwrap().0 = name.clone();
            }
            Self::Format(format) => {
                let label = view.format_label;
                world.get_mut::<SyncCastle>(owner).unwrap().format = *format;
                world.get_mut::<Text>(label).unwrap().0 = format_name(*format).into();
            }
            Self::Reload => {
                let mut view = world.get_mut::<SyncCastle>(owner).unwrap();
                view.config_requested = false;
                view.library_requested = false;
                status(world, owner, "Loading…");
            }
            Self::Save | Self::Toggle => {
                let path = world
                    .get::<EditableText>(view.path)
                    .unwrap()
                    .value()
                    .to_string();
                let action = if matches!(self, Self::Toggle) {
                    engine::actions::Action::SetFileSyncEnabled {
                        enabled: !view.enabled,
                    }
                } else {
                    engine::actions::Action::ConfigureFileSync {
                        protein: view.protein.clone(),
                        path,
                        format: view.format,
                        enabled: true,
                    }
                };
                if matches!(self, Self::Save) && !view.enabled {
                    status(world, owner, "Enable file sync first");
                    return;
                }
                world.resource_mut::<Requests>().next += 1;
                let id = format!("sync-save-{}", world.resource::<Requests>().next);
                match send(
                    world,
                    ClientMessage::Act {
                        id: id.clone(),
                        action,
                    },
                ) {
                    Ok(()) => {
                        world.get_mut::<SyncCastle>(owner).unwrap().pending = Some(id);
                        status(world, owner, "Saving…");
                    }
                    Err(error) => {
                        let enabled = world.get::<SyncCastle>(owner).unwrap().confirmed_enabled;
                        set_enabled(world, owner, enabled);
                        status(world, owner, error);
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

fn button(world: &mut World, parent: Entity, owner: Entity, name: &str, command: Command) {
    crate::information::action_button(world, parent, owner, name, crate::actions![command]);
}

pub(crate) fn populate(world: &mut World, root: Entity, sand: Entity) -> Entity {
    world.init_resource::<Requests>();
    world
        .entity_mut(sand)
        .insert((
            crate::castle::Castle,
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                padding: UiRect::all(px(12)),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition::default(),
            crate::token_style::background(crate::tokens::Token::Surface),
        ))
        .observe(
            |mut event: On<Pointer<bevy::picking::events::Scroll>>,
             mut scrolls: Query<&mut ScrollPosition>| {
                if let Ok(mut scroll) = scrolls.get_mut(event.entity) {
                    let step = if event.unit == bevy::input::mouse::MouseScrollUnit::Line {
                        24.0
                    } else {
                        1.0
                    };
                    scroll.0.y = (scroll.0.y - event.y * step).max(0.0);
                    event.propagate(false);
                }
            },
        );
    label(world, sand, "Sync", 22.0);
    let toggle = crate::information::action_button(
        world,
        sand,
        sand,
        "File sync: off",
        crate::actions![Command::Toggle],
    );
    let controls = world
        .spawn((
            Node {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                ..default()
            },
            ChildOf(sand),
        ))
        .id();
    let selected = label(world, controls, "All local Records", 16.0);
    world.entity_mut(selected).insert(crate::icons::Tooltip("Sync all local Records, or choose a saved Protein without aggregation to limit which Records are written to the directory.".into()));
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
    let mut text = crate::sand::editable("");
    text.allow_newlines = false;
    text.visible_lines = Some(1.0);
    text.max_characters = Some(4096);
    let path = world
        .spawn((
            text,
            world.resource::<crate::theme::Typography>().text(14.0),
            crate::token_style::text(crate::tokens::Token::Ink),
            crate::token_style::CursorToken(crate::tokens::Token::Accent),
            crate::token_style::border(crate::tokens::Token::Accent),
            crate::icons::Tooltip(
                "Absolute directory on this computer. Files and Records sync both ways.".into(),
            ),
            bevy::input_focus::tab_navigation::TabIndex(0),
            Node {
                width: percent(100),
                min_height: px(30),
                border: UiRect::all(px(1)),
                padding: UiRect::all(px(4)),
                flex_shrink: 0.0,
                ..default()
            },
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
    button(world, sand, sand, "Reload settings", Command::Reload);
    let status = label(world, sand, "Loading…", 14.0);
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
        protein: String::new(),
        format: FileFormat::Lingua,
        config_requested: false,
        library_requested: false,
        pending: None,
        names: Default::default(),
    });
    set_enabled(world, sand, false);
    crate::information::sync::panel(world, root, sand);
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
    }).unwrap()
}

fn maintain(world: &mut World) {
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<SyncCastle>>()
        .iter(world)
        .collect();
    let active: HashSet<_> = owners
        .iter()
        .flat_map(|owner| [subscription(*owner, false), subscription(*owner, true)])
        .collect();
    let stale: Vec<_> = world
        .resource::<Requests>()
        .subscriptions
        .difference(&active)
        .cloned()
        .collect();
    for id in stale {
        if send(world, ClientMessage::Unsubscribe { id: id.clone() }).is_ok() {
            world.resource_mut::<Requests>().subscriptions.remove(&id);
        }
    }
    for owner in owners {
        for library in [false, true] {
            let view = world.get::<SyncCastle>(owner).unwrap();
            if if library {
                view.library_requested
            } else {
                view.config_requested
            } {
                continue;
            }
            let id = subscription(owner, library);
            match send(
                world,
                ClientMessage::Subscribe {
                    id: id.clone(),
                    protein: query(library),
                },
            ) {
                Ok(()) => {
                    world.resource_mut::<Requests>().subscriptions.insert(id);
                    let mut view = world.get_mut::<SyncCastle>(owner).unwrap();
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
}

fn receive(world: &mut World, mut cursor: Local<bevy::ecs::message::MessageCursor<CellMessage>>) {
    let messages: Vec<_> = cursor
        .read(world.resource::<Messages<CellMessage>>())
        .map(|message| message.0.clone())
        .collect();
    for message in messages {
        receive_message(world, message);
    }
}

fn receive_message(world: &mut World, message: ServerMessage) {
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<SyncCastle>>()
        .iter(world)
        .collect();
    for owner in owners {
        let view = world.get::<SyncCastle>(owner).unwrap();
        match &message {
            ServerMessage::Snapshot { id, rows, .. } | ServerMessage::Update { id, rows, .. }
                if *id == subscription(owner, true) =>
            {
                let parent = view.library;
                let selected = view.selected;
                let protein = view.protein.clone();
                world.get_mut::<SyncCastle>(owner).unwrap().names = rows
                    .iter()
                    .filter_map(|row| {
                        Some((
                            row["uid"].as_str()?.to_owned(),
                            row["head"].as_str()?.to_owned(),
                        ))
                    })
                    .collect();
                world.entity_mut(parent).despawn_children();
                button(
                    world,
                    parent,
                    owner,
                    "All local Records",
                    Command::Select(String::new(), "All local Records".into()),
                );
                for row in rows {
                    if let Some(uid) = row["uid"].as_str() {
                        let name = row["head"].as_str().unwrap_or(uid);
                        if uid == protein {
                            world.get_mut::<Text>(selected).unwrap().0 = name.into();
                        }
                        button(
                            world,
                            parent,
                            owner,
                            name,
                            Command::Select(uid.into(), name.into()),
                        );
                    }
                }
            }
            ServerMessage::Update { id, rows } if *id == subscription(owner, false) => {
                if view.pending.is_none() {
                    let config = rows
                        .first()
                        .map(|row| &row["extension"])
                        .unwrap_or(&Value::Null);
                    world
                        .get_mut::<SyncCastle>(owner)
                        .unwrap()
                        .confirmed_enabled = config["enabled"] == true;
                    set_enabled(world, owner, config["enabled"] == true);
                    status(world, owner, config_status(config));
                }
            }
            ServerMessage::Snapshot { id, rows, .. } if *id == subscription(owner, false) => {
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
                world
                    .get_mut::<EditableText>(path)
                    .unwrap()
                    .editor
                    .set_text(config["path"].as_str().unwrap_or_default());
                world.get_mut::<Text>(selected).unwrap().0 = if protein.is_empty() {
                    "All local Records".into()
                } else {
                    name
                };
                world.get_mut::<Text>(format_label).unwrap().0 = format_name(format).into();
                let mut view = world.get_mut::<SyncCastle>(owner).unwrap();
                view.protein = protein;
                view.format = format;
                world
                    .get_mut::<SyncCastle>(owner)
                    .unwrap()
                    .confirmed_enabled = config["enabled"] == true;
                set_enabled(world, owner, config["enabled"] == true);
                status(world, owner, config_status(config));
            }
            ServerMessage::ActionOk { id, .. } if view.pending.as_ref() == Some(id) => {
                let mut view = world.get_mut::<SyncCastle>(owner).unwrap();
                view.pending = None;
                view.config_requested = false;
                status(world, owner, "Saved");
            }
            ServerMessage::Error { id, message, .. }
                if view.pending.as_ref() == Some(id)
                    || *id == subscription(owner, false)
                    || *id == subscription(owner, true)
                    || id == crate::cell_bridge::CONNECTION =>
            {
                let enabled = world.get::<SyncCastle>(owner).unwrap().confirmed_enabled;
                world.get_mut::<SyncCastle>(owner).unwrap().pending = None;
                set_enabled(world, owner, enabled);
                status(world, owner, message);
            }
            _ => {}
        }
    }
}

pub(crate) mod tests {
    use super::*;

    fn fixture() -> (World, Entity) {
        let mut world = World::new();
        crate::laboratory::isolate(&mut world);
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::theme::Typography>();
        let root = world.spawn_empty().id();
        let castle = crate::sand_store::spawn_sand(
            &mut world,
            root,
            1,
            crate::sand_store::SandKind::Sync,
            "",
            bevy::math::DVec2::ZERO,
        );
        (world, castle)
    }

    #[cfg_attr(test, test)]
    fn directory_draft_survives_updates_and_failed_save() {
        let (mut world, castle) = fixture();
        assert!(world.get::<crate::castle::Castle>(castle).is_some());
        receive_message(
            &mut world,
            ServerMessage::Snapshot {
                id: subscription(castle, false),
                rows: vec![
                    json!({"extension":{"enabled":true,"path":"/saved","protein":"protein-a","format":"markdown"}}),
                ],
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
        receive_message(
            &mut world,
            ServerMessage::Update {
                id: subscription(castle, false),
                rows: vec![
                    json!({"extension":{"enabled":false,"path":"/elsewhere","protein":"protein-a","format":"markdown"}}),
                ],
            },
        );
        receive_message(
            &mut world,
            ServerMessage::Update {
                id: subscription(castle, true),
                rows: vec![json!({"uid":"protein-b","head":"Renamed notes"})],
            },
        );
        world.get_mut::<SyncCastle>(castle).unwrap().pending = Some("save-test".into());
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
        assert!(crate::sand_text::snapshot(&world, castle).is_empty());
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
    fn sync_castle_restores_after_workspace_restart() {
        fn open(path: std::path::PathBuf) -> (App, Entity) {
            let mut app = App::new();
            crate::laboratory::isolate(app.world_mut());
            app.add_plugins(MinimalPlugins)
                .init_resource::<Assets<Font>>()
                .init_resource::<crate::theme::Typography>()
                .init_resource::<bevy::input_focus::InputFocus>()
                .insert_resource(crate::workspace::WorkspaceFile::new(path))
                .add_plugins((crate::workspace::WorkspacePlugin, SyncCastlePlugin));
            let root = app.world_mut().spawn(crate::container::BoxRoot).id();
            app.update();
            (app, root)
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interface.json");
        let (mut app, root) = open(path.clone());
        crate::sand_store::spawn_sand(
            app.world_mut(),
            root,
            1,
            crate::sand_store::SandKind::Sync,
            "",
            bevy::math::DVec2::new(20.0, 30.0),
        );
        app.world_mut().write_message(AppExit::Success);
        app.update();
        drop(app);
        let (mut app, _) = open(path);
        let (stored, item) = app.world_mut().query_filtered::<(&crate::sand_store::StoredSand, &crate::canvas::CanvasItem), With<SyncCastle>>().single(app.world()).unwrap();
        assert_eq!(stored.kind, crate::sand_store::SandKind::Sync);
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
            store: engine.store.clone(),
            engine: engine.clone(),
            lanes: std::sync::Arc::new(cell::LaneHub::new()),
            wire: Default::default(),
            fiote: None,
            information: None,
        };
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Font>>()
            .init_resource::<crate::theme::Typography>()
            .insert_resource(crate::app::CellHandle(runtime))
            .insert_resource(crate::wake::WakeSignal::new(|| {}))
            .add_plugins((SyncCastlePlugin, crate::cell_bridge::CellBridgePlugin));
        let root = app.world_mut().spawn_empty().id();
        let castle = crate::sand_store::spawn_sand(
            app.world_mut(),
            root,
            1,
            crate::sand_store::SandKind::Sync,
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

    crate::laboratory_cases! {
        directory_draft_survives_updates_and_failed_save,
        sync_castle_restores_after_workspace_restart,
        async castle_saves_and_stops_directory_sync_through_the_cell,
    }
}
