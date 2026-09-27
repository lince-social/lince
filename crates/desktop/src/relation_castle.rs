use crate::{
    actions::Action,
    area::{AreaShape, InfluenceArea},
    protein_area::Config,
};
use bevy::{math::DVec2, prelude::*};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

#[derive(Component)]
pub(crate) struct RelationLink {
    pub owner: Entity,
    pub uid: String,
}

pub fn config() -> Config {
    let mut config = Config {
        hide_filled: true,
        relations: true,
        motion: Some(Default::default()),
        width: 320.0,
        max_height: Some(800.0),
        ..Config::records()
    };
    config.draft.name = "Relations".into();
    config
}

pub fn spawn(world: &mut World, root: Entity) -> Option<Entity> {
    let workspace = world.get::<crate::workspace::Workspaces>(root)?.active;
    let center = world
        .get::<crate::canvas::CanvasView>(root)
        .map_or(DVec2::ZERO, |view| view.center);
    let mut area = InfluenceArea::new(AreaShape::Circle, center, DVec2::splat(1200.0));
    area.name = "Relation Castle".into();
    area.protein = Some(config());
    let entity = crate::area::spawn_area(world, root, workspace, area)?;
    if !crate::workspace_config::enabled(world, root, workspace) {
        crate::workspace_config::set_physics(world, root, workspace, true);
    }
    Some(entity)
}

pub(crate) fn reconcile(
    world: &mut World,
    owner: Entity,
    config: &Config,
    rows: &[Value],
    records: &HashMap<String, Entity>,
) {
    let mut existing: HashMap<_, _> = world
        .query::<(Entity, &RelationLink)>()
        .iter(world)
        .filter(|(_, link)| link.owner == owner)
        .map(|(entity, link)| (link.uid.clone(), entity))
        .collect();
    let mut links = BTreeMap::new();
    if config.relations && config.enabled {
        for row in rows {
            for link in row["links"].as_array().into_iter().flatten() {
                let (Some(uid), Some(from), Some(to), Some(kind)) = (
                    link["uid"].as_str(),
                    link["from"].as_str(),
                    link["to"].as_str(),
                    link["kind"].as_str(),
                ) else {
                    continue;
                };
                if uid.is_empty() || uid.len() > 128 || kind.len() > 1024 || from == to {
                    continue;
                }
                let (Some(from), Some(to)) = (records.get(from), records.get(to)) else {
                    continue;
                };
                links
                    .entry(uid.to_string())
                    .or_insert((*from, *to, kind.to_string()));
                if links.len() >= 4096 {
                    break;
                }
            }
            if links.len() >= 4096 {
                break;
            }
        }
    }
    for (uid, (from, to, label)) in links {
        if let Some(entity) = existing.remove(&uid) {
            if let Some(mut arrow) = world.get_mut::<crate::arrow_sand::ArrowSand>(entity) {
                arrow.from = from;
                arrow.to = to;
                arrow.label = label;
            }
        } else if let Some(entity) = crate::arrow_sand::spawn(world, from, to, label) {
            world.entity_mut(entity).insert(RelationLink { owner, uid });
        }
    }
    for entity in existing.into_values() {
        world.despawn(entity);
    }
}

#[derive(Clone)]
struct Add;

impl Action for Add {
    fn apply(&self, world: &mut World, root: Entity) {
        if let Some(entity) = spawn(world, root) {
            if let Some(mut editor) = world.get_mut::<crate::area_panel::AreaEditor>(root) {
                editor.selected = Some(entity);
            }
        }
    }
}

pub(crate) fn store_entry(world: &mut World, root: Entity, parent: Entity) {
    crate::sand_store::castle_entry(
        world,
        root,
        parent,
        "Relation Castle",
        "Records connected by assertion arrows, pulled toward their spawn center and kept apart.",
        Add,
        |world, _| {
            let entity = world
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(px(12)),
                        ..default()
                    },
                    crate::canvas::CanvasItem {
                        position: DVec2::ZERO,
                        size: Vec2::new(320.0, 120.0),
                    },
                    crate::token_style::background(crate::tokens::Token::Surface),
                ))
                .id();
            crate::protein_area::preview_record(
                world,
                entity,
                &config(),
                &serde_json::json!({"head":"Connected Record", "body":"Details stay in the accordion", "quantity":"3"}),
            );
            entity
        },
    );
}
