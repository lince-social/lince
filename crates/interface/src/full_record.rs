use crate::{
    actions::Action,
    protein_area::{Config, RecordBinding, Source},
};
use bevy::{math::DVec2, prelude::*};
use serde_json::json;

#[derive(Component)]
pub(crate) struct RecordCard;

pub fn config(reference: &str, source: Source) -> Config {
    let mut config = Config {
        enabled: true,
        show_labels: true,
        max_height: Some(1000.0),
        group_with_source: true,
        source,
        width: 520.0,
        columns: 1,
        ..Config::records()
    };
    let predicate = if nucleus::valid_uid(reference, "r") {
        json!({"uid_eq":reference})
    } else {
        json!({"slug_eq":reference.trim_start_matches('#')})
    };
    config.draft.name = "Record".into();
    config.draft.query =
        json!({"source":"record","where":[{"all":[predicate]}],"order":[],"limit":1});
    config
}

pub fn open(world: &mut World, root: Entity, reference: &str, source: Source) -> Option<Entity> {
    if reference.trim().is_empty() {
        return None;
    }
    let workspace = world.get::<crate::workspace::Workspaces>(root)?.active;
    let position = world
        .get::<crate::canvas::CanvasView>(root)
        .map_or(DVec2::ZERO, |view| view.center);
    let mut area = crate::area::InfluenceArea::new(
        crate::area::AreaShape::Polygon(vec![
            [-0.5, -0.5],
            [0.5, -0.5],
            [0.5, 0.5],
            [-0.5, 0.5],
            [-0.5, -0.5],
        ]),
        position,
        DVec2::new(520.0, 640.0),
    );
    area.name = "Record Castle".into();
    area.protein = Some(config(reference, source));
    crate::area::spawn_area(world, root, workspace, area)
}

pub fn open_fiote(world: &mut World, root: Entity, reference: &str) -> Option<Entity> {
    let entity = open(world, root, reference, Source::Local)?;
    let mut area = world.get_mut::<crate::area::InfluenceArea>(entity)?;
    area.name = "Fiote Castle".into();
    area.protein.as_mut()?.fiote = true;
    Some(entity)
}

pub(crate) fn fit_source(world: &mut World, source: Entity, castle: Entity) {
    if world
        .get::<crate::canvas_selection::SandGroup>(source)
        .is_none()
        || world.get::<crate::canvas_selection::SandGroup>(source)
            != world.get::<crate::canvas_selection::SandGroup>(castle)
    {
        return;
    }
    let Some(item) = world.get::<crate::canvas::CanvasItem>(castle).copied() else {
        return;
    };
    let Some(mut area) = world.get_mut::<crate::area::InfluenceArea>(source) else {
        return;
    };
    if area.center == item.position.to_array() && area.size == item.size.as_dvec2().to_array() {
        return;
    }
    area.center = item.position.to_array();
    area.size = item.size.as_dvec2().to_array();
    world.entity_mut(source).insert(item);
    let members = crate::topology::groups::members(world, source);
    crate::topology::groups::attach(world, &members);
}

#[derive(Clone)]
pub struct Open(pub RecordBinding);

impl Action for Open {
    fn apply(&self, world: &mut World, _: Entity) {
        let mut cursor = Some(self.0.area);
        while let Some(entity) = cursor {
            if world.get::<crate::workspace::Workspaces>(entity).is_some() {
                open(world, entity, &self.0.uid, self.0.source.clone());
                return;
            }
            cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
    }
}

#[derive(Component)]
struct Creating(String);

#[derive(Clone)]
struct AddCastle;

impl Action for AddCastle {
    fn apply(&self, world: &mut World, root: Entity) {
        world.trigger(crate::record_creation::CreateRecord { entity: root });
    }
}

#[derive(Clone)]
struct AddFiote;

#[derive(Clone)]
struct AddCommand;

impl Action for AddCommand {
    fn apply(&self, world: &mut World, root: Entity) {
        if crate::laboratory::active(world) {
            return;
        }
        let Some(entity) = open(world, root, "pending", Source::Local) else {
            return;
        };
        let mut area = world.get_mut::<crate::area::InfluenceArea>(entity).unwrap();
        area.name = "Command Castle".into();
        let config = area.protein.as_mut().unwrap();
        config.enabled = false;
        config.command = Some(Default::default());
        config.width = 820.0;
        let id = nucleus::new_uid("create-command");
        let result = crate::sand_panel::send(
            world,
            cell::ClientMessage::Act {
                id: id.clone(),
                action: engine::actions::Action::CreateRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Command,
                    head: "Command".into(),
                    body: "printf 'Hello\\n'\n".into(),
                    quantity: 0.0,
                },
            },
        );
        match result {
            Ok(()) => {
                world.entity_mut(entity).insert(Creating(id));
            }
            Err(error) => {
                world.despawn(entity);
                crate::notifications::report(world, "interface::command", &error);
            }
        }
    }
}

