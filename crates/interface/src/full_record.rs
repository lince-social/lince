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
        viewport_height: Some(640.0),
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
        DVec2::new(560.0, 680.0),
    );
    area.name = "Record Castle".into();
    area.protein = Some(config(reference, source));
    crate::area::spawn_area(world, root, workspace, area)
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
struct AddCastle(bool);

impl Action for AddCastle {
    fn apply(&self, world: &mut World, root: Entity) {
        if crate::laboratory::active(world) {
            return;
        }
        let area = if self.0 {
            crate::thread_castle::open(world, root, "pending", Source::Local)
        } else {
            open(world, root, "pending", Source::Local)
        };
        let Some(area) = area else { return };
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
                    .try_send(cell::ClientMessage::Act {
                        id: id.clone(),
                        action: engine::actions::Action::CreateRecord {
                            slug: None,
                            kind: nucleus::RecordKind::Plain,
                            head: String::new(),
                            body: String::new(),
                            quantity: 0.0,
                        },
                    })
                    .map_err(|_| "The local Organ is busy or disconnected. Try again.")
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
        "Record Castle",
        "A new Record with its editable fields.",
        AddCastle(false),
        |world, _| preview(world, false),
    );
    crate::sand_store::castle_entry(
        world,
        root,
        parent,
        "Thread Castle",
        "Conversations on a new Record.",
        AddCastle(true),
        |world, _| preview(world, true),
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
    async fn store_castles_create_records_without_slugs_and_keep_their_placement() {
        let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
        let runtime = cell::CellRuntime {
            engine: engine.clone(),
            store: engine.store.clone(),
            lanes: std::sync::Arc::new(cell::LaneHub::new()),
            wire: Default::default(),
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
        AddCastle(false).apply(app.world_mut(), root);
        AddCastle(true).apply(app.world_mut(), root);
        let entities: Vec<_> = app
            .world_mut()
            .query_filtered::<Entity, With<Creating>>()
            .iter(app.world())
            .collect();
        assert_eq!(entities.len(), 2);
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
            assert_eq!(rows[0]["head"], "");
            if area.name == "Thread Castle" {
                assert_eq!(config.bindings.len(), 1);
                assert_eq!(config.bindings[0].property, "threads");
            } else {
                assert!(config.bindings.len() > 1);
            }
            identities.push(uid.to_string());
        }
        assert_ne!(identities[0], identities[1]);
    }

    #[test]
    fn disconnected_creation_does_not_leave_an_empty_castle() {
        let mut world = World::new();
        let root = world.spawn(crate::workspace::Workspaces::default()).id();
        AddCastle(false).apply(&mut world, root);
        AddCastle(true).apply(&mut world, root);
        assert_eq!(
            world
                .query::<&crate::area::InfluenceArea>()
                .iter(&world)
                .count(),
            0
        );
    }
}