impl Action for AddFiote {
    fn apply(&self, world: &mut World, root: Entity) {
        create(world, root);
    }
}

fn create(world: &mut World, root: Entity) {
    if crate::laboratory::active(world) {
        return;
    }
    let Some(area) = open_fiote(world, root, "pending") else {
        return;
    };
    world
        .get_mut::<crate::area::InfluenceArea>(area)
        .unwrap()
        .protein
        .as_mut()
        .unwrap()
        .enabled = false;
    let id = format!("store-record-{}", area.to_bits());
    let result = world
        .get_non_send::<crate::cell_bridge::CellBridge>()
        .ok_or("The local Organ is not connected.")
        .and_then(|bridge| {
            bridge
                .outgoing
                .try_send(cell::ClientMessage::Fiote {
                    id: id.clone(),
                    request: cell::FioteRequest::Directory,
                })
                .map_err(|_| "The local Organ is not available.")
        });
    match result {
        Ok(()) => {
            world.entity_mut(area).insert(Creating(id));
        }
        Err(error) => {
            world.despawn(area);
            crate::notifications::report(world, "interface::record", error);
        }
    }
}

pub(crate) fn receive(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::cell_bridge::CellMessage>>,
) {
    let Some(messages) = world.get_resource::<Messages<crate::cell_bridge::CellMessage>>() else {
        return;
    };
    let events: Vec<_> = cursor
        .read(messages)
        .map(|message| message.0.clone())
        .collect();
    for event in events {
        if let cell::ServerMessage::Error { id, message, .. } = &event
            && id == crate::cell_bridge::CONNECTION
        {
            let pending: Vec<_> = world
                .query_filtered::<Entity, With<Creating>>()
                .iter(world)
                .collect();
            if !pending.is_empty() {
                for entity in pending {
                    world.despawn(entity);
                }
                crate::notifications::report(world, "interface::record", message);
            }
            continue;
        }
        let (id, result) = match event {
            cell::ServerMessage::Fiote { id, status } => (id, Ok(status.record)),
            cell::ServerMessage::ActionOk { id, created, .. } => (
                id,
                created.ok_or_else(|| "The Organ did not return the new Record.".to_string()),
            ),
            cell::ServerMessage::Error { id, message, .. } => (id, Err(message)),
            _ => continue,
        };
        let area = world
            .query::<(Entity, &Creating)>()
            .iter(world)
            .find(|(_, pending)| pending.0 == id)
            .map(|(entity, _)| entity);
        let Some(entity) = area else { continue };
        world.entity_mut(entity).remove::<Creating>();
        match result {
            Ok(uid) => {
                let mut area = world.get_mut::<crate::area::InfluenceArea>(entity).unwrap();
                let config = area.protein.as_mut().unwrap();
                config.draft.query["where"] = json!([{"all":[{"uid_eq":uid}]}]);
                config.enabled = true;
            }
            Err(error) => {
                world.despawn(entity);
                crate::notifications::report(world, "interface::record", &error);
            }
        }
    }
}

pub(crate) fn store_entry(world: &mut World, root: Entity, parent: Entity) {
    crate::sand_store::castle_entry(
        world,
        root,
        parent,
        "Command Castle",
        "Run a Bash Record locally, interact with its terminal and reopen recent runs.",
        AddCommand,
        |world, _| preview(world, false),
    );
    crate::sand_store::castle_entry(
        world,
        root,
        parent,
        "Fiote Castle",
        "An agent with its prompt in the description and a session in each thread.",
        AddFiote,
        |world, _| preview(world, false),
    );
    crate::sand_store::castle_entry(
        world,
        root,
        parent,
        "Record Castle",
        "Find a Record by its properties, or create one with the values you enter.",
        AddCastle,
        |world, _| preview(world, false),
    );
}

fn preview(world: &mut World, threads: bool) -> Entity {
    let entity = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(12)),
                row_gap: px(8),
                ..default()
            },
            crate::canvas::CanvasItem {
                position: DVec2::ZERO,
                size: Vec2::new(560.0, 680.0),
            },
            crate::token_style::background(crate::tokens::Token::Surface),
        ))
        .id();
    let data = json!({"head":"Untitled", "body":"", "quantity":0, "threads":[]});
    if threads {
        crate::thread_castle::populate(
            world,
            entity,
            RecordBinding {
                area: entity,
                uid: String::new(),
                source: Source::Local,
            },
            &data,
        );
    } else {
        crate::scroll_sand::attach(world, entity);
        crate::protein_area::preview_record(
            world,
            entity,
            &config("preview", Source::Local),
            &data,
        );
    }
    entity
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fiote_castle_opens_seeded_agent_at_original_placement() {
        let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
        let directory = tempfile::tempdir().unwrap();
        let host = std::sync::Arc::new(
            cell::fiote::Host::open(engine.clone(), directory.path().into())
                .await
                .unwrap(),
        );
        let runtime = cell::CellRuntime {
            commands: Default::default(),
            engine: engine.clone(),
            store: engine.store.clone(),
            lanes: std::sync::Arc::new(cell::LaneHub::new()),
            wire: Default::default(),
            fiote: Some(host),
            information: None,
        };
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(crate::app::CellHandle(runtime))
            .insert_resource(crate::wake::WakeSignal::new(|| {}))
            .add_plugins(crate::cell_bridge::CellBridgePlugin)
            .add_systems(Update, receive.after(crate::cell_bridge::ReceiveCell));
        let root = app
            .world_mut()
            .spawn((
                crate::workspace::Workspaces::default(),
                crate::canvas::CanvasView {
                    center: DVec2::new(120.0, 80.0),
                    ..default()
                },
            ))
            .id();
        app.update();
        AddFiote.apply(app.world_mut(), root);
        let entities: Vec<_> = app
            .world_mut()
            .query_filtered::<Entity, With<Creating>>()
            .iter(app.world())
            .collect();
        assert_eq!(entities.len(), 1);
        app.world_mut()
            .get_mut::<crate::workspace::Workspaces>(root)
            .unwrap()
            .active = 2;
        app.world_mut()
            .get_mut::<crate::canvas::CanvasView>(root)
            .unwrap()
            .center = DVec2::splat(500.0);
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while entities
                .iter()
                .any(|entity| app.world().get::<Creating>(*entity).is_some())
            {
                app.update();
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let mut identities = Vec::new();
        for entity in entities {
            let world = app.world();
            let area = world.get::<crate::area::InfluenceArea>(entity).unwrap();
            assert_eq!(area.center, [120.0, 80.0]);
            assert_eq!(
                world
                    .get::<crate::workspace::WorkspaceMember>(entity)
                    .unwrap()
                    .0,
                1
            );
            let config = area.protein.as_ref().unwrap();
            assert!(config.enabled);
            let uid = config.draft.query["where"][0]["all"][0]["uid_eq"]
                .as_str()
                .unwrap();
            assert!(nucleus::valid_uid(uid, "r"));
            let query: protein::Protein = serde_json::from_value(json!({"source":"record", "where":[{"uid_eq":uid}], "fields":["uid", "slug", "head"]})).unwrap();
            let rows = protein::execute(&engine.store, &query).await.unwrap();
            assert_eq!(rows.len(), 1);
            assert!(rows[0]["slug"].is_null());
            assert_eq!(
                rows[0]["head"],
                if area.name == "Fiote Castle" {
                    "Development Fiote"
                } else {
                    ""
                }
            );
            if area.name == "Fiote Castle" {
                assert!(config.record_cards && config.fiote);
                assert!(config.bindings.len() > 3);
            }

            identities.push(uid.to_string());
        }
        assert_eq!(identities.len(), 1);
    }

    #[test]
    fn disconnected_creation_does_not_leave_an_empty_castle() {
        let mut world = World::new();
        let root = world.spawn(crate::workspace::Workspaces::default()).id();
        AddCastle.apply(&mut world, root);
        AddFiote.apply(&mut world, root);
        assert_eq!(
            world
                .query::<&crate::area::InfluenceArea>()
                .iter(&world)
                .count(),
            0
        );
    }
}
